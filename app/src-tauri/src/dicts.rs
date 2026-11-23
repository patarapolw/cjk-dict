//! Dictionaries: sync the library (Yomitan zips + ids.txt + manifest.json) from R2, build
//! search.db from it, search it, and serve media from the zips.
//!
//! Files land in `<app data>/dicts/incoming/` (via the `download_url` command, resumable).
//! Once all of them are there, `dict_commit` builds the new search.db next to them, from
//! the one in use (only the dictionaries that changed are indexed), while searches go on
//! with that one. Then it moves everything into `<app data>/dicts/`, with the open database
//! closed first (Windows can't replace open files).

use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tauri_plugin_http::reqwest;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use yomitan_parser::search::{
    self, DictInfo, DictSource, HanChar, HanList, SearchDb, SearchRequest, SearchResult,
};

use crate::{AppState, error::AppError};

const MANIFEST: &str = "manifest.json";
const SEARCH_DB: &str = "search.db";
const IDS_TXT: &str = "ids.txt";
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
    /// the terms' language, e.g. `ja-JP`, as dict-admin indexed it
    #[serde(default)]
    lang: Option<String>,
    /// the definitions' language, e.g. `en`
    #[serde(default)]
    target_lang: Option<String>,
    #[serde(flatten)]
    file: FileInfo,
}

/// dict-admin's `manifest.json`: the files of the library, and the dictionaries in build
/// order. One from before the app built search.db (version 1) lists `search_db` in place of
/// `ids`; the app ignores that.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    /// the CJKVI IDS database's `ids.txt`, for the character tables
    #[serde(default)]
    ids: Option<FileInfo>,
    dicts: Vec<ManifestDict>,
}

impl Manifest {
    fn files(&self) -> impl Iterator<Item = &FileInfo> {
        self.ids.iter().chain(self.dicts.iter().map(|d| &d.file))
    }
}

/// What a commit indexes, from what search.db was built from (`old`, the committed
/// manifest) and what it is to be built from (`new`)
#[derive(Debug, PartialEq)]
struct IndexPlan {
    /// build search.db from scratch, of every dictionary
    full: bool,
    /// the dictionaries to index, in build order
    index: Vec<String>,
    /// the dictionaries to take out first: removed or changed ones
    remove: Vec<String>,
}

/// `full` when there is no search.db to update (`current` is false: none, or one built
/// otherwise, e.g. the one an earlier version downloaded) or nothing it was built from.
/// Else the dictionaries that are new or changed (zip or languages) are indexed again.
fn index_plan(old: Option<&Manifest>, new: &Manifest, current: bool) -> IndexPlan {
    let all = || new.dicts.iter().map(|d| d.name.clone()).collect();
    let Some(old) = old.filter(|_| current) else {
        return IndexPlan {
            full: true,
            index: all(),
            remove: vec![],
        };
    };
    let same = |a: &ManifestDict, b: &ManifestDict| {
        a.file.sha256 == b.file.sha256 && a.lang == b.lang && a.target_lang == b.target_lang
    };
    let index: Vec<String> = new
        .dicts
        .iter()
        .filter(|d| !old.dicts.iter().any(|o| o.name == d.name && same(o, d)))
        .map(|d| d.name.clone())
        .collect();
    let remove = old
        .dicts
        .iter()
        .filter(|o| !new.dicts.iter().any(|d| d.name == o.name) || index.contains(&o.name))
        .map(|o| o.name.clone())
        .collect();
    IndexPlan {
        full: false,
        index,
        remove,
    }
}

