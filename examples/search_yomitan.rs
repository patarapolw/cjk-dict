use std::path::{Path, PathBuf};

use yomitan_parser::{
    error::YomitanError,
    search::YomitanSearch,
    tokenize::{Lang, TokenizerMapper},
};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let root_dir = Path::new("tmp");

    let tok = TokenizerMapper::new(PathBuf::from(
        "C:\\Users\\HP\\AppData\\Roaming\\cc.polv.cjk-explainer\\lindera",
    ));

    let _ = YomitanSearch::init(
        root_dir.join("yomitan.db"),
        vec!["jitenon-kotowaza", "jitsuyou", "Pixiv", "sankoku8", "smk8"],
        Lang::Ja,
        tok,
        |p| {
            if p.current % 10_000 == 0 {
                println!("{:?}", p);
            }
        },
    )
    .await?;

    Ok(())
}
