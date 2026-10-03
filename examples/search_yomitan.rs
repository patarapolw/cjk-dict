use std::path::{Path, PathBuf};

use yomitan_parser::{
    error::YomitanError,
    search::{YomitanSearch, YomitanSearchInitProgress},
    tokenize::{Lang, TokenizerMapper},
};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let root_dir = Path::new("tmp");

    let tok = TokenizerMapper::new(PathBuf::from(
        "C:\\Users\\HP\\AppData\\Roaming\\cc.polv.cjk-explainer\\lindera",
    ));

    let yomi = YomitanSearch::new(
        root_dir.join("yomitan.db"),
        vec!["jitenon-kotowaza", "jitsuyou", "Pixiv", "sankoku8", "smk8"],
    )
    .await?;

    fn cb(p: YomitanSearchInitProgress) {
        if p.current % 10_000 == 0 {
            println!("{:?}", p);
        }
    }

    yomi.import(vec!["Pixiv"], None, &tok, cb).await?;
    yomi.import(
        vec!["jitenon-kotowaza", "jitsuyou", "sankoku8", "smk8"],
        Some(Lang::Ja),
        &tok,
        cb,
    )
    .await?;

    Ok(())
}
