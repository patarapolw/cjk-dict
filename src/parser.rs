use std::{collections::HashMap, path::PathBuf, time::Duration};

use futures::future::join_all;
use serde::{Deserialize, Serialize};
use sqlx::{
    Pool, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use tokio::fs::{create_dir_all, read_dir, read_to_string, remove_file};
use zip::ZipArchive;

use crate::error::YomitanError;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    String(String),
    Number(f64),
}

/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-index-schema.json
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct YomitanIndex {
    pub title: String,
    pub revision: String,

    // anyOf required: format | version
    // dependencies: "isUpdatable": ["indexUrl", "downloadUrl"]
    #[serde(default)]
    pub minimum_yomitan_version: Option<String>,
    #[serde(default)]
    pub sequenced: bool, // [default=false] Whether or not this dictionary contains sequencing information for related terms.
    // serde(default) defaults to false for bool
    #[serde(default)]
    pub format: Option<i8>, // [1|2|3] Format of data found in the JSON data files.
    #[serde(default)]
    pub version: Option<i8>, // [1|2|3] Alias for format.
    #[serde(default)]
    pub author: Option<String>, // Creator of the dictionary.
    #[serde(default)]
    pub is_updatable: bool, // Whether this dictionary contains links to its latest version.
    #[serde(default)]
    pub index_url: Option<String>, // URL for the index file of the latest revision of the dictionary, used to check for updates.
    #[serde(default)]
    pub download_url: Option<String>, // URL for the download of the latest revision of the dictionary.
    #[serde(default)]
    pub url: Option<String>, // URL for the source of the dictionary, displayed in the dictionary details.
    #[serde(default)]
    pub description: Option<String>, // Description of the dictionary data.
    #[serde(default)]
    pub attribution: Option<String>, // Attribution information for the dictionary data.
    #[serde(default)]
    pub source_language: Option<String>, // Language of the terms in the dictionary.
    #[serde(default)]
    pub target_language: Option<String>, // Main language of the definitions in the dictionary.
    #[serde(default)]
    pub frequency_mode: Option<String>, // ["occurrence-based" | "rank-based"]
    #[serde(default)]
    pub tag_meta: Option<HashMap<String, serde_json::Value>>, // Tag information for terms and kanji. This object is obsolete and individual tag files should be used instead.
}

/// TODO: @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-kanji-bank-v1-schema.json
/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-kanji-bank-v3-schema.json
#[derive(Deserialize)]
struct KanjiBankV3Entry(
    String,            // Kanji character
    String, // String of space-separated onyomi readings for the kanji character. An empty string is treated as no readings.
    String, // String of space-separated kunyomi readings for the kanji character. An empty string is treated as no readings.
    String, // String of space-separated tags for the kanji character. An empty string is treated as no tags.
    Vec<String>, // Array<String> of meanings for the kanji character.
    serde_json::Value, // Various stats for the kanji character.
);

/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-kanji-meta-bank-v3-schema.json
#[derive(Deserialize)]
struct KanjiMetaBankEntry(
    String,
    String,         // Type of data. \"freq\" corresponds to frequency information.
    StringOrNumber, // Data for the character.
);

/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-tag-bank-v3-schema.json
#[derive(Deserialize)]
struct TagBankEntry(
    String, // tag name
    String, // category for the tag
    f64,    // sorting order for the tag
    String, // notes for the tag
    f64, // Score used to determine popularity. Negative values are more rare and positive values are more frequent. This score is also used to sort search results.
);

/// TODO: @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-bank-v1-schema.json
/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-bank-v3-schema.json
#[derive(Deserialize)]
struct TermBankV3Entry(
    String,                 // The text for the term.
    String, // Reading of the term, or an empty string if the reading is the same as the term.
    Option<String>, // String of space-separated tags for the definition. An empty string is treated as no tags.
    String, // String of space-separated rule identifiers for the definition which is used to validate deinflection. An empty string should be used for words which aren't inflected.
    f64, // Score used to determine popularity. Negative values are more rare and positive values are more frequent. This score is also used to sort search results.
    Vec<serde_json::Value>, // Array of definitions for the term. One of:
    // - Single definition for the term. (String)
    // - Single detailed definition for the term. (Object, required [type] in ["text", "image", "structured-content"])
    // - Deinflection of the term to an uninflected term. (2-tuple, [string, string[]])
    i64, // Sequence number for the term. Terms with the same sequence number can be shown together when the \"resultOutputMode\" option is set to \"merge\".
    String, // String of space-separated tags for the term. An empty string is treated as no tags.
);

