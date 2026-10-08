use std::{borrow::Cow, collections::HashMap, path::PathBuf};

use futures::TryStreamExt;
use serde::Serialize;
use sqlx::{Pool, Row, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use tokio::fs::remove_dir_all;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::{
    error::YomitanError,
    parser::{YomitanIndex, YomitanReader},
    tokenize::{Lang, TokenizerMapper},
};

pub struct YomitanSearch {
    pub db: Pool<Sqlite>,
    pub dicts: HashMap<String, YomitanReader>,
    pub root_dir: PathBuf,
}

#[derive(Debug, Serialize, Clone)]
pub struct YomitanSearchInitProgress {
    pub dict: String,
    pub current: i64,
    pub total: i64,
}

impl YomitanSearch {
    pub async fn new(db_path: PathBuf, dict_paths: Vec<&str>) -> Result<Self, YomitanError> {
        let options = SqliteConnectOptions::new()
            .filename(db_path.clone())
            .create_if_missing(true)
            .foreign_keys(false);

        let root_dir = db_path
            .parent()
            .ok_or(format!("db_path {db_path:?} must have parent dir"))?;

        let db = SqlitePool::connect_with(options).await?;
        sqlx::migrate!("migrations/search").run(&db).await?;

        let mut dicts = HashMap::new();

        for d in dict_paths {
            let reader = YomitanReader::open(root_dir.join(d));
            dicts.insert(d.to_string(), reader);
        }

        Ok(Self {
            db,
            dicts,
            root_dir: root_dir.to_path_buf(),
        })
    }

    pub async fn import(
        &self,
        dict: &str,
        source_language: Option<Lang>,
        tokenizer_mapper: &TokenizerMapper,
        progress_callback: impl Fn(YomitanSearchInitProgress),
    ) -> Result<&Self, YomitanError> {
        let reader = self.dicts[dict].clone();

        let result = sqlx::query("SELECT `path` FROM `index` WHERE `path` = $1")
            .bind(dict)
            .fetch_optional(&self.db)
            .await?;
        if result.is_none() {
            let reader_db = reader.clone().db;

            let total: i64 = match sqlx::query("SELECT count(1) FROM term")
                .fetch_optional(&reader_db)
                .await?
            {
                Some(r) => r.get(0),
                _ => return Ok(self),
            };
            let mut current: i64 = 0;

            let mut tx = self.db.begin().await?;

            let data_str = reader.clone().get_index_json_str().await?;
            let data: YomitanIndex = serde_json::from_str(&data_str)?;

            let r = sqlx::query(
                "INSERT INTO `index` (`path`, `L1`, `L2`, `data`)
                    VALUES ($1, $2, $3, $4)",
            )
            .bind(dict)
            .bind(data.source_language)
            .bind(data.target_language)
            .bind(data_str)
            .execute(&mut *tx)
            .await?;

            let index_rowid = r.last_insert_rowid();

            let join_tokens =
                async |text: &str, lang: Lang| -> Result<Option<String>, YomitanError> {
                    if let Some(s_lang) = source_language {
                        if s_lang != lang {
                            return Ok(None);
                        }
                    }

                    if let Some(g_lang) = guess_lang(text) {
                        if g_lang != lang {
                            return Ok(None);
                        }
                    }

                    Ok(tokenizer_mapper
                        .tokenize(lang, text)
                        .await?
                        .join(" ")
                        .non_empty_trimmed()
                        .map(|s| format!(" {s} ")))
                };

            let mut term_stream = sqlx::query("SELECT *, rowid FROM term").fetch(&reader_db);
            while let Some(row) = term_stream.try_next().await? {
                let term: String = row.get("term");
                let p1_term = normalize_text(&term);
                let p2_term_ja = join_tokens(&term, Lang::Ja).await?;
                let p3_term_zh = join_tokens(&term, Lang::Zh).await?;
                let p4_term_ko = join_tokens(&term, Lang::Ko).await?;

                let reading: String = row.get("reading");
                let p5_reading = normalize_text(&reading)
                    .non_empty_trimmed()
                    .map(|s| format!(" {s} "));

                let p6_def_tags = row
                    .get::<Option<String>, _>("def_tags")
                    .non_empty_trimmed()
                    .map(|s| format!(" {s} "));
                let p7_rules = row
                    .get::<Option<String>, _>("rules")
                    .non_empty_trimmed()
                    .map(|s| format!(" {s} "));
                let p8_score: Option<i64> = row.get("score");
                let p9_sequence: Option<i64> = row.get("sequence");
                let p10_tags = row
                    .get::<Option<String>, _>("tags")
                    .non_empty_trimmed()
                    .map(|s| format!(" {s} "));

                let term_rowid: i64 = row.get("rowid");

                sqlx::query(
                    "INSERT INTO `term` (
                        `term`, `term_ja`, `term_zh`, `term_ko`, `reading`,     -- 1,2,3,4,5
                        `def_tags`, `rules`, `score`, `sequence`, `tags`,       -- 6,7,8,9,10
                        `index_rowid`, `term_rowid`                             -- 11,12
                        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                )
                .bind(p1_term)
                .bind(p2_term_ja)
                .bind(p3_term_zh)
                .bind(p4_term_ko)
                .bind(p5_reading)
                .bind(p6_def_tags)
                .bind(p7_rules)
                .bind(p8_score)
                .bind(p9_sequence)
                .bind(p10_tags)
                .bind(index_rowid)
                .bind(term_rowid)
                .execute(&mut *tx)
                .await?;

                current += 1;
                progress_callback(YomitanSearchInitProgress {
                    dict: dict.to_string(),
                    current,
                    total,
                })
            }

            tx.commit().await?;
        }

        Ok(self)
    }

    /// returns true if a new dict was added, false if replaced otherwise
    pub fn add(&mut self, dict: &str) -> bool {
        self.dicts
            .insert(
                dict.to_string(),
                YomitanReader::open(self.root_dir.join(dict)),
            )
            .is_none()
    }

    pub async fn delete(&self, dict: &str, cb: impl Fn(String)) -> Result<bool, YomitanError> {
        let mut tx = self.db.begin().await?;

        // throws Error and exit if the row doesn't exist.
        let rowid: i64 = {
            let row = sqlx::query("SELECT rowid FROM `index` WHERE `path` = $1")
                .bind(dict)
                .fetch_one(&mut *tx)
                .await?;
            row.get(0)
        };

        let reader = self.dicts[dict].clone();

        if reader.root_dir.exists() && reader.root_dir.is_dir() {
            reader.db.close().await;
            remove_dir_all(reader.root_dir).await?;

            cb(format!("closed and deleted {dict}"))
        } else {
            return Err(YomitanError::from(format!(
                "Yomitan dict dir does not exist: {:?}",
                reader.root_dir
            )));
        }

        const FTS: [(&str, &[&str]); 4] = [
            ("tags", &["def_tags", "rules", "tags"]),
            ("ja", &["term_ja"]),
            ("zh", &["term_zh"]),
            ("ko", &["term_ko"]),
        ];

        // BEFORE deleting from `term`
        for (t, cols) in FTS {
            let cols = cols.join(", ");
            sqlx::query(&format!(
                "INSERT INTO term_{t}_fts(term_{t}_fts, rowid, {cols})
                SELECT 'delete', rowid, {cols} FROM term_{t} WHERE index_rowid = $1"
            ))
            .bind(rowid)
            .execute(&mut *tx)
            .await?;

            cb(format!("cleared term_{t}_fts for {dict}"));
        }

        sqlx::query("DELETE FROM term WHERE index_rowid = $1")
            .bind(rowid)
            .execute(&mut *tx)
            .await?;

        cb(format!("deleted terms for {dict}"));

        sqlx::query("DELETE FROM `index` WHERE rowid = $1")
            .bind(rowid)
            .execute(&mut *tx)
            .await?;

        cb(format!("deleted index for {dict}, awaiting commit"));

        tx.commit().await?;

        Ok(true)
    }
}

fn nfkc(text: &str) -> Cow<'_, str> {
    if is_nfkc_quick(text.chars()) == IsNormalized::Yes {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfkc().collect())
    }
}

