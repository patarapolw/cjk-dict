//! Admin CLI for the cjk-dict Worker. This is the only way dictionaries are added to or
//! removed from D1 / R2; the Worker itself is read-only.

mod download;
mod import;
mod lindera;
mod remove;
mod sql;
mod wrangler;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::wrangler::Wrangler;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(flatten)]
    target: Target,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct Target {
    /// Directory containing wrangler.toml
    #[arg(
        long,
        global = true,
        env = "CJK_DICT_WORKER_DIR",
        default_value = "worker"
    )]
    worker_dir: PathBuf,

    /// D1 database name (as in wrangler.toml)
    #[arg(
        long,
        global = true,
        env = "CJK_DICT_DATABASE",
        default_value = "cjk-dict"
    )]
    database: String,

    /// R2 bucket name (as in wrangler.toml)
    #[arg(
        long,
        global = true,
        env = "CJK_DICT_BUCKET",
        default_value = "cjk-dict"
    )]
    bucket: String,

    /// Use local (wrangler dev) D1/R2 instead of Cloudflare
    #[arg(long, global = true)]
    local: bool,

    /// Command used to run wrangler
    #[arg(long, global = true, env = "WRANGLER", default_value = "npx wrangler")]
    wrangler: String,
}

#[derive(Subcommand)]
enum Cmd {
    /// Apply D1 migrations (worker/migrations)
    Migrate,

    /// List dictionaries in D1
    List,

    /// Add a Yomitan dictionary zip (local path or http(s) URL)
    Add {
        source: String,

        /// Dictionary name/key (default: zip file stem)
        #[arg(long)]
        name: Option<String>,

        /// Source language, e.g. ja-JP / zh-CN / ko-KR (default: index.json sourceLanguage)
        #[arg(long)]
        lang: Option<String>,

        /// Remove an existing dictionary of the same name first
        #[arg(long)]
        replace: bool,

        /// Don't upload the zip to R2 (it is already there)
        #[arg(long)]
        no_upload: bool,

        /// Lindera tokenizer dictionaries (downloaded if missing)
        #[arg(long, env = "CJK_DICT_LINDERA_DIR", default_value = "tmp/lindera")]
        lindera_dir: PathBuf,

        /// Where downloaded zips and generated SQL go
        #[arg(long, default_value = "tmp")]
        work_dir: PathBuf,

        /// Keep generated SQL files
        #[arg(long)]
        keep_sql: bool,

        /// Statements per `wrangler d1 execute --file` call
        #[arg(long, default_value_t = 50_000)]
        chunk_size: usize,
    },

    /// Remove a dictionary from D1 and R2
    Remove {
        name: String,

        /// Rows per DELETE batch
        #[arg(long, default_value_t = 20_000)]
        batch: i64,

        /// Delete the R2 object even if the D1 row is missing
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let t = cli.target;
    let w = Wrangler {
        cmd: t.wrangler.split_whitespace().map(String::from).collect(),
        worker_dir: t.worker_dir,
        database: t.database,
        bucket: t.bucket,
        local: t.local,
    };
    if w.cmd.is_empty() {
        bail!("--wrangler must not be empty");
    }

    match cli.cmd {
        Cmd::Migrate => w.migrate()?,
        Cmd::List => {
            let rows = w.query_rows(
                "SELECT id, name, title, revision, L1, L2, status, term_count, zip_size FROM dict ORDER BY id",
            )?;
            for r in rows {
                println!("{r}");
            }
        }
        Cmd::Add {
            source,
            name,
            lang,
            replace,
            no_upload,
            lindera_dir,
            work_dir,
            keep_sql,
            chunk_size,
        } => {
            let lang = match lang {
                Some(l) => Some(
                    import::parse_lang(&l).with_context(|| format!("unsupported --lang {l}"))?,
                ),
                None => None,
            };

            let zip = if source.starts_with("http://") || source.starts_with("https://") {
                let filename = source
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.split('?').next())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("dict.zip");
                let dir = work_dir.join("downloads");
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(filename);
                if !path.exists() {
                    eprintln!("downloading {source}");
                    download::to_file(&source, &path)?;
                }
                path
            } else {
                PathBuf::from(&source)
            };

            let name = match name {
                Some(n) => n,
                None => zip
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .context("cannot derive --name from source")?
                    .to_string(),
            };

            if replace {
                match remove::remove(&w, &name, 20_000, false) {
                    Ok(()) => {}
                    Err(e) if e.to_string().contains("not found") => {}
                    Err(e) => return Err(e),
                }
            }

            import::add(
                &w,
                import::AddOpts {
                    zip,
                    name,
                    lang,
                    lindera_dir,
                    sql_dir: work_dir.join("sql"),
                    keep_sql,
                    chunk_size,
                    upload: !no_upload,
                },
            )
            .await?;
        }
        Cmd::Remove { name, batch, force } => remove::remove(&w, &name, batch, force)?,
    }

    Ok(())
}