/// `dict-index-progress`: what building search.db is at
#[derive(Clone, Serialize)]
struct IndexProgress<'a> {
    /// a dictionary's name, or a step such as `full-text index`
    step: &'a str,
    /// term banks indexed so far, of `total`; 0 when not counted
    done: usize,
    total: usize,
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
                "no dictionary source configured: build with CJK_DICT_R2_* (or CJK_DICT_BASE_URL)"
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

    /// Whether `<dir>/search.db` is there, built as this version builds it, so that a
    /// commit can update it rather than build it from scratch
    fn index_current(&self) -> bool {
        search::build_version(&self.dir.join(SEARCH_DB))
            .ok()
            .flatten()
            .as_deref()
            == Some(search::BUILD_VERSION)
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

    /// file -> sha256, for files in place that the committed manifest vouches for
    fn committed(&self) -> HashMap<String, String> {
        self.local_manifest()
            .map(|m| {
                m.files()
                    .filter(|f| self.dir.join(&f.file).exists())
                    .map(|f| (f.file.clone(), f.sha256.clone()))
                    .collect()
            })
            .unwrap_or_default()
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

    /// Whether `path` holds exactly `f`. Sizes are compared first, so most mismatches
    /// are cheap.
    fn has_file(&self, app: &AppHandle, path: &Path, f: &FileInfo) -> Result<bool, AppError> {
        match std::fs::metadata(path) {
            Ok(m) if m.is_file() && m.len() == f.size => Ok(self.sha256(app, path)? == f.sha256),
            _ => Ok(false),
        }
    }
}

#[derive(Clone, Serialize)]
struct VerifyProgress<'a> {
    file: &'a str,
    /// bytes hashed so far
    hashed: u64,
    size: u64,
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
pub struct PlannedFile {
    /// relative to app data dir, for the `download_url` command
    filename: String,
    url: String,
    size: u64,
}

/// Fetch the remote manifest and list the files that need downloading.
/// Empty = up to date. Remembers the manifest until `dict_commit`.
#[tauri::command]
pub async fn dict_plan(app: AppHandle) -> Result<Vec<PlannedFile>, AppError> {
    let remote = Remote::from_env()?;
    let res = reqwest::get(remote.url(MANIFEST)).await?;
    if !res.status().is_success() {
        return Err(AppError::Error(format!(
            "manifest.json: HTTP {}",
            res.status()
        )));
    }
    let text = res.text().await?;
    let remote_manifest: Manifest = serde_json::from_str(&text)?;
    if remote_manifest.ids.is_none() {
        return Err(AppError::Error(
            "the library is from before the app built search.db: build and upload it again \
             with dict-admin"
                .into(),
        ));
    }

    // hashing files and committing is blocking work
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DictState>();
        let committed = state.committed();
        let incoming = state.dir.join(INCOMING);
        std::fs::create_dir_all(&incoming)?;

        let mut out = vec![];
        for f in remote_manifest.files() {
            if committed.get(&f.file) == Some(&f.sha256) {
                continue;
            }
            // Downloaded by an earlier run that was closed before the rest finished (or
            // whose commit failed): nothing is committed until every file is there.
            let new = incoming.join(&f.file);
            if new.exists() {
                if state.has_file(&app, &new, f)? {
                    continue;
                }
                let _ = std::fs::remove_file(&new);
            }
            // moved into place by a commit that failed partway, before the manifest
            if state.has_file(&app, &state.dir.join(&f.file), f)? {
                continue;
            }
            out.push(PlannedFile {
                filename: format!("dicts/{INCOMING}/{}", f.file),
                url: remote.url(&f.file),
                size: f.size,
            });
        }

        let unchanged =
            std::fs::read_to_string(state.dir.join(MANIFEST)).ok() == Some(text.clone());
        if unchanged && out.is_empty() && state.index_current() {
            let _ = std::fs::remove_file(state.dir.join(PENDING));
            return Ok(out);
        }
        // `dict_commit` once these (and the tokenizer dictionaries, to build search.db) are
        // downloaded; right away if there are none, e.g. when dictionaries were only removed
        std::fs::write(state.dir.join(PENDING), &text)?;
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
}

/// Verify downloaded files, build search.db from them, move them into place, drop removed
/// dictionaries, reopen. Nothing to do without a plan (`dict_plan`) to commit. Needs the
/// tokenizer dictionaries.
#[tauri::command]
pub async fn dict_commit(app: AppHandle) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DictState>();
        commit(&app, &state)?;
        Ok(state.get().map(|db| db.dict_names()).unwrap_or_default())
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
}

