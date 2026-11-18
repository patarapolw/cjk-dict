use tauri::State;
use yomitan_parser::{error::YomitanError, search::parse_lang};

use crate::{AppState, error::AppError};

#[tauri::command]
pub async fn segment(
    lang: &str,
    text: &str,
    state: State<'_, AppState>,
) -> Result<Vec<(String, Vec<String>)>, AppError> {
    Ok(state
        .tokenizer_mapper
        .segment(
            parse_lang(lang).ok_or(YomitanError::UnsupportedLanguageError(lang.to_string()))?,
            text,
        )
        .await?)
}

#[tauri::command]
pub async fn tokenize(
    lang: &str,
    text: &str,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    Ok(state
        .tokenizer_mapper
        .tokenize(
            parse_lang(lang).ok_or(YomitanError::UnsupportedLanguageError(lang.to_string()))?,
            text,
        )
        .await?)
}
