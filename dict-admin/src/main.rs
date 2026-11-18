//! Admin CLI for cjk-dict.
//!
//! The library is a folder of Yomitan zips (`<name>.zip`) + `dicts.json`. Zips are stored
//! with their term banks re-split into small ones (see `parser::rebank`); otherwise as published.
//! `build` indexes them into `search.db` and writes `manifest.json`; `upload` mirrors the
//! folder to R2 with rclone. The app downloads the zips + search.db and reads definitions
//! and media straight from the zips. This is the only way dictionaries are added or removed.

mod defaults;
mod download;
mod ids;
mod kanjidic;
mod library;
mod lindera;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use yomitan_parser::{
    parser::{self, BankKind, YomitanZip},
    search::{self, DictSource, lang_tag, parse_lang},
    tokenize::TokenizerMapper,
};

use crate::library::{
    DICTS_JSON, DictEntry, Library, MANIFEST, Manifest, ManifestDict, SEARCH_DB, check_name,
    file_info,
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

    /// Show the R2 mirror size, grouped by dictionary language
    Size {
        /// Only show dictionary zips for this language (e.g. ja-JP)
        #[arg(long)]
        lang: Option<String>,
    },

    /// Add a Yomitan dictionary zip (local path or http(s) URL) to the library. KANJIDIC2
    /// JSON from jmdict-simplified (or a zip of it) is converted to one.
    Add {
        source: String,

        /// Dictionary name (default: zip file stem); the zip is stored as <name>.zip
        #[arg(long)]
        name: Option<String>,

        /// Source language, the terms', e.g. ja-JP / zh / zh-TW / ko-KR (default: index.json
        /// sourceLanguage)
        #[arg(long)]
        lang: Option<String>,

        /// Target language, the definitions', e.g. en / ja (default: index.json
        /// targetLanguage)
        #[arg(long)]
        target_lang: Option<String>,

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

        /// Print the default dictionaries (name, source and target language overrides, download
        /// URL; tab-separated), then exit
        #[arg(long)]
        list: bool,

        /// Set the default dictionaries' source and target languages in dicts.json, for those
        /// already in the library, without downloading anything; then exit
        #[arg(long)]
        langs: bool,
    },

    /// Remove a dictionary from the library
    Remove { name: String },

    /// Build search.db and manifest.json from the library
    Build {
        /// Lindera tokenizer dictionaries (downloaded if missing)
        #[arg(long, env = "CJK_DICT_LINDERA_DIR", default_value = "tmp/lindera")]
        lindera_dir: PathBuf,

        /// Character decomposition data: the CJKVI IDS database's ids.txt (downloaded if
        /// missing; delete it to update)
        #[arg(long, env = "CJK_DICT_IDS_DIR", default_value = "tmp/cjkvi-ids")]
        ids_dir: PathBuf,
    },

    /// Rebuild search.db and manifest.json from scratch
    Rebuild {
        /// Lindera tokenizer dictionaries (downloaded if missing)
        #[arg(long, env = "CJK_DICT_LINDERA_DIR", default_value = "tmp/lindera")]
        lindera_dir: PathBuf,

        /// Character decomposition data: the CJKVI IDS database's ids.txt (downloaded if
        /// missing; delete it to update)
        #[arg(long, env = "CJK_DICT_IDS_DIR", default_value = "tmp/cjkvi-ids")]
        ids_dir: PathBuf,
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
                    .map(|x| x.counts())
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
        Cmd::Size { lang } => size(&lib, lang.as_deref())?,
        Cmd::Add {
            source,
            name,
            lang,
            target_lang,
            replace,
        } => {
            add(&lib, &source, name, lang, target_lang, replace)?;
            eprintln!("run `build` (and `upload`) to publish");
        }
        Cmd::Defaults {
            only,
            replace,
            list,
            langs,
        } => {
            if langs {
                let mut dicts = lib.dicts()?;
                let changed = apply_default_langs(&mut dicts, &only);
                for c in &changed {
                    eprintln!("{c}");
                }
                if changed.is_empty() {
                    eprintln!("dicts.json already has the default languages");
                } else {
                    lib.save_dicts(&dicts)?;
                    eprintln!("run `build` (and `upload`) to publish");
                }
                return Ok(());
            }
            let resolved = defaults::resolve(&only)?;
            if list {
                for d in &resolved {
                    println!(
                        "{}\t{}\t{}\t{}",
                        d.name,
                        d.lang.unwrap_or("-"),
                        d.target_lang.unwrap_or("-"),
                        d.url
                    );
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
                    d.target_lang.map(String::from),
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
        Cmd::Build {
            lindera_dir,
            ids_dir,
        } => build(&lib, &lindera_dir, &ids_dir, false).await?,
        Cmd::Rebuild {
            lindera_dir,
            ids_dir,
        } => build(&lib, &lindera_dir, &ids_dir, true).await?,
        Cmd::Upload { remote, dry_run } => upload(&lib, &remote, dry_run)?,
    }

    Ok(())
}

/// Set the default dictionaries' language overrides on their dicts.json entries (all, or
/// `only`); returns what changed, e.g. `CC-CEDICT: lang zh-CN -> zh`.
fn apply_default_langs(dicts: &mut [DictEntry], only: &[String]) -> Vec<String> {
    let show = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
    let mut changed = vec![];
    for (name, lang, target_lang) in defaults::langs() {
        if !only.is_empty() && !only.iter().any(|n| n == name) {
            continue;
        }
        let Some(d) = dicts.iter_mut().find(|d| d.name == name) else {
            continue;
        };
        for (field, slot, value) in [
            ("lang", &mut d.lang, lang),
            ("target_lang", &mut d.target_lang, target_lang),
        ] {
            let value = value.map(String::from);
            if *slot != value {
                changed.push(format!(
                    "{name}: {field} {} -> {}",
                    show(slot),
                    show(&value)
                ));
                *slot = value;
            }
        }
    }
    changed
}

/// Copy or download a zip into the library as `<name>.zip` and record it in dicts.json.
fn add(
    lib: &Library,
    source: &str,
    name: Option<String>,
    lang: Option<String>,
    target_lang: Option<String>,
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
    // KANJIDIC2 (jmdict-simplified JSON): converted to a Yomitan kanji dictionary
    if kanjidic::is_kanjidic(&tmp) {
        let converted = lib.dir.join(format!(".{name}.zip.converted"));
        let done = kanjidic::convert(&tmp, &converted)
            .and_then(|n| Ok(std::fs::rename(&converted, &tmp).map(|_| n)?));
        match done {
            Ok(n) => eprintln!("converted KANJIDIC2 JSON: {n} characters"),
            Err(e) => {
                let _ = std::fs::remove_file(&converted);
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    let index = YomitanZip::open(&tmp)
        .and_then(|mut z| Ok((z.index()?, z.banks())))
        .with_context(|| format!("{source} is not a Yomitan dictionary zip"))
        .and_then(|(index, banks)| {
            // e.g. a release whose conversion failed, with only index.json and tags
            if !banks
                .iter()
                .any(|b| matches!(b.kind, BankKind::Term | BankKind::Kanji))
            {
                bail!("{source} has no term or kanji entries");
            }
            Ok(index)
        });
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
        target_lang,
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
                "{name}: split term and kanji banks into {} KB ones ({:.1}s)",
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

async fn build(lib: &Library, lindera_dir: &Path, ids_dir: &Path, rebuild: bool) -> Result<()> {
    let dicts = lib.dicts()?;
    let db = lib.dir.join(SEARCH_DB);
    // built before katakana and hiragana were matched alike: its terms can't be updated
    let rebuild = rebuild || {
        let old = search::needs_rebuild(&db)?;
        if old {
            eprintln!("{SEARCH_DB} predates kana folding: rebuilding it from scratch");
        }
        old
    };
    let previous = if !rebuild && db.is_file() {
        lib.manifest()?
    } else {
        None
    };
    let rebuild_from_scratch = rebuild || previous.is_none();
    let mut sources = vec![];
    let mut changed_sources = vec![];
    let mut zip_infos = std::collections::HashMap::new();
    let mut changed_names = vec![];
    for d in &dicts {
        let zip = lib.zip_path(&d.name);
        if !zip.exists() {
            bail!("{} is missing", zip.display());
        }
        let file = file_info(&zip, &format!("{}.zip", d.name))?;
        let old = previous
            .as_ref()
            .and_then(|m| m.dicts.iter().find(|x| x.name == d.name));
        let requested_lang = d.lang.as_deref().and_then(lang_tag);
        let lang_changed = requested_lang
            .as_ref()
            .is_some_and(|lang| old.and_then(|x| x.lang.as_deref()) != Some(lang.as_str()));
        let target_lang_changed = d
            .target_lang
            .as_ref()
            .is_some_and(|lang| old.and_then(|x| x.target_lang.as_deref()) != Some(lang.as_str()));
        let changed = rebuild_from_scratch
            || old.is_none_or(|x| x.file.size != file.size || x.file.sha256 != file.sha256)
            || lang_changed
            || target_lang_changed;
        let source = DictSource {
            name: d.name.clone(),
            zip,
            lang: requested_lang,
            target_lang: d.target_lang.clone(),
        };
        if changed {
            changed_names.push(d.name.clone());
            changed_sources.push(DictSource {
                name: source.name.clone(),
                zip: source.zip.clone(),
                lang: source.lang.clone(),
                target_lang: source.target_lang.clone(),
            });
        }
        sources.push(source);
        zip_infos.insert(d.name.clone(), file);
    }

    // zips added before `add` rebanked them
    for s in &changed_sources {
        rebank(&s.zip, &s.name)?;
        zip_infos.insert(
            s.name.clone(),
            file_info(&s.zip, &format!("{}.zip", s.name))?,
        );
    }

    if !changed_sources.is_empty() {
        lindera::ensure(lindera_dir)?;
    }
    let tok = TokenizerMapper::new(lindera_dir.to_path_buf());

    let t = Instant::now();
    let removed_names = previous
        .as_ref()
        .map(|m| {
            m.dicts
                .iter()
                .filter(|old| !dicts.iter().any(|d| d.name == old.name))
                .map(|old| old.name.clone())
                .chain(changed_names.iter().cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let infos = if rebuild_from_scratch {
        search::build(&db, &sources, &tok, |name, i, n| {
            if n > 0 {
                eprint!("\r{name}: bank {i}/{n}   ");
            } else {
                eprintln!("\r{name}...");
            }
        })
        .await?
    } else {
        search::build_incremental(&db, &changed_sources, &removed_names, &tok, |name, i, n| {
            if n > 0 {
                eprint!("\r{name}: bank {i}/{n}   ");
            } else {
                eprintln!("\r{name}...");
            }
        })
        .await?
    };
    eprintln!(
        "{} {SEARCH_DB} in {:.1}s",
        if rebuild_from_scratch {
            "rebuilt"
        } else {
            "updated"
        },
        t.elapsed().as_secs_f64()
    );

    // character decomposition; how often characters are used follows the dictionaries
    let ids_txt = ids::ensure(ids_dir)?;
    let t = Instant::now();
    let dicts_changed =
        rebuild_from_scratch || !changed_sources.is_empty() || !removed_names.is_empty();
    if search::build_chars(&db, &ids_txt, dicts_changed)? {
        eprintln!("character tables in {:.1}s", t.elapsed().as_secs_f64());
    }
    if search::missing_kanji_readings(&db)? {
        eprintln!(
            "warning: no kanji dictionary with Japanese readings, so `*<興>{{キョウ}}` finds no \
             Japanese words: pnpm admin defaults --only KANJIDIC"
        );
    }

    let indexed: std::collections::HashMap<_, _> = infos
        .into_iter()
        .map(|info| (info.name.clone(), info))
        .collect();
    // from search.db: dictionaries indexed in an earlier build count too
    let char_counts = search::char_counts(&db)?;
    let mut out = vec![];
    for d in &dicts {
        let file = zip_infos.remove(&d.name).expect("zip info was collected");
        let info = indexed.get(&d.name);
        let old = previous
            .as_ref()
            .and_then(|m| m.dicts.iter().find(|x| x.name == d.name));
        let (title, revision, lang, target_lang, term_count) = match (info, old) {
            (Some(info), _) => (
                info.title.clone(),
                info.revision.clone(),
                info.lang.clone(),
                info.target_lang.clone(),
                info.term_count,
            ),
            (None, Some(old)) => (
                old.title.clone(),
                old.revision.clone(),
                old.lang.clone(),
                old.target_lang.clone(),
                old.term_count,
            ),
            (None, None) => bail!("missing build information for {}", d.name),
        };
        out.push(ManifestDict {
            name: d.name.clone(),
            title,
            revision,
            lang,
            target_lang,
            term_count,
            char_count: char_counts.get(&d.name).copied().unwrap_or(0),
            file,
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
            "  {}: {}, {:.1} MB",
            d.name,
            d.counts(),
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

fn size(lib: &Library, lang: Option<&str>) -> Result<()> {
    let manifest = lib.check_fresh()?;
    let lang_filter = lang
        .map(|lang| size_group(lang).with_context(|| format!("unsupported language {lang}")))
        .transpose()?;
    let zip_sizes = zip_size_by_lang(&manifest.dicts, lang_filter.as_deref());

    println!("Language\tDictionaries\tZIP size");
    for (lang, (count, bytes)) in &zip_sizes {
        println!("{lang}\t{count}\t{:.1} MB", *bytes as f64 / 1e6);
    }
    let zip_total: u64 = zip_sizes.values().map(|(_, size)| size).sum();
    let dict_count: usize = zip_sizes.values().map(|(count, _)| count).sum();
    println!("ZIP total\t{dict_count}\t{:.1} MB", zip_total as f64 / 1e6);

    if lang_filter.is_some() {
        println!("(ZIP total is filtered; shared files and upload remain whole-library.)");
    } else {
        let dicts_path = lib.dir.join(DICTS_JSON);
        let dicts_size = match std::fs::metadata(dicts_path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        let shared_files = [
            (SEARCH_DB, std::fs::metadata(lib.dir.join(SEARCH_DB))?.len()),
            (DICTS_JSON, dicts_size),
            (MANIFEST, std::fs::metadata(lib.dir.join(MANIFEST))?.len()),
        ];
        let shared_total: u64 = shared_files.iter().map(|(_, size)| size).sum();
        for (file, bytes) in shared_files {
            println!("{file}\t\t{:.1} MB", bytes as f64 / 1e6);
        }
        println!(
            "R2 mirror total\t\t{:.1} MB",
            (zip_total + shared_total) as f64 / 1e6
        );
    }
    Ok(())
}

/// The language `size` groups a dictionary under: `ja-JP`, `ko-KR`, or `zh` for any Chinese
fn size_group(lang: &str) -> Option<String> {
    lang_tag(lang.split(['-', '_']).next()?)
}

fn zip_size_by_lang(
    dicts: &[ManifestDict],
    lang_filter: Option<&str>,
) -> BTreeMap<String, (usize, u64)> {
    let mut sizes = BTreeMap::new();
    for dict in dicts {
        let lang = dict
            .lang
            .as_deref()
            .map(|lang| size_group(lang).unwrap_or_else(|| lang.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        if lang_filter.is_some_and(|filter| filter != lang) {
            continue;
        }
        let (count, size) = sizes.entry(lang).or_insert((0, 0));
        *count += 1;
        *size += dict.file.size;
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_langs_are_set_without_downloading() {
        let entry = |name: &str, lang: Option<&str>| DictEntry {
            name: name.into(),
            lang: lang.map(String::from),
            target_lang: None,
            source: None,
        };
        let mut dicts = vec![
            entry("CC-CEDICT", Some("zh-CN")),
            entry("Pixiv", None),
            entry("wty-ja-en", Some("ja-JP")),
            entry("smk8", Some("ja-JP")),
        ];
        assert_eq!(
            apply_default_langs(&mut dicts, &[]),
            [
                "Pixiv: target_lang - -> ja",
                "CC-CEDICT: lang zh-CN -> zh",
                "CC-CEDICT: target_lang - -> en",
            ]
        );
        // not a default: left alone
        assert_eq!(dicts[3].target_lang, None);
        assert!(apply_default_langs(&mut dicts, &[]).is_empty());
    }
    use crate::library::FileInfo;

    fn dict(name: &str, lang: Option<&str>, size: u64) -> ManifestDict {
        ManifestDict {
            name: name.into(),
            title: name.into(),
            revision: "1".into(),
            lang: lang.map(String::from),
            target_lang: None,
            term_count: 0,
            char_count: 0,
            file: FileInfo {
                file: format!("{name}.zip"),
                size,
                sha256: String::new(),
            },
        }
    }

    #[test]
    fn counts_say_what_a_dictionary_has() {
        let mut d = dict("x", None, 0);
        d.term_count = 528000;
        assert_eq!(d.counts(), "528000 terms");
        d.char_count = 10384;
        assert_eq!(d.counts(), "528000 terms, 10384 characters");
        d.term_count = 0;
        assert_eq!(d.counts(), "10384 characters");
        // manifests from before char_count
        let old: ManifestDict = serde_json::from_str(
            r#"{"name":"x","title":"x","revision":"1","lang":null,"term_count":5,"file":"x.zip","size":1,"sha256":""}"#,
        )
        .unwrap();
        assert_eq!(old.char_count, 0);
        assert!(!serde_json::to_string(&old).unwrap().contains("char_count"));
    }

    #[test]
    fn zip_sizes_are_grouped_and_filterable_by_language() {
        let dicts = [
            dict("ja-one", Some("ja"), 10),
            dict("ja-two", Some("ja-JP"), 20),
            dict("zh", Some("zh-CN"), 30),
            dict("zh-tw", Some("zh-TW"), 5),
            dict("unknown", None, 40),
        ];

        assert_eq!(
            zip_size_by_lang(&dicts, None),
            BTreeMap::from([
                ("ja-JP".into(), (2, 30)),
                ("unknown".into(), (1, 40)),
                ("zh".into(), (2, 35)),
            ])
        );
        assert_eq!(
            zip_size_by_lang(&dicts, Some("ja-JP")),
            BTreeMap::from([("ja-JP".into(), (2, 30))])
        );
        assert!(zip_size_by_lang(&dicts, Some("ko-KR")).is_empty());
    }
}
