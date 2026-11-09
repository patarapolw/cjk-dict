//! Ensure lindera tokenizer dictionaries exist (same layout as the cjk-explainer app).

use std::{fs::File, path::Path};

use anyhow::{Context, Result};

pub const LINDERA_VERSION: &str = "6.2.0";
pub const MODELS: [&str; 3] = ["unidic", "ko-dic", "cc-cedict"];

pub fn ensure(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for model in MODELS {
        if dir.join(format!("lindera-{model}")).exists() {
            continue;
        }
        let filename = format!("lindera-{model}-{LINDERA_VERSION}.zip");
        let url = format!(
            "https://github.com/lindera/lindera/releases/download/v{LINDERA_VERSION}/{filename}"
        );
        let zip_path = dir.join(&filename);
        if !zip_path.exists() {
            eprintln!("downloading {url}");
            crate::download::to_file(&url, &zip_path)?;
        }
        eprintln!("extracting {filename}");
        let mut archive = zip::ZipArchive::new(File::open(&zip_path)?)?;
        archive
            .extract(dir)
            .with_context(|| format!("extracting {filename}"))?;
        std::fs::remove_file(&zip_path)?;
    }
    Ok(())
}
