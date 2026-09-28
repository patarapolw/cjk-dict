use std::path::Path;

use yomitan_parser::{error::YomitanError, search::YomitanSearch};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let root_dir = Path::new("tmp");

    let _ = YomitanSearch::init(
        root_dir.join("yomitan.db"),
        vec!["jitenon-kotowaza", "jitsuyou", "Pixiv", "sankoku8", "smk8"],
        "ja-JP",
        |p| {
            if p.current % 10_000 == 0 {
                println!("{:?}", p);
            }
        },
    )
    .await?;

    Ok(())
}
