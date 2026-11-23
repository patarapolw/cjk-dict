//! Keep the app running while it syncs dictionaries.
//!
//! On Android, an app in the background (or with the screen off) is soon frozen, which
//! stalls downloading and checksumming gigabytes of dictionaries, and building search.db
//! from them. While a sync runs, the frontend reports its progress here, and on Android a
//! foreground service (`SyncService.kt`) shows it as a notification and keeps the app
//! running. Building search.db reports from Rust too ([`set_native_sync_status`]), as it
//! runs for long without the page. Desktops don't freeze apps in the background: nothing
//! to do there.

use crate::error::AppError;

/// Show the sync's progress and keep the app running for it; `text: None` ends that.
/// `percent: None` shows an indeterminate progress bar.
#[tauri::command]
pub async fn set_sync_status<R: tauri::Runtime>(
    webview: tauri::Webview<R>,
    text: Option<String>,
    percent: Option<u8>,
) -> Result<(), AppError> {
    #[cfg(target_os = "android")]
    {
        android::set_sync_status(&webview, text, percent)
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (webview, text, percent);
        Ok(())
    }
}

/// [`set_sync_status`] from Rust, for work the page may not hear of for minutes: building
/// search.db. Each call also re-arms SyncService's wake lock, which otherwise lapses after
/// 10 minutes without one. Failures are only logged.
pub fn set_native_sync_status(app: &tauri::AppHandle, text: Option<String>, percent: Option<u8>) {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let Some(window) = app.webview_windows().into_values().next() else {
            return;
        };
        if let Err(e) =
            android::set_sync_status(AsRef::<tauri::Webview>::as_ref(&window), text, percent)
        {
            eprintln!("setSyncStatus failed: {e}");
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, text, percent);
    }
}

#[cfg(target_os = "android")]
mod android {
    use jni::{
        JNIEnv,
        objects::{JObject, JValue},
    };

    use crate::error::AppError;

    pub fn set_sync_status<R: tauri::Runtime>(
        webview: &tauri::Webview<R>,
        text: Option<String>,
        percent: Option<u8>,
    ) -> Result<(), AppError> {
        webview.with_webview(move |platform| {
            // runs on the main thread, as SyncService expects
            platform.jni_handle().exec(move |env, activity, _webview| {
                if let Err(e) = call(env, activity, text.as_deref(), percent) {
                    // a pending Java exception would abort the next JNI call
                    if env.exception_check().unwrap_or(false) {
                        let _ = env.exception_describe();
                        let _ = env.exception_clear();
                    }
                    eprintln!("setSyncStatus failed: {e}");
                }
            });
        })?;
        Ok(())
    }

    /// `MainActivity.setSyncStatus(text, percent)`
    fn call(
        env: &mut JNIEnv,
        activity: &JObject,
        text: Option<&str>,
        percent: Option<u8>,
    ) -> jni::errors::Result<()> {
        let text = match text {
            Some(t) => JObject::from(env.new_string(t)?),
            None => JObject::null(),
        };
        let percent = percent.map_or(-1, i32::from);
        env.call_method(
            activity,
            "setSyncStatus",
            "(Ljava/lang/String;I)V",
            &[JValue::Object(&text), JValue::Int(percent)],
        )?;
        Ok(())
    }
}
