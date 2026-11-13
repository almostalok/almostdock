use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OxideError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Win32 error: {0}")]
    Win32(String),

    #[error("DWM configuration error: {0}")]
    Dwm(String),

    #[error("Font error: {0}")]
    Font(String),

    #[error("Render error: {0}")]
    Render(String),

    #[error("Path not found: {0:?}")]
    NotFound(PathBuf),

    #[error("Shell error: {0}")]
    Shell(String),
}

pub type Result<T> = std::result::Result<T, OxideError>;
