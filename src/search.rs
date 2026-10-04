use std::{borrow::Cow, collections::HashMap, path::PathBuf};

use futures::TryStreamExt;
use serde::Serialize;
use sqlx::{Pool, Row, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::{
    error::YomitanError,
    parser::{YomitanIndex, YomitanReader},
    tokenize::{Lang, TokenizerMapper},
};

pub struct YomitanSearch {
    pub db: Pool<Sqlite>,
    pub dicts: HashMap<String, YomitanReader>,
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

        Ok(Self { db, dicts })
    }

    pub async fn import(
        &self,
        dict_paths: Vec<&str>,
        source_language: Option<Lang>,
        tokenizer_mapper: &TokenizerMapper,
        progress_callback: impl Fn(YomitanSearchInitProgress),
    ) -> Result<(), YomitanError> {
        for d in dict_paths {
            let reader = self
                .dicts
                .get(d)
                .ok_or(format!("dict {d} must be defined in ::new"))?;

            let result = sqlx::query("SELECT `path` FROM `index` WHERE `path` = $1")
                .bind(&d)
                .fetch_optional(&self.db)
                .await?;
            if result.is_none() {
                let reader_db = reader.clone().db;

                let total: i64 = match sqlx::query("SELECT count(1) FROM term")
                    .fetch_optional(&reader_db)
                    .await?
                {
                    Some(r) => r.get(0),
                    _ => continue,
                };
                let mut current: i64 = 0;

                let mut tx = self.db.begin().await?;

                let data_str = reader.clone().get_index_json_str().await?;
                let data: YomitanIndex = serde_json::from_str(&data_str)?;

                let r = sqlx::query(
                    "INSERT INTO `index` (`path`, `L1`, `L2`, `data`)
                    VALUES ($1, $2, $3, $4)",
                )
                .bind(&d)
                .bind(data.source_language)
                .bind(data.target_language)
                .bind(data_str)
                .execute(&mut *tx)
                .await?;

                let index_rowid = r.last_insert_rowid();

                let join_tokens = async |text: &str, lang: Lang| -> Result<String, YomitanError> {
                    if let Some(s_lang) = source_language {
                        if s_lang != lang {
                            return Ok(String::new());
                        }
                    }

                    if let Some(g_lang) = guess_lang(text) {
                        if g_lang != lang {
                            return Ok(String::new());
                        }
                    }

                    Ok(tokenizer_mapper.tokenize(lang, text).await?.join(" "))
                };

                let mut term_stream = sqlx::query("SELECT *, rowid FROM term").fetch(&reader_db);
                while let Some(row) = term_stream.try_next().await? {
                    let term: String = row.get("term");
                    let p1_term = normalize_text(&term);
                    let p2_term_ja = join_tokens(&term, Lang::Ja).await?;
                    let p3_term_zh = join_tokens(&term, Lang::Zh).await?;
                    let p4_term_ko = join_tokens(&term, Lang::Ko).await?;

                    let reading: String = row.get("reading");
                    let p5_reading = normalize_text(&reading);

                    let p6_def_tags: Option<String> = row.get("def_tags");
                    let p7_rules: Option<String> = row.get("rules");
                    let p8_score: Option<i64> = row.get("score");
                    let p9_sequence: Option<i64> = row.get("sequence");
                    let p10_tags: Option<String> = row.get("tags");

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
                        dict: d.to_string(),
                        current,
                        total,
                    })
                }

                tx.commit().await?;
            }
        }

        Ok(())
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