/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-meta-bank-v3-schema.json
#[derive(Deserialize)]
struct TermMetaBankEntry(
    String, // The text for the term.
    String, // Type of data. \"freq\" corresponds to frequency information; \"pitch\" corresponds to pitch information. \"ipa\" corresponds to IPA transcription.
    serde_json::Value, // Data for the term. One of:
            // - "freq" => string | number | { value: number, displayValue: string } | { reading: string, frequency: string | number | {...} }
            // - "pitch" => { reading: string, pitches: { position: int | string, nasal?: int | int[], devoice?: int | int[], tag?: string[] }[] }
            // - "ipa" => { reading: string, transcriptions: { ipa: string, tag?: string[] }[]}
);

#[derive(Debug, Clone, Serialize)]
pub struct YomitanImportProgress {
    pub bank: String,
    pub current: u32,
    pub total: usize,
}

pub struct YomitanParser {
    root_dir: PathBuf,
}

impl YomitanParser {
    pub async fn from_zip(zip_file: PathBuf, out_dir: PathBuf) -> Result<Self, YomitanError> {
        let root_dir = out_dir.clone();
        create_dir_all(root_dir.clone()).await?;

        tokio::task::spawn_blocking(move || unzip(zip_file, out_dir)).await??;

        Ok(Self { root_dir })
    }

