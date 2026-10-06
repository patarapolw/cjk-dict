use std::{error::Error, fmt, sync::PoisonError};

use lindera::error::LinderaError;
use sqlx::migrate::MigrateError;
use zip::result::ZipError;

#[derive(Debug)]
pub enum YomitanError {
    SqlxError(sqlx::Error),
    SqlxMigrateError(MigrateError),
    TokioJoinError(tokio::task::JoinError),
    IOError(std::io::Error),
    ZipError(ZipError),
    JSONError(serde_json::Error),
    LinderaError(LinderaError),
    MutexPoisonedError(String),
    UnsupportedLanguageError(String),
    Error(String),
}

impl fmt::Display for YomitanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SqlxError(e) => write!(f, "sqlx::Error: {e}"),
            Self::SqlxMigrateError(e) => write!(f, "sqlx::migrate::MigrateError: {e}"),
            Self::TokioJoinError(e) => write!(f, "tokio::task::JoinError: {e}"),
            Self::IOError(e) => write!(f, "std::io::Error: {e}"),
            Self::ZipError(e) => write!(f, "ZipError: {e}"),
            Self::JSONError(e) => write!(f, "serde_json::Error: {e}"),
            Self::LinderaError(e) => write!(f, "LinderaError: {e}"),
            Self::MutexPoisonedError(e) => write!(f, "MutexPoisonedError: {e}"),
            Self::UnsupportedLanguageError(e) => write!(f, "UnsupportedLanguageError: {e}"),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

impl Error for YomitanError {}

impl From<sqlx::Error> for YomitanError {
    fn from(e: sqlx::Error) -> Self {
        Self::SqlxError(e)
    }
}

impl From<MigrateError> for YomitanError {
    fn from(e: MigrateError) -> Self {
        Self::SqlxMigrateError(e)
    }
}

impl From<tokio::task::JoinError> for YomitanError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::TokioJoinError(e)
    }
}

impl From<std::io::Error> for YomitanError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(e)
    }
}

impl From<zip::result::ZipError> for YomitanError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::ZipError(e)
    }
}

impl From<serde_json::Error> for YomitanError {
    fn from(e: serde_json::Error) -> Self {
        Self::JSONError(e)
    }
}

impl From<LinderaError> for YomitanError {
    fn from(e: LinderaError) -> Self {
        Self::LinderaError(e)
    }
}

impl<T> From<PoisonError<T>> for YomitanError {
    fn from(e: PoisonError<T>) -> Self {
        Self::MutexPoisonedError(e.to_string())
    }
}

impl From<String> for YomitanError {
    fn from(e: String) -> Self {
        Self::Error(e)
    }
}

impl serde::Serialize for YomitanError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
