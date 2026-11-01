//! Error taxonomy for the diff crate (`DiffError`) and its `Result` alias.

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, DiffError>;

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("P4k error: {0}")]
    P4k(#[from] starbreaker_p4k::P4kError),

    #[error("DataCore parse error: {0}")]
    DataCoreParse(#[from] starbreaker_datacore::error::ParseError),

    #[error("DataCore export error: {0}")]
    DataCoreExport(#[from] starbreaker_datacore::error::ExportError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("inventory generation was cancelled")]
    Cancelled,

    #[error("DataCore file not found in P4k; tried Data\\Game2.dcb and Data\\Game.dcb")]
    DataCoreNotFound,

    #[error("unsupported diff source: {0}")]
    UnsupportedSource(PathBuf),
}
