use tauri::Manager;
use yomitan_parser::tokenize::TokenizerMapper;

mod download;
mod error;
mod tokenize;

struct AppState {
    tokenizer_mapper: TokenizerMapper,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.handle().path().app_data_dir()?;

            app.manage(AppState {
                tokenizer_mapper: TokenizerMapper::new(app_data_dir.join("lindera")),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            download::download_url,
            download::unzip,
            download::download_and_unzip,
            tokenize::segment,
            tokenize::tokenize,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
