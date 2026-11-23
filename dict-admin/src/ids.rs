//! The CJKVI IDS database's `ids.txt`: how CJK characters break down into components,
//! for the character tables of `search.db` (`yomitan_parser::search::build_chars`). Kept in
//! the library, as the app builds its search.db from it too.
//!
//! @see https://github.com/cjkvi/cjkvi-ids. `ids.txt` is derived from the CHISE project's
//! IDS database and follows its license (GPLv2).

use std::path::{Path, PathBuf};

use anyhow::Result;

pub const IDS_URL: &str = "https://raw.githubusercontent.com/cjkvi/cjkvi-ids/master/ids.txt";

use crate::library::IDS_TXT;

/// `<library>/ids.txt`, downloaded if missing. Delete it to get the latest.
pub fn ensure(library: &Path) -> Result<PathBuf> {
    let path = library.join(IDS_TXT);
    if !path.is_file() {
        eprintln!("downloading {IDS_URL}");
        let tmp = library.join(format!(".{IDS_TXT}.part"));
        crate::download::to_file(IDS_URL, &tmp)?;
        std::fs::rename(&tmp, &path)?;
    }
    Ok(path)
}
