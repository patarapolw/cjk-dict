use std::{error::Error, fmt, sync::PoisonError};

use yomitan_parser::error::YomitanError;
use zip::result::ZipError;

#[derive(Debug)]
pub enum AppError {
    TauriError(tauri::Error),
    TauriReqwestError(tauri_plugin_http::reqwest::Error),
    YomitanError(YomitanError),
    MutexPoisonedError(String),
    IOError(std::io::Error),
    ZipError(ZipError),
    TokioJoinError(tokio::task::JoinError),
    SerdeJsonError(serde_json::Error),
    Error(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TauriError(e) => write!(f, "tauri::Error: {e}"),
            Self::YomitanError(e) => write!(f, "YomitanError: {e}"),
            Self::TauriReqwestError(e) => write!(f, "TauriReqwest: {e}"),
            Self::MutexPoisonedError(e) => write!(f, "MutexPoisoned: {e}"),
            Self::IOError(e) => write!(f, "IOError: {e}"),
            Self::ZipError(e) => write!(f, "ZipError: {e}"),
            Self::TokioJoinError(e) => write!(f, "TokioJoinError: {e}"),
            Self::SerdeJsonError(e) => write!(f, "SerdeJsonError: {e}"),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

impl Error for AppError {}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self::TauriError(e)
    }
}

impl From<tauri_plugin_http::reqwest::Error> for AppError {
    fn from(e: tauri_plugin_http::reqwest::Error) -> Self {
        Self::TauriReqwestError(e)
    }
}

impl From<YomitanError> for AppError {
    fn from(e: YomitanError) -> Self {
        Self::YomitanError(e)
    }
}

impl<T> From<PoisonError<T>> for AppError {
    fn from(e: PoisonError<T>) -> Self {
        Self::MutexPoisonedError(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(e)
    }
}

impl From<ZipError> for AppError {
    fn from(e: ZipError) -> Self {
        Self::ZipError(e)
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::TokioJoinError(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerdeJsonError(e)
    }
}

impl From<String> for AppError {
    fn from(e: String) -> Self {
        Self::Error(e)
    }
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
