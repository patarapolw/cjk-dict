use std::{borrow::Cow, collections::HashMap, path::PathBuf, time::Duration};

use futures::TryStreamExt;
use itertools::Itertools;
use serde::Serialize;
use sqlx::{
    Pool, Row, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use tokio::fs::{File, remove_dir_all};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::{
    error::YomitanError,
    parser::{YomitanIndex, YomitanReader},
    tokenize::{Lang, TokenizerMapper},
    util::{NonEmpty, OptionNonEmpty},
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
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(false)
            .pragma("cache_size", "-64000") // approximately 64 MiB
            .pragma("temp_store", "MEMORY")
            .pragma("mmap_size", "268435456"); // 256 MiB

        let root_dir = db_path
            .parent()
            .ok_or(format!("db_path {db_path:?} must have parent dir"))?;

        let db = SqlitePool::connect_with(options).await?;
        sqlx::migrate!("migrations/search").run(&db).await?;
        println!("migrated YomitanSearch");

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

            let mut term_stream = sqlx::query("SELECT *, rowid FROM term").fetch(&reader_db);
            while let Some(row) = term_stream.try_next().await? {
                let term: String = row.get("term");
                let p1_term = normalize_text(&term);
                let p2_term_ja = tokenizer_mapper
                    .join_tokens(&term, Lang::Ja, &source_language)
                    .await?;
                let p3_term_zh = tokenizer_mapper
                    .join_tokens(&term, Lang::Zh, &source_language)
                    .await?;
                let p4_term_ko = tokenizer_mapper
                    .join_tokens(&term, Lang::Ko, &source_language)
                    .await?;

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
                let p9_tags = row
                    .get::<Option<String>, _>("tags")
                    .non_empty_trimmed()
                    .map(|s| format!(" {s} "));

                let term_rowid: i64 = row.get("rowid");

                let cols = [
                    "term", "term_ja", "term_zh", "term_ko", "reading", "def_tags", "rules",
                    "score", "tags",
                ];
                fn update_space_sep(c: &str) -> String {
                    format!(
                        r#"
                    "{c}" = IIF("{c}" = excluded.{c}, "{c}", NULLIF(
                        FORMAT( ' %s %s ', TRIM("{c}"), TRIM(excluded.{c}) ),
                        '   '))"#
                    )
                }

                sqlx::query(&format!(
                    "INSERT INTO `term_search` ({}, ref)
                    VALUES ({}, json_array(json_object(
                        'dict_id', $10,
                        'term_id', $11,
                        'score', $9
                    )))
                    ON CONFLICT DO UPDATE SET {},
                        ref = json_insert(ref, '$[#]', json_extract(excluded.ref, '$[0]'))
                ",
                    cols.map(|c| format!("{c}")).join(","),
                    cols.iter()
                        .enumerate()
                        .map(|(i, _)| format!("${}", i + 1))
                        .join(","),
                    ["def_tags", "rules", "tags"]
                        .map(update_space_sep)
                        .join(",")
                ))
                .bind(p1_term)
                .bind(p2_term_ja)
                .bind(p3_term_zh)
                .bind(p4_term_ko)
                .bind(p5_reading)
                .bind(p6_def_tags)
                .bind(p7_rules)
                .bind(p8_score)
                .bind(p9_tags)
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

        File::create(reader.root_dir.join(".imported")).await?;

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

        sqlx::query(
            "
        CREATE TEMP TABLE ds_link (
            dict_id INTEGER NOT NULL,
            search_id INTEGER NOT NULL,
            PRIMARY KEY (dict_id, search_id)
        );
        ",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "
        INSERT INTO ds_link (dict_id, search_id)
        SELECT json_extract(json_each.value, '$.dict_id'), term_search.rowid
        FROM term_search, json_each(ref)
        WHERE TRUE
        ON CONFLICT DO NOTHING;
        ",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM ds_link WHERE dict_id = 1;")
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "
        DELETE FROM term_search WHERE rowid NOT IN (
            SELECT search_id FROM ds_link
        );
        ",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("DROP TABLE ds_link;").execute(&mut *tx).await?;

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
