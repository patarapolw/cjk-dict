//! cargo run -p yomitan-parser --features zip --example test_tokenize -- <dict.zip> <lindera-dir>

use std::path::PathBuf;

use yomitan_parser::{
    error::YomitanError,
    parser::{BankEntries, BankKind, YomitanZip},
    tokenize::{Lang, TokenizerMapper},
};

#[tokio::main]
async fn main() -> Result<(), YomitanError> {
    let mut args = std::env::args().skip(1);
    let zip_path = args
        .next()
        .ok_or("usage: <dict.zip> <lindera-dir>".to_string())?;
    let lindera_dir = args.next().unwrap_or_else(|| "lindera".to_string());

    let tok = TokenizerMapper::new(PathBuf::from(lindera_dir));
    let mut zip = YomitanZip::open(zip_path)?;

    let Some(bank) = zip.banks().into_iter().find(|b| b.kind == BankKind::Term) else {
        return Ok(());
    };
    if let BankEntries::Term(mut terms) = zip.read_bank(&bank)? {
        terms.sort_by_key(|t| std::cmp::Reverse(t.term.chars().count()));
        for t in terms.iter().take(10) {
            println!("{} {:?}", t.term, tok.tokenize(Lang::Ja, &t.term).await?);
        }
    }

    Ok(())
}
