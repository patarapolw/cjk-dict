//! The local dictionary library: `<name>.zip` files + `dicts.json`, plus the
//! `search.db` / `manifest.json` that `build` writes. `upload` mirrors it to R2.

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DICTS_JSON: &str = "dicts.json";
pub const SEARCH_DB: &str = "search.db";
pub const MANIFEST: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictEntry {
    pub name: String,
    /// overrides index.json sourceLanguage, e.g. ja-JP
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// where it came from (URL or file name), informational
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// What the app downloads: compare `sha256` with the local copy to decide what to fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub search_db: FileInfo,
    pub dicts: Vec<ManifestDict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDict {
    pub name: String,
    pub title: String,
    pub revision: String,
    pub lang: Option<String>,
    pub term_count: usize,
    #[serde(flatten)]
    pub file: FileInfo,
}

pub struct Library {
    pub dir: PathBuf,
}

impl Library {
    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating library {}", dir.display()))?;
        Ok(Self { dir })
    }

    pub fn zip_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.zip"))
    }

    pub fn dicts(&self) -> Result<Vec<DictEntry>> {
        let p = self.dir.join(DICTS_JSON);
        if !p.exists() {
            return Ok(vec![]);
        }
        serde_json::from_str(&std::fs::read_to_string(&p)?)
            .with_context(|| format!("parsing {}", p.display()))
    }

    pub fn save_dicts(&self, dicts: &[DictEntry]) -> Result<()> {
        let tmp = self.dir.join(format!("{DICTS_JSON}.tmp"));
        std::fs::write(&tmp, serde_json::to_string_pretty(dicts)? + "\n")?;
        std::fs::rename(tmp, self.dir.join(DICTS_JSON))?;
        Ok(())
    }

    pub fn manifest(&self) -> Result<Option<Manifest>> {
        let p = self.dir.join(MANIFEST);
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&std::fs::read_to_string(p)?)?))
    }

    /// Errors unless manifest.json matches the current zips and dicts.json.
    pub fn check_fresh(&self) -> Result<Manifest> {
        let Some(m) = self.manifest()? else {
            bail!("no {MANIFEST}: run `build` first");
        };
        let dicts = self.dicts()?;
        let names: Vec<&str> = dicts.iter().map(|d| d.name.as_str()).collect();
        let built: Vec<&str> = m.dicts.iter().map(|d| d.name.as_str()).collect();
        if names != built {
            bail!("{MANIFEST} is out of date (dictionaries changed): run `build`");
        }
        for d in &m.dicts {
            let size = std::fs::metadata(self.zip_path(&d.name))?.len();
            if size != d.file.size {
                bail!("{}.zip changed since the last build: run `build`", d.name);
            }
        }
        Ok(m)
    }
}

pub fn file_info(path: &Path, file: &str) -> Result<FileInfo> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        size += n as u64;
    }
    let sha256 = h
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(FileInfo {
        file: file.to_string(),
        size,
        sha256,
    })
}

/// Dict names become file names and URL paths.
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("invalid dictionary name {name:?}: use letters, digits, '-', '_' and '.'");
    }
    Ok(())
}
