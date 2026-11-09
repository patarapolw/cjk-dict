use std::{borrow::Cow, collections::HashMap, path::PathBuf, time::Duration};

use futures::TryStreamExt;
use itertools::Itertools;
use serde::Serialize;
use serde_json::json;
use sqlx::{
    Pool, Row, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use tokio::fs::{self, read_dir, read_to_string, remove_dir_all};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkc_quick};

use crate::{
    error::YomitanError,
    parser::{YomitanIndex, YomitanReader},
    pinyin::normalize_pinyin,
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
            .pragma("mmap_size", "268435456") // 256 MiB
            .optimize_on_close(true, None);

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

    pub async fn cleanup(
        &mut self,
        tokenizer_mapper: &TokenizerMapper,
        progress_callback: &impl Fn(YomitanSearchInitProgress),
        delete_callback: &impl Fn(String),
    ) -> Result<(), YomitanError> {
        let mut existing_folders: Vec<String> = vec![];

        let dict_root = &self.root_dir.join("yomitan");
        let mut entries = read_dir(dict_root).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }

            let entry_path = &entry
                .path()
                .strip_prefix(&self.root_dir)?
                .to_string_lossy()
                .replace('\\', "/");

            existing_folders.push(entry_path.to_string());
            self.add(entry_path);
        }

        let dict_term_count = self.dict_term_count(None).await?;

        let mut to_be_deleted: Vec<String> = existing_folders
            .into_iter()
            .filter(|s| dict_term_count.get(&s.to_string()).is_none())
            .collect();

        for (dict, _) in dict_term_count {
            let r = self
                .import(&dict, None, tokenizer_mapper, progress_callback)
                .await?;
            if r.is_none() {
                to_be_deleted.push(dict);
            }
        }

        self.delete_many(
            to_be_deleted.iter().map(String::as_str).collect(),
            delete_callback,
        )
        .await?;

        Ok(())
    }

    async fn dict_term_count(
        &self,
        dicts: Option<Vec<&str>>,
    ) -> Result<HashMap<String, (Option<i64>, Option<i64>)>, YomitanError> {
        let existing_indexes: Vec<(i64, String)> = sqlx::query_as("SELECT id, `path` FROM `index`")
            .fetch_all(&self.db)
            .await?;
        let existing_indexes = if let Some(dicts) = dicts
            && !dicts.is_empty()
        {
            existing_indexes
                .into_iter()
                .filter(|r| dicts.contains(&r.1.as_str()))
                .collect()
        } else {
            existing_indexes
        };

        let mut status: HashMap<String, (Option<i64>, Option<i64>)> = existing_indexes
            .iter()
            .map(|r| (r.1.clone(), (None, None)))
            .collect();

        for (index_id, dict) in existing_indexes {
            let existing_total: Option<i64> =
                sqlx::query_scalar("SELECT count(1) FROM term_link WHERE dict_id = $1")
                    .bind(index_id)
                    .fetch_optional(&self.db)
                    .await?;

            let reader = match self.dicts.get(&dict) {
                Some(d) => d.clone(),
                None => {
                    status.remove(&dict);
                    continue;
                }
            };

            let reader_db = reader.clone().db;

            let stats_path = &reader.root_dir.join("stats.json");
            let total: i64 = if stats_path.exists() {
                let stats_text: String = read_to_string(stats_path).await?;
                let stats_json: serde_json::Value = serde_json::from_str(&stats_text)?;
                stats_json
                    .get("term")
                    .map(|v| v.as_i64().unwrap_or(0))
                    .unwrap_or(0)
            } else {
                sqlx::query_scalar("SELECT count(1) FROM term")
                    .fetch_one(&reader_db)
                    .await?
            };

            status.insert(dict, (Some(total), existing_total));
        }

        Ok(status)
    }

    pub async fn import(
        &self,
        dict: &str,
        source_language: Option<Lang>,
        tokenizer_mapper: &TokenizerMapper,
        progress_callback: impl Fn(YomitanSearchInitProgress),
    ) -> Result<Option<&Self>, YomitanError> {
        let reader = match self.dicts.get(dict) {
            Some(r) => r,
            None => return Ok(None),
        };
        let reader_db = reader.clone().db;

        let (total, existing_total) = self.dict_term_count(Some(vec![dict])).await?[dict];
        let mut total = total.unwrap_or(0);

        if Some(total) != existing_total {
            let existing_index: Option<(i64, String)> =
                sqlx::query_as("SELECT id, L1 FROM `index` WHERE `path` = $1")
                    .bind(dict)
                    .fetch_optional(&self.db)
                    .await?;

            let index_id = existing_index.as_ref().map(|r| r.0);
            let source_language = existing_index
                .map(|r| Lang::from(&r.1))
                .flatten()
                .or(source_language);

            let mut current: i64 = 0;
            let mut tx = self.db.begin().await?;

            let index_id = match index_id {
                Some(index_id) => {
                    sqlx::query("DELETE FROM term_link WHERE dict_id = $1")
                        .bind(index_id)
                        .execute(&mut *tx)
                        .await?;

                    index_id
                }
                None => {
                    let data_str = reader.clone().get_index_json_str().await?;
                    let data: YomitanIndex = serde_json::from_str(&data_str)?;

                    let index_id: i64 = sqlx::query_scalar(
                        "INSERT INTO `index` (`path`, `L1`, `L2`, `data`)
                            VALUES ($1, $2, $3, $4)
                            RETURNING rowid",
                    )
                    .bind(dict)
                    .bind(data.source_language)
                    .bind(data.target_language)
                    .bind(data_str)
                    .fetch_one(&mut *tx)
                    .await?;

                    index_id
                }
            };

            let mut term_stream = sqlx::query("SELECT *, rowid FROM term").fetch(&reader_db);
            while let Some(row) = term_stream.try_next().await? {
                let term: String = row.get("term");
                let p1_term = normalize_text(&term);
                let p2_term_ja = tokenizer_mapper
                    .join_tokens(&term, Lang::Ja, &source_language)
                    .await?
                    .unwrap_or(String::new());
                let p3_term_zh = tokenizer_mapper
                    .join_tokens(&term, Lang::Zh, &source_language)
                    .await?
                    .unwrap_or(String::new());
                let p4_term_ko = tokenizer_mapper
                    .join_tokens(&term, Lang::Ko, &source_language)
                    .await?
                    .unwrap_or(String::new());

                let reading_str: &str = row.get("reading");
                let reading = if source_language == Some(Lang::Zh) {
                    normalize_pinyin(reading_str).unwrap_or_else(|| reading_str.to_string())
                } else {
                    reading_str.to_string()
                };

                let p5_reading = normalize_text(&reading)
                    .non_empty_trimmed()
                    .map(|s| format!(" {s} "))
                    .unwrap_or(String::new());

                let p6_def_tags = row.get::<Option<String>, _>("def_tags").non_empty_trimmed();
                let p7_rules = row.get::<Option<String>, _>("rules").non_empty_trimmed();
                let p8_score: Option<i64> = row.get("score");
                let p9_tags = row.get::<Option<String>, _>("tags").non_empty_trimmed();

                let term_rowid: i64 = row.get("rowid");

                const COLS: [&str; 9] = [
                    "term", "term_ja", "term_zh", "term_ko", "reading", "def_tags", "rules",
                    "score", "tags",
                ];
                fn update_space_sep(c: &str) -> String {
                    format!(
                        r#""{c}" = CASE
                            WHEN "{c}" IS excluded.{c} THEN "{c}"
                            ELSE NULLIF(trim(printf('%s %s', "{c}", excluded.{c})), '')
                        END"#
                    )
                }

                let search_id: i64 = sqlx::query_scalar(&format!(
                    "INSERT INTO `term_search` ({})
                    VALUES ({})
                    ON CONFLICT DO UPDATE SET {}
                    RETURNING rowid;
                ",
                    COLS.map(|c| format!("{c}")).join(","),
                    (1..=COLS.len()).map(|i| format!("${i}")).join(","),
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
                .fetch_one(&mut *tx)
                .await?;

                sqlx::query(
                    "INSERT INTO term_link (dict_id, search_id, term_id, score)
                    VALUES ($1, $2, $3, $4)",
                )
                .bind(index_id)
                .bind(search_id)
                .bind(term_rowid)
                .bind(p8_score)
                .execute(&mut *tx)
                .await?;

                current += 1;
                progress_callback(YomitanSearchInitProgress {
                    dict: dict.to_string(),
                    current,
                    total,
                })
            }

            total = current;

            tx.commit().await?;
        }

        let stats_path = &reader.root_dir.join("stats.json");
        let stats_str = serde_json::to_string(&json!({ "term": total }))?;
        fs::write(stats_path, stats_str).await?;

        Ok(Some(self))
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

    pub async fn delete(
        &self,
        dict: &str,
        delete_callback: impl Fn(String),
    ) -> Result<bool, YomitanError> {
        self.delete_many(vec![dict], delete_callback).await
    }

    pub async fn delete_many(
        &self,
        dicts: Vec<&str>,
        delete_callback: impl Fn(String),
    ) -> Result<bool, YomitanError> {
        let mut tx = self.db.begin().await?;

        for dict in dicts {
            let reader = self.dicts[dict].clone();

            if reader.root_dir.exists() && reader.root_dir.is_dir() {
                reader.db.close().await;
                remove_dir_all(reader.root_dir).await?;

                delete_callback(format!("closed and deleted {dict}"))
            } else {
                return Err(YomitanError::from(format!(
                    "Yomitan dict dir does not exist: {:?}",
                    reader.root_dir
                )));
            }

            let index_id: Option<i64> =
                sqlx::query_scalar("SELECT id FROM `index` WHERE `path` = $1")
                    .bind(dict)
                    .fetch_optional(&mut *tx)
                    .await?;

            let index_id = match index_id {
                Some(i) => i,
                None => continue,
            };

            sqlx::query(
                "
            DELETE FROM term_link
            WHERE dict_id = $1
            OR dict_id NOT IN (SELECT id FROM `index`);
            ",
            )
            .bind(index_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "
            DELETE FROM term_search WHERE id NOT IN (
                SELECT search_id FROM term_link
            );
            ",
            )
            .execute(&mut *tx)
            .await?;

            delete_callback(format!("deleted terms for {dict}"));

            sqlx::query("DELETE FROM `index` WHERE id = $1")
                .bind(index_id)
                .execute(&mut *tx)
                .await?;

            delete_callback(format!("deleted index for {dict}, awaiting commit"));
        }

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