fn commit(app: &AppHandle, state: &DictState) -> Result<(), AppError> {
    let pending = state.dir.join(PENDING);
    let Ok(text) = std::fs::read_to_string(&pending) else {
        return Ok(());
    };
    let m: Manifest = serde_json::from_str(&text)?;
    let incoming = state.dir.join(INCOMING);
    let committed = state.committed();

    // everything must be present and intact before anything is replaced
    for f in m.files() {
        let new = incoming.join(&f.file);
        let path = if new.exists() {
            new
        } else {
            // in place and already verified by an earlier commit
            if committed.get(&f.file) == Some(&f.sha256) {
                continue;
            }
            state.dir.join(&f.file)
        };
        if !path.exists() {
            return Err(AppError::Error(format!("{} was not downloaded", f.file)));
        }
        if state.sha256(app, &path)? != f.sha256 {
            let _ = std::fs::remove_file(&path);
            return Err(AppError::Error(format!(
                "{} is corrupt (checksum mismatch); it will be downloaded again",
                f.file
            )));
        }
    }

    // the new search.db, next to the files downloaded, while the one in use stays open
    let new_db = incoming.join(SEARCH_DB);
    build_index(app, state, &m, &new_db)?;

    // close search.db and the zips before replacing them
    state.close()?;
    for f in m.files() {
        let new = incoming.join(&f.file);
        if new.exists() {
            std::fs::rename(&new, state.dir.join(&f.file))?;
        }
    }
    std::fs::rename(&new_db, state.dir.join(SEARCH_DB))?;
    let keep: Vec<&str> = m.files().map(|f| f.file.as_str()).collect();
    for e in std::fs::read_dir(&state.dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".zip") && !keep.contains(&name.as_str()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    std::fs::rename(&pending, state.dir.join(MANIFEST))?;
    // the files moved: `committed()` vouches for them from now on
    state.hashed.lock()?.clear();
    state.reopen()
}

/// Build `out`, the search.db of `m`'s dictionaries: an update of the one in use, or one
/// from scratch. Reports progress as `dict-index-progress` events.
fn build_index(
    app: &AppHandle,
    state: &DictState,
    m: &Manifest,
    out: &Path,
) -> Result<(), AppError> {
    let incoming = state.dir.join(INCOMING);
    // new files first: those in place are the old ones of changed dictionaries
    let zip_dirs = [incoming.clone(), state.dir.clone()];
    let find = |file: &str| {
        zip_dirs
            .iter()
            .map(|d| d.join(file))
            .find(|p| p.is_file())
            .ok_or_else(|| AppError::Error(format!("{file} is missing")))
    };
    let plan = index_plan(state.local_manifest().as_ref(), m, state.index_current());
    let sources = m
        .dicts
        .iter()
        .filter(|d| plan.index.contains(&d.name))
        .map(|d| {
            Ok(DictSource {
                name: d.name.clone(),
                zip: find(&d.file.file)?,
                lang: d.lang.clone(),
                target_lang: d.target_lang.clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let ids_txt = find(IDS_TXT)?;

    let progress = |step: &str, done: usize, total: usize| {
        let _ = app.emit("dict-index-progress", IndexProgress { step, done, total });
    };
    let tok = &app.state::<AppState>().tokenizer_mapper;
    let base = state.dir.join(SEARCH_DB);
    tauri::async_runtime::block_on(async {
        if plan.full {
            search::build(out, &sources, tok, progress).await
        } else {
            search::update(&base, out, &sources, &plan.remove, &zip_dirs, tok, progress).await
        }
    })?;
    progress("characters", 0, 0);
    let changed = plan.full || !plan.index.is_empty() || !plan.remove.is_empty();
    search::build_chars_from(out, &ids_txt, changed, &zip_dirs)?;
    Ok(())
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

/// The Han characters of `text`: how each breaks down, and its kanji dictionary entries.
#[tauri::command]
pub async fn han_chars(
    state: State<'_, DictState>,
    text: String,
) -> Result<Vec<HanChar>, AppError> {
    let db = state.get()?;
    tauri::async_runtime::spawn_blocking(move || db.han_chars(&text))
        .await
        .map_err(|e| AppError::Error(e.to_string()))?
        .map_err(AppError::from)
}

/// Characters made of all the characters of `components`, the most used first. With
/// `direct`, only those whose IDS name the components themselves.
#[tauri::command]
pub async fn han_containing(
    state: State<'_, DictState>,
    components: String,
    limit: Option<u32>,
    offset: Option<u32>,
    direct: Option<bool>,
) -> Result<HanList, AppError> {
    let db = state.get()?;
    let direct = direct.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        db.han_containing(&components, limit, offset, direct)
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

    fn manifest(dicts: &[(&str, &str, Option<&str>)]) -> Manifest {
        Manifest {
            version: 2,
            ids: Some(FileInfo {
                file: IDS_TXT.into(),
                size: 1,
                sha256: "i".into(),
            }),
            dicts: dicts
                .iter()
                .map(|(name, sha256, lang)| ManifestDict {
                    name: name.to_string(),
                    lang: lang.map(String::from),
                    target_lang: None,
                    file: FileInfo {
                        file: format!("{name}.zip"),
                        size: 1,
                        sha256: sha256.to_string(),
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn only_changed_dictionaries_are_indexed_again() {
        let old = manifest(&[
            ("A", "a", None),
            ("B", "b", None),
            ("C", "c", Some("ja-JP")),
        ]);
        // B replaced, C's language changed, D added, A kept
        let new = manifest(&[
            ("A", "a", None),
            ("B", "b2", None),
            ("C", "c", Some("zh")),
            ("D", "d", None),
        ]);
        assert_eq!(
            index_plan(Some(&old), &new, true),
            IndexPlan {
                full: false,
                index: vec!["B".into(), "C".into(), "D".into()],
                remove: vec!["B".into(), "C".into()],
            }
        );
        let fewer = manifest(&[("C", "c", Some("ja-JP"))]);
        assert_eq!(
            index_plan(Some(&old), &fewer, true),
            IndexPlan {
                full: false,
                index: vec![],
                remove: vec!["A".into(), "B".into()],
            }
        );
    }

    #[test]
    fn without_a_current_search_db_everything_is_indexed() {
        let old = manifest(&[("A", "a", None)]);
        let new = manifest(&[("A", "a", None), ("B", "b", None)]);
        let full = IndexPlan {
            full: true,
            index: vec!["A".into(), "B".into()],
            remove: vec![],
        };
        // e.g. the search.db an earlier version downloaded
        assert_eq!(index_plan(Some(&old), &new, false), full);
        assert_eq!(index_plan(None, &new, true), full);
    }

    #[test]
    fn a_manifest_from_before_reads() {
        let m: Manifest = serde_json::from_str(
            r#"{"version":1,"search_db":{"file":"search.db","size":5,"sha256":"s"},
                "dicts":[{"name":"A","title":"A","revision":"1","lang":"ja-JP","term_count":5,
                          "file":"A.zip","size":1,"sha256":"a"}]}"#,
        )
        .unwrap();
        assert!(m.ids.is_none());
        assert_eq!(m.dicts[0].lang.as_deref(), Some("ja-JP"));
        assert_eq!(
            m.files().map(|f| f.file.as_str()).collect::<Vec<_>>(),
            ["A.zip"]
        );
    }

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
}
