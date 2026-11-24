//! `font://`: fonts downloaded to `<app data>/fonts/`, for the webview's `@font-face`
//! rules (`app/src/util/fonts.ts`). Only a font file's name is served, nothing else from
//! the app data folder.

use std::path::PathBuf;

use tauri::{
    AppHandle, Manager,
    http::{Request, Response, StatusCode},
};

/// the folder in the app data folder that the fonts are downloaded to
pub const FONTS_DIR: &str = "fonts";

/// `<app data>/fonts/`, resolved once at startup. On Android, resolving a path goes through
/// a plugin call on the main thread; a font request that did so could wait on a main
/// thread waiting on the webview to draw, which waits on the font: the page froze.
pub struct FontsDir(pub PathBuf);

/// The font's MIME type, if `name` is a plain font file name: `HanaMinA.otf`, not
/// `../search.db` or `sub/HanaMinA.otf`.
fn font_mime(name: &str) -> Option<&'static str> {
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !valid {
        return None;
    }
    match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "otf" => Some("font/otf"),
        "ttf" => Some("font/ttf"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

pub fn font_protocol(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap()
    };
    let name = request.uri().path().trim_start_matches('/');
    let Some(mime) = font_mime(name) else {
        return not_found();
    };
    let Some(dir) = app.try_state::<FontsDir>() else {
        return not_found();
    };
    match std::fs::read(dir.0.join(name)) {
        Ok(bytes) => Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "max-age=31536000, immutable")
            // fonts are always fetched with CORS, and the page (tauri.localhost) is
            // another origin
            .header("Access-Control-Allow-Origin", "*")
            .body(bytes)
            .unwrap(),
        Err(_) => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use super::font_mime;

    #[test]
    fn serves_only_font_file_names() {
        assert_eq!(font_mime("HanaMinA.otf"), Some("font/otf"));
        assert_eq!(font_mime("x.WOFF2"), Some("font/woff2"));
        assert_eq!(font_mime("../search.db"), None);
        assert_eq!(font_mime("..%2Fsearch.otf"), None);
        assert_eq!(font_mime("sub/HanaMinA.otf"), None);
        assert_eq!(font_mime("sub\\HanaMinA.otf"), None);
        assert_eq!(font_mime(".otf"), None);
        assert_eq!(font_mime("HanaMinA.otf.dl-tmp"), None);
        assert_eq!(font_mime("search.db"), None);
        assert_eq!(font_mime(""), None);
    }
}
