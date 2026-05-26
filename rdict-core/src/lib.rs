#![forbid(unsafe_code)]

pub mod german;
pub mod parse;
pub mod rdict;
pub mod source;
pub mod youdao;

use std::path::PathBuf;
use thiserror::Error;

/// Check if text contains CJK (Chinese, Japanese, Korean) characters
///
/// Used to determine whether to route to English→Chinese or Chinese→English parsing
/// for the Youdao source. The CJK range covers common CJK Unified Ideographs.
pub(crate) fn is_cjk(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid UTF-8 database path: {0}")]
    InvalidDatabasePath(PathBuf),

    #[error("No translation results")]
    NoTranslationResults,

    #[error("Failed to parse response: {0}")]
    Parse(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
