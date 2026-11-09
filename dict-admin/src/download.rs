use std::{fs::File, io::copy, path::Path};

use anyhow::{Result, bail};

pub fn to_file(url: &str, path: &Path) -> Result<()> {
    // reqwest::blocking owns a runtime; keep it off the async worker thread
    tokio::task::block_in_place(|| to_file_blocking(url, path))
}

fn to_file_blocking(url: &str, path: &Path) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("cjk-dict-admin")
        .timeout(None)
        .build()?;
    let mut res = client.get(url).send()?;
    if !res.status().is_success() {
        bail!("GET {url}: {}", res.status());
    }
    let tmp = path.with_extension("dl-tmp");
    {
        let mut f = File::create(&tmp)?;
        copy(&mut res, &mut f)?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}
