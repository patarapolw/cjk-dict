//! The CJKVI IDS database's `ids.txt`: how CJK characters break down into components,
//! for the character tables of `search.db` (`yomitan_parser::search::build_chars`).
//!
//! @see https://github.com/cjkvi/cjkvi-ids. `ids.txt` is derived from the CHISE project's
//! IDS database and follows its license (GPLv2).

use std::path::{Path, PathBuf};

use anyhow::Result;

pub const IDS_URL: &str = "https://raw.githubusercontent.com/cjkvi/cjkvi-ids/master/ids.txt";

/// `<dir>/ids.txt`, downloaded if missing. Delete it to get the latest.
pub fn ensure(dir: &Path) -> Result<PathBuf> {
    let path = dir.join("ids.txt");
    if !path.is_file() {
        std::fs::create_dir_all(dir)?;
        eprintln!("downloading {IDS_URL}");
        crate::download::to_file(IDS_URL, &path)?;
    }
    Ok(path)
}
