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

    let dicts = vec!["jitenon-kotowaza", "jitsuyou", "sankoku8", "smk8"];
    let mut all_dicts = dicts.clone();
    all_dicts.push("PixivLight");

    let yomi = YomitanSearch::new(root_dir.join("search.db"), all_dicts).await?;

    fn cb(p: YomitanSearchInitProgress) {
        if p.current % 10_000 == 0 {
            println!("{:?}", p);
        }
    }

    yomi.import("PixivLight", None, &tok, cb).await?;

    for dict in dicts {
        yomi.import(dict, Some(Lang::Ja), &tok, cb).await?;
    }

    Ok(())
}
