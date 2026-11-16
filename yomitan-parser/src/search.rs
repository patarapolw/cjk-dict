//! `search.db`: a small SQLite index over Yomitan zips.
//!
//! It holds only what search needs (normalized terms, tokens, readings, tags, scores, and
//! where each entry is: dict + `term_bank_N.json` + array index). Definitions and media
//! stay in the zips, which are read on demand.
//!
//! - [`build`] (dict-admin): zips -> `search.db`
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
    error::YomitanError,
    korean::{self, RomanQuery},
    parser::{BankKind, YomitanZip},
    pinyin::{expand_pinyin, normalize_pinyin},
    tokenize::{Lang, TokenizerMapper},
    util::{NonEmpty, OptionNonEmpty, normalize_text},
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
    conn.execute_batch(FTS_TRIGGERS)?;

    if backfill_roman {
        progress("romanization keys", 0, 0);
        let tx = conn.transaction()?;
        {
            let mut st = tx.prepare("SELECT id, term, score FROM term_search")?;
            let mut ins = tx.prepare(ROMAN_UPSERT)?;
            let rows = st.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            })?;
            for row in rows {
                let (id, term, score) = row?;
                for key in korean::keys(&term) {
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

                    let p1 = normalize_text(term);
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
                    let reading = if lang == Some(Lang::Zh) {
                        normalize_pinyin(reading).unwrap_or_else(|| reading.to_string())
                    } else {
                        reading.to_string()
                    };
                    let p5 = normalize_text(&reading)
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
                    for key in korean::keys(&p1) {
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
        tx.commit()?;

        infos.push(DictInfo {
            name: d.name.clone(),
            title: index.title,
            revision: index.revision,
            lang: lang.map(|l| l.to_string()),
            term_count: count,
        });
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

const MAX_LIMIT: u32 = 50;
/// 5^4: up to four toneless syllables are expanded; beyond that, match the raw reading
const MAX_READINGS: usize = 625;
/// Term banks kept in memory, by JSON size. dict-admin cuts banks to ~256 KB
/// ([`crate::parser::rebank`]), but zips from before that have 3-30 MB banks.
const BANK_CACHE_BYTES: usize = 64 << 20;

type BankKey = (i64, i64);
/// A term bank split into rows, each still raw JSON: splitting is far cheaper than parsing
/// the whole bank into `Value`s, and a search only needs a handful of rows from it.
type Bank = Arc<Vec<Box<RawValue>>>;

pub struct SearchDb {
    conn: Mutex<Connection>,
    dicts_dir: PathBuf,
    /// dict_id -> name
    names: HashMap<i64, String>,
    /// search.db has `term_roman`; built before it, romanized searches romanize each row
    has_roman: bool,
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
        let has_roman = has_table(&conn, "term_roman")?;
        Ok(Self {
            has_roman,
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

    pub fn search(&self, req: &SearchRequest) -> Result<SearchResult, YomitanError> {
        let limit = req.limit.unwrap_or(10).clamp(1, MAX_LIMIT);
        let offset = req.offset.unwrap_or(0);
        let lang = req.lang.split('-').next().unwrap_or("");
        let lang = if ["ja", "zh", "ko"].contains(&lang) {
            lang
        } else {
            ""
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
                vec![reading.to_string()]
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

            if let (Some(roman), true) = (&roman, self.has_roman) {
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
            } else if req.kind.as_deref() == Some("fts") {
                let langs: Vec<&str> = if lang.is_empty() {
                    vec!["ja", "zh", "ko"]
                } else {
                    vec![lang]
                };
                let mut q = langs
                    .iter()
                    .filter_map(|l| {
                        let v = req.tokens.get(*l)?.trim();
                        (!v.is_empty()).then(|| format!("term_{l}:{}", fts_phrase(v)))
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
                let mut q_term = normalize_text(&req.term);
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
                let reading_glob = normalize_text(req.reading_glob.trim());
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
            let rows = match self.bank(dict_id, name, bank) {
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

    fn bank(&self, dict_id: i64, name: &str, bank: i64) -> Result<Bank, YomitanError> {
        let key = (dict_id, bank);
        {
            let mut cache = self.banks.lock()?;
            if let Some(pos) = cache.iter().position(|(k, ..)| *k == key) {
                let hit = cache.remove(pos).unwrap();
                let rows = hit.1.clone();
                cache.push_front(hit);
                return Ok(rows);
            }
        }
        let text = self.with_zip(name, |z| {
            z.read_to_string(&format!("term_bank_{bank}.json"))
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
