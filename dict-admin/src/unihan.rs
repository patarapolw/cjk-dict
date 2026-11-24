//! Unihan's `Unihan_Variants.txt`: the variant forms of Han characters (学 → 學), for
//! `search.db`'s `han_variant` (`yomitan_parser::search::build_chars`).
//!
//! @see https://www.unicode.org/reports/tr38/. Unicode License v3.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The full Unihan release is only on unicode.org, as a zip of every field
pub const UNIHAN_URL: &str = "https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip";
const VARIANTS: &str = "Unihan_Variants.txt";

/// `<dir>/Unihan_Variants.txt`, taken from a download of `Unihan.zip` if missing; the zip
/// isn't kept. Delete it to get the latest.
pub fn ensure(dir: &Path) -> Result<PathBuf> {
    let path = dir.join(VARIANTS);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(dir)?;
    let zip_path = dir.join("Unihan.zip");
    eprintln!("downloading {UNIHAN_URL}");
    crate::download::to_file(UNIHAN_URL, &zip_path)?;
    let extracted = extract(&zip_path, &path);
    std::fs::remove_file(&zip_path)?;
    extracted.with_context(|| format!("{VARIANTS} from {}", zip_path.display()))?;
    Ok(path)
}

fn extract(zip_path: &Path, to: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut entry = zip.by_name(VARIANTS)?;
    let tmp = to.with_extension("dl-tmp");
    std::io::copy(&mut entry, &mut std::fs::File::create(&tmp)?)?;
    std::fs::rename(tmp, to)?;
    Ok(())
}
