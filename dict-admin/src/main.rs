//! Admin CLI for cjk-dict.
//!
//! The library is a folder of Yomitan zips (`<name>.zip`) + `dicts.json`. Zips are stored
//! with their term banks re-split into small ones (see `parser::rebank`); otherwise as published.
//! `build` indexes them into `search.db` and writes `manifest.json`; `upload` mirrors the
//! folder to R2 with rclone. The app downloads the zips + search.db and reads definitions
//! and media straight from the zips. This is the only way dictionaries are added or removed.

mod defaults;
mod download;
mod library;
mod lindera;

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use yomitan_parser::{
    parser::{self, YomitanZip},
    search::{self, DictSource, parse_lang},
    tokenize::TokenizerMapper,
};

use crate::library::{
    DictEntry, Library, MANIFEST, Manifest, ManifestDict, SEARCH_DB, check_name, file_info,
};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Library folder: <name>.zip + dicts.json (+ search.db, manifest.json after `build`)
    #[arg(
        long,
        global = true,
        env = "CJK_DICT_LIBRARY",
        default_value = "library"
    )]
    library: PathBuf,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List dictionaries in the library
    List,

    /// Add a Yomitan dictionary zip (local path or http(s) URL) to the library
    Add {
        source: String,

        /// Dictionary name (default: zip file stem); the zip is stored as <name>.zip
        #[arg(long)]
        name: Option<String>,

        /// Source language, e.g. ja-JP / zh-CN / ko-KR (default: index.json sourceLanguage)
        #[arg(long)]
        lang: Option<String>,

        /// Overwrite a dictionary of the same name
        #[arg(long)]
        replace: bool,
    },

    /// Add the default dictionaries (latest releases); existing ones are skipped
    Defaults {
        /// Only these dictionaries (repeatable); see --list for names
        #[arg(long)]
        only: Vec<String>,

        /// Re-download dictionaries that already exist (e.g. to update to the latest release)
        #[arg(long)]
        replace: bool,

        /// Print the default dictionaries and their download URLs, then exit
        #[arg(long)]
        list: bool,
    },

    /// Remove a dictionary from the library
    Remove { name: String },

    /// Build search.db and manifest.json from the library
    Build {
        /// Lindera tokenizer dictionaries (downloaded if missing)
        #[arg(long, env = "CJK_DICT_LINDERA_DIR", default_value = "tmp/lindera")]
        lindera_dir: PathBuf,
    },

    /// Mirror the library to R2 with rclone (zips first, then search.db, then manifest.json)
    Upload {
        /// rclone destination, e.g. r2:cjk-dict
        #[arg(long, env = "CJK_DICT_REMOTE")]
        remote: String,

        /// Show what would be transferred without doing it
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // CJK_DICT_LIBRARY, CJK_DICT_REMOTE, ... from .env (this or a parent folder);
    // variables already set in the environment win
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    let lib = Library::new(cli.library)?;

    match cli.cmd {
        Cmd::List => {
            let built = lib.manifest()?;
            for d in lib.dicts()? {
                let size = std::fs::metadata(lib.zip_path(&d.name))
                    .map(|m| format!("{:.1} MB", m.len() as f64 / 1e6))
                    .unwrap_or_else(|_| "missing!".into());
                let terms = built
                    .as_ref()
                    .and_then(|m| m.dicts.iter().find(|x| x.name == d.name))
                    .map(|x| format!("{} terms", x.term_count))
                    .unwrap_or_else(|| "not built".into());
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    d.name,
                    d.lang.as_deref().unwrap_or("-"),
                    size,
                    terms,
                    d.source.as_deref().unwrap_or("")
                );
            }
        }
        Cmd::Add {
            source,
            name,
            lang,
            replace,
        } => {
            add(&lib, &source, name, lang, replace)?;
            eprintln!("run `build` (and `upload`) to publish");
        }
        Cmd::Defaults {
            only,
            replace,
            list,
        } => {
            let resolved = defaults::resolve(&only)?;
            if list {
                for d in &resolved {
                    println!("{}\t{}\t{}", d.name, d.lang.unwrap_or("-"), d.url);
                }
                return Ok(());
            }
            let existing: Vec<String> = lib.dicts()?.into_iter().map(|d| d.name).collect();
            let mut failed = vec![];
            for d in resolved {
                if existing.iter().any(|n| n == d.name) && !replace {
                    eprintln!(
                        "skip {}: already in the library (--replace to update)",
                        d.name
                    );
                    continue;
                }
                if let Err(e) = add(
                    &lib,
                    &d.url,
                    Some(d.name.to_string()),
                    d.lang.map(String::from),
                    replace,
                ) {
                    eprintln!("error: {}: {e:#}", d.name);
                    failed.push(d.name);
                }
            }
            if !failed.is_empty() {
                bail!("failed: {}", failed.join(", "));
            }
            eprintln!("run `build` (and `upload`) to publish");
        }
        Cmd::Remove { name } => {
            let mut dicts = lib.dicts()?;
            let before = dicts.len();
            dicts.retain(|d| d.name != name);
            if dicts.len() == before {
                bail!("{name:?} is not in the library");
            }
            lib.save_dicts(&dicts)?;
            let _ = std::fs::remove_file(lib.zip_path(&name));
            eprintln!("removed {name}; run `build` and `upload` to publish");
        }
        Cmd::Build { lindera_dir } => build(&lib, &lindera_dir).await?,
        Cmd::Upload { remote, dry_run } => upload(&lib, &remote, dry_run)?,
    }

    Ok(())
}

