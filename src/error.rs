use std::{error::Error, fmt};

use sqlx::migrate::MigrateError;
use zip::result::ZipError;

#[derive(Debug)]
pub enum YomitanError {
    Sqlx(sqlx::Error),
    SqlxMigrate(MigrateError),
    TokioJoin(tokio::task::JoinError),
    StdIo(std::io::Error),
    Zip(ZipError),
    JSON(serde_json::Error),
    Error(String),
}

impl fmt::Display for YomitanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlx(e) => write!(f, "sqlx::Error: {e}"),
            Self::SqlxMigrate(e) => write!(f, "sqlx::migrate::MigrateError: {e}"),
            Self::TokioJoin(e) => write!(f, "tokio::task::JoinError: {e}"),
            Self::StdIo(e) => write!(f, "std::io::Error: {e}"),
            Self::Zip(e) => write!(f, "ZipError: {e}"),
            Self::JSON(e) => write!(f, "serde_json::Error: {e}"),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

impl Error for YomitanError {}

impl From<sqlx::Error> for YomitanError {
    fn from(e: sqlx::Error) -> Self {
        Self::Sqlx(e)
    }
}

impl From<MigrateError> for YomitanError {
    fn from(e: MigrateError) -> Self {
        Self::SqlxMigrate(e)
    }
}

impl From<tokio::task::JoinError> for YomitanError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::TokioJoin(e)
    }
}

impl From<std::io::Error> for YomitanError {
    fn from(e: std::io::Error) -> Self {
        Self::StdIo(e)
    }
}

impl From<zip::result::ZipError> for YomitanError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}

impl From<serde_json::Error> for YomitanError {
    fn from(e: serde_json::Error) -> Self {
        Self::JSON(e)
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
