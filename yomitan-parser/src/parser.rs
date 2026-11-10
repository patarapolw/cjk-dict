//! Read a Yomitan dictionary zip in place (no extraction).
//!
//! - [`YomitanZip::index_json_str`] / [`YomitanZip::index`]: `index.json`
//! - [`YomitanZip::banks`] + [`YomitanZip::read_bank`]: `*_bank_N.json` entries
//! - [`YomitanZip::asset_names`] + [`YomitanZip::read_bytes`]: every other file (images etc.)

use std::{collections::HashMap, fs::File, io::Read, path::Path};

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::error::YomitanError;

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrNumber {
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
pub(crate) struct KanjiBankV3Entry(
    String,            // Kanji character
    String, // String of space-separated onyomi readings for the kanji character. An empty string is treated as no readings.
    String, // String of space-separated kunyomi readings for the kanji character. An empty string is treated as no readings.
    String, // String of space-separated tags for the kanji character. An empty string is treated as no tags.
    Vec<String>, // Array<String> of meanings for the kanji character.
    serde_json::Value, // Various stats for the kanji character.
);

/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-kanji-meta-bank-v3-schema.json
#[derive(Deserialize)]
pub(crate) struct KanjiMetaBankEntry(
    String,
    String,         // Type of data. \"freq\" corresponds to frequency information.
    StringOrNumber, // Data for the character.
);

/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-tag-bank-v3-schema.json
#[derive(Deserialize)]
pub(crate) struct TagBankEntry(
    String, // tag name
    String, // category for the tag
    f64,    // sorting order for the tag
    String, // notes for the tag
    f64, // Score used to determine popularity. Negative values are more rare and positive values are more frequent. This score is also used to sort search results.
);

/// TODO: @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-bank-v1-schema.json
/// @see https://github.com/yomidevs/yomitan/blob/master/ext/data/schemas/dictionary-term-bank-v3-schema.json
#[derive(Deserialize)]
pub(crate) struct TermBankV3Entry(
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
pub(crate) struct TermMetaBankEntry(
    String, // The text for the term.
    String, // Type of data. \"freq\" corresponds to frequency information; \"pitch\" corresponds to pitch information. \"ipa\" corresponds to IPA transcription.
    serde_json::Value, // Data for the term. One of:
            // - "freq" => string | number | { value: number, displayValue: string } | { reading: string, frequency: string | number | {...} }
            // - "pitch" => { reading: string, pitches: { position: int | string, nasal?: int | int[], devoice?: int | int[], tag?: string[] }[] }
            // - "ipa" => { reading: string, transcriptions: { ipa: string, tag?: string[] }[]}
);

#[derive(Debug, Clone)]
pub struct TermEntry {
    pub term: String,
    pub reading: String,
    pub def_tags: Option<String>,
    pub rules: String,
    pub score: f64,
    /// JSON array of definitions
    pub glossary: String,
    pub sequence: i64,
    pub tags: String,
}

#[derive(Debug, Clone)]
pub struct TermMetaEntry {
    pub term: String,
    pub r#type: String,
    /// JSON
    pub data: String,
}

#[derive(Debug, Clone)]
pub struct KanjiEntry {
    pub kanji: String,
    pub onyomi: String,
    pub kunyomi: String,
    pub tags: String,
    /// JSON array of strings
    pub meanings: String,
    /// JSON object
    pub stats: String,
}

#[derive(Debug, Clone)]
pub struct KanjiMetaEntry {
    pub kanji: String,
    pub r#type: String,
    pub data: StringOrNumber,
}

#[derive(Debug, Clone)]
pub struct TagEntry {
    pub name: String,
    pub category: String,
    pub sort_order: f64,
    pub notes: String,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub enum BankEntries {
    Term(Vec<TermEntry>),
    TermMeta(Vec<TermMetaEntry>),
    Kanji(Vec<KanjiEntry>),
    KanjiMeta(Vec<KanjiMetaEntry>),
    Tag(Vec<TagEntry>),
}

impl BankEntries {
    pub fn len(&self) -> usize {
        match self {
            Self::Term(v) => v.len(),
            Self::TermMeta(v) => v.len(),
            Self::Kanji(v) => v.len(),
            Self::KanjiMeta(v) => v.len(),
            Self::Tag(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BankKind {
    // order matters: tags first, terms last-ish
    Tag,
    Kanji,
    KanjiMeta,
    Term,
    TermMeta,
}

impl BankKind {
    pub fn from_name(bank_name: &str) -> Option<Self> {
        match bank_name {
            "tag_bank" => Some(Self::Tag),
            "kanji_bank" => Some(Self::Kanji),
            "kanji_meta_bank" => Some(Self::KanjiMeta),
            "term_bank" => Some(Self::Term),
            "term_meta_bank" => Some(Self::TermMeta),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BankFile {
    pub kind: BankKind,
    pub index: u32,
    /// file name inside the zip
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct YomitanImportProgress {
    pub bank: String,
    pub current: u32,
    pub total: usize,
}

pub struct YomitanZip {
    archive: ZipArchive<File>,
}

impl YomitanZip {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, YomitanError> {
        let file = File::open(path)?;
        Ok(Self {
            archive: ZipArchive::new(file)?,
        })
    }

    pub fn read_to_string(&mut self, name: &str) -> Result<String, YomitanError> {
        let mut f = self.archive.by_name(name)?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        Ok(s)
    }

    pub fn index_json_str(&mut self) -> Result<String, YomitanError> {
        self.read_to_string("index.json")
    }

    pub fn index(&mut self) -> Result<YomitanIndex, YomitanError> {
        Ok(serde_json::from_str(&self.index_json_str()?)?)
    }

    /// `*_bank_N.json` files, sorted by kind then N
    pub fn banks(&self) -> Vec<BankFile> {
        let mut out: Vec<BankFile> = self
            .archive
            .file_names()
            .filter_map(|name| {
                let stem = name.strip_suffix(".json")?;
                if stem.contains('/') {
                    return None;
                }
                let (bank_name, idx) = stem.rsplit_once('_')?;
                Some(BankFile {
                    kind: BankKind::from_name(bank_name)?,
                    index: idx.parse().ok()?,
                    name: name.to_string(),
                })
            })
            .collect();
        out.sort_by_key(|b| (b.kind, b.index));
        out
    }

    pub fn read_bank(&mut self, bank: &BankFile) -> Result<BankEntries, YomitanError> {
        let json_str = self.read_to_string(&bank.name)?;

        Ok(match bank.kind {
            BankKind::Term => {
                let rs: Vec<TermBankV3Entry> = serde_json::from_str(&json_str)?;
                let mut out = Vec::with_capacity(rs.len());
                for r in rs {
                    out.push(TermEntry {
                        glossary: serde_json::to_string(&r.5)?,
                        term: r.0,
                        reading: r.1,
                        def_tags: r.2,
                        rules: r.3,
                        score: r.4,
                        sequence: r.6,
                        tags: r.7,
                    });
                }
                BankEntries::Term(out)
            }
            BankKind::TermMeta => {
                let rs: Vec<TermMetaBankEntry> = serde_json::from_str(&json_str)?;
                let mut out = Vec::with_capacity(rs.len());
                for r in rs {
                    out.push(TermMetaEntry {
                        data: serde_json::to_string(&r.2)?,
                        term: r.0,
                        r#type: r.1,
                    });
                }
                BankEntries::TermMeta(out)
            }
            BankKind::Kanji => {
                let rs: Vec<KanjiBankV3Entry> = serde_json::from_str(&json_str)?;
                let mut out = Vec::with_capacity(rs.len());
                for r in rs {
                    out.push(KanjiEntry {
                        meanings: serde_json::to_string(&r.4)?,
                        stats: serde_json::to_string(&r.5)?,
                        kanji: r.0,
                        onyomi: r.1,
                        kunyomi: r.2,
                        tags: r.3,
                    });
                }
                BankEntries::Kanji(out)
            }
            BankKind::KanjiMeta => {
                let rs: Vec<KanjiMetaBankEntry> = serde_json::from_str(&json_str)?;
                BankEntries::KanjiMeta(
                    rs.into_iter()
                        .map(|r| KanjiMetaEntry {
                            kanji: r.0,
                            r#type: r.1,
                            data: r.2,
                        })
                        .collect(),
                )
            }
            BankKind::Tag => {
                let rs: Vec<TagBankEntry> = serde_json::from_str(&json_str)?;
                BankEntries::Tag(
                    rs.into_iter()
                        .map(|r| TagEntry {
                            name: r.0,
                            category: r.1,
                            sort_order: r.2,
                            notes: r.3,
                            score: r.4,
                        })
                        .collect(),
                )
            }
        })
    }

    /// A bank file as raw JSON rows (array of arrays).
    pub fn read_json_array(&mut self, name: &str) -> Result<Vec<serde_json::Value>, YomitanError> {
        Ok(serde_json::from_str(&self.read_to_string(name)?)?)
    }

    /// Count entries per bank kind (reads every bank once).
    pub fn count(&mut self) -> Result<HashMap<BankKind, usize>, YomitanError> {
        let mut out = HashMap::new();
        for b in self.banks() {
            let n = self.read_bank(&b)?.len();
            *out.entry(b.kind).or_insert(0) += n;
        }
        Ok(out)
    }

    /// Every non-directory file that is neither `index.json` nor a bank file
    /// (images etc. referenced by structured content `path`).
    pub fn asset_names(&self) -> Vec<String> {
        let bank_names: std::collections::HashSet<String> =
            self.banks().into_iter().map(|b| b.name).collect();
        self.archive
            .file_names()
            .filter(|n| !n.ends_with('/') && *n != "index.json" && !bank_names.contains(*n))
            .map(String::from)
            .collect()
    }

    /// Decompressed content of a file in the zip.
    pub fn read_bytes(&mut self, name: &str) -> Result<Vec<u8>, YomitanError> {
        let mut f = self.archive.by_name(name)?;
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }
}
