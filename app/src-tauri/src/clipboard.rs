//! Clipboard reading for the clipboard monitor.
//!
//! On Android, `tauri-plugin-clipboard-manager`'s `readText` rejects any clip whose
//! MIME type is not `text/plain` — which includes HTML copied from browsers and most
//! apps (`text/html`). Read the clip ourselves through JNI instead, and convert HTML
//! to plain text when the clip carries no plain-text alternative.

use crate::error::AppError;

/// Returns the clipboard content as plain text, or `None` when the clipboard is empty
/// or holds nothing that can be turned into text.
#[tauri::command]
pub async fn read_clipboard_text<R: tauri::Runtime>(
    webview: tauri::Webview<R>,
) -> Result<Option<String>, AppError> {
    #[cfg(target_os = "android")]
    {
        android::read_text(&webview).await
    }

    #[cfg(not(target_os = "android"))]
    {
        use tauri::Manager;
        use tauri_plugin_clipboard_manager::ClipboardExt;

        let app = webview.app_handle().clone();
        // must not run on the main thread, see `Clipboard::read_text`
        let text = tokio::task::spawn_blocking(move || app.clipboard().read_text().ok()).await?;
        Ok(text)
    }
}

#[cfg(target_os = "android")]
mod android {
    use jni::{
        JNIEnv,
        objects::{JObject, JString, JValue},
    };

    use crate::error::AppError;

    /// `Html.FROM_HTML_MODE_LEGACY`
    const FROM_HTML_MODE_LEGACY: i32 = 0;

    pub async fn read_text<R: tauri::Runtime>(
        webview: &tauri::Webview<R>,
    ) -> Result<Option<String>, AppError> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        webview.with_webview(move |platform| {
            platform.jni_handle().exec(move |env, activity, _webview| {
                let result = read_primary_clip(env, activity).map_err(|e| {
                    // a pending Java exception would abort the next JNI call
                    if env.exception_check().unwrap_or(false) {
                        let _ = env.exception_clear();
                    }
                    e.to_string()
                });
                let _ = tx.send(result);
            });
        })?;

        rx.await
            .map_err(|e| AppError::Error(format!("clipboard read was dropped: {e}")))?
            .map_err(|e| AppError::Error(format!("clipboard read failed: {e}")))
    }

    fn read_primary_clip(env: &mut JNIEnv, activity: &JObject) -> jni::errors::Result<Option<String>> {
        let service = env.new_string("clipboard")?; // Context.CLIPBOARD_SERVICE
        let manager = env
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&service)],
            )?
            .l()?;
        if manager.is_null() {
            return Ok(None);
        }

        let clip = env
            .call_method(&manager, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?
            .l()?;
        if clip.is_null() {
            return Ok(None);
        }
        if env.call_method(&clip, "getItemCount", "()I", &[])?.i()? < 1 {
            return Ok(None);
        }
        let item = env
            .call_method(
                &clip,
                "getItemAt",
                "(I)Landroid/content/ClipData$Item;",
                &[JValue::Int(0)],
            )?
            .l()?;
        if item.is_null() {
            return Ok(None);
        }

        // Plain-text alternative, present for `text/plain` clips and for HTML clips
        // created with `ClipData.newHtmlText`.
        let text = env
            .call_method(&item, "getText", "()Ljava/lang/CharSequence;", &[])?
            .l()?;
        if let Some(text) = char_sequence_to_string(env, &text)?
            && !text.trim().is_empty()
            && !looks_like_html(&text)
        {
            return Ok(Some(text));
        }

        // HTML-only clip (or a "plain text" that is really markup): render it to text.
        let html = env
            .call_method(&item, "getHtmlText", "()Ljava/lang/String;", &[])?
            .l()?;
        let html = if html.is_null() {
            match char_sequence_to_string(env, &text)? {
                Some(t) if looks_like_html(&t) => t,
                _ => return Ok(None),
            }
        } else {
            env.get_string(&JString::from(html))?.into()
        };

        html_to_text(env, &html)
    }

    /// `android.text.Html.fromHtml(html, FROM_HTML_MODE_LEGACY).toString()`
    fn html_to_text(env: &mut JNIEnv, html: &str) -> jni::errors::Result<Option<String>> {
        let html = env.new_string(html)?;
        let spanned = env
            .call_static_method(
                "android/text/Html",
                "fromHtml",
                "(Ljava/lang/String;I)Landroid/text/Spanned;",
                &[JValue::Object(&html), JValue::Int(FROM_HTML_MODE_LEGACY)],
            )?
            .l()?;
        Ok(char_sequence_to_string(env, &spanned)?.map(|text| {
            // drop object replacement characters left in place of images
            text.replace('\u{FFFC}', "")
        }))
    }

    fn char_sequence_to_string(
        env: &mut JNIEnv,
        seq: &JObject,
    ) -> jni::errors::Result<Option<String>> {
        if seq.is_null() {
            return Ok(None);
        }
        let s = env
            .call_method(seq, "toString", "()Ljava/lang/String;", &[])?
            .l()?;
        if s.is_null() {
            return Ok(None);
        }
        Ok(Some(env.get_string(&JString::from(s))?.into()))
    }

    /// Cheap check for markup pasted as plain text, e.g. `<span>漢字</span>`.
    fn looks_like_html(text: &str) -> bool {
        let t = text.trim_start();
        t.starts_with('<')
            && t.trim_end().ends_with('>')
            && (t.contains("</") || t.contains("/>") || t.contains("<br"))
    }
}
