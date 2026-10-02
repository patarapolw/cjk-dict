use std::{collections::HashMap, path::PathBuf};

use futures::TryStreamExt;
use serde::Serialize;
use sqlx::{Pool, Row, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};

use crate::{
    error::YomitanError,
    parser::YomitanReader,
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
    pub async fn init(
        db_path: PathBuf,
        dict_paths: Vec<&str>,
        source_language: Lang,
        tokenizer_mapper: TokenizerMapper,
        progress_callback: impl Fn(YomitanSearchInitProgress),
    ) -> Result<Self, YomitanError> {
        let options = SqliteConnectOptions::new()
            .filename(db_path.clone())
            .create_if_missing(true)
            .foreign_keys(false);

        let root_dir = db_path.parent().expect("db_path must have parent dir");

        let db = SqlitePool::connect_with(options.clone()).await?;
        sqlx::migrate!("migrations/search").run(&db).await?;

        let mut dicts = HashMap::new();

        for d in dict_paths {
            let reader = YomitanReader::open(root_dir.join(d));

            let result = sqlx::query("SELECT `path` FROM `index` WHERE `path` = $1")
                .bind(&d)
                .fetch_optional(&db)
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

                let mut tx = db.begin().await?;

                let index_json = reader.clone().get_index_json().await?;

                let idx_clone = index_json.clone();
                let lang_1 = idx_clone
                    .source_language
                    .unwrap_or(source_language.to_string());
                let lang_2 = idx_clone.target_language.unwrap_or("en".to_string());

                let data_str = serde_json::to_string(&index_json)?;

                let r = sqlx::query(
                    "INSERT INTO `index` (`path`, `L1`, `L2`, `data`)
                    VALUES ($1, $2, $3, $4)",
                )
                .bind(&d)
                .bind(&lang_1)
                .bind(&lang_2)
                .bind(data_str)
                .execute(&mut *tx)
                .await?;

                let index_rowid = r.last_insert_rowid();
                let is_ja = lang_1.starts_with("ja");

                let join_tokens_if = async |s: &str| -> Result<String, YomitanError> {
                    Ok(if is_ja {
                        tokenizer_mapper
                            .tokenize(source_language, s)
                            .await?
                            .join(" ")
                    } else {
                        "".to_string()
                    })
                };

                let mut term_stream = sqlx::query("SELECT *, rowid FROM term").fetch(&reader_db);
                while let Some(row) = term_stream.try_next().await? {
                    let p1_term: String = row.get("term");
                    let p2_term = join_tokens_if(&p1_term).await?;

                    let p3_reading: String = row.get("reading");
                    let p4_reading = join_tokens_if(&p3_reading).await?;

                    let p5_def_tags: Option<String> = row.get("def_tags");
                    let p6_rules: Option<String> = row.get("rules");
                    let p7_score: Option<i64> = row.get("score");
                    let p8_sequence: Option<i64> = row.get("sequence");
                    let p9_tags: Option<String> = row.get("tags");

                    let p11_term_rowid: i64 = row.get("rowid");

                    sqlx::query(
                        "INSERT INTO `term` (
                        `term`, `term_ja`,                                  -- 1,2
                        `reading`, `reading_ja`,                            -- 3,4
                        `def_tags`, `rules`, `score`, `sequence`, `tags`,   -- 5,6,7,8,9
                        `index_rowid`, `term_rowid`,                        -- 10,11
                        `L1`, `L2`                                          -- 12,13
                        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                    )
                    .bind(p1_term)
                    .bind(p2_term)
                    .bind(p3_reading)
                    .bind(p4_reading)
                    .bind(p5_def_tags)
                    .bind(p6_rules)
                    .bind(p7_score)
                    .bind(p8_sequence)
                    .bind(p9_tags)
                    .bind(index_rowid)
                    .bind(p11_term_rowid)
                    .bind(&lang_1)
                    .bind(&lang_2)
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

            dicts.insert(d.to_string(), reader);
        }

        db.set_connect_options(options.foreign_keys(true));

        Ok(Self { db, dicts })
    }
}
