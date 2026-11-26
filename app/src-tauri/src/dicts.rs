//! Dictionaries: sync the library (Yomitan zips + search.db.zst + manifest.json) from R2,
//! search it, and serve media from the zips.
//!
//! Files land in `<app data>/dicts/incoming/` (via the `download_url` command, resumable).
//! Once all of them are there, `dict_commit` checks them, unpacks search.db.zst, and moves
//! everything into `<app data>/dicts/`, with the open database closed first (Windows can't
//! replace open files).

use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tauri_plugin_http::reqwest;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use yomitan_parser::char_reading::Latin;
use yomitan_parser::search::{
    DictInfo, HanChar, HanList, SEARCH_DB_COMPAT, SearchDb, SearchRequest, SearchResult, parse_lang,
};

use crate::error::AppError;

const MANIFEST: &str = "manifest.json";
const SEARCH_DB: &str = "search.db";
const INCOMING: &str = "incoming";
const PENDING: &str = "manifest.pending.json";

/// Build-time configuration (`option_env!`), overridable at runtime by the same env var.
macro_rules! config {
    ($key:literal) => {
        std::env::var($key)
            .ok()
            .or_else(|| option_env!($key).map(String::from))
            .filter(|s| !s.is_empty())
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileInfo {
    file: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestDict {
    name: String,
    #[serde(flatten)]
    file: FileInfo,
}

/// A compressed file, and what it unpacks to
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackedFile {
    #[serde(flatten)]
    file: FileInfo,
    unpacked_size: u64,
    unpacked_sha256: String,
    /// the unpacked search.db's compatibility level; absent in manifests from before: 1
    #[serde(default = "compat_1")]
    compat: u32,
}

fn compat_1() -> u32 {
    1
}

impl PackedFile {
    /// Whether this app can read the search.db: one of a higher level needs a newer app
    fn readable(&self) -> bool {
        self.compat <= SEARCH_DB_COMPAT
    }
}

/// dict-admin's `manifest.json`. Version 2 lists `search_db_zst`; version 1, `search_db`,
/// uncompressed. A remote one must be version 2; the committed one may be version 1, from
/// before, which still vouches for the files in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_db: Option<FileInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_db_zst: Option<PackedFile>,
    dicts: Vec<ManifestDict>,
}

impl Manifest {
    fn zips(&self) -> impl Iterator<Item = &FileInfo> {
        self.dicts.iter().map(|d| &d.file)
    }

    /// search.db's SHA-256, unpacked
    fn search_db_sha256(&self) -> Option<&str> {
        self.search_db_zst
            .as_ref()
            .map(|z| z.unpacked_sha256.as_str())
            .or(self.search_db.as_ref().map(|f| f.sha256.as_str()))
    }

    fn zst(&self) -> Result<&PackedFile, AppError> {
        self.search_db_zst.as_ref().ok_or_else(|| {
            AppError::Error(
                "the library is from before search.db.zst: build and upload it again with                  dict-admin"
                    .into(),
            )
        })
    }
}

/// Where the library is downloaded from.
enum Remote {
    /// private R2 bucket: presigned GET URLs from a read-only API token
    R2 {
        bucket: rusty_s3::Bucket,
        creds: rusty_s3::Credentials,
        prefix: String,
    },
    /// plain HTTP, e.g. `rclone serve http library` during development
    Http(String),
}

impl Remote {
    fn from_env() -> Result<Self, AppError> {
        if let Some(base) = config!("CJK_DICT_BASE_URL") {
            return Ok(Self::Http(base.trim_end_matches('/').to_string()));
        }
        let (Some(account), Some(key), Some(secret), Some(bucket)) = (
            config!("CJK_DICT_R2_ACCOUNT_ID"),
            config!("CJK_DICT_R2_ACCESS_KEY_ID"),
            config!("CJK_DICT_R2_SECRET_ACCESS_KEY"),
            config!("CJK_DICT_R2_BUCKET"),
        ) else {
            return Err(AppError::Error(
                "no dictionary source configured: build with CJK_DICT_R2_* (or \
                 CJK_DICT_BASE_URL), or push the library \
                 to the app (pnpm desktop:push or pnpm android:push)"
                    .into(),
            ));
        };
        let endpoint = url::Url::parse(&format!("https://{account}.r2.cloudflarestorage.com"))
            .map_err(|e| AppError::Error(e.to_string()))?;
        let bucket = rusty_s3::Bucket::new(endpoint, rusty_s3::UrlStyle::Path, bucket, "auto")
            .map_err(|e| AppError::Error(e.to_string()))?;
        let prefix = config!("CJK_DICT_R2_PREFIX")
            .map(|p| format!("{}/", p.trim_matches('/')))
            .unwrap_or_default();
        Ok(Self::R2 {
            bucket,
            creds: rusty_s3::Credentials::new(key, secret),
            prefix,
        })
    }

    fn url(&self, file: &str) -> String {
        match self {
            Self::Http(base) => format!("{base}/{file}"),
            Self::R2 {
                bucket,
                creds,
                prefix,
            } => {
                use rusty_s3::S3Action;
                bucket
                    .get_object(Some(creds), &format!("{prefix}{file}"))
                    .sign(Duration::from_secs(24 * 3600))
                    .to_string()
            }
        }
    }
}

/// A file's size and modification time: while both are unchanged, a checksum computed
/// earlier in this run still holds.
#[derive(Clone, PartialEq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

pub struct DictState {
    dir: PathBuf,
    db: Mutex<Option<Arc<SearchDb>>>,
    /// path -> (stamp, sha256) of files hashed this run, so that `commit` right after
    /// `dict_plan` doesn't hash the whole library (gigabytes) a second time
    hashed: Mutex<HashMap<PathBuf, (Stamp, String)>>,
}

impl DictState {
    pub fn new(app_data_dir: &Path) -> Self {
        let s = Self {
            dir: app_data_dir.join("dicts"),
            db: Mutex::new(None),
            hashed: Mutex::new(HashMap::new()),
        };
        if let Err(e) = s.reopen() {
            eprintln!("dictionaries not opened: {e}");
        }
        s
    }

    /// Drop the open database and wait for in-flight searches to let go of it:
    /// Windows can't replace search.db or a zip while it's open.
    fn close(&self) -> Result<(), AppError> {
        let Some(mut db) = self.db.lock()?.take() else {
            return Ok(());
        };
        for _ in 0..250 {
            match Arc::try_unwrap(db) {
                Ok(db) => {
                    drop(db);
                    return Ok(());
                }
                Err(shared) => {
                    db = shared;
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        *self.db.lock()? = Some(db);
        Err(AppError::Error(
            "dictionaries are still in use, try again later".into(),
        ))
    }

    fn reopen(&self) -> Result<(), AppError> {
        let mut slot = self.db.lock()?;
        *slot = None;
        let db = self.dir.join(SEARCH_DB);
        if db.exists() && self.dir.join(MANIFEST).exists() {
            *slot = Some(Arc::new(SearchDb::open(&db, &self.dir)?));
        }
        Ok(())
    }

    fn get(&self) -> Result<Arc<SearchDb>, AppError> {
        self.db
            .lock()?
            .clone()
            .ok_or_else(|| AppError::Error("no dictionaries downloaded yet".into()))
    }

    fn local_manifest(&self) -> Option<Manifest> {
        let s = std::fs::read_to_string(self.dir.join(MANIFEST)).ok()?;
        serde_json::from_str(&s).ok()
    }

    /// file -> sha256, for files in place that the committed manifest vouches for:
    /// the zips, and search.db (unpacked)
    fn committed(&self) -> HashMap<String, String> {
        let Some(m) = self.local_manifest() else {
            return HashMap::new();
        };
        let mut out: HashMap<String, String> = m
            .zips()
            .filter(|f| self.dir.join(&f.file).exists())
            .map(|f| (f.file.clone(), f.sha256.clone()))
            .collect();
        if let Some(sum) = m.search_db_sha256() {
            if self.dir.join(SEARCH_DB).exists() {
                out.insert(SEARCH_DB.into(), sum.into());
            }
        }
        out
    }

    /// `path`'s SHA-256, reusing this run's checksum if the file is unchanged since.
    /// Reports progress as `dict-verify-progress` events: hashing the library takes
    /// minutes on a phone.
    fn sha256(&self, app: &AppHandle, path: &Path) -> Result<String, AppError> {
        let stamp = |m: &std::fs::Metadata| Stamp {
            len: m.len(),
            modified: m.modified().ok(),
        };
        let before = stamp(&std::fs::metadata(path)?);
        if let Some((s, sum)) = self.hashed.lock()?.get(path) {
            if *s == before {
                return Ok(sum.clone());
            }
        }

        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let sum = sha256_file(path, |hashed| {
            let _ = app.emit(
                "dict-verify-progress",
                VerifyProgress {
                    file: &file,
                    hashed,
                    size: before.len,
                    unpacking: false,
                },
            );
        })?;
        // only trust the checksum if the file didn't change while it was read
        if stamp(&std::fs::metadata(path)?) == before {
            self.hashed
                .lock()?
                .insert(path.to_path_buf(), (before, sum.clone()));
        }
        Ok(sum)
    }
}

/// Whether `path` is a file of `f`'s size. `dict_plan` checks no more: checksums take
/// minutes on a phone, so `dict_commit` checks them, once every download is done.
fn has_size(path: &Path, f: &FileInfo) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == f.size)
}

#[derive(Clone, Serialize)]
struct VerifyProgress<'a> {
    file: &'a str,
    /// bytes hashed so far (unpacked, when `unpacking`)
    hashed: u64,
    size: u64,
    /// unpacking search.db rather than checking a file
    unpacking: bool,
}

/// Unpack `zst` into `out`, returning the SHA-256 of what was written, and calling
/// `report` with the bytes written so far every 16 MiB and at the end.
fn unpack(zst: &Path, out: &Path, mut report: impl FnMut(u64)) -> Result<String, AppError> {
    const CHUNK: usize = 1 << 20;
    const REPORT_EVERY: u64 = 16 << 20;
    let mut dec = zstd::Decoder::new(File::open(zst)?)?;
    let mut w = BufWriter::new(File::create(out)?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut written = 0u64;
    loop {
        let n = dec.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
        h.update(&buf[..n]);
        let before = written;
        written += n as u64;
        if before / REPORT_EVERY != written / REPORT_EVERY {
            report(written);
        }
    }
    w.flush()?;
    report(written);
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// SHA-256 of a file, calling `progress` with the bytes hashed so far every 16 MiB and
/// at the end.
fn sha256_file(path: &Path, mut progress: impl FnMut(u64)) -> Result<String, AppError> {
    const CHUNK: usize = 1 << 20;
    const REPORT_EVERY: u64 = 16 << 20;
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut hashed = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        let before = hashed;
        hashed += n as u64;
        if before / REPORT_EVERY != hashed / REPORT_EVERY {
            progress(hashed);
        }
    }
    progress(hashed);
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Serialize)]
pub struct Plan {
    /// what to download; empty = up to date, or `needs_newer_app`
    files: Vec<PlannedFile>,
    /// The remote library's search.db needs a newer app (its compatibility level is
    /// higher than this app reads): the app keeps the library it has, and downloads none
    /// of it, since the zips and search.db must match.
    needs_newer_app: bool,
}

#[derive(Serialize)]
pub struct PlannedFile {
    /// relative to app data dir, for the `download_url` command
    filename: String,
    url: String,
    size: u64,
}

/// Fetch the remote manifest and list the files that need downloading.
/// Empty = up to date. Remembers the manifest until `dict_commit`.
///
/// Without a remote (none configured, or its manifest can't be fetched), it plans from a
/// manifest pushed to `incoming/` with the library's files (`scripts/push-library.mjs`):
/// then every file must be on the device already, as nothing can be downloaded.
#[tauri::command]
pub async fn dict_plan(app: AppHandle) -> Result<Plan, AppError> {
    let dir = app.state::<DictState>().dir.clone();
    let (remote, text) = match Remote::from_env() {
        Ok(remote) => match fetch_manifest(&remote).await {
            Ok(text) => {
                // the remote's is the library now
                let _ = std::fs::remove_file(dir.join(INCOMING).join(MANIFEST));
                (Some(remote), text)
            }
            Err(e) => (None, manifest_without_remote(&dir, e, true)?),
        },
        Err(e) => (None, manifest_without_remote(&dir, e, false)?),
    };

    // reading the library and committing is blocking work
    tauri::async_runtime::spawn_blocking(move || {
        plan(&app.state::<DictState>(), &text, remote.as_ref())
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
}

/// The manifest to plan from when the remote's can't be had (`e` says why): one pushed to
/// `incoming/`, else, with no remote `configured`, the committed one, so that a library
/// pushed and committed before is checked rather than every launch failing for want of a
/// remote. A configured remote that can't be reached stays an error.
fn manifest_without_remote(dir: &Path, e: AppError, configured: bool) -> Result<String, AppError> {
    if let Ok(text) = std::fs::read_to_string(dir.join(INCOMING).join(MANIFEST)) {
        return Ok(text);
    }
    if !configured {
        if let Ok(text) = std::fs::read_to_string(dir.join(MANIFEST)) {
            return Ok(text);
        }
    }
    Err(e)
}

async fn fetch_manifest(remote: &Remote) -> Result<String, AppError> {
    let res = reqwest::get(remote.url(MANIFEST)).await?;
    if !res.status().is_success() {
        return Err(AppError::Error(format!(
            "manifest.json: HTTP {}",
            res.status()
        )));
    }
    Ok(res.text().await?)
}

/// What to download for the library of manifest `text`, from `remote`; with none, only
/// what's already on the device can be used, and a missing file is an error.
fn plan(state: &DictState, text: &str, remote: Option<&Remote>) -> Result<Plan, AppError> {
    let manifest: Manifest = serde_json::from_str(text)?;
    let zst = manifest.zst()?;
    if !zst.readable() {
        // nor a plan from before, whose files R2 may no longer have
        let _ = std::fs::remove_file(state.dir.join(PENDING));
        return Ok(Plan {
            files: vec![],
            needs_newer_app: true,
        });
    }
    let committed = state.committed();
    let incoming = state.dir.join(INCOMING);
    std::fs::create_dir_all(&incoming)?;

    let mut wanted: Vec<&FileInfo> = vec![];
    for f in manifest.zips() {
        if committed.get(&f.file) == Some(&f.sha256) {
            continue;
        }
        // moved into place by a commit that failed partway, before the manifest
        if has_size(&state.dir.join(&f.file), f) {
            continue;
        }
        wanted.push(f);
    }
    if committed.get(SEARCH_DB) != Some(&zst.unpacked_sha256) {
        wanted.push(&zst.file);
    }

    // files the committed manifest doesn't vouch for: downloaded, or waiting in incoming/
    let all_in_place = wanted.is_empty();
    let mut out = vec![];
    let mut missing = vec![];
    for f in wanted {
        // Downloaded by an earlier run that was closed before the rest finished (or
        // whose commit failed): nothing is committed until every file is there.
        let new = incoming.join(&f.file);
        if new.exists() {
            if has_size(&new, f) {
                continue;
            }
            let _ = std::fs::remove_file(&new);
        }
        match remote {
            Some(remote) => out.push(PlannedFile {
                filename: format!("dicts/{INCOMING}/{}", f.file),
                url: remote.url(&f.file),
                size: f.size,
            }),
            None => missing.push(f.file.as_str()),
        }
    }
    if !missing.is_empty() {
        let _ = std::fs::remove_file(state.dir.join(PENDING));
        return Err(AppError::Error(format!(
            "{}: not on the device, and no dictionary source to download them from: push \
             the library again (pnpm desktop:push or pnpm android:push)",
            missing.join(", ")
        )));
    }

    let unchanged = std::fs::read_to_string(state.dir.join(MANIFEST))
        .ok()
        .as_deref()
        == Some(text);
    // up to date only with everything in place: files waiting in incoming/ (e.g. pushed
    // by scripts/push-library.mjs) are committed, even under the same manifest
    if unchanged && all_in_place {
        let _ = std::fs::remove_file(state.dir.join(PENDING));
        return Ok(Plan {
            files: out,
            needs_newer_app: false,
        });
    }
    // `dict_commit` once these are downloaded, and every other download (the page
    // waits for all of them); right away if there are none, e.g. when dictionaries
    // were only removed
    std::fs::write(state.dir.join(PENDING), text)?;
    Ok(Plan {
        files: out,
        needs_newer_app: false,
    })
}

#[derive(Serialize)]
pub struct CommitResult {
    /// whether a plan was committed: the dictionaries changed
    committed: bool,
    /// Files that failed their checksum, now deleted: nothing was committed. `dict_plan`
    /// lists them to download again; then commit again.
    corrupt: Vec<String>,
}

/// Verify downloaded files, unpack search.db.zst, move them into place, drop removed
/// dictionaries, reopen. Nothing to do without a plan (`dict_plan`) to commit.
#[tauri::command]
pub async fn dict_commit(app: AppHandle) -> Result<CommitResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DictState>();
        let committed = state.dir.join(PENDING).exists();
        let corrupt = commit(&app, &state)?;
        Ok(CommitResult {
            committed: committed && corrupt.is_empty(),
            corrupt,
        })
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
}

/// The files that failed their checksum, if any: then nothing was committed.
fn commit(app: &AppHandle, state: &DictState) -> Result<Vec<String>, AppError> {
    let pending = state.dir.join(PENDING);
    let Ok(text) = std::fs::read_to_string(&pending) else {
        return Ok(vec![]);
    };
    let m: Manifest = serde_json::from_str(&text)?;
    let zst = m.zst()?;
    let incoming = state.dir.join(INCOMING);
    let committed = state.committed();

    // everything must be present and intact before anything is replaced: check every file,
    // so that all the corrupt ones download again at once
    let mut corrupt = vec![];
    let mut check = |path: PathBuf, f: &FileInfo| -> Result<(), AppError> {
        if !path.exists() {
            return Err(AppError::Error(format!("{} was not downloaded", f.file)));
        }
        if state.sha256(app, &path)? != f.sha256 {
            let _ = std::fs::remove_file(&path);
            corrupt.push(f.file.clone());
        }
        Ok(())
    };
    for f in m.zips() {
        let new = incoming.join(&f.file);
        if new.exists() {
            check(new, f)?;
        } else if committed.get(&f.file) != Some(&f.sha256) {
            check(state.dir.join(&f.file), f)?;
        }
        // else in place and already verified by an earlier commit
    }
    let db_current = committed.get(SEARCH_DB) == Some(&zst.unpacked_sha256);
    let zst_path = incoming.join(&zst.file.file);
    if !db_current {
        check(zst_path.clone(), &zst.file)?;
    }
    if !corrupt.is_empty() {
        return Ok(corrupt);
    }

    // the new search.db, next to the files downloaded, while the one in use stays open
    let new_db = incoming.join(SEARCH_DB);
    if !db_current {
        let sum = unpack(&zst_path, &new_db, |written| {
            let _ = app.emit(
                "dict-verify-progress",
                VerifyProgress {
                    file: SEARCH_DB,
                    hashed: written,
                    size: zst.unpacked_size,
                    unpacking: true,
                },
            );
        })?;
        if sum != zst.unpacked_sha256 {
            let _ = std::fs::remove_file(&new_db);
            let _ = std::fs::remove_file(&zst_path);
            return Ok(vec![zst.file.file.clone()]);
        }
    }

    // close search.db and the zips before replacing them
    state.close()?;
    for f in m.zips() {
        let new = incoming.join(&f.file);
        if new.exists() {
            std::fs::rename(&new, state.dir.join(&f.file))?;
        }
    }
    if !db_current {
        std::fs::rename(&new_db, state.dir.join(SEARCH_DB))?;
    }
    let keep: Vec<&str> = m.zips().map(|f| f.file.as_str()).collect();
    for e in std::fs::read_dir(&state.dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".zip") && !keep.contains(&name.as_str()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    std::fs::rename(&pending, state.dir.join(MANIFEST))?;
    // a pushed manifest, now committed
    let pushed = incoming.join(MANIFEST);
    if std::fs::read_to_string(&pushed).is_ok_and(|p| p == text) {
        let _ = std::fs::remove_file(&pushed);
    }
    // unpacked and in place: the manifest vouches for search.db from now on
    let _ = std::fs::remove_file(&zst_path);
    // the files moved: `committed()` vouches for them from now on
    state.hashed.lock()?.clear();
    state.reopen()?;
    Ok(vec![])
}

#[tauri::command]
pub async fn search(
    state: State<'_, DictState>,
    req: SearchRequest,
) -> Result<SearchResult, AppError> {
    let db = state.get()?;
    tauri::async_runtime::spawn_blocking(move || db.search(&req))
        .await
        .map_err(|e| AppError::Error(e.to_string()))?
        .map_err(AppError::from)
}

/// The dictionaries in search.db, in build order, e.g. to choose which to search.
#[tauri::command]
pub async fn dicts(state: State<'_, DictState>) -> Result<Vec<DictInfo>, AppError> {
    let db = state.get()?;
    tauri::async_runtime::spawn_blocking(move || db.dicts())
        .await
        .map_err(|e| AppError::Error(e.to_string()))?
        .map_err(AppError::from)
}

/// The Han characters of `text`: how each breaks down, and its kanji dictionary entries;
/// how many terms use each, of `lang` (the search language, e.g. `ja-JP`), or with none, all.
#[tauri::command]
pub async fn han_chars(
    state: State<'_, DictState>,
    text: String,
    lang: Option<String>,
) -> Result<Vec<HanChar>, AppError> {
    let db = state.get()?;
    let lang = lang.as_deref().and_then(parse_lang);
    tauri::async_runtime::spawn_blocking(move || db.han_chars(&text, lang))
        .await
        .map_err(|e| AppError::Error(e.to_string()))?
        .map_err(AppError::from)
}

/// Characters made of all the characters of `components`, the most used first, in the
/// terms of `lang` or with none, all. With `direct`, only those whose IDS name the
/// components themselves.
#[tauri::command]
pub async fn han_containing(
    state: State<'_, DictState>,
    components: String,
    limit: Option<u32>,
    offset: Option<u32>,
    direct: Option<bool>,
    lang: Option<String>,
) -> Result<HanList, AppError> {
    let db = state.get()?;
    let direct = direct.unwrap_or(false);
    let lang = lang.as_deref().and_then(parse_lang);
    tauri::async_runtime::spawn_blocking(move || {
        db.han_containing(&components, limit, offset, direct, lang)
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
    .map_err(AppError::from)
}

/// Characters read any of `readings` (kana, pinyin or Hangul), the most used first, in the
/// terms of `lang` or with none, all. Latin letters are pinyin (`pinyin`, the default),
/// Korean romanization (`korean`), or both.
#[tauri::command]
pub async fn han_by_reading(
    state: State<'_, DictState>,
    readings: Vec<String>,
    pinyin: Option<bool>,
    korean: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
    lang: Option<String>,
) -> Result<HanList, AppError> {
    let db = state.get()?;
    let latin = Latin {
        pinyin: pinyin.unwrap_or(true),
        korean: korean.unwrap_or(false),
    };
    let lang = lang.as_deref().and_then(parse_lang);
    tauri::async_runtime::spawn_blocking(move || {
        db.han_by_reading(&readings, latin, limit, offset, lang)
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
    .map_err(AppError::from)
}

/// A dictionary's `styles.css`, if its zip has one. The view scopes it to that
/// dictionary's entries, as Yomitan does.
#[tauri::command]
pub async fn dict_styles(
    state: State<'_, DictState>,
    dict: String,
) -> Result<Option<String>, AppError> {
    let db = state.get()?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(db
            .asset(&dict, "styles.css")?
            .map(|b| String::from_utf8_lossy(&b).into_owned()))
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
}

/// `dict://localhost/<dict>/<path inside zip>` -> the file from the dict's zip.
pub fn asset_protocol(
    app: &AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Response, StatusCode};

    let path = request.uri().path().trim_start_matches('/');
    let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap()
    };
    let Some((dict, file)) = path.split_once('/') else {
        return not_found();
    };
    let Ok(db) = app.state::<DictState>().get() else {
        return not_found();
    };
    match db.asset(dict, file) {
        Ok(Some(bytes)) => {
            let ext = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "avif" => "image/avif",
                "svg" => "image/svg+xml",
                "bmp" => "image/bmp",
                "css" => "text/css",
                "mp3" => "audio/mpeg",
                "ogg" => "audio/ogg",
                "wav" => "audio/wav",
                _ => "application/octet-stream",
            };
            Response::builder()
                .header("Content-Type", mime)
                .header("Cache-Control", "max-age=31536000, immutable")
                // the page (tauri.localhost) loads these cross-origin, and CSS masks
                // (monochrome images) are fetched with CORS: without this they don't show
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes)
                .unwrap()
        }
        _ => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_file_reports_progress() {
        let path = std::env::temp_dir().join(format!("cjk-dict-sha-{}", std::process::id()));
        // 40 MiB: reports at 16 and 32 MiB, then at the end
        std::fs::write(&path, vec![0u8; 40 << 20]).unwrap();
        let mut reports = vec![];
        let sum = sha256_file(&path, |n| reports.push(n)).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(reports, [16 << 20, 32 << 20, 40 << 20]);
        let expected: String = Sha256::digest(vec![0u8; 40 << 20])
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(sum, expected);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cjk-dict-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unpack_writes_what_was_packed_and_its_checksum() {
        let dir = temp_dir("unpack");
        // 20 MiB that compress
        let data: Vec<u8> = (0..20u32 << 20).map(|i| (i % 251) as u8).collect();
        let zst = dir.join("search.db.zst");
        std::fs::write(&zst, zstd::encode_all(&data[..], 3).unwrap()).unwrap();

        let out = dir.join("search.db");
        let mut reports = vec![];
        let sum = unpack(&zst, &out, |n| reports.push(n)).unwrap();

        assert_eq!(std::fs::read(&out).unwrap(), data);
        // reads end where the decoder's do: once past 16 MiB, then at the end
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert!(reports[0] >= 16 << 20 && reports[0] < 20 << 20);
        assert_eq!(reports[1], 20 << 20);
        let expected: String = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(sum, expected);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unpack_fails_on_a_corrupt_file() {
        let dir = temp_dir("unpack-corrupt");
        let zst = dir.join("search.db.zst");
        std::fs::write(&zst, b"not zstd").unwrap();
        assert!(unpack(&zst, &dir.join("search.db"), |_| {}).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    const V1: &str = r#"{"version":1,
        "search_db":{"file":"search.db","size":5,"sha256":"db1"},
        "dicts":[{"name":"A","title":"A","revision":"1","lang":null,"term_count":1,
                  "file":"A.zip","size":1,"sha256":"a"}]}"#;
    const V2: &str = r#"{"version":2,
        "search_db_zst":{"file":"search.db.zst","size":2,"sha256":"z",
                         "unpacked_size":5,"unpacked_sha256":"db2"},
        "dicts":[{"name":"A","title":"A","revision":"1","lang":null,"term_count":1,
                  "file":"A.zip","size":1,"sha256":"a"}]}"#;

    #[test]
    fn manifests_of_either_version_say_what_search_db_is() {
        let v1: Manifest = serde_json::from_str(V1).unwrap();
        assert_eq!(v1.search_db_sha256(), Some("db1"));
        assert!(v1.zst().is_err(), "a remote version 1 manifest is refused");

        let v2: Manifest = serde_json::from_str(V2).unwrap();
        assert_eq!(v2.search_db_sha256(), Some("db2"));
        assert_eq!(v2.zst().unwrap().file.file, "search.db.zst");
        // from before compatibility levels: level 1, which every app reads
        assert_eq!(v2.zst().unwrap().compat, 1);
        assert!(v2.zst().unwrap().readable());
        assert_eq!(
            v2.zips().map(|f| f.file.as_str()).collect::<Vec<_>>(),
            ["A.zip"]
        );
    }

    #[test]
    fn a_search_db_of_a_higher_level_needs_a_newer_app() {
        let at = |compat: u32| {
            let m: Manifest = serde_json::from_str(&V2.replace(
                r#""unpacked_sha256":"db2""#,
                &format!(r#""unpacked_sha256":"db2","compat":{compat}"#),
            ))
            .unwrap();
            m.zst().unwrap().readable()
        };
        assert!(at(SEARCH_DB_COMPAT));
        assert!(at(SEARCH_DB_COMPAT - 1));
        assert!(!at(SEARCH_DB_COMPAT + 1));
    }

    #[test]
    fn without_a_remote_it_plans_from_the_files_on_the_device() {
        let app_data = temp_dir("offline");
        let state = DictState::new(&app_data);
        let incoming = state.dir.join(INCOMING);
        std::fs::create_dir_all(&incoming).unwrap();

        // nothing pushed: an error naming what's missing, and nothing to commit
        let e = plan(&state, V2, None).err().expect("files are missing");
        assert!(e.to_string().contains("A.zip, search.db.zst"), "{e}");
        assert!(!state.dir.join(PENDING).exists());

        // all pushed, of the manifest's sizes: committed as they are
        std::fs::write(incoming.join("A.zip"), b"a").unwrap();
        std::fs::write(incoming.join("search.db.zst"), b"zz").unwrap();
        let p = plan(&state, V2, None).unwrap();
        assert!(p.files.is_empty() && !p.needs_newer_app);
        assert_eq!(
            std::fs::read_to_string(state.dir.join(PENDING)).unwrap(),
            V2
        );

        // one of another size: deleted, and missing
        std::fs::write(incoming.join("A.zip"), b"ab").unwrap();
        assert!(plan(&state, V2, None).is_err());
        assert!(!incoming.join("A.zip").exists());
        assert!(!state.dir.join(PENDING).exists());
        std::fs::remove_dir_all(&app_data).unwrap();
    }

    #[test]
    fn without_a_remote_a_committed_library_is_up_to_date() {
        let app_data = temp_dir("offline-committed");
        let state = DictState::new(&app_data);
        let no_remote = || AppError::Error("no remote".into());

        // nothing pushed, nothing committed: the error stands
        std::fs::create_dir_all(state.dir.join(INCOMING)).unwrap();
        assert!(manifest_without_remote(&state.dir, no_remote(), false).is_err());

        // committed (and its pushed manifest deleted): planned from, and nothing to do
        std::fs::write(state.dir.join(MANIFEST), V2).unwrap();
        std::fs::write(state.dir.join("A.zip"), b"a").unwrap();
        std::fs::write(state.dir.join(SEARCH_DB), b"12345").unwrap();
        let text = manifest_without_remote(&state.dir, no_remote(), false).unwrap();
        assert_eq!(text, V2);
        let p = plan(&state, &text, None).unwrap();
        assert!(p.files.is_empty() && !p.needs_newer_app);
        assert!(!state.dir.join(PENDING).exists());

        // a remote configured but unreachable is still an error
        assert!(manifest_without_remote(&state.dir, no_remote(), true).is_err());

        // a pushed manifest comes first
        std::fs::write(state.dir.join(INCOMING).join(MANIFEST), V1).unwrap();
        assert_eq!(
            manifest_without_remote(&state.dir, no_remote(), true).unwrap(),
            V1
        );
        std::fs::remove_dir_all(&app_data).unwrap();
    }

    #[test]
    fn with_a_remote_it_downloads_what_is_missing() {
        let app_data = temp_dir("online");
        let state = DictState::new(&app_data);
        let remote = Remote::Http("http://127.0.0.1:8080".into());
        let p = plan(&state, V2, Some(&remote)).unwrap();
        assert_eq!(
            p.files.iter().map(|f| f.url.as_str()).collect::<Vec<_>>(),
            [
                "http://127.0.0.1:8080/A.zip",
                "http://127.0.0.1:8080/search.db.zst"
            ]
        );
        assert!(state.dir.join(PENDING).exists());
        std::fs::remove_dir_all(&app_data).unwrap();
    }

    #[test]
    fn a_version_1_manifest_still_vouches_for_the_files_in_place() {
        let app_data = temp_dir("committed");
        let state = DictState::new(&app_data);
        std::fs::create_dir_all(&state.dir).unwrap();
        std::fs::write(state.dir.join(MANIFEST), V1).unwrap();
        // no files in place: nothing is vouched for
        assert!(state.committed().is_empty());

        std::fs::write(state.dir.join("A.zip"), b"a").unwrap();
        std::fs::write(state.dir.join(SEARCH_DB), b"12345").unwrap();
        let committed = state.committed();
        assert_eq!(committed.get("A.zip").map(String::as_str), Some("a"));
        // search.db by its unpacked checksum, which a version 2 manifest compares with
        assert_eq!(committed.get(SEARCH_DB).map(String::as_str), Some("db1"));
        std::fs::remove_dir_all(&app_data).unwrap();
    }
}