/// Copy or download a zip into the library as `<name>.zip` and record it in dicts.json.
fn add(
    lib: &Library,
    source: &str,
    name: Option<String>,
    lang: Option<String>,
    replace: bool,
) -> Result<()> {
    if let Some(l) = &lang {
        parse_lang(l).with_context(|| format!("unsupported language {l}"))?;
    }
    let is_url = source.starts_with("http://") || source.starts_with("https://");
    let file_name = if is_url {
        source
            .rsplit('/')
            .next()
            .and_then(|s| s.split('?').next())
            .unwrap_or("dict.zip")
            .to_string()
    } else {
        Path::new(source)
            .file_name()
            .context("source has no file name")?
            .to_string_lossy()
            .into_owned()
    };
    let name = match name {
        Some(n) => n,
        None => Path::new(&file_name)
            .file_stem()
            .context("cannot derive --name")?
            .to_string_lossy()
            .into_owned(),
    };
    check_name(&name)?;

    let mut dicts = lib.dicts()?;
    if dicts.iter().any(|d| d.name == name) && !replace {
        bail!("{name:?} is already in the library (--replace to overwrite)");
    }

    // fetch to a temp file, validate, then move into place
    let tmp = lib.dir.join(format!(".{name}.zip.part"));
    if is_url {
        eprintln!("downloading {source}");
        download::to_file(source, &tmp)?;
    } else {
        std::fs::copy(source, &tmp).with_context(|| format!("copying {source}"))?;
    }
    let index = YomitanZip::open(&tmp)
        .and_then(|mut z| z.index())
        .with_context(|| format!("{source} is not a Yomitan dictionary zip"));
    let index = match index {
        Ok(i) => i,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if let Err(e) = rebank(&tmp, &name) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, lib.zip_path(&name))?;

    let entry = DictEntry {
        name: name.clone(),
        lang,
        source: Some(if is_url {
            source.to_string()
        } else {
            file_name
        }),
    };
    match dicts.iter_mut().find(|d| d.name == name) {
        Some(d) => *d = entry,
        None => dicts.push(entry),
    }
    lib.save_dicts(&dicts)?;
    eprintln!("added {name}: {} ({})", index.title, index.revision);
    Ok(())
}

