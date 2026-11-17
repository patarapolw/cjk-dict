//! `search.db`: a small SQLite index over Yomitan zips.
//!
//! It holds only what search needs (normalized terms, tokens, readings, tags, scores, and
//! where each entry is: dict + `term_bank_N.json` + array index). Definitions and media
//! stay in the zips, which are read on demand.
//!
//! - [`build`] (dict-admin): zips -> `search.db`
//! - [`build_chars`] (dict-admin): character decomposition ([`crate::ids`]) -> `search.db`
//! - [`SearchDb`] (app): query `search.db`, then read entries / media from the zips

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, functions::FunctionFlags, params, params_from_iter,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};

use crate::{
    char_reading::{self, KanaReadings},
    error::YomitanError,
    ids::{self, Ids},
    korean::{self, RomanQuery},
    parser::{BankKind, YomitanZip},
    pinyin::{expand_pinyin, normalize_pinyin},
    tokenize::{Lang, TokenizerMapper},
    util::{NonEmpty, OptionNonEmpty, fold_kana, normalize_term, normalize_text},
};

const SCHEMA: &str = r#"
CREATE TABLE dict (
  id          INTEGER PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,   -- key; the zip is <name>.zip
  title       TEXT NOT NULL,
  revision    TEXT,
  L1          TEXT,                   -- source language, e.g. ja-JP
  L2          TEXT,
  index_json  TEXT,
  term_count  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE term_search (
  id          INTEGER PRIMARY KEY,
  term        TEXT NOT NULL,            -- NFKC + uppercase
  term_ja     TEXT NOT NULL DEFAULT '', -- space-separated tokens
  term_zh     TEXT NOT NULL DEFAULT '',
  term_ko     TEXT NOT NULL DEFAULT '',
  reading     TEXT NOT NULL DEFAULT '' CHECK (reading = '' OR reading LIKE ' %_ '),
  def_tags    TEXT,
  rules       TEXT,
  score       INTEGER,                  -- max score
  tags        TEXT
);
CREATE UNIQUE INDEX idx_u_term_search ON term_search (
  term, reading, (term_ja != ''), (term_zh != ''), (term_ko != '')
);
CREATE INDEX idx_term_search_score ON term_search (score);

-- where each entry lives: dict zip, term_bank_<bank>.json, array index
CREATE TABLE term_link (
  search_id   INTEGER NOT NULL,
  dict_id     INTEGER NOT NULL,
  bank        INTEGER NOT NULL,
  idx         INTEGER NOT NULL,
  PRIMARY KEY (search_id, dict_id, bank, idx)
) WITHOUT ROWID;

-- external-content FTS, filled once at the end of the build
CREATE VIRTUAL TABLE term_search_fts USING fts5 (
  term_ja, term_zh, term_ko, reading, def_tags, rules, tags,
  content='term_search', content_rowid='id',
  tokenize="unicode61 tokenchars '-'"
);
"#;

/// Korean romanization keys ([`crate::korean::keys`]), to search Hangul terms by typing
/// romanization. Created on an existing search.db too, which an update then backfills.
const ROMAN_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS term_roman (
  key         TEXT NOT NULL,            -- loosened romanization, e.g. hangugo for 한국어
  search_id   INTEGER NOT NULL,
  score       INTEGER,                  -- term_search.score
  PRIMARY KEY (key, search_id)
) WITHOUT ROWID;
-- to walk the matches of a short prefix by score
CREATE INDEX IF NOT EXISTS idx_term_roman_score ON term_roman (score);
"#;

/// the score follows term_search's, which grows as dictionaries add the term
const ROMAN_UPSERT: &str = "INSERT INTO term_roman (key, search_id, score) VALUES (?1, ?2, ?3)
     ON CONFLICT DO UPDATE SET score = excluded.score";

/// [`korean::keys`] of the reading, e.g. 광속 for 光速, or else of the term.
fn roman_keys(term: &str, reading: &str) -> Vec<String> {
    let keys = korean::keys(reading.trim());
    if keys.is_empty() {
        korean::keys(term)
    } else {
        keys
    }
}

/// Where each single-character entry of a dictionary's kanji banks is, e.g. KANJIDIC's.
/// Created on an existing search.db too, which an update then backfills.
const KANJI_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS kanji_link (
  kanji       TEXT NOT NULL,            -- the character, as the dictionary has it
  dict_id     INTEGER NOT NULL,
  bank        INTEGER NOT NULL,         -- kanji_bank_<bank>.json
  idx         INTEGER NOT NULL,
  PRIMARY KEY (kanji, dict_id, bank, idx)
) WITHOUT ROWID;
"#;

/// Han characters: how they break down ([`crate::ids`]), their Korean readings, and how
/// many terms use them. Filled by [`build_chars`].
const HAN_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS han_char (
  ch          TEXT PRIMARY KEY,
  ids         TEXT NOT NULL DEFAULT '', -- ids.txt's IDS columns, tab-separated: ⿹②一[GTKV]\t⿻②一[J]
  hangul      TEXT NOT NULL DEFAULT '', -- space-separated Hangul readings (eum), e.g. 락 악 요 for 樂
  uses        INTEGER NOT NULL DEFAULT 0 -- terms in term_search with the character
) WITHOUT ROWID;
-- the components each IDS names directly; characters made of a component are found by
-- following it up recursively
CREATE TABLE IF NOT EXISTS han_component (
  component   TEXT NOT NULL,
  ch          TEXT NOT NULL,
  PRIMARY KEY (component, ch)
) WITHOUT ROWID;
"#;

/// Each Han character's reading within a term ([`crate::char_reading`]), to find the words
/// with a character read so anywhere: `*<興>{キョウ}`. Filled by [`build_chars`].
const TERM_CHAR_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS term_char (
  ch          TEXT NOT NULL,            -- as term_search.term has it
  reading     TEXT NOT NULL,            -- きょう (a kanji dictionary's), XING1, 흥
  search_id   INTEGER NOT NULL,
  PRIMARY KEY (ch, reading, search_id)
) WITHOUT ROWID;
"#;

/// `meta.term_char`: how `term_char` was filled. A build fills it again when this changes.
const TERM_CHAR_VERSION: &str = "1";

/// Facts about how search.db was built, e.g. `kana` = 1: terms and readings have katakana
/// as hiragana ([`normalize_term`]).
const META_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
  key         TEXT PRIMARY KEY,
  value       TEXT
) WITHOUT ROWID;
"#;

fn meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    if !has_table(conn, "meta")? {
        return Ok(None);
    }
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

/// Kanji bank entries (single characters) indexed per dictionary name, from `kanji_link`
pub fn char_counts(db: &Path) -> Result<HashMap<String, usize>, YomitanError> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !has_table(&conn, "kanji_link")? {
        return Ok(HashMap::new());
    }
    let mut st = conn.prepare(
        "SELECT d.name, count(*) FROM kanji_link k JOIN dict d ON d.id = k.dict_id GROUP BY d.name",
    )?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Whether `db` has Japanese terms with readings but no kanji dictionary with kana readings
/// (e.g. KANJIDIC), so that [`build_chars`] could not index the readings of their kanji:
/// `*<興>{キョウ}` finds no Japanese words then.
pub fn missing_kanji_readings(db: &Path) -> Result<bool, YomitanError> {
    if !db.is_file() {
        return Ok(false);
    }
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if meta(&conn, "kana_kanji")?.as_deref() != Some("0") {
        return Ok(false);
    }
    Ok(conn
        .query_row(
            "SELECT 1 FROM term_search WHERE term_ja != '' AND reading != '' LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Whether `db` must be rebuilt from scratch ([`build`]) before dictionaries can be added to
/// it: it was built before kana folding, and its terms still have katakana.
pub fn needs_rebuild(db: &Path) -> Result<bool, YomitanError> {
    if !db.is_file() {
        return Ok(false);
    }
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    Ok(meta(&conn, "kana")?.is_none())
}

fn has_table(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

const FTS_TRIGGERS: &str = r#"
CREATE TRIGGER IF NOT EXISTS term_search_ai AFTER INSERT ON term_search BEGIN
  INSERT INTO term_search_fts (rowid, term_ja, term_zh, term_ko, reading, def_tags, rules, tags)
  VALUES (new.id, new.term_ja, new.term_zh, new.term_ko, new.reading, new.def_tags, new.rules, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS term_search_ad AFTER DELETE ON term_search BEGIN
  INSERT INTO term_search_fts (term_search_fts, rowid, term_ja, term_zh, term_ko, reading, def_tags, rules, tags)
  VALUES ('delete', old.id, old.term_ja, old.term_zh, old.term_ko, old.reading, old.def_tags, old.rules, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS term_search_au AFTER UPDATE ON term_search BEGIN
  INSERT INTO term_search_fts (term_search_fts, rowid, term_ja, term_zh, term_ko, reading, def_tags, rules, tags)
  VALUES ('delete', old.id, old.term_ja, old.term_zh, old.term_ko, old.reading, old.def_tags, old.rules, old.tags);
  INSERT INTO term_search_fts (rowid, term_ja, term_zh, term_ko, reading, def_tags, rules, tags)
  VALUES (new.id, new.term_ja, new.term_zh, new.term_ko, new.reading, new.def_tags, new.rules, new.tags);
END;
"#;

/// A dictionary to index.
pub struct DictSource {
    pub name: String,
    pub zip: PathBuf,
    /// overrides index.json `sourceLanguage`
    pub lang: Option<Lang>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DictInfo {
    pub name: String,
    pub title: String,
    pub revision: String,
    pub lang: Option<String>,
    pub term_count: usize,
}

pub fn parse_lang(s: &str) -> Option<Lang> {
    Lang::from(s).or_else(|| match s.split(['-', '_']).next()? {
        "ja" => Some(Lang::Ja),
        "zh" => Some(Lang::Zh),
        "ko" => Some(Lang::Ko),
        _ => None,
    })
}

/// Build `out` from scratch (written to `<out>.tmp`, then renamed).
pub async fn build(
    out: &Path,
    dicts: &[DictSource],
    tok: &TokenizerMapper,
    progress: impl Fn(&str, usize, usize),
) -> Result<Vec<DictInfo>, YomitanError> {
    build_inner(out, dicts, &[], tok, progress, false).await
}

/// Update only the supplied dictionaries, first removing their old entries and any other
/// dictionaries no longer present in the library.
pub async fn build_incremental(
    out: &Path,
    dicts: &[DictSource],
    remove: &[String],
    tok: &TokenizerMapper,
    progress: impl Fn(&str, usize, usize),
) -> Result<Vec<DictInfo>, YomitanError> {
    build_inner(out, dicts, remove, tok, progress, true).await
}

async fn build_inner(
    out: &Path,
    dicts: &[DictSource],
    remove: &[String],
    tok: &TokenizerMapper,
    progress: impl Fn(&str, usize, usize),
    incremental: bool,
) -> Result<Vec<DictInfo>, YomitanError> {
    let tmp = out.with_extension("db.tmp");
    let _ = std::fs::remove_file(&tmp);
    let has_existing = incremental && out.is_file();
    if has_existing {
        std::fs::copy(out, &tmp)?;
    }
    let mut conn = Connection::open(&tmp)?;
    conn.execute_batch(
        "PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY; PRAGMA cache_size = -262144;",
    )?;
    if !has_existing {
        conn.execute_batch(SCHEMA)?;
    }
    let backfill_roman = has_existing && !has_table(&conn, "term_roman")?;
    conn.execute_batch(ROMAN_SCHEMA)?;
    let backfill_kanji = has_existing && !has_table(&conn, "kanji_link")?;
    conn.execute_batch(KANJI_SCHEMA)?;
    let kana_folded = !has_existing || meta(&conn, "kana")?.is_some();
    conn.execute_batch(META_SCHEMA)?;
    if !kana_folded && !dicts.is_empty() {
        drop(conn);
        let _ = std::fs::remove_file(&tmp);
        return Err(YomitanError::Error(
            "search.db was built before katakana and hiragana were matched alike: rebuild it"
                .into(),
        ));
    }
    if !has_existing {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('kana', '1')",
            [],
        )?;
    }
    conn.execute_batch(FTS_TRIGGERS)?;

    if backfill_roman {
        progress("romanization keys", 0, 0);
        let tx = conn.transaction()?;
        {
            let mut st = tx.prepare("SELECT id, term, reading, score FROM term_search")?;
            let mut ins = tx.prepare(ROMAN_UPSERT)?;
            let rows = st.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?;
            for row in rows {
                let (id, term, reading, score) = row?;
                for key in roman_keys(&term, &reading) {
                    ins.execute(params![key, id, score])?;
                }
            }
        }
        tx.commit()?;
    }

    if has_existing {
        let tx = conn.transaction()?;
        for name in remove {
            tx.execute(
                "DELETE FROM term_link WHERE dict_id IN (SELECT id FROM dict WHERE name = ?1)",
                [name],
            )?;
            tx.execute(
                "DELETE FROM kanji_link WHERE dict_id IN (SELECT id FROM dict WHERE name = ?1)",
                [name],
            )?;
            tx.execute("DELETE FROM dict WHERE name = ?1", [name])?;
        }
        tx.execute(
            "DELETE FROM term_search WHERE id NOT IN (SELECT DISTINCT search_id FROM term_link)",
            [],
        )?;
        tx.execute(
            "DELETE FROM term_roman WHERE search_id NOT IN (SELECT id FROM term_search)",
            [],
        )?;
        tx.commit()?;
    }

    // a search.db from before kanji_link: index the kanji banks of the dictionaries it has,
    // from the zips next to it
    if backfill_kanji {
        progress("kanji entries", 0, 0);
        let dir = out.parent().unwrap_or(Path::new("."));
        let rows: Vec<(i64, String)> = {
            let mut st = conn.prepare("SELECT id, name FROM dict")?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let tx = conn.transaction()?;
        for (dict_id, name) in rows {
            let zip = dir.join(format!("{name}.zip"));
            if zip.is_file() {
                index_kanji(&tx, &mut YomitanZip::open(&zip)?, dict_id)?;
            }
        }
        tx.commit()?;
    }

    let mut infos = vec![];
    for d in dicts {
        let mut zip = YomitanZip::open(&d.zip)?;
        let index_str = zip.index_json_str()?;
        let index = zip.index()?;
        let lang = d
            .lang
            .or_else(|| index.source_language.as_deref().and_then(parse_lang));

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO dict (name, title, revision, L1, L2, index_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                d.name,
                index.title,
                index.revision,
                lang.map(|l| l.to_string()),
                index.target_language,
                index_str
            ],
        )?;
        let dict_id = tx.last_insert_rowid();

        let banks: Vec<_> = zip
            .banks()
            .into_iter()
            .filter(|b| b.kind == BankKind::Term)
            .collect();
        let mut count = 0usize;
        {
            let mut upsert = tx.prepare(
                r#"INSERT INTO term_search (term, term_ja, term_zh, term_ko, reading, def_tags, rules, score, tags)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT DO UPDATE SET
                  def_tags = CASE WHEN def_tags IS excluded.def_tags THEN def_tags ELSE NULLIF(trim(printf('%s %s', def_tags, excluded.def_tags)), '') END,
                  rules = CASE WHEN rules IS excluded.rules THEN rules ELSE NULLIF(trim(printf('%s %s', rules, excluded.rules)), '') END,
                  tags = CASE WHEN tags IS excluded.tags THEN tags ELSE NULLIF(trim(printf('%s %s', tags, excluded.tags)), '') END,
                  score = max(coalesce(score, excluded.score), coalesce(excluded.score, score))
                RETURNING id, score"#,
            )?;
            let mut link = tx.prepare(
                "INSERT OR IGNORE INTO term_link (search_id, dict_id, bank, idx) VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut roman = tx.prepare(ROMAN_UPSERT)?;

            for (bi, bank) in banks.iter().enumerate() {
                progress(&d.name, bi + 1, banks.len());
                let rows = zip.read_json_array(&bank.name)?;
                for (idx, row) in rows.iter().enumerate() {
                    let term = row.get(0).and_then(Value::as_str).unwrap_or("");
                    if term.is_empty() {
                        continue;
                    }
                    let reading = row.get(1).and_then(Value::as_str).unwrap_or("");
                    let def_tags = row.get(2).and_then(Value::as_str).map(String::from);
                    let rules = row.get(3).and_then(Value::as_str).map(String::from);
                    let score = row.get(4).and_then(Value::as_f64);
                    let tags = row.get(7).and_then(Value::as_str).map(String::from);

                    let p1 = normalize_term(term);
                    let ja = tok
                        .join_tokens(term, Lang::Ja, &lang)
                        .await?
                        .unwrap_or_default();
                    let zh = tok
                        .join_tokens(term, Lang::Zh, &lang)
                        .await?
                        .unwrap_or_default();
                    let ko = tok
                        .join_tokens(term, Lang::Ko, &lang)
                        .await?
                        .unwrap_or_default();
                    let ja = fold_kana(&ja).into_owned();
                    let reading = if lang == Some(Lang::Zh) {
                        normalize_pinyin(reading).unwrap_or_else(|| reading.to_string())
                    } else {
                        reading.to_string()
                    };
                    let p5 = normalize_term(&reading)
                        .non_empty_trimmed()
                        .map(|s| format!(" {s} "))
                        .unwrap_or_default();

                    let (search_id, score): (i64, Option<i64>) = upsert.query_row(
                        params![
                            p1,
                            ja,
                            zh,
                            ko,
                            p5,
                            def_tags.non_empty_trimmed(),
                            rules.non_empty_trimmed(),
                            score.map(|s| s.round() as i64),
                            tags.non_empty_trimmed()
                        ],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    link.execute(params![search_id, dict_id, bank.index, idx as i64])?;
                    for key in roman_keys(&p1, &p5) {
                        roman.execute(params![key, search_id, score])?;
                    }
                    count += 1;
                }
            }
        }
        tx.execute(
            "UPDATE dict SET term_count = ?1 WHERE id = ?2",
            params![count as i64, dict_id],
        )?;
        let char_count = index_kanji(&tx, &mut zip, dict_id)?;
        if char_count > 0 {
            progress(&format!("{}: {char_count} characters", d.name), 0, 0);
        }
        tx.commit()?;

        infos.push(DictInfo {
            name: d.name.clone(),
            title: index.title,
            revision: index.revision,
            lang: lang.map(|l| l.to_string()),
            term_count: count,
        });
    }

    #[cfg(feature = "hangul-readings")]
    {
        progress("Hangul readings", 0, 0);
        fill_hangul_readings(&mut conn)?;
    }

    if !has_existing {
        progress("full-text index", 0, 0);
        conn.execute_batch(
            "INSERT INTO term_search_fts (term_search_fts) VALUES ('rebuild');
             INSERT INTO term_search_fts (term_search_fts) VALUES ('optimize');
             ANALYZE;",
        )?;
        conn.execute_batch("VACUUM;")?;
    }
    drop(conn);
    std::fs::rename(&tmp, out)?;
    Ok(infos)
}

/// Index a dictionary's kanji banks into `kanji_link`: entries for a single character only
/// (some dictionaries, e.g. CC-CEDICT's Hanzi edition, put whole words in kanji banks).
fn index_kanji(
    tx: &rusqlite::Transaction,
    zip: &mut YomitanZip,
    dict_id: i64,
) -> Result<usize, YomitanError> {
    let banks: Vec<_> = zip
        .banks()
        .into_iter()
        .filter(|b| b.kind == BankKind::Kanji)
        .collect();
    let mut link = tx.prepare(
        "INSERT OR IGNORE INTO kanji_link (kanji, dict_id, bank, idx) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut count = 0;
    for bank in banks {
        let rows = zip.read_json_array(&bank.name)?;
        for (idx, row) in rows.iter().enumerate() {
            let kanji = row.get(0).and_then(Value::as_str).unwrap_or("").trim();
            if !ids::is_han_char(kanji) {
                continue;
            }
            link.execute(params![kanji, dict_id, bank.index, idx as i64])?;
            count += 1;
        }
    }
    Ok(count)
}

/// FNV-1a, to tell whether `ids.txt` changed since the last [`build_chars`]
fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{}:{h:016x}", bytes.len())
}

/// Fill the character tables of `db` (built by [`build`]) from `ids_txt`, the CJKVI IDS
/// database's `ids.txt` ([`crate::ids`]): each character's decomposition, its components,
/// its Hangul readings (with the `hangul-readings` feature), and how many terms use it, to
/// list the common characters made of a component first.
///
/// Does nothing, and returns `false`, when `ids.txt` is unchanged since it last filled them
/// and `recount` is false; pass `recount` when dictionaries changed, since the counts follow
/// the terms. Leaving search.db as it was keeps its checksum, so the app does not download
/// it again.
pub fn build_chars(db: &Path, ids_txt: &Path, recount: bool) -> Result<bool, YomitanError> {
    let text = std::fs::read_to_string(ids_txt)?;
    let fp = fingerprint(text.as_bytes());

    let mut conn = Connection::open(db)?;
    conn.execute_batch(HAN_SCHEMA)?;
    conn.execute_batch(META_SCHEMA)?;
    let stored: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'ids'", [], |r| r.get(0))
        .optional()?;
    let empty = conn
        .query_row("SELECT 1 FROM han_char LIMIT 1", [], |_| Ok(()))
        .optional()?
        .is_none();
    let han_current = !recount && !empty && stored.as_deref() == Some(fp.as_str());
    let term_char_current =
        !recount && meta(&conn, "term_char")?.as_deref() == Some(TERM_CHAR_VERSION);
    if han_current && term_char_current {
        return Ok(false);
    }
    if !han_current {
        fill_han(&mut conn, &text, fp)?;
    }
    if !term_char_current {
        fill_term_char(&mut conn, db.parent().unwrap_or(Path::new(".")))?;
    }
    conn.execute_batch("ANALYZE;")?;
    Ok(true)
}

/// The character tables from ids.txt (`text`, fingerprinted `fp`)
fn fill_han(conn: &mut Connection, text: &str, fp: String) -> Result<(), YomitanError> {
    // terms with each character
    let mut uses: HashMap<String, i64> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT term FROM term_search")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let term = r.get_ref(0)?.as_str().unwrap_or("");
            for ch in ids::han_chars(term) {
                *uses.entry(ch).or_insert(0) += 1;
            }
        }
    }

    let tx = conn.transaction()?;
    tx.execute_batch("DELETE FROM han_char; DELETE FROM han_component;")?;
    {
        let mut ins_char = tx.prepare(
            "INSERT OR REPLACE INTO han_char (ch, ids, hangul, uses) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut ins_comp =
            tx.prepare("INSERT OR IGNORE INTO han_component (component, ch) VALUES (?1, ?2)")?;
        let mut add = |ch: &str, ids: &str, comps: &[String]| -> Result<(), YomitanError> {
            ins_char.execute(params![
                ch,
                ids,
                hangul_of(ch),
                uses.get(ch).copied().unwrap_or(0)
            ])?;
            for c in comps {
                ins_comp.execute(params![c, ch])?;
            }
            Ok(())
        };
        let mut seen = std::collections::HashSet::new();
        for line in text.lines() {
            let Some(l) = ids::parse_line(line) else {
                continue;
            };
            let cols = l
                .ids
                .iter()
                .map(|i| {
                    if i.regions.is_empty() {
                        i.ids.clone()
                    } else {
                        format!("{}[{}]", i.ids, i.regions)
                    }
                })
                .collect::<Vec<_>>()
                .join("\t");
            add(&l.ch, &cols, &ids::line_components(&l))?;
            seen.insert(l.ch);
        }
        // characters of the kanji dictionaries that ids.txt does not have
        let extra: Vec<String> = {
            let mut st = tx.prepare("SELECT DISTINCT kanji FROM kanji_link")?;
            st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
        };
        for ch in extra {
            if !seen.contains(&ch) {
                add(&ch, "", &[])?;
            }
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('ids', ?1)",
        [fp],
    )?;
    tx.commit()?;
    Ok(())
}

/// `term_char`: the readings of each term's characters. Japanese needs kanji readings, from
/// the kanji dictionaries (zips in `dir`) whose readings are kana, e.g. KANJIDIC.
fn fill_term_char(conn: &mut Connection, dir: &Path) -> Result<(), YomitanError> {
    conn.execute_batch(TERM_CHAR_SCHEMA)?;
    let mut kana = KanaReadings::default();
    if has_table(conn, "kanji_link")? {
        let names: Vec<String> = conn
            .prepare("SELECT name FROM dict WHERE id IN (SELECT DISTINCT dict_id FROM kanji_link)")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for name in names {
            let path = dir.join(format!("{name}.zip"));
            if !path.is_file() {
                continue;
            }
            let mut zip = YomitanZip::open(&path)?;
            for bank in zip
                .banks()
                .into_iter()
                .filter(|b| b.kind == BankKind::Kanji)
            {
                for row in zip.read_json_array(&bank.name)? {
                    let s = |i: usize| row.get(i).and_then(Value::as_str).unwrap_or("");
                    let kanji = s(0);
                    kana.add(kanji, s(1));
                    kana.add(kanji, s(2));
                    // KANJIDIC's name readings, as dict-admin converts them
                    if let Some(n) = row
                        .get(5)
                        .and_then(|v| v.get("nanori"))
                        .and_then(Value::as_str)
                    {
                        kana.add(kanji, n);
                    }
                }
            }
        }
    }

    let mut rows: Vec<(String, String, i64)> = vec![];
    {
        let mut st =
            conn.prepare("SELECT id, term, reading FROM term_search WHERE reading != ''")?;
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: i64 = r.get(0)?;
            let term = r.get_ref(1)?.as_str().unwrap_or("");
            let reading = r.get_ref(2)?.as_str().unwrap_or("");
            for (ch, rd) in char_reading::align(term, reading, &kana) {
                rows.push((ch, rd, id));
            }
        }
    }
    // in key order: the table is written once, without page splits
    rows.sort();
    rows.dedup();

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM term_char", [])?;
    {
        let mut ins =
            tx.prepare("INSERT INTO term_char (ch, reading, search_id) VALUES (?1, ?2, ?3)")?;
        for (ch, rd, id) in &rows {
            ins.execute(params![ch, rd, id])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('term_char', ?1)",
        [TERM_CHAR_VERSION],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('kana_kanji', ?1)",
        [kana.len().to_string()],
    )?;
    tx.commit()?;
    Ok(())
}

/// Hangul readings (eum) of a Hanja, as a dictionary lists them: without the initial-sound
/// rule, so 李 is 리.
#[cfg(feature = "hangul-readings")]
fn hangul_of(ch: &str) -> String {
    let mut cs = ch.chars();
    match (cs.next(), cs.next()) {
        (Some(c), None) if hanja_hangul::is_hanja(c) => hanja_hangul::Converter::global()
            .readings(ch, false, 8)
            .join(" "),
        _ => String::new(),
    }
}

#[cfg(not(feature = "hangul-readings"))]
fn hangul_of(_ch: &str) -> String {
    String::new()
}

/// Korean dictionaries give no readings: give Korean headwords written in Hanja one in
/// Hangul ([`crate::hanja::hangul_readings`]), so that `{광속}` finds 光速. Of the readings a
/// word with characters of several readings can have (樂: 락, 낙, 악, 요), the one that is a
/// Hangul headword too is kept, else the most likely one.
///
/// Runs after every build, so an update fills in a search.db from before this, and the rows
/// of a re-indexed dictionary (inserted without a reading) are merged into the rows that
/// already have one.
#[cfg(feature = "hangul-readings")]
fn fill_hangul_readings(conn: &mut Connection) -> Result<usize, YomitanError> {
    use crate::hanja::hangul_readings;
    use hanja_hangul::contains_hanja;

    let tx = conn.transaction()?;
    let mut filled = 0;
    {
        // Korean only: a dictionary without a language gives Han terms tokens of every language
        let mut st = tx.prepare(
            "SELECT id, term FROM term_search
             WHERE reading = '' AND term_ko != '' AND term_ja = '' AND term_zh = ''",
        )?;
        let mut rows: Vec<(i64, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows.retain(|(_, term)| contains_hanja(term));

        let mut is_headword =
            tx.prepare("SELECT 1 FROM term_search WHERE term = ?1 AND term_ko != '' LIMIT 1")?;
        let mut set = tx.prepare(
            "UPDATE OR IGNORE term_search SET reading = ?2 WHERE id = ?1 RETURNING score",
        )?;
        // the row already there with this term and reading, for the same languages
        let mut same = tx.prepare(
            "SELECT t.id FROM term_search s JOIN term_search t ON t.term = s.term
             WHERE s.id = ?1 AND t.reading = ?2 AND t.id != s.id
               AND (t.term_ja != '') = (s.term_ja != '') AND (t.term_zh != '') = (s.term_zh != '')
               AND (t.term_ko != '') = (s.term_ko != '')",
        )?;
        // merged like the build's upsert
        let mut merge = tx.prepare(
            "UPDATE term_search AS t SET
               def_tags = CASE WHEN t.def_tags IS s.def_tags THEN t.def_tags ELSE NULLIF(trim(printf('%s %s', t.def_tags, s.def_tags)), '') END,
               rules = CASE WHEN t.rules IS s.rules THEN t.rules ELSE NULLIF(trim(printf('%s %s', t.rules, s.rules)), '') END,
               tags = CASE WHEN t.tags IS s.tags THEN t.tags ELSE NULLIF(trim(printf('%s %s', t.tags, s.tags)), '') END,
               score = max(coalesce(t.score, s.score), coalesce(s.score, t.score))
             FROM term_search AS s WHERE t.id = ?1 AND s.id = ?2
             RETURNING score",
        )?;
        let mut move_links = tx.prepare(
            "INSERT OR IGNORE INTO term_link (search_id, dict_id, bank, idx)
             SELECT ?1, dict_id, bank, idx FROM term_link WHERE search_id = ?2",
        )?;
        let mut drop_links = tx.prepare("DELETE FROM term_link WHERE search_id = ?1")?;
        let mut drop_roman = tx.prepare("DELETE FROM term_roman WHERE search_id = ?1")?;
        let mut drop_row = tx.prepare("DELETE FROM term_search WHERE id = ?1")?;
        let mut roman_score =
            tx.prepare("UPDATE term_roman SET score = ?2 WHERE search_id = ?1")?;
        let mut roman = tx.prepare(ROMAN_UPSERT)?;

        for (id, term) in rows {
            let readings = hangul_readings(&term);
            let Some(mut reading) = readings.first().cloned() else {
                continue;
            };
            if readings.len() > 1 {
                for r in &readings {
                    if is_headword.exists([normalize_text(r)])? {
                        reading = r.clone();
                        break;
                    }
                }
            }
            let reading = format!(" {} ", normalize_text(&reading));

            let (target, score): (i64, Option<i64>) = match set
                .query_row(params![id, reading], |r| r.get(0))
                .optional()?
            {
                Some(score) => (id, score),
                None => {
                    let Some(target) = same
                        .query_row(params![id, reading], |r| r.get::<_, i64>(0))
                        .optional()?
                    else {
                        continue;
                    };
                    let score = merge.query_row(params![target, id], |r| r.get(0))?;
                    move_links.execute(params![target, id])?;
                    drop_links.execute([id])?;
                    drop_roman.execute([id])?;
                    drop_row.execute([id])?;
                    roman_score.execute(params![target, score])?;
                    (target, score)
                }
            };
            for key in roman_keys(&term, &reading) {
                roman.execute(params![key, target, score])?;
            }
            filled += 1;
        }
    }
    tx.commit()?;
    Ok(filled)
}

#[cfg(test)]
mod incremental_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn incremental_build_removes_deleted_dictionaries() {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("cjk-dict-incremental-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("search.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch(FTS_TRIGGERS).unwrap();
            conn.execute(
                "INSERT INTO dict (name, title) VALUES ('removed', 'Removed')",
                [],
            )
            .unwrap();
            let search_id = conn
                .query_row(
                    "INSERT INTO term_search (term, term_ja) VALUES ('TEST', 'test') RETURNING id",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap();
            conn.execute(
                "INSERT INTO term_link (search_id, dict_id, bank, idx) VALUES (?1, 1, 1, 0)",
                [search_id],
            )
            .unwrap();
        }

        let tok = TokenizerMapper::new(dir.clone());
        build_incremental(&db_path, &[], &["removed".to_string()], &tok, |_, _, _| {})
            .await
            .unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let dict_count = conn
            .query_row("SELECT count(*) FROM dict", [], |row| row.get::<_, i64>(0))
            .unwrap();
        let term_count = conn
            .query_row(
                "SELECT count(*) FROM term_search_fts WHERE term_search_fts MATCH 'test'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(dict_count, 0);
        assert_eq!(term_count, 0);
        // Windows can't delete a file that is still open
        drop(conn);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod reading_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// search.db with a few terms and no zips (entries come back empty)
    fn fixture() -> (PathBuf, SearchDb) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("cjk-dict-reading-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("search.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(FTS_TRIGGERS).unwrap();
        for (term, reading) in [
            ("日本", " にほん "),
            ("日本", " にっぽん "),
            ("日本語", " にほんご "),
            ("日本橋", " にほんばし "),
            ("日本橋", " にっぽんばし "),
        ] {
            conn.execute(
                "INSERT INTO term_search (term, term_ja, reading) VALUES (?1, ?1, ?2)",
                [term, reading],
            )
            .unwrap();
        }
        drop(conn);
        let db = SearchDb::open(&db_path, &dir).unwrap();
        (dir, db)
    }

    fn found(db: &SearchDb, req: SearchRequest) -> Vec<(String, String)> {
        let mut out: Vec<_> = db
            .search(&req)
            .unwrap()
            .items
            .into_iter()
            .map(|i| (i.term, i.reading.trim().to_string()))
            .collect();
        out.sort();
        out
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn reading_glob_narrows_a_term_search() {
        let (dir, db) = fixture();
        let req = |kind: Option<&str>, term: &str, glob: &str| SearchRequest {
            kind: kind.map(String::from),
            term: term.into(),
            reading_glob: glob.into(),
            ..Default::default()
        };

        // <日本>{にほん} (prefix): both readings that start with にほん
        assert_eq!(
            found(&db, req(None, "日本", "にほん")),
            pairs(&[
                ("日本", "にほん"),
                ("日本橋", "にほんばし"),
                ("日本語", "にほんご")
            ])
        );
        // =<日本>{にほん}
        assert_eq!(
            found(&db, req(Some("exact"), "日本", "にほん")),
            pairs(&[("日本", "にほん")])
        );
        // =<日本>{にっぽん}橋: the unknown 橋 reading is a wildcard
        assert_eq!(
            found(&db, req(Some("exact"), "日本橋", "にっぽん*")),
            pairs(&[("日本橋", "にっぽんばし")])
        );
        // a pattern of wildcards only does not filter
        assert_eq!(found(&db, req(Some("exact"), "日本", "*")).len(), 2);
        // Windows can't delete a file that is still open
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// `*`, `?` and `[...]` in a term are GLOB wildcards, except in an exact search
    #[test]
    fn wildcards_in_a_term() {
        let (dir, db) = fixture();
        let req = |kind: Option<&str>, term: &str| SearchRequest {
            kind: kind.map(String::from),
            term: term.into(),
            ..Default::default()
        };
        let terms = |r| -> Vec<String> {
            let mut v: Vec<String> = found(&db, r).into_iter().map(|(t, _)| t).collect();
            v.sort();
            v.dedup();
            v
        };
        assert_eq!(terms(req(None, "?本")), ["日本", "日本橋", "日本語"]);
        assert_eq!(terms(req(None, "*語")), ["日本語"]);
        assert_eq!(terms(req(None, "日本[語橋]")), ["日本橋", "日本語"]);
        assert_eq!(terms(req(None, "日本[^語]")), ["日本橋"]);
        assert!(terms(req(Some("exact"), "日?")).is_empty());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reading_alone_is_a_prefix_search_unless_exact() {
        let (dir, db) = fixture();
        let req = |kind: Option<&str>, reading: &str| SearchRequest {
            kind: kind.map(String::from),
            lang: "ja-JP".into(),
            reading: reading.into(),
            ..Default::default()
        };
        assert_eq!(
            found(&db, req(None, "にっぽん")),
            pairs(&[("日本", "にっぽん"), ("日本橋", "にっぽんばし")])
        );
        assert_eq!(
            found(&db, req(Some("exact"), "にっぽん")),
            pairs(&[("日本", "にっぽん")])
        );
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod romanized_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn found(db: &SearchDb, term: &str, kind: Option<&str>, romanized: bool) -> Vec<String> {
        db.search(&SearchRequest {
            kind: kind.map(String::from),
            lang: "ko-KR".into(),
            term: term.into(),
            romanized,
            ..Default::default()
        })
        .unwrap()
        .items
        .into_iter()
        .map(|i| i.term)
        .collect()
    }

    fn check(db: &SearchDb) {
        for typed in ["hangugeo", "hangukeo", "hangug-eo"] {
            assert_eq!(found(db, typed, Some("exact"), true), ["한국어"], "{typed}");
            // prefix: the exact match first, then by score
            assert_eq!(
                found(db, typed, None, true),
                ["한국어", "한국어학"],
                "{typed}"
            );
        }
        assert_eq!(
            found(db, "hanguk", None, true),
            ["한국", "한국어", "한국어학"]
        );
        assert_eq!(found(db, "gungmul", Some("exact"), true), ["국물"]);
        assert_eq!(found(db, "gukmul", Some("exact"), true), ["국물"]);
        assert_eq!(found(db, "silla", Some("exact"), true), ["신라"]);
        assert_eq!(found(db, "gachi", Some("exact"), true), ["같이"]);
        // cut off in the middle of eu: se → su
        assert_eq!(found(db, "se", None, true), ["세우다"]);
        // Hangul is unaffected
        assert_eq!(found(db, "한국어", Some("exact"), true), ["한국어"]);
        // off: a plain term search (nothing Korean is spelled in Latin letters here)
        assert!(found(db, "hangugeo", Some("exact"), false).is_empty());
    }

    /// A letter or two matches too many keys to sort them all: exact matches, then the rest
    /// by score, page by page.
    #[tokio::test]
    async fn common_prefix_walks_by_score() {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "cjk-dict-romanized-common-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("search.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(ROMAN_SCHEMA).unwrap();
        let mut terms: Vec<(String, i64)> = vec![
            ("하늘".into(), 50),
            ("한국".into(), 40),
            ("하".into(), 1),
            ("해".into(), 2),
            ("나라".into(), 99),
        ];
        // filler: 하가, 하각, ... with score 0
        terms.extend(
            (0..=COMMON_PREFIX_KEYS as u32)
                .map(|i| char::from_u32(0xAC00 + i).unwrap())
                .filter(|&c| c != '늘')
                .map(|c| (format!("하{c}"), 0)),
        );
        conn.execute_batch("BEGIN").unwrap();
        for (term, score) in &terms {
            let id: i64 = conn
                .query_row(
                    "INSERT INTO term_search (term, term_ko, score) VALUES (?1, ?1, ?2) RETURNING id",
                    params![term, score],
                    |r| r.get(0),
                )
                .unwrap();
            for key in korean::keys(term) {
                conn.execute(ROMAN_UPSERT, params![key, id, score]).unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        drop(conn);
        let db = SearchDb::open(&db_path, &dir).unwrap();
        let page = |offset: u32| -> Vec<String> {
            db.search(&SearchRequest {
                lang: "ko-KR".into(),
                term: "ha".into(),
                romanized: true,
                limit: Some(3),
                offset: Some(offset),
                ..Default::default()
            })
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.term)
            .collect()
        };
        // ha, and he (ae) for an unfinished hae, are exact: 해, 하
        assert_eq!(page(0), ["해", "하", "하늘"]);
        assert_eq!(page(1), ["하", "하늘", "한국"]);
        assert_eq!(page(3).len(), 3);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A search.db from before `term_roman` is searched by romanizing each row, and gets the
    /// table on its next update.
    #[tokio::test]
    async fn romanization_finds_hangul_terms() {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("cjk-dict-romanized-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("search.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute("INSERT INTO dict (name, title) VALUES ('ko', 'Korean')", [])
            .unwrap();
        for (term, score) in [
            ("한국어", 10),
            ("한국", 20),
            ("한국어학", 5),
            ("국물", 0),
            ("신라", 0),
            ("같이", 0),
            ("세우다", 0),
            ("하늘", 0),
        ] {
            conn.execute(
                "INSERT INTO term_search (term, term_ko, score) VALUES (?1, ?1, ?2)",
                params![term, score],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO term_search (term, term_ja) VALUES ('HANGUGEO', 'HANGUGEO')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO term_link (search_id, dict_id, bank, idx) SELECT id, 1, 1, id FROM term_search",
            [],
        )
        .unwrap();
        drop(conn);

        let db = SearchDb::open(&db_path, &dir).unwrap();
        assert!(!db.has_roman);
        check(&db);
        drop(db);

        let tok = TokenizerMapper::new(dir.clone());
        build_incremental(&db_path, &[], &[], &tok, |_, _, _| {})
            .await
            .unwrap();
        let db = SearchDb::open(&db_path, &dir).unwrap();
        assert!(db.has_roman);
        check(&db);

        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchRequest {
    /// "fts" = token search; "exact" = term equality; anything else = prefix search
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// e.g. "ja-JP"; empty = all languages
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub term: String,
    /// pinyin is expanded to every tone combination when lang is zh-CN or empty
    #[serde(default)]
    pub reading: String,
    /// GLOB pattern over the whole reading (e.g. `にほん*` from `<日本>{にほん}語`), applied
    /// on top of `term`. Prefix search also matches readings that only start with it.
    #[serde(default)]
    pub reading_glob: String,
    /// fts: tokenized `term`, space-separated, per short language ("ja", "zh", "ko")
    #[serde(default)]
    pub tokens: HashMap<String, String>,
    /// `term` may be Korean romanization (`hangugeo` for 한국어), matched against how Hangul
    /// terms are romanized. Ignored unless `term` is all Latin letters. "fts" is a prefix
    /// search then.
    #[serde(default)]
    pub romanized: bool,
    /// With `ch_reading`: the words with the character `ch` read so anywhere in them
    /// (`*<興>{キョウ}`), by search.db's index of characters' readings
    /// ([`crate::char_reading`]). A search.db without it matches `*ch` and `*ch_reading`.
    /// Replaces `term` and the readings.
    #[serde(default)]
    pub ch: String,
    #[serde(default)]
    pub ch_reading: String,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SearchItem {
    pub id: i64,
    pub term: String,
    pub reading: String,
    pub def_tags: Option<String>,
    pub rules: Option<String>,
    pub score: Option<i64>,
    pub tags: Option<String>,
    pub entries: Vec<TermEntryOut>,
}

/// One dictionary entry, read from the zip (same shape the Vue components render).
#[derive(Debug, Serialize)]
pub struct TermEntryOut {
    pub dict_id: i64,
    /// dictionary name
    pub dict_path: String,
    pub term_id: i64,
    pub term: String,
    pub reading: String,
    pub def_tags: String,
    pub rules: String,
    pub score: Option<f64>,
    /// JSON array
    pub glossary: String,
    pub sequence: Option<i64>,
    pub tags: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub items: Vec<SearchItem>,
    pub next: Option<u32>,
}

/// A Han character: how it breaks down ([`crate::ids`]) and its kanji dictionary entries.
#[derive(Debug, Default, Serialize)]
pub struct HanChar {
    pub ch: String,
    /// the ways to write it, from ids.txt; empty if search.db has no decomposition for it
    pub ids: Vec<Ids>,
    /// the components the IDS name, from all of them
    pub components: Vec<String>,
    /// every component, broken down all the way
    pub all_components: Vec<String>,
    /// Hangul readings (eum)
    pub hangul: Vec<String>,
    /// terms with the character
    pub uses: i64,
    pub entries: Vec<KanjiEntryOut>,
}

/// One kanji dictionary entry, read from the zip.
#[derive(Debug, Serialize)]
pub struct KanjiEntryOut {
    pub dict_id: i64,
    /// dictionary name
    pub dict_path: String,
    /// the dictionary's source language
    pub lang: Option<String>,
    pub character: String,
    /// space-separated
    pub onyomi: String,
    /// space-separated
    pub kunyomi: String,
    /// space-separated
    pub tags: String,
    pub meanings: Vec<String>,
    /// (name, value) by name, e.g. ("strokes", "4"); `tag_meta` has their sort order
    pub stats: Vec<(String, String)>,
    /// what the dictionary's tag banks say about `tags` and the stats' names
    pub tag_meta: HashMap<String, TagMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagMeta {
    pub category: String,
    pub order: f64,
    pub notes: String,
}

#[derive(Debug, Serialize)]
pub struct HanBrief {
    pub ch: String,
    pub uses: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct HanList {
    pub items: Vec<HanBrief>,
    pub next: Option<u32>,
}

/// characters [`SearchDb::han_chars`] looks up at once
pub const MAX_HAN_CHARS: usize = 20;
const MAX_HAN_LIST: u32 = 200;

const MAX_LIMIT: u32 = 50;
/// 5^4: up to four toneless syllables are expanded; beyond that, match the raw reading
const MAX_READINGS: usize = 625;
/// Term banks kept in memory, by JSON size. dict-admin cuts banks to ~256 KB
/// ([`crate::parser::rebank`]), but zips from before that have 3-30 MB banks.
const BANK_CACHE_BYTES: usize = 64 << 20;

/// (dict_id, kanji bank?, N of `term_bank_N.json` / `kanji_bank_N.json`)
type BankKey = (i64, bool, i64);
/// A term bank split into rows, each still raw JSON: splitting is far cheaper than parsing
/// the whole bank into `Value`s, and a search only needs a handful of rows from it.
type Bank = Arc<Vec<Box<RawValue>>>;

pub struct SearchDb {
    conn: Mutex<Connection>,
    dicts_dir: PathBuf,
    /// dict_id -> name
    names: HashMap<i64, String>,
    /// dict_id -> source language, e.g. ja-JP
    langs: HashMap<i64, String>,
    /// search.db has `kanji_link` / `han_char`
    has_kanji: bool,
    has_han: bool,
    /// dict_id -> its tag banks, for kanji entries' tags and stats
    tag_meta: Mutex<HashMap<i64, Arc<HashMap<String, TagMeta>>>>,
    /// search.db has `term_roman`; built before it, romanized searches romanize each row
    has_roman: bool,
    /// search.db has katakana as hiragana ([`normalize_term`]); queries are folded alike
    kana_folded: bool,
    /// search.db has `term_char`, filled
    has_term_char: bool,
    zips: Mutex<HashMap<String, YomitanZip>>,
    /// most recently used first; (key, rows, JSON bytes)
    banks: Mutex<VecDeque<(BankKey, Bank, usize)>>,
}

/// `ko_roman_rank(term, query)`: [`RomanQuery::rank`], `query` from [`RomanQuery::encode`].
fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    let cache: Mutex<Option<(String, RomanQuery)>> = Mutex::new(None);
    conn.create_scalar_function(
        "ko_roman_rank",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let term = ctx.get_raw(0).as_str().unwrap_or("");
            let encoded = ctx.get_raw(1).as_str().unwrap_or("");
            let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.as_ref().is_none_or(|(k, _)| k != encoded) {
                *cache = Some((encoded.to_string(), RomanQuery::decode(encoded)));
            }
            Ok(cache.as_ref().map_or(0, |(_, q)| q.rank(term)) as i64)
        },
    )
}

/// A prefix whose keys reach this many is common (a letter or two): walking the keys by
/// score finds a page sooner than sorting every match.
const COMMON_PREFIX_KEYS: usize = 1000;

/// Romanized search over `term_roman`: terms romanizing to the query first, then (prefix
/// search) those starting with it, each by score. `lang_filter`: SQL ending in `AND`, or "".
fn search_roman(
    conn: &Connection,
    roman: &RomanQuery,
    lang_filter: &str,
    limit: u32,
    offset: u32,
    map_row: impl Fn(&rusqlite::Row) -> rusqlite::Result<SearchItem>,
) -> Result<Vec<SearchItem>, YomitanError> {
    let patterns = roman.patterns();
    let exact_in = (1..=patterns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    // key ranges `>= p AND < p{` ('{' follows 'z'), with the patterns as ?1..?n
    let mut ps: Vec<String> = patterns.to_vec();
    let mut ranges = vec![];
    for (i, p) in patterns.iter().enumerate() {
        ps.push(format!("{p}{{"));
        ranges.push(format!("(key >= ?{} AND key < ?{})", i + 1, ps.len()));
    }
    let ranges = ranges.join(" OR ");

    let common = roman.prefix
        && {
            let sql = format!(
                "SELECT count(*) FROM (SELECT 1 FROM term_roman WHERE {ranges} LIMIT {COMMON_PREFIX_KEYS})"
            );
            let n: i64 = conn
                .prepare_cached(&sql)?
                .query_row(params_from_iter(ps.iter()), |r| r.get(0))?;
            n as usize >= COMMON_PREFIX_KEYS
        };

    if !common {
        // few matches: sort them all
        let (wh, ps) = if roman.prefix {
            (ranges, &ps[..])
        } else {
            (format!("key IN ({exact_in})"), patterns)
        };
        let sql = format!(
            "SELECT s.*, max(r.key IN ({exact_in})) AS roman_exact
             FROM term_roman r JOIN term_search s ON s.id = r.search_id
             WHERE {lang_filter}({wh})
             GROUP BY s.id ORDER BY roman_exact DESC, s.score DESC LIMIT {limit} OFFSET {offset}"
        );
        let mut st = conn.prepare_cached(&sql)?;
        return Ok(st
            .query_map(params_from_iter(ps.iter()), map_row)?
            .collect::<Result<_, _>>()?);
    }

    // exact matches (few, by the key index), then the keys in the prefix ranges, walking the
    // score index until a page is found. A term has up to two keys: skip the repeats.
    let mut ids: Vec<i64> = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut skip = offset as usize;
    let mut take = |id: i64, ids: &mut Vec<i64>| {
        if seen.insert(id) {
            if skip > 0 {
                skip -= 1;
            } else {
                ids.push(id);
            }
        }
        ids.len() < limit as usize
    };
    let mut st = conn.prepare_cached(&format!(
        "SELECT search_id FROM term_roman WHERE key IN ({exact_in}) ORDER BY score DESC"
    ))?;
    let mut rows = st.query(params_from_iter(patterns.iter()))?;
    let mut more = true;
    while more && let Some(r) = rows.next()? {
        more = take(r.get(0)?, &mut ids);
    }
    drop(rows);
    let mut st = conn.prepare_cached(&format!(
        "SELECT search_id FROM term_roman INDEXED BY idx_term_roman_score
         WHERE {ranges} ORDER BY score DESC"
    ))?;
    let mut rows = st.query(params_from_iter(ps.iter()))?;
    while more && let Some(r) = rows.next()? {
        more = take(r.get(0)?, &mut ids);
    }
    drop(rows);

    let mut st = conn.prepare_cached(&format!(
        "SELECT * FROM term_search WHERE {lang_filter}id = ?1"
    ))?;
    let mut out = vec![];
    for id in ids {
        if let Some(item) = st.query_row([id], &map_row).optional()? {
            out.push(item);
        }
    }
    Ok(out)
}

fn fts_phrase(s: &str) -> String {
    format!("\"{}\"", s.replace('"', " "))
}

/// `prefix`: the last syllable may be incomplete, e.g. "にほん" matches "にほんご".
fn reading_match(readings: &[String], prefix: bool) -> String {
    let star = if prefix { "*" } else { "" };
    let q = readings
        .iter()
        .map(|r| format!("{}{star}", fts_phrase(r)))
        .collect::<Vec<_>>()
        .join(" OR ");
    if readings.len() > 1 {
        format!("reading:({q})")
    } else {
        format!("reading:{q}")
    }
}

impl SearchDb {
    /// `dicts_dir` holds `<name>.zip` for every dict in the db.
    pub fn open(db: &Path, dicts_dir: &Path) -> Result<Self, YomitanError> {
        let conn = Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        register_functions(&conn)?;
        let names = {
            let mut st = conn.prepare("SELECT id, name FROM dict")?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<HashMap<i64, String>, _>>()?
        };
        let langs = {
            let mut st = conn.prepare("SELECT id, L1 FROM dict WHERE L1 IS NOT NULL")?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<HashMap<i64, String>, _>>()?
        };
        let has_roman = has_table(&conn, "term_roman")?;
        let kana_folded = meta(&conn, "kana")?.is_some();
        let has_term_char = meta(&conn, "term_char")?.is_some();
        let has_kanji = has_table(&conn, "kanji_link")?;
        let has_han = has_table(&conn, "han_char")?;
        Ok(Self {
            has_roman,
            kana_folded,
            has_term_char,
            has_kanji,
            has_han,
            langs,
            tag_meta: Mutex::new(HashMap::new()),
            conn: Mutex::new(conn),
            dicts_dir: dicts_dir.to_path_buf(),
            names,
            zips: Mutex::new(HashMap::new()),
            banks: Mutex::new(VecDeque::new()),
        })
    }

    pub fn dict_names(&self) -> Vec<String> {
        self.names.values().cloned().collect()
    }

    /// Query text as search.db stores terms: [`normalize_term`], or [`normalize_text`] for a
    /// search.db from before kana folding
    fn norm(&self, text: &str) -> String {
        if self.kana_folded {
            normalize_term(text)
        } else {
            normalize_text(text)
        }
    }

    fn fold<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if self.kana_folded {
            fold_kana(text)
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }

    pub fn search(&self, req: &SearchRequest) -> Result<SearchResult, YomitanError> {
        let limit = req.limit.unwrap_or(10).clamp(1, MAX_LIMIT);
        let offset = req.offset.unwrap_or(0);
        let lang = req.lang.split('-').next().unwrap_or("");
        let lang = if ["ja", "zh", "ko"].contains(&lang) {
            lang
        } else {
            ""
        };

        let ch = req.ch.trim();
        let ch_reading = req.ch_reading.trim();
        let ch_forms = if ch.is_empty() || ch_reading.is_empty() {
            None
        } else if self.has_term_char {
            Some((self.norm(ch), char_reading::query_forms(ch_reading)))
        } else {
            // a search.db from before term_char: words with the character, read so somewhere
            // kana and Hangul as a pattern over the reading; pinyin as reading tokens
            let pattern = !ch_reading.chars().any(|c| c.is_ascii_alphabetic());
            return self.search(&SearchRequest {
                lang: req.lang.clone(),
                term: format!("*{ch}"),
                reading_glob: if pattern {
                    format!("*{}", ch_reading.split('.').next().unwrap_or(""))
                } else {
                    String::new()
                },
                reading: if pattern {
                    String::new()
                } else {
                    ch_reading.to_string()
                },
                limit: req.limit,
                offset: req.offset,
                ..Default::default()
            });
        };

        let reading = req.reading.trim();
        let readings: Vec<String> = if reading.is_empty() {
            vec![]
        } else {
            let expanded = if lang.is_empty() || lang == "zh" {
                expand_pinyin(reading)
            } else {
                vec![]
            };
            if !expanded.is_empty() && expanded.len() <= MAX_READINGS {
                expanded
            } else {
                vec![self.fold(reading).into_owned()]
            }
        };

        let mut items: Vec<SearchItem> = {
            let conn = self.conn.lock()?;
            let map_row = |r: &rusqlite::Row| {
                Ok(SearchItem {
                    id: r.get("id")?,
                    term: r.get("term")?,
                    reading: r.get("reading")?,
                    def_tags: r.get("def_tags")?,
                    rules: r.get("rules")?,
                    score: r.get("score")?,
                    tags: r.get("tags")?,
                    entries: vec![],
                })
            };

            let is_exact = req.kind.as_deref() == Some("exact");
            let roman = req
                .romanized
                .then(|| RomanQuery::new(req.term.trim(), !is_exact))
                .flatten();

            let lang_filter = if lang.is_empty() {
                String::new()
            } else {
                format!("term_{lang} != '' AND ")
            };

            // Korean: a Hangul word is its own reading. Typed as a term or as `{reading}`, it
            // finds Hangul headwords spelled so and Hanja headwords read so (광속: 光速).
            let is_fts = req.kind.as_deref() == Some("fts");
            let hangul_word = if roman.is_some() || is_fts || !["", "ko"].contains(&lang) {
                None
            } else if reading.is_empty() && req.reading_glob.trim().is_empty() {
                Some(req.term.trim())
            } else if req.term.trim().is_empty() && readings.len() == 1 {
                Some(reading)
            } else {
                None
            }
            .filter(|w| korean::is_hangul(w))
            .map(|w| self.norm(w));

            if let Some((ch, forms)) = &ch_forms {
                let mut ps: Vec<&str> = vec![ch.as_str()];
                ps.extend(forms.iter().map(String::as_str));
                let sql = format!(
                    "SELECT * FROM term_search WHERE {lang_filter}id IN (
                       SELECT search_id FROM term_char WHERE ch = ?1 AND reading IN ({})
                     ) ORDER BY score DESC LIMIT {} OFFSET {}",
                    (2..=ps.len())
                        .map(|i| format!("?{i}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    limit + 1,
                    offset
                );
                let mut st = conn.prepare_cached(&sql)?;
                st.query_map(params_from_iter(ps.iter()), map_row)?
                    .collect::<Result<_, _>>()?
            } else if let Some(word) = hangul_word {
                let (op, term) = if is_exact {
                    ("=", word.clone())
                } else {
                    ("GLOB", format!("{word}*"))
                };
                let sql = format!(
                    "SELECT * FROM term_search WHERE {lang_filter}id IN (
                       SELECT id FROM term_search WHERE term {op} ?1
                       UNION SELECT rowid FROM term_search_fts WHERE term_search_fts MATCH ?2
                     ) ORDER BY score DESC LIMIT {} OFFSET {}",
                    limit + 1,
                    offset
                );
                let mut st = conn.prepare_cached(&sql)?;
                st.query_map(params![term, reading_match(&[word], !is_exact)], map_row)?
                    .collect::<Result<_, _>>()?
            } else if let (Some(roman), true) = (&roman, self.has_roman) {
                search_roman(&conn, roman, &lang_filter, limit + 1, offset, map_row)?
            } else if let Some(roman) = roman {
                // search.db from before `term_roman`: the rows whose first character the query
                // can start with, by the index on `term`; then romanize each one
                let ranges = roman.first_char_ranges();
                let mut ps: Vec<String> = vec![roman.encode()];
                let mut or = vec![];
                for (a, b) in ranges {
                    ps.push(a.to_string());
                    ps.push(b.to_string());
                    or.push(format!(
                        "(term >= ?{} AND term < ?{})",
                        ps.len() - 1,
                        ps.len()
                    ));
                }
                if or.is_empty() {
                    vec![]
                } else {
                    let sql = format!(
                        "SELECT * FROM (
                           SELECT *, ko_roman_rank(term, ?1) AS roman_rank FROM term_search
                           WHERE {lang_filter}({})
                         ) WHERE roman_rank > 0
                         ORDER BY roman_rank DESC, score DESC LIMIT {} OFFSET {}",
                        or.join(" OR "),
                        limit + 1,
                        offset
                    );
                    let mut st = conn.prepare_cached(&sql)?;
                    st.query_map(params_from_iter(ps.iter()), map_row)?
                        .collect::<Result<_, _>>()?
                }
            } else if is_fts {
                let langs: Vec<&str> = if lang.is_empty() {
                    vec!["ja", "zh", "ko"]
                } else {
                    vec![lang]
                };
                let mut q = langs
                    .iter()
                    .filter_map(|l| {
                        let v = self.fold(req.tokens.get(*l)?.trim());
                        (!v.is_empty()).then(|| format!("term_{l}:{}", fts_phrase(&v)))
                    })
                    .collect::<Vec<_>>()
                    .join(" OR ");
                if !readings.is_empty() {
                    // Reading expansion can get large. Put term match first.
                    q = if q.is_empty() {
                        reading_match(&readings, false)
                    } else {
                        format!("({q}) AND {}", reading_match(&readings, false))
                    };
                }
                if q.is_empty() {
                    vec![]
                } else {
                    let mut st = conn.prepare_cached(
                        "SELECT s.* FROM term_search_fts f JOIN term_search s ON s.id = f.rowid
                         WHERE term_search_fts MATCH ?1 ORDER BY f.rank LIMIT ?2 OFFSET ?3",
                    )?;
                    st.query_map(params![q, limit + 1, offset], map_row)?
                        .collect::<Result<_, _>>()?
                }
            } else {
                let mut q_term = self.norm(&req.term);
                if !is_exact && !q_term.is_empty() && !q_term.ends_with(' ') {
                    q_term.push('*');
                }

                let mut wh: Vec<String> = vec![];
                let mut ps: Vec<String> = vec![];
                if !lang.is_empty() {
                    wh.push(format!("term_{lang} != ''"));
                }
                if !q_term.is_empty() {
                    ps.push(q_term);
                    wh.push(format!(
                        "term {} ?{}",
                        if is_exact { "=" } else { "GLOB" },
                        ps.len()
                    ));
                }
                // only alongside a term: the term's index narrows the rows to scan
                let reading_glob = self.norm(req.reading_glob.trim());
                if !ps.is_empty() && reading_glob.chars().any(|c| !"*?".contains(c)) {
                    ps.push(format!(
                        " {reading_glob}{}",
                        if is_exact { " " } else { "*" }
                    ));
                    wh.push(format!("reading GLOB ?{}", ps.len()));
                }
                if !readings.is_empty() {
                    ps.push(reading_match(&readings, !is_exact));
                    wh.push(format!(
                        "id IN (SELECT rowid FROM term_search_fts WHERE term_search_fts MATCH ?{})",
                        ps.len()
                    ));
                }
                if ps.is_empty() {
                    vec![]
                } else {
                    let sql = format!(
                        "SELECT * FROM term_search WHERE {} ORDER BY score DESC LIMIT {} OFFSET {}",
                        wh.join(" AND "),
                        limit + 1,
                        offset
                    );
                    let mut st = conn.prepare_cached(&sql)?;
                    st.query_map(params_from_iter(ps.iter()), map_row)?
                        .collect::<Result<_, _>>()?
                }
            }
        };

        let next = if items.len() > limit as usize {
            items.pop();
            Some(offset + limit)
        } else {
            None
        };

        self.expand(&mut items)?;
        Ok(SearchResult { items, next })
    }

    fn expand(&self, items: &mut [SearchItem]) -> Result<(), YomitanError> {
        if items.is_empty() {
            return Ok(());
        }
        let links: Vec<(i64, i64, i64, i64)> = {
            let conn = self.conn.lock()?;
            let sql = format!(
                "SELECT search_id, dict_id, bank, idx FROM term_link WHERE search_id IN ({}) ORDER BY dict_id, bank, idx",
                items.iter().map(|_| "?").collect::<Vec<_>>().join(",")
            );
            let mut st = conn.prepare(&sql)?;
            st.query_map(params_from_iter(items.iter().map(|i| i.id)), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<Result<_, _>>()?
        };

        for (search_id, dict_id, bank, idx) in links {
            let Some(name) = self.names.get(&dict_id) else {
                continue;
            };
            let rows = match self.bank(dict_id, name, false, bank) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{name} term_bank_{bank}: {e}");
                    continue;
                }
            };
            let Some(row) = rows.get(idx as usize) else {
                continue;
            };
            let row: Vec<Value> = match serde_json::from_str(row.get()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{name} term_bank_{bank}[{idx}]: {e}");
                    continue;
                }
            };
            let s = |i: usize| row.get(i).and_then(Value::as_str).unwrap_or("").to_string();
            let entry = TermEntryOut {
                dict_id,
                dict_path: name.clone(),
                term_id: (bank << 32) | idx,
                term: s(0),
                reading: s(1),
                def_tags: s(2),
                rules: s(3),
                score: row.get(4).and_then(Value::as_f64),
                glossary: row
                    .get(5)
                    .map(|g| g.to_string())
                    .unwrap_or_else(|| "[]".into()),
                sequence: row.get(6).and_then(Value::as_i64),
                tags: s(7),
            };
            if let Some(item) = items.iter_mut().find(|i| i.id == search_id) {
                item.entries.push(entry);
            }
        }
        Ok(())
    }

    fn zip_path(&self, name: &str) -> PathBuf {
        self.dicts_dir.join(format!("{name}.zip"))
    }

    fn with_zip<T>(
        &self,
        name: &str,
        f: impl FnOnce(&mut YomitanZip) -> Result<T, YomitanError>,
    ) -> Result<T, YomitanError> {
        let mut zips = self.zips.lock()?;
        if !zips.contains_key(name) {
            zips.insert(name.to_string(), YomitanZip::open(self.zip_path(name))?);
        }
        f(zips.get_mut(name).unwrap())
    }

    fn bank(&self, dict_id: i64, name: &str, kanji: bool, bank: i64) -> Result<Bank, YomitanError> {
        let key = (dict_id, kanji, bank);
        {
            let mut cache = self.banks.lock()?;
            if let Some(pos) = cache.iter().position(|(k, ..)| *k == key) {
                let hit = cache.remove(pos).unwrap();
                let rows = hit.1.clone();
                cache.push_front(hit);
                return Ok(rows);
            }
        }
        let kind = if kanji { "kanji" } else { "term" };
        let text = self.with_zip(name, |z| {
            z.read_to_string(&format!("{kind}_bank_{bank}.json"))
        })?;
        let size = text.len();
        let rows: Bank = Arc::new(serde_json::from_str(&text)?);
        drop(text);

        let mut cache = self.banks.lock()?;
        cache.push_front((key, rows.clone(), size));
        let mut total = 0;
        // always keep the newest, even if it alone is over budget
        let keep = cache
            .iter()
            .take_while(|(.., s)| {
                total += s;
                total <= BANK_CACHE_BYTES
            })
            .count()
            .max(1);
        cache.truncate(keep);
        Ok(rows)
    }

    /// The Han characters of `text` (up to [`MAX_HAN_CHARS`]): how each breaks down, and
    /// its entries in the kanji dictionaries.
    pub fn han_chars(&self, text: &str) -> Result<Vec<HanChar>, YomitanError> {
        let mut out = vec![];
        for ch in ids::han_chars(text).into_iter().take(MAX_HAN_CHARS) {
            let mut c = HanChar {
                ch: ch.clone(),
                ..Default::default()
            };
            let mut links: Vec<(i64, i64, i64)> = vec![];
            {
                let conn = self.conn.lock()?;
                if self.has_han {
                    let row: Option<(String, String, i64)> = conn
                        .prepare_cached("SELECT ids, hangul, uses FROM han_char WHERE ch = ?1")?
                        .query_row([&ch], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                        .optional()?;
                    if let Some((ids_cols, hangul, uses)) = row {
                        c.ids = ids::parse_columns(&ids_cols);
                        c.hangul = hangul.split_whitespace().map(String::from).collect();
                        c.uses = uses;
                    }
                    for i in &c.ids {
                        for comp in ids::components(&i.ids, &ch) {
                            if !c.components.contains(&comp) {
                                c.components.push(comp);
                            }
                        }
                    }
                    // breadth first, through each component's own IDS
                    let mut st = conn.prepare_cached("SELECT ids FROM han_char WHERE ch = ?1")?;
                    let mut queue: VecDeque<String> = c.components.iter().cloned().collect();
                    while let Some(comp) = queue.pop_front() {
                        if comp == ch || c.all_components.contains(&comp) {
                            continue;
                        }
                        let cols: Option<String> =
                            st.query_row([&comp], |r| r.get(0)).optional()?;
                        for i in ids::parse_columns(cols.as_deref().unwrap_or("")) {
                            queue.extend(ids::components(&i.ids, &comp));
                        }
                        c.all_components.push(comp);
                    }
                }
                if self.has_kanji {
                    links = conn
                        .prepare_cached(
                            "SELECT dict_id, bank, idx FROM kanji_link WHERE kanji = ?1
                             ORDER BY dict_id, bank, idx",
                        )?
                        .query_map([&ch], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                        .collect::<Result<_, _>>()?;
                }
            }
            for (dict_id, bank, idx) in links {
                match self.kanji_entry(dict_id, bank, idx) {
                    Ok(Some(e)) => c.entries.push(e),
                    Ok(None) => {}
                    Err(e) => eprintln!("kanji_bank_{bank}[{idx}] of dict {dict_id}: {e}"),
                }
            }
            out.push(c);
        }
        Ok(out)
    }

    fn kanji_entry(
        &self,
        dict_id: i64,
        bank: i64,
        idx: i64,
    ) -> Result<Option<KanjiEntryOut>, YomitanError> {
        let Some(name) = self.names.get(&dict_id) else {
            return Ok(None);
        };
        let rows = self.bank(dict_id, name, true, bank)?;
        let Some(row) = rows.get(idx as usize) else {
            return Ok(None);
        };
        let row: Vec<Value> = serde_json::from_str(row.get())?;
        let s = |i: usize| row.get(i).and_then(Value::as_str).unwrap_or("").to_string();
        let meanings = match row.get(4) {
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    v => v.to_string(),
                })
                .collect(),
            _ => vec![],
        };
        let stats: Vec<(String, String)> = match row.get(5) {
            Some(Value::Object(o)) => o
                .iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(s) => s.clone(),
                        v => v.to_string(),
                    };
                    (k.clone(), v)
                })
                .collect(),
            _ => vec![],
        };
        let tags = s(3);
        // what the dictionary says about the tags and stats it uses
        let all = self.tag_meta(dict_id, name)?;
        let tag_meta: HashMap<String, TagMeta> = tags
            .split_whitespace()
            .chain(stats.iter().map(|(k, _)| k.as_str()))
            .filter_map(|t| Some((t.to_string(), all.get(t)?.clone())))
            .collect();
        Ok(Some(KanjiEntryOut {
            dict_id,
            dict_path: name.clone(),
            lang: self.langs.get(&dict_id).cloned(),
            character: s(0),
            onyomi: s(1),
            kunyomi: s(2),
            tags,
            meanings,
            stats,
            tag_meta,
        }))
    }

    /// A dictionary's tag banks, read once
    fn tag_meta(
        &self,
        dict_id: i64,
        name: &str,
    ) -> Result<Arc<HashMap<String, TagMeta>>, YomitanError> {
        if let Some(m) = self.tag_meta.lock()?.get(&dict_id) {
            return Ok(m.clone());
        }
        let mut out = HashMap::new();
        self.with_zip(name, |z| {
            for b in z.banks().into_iter().filter(|b| b.kind == BankKind::Tag) {
                // a malformed tag bank only costs the tags' notes
                let Ok(rows) = z.read_json_array(&b.name) else {
                    continue;
                };
                for r in rows {
                    let Some(tag) = r.get(0).and_then(Value::as_str) else {
                        continue;
                    };
                    out.insert(
                        tag.to_string(),
                        TagMeta {
                            category: r.get(1).and_then(Value::as_str).unwrap_or("").into(),
                            order: r.get(2).and_then(Value::as_f64).unwrap_or(0.0),
                            notes: r.get(3).and_then(Value::as_str).unwrap_or("").into(),
                        },
                    );
                }
            }
            Ok(())
        })?;
        let out = Arc::new(out);
        self.tag_meta.lock()?.insert(dict_id, out.clone());
        Ok(out)
    }

    /// Characters made of all of `components` (Han characters, e.g. `木口` for 呆, 杏, ...),
    /// following each component up through the characters built from it. A radical also
    /// matches its forms as a component (水: 氵), see [`ids::component_forms`]. The
    /// characters the dictionaries use most come first.
    pub fn han_containing(
        &self,
        components: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<HanList, YomitanError> {
        let limit = limit.unwrap_or(60).clamp(1, MAX_HAN_LIST);
        let offset = offset.unwrap_or(0);
        let comps: Vec<String> = ids::han_chars(components)
            .into_iter()
            .map(|c| ids::normalize_component(&c))
            .collect();
        if !self.has_han || comps.is_empty() {
            return Ok(HanList::default());
        }
        let mut ps: Vec<String> = vec![];
        let mut ctes = vec![];
        let mut sets = vec![];
        for (i, c) in comps.iter().enumerate() {
            let forms = ids::component_forms(c);
            let start = ps.len();
            ps.extend(forms);
            let seeds = (start + 1..=ps.len())
                .map(|n| format!("?{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            ctes.push(format!(
                "up{i}(ch) AS (
                   SELECT ch FROM han_component WHERE component IN ({seeds})
                   UNION SELECT h.ch FROM han_component h JOIN up{i} ON h.component = up{i}.ch
                 )"
            ));
            sets.push(format!("SELECT ch FROM up{i}"));
        }
        let sql = format!(
            "WITH RECURSIVE {} SELECT h.ch, h.uses FROM han_char h
             WHERE h.ch IN ({}) ORDER BY h.uses DESC, h.ch LIMIT {} OFFSET {offset}",
            ctes.join(", "),
            sets.join(" INTERSECT "),
            limit + 1,
        );
        let conn = self.conn.lock()?;
        let mut items: Vec<HanBrief> = conn
            .prepare(&sql)?
            .query_map(params_from_iter(ps.iter()), |r| {
                Ok(HanBrief {
                    ch: r.get(0)?,
                    uses: r.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let next = if items.len() > limit as usize {
            items.pop();
            Some(offset + limit)
        } else {
            None
        };
        Ok(HanList { items, next })
    }

    /// A media file from a dict's zip (structured-content `path`).
    pub fn asset(&self, name: &str, path: &str) -> Result<Option<Vec<u8>>, YomitanError> {
        if !self.names.values().any(|n| n == name) {
            return Ok(None);
        }
        self.with_zip(name, |z| z.read_asset(path))
    }

    /// Dict info rows, e.g. for a settings/about view.
    pub fn dicts(&self) -> Result<Vec<DictInfo>, YomitanError> {
        let conn = self.conn.lock()?;
        let mut st =
            conn.prepare("SELECT name, title, revision, L1, term_count FROM dict ORDER BY id")?;
        let rows = st
            .query_map([], |r| {
                Ok(DictInfo {
                    name: r.get(0)?,
                    title: r.get(1)?,
                    revision: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    lang: r.get(3)?,
                    term_count: r.get::<_, i64>(4)? as usize,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Whether a dict id exists (used by tests).
    pub fn has_dict(&self, name: &str) -> Result<bool, YomitanError> {
        let conn = self.conn.lock()?;
        Ok(conn
            .query_row("SELECT 1 FROM dict WHERE name = ?1", [name], |_| Ok(()))
            .optional()?
            .is_some())
    }
}

#[cfg(all(test, feature = "hangul-readings"))]
mod hangul_reading_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn terms(db: &SearchDb, req: SearchRequest) -> Vec<String> {
        db.search(&SearchRequest {
            lang: "ko-KR".into(),
            ..req
        })
        .unwrap()
        .items
        .into_iter()
        .map(|i| i.term)
        .collect()
    }

    /// A build gives Korean Hanja headwords Hangul readings, which search then finds.
    #[tokio::test]
    async fn hanja_headwords_get_hangul_readings() {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cjk-dict-hanja-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("search.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(FTS_TRIGGERS).unwrap();
        conn.execute("INSERT INTO dict (name, title) VALUES ('ko', 'Korean')", [])
            .unwrap();
        // (term, reading, score); Korean unless noted
        for (term, reading, score) in [
            ("光速", "", 10),
            ("광속", "", 20),
            ("光나다", "", 0),
            // 樂 is read 악, 낙, 요, ...: 요 is the one that is a headword here
            ("樂", "", 0),
            ("요", "", 0),
            // the same word from a dictionary that was indexed again: merged into the row
            // that already has the reading
            ("克己", " 극기 ", 5),
            ("克己", "", 7),
        ] {
            conn.execute(
                "INSERT INTO term_search (term, term_ko, reading, score, tags) VALUES (?1, ?1, ?2, ?3, ?4)",
                params![term, reading, score, format!("t{score}")],
            )
            .unwrap();
        }
        // Japanese: no Hangul reading
        conn.execute(
            "INSERT INTO term_search (term, term_ja, reading) VALUES ('光速', '光速', ' こうそく ')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO term_search (term, term_ja, term_zh, term_ko) VALUES ('光', '光', '光', '光')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO term_link (search_id, dict_id, bank, idx) SELECT id, 1, 1, id FROM term_search",
            [],
        )
        .unwrap();
        drop(conn);

        let tok = TokenizerMapper::new(dir.clone());
        build_incremental(&db_path, &[], &[], &tok, |_, _, _| {})
            .await
            .unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let readings: Vec<(String, String, Option<i64>, Option<String>, i64)> = conn
            .prepare(
                "SELECT term, reading, score, tags, (SELECT count(*) FROM term_link WHERE search_id = id)
                 FROM term_search ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let readings: Vec<_> = readings
            .iter()
            .map(|(t, r, s, tags, links)| {
                (
                    t.as_str(),
                    r.as_str(),
                    *s,
                    tags.as_deref().unwrap_or(""),
                    *links,
                )
            })
            .collect();
        assert_eq!(
            readings,
            [
                ("光速", " 광속 ", Some(10), "t10", 1),
                ("광속", "", Some(20), "t20", 1),
                ("光나다", " 광나다 ", Some(0), "t0", 1),
                ("樂", " 요 ", Some(0), "t0", 1),
                ("요", "", Some(0), "t0", 1),
                ("克己", " 극기 ", Some(7), "t5 t7", 2),
                ("光速", " こうそく ", None, "", 1),
                ("光", "", None, "", 1),
            ]
        );
        drop(conn);

        let db = SearchDb::open(&db_path, &dir).unwrap();
        let req = |term: &str, reading: &str, kind: Option<&str>| SearchRequest {
            term: term.into(),
            reading: reading.into(),
            kind: kind.map(String::from),
            ..Default::default()
        };
        // {광속}: by reading; a Hangul word is its own reading
        assert_eq!(terms(&db, req("", "광속", Some("exact"))), ["광속", "光速"]);
        assert_eq!(terms(&db, req("", "광", None)), ["광속", "光速", "光나다"]);
        // 광속: Hanja headwords read so too
        assert_eq!(terms(&db, req("광속", "", Some("exact"))), ["광속", "光速"]);
        // <光速>{광속}
        assert_eq!(terms(&db, req("光速", "광속", Some("exact"))), ["光速"]);
        assert_eq!(
            terms(&db, req("光나다", "광나다", Some("exact"))),
            ["光나다"]
        );
        // by romanization
        assert_eq!(
            terms(
                &db,
                SearchRequest {
                    romanized: true,
                    ..req("gwangsok", "", Some("exact"))
                }
            ),
            ["광속", "光速"]
        );
        // Japanese is left alone
        let ja = db
            .search(&SearchRequest {
                lang: "ja-JP".into(),
                ..req("", "こうそく", Some("exact"))
            })
            .unwrap();
        assert_eq!(ja.items.len(), 1);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod han_tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn temp_dir(name: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cjk-dict-{name}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// a KANJIDIC-like dictionary: kanji banks and a tag bank, no terms
    fn write_kanji_zip(path: &Path) {
        let mut w = ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = SimpleFileOptions::default();
        for (name, json) in [
            (
                "index.json",
                r#"{"title":"Kanji","revision":"1","format":3,"sourceLanguage":"ja"}"#,
            ),
            (
                "kanji_bank_1.json",
                r#"[["林","リン","はやし","jouyou",["grove","forest"],{"strokes":"8","freq":"1000"}],
                    ["林檎","","","",["apple"],{}],
                    ["森","シン","もり","jouyou",["forest"],{"strokes":"12"}]]"#,
            ),
            (
                "tag_bank_1.json",
                r#"[["jouyou","frequent",0,"included in list of regular-use characters",0],
                    ["strokes","misc",0,"Stroke count",0]]"#,
            ),
        ] {
            w.start_file(name, opts).unwrap();
            w.write_all(json.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    const IDS: &str = "# test\n\
        U+6728\t木\t木\n\
        U+6797\t林\t⿰木木\n\
        U+68EE\t森\t⿱木林\n\
        U+5446\t呆\t⿱口木\n\
        U+674F\t杏\t⿱木口\n\
        U+674E\t李\t⿱木子\n\
        U+53EF\t可\t⿹丁口[GTKV]\t⿹⿱一口亅[J]\n\
        U+6CB3\t河\t⿰氵可\n";

    fn chars(list: HanList) -> Vec<String> {
        list.items.into_iter().map(|i| i.ch).collect()
    }

    #[tokio::test]
    async fn characters_break_down_and_are_found_by_component() {
        let dir = temp_dir("han");
        let db_path = dir.join("search.db");
        write_kanji_zip(&dir.join("kanji.zip"));
        let tok = TokenizerMapper::new(dir.clone());
        build(
            &db_path,
            &[DictSource {
                name: "kanji".into(),
                zip: dir.join("kanji.zip"),
                lang: None,
            }],
            &tok,
            |_, _, _| {},
        )
        .await
        .unwrap();
        {
            // terms, for how often characters are used: 林 twice, 森 once
            let conn = Connection::open(&db_path).unwrap();
            for t in ["林", "森林", "河"] {
                conn.execute(
                    "INSERT INTO term_search (term, term_ja) VALUES (?1, ?1)",
                    [t],
                )
                .unwrap();
            }
        }
        let ids_txt = dir.join("ids.txt");
        std::fs::write(&ids_txt, IDS).unwrap();
        assert!(build_chars(&db_path, &ids_txt, false).unwrap());
        // unchanged: search.db is left alone
        let before = std::fs::read(&db_path).unwrap();
        assert!(!build_chars(&db_path, &ids_txt, false).unwrap());
        assert_eq!(std::fs::read(&db_path).unwrap(), before);

        let db = SearchDb::open(&db_path, &dir).unwrap();
        let found = db.han_chars("森の林檎").unwrap();
        assert_eq!(
            found.iter().map(|c| c.ch.as_str()).collect::<Vec<_>>(),
            ["森", "林", "檎"]
        );
        let mori = &found[0];
        assert_eq!(mori.ids[0].ids, "⿱木林");
        assert_eq!(mori.components, ["木", "林"]);
        let mut all = mori.all_components.clone();
        all.sort();
        assert_eq!(all, ["木", "林"]);
        assert_eq!(mori.uses, 1);
        assert_eq!(mori.entries.len(), 1);
        assert_eq!(mori.entries[0].kunyomi, "もり");
        assert_eq!(mori.entries[0].lang.as_deref(), Some("ja-JP"));
        let hayashi = &found[1];
        assert_eq!(hayashi.uses, 2);
        assert_eq!(hayashi.entries[0].meanings, ["grove", "forest"]);
        assert_eq!(
            hayashi.entries[0].stats,
            [
                ("freq".to_string(), "1000".to_string()),
                ("strokes".to_string(), "8".to_string())
            ]
        );
        assert_eq!(hayashi.entries[0].tag_meta["strokes"].notes, "Stroke count");
        assert_eq!(hayashi.entries[0].tag_meta["jouyou"].category, "frequent");
        // neither in ids.txt nor in a kanji dictionary
        assert!(found[2].ids.is_empty() && found[2].entries.is_empty());

        let regions = &db.han_chars("可").unwrap()[0].ids;
        assert_eq!(regions[1].regions, "J");

        // the most used first, then by code point
        assert_eq!(
            chars(db.han_containing("木", None, None).unwrap()),
            ["林", "森", "呆", "李", "杏"]
        );
        assert_eq!(
            chars(db.han_containing("木口", None, None).unwrap()),
            ["呆", "杏"]
        );
        // through 可; ⼝ is the Kangxi radical
        assert_eq!(
            chars(db.han_containing("⼝", None, None).unwrap()),
            ["河", "可", "呆", "杏"]
        );
        // 河 is written with 氵
        assert_eq!(chars(db.han_containing("水", None, None).unwrap()), ["河"]);
        let page = db.han_containing("木", Some(2), Some(2)).unwrap();
        assert_eq!(chars(page), ["呆", "李"]);
        assert_eq!(
            db.han_containing("木", Some(2), Some(2)).unwrap().next,
            Some(4)
        );
        assert!(
            db.han_containing("か", None, None)
                .unwrap()
                .items
                .is_empty()
        );

        #[cfg(feature = "hangul-readings")]
        assert!(
            db.han_chars("李").unwrap()[0]
                .hangul
                .contains(&"리".to_string())
        );

        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// `*<林>{リン}`: the words with 林 read so anywhere, by `term_char`, in each language
    #[tokio::test]
    async fn characters_readings_are_indexed() {
        let dir = temp_dir("han-reading");
        let db_path = dir.join("search.db");
        write_kanji_zip(&dir.join("kanji.zip"));
        let tok = TokenizerMapper::new(dir.clone());
        let kanji = DictSource {
            name: "kanji".into(),
            zip: dir.join("kanji.zip"),
            lang: None,
        };
        build(&db_path, &[kanji], &tok, |_, _, _| {}).await.unwrap();
        {
            let conn = Connection::open(&db_path).unwrap();
            for (term, lang, reading, score) in [
                ("森林", "ja", " しんりん ", 5),
                ("林", "ja", " はやし ", 3),
                // 林 not read リン: not a match, though りん is in the reading
                ("森", "ja", " もりりん ", 9),
                ("森林", "zh", " SEN1 LIN2 ", 0),
                ("森林", "ko", " 삼림 ", 0),
                // the initial-sound rule: 林 is 임 at the start
                ("林業", "ko", " 임업 ", 0),
            ] {
                conn.execute(
                    &format!(
                        "INSERT INTO term_search (term, term_{lang}, reading, score) VALUES (?1, ?1, ?2, ?3)"
                    ),
                    params![term, reading, score],
                )
                .unwrap();
            }
        }
        let ids_txt = dir.join("ids.txt");
        std::fs::write(&ids_txt, IDS).unwrap();

        // before term_char: a search.db is searched by patterns
        let db = SearchDb::open(&db_path, &dir).unwrap();
        let find = |db: &SearchDb, lang: &str, ch: &str, r: &str| -> Vec<(String, String)> {
            db.search(&SearchRequest {
                lang: lang.into(),
                ch: ch.into(),
                ch_reading: r.into(),
                ..Default::default()
            })
            .unwrap()
            .items
            .into_iter()
            .map(|i| (i.term, i.reading.trim().to_string()))
            .collect()
        };
        let pairs = |v: &[(&str, &str)]| -> Vec<(String, String)> {
            v.iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect()
        };
        assert_eq!(
            find(&db, "ja-JP", "林", "リン"),
            pairs(&[("森林", "しんりん")])
        );
        drop(db);

        assert!(build_chars(&db_path, &ids_txt, false).unwrap());
        // nothing changed: left alone
        let before = std::fs::read(&db_path).unwrap();
        assert!(!build_chars(&db_path, &ids_txt, false).unwrap());
        assert_eq!(std::fs::read(&db_path).unwrap(), before);

        assert!(!missing_kanji_readings(&db_path).unwrap());
        // 林 and 森; the multi-character 林檎 is left out
        assert_eq!(char_counts(&db_path).unwrap()["kanji"], 2);
        let db = SearchDb::open(&db_path, &dir).unwrap();
        assert!(db.has_term_char);
        assert_eq!(
            find(&db, "ja-JP", "林", "リン"),
            pairs(&[("森林", "しんりん")])
        );
        assert_eq!(
            find(&db, "ja-JP", "林", "はやし"),
            pairs(&[("林", "はやし")])
        );
        assert_eq!(
            find(&db, "ja-JP", "森", "シン"),
            pairs(&[("森林", "しんりん")])
        );
        assert_eq!(
            find(&db, "zh-CN", "林", "lín"),
            pairs(&[("森林", "SEN1 LIN2")])
        );
        // toneless: every tone
        assert_eq!(find(&db, "zh-CN", "林", "lin").len(), 1);
        let mut ko = find(&db, "ko-KR", "林", "림");
        ko.sort();
        assert_eq!(ko, pairs(&[("林業", "임업"), ("森林", "삼림")]));
        // all languages
        assert_eq!(find(&db, "", "林", "リン").len(), 1);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A search.db from before kanji_link gets the kanji entries of the dictionaries it has
    /// on its next update; one without the character tables still searches.
    #[tokio::test]
    async fn an_older_search_db_gets_kanji_entries() {
        let dir = temp_dir("han-backfill");
        let db_path = dir.join("search.db");
        write_kanji_zip(&dir.join("kanji.zip"));
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute(
                "INSERT INTO dict (name, title, L1) VALUES ('kanji', 'Kanji', 'ja-JP')",
                [],
            )
            .unwrap();
        }
        let db = SearchDb::open(&db_path, &dir).unwrap();
        assert!(db.han_chars("林").unwrap()[0].entries.is_empty());
        assert!(
            db.han_containing("木", None, None)
                .unwrap()
                .items
                .is_empty()
        );
        drop(db);

        let tok = TokenizerMapper::new(dir.clone());
        build_incremental(&db_path, &[], &[], &tok, |_, _, _| {})
            .await
            .unwrap();
        let db = SearchDb::open(&db_path, &dir).unwrap();
        let found = db.han_chars("林森").unwrap();
        assert_eq!(found[0].entries[0].onyomi, "リン");
        assert_eq!(found[1].entries[0].onyomi, "シン");
        drop(db);

        // removing the dictionary removes its entries
        build_incremental(&db_path, &[], &["kanji".to_string()], &tok, |_, _, _| {})
            .await
            .unwrap();
        let conn = Connection::open(&db_path).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM kanji_link", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        drop(conn);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod kana_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cjk-dict-kana-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// terms as a build since kana folding stores them: コーヒー as こーひー
    fn fixture(folded: bool) -> (PathBuf, PathBuf) {
        let dir = temp_dir();
        let db_path = dir.join("search.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(FTS_TRIGGERS).unwrap();
        if folded {
            conn.execute_batch(META_SCHEMA).unwrap();
            conn.execute("INSERT INTO meta (key, value) VALUES ('kana', '1')", [])
                .unwrap();
        }
        let f = |s: &str| {
            if folded {
                normalize_term(s)
            } else {
                normalize_text(s)
            }
        };
        for (term, reading) in [
            ("コーヒー", ""),
            ("興味", "きょうみ"),
            ("興行", "こうぎょう"),
            ("復興", "ふっこう"),
            ("ドア", ""),
        ] {
            let reading = if reading.is_empty() {
                String::new()
            } else {
                format!(" {} ", f(reading))
            };
            conn.execute(
                "INSERT INTO term_search (term, term_ja, reading) VALUES (?1, ?2, ?3)",
                params![f(term), fold_kana(term), reading],
            )
            .unwrap();
        }
        (dir, db_path)
    }

    fn terms(db: &SearchDb, req: SearchRequest) -> Vec<String> {
        let mut out: Vec<String> = db
            .search(&SearchRequest {
                lang: "ja-JP".into(),
                ..req
            })
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.term)
            .collect();
        out.sort();
        out
    }

    #[test]
    fn katakana_and_hiragana_find_each_other() {
        let (dir, db_path) = fixture(true);
        let db = SearchDb::open(&db_path, &dir).unwrap();
        let term = |t: &str| SearchRequest {
            term: t.into(),
            ..Default::default()
        };
        assert_eq!(terms(&db, term("こーひー")), ["こーひー"]);
        assert_eq!(terms(&db, term("コーヒー")), ["こーひー"]);
        assert_eq!(terms(&db, term("ｺｰ")), ["こーひー"]);
        // <興>{キョウ}
        assert_eq!(
            terms(
                &db,
                SearchRequest {
                    term: "興".into(),
                    reading_glob: "キョウ".into(),
                    ..Default::default()
                }
            ),
            ["興味"]
        );
        // *<興>{コウ}: anywhere in the word
        assert_eq!(
            terms(
                &db,
                SearchRequest {
                    term: "*興".into(),
                    reading_glob: "*コウ".into(),
                    ..Default::default()
                }
            ),
            ["復興", "興行"]
        );
        // {キョウミ}
        assert_eq!(
            terms(
                &db,
                SearchRequest {
                    reading: "キョウミ".into(),
                    kind: Some("exact".into()),
                    ..Default::default()
                }
            ),
            ["興味"]
        );
        // ~ tokens
        let mut tokens = HashMap::new();
        tokens.insert("ja".to_string(), "どあ".to_string());
        assert_eq!(
            terms(
                &db,
                SearchRequest {
                    kind: Some("fts".into()),
                    term: "どあ".into(),
                    tokens,
                    ..Default::default()
                }
            ),
            ["どあ"]
        );
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A search.db from before kana folding is searched as before, and must be rebuilt
    /// before dictionaries are added to it.
    #[tokio::test]
    async fn an_older_search_db_is_searched_as_before_and_rebuilt() {
        let (dir, db_path) = fixture(false);
        assert!(needs_rebuild(&db_path).unwrap());
        let db = SearchDb::open(&db_path, &dir).unwrap();
        let term = |t: &str| SearchRequest {
            term: t.into(),
            ..Default::default()
        };
        assert_eq!(terms(&db, term("コーヒー")), ["コーヒー"]);
        assert!(terms(&db, term("こーひー")).is_empty());
        drop(db);

        let tok = TokenizerMapper::new(dir.clone());
        let source = DictSource {
            name: "x".into(),
            zip: dir.join("x.zip"),
            lang: None,
        };
        let err = build_incremental(&db_path, &[source], &[], &tok, |_, _, _| {})
            .await
            .unwrap_err();
        assert!(err.to_string().contains("rebuild"), "{err}");
        // removing dictionaries still works
        build_incremental(&db_path, &[], &[], &tok, |_, _, _| {})
            .await
            .unwrap();
        assert!(needs_rebuild(&db_path).unwrap());

        // a fresh build is folded
        let fresh = dir.join("fresh.db");
        build(&fresh, &[], &tok, |_, _, _| {}).await.unwrap();
        assert!(!needs_rebuild(&fresh).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod kanji_reading_warning_tests {
    use super::*;

    /// Japanese terms with readings and no kanji dictionary: build_chars can't index their
    /// kanji's readings, which dict-admin warns about
    #[tokio::test]
    async fn missing_kanji_readings_are_told() {
        let dir = std::env::temp_dir().join(format!(
            "cjk-dict-kana-kanji-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("search.db");
        let tok = TokenizerMapper::new(dir.clone());
        build(&db_path, &[], &tok, |_, _, _| {}).await.unwrap();
        let ids_txt = dir.join("ids.txt");
        std::fs::write(&ids_txt, "U+6797\t林\t⿰木木\n").unwrap();

        // no Japanese terms: nothing to warn about
        build_chars(&db_path, &ids_txt, true).unwrap();
        assert!(!missing_kanji_readings(&db_path).unwrap());

        Connection::open(&db_path)
            .unwrap()
            .execute(
                "INSERT INTO term_search (term, term_ja, reading) VALUES ('林', '林', ' はやし ')",
                [],
            )
            .unwrap();
        build_chars(&db_path, &ids_txt, true).unwrap();
        assert!(missing_kanji_readings(&db_path).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
