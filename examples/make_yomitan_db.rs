use std::path::{Path, PathBuf};

use tokio::fs::read_dir;
use yomitan_parser::{error::YomitanError, parser::YomitanParser};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let mut zip_paths: Vec<PathBuf> = vec![];

    let root = Path::new("D:\\Projects\\cjdic2\\tmp");

    // zip_paths.append(&mut read_dir_for_ext(root, "zip").await?);
    zip_paths.append(&mut read_dir_for_ext(root.join("yomitan"), "zip").await?);

    for en in zip_paths.iter() {
        let stem = en.as_path().file_stem().unwrap();
        println!("{:?}", stem);

        let yomi = YomitanParser::from_zip(en.clone(), Path::new("tmp").join(stem)).await?;
        yomi.create_db(|p| {
            if p.current % 50 == 1 {
                println!("{} ({}/{})", p.bank, p.current, p.total);
            }
        })
        .await?;
    }

    Ok(())
}

async fn read_dir_for_ext(path: impl AsRef<Path>, ext: &str) -> Result<Vec<PathBuf>, YomitanError> {
    let mut out_paths: Vec<PathBuf> = vec![];

    let mut entries = read_dir(path).await?;
    while let Some(en) = entries.next_entry().await? {
        if !en.file_type().await?.is_file() {
            continue;
        }
        let path = en.path();
        if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case(ext))
        {
            out_paths.push(en.path());
        }
    }

    Ok(out_paths)
}
