use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_http::reqwest;
use tokio::{fs::remove_file, io::AsyncWriteExt};

use serde::Serialize;
use zip::ZipArchive;

use crate::error::AppError;

#[tauri::command]
pub async fn download_url(app: AppHandle, url: &str, filename: &str) -> Result<bool, AppError> {
    let app_data_dir = app.path().app_data_dir()?;

    download_url_local(url, &app_data_dir, filename, |progress| {
        app.emit("download-url-progress", progress).unwrap();
    })
    .await
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UnzipProgress {
    filename: String,
    out_dir: String,
}

#[tauri::command]
pub async fn unzip(app: AppHandle, filename: &str, out_dir: &str) -> Result<(), AppError> {
    let app_data_dir = app.path().app_data_dir()?;
    let filepath = app_data_dir.join(filename);
    let out_path = app_data_dir.join(out_dir);

    app.emit(
        "unzip-progress",
        UnzipProgress {
            filename: filename.to_string(),
            out_dir: out_dir.to_string(),
        },
    )?;

    tokio::task::spawn_blocking(move || unzip_sync(filepath, out_path)).await??;

    Ok(())
}

#[tauri::command]
pub async fn download_and_unzip(
    app: AppHandle,
    url: &str,
    filename: &str,
    out_dir: &str,
) -> Result<bool, AppError> {
    let out_path = app.path().app_data_dir()?.join(out_dir);

    let app_cache_dir = app.path().app_cache_dir()?;
    let zip_filepath = app_cache_dir.join(filename);

    if !zip_filepath.exists() {
        let r = download_url_local(url, &app_cache_dir, filename, |progress| {
            app.emit("download-url-progress", progress).unwrap();
        })
        .await?;
        if !r {
            return Ok(false);
        }

        app.emit(
            "unzip-progress",
            UnzipProgress {
                filename: filename.to_string(),
                out_dir: out_dir.to_string(),
            },
        )?;
    } else {
        app.emit(
            "unzip-progress",
            UnzipProgress {
                filename: format!("cached {filename}"),
                out_dir: out_dir.to_string(),
            },
        )?;
    }

    let zip = zip_filepath.clone();
    tokio::task::spawn_blocking(move || unzip_sync(zip, out_path)).await??;
    // extracted: the cached zip is no longer needed
    let _ = remove_file(&zip_filepath).await;

    Ok(true)
}

/// Extract into a staging dir next to `out_dir`, then move each top-level entry into place,
/// so an interrupted unzip never leaves a half-extracted `out_dir/<entry>` that looks installed.
fn unzip_sync(filepath: PathBuf, out_dir: PathBuf) -> Result<(), AppError> {
    let stem = filepath
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staging = out_dir.join(format!(".extract-{stem}"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    let file = std::fs::File::open(&filepath)?; // Use sync version in thread
    let mut archive = ZipArchive::new(file)?;
    archive.extract(&staging)?;

    for entry in std::fs::read_dir(&staging)? {
        let entry = entry?;
        let target = out_dir.join(entry.file_name());
        if target.is_dir() {
            std::fs::remove_dir_all(&target)?;
        } else if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(entry.path(), &target)?;
    }
    std::fs::remove_dir_all(&staging)?;

    println!("Unzipped {:?} to {:?}", &filepath, &out_dir);
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    url: String,
    filepath: String,
    content_length: u64,
    downloaded: u64,
}

/// Download to `<filename>.dl-tmp` (resuming it with a Range request if present),
/// and rename to `<filename>` only once complete.
async fn download_url_local<Callback>(
    url: &str,
    out_dir: &PathBuf,
    filename: &str,
    callback: Callback,
) -> Result<bool, AppError>
where
    Callback: Fn(DownloadProgress),
{
    tokio::fs::create_dir_all(out_dir).await?;

    let client = reqwest::Client::new();
    let file_pf = out_dir.join(filename);
    let temp_pf = out_dir.join(format!("{}.dl-tmp", filename));

    // Check existing partial download size
    let existing_size = tokio::fs::metadata(&temp_pf)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let mut request = client.get(url);
    // Request only the remaining bytes
    if existing_size > 0 {
        request = request.header("Range", format!("bytes={}-", existing_size));
    }

    let mut response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Error(format!("GET {url}: {status}")));
    }

    // 206 = server honored the Range; anything else restarts from zero
    let resumed = existing_size > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut downloaded: u64 = if resumed { existing_size } else { 0 };
    let content_length = response
        .content_length()
        .map(|n| n + downloaded)
        .unwrap_or(0);

    let mut file = if resumed {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&temp_pf)
            .await?
    } else {
        tokio::fs::File::create(&temp_pf).await?
    };

    // Downloading the file in chunks
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        callback(DownloadProgress {
            url: url.to_owned(),
            filepath: filename.to_owned(),
            content_length,
            downloaded,
        });
    }
    file.flush().await?;
    drop(file); // ensure file handle is released before renaming

    // Verify download completeness; keep the partial file for resuming next time
    if content_length > 0 && downloaded < content_length {
        return Err(AppError::Error(format!(
            "Incomplete download: expected {} bytes, got {}",
            content_length, downloaded
        )));
    }

    tokio::fs::rename(&temp_pf, &file_pf).await?;

    println!("File downloaded {} MB to {:?}", downloaded >> 20, &file_pf);

    Ok(true)
}
