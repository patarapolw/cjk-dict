use std::path::PathBuf;

use futures::TryStreamExt;
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
use yomitan_parser::{error::YomitanError, tokenize::TokenizerMapper};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let tok = TokenizerMapper::new(PathBuf::from(
        "C:\\Users\\HP\\AppData\\Roaming\\cc.polv.cjk-explainer\\lindera",
    ));

    let options = SqliteConnectOptions::new().filename(PathBuf::from("tmp/Pixiv/content.db"));
    let db = SqlitePool::connect_with(options).await?;

    let mut rs =
        sqlx::query("SELECT term FROM term ORDER BY length(term) DESC LIMIT 10").fetch(&db);
    while let Some(r) = rs.try_next().await? {
        let term: &str = r.get(0);
        println!(
            "{} {:?}",
            term,
            tok.tokenize(yomitan_parser::tokenize::Lang::Ja, term)
                .await?
        );
    }

    Ok(())
}
