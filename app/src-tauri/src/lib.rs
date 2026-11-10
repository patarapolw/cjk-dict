use tauri::Manager;
use yomitan_parser::tokenize::TokenizerMapper;

mod clipboard;
mod dicts;
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        // media referenced by structured content, read from the dictionary zips
        .register_asynchronous_uri_scheme_protocol("dict", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(dicts::asset_protocol(&app, request));
            });
        })
        .setup(|app| {
            let app_data_dir = app.handle().path().app_data_dir()?;

            app.manage(AppState {
                tokenizer_mapper: TokenizerMapper::new(app_data_dir.join("lindera")),
            });
            app.manage(dicts::DictState::new(&app_data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            clipboard::read_clipboard_text,
            download::download_url,
            download::unzip,
            download::download_and_unzip,
            tokenize::segment,
            tokenize::tokenize,
            dicts::dict_plan,
            dicts::dict_commit,
            dicts::search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
