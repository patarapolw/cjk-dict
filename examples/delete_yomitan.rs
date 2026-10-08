use std::path::Path;

use yomitan_parser::{error::YomitanError, search::YomitanSearch};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let root_dir = Path::new("tmp");

    let yomi = YomitanSearch::new(root_dir.join("yomitan.db"), vec!["Pixiv"]).await?;

    yomi.delete("Pixiv", |s| {
        println!("{}", s);
    })
    .await?;

    Ok(())
}