/// Re-split the zip's term banks into small ones (in place), so the app inflates little to
/// read an entry. No-op when they already are small.
fn rebank(zip: &Path, name: &str) -> Result<()> {
    let tmp = zip.with_extension("zip.rebank");
    let t = Instant::now();
    let done = parser::rebank(zip, &tmp, parser::TERM_BANK_BYTES)
        .with_context(|| format!("rebanking {name}"));
    match done {
        Ok(true) => {
            std::fs::rename(&tmp, zip)?;
            eprintln!(
                "{name}: split term banks into {} KB ones ({:.1}s)",
                parser::TERM_BANK_BYTES >> 10,
                t.elapsed().as_secs_f64()
            );
            Ok(())
        }
        Ok(false) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

async fn build(lib: &Library, lindera_dir: &Path) -> Result<()> {
    let dicts = lib.dicts()?;
    if dicts.is_empty() {
        bail!("the library is empty: `add` or `defaults` first");
    }
    let sources = dicts
        .iter()
        .map(|d| {
            let zip = lib.zip_path(&d.name);
            if !zip.exists() {
                bail!("{} is missing", zip.display());
            }
            Ok(DictSource {
                name: d.name.clone(),
                zip,
                lang: d.lang.as_deref().and_then(parse_lang),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // zips added before `add` rebanked them
    for s in &sources {
        rebank(&s.zip, &s.name)?;
    }

    lindera::ensure(lindera_dir)?;
    let tok = TokenizerMapper::new(lindera_dir.to_path_buf());

    let t = Instant::now();
    let db = lib.dir.join(SEARCH_DB);
    let infos = search::build(&db, &sources, &tok, |name, i, n| {
        if n > 0 {
            eprint!("\r{name}: bank {i}/{n}   ");
        } else {
            eprintln!("\r{name}...");
        }
    })
    .await?;
    eprintln!("built {SEARCH_DB} in {:.1}s", t.elapsed().as_secs_f64());

    let mut out = vec![];
    for info in infos {
        let file = format!("{}.zip", info.name);
        out.push(ManifestDict {
            file: file_info(&lib.zip_path(&info.name), &file)?,
            name: info.name,
            title: info.title,
            revision: info.revision,
            lang: info.lang,
            term_count: info.term_count,
        });
    }
    let manifest = Manifest {
        version: 1,
        search_db: file_info(&db, SEARCH_DB)?,
        dicts: out,
    };
    let tmp = lib.dir.join(format!("{MANIFEST}.tmp"));
    std::fs::write(&tmp, serde_json::to_string_pretty(&manifest)? + "\n")?;
    std::fs::rename(tmp, lib.dir.join(MANIFEST))?;

    for d in &manifest.dicts {
        eprintln!(
            "  {}: {} terms, {:.1} MB",
            d.name,
            d.term_count,
            d.file.size as f64 / 1e6
        );
    }
    eprintln!(
        "  {SEARCH_DB}: {:.1} MB",
        manifest.search_db.size as f64 / 1e6
    );
    Ok(())
}

/// Mirror the library to R2. Order matters: the app reads manifest.json first, so
/// everything it points to must already be there; deletions happen last.
fn upload(lib: &Library, remote: &str, dry_run: bool) -> Result<()> {
    lib.check_fresh()?;
    let dir = lib.dir.to_string_lossy().into_owned();
    let rclone = |args: &[&str]| -> Result<()> {
        eprintln!("rclone {}", args.join(" "));
        let mut c = Command::new("rclone");
        c.args(args).arg("--progress");
        if dry_run {
            c.arg("--dry-run");
        }
        let st = c
            .status()
            .context("failed to run rclone; is it installed and on PATH?")?;
        if !st.success() {
            bail!("rclone failed ({st})");
        }
        Ok(())
    };
    let d = dir.as_str();
    rclone(&[
        "copy",
        d,
        remote,
        "--include",
        "*.zip",
        "--include",
        "dicts.json",
    ])?;
    rclone(&["copy", d, remote, "--include", SEARCH_DB])?;
    rclone(&["copy", d, remote, "--include", MANIFEST])?;
    rclone(&[
        "sync",
        d,
        remote,
        "--include",
        "*.zip",
        "--include",
        "dicts.json",
        "--include",
        SEARCH_DB,
        "--include",
        MANIFEST,
    ])?;
    Ok(())
}
