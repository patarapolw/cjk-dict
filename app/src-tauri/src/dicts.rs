//! Dictionaries: sync the library (Yomitan zips + search.db + manifest.json) from R2,
//! search it, and serve media from the zips.
//!
//! Files land in `<app data>/dicts/incoming/` (via the `download_url` command, resumable)
//! and are moved into `<app data>/dicts/` by `dict_commit` once all of them are there,
//! with the open database closed first (Windows can't replace open files).

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
use yomitan_parser::search::{DictInfo, HanChar, HanList, SearchDb, SearchRequest, SearchResult};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    search_db: FileInfo,
    dicts: Vec<ManifestDict>,
}

impl Manifest {
    fn files(&self) -> impl Iterator<Item = &FileInfo> {
        std::iter::once(&self.search_db).chain(self.dicts.iter().map(|d| &d.file))
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
        if unchanged && out.is_empty() {
            let _ = std::fs::remove_file(state.dir.join(PENDING));
            return Ok(out);
        }
        std::fs::write(state.dir.join(PENDING), &text)?;
        // nothing to download, but dicts may have been removed: commit right away
        if out.is_empty() {
            commit(&app, &state)?;
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Error(e.to_string()))?
}

/// Verify downloaded files, move them into place, drop removed dictionaries, reopen.
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

    // close search.db and the zips before replacing them
    state.close()?;
    for f in m.files() {
        let new = incoming.join(&f.file);
        if new.exists() {
            std::fs::rename(&new, state.dir.join(&f.file))?;
        }
    }
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
