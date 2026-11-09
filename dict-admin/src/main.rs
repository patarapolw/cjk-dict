//! Admin CLI for the cjk-dict Worker. This is the only way dictionaries are added to or
//! removed from D1 / R2; the Worker itself is read-only.

mod defaults;
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

    /// Command used to run wrangler [default: `node <worker-dir>/node_modules/wrangler/bin/wrangler.js`,
    /// i.e. the workspace's wrangler, run without a shell so it works the same on Windows]
    #[arg(long, global = true, env = "WRANGLER")]
    wrangler: Option<String>,
}

/// Options shared by `add` and `defaults`
#[derive(Args, Clone)]
struct ImportArgs {
    /// Don't upload packs to R2 (they are already there)
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

        #[command(flatten)]
        import: ImportArgs,
    },

    /// Import the default dictionaries (latest releases); already imported ones are skipped
    Defaults {
        /// Only these dictionaries (repeatable); see --list for names
        #[arg(long)]
        only: Vec<String>,

        /// Re-import dictionaries that already exist (e.g. to update to the latest release)
        #[arg(long)]
        replace: bool,

        /// Print the default dictionaries and their download URLs, then exit
        #[arg(long)]
        list: bool,

        #[command(flatten)]
        import: ImportArgs,
    },

    /// Remove a dictionary from D1 and R2
    Remove {
        name: String,

        /// Rows per DELETE batch
        #[arg(long, default_value_t = 20_000)]
        batch: i64,

        /// Delete the R2 packs even if the D1 row is missing
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let t = cli.target;
    let cmd: Vec<String> = match &t.wrangler {
        Some(c) => c.split_whitespace().map(String::from).collect(),
        None => {
            // Run wrangler's JS entry with node directly: on Windows `pnpm`/`npx` are .cmd
            // shims, which Command can't spawn without going through cmd.exe quoting.
            let js = t.worker_dir.join("node_modules/wrangler/bin/wrangler.js");
            if !js.exists() {
                bail!(
                    "{} not found: run `pnpm install` at the repo root, or pass --wrangler",
                    js.display()
                );
            }
            vec![
                "node".to_string(),
                std::path::absolute(&js)?.to_string_lossy().into_owned(),
            ]
        }
    };
    let w = Wrangler {
        cmd,
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
                "SELECT id, name, title, revision, L1, L2, status, term_count, assets_size, source FROM dict ORDER BY id",
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
            import,
        } => {
            let lang = match lang {
                Some(l) => Some(
                    import::parse_lang(&l).with_context(|| format!("unsupported --lang {l}"))?,
                ),
                None => None,
            };
            add_source(&w, &source, name, lang, replace, &import).await?;
        }
        Cmd::Defaults {
            only,
            replace,
            list,
            import,
        } => {
            let resolved = defaults::resolve(&only)?;
            if list {
                for d in &resolved {
                    println!("{}\t{}\t{}", d.name, d.lang.unwrap_or("-"), d.url);
                }
                return Ok(());
            }

            let existing: std::collections::HashMap<String, String> = w
                .query_rows("SELECT name, status FROM dict")?
                .into_iter()
                .filter_map(|r| {
                    Some((
                        r["name"].as_str()?.to_string(),
                        r["status"].as_str().unwrap_or("").to_string(),
                    ))
                })
                .collect();

            let mut failed = vec![];
            for d in resolved {
                if let Some(status) = existing.get(d.name) {
                    if !replace {
                        eprintln!(
                            "skip {}: already imported (status {status}); --replace to re-import",
                            d.name
                        );
                        continue;
                    }
                }
                eprintln!("== {} ({})", d.name, d.url);
                let lang = d.lang.and_then(import::parse_lang);
                if let Err(e) =
                    add_source(&w, &d.url, Some(d.name.to_string()), lang, replace, &import).await
                {
                    eprintln!("error: {}: {e:#}", d.name);
                    failed.push(d.name);
                }
            }
            if !failed.is_empty() {
                bail!("failed: {}", failed.join(", "));
            }
        }
        Cmd::Remove { name, batch, force } => remove::remove(&w, &name, batch, force)?,
    }

    Ok(())
}

/// Download (if a URL), optionally remove the existing dict, and import.
async fn add_source(
    w: &Wrangler,
    source: &str,
    name: Option<String>,
    lang: Option<yomitan_parser::tokenize::Lang>,
    replace: bool,
    opts: &ImportArgs,
) -> Result<()> {
    let is_url = source.starts_with("http://") || source.starts_with("https://");

    let source_label = if is_url {
        source.to_string()
    } else {
        PathBuf::from(source)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| source.to_string())
    };

    let zip = if is_url {
        let filename = source
            .rsplit('/')
            .next()
            .and_then(|s| s.split('?').next())
            .filter(|s| !s.is_empty())
            .unwrap_or("dict.zip");
        let dir = opts.work_dir.join("downloads");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(filename);
        if !path.exists() {
            eprintln!("downloading {source}");
            download::to_file(source, &path)?;
        }
        path
    } else {
        PathBuf::from(source)
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
        match remove::remove(w, &name, 20_000, false) {
            Ok(()) => {}
            Err(e) if e.to_string().contains("not found") => {}
            Err(e) => return Err(e),
        }
    }

    import::add(
        w,
        import::AddOpts {
            zip,
            source: source_label,
            name,
            lang,
            lindera_dir: opts.lindera_dir.clone(),
            sql_dir: opts.work_dir.join("sql"),
            keep_sql: opts.keep_sql,
            chunk_size: opts.chunk_size,
            upload: !opts.no_upload,
        },
    )
    .await
}