    pub fn from_dir(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub async fn create_db(
        self,
        progress_callback: impl Fn(YomitanImportProgress),
    ) -> Result<(), YomitanError> {
        let db_pathbuf = self.root_dir.join("content.db");
        let db_path = db_pathbuf.as_path();
        if db_path.exists() {
            remove_file(&db_path).await?;
        }

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(false)
            .pragma("cache_size", "-64000") // approximately 64 MiB
            .pragma("temp_store", "MEMORY")
            .pragma("mmap_size", "268435456"); // 256 MiB

        let db = SqlitePool::connect_with(options).await?;

        sqlx::migrate!("migrations/parser").run(&db).await?;

        let mut json_files: HashMap<String, Vec<(u32, PathBuf)>> = HashMap::new();

        let mut entries = read_dir(self.root_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_file() {
                continue;
            }

            let path = entry.path();

            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                continue;
            }

            if let Some(filestem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some((bank_name, id_str)) = filestem.rsplit_once("_")
                    && let Ok(idx) = id_str.parse::<u32>()
                {
                    match json_files.get_mut(bank_name) {
                        Some(rs) => {
                            rs.push((idx, path));
                        }
                        _ => {
                            json_files.insert(bank_name.to_string(), vec![(idx, path)]);
                        }
                    };
                }
            }
        }

        let mut parsed_files: Vec<&PathBuf> = vec![];

        for (bank_name, rs) in json_files.iter_mut() {
            rs.sort_by_key(|r| r.0);

            let total = rs.len();

            for (idx, path) in rs {
                progress_callback(YomitanImportProgress {
                    bank: bank_name.to_string(),
                    current: *idx,
                    total,
                });

                let json_str = tokio::fs::read_to_string(&path).await?;

                match bank_name.as_str() {
                    "kanji_bank" => {
                        parsed_files.push(path);
                        let rs: Vec<KanjiBankV3Entry> = serde_json::from_str(&json_str)?;

                        let mut tx = db.begin().await?;
                        for r in rs {
                            let r4s = serde_json::to_string(&r.4)?;
                            let r5s = serde_json::to_string(&r.5)?;

                            sqlx::query("
                                    INSERT INTO kanji (kanji, onyomi, kunyomi, tags, meanings, stats)
                                    VALUES (?, ?, ?, ?, ?, ?)
                                ")
                                    .bind(r.0)
                                    .bind(r.1)
                                    .bind(r.2)
                                    .bind(r.3)
                                    .bind(r4s)
                                    .bind(r5s)
                                    .execute(&mut *tx)
                                    .await?;
                        }
                        tx.commit().await?;
                    }
                    "kanji_meta_bank" => {
                        parsed_files.push(path);
                        let rs: Vec<KanjiMetaBankEntry> = serde_json::from_str(&json_str)?;

                        let mut tx = db.begin().await?;
                        for r in rs {
                            let mut q = sqlx::query(
                                "
                                    INSERT INTO kanji_meta (kanji, `type`, `data`)
                                    VALUES (?, ?, ?)
                                ",
                            )
                            .bind(r.0)
                            .bind(r.1);

                            match r.2 {
                                StringOrNumber::Number(i) => q = q.bind(i),
                                StringOrNumber::String(s) => q = q.bind(s),
                            }

                            q.execute(&mut *tx).await?;
                        }
                        tx.commit().await?;
                    }
                    "tag_bank" => {
                        parsed_files.push(path);
                        let rs: Vec<TagBankEntry> = serde_json::from_str(&json_str)?;

                        let mut tx = db.begin().await?;
                        for r in rs {
                            sqlx::query(
                                "
                                    INSERT INTO tag (name, category, sort_order, notes, score)
                                    VALUES (?, ?, ?, ?, ?)
                                ",
                            )
                            .bind(r.0)
                            .bind(r.1)
                            .bind(r.2)
                            .bind(r.3)
                            .bind(r.4)
                            .execute(&mut *tx)
                            .await?;
                        }
                        tx.commit().await?;
                    }
                    "term_bank" => {
                        parsed_files.push(path);
                        let rs: Vec<TermBankV3Entry> = serde_json::from_str(&json_str)?;

                        let mut tx = db.begin().await?;
                        for r in rs {
                            let r5s = serde_json::to_string(&r.5)?;

                            sqlx::query(
                                    "
                                    INSERT INTO term (term, reading, def_tags, rules, score, glossary, sequence, tags)
                                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                                ",
                                )
                                .bind(r.0)
                                .bind(r.1)
                                .bind(r.2)
                                .bind(r.3)
                                .bind(r.4)
                                .bind(r5s)
                                .bind(r.6)
                                .bind(r.7)
                                .execute(&mut *tx)
                                .await?;
                        }
                        tx.commit().await?;
                    }
                    "term_meta_bank" => {
                        parsed_files.push(path);
                        let rs: Vec<TermMetaBankEntry> = serde_json::from_str(&json_str)?;

                        let mut tx = db.begin().await?;
                        for r in rs {
                            let r2s = serde_json::to_string(&r.2)?;

                            sqlx::query(
                                "
                                    INSERT INTO term_meta (term, type, `data`)
                                    VALUES (?, ?, ?)
                                ",
                            )
                            .bind(r.0)
                            .bind(r.1)
                            .bind(r2s)
                            .execute(&mut *tx)
                            .await?;
                        }
                        tx.commit().await?;
                    }
                    _ => (),
                };
            }
        }

        let rows = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&db)
            .await?;
        if rows.is_empty() {
            db.close().await;
            join_all(parsed_files.iter().map(|f| remove_file(f))).await;

            Ok(())
        } else {
            let e = format!("import left {} FK violations", rows.len());
            db.close().await;

            Err(YomitanError::Error(e))
        }
    }
}

fn unzip(path: PathBuf, out_dir: PathBuf) -> zip::result::ZipResult<()> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(out_dir)?;
    Ok(())
}

#[derive(Clone)]
pub struct YomitanReader {
    pub root_dir: PathBuf,
    pub db: Pool<Sqlite>,
}

impl YomitanReader {
    pub fn open(root_dir: PathBuf) -> Self {
        let options = SqliteConnectOptions::new()
            .filename(root_dir.join("content.db"))
            .foreign_keys(true);

        // connect with lazy doesn't need to await,
        // but it's prob a large db file taking load time, anyway.
        let db = SqlitePool::connect_lazy_with(options);

        Self { root_dir, db }
    }

    pub async fn get_index_json_str(self) -> Result<String, YomitanError> {
        Ok(read_to_string(self.root_dir.join("index.json")).await?)
    }

    pub async fn get_index_json(self) -> Result<YomitanIndex, YomitanError> {
        Ok(serde_json::from_str(&self.get_index_json_str().await?)?)
    }
}