pub fn normalize_text(text: &str) -> String {
    nfkc(text).to_uppercase()
}

#[derive(Default)]
struct ScriptCounts {
    kana: usize,
    hangul: usize,
    han: usize,
}

fn count_scripts(text: &str) -> ScriptCounts {
    let mut c = ScriptCounts::default();
    for ch in text.chars() {
        match ch as u32 {
            // Hiragana, Katakana, Katakana Phonetic Extensions, half-width katakana
            0x3041..=0x3096
            | 0x309D..=0x309F
            | 0x30A1..=0x30FA
            | 0x30FD..=0x30FF
            | 0x31F0..=0x31FF
            | 0xFF66..=0xFF9F => c.kana += 1,

            // Hangul syllables, Jamo, Compatibility Jamo, Jamo Extended-A/B
            0xAC00..=0xD7A3
            | 0x1100..=0x11FF
            | 0x3130..=0x318F
            | 0xA960..=0xA97F
            | 0xD7B0..=0xD7FF => c.hangul += 1,

            // CJK ideographs: Unified, Extension A, Extension B+, Compatibility
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2FA1F | 0xF900..=0xFAFF => c.han += 1,

            _ => {}
        }
    }
    c
}

pub fn guess_lang(text: &str) -> Option<Lang> {
    let c = count_scripts(text);
    match (c.kana, c.hangul, c.han) {
        (0, 0, 0) => None, // no CJK at all
        (k, h, _) if k > 0 && h > 0 => {
            // mixed: go with the larger share
            Some(if k >= h { Lang::Ja } else { Lang::Ko })
        }
        (k, 0, _) if k > 0 => Some(Lang::Ja), // any kana => Japanese
        (0, h, _) if h > 0 => Some(Lang::Ko), // Hangul (even with Hanja) => Korean
        (0, 0, _) => None,                    // Han only: Chinese or Japanese, ambiguous
        _ => None,
    }
}

pub trait NonEmpty: Sized {
    fn non_empty_trimmed(self) -> Option<Self>;
    fn non_empty(self) -> Option<Self>;
}

impl NonEmpty for String {
    fn non_empty_trimmed(self) -> Option<Self> {
        let t = self.trim();
        (!t.is_empty()).then(|| t.to_owned()) // also strips surrounding whitespace
    }
    fn non_empty(self) -> Option<String> {
        (!self.is_empty()).then_some(self)
    }
}

pub trait OptionNonEmpty {
    fn non_empty_trimmed(self) -> Self;
    fn non_empty(self) -> Self;
}

impl OptionNonEmpty for Option<String> {
    fn non_empty_trimmed(self) -> Self {
        self.and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else if t.len() == s.len() {
                Some(s)
            }
            // already trimmed: reuse
            else {
                Some(t.to_owned())
            }
        })
    }
    fn non_empty(self) -> Self {
        self.filter(|s| !s.is_empty())
    }
}
