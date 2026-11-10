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

use rusqlite::{Connection, OpenFlags, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};

use crate::{
    error::YomitanError,
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
    let tmp = out.with_extension("db.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut conn = Connection::open(&tmp)?;
    conn.execute_batch(
        "PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY; PRAGMA cache_size = -262144;",
    )?;
    conn.execute_batch(SCHEMA)?;

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
                RETURNING id"#,
            )?;
            let mut link = tx.prepare(
                "INSERT OR IGNORE INTO term_link (search_id, dict_id, bank, idx) VALUES (?1, ?2, ?3, ?4)",
            )?;

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

                    let search_id: i64 = upsert.query_row(
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
                        |r| r.get(0),
                    )?;
                    link.execute(params![search_id, dict_id, bank.index, idx as i64])?;
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

    progress("full-text index", 0, 0);
    conn.execute_batch(
        "INSERT INTO term_search_fts (term_search_fts) VALUES ('rebuild');
         INSERT INTO term_search_fts (term_search_fts) VALUES ('optimize');
         ANALYZE;",
    )?;
    conn.execute_batch("VACUUM;")?;
    drop(conn);
    std::fs::rename(&tmp, out)?;
    Ok(infos)
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
    /// fts: tokenized `term`, space-separated, per short language ("ja", "zh", "ko")
    #[serde(default)]
    pub tokens: HashMap<String, String>,
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
    zips: Mutex<HashMap<String, YomitanZip>>,
    /// most recently used first; (key, rows, JSON bytes)
    banks: Mutex<VecDeque<(BankKey, Bank, usize)>>,
}

fn fts_phrase(s: &str) -> String {
    format!("\"{}\"", s.replace('"', " "))
}

fn reading_match(readings: &[String]) -> String {
    let q = readings
        .iter()
        .map(|r| fts_phrase(r))
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
        let names = {
            let mut st = conn.prepare("SELECT id, name FROM dict")?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<HashMap<i64, String>, _>>()?
        };
        Ok(Self {
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

            if req.kind.as_deref() == Some("fts") {
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
                        reading_match(&readings)
                    } else {
                        format!("({q}) AND {}", reading_match(&readings))
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
                let is_exact = req.kind.as_deref() == Some("exact");
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
                if !readings.is_empty() {
                    ps.push(reading_match(&readings));
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
        let text = self.with_zip(name, |z| z.read_to_string(&format!("term_bank_{bank}.json")))?;
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
        self.with_zip(name, |z| z.read_bytes(path).map(Some))
            .or_else(|e| match e {
                YomitanError::ZipError(zip::result::ZipError::FileNotFound) => Ok(None),
                e => Err(e),
            })
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
