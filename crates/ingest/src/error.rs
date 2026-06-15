use thiserror::Error;

/// Errors produced by the athenaeum-ingest crate.
#[derive(Debug, Error)]
pub enum IngestError {
    #[error("unsupported file type: {0}")]
    UnsupportedFileType(String),

    #[error("parse failed: {0}")]
    ParseFailed(String),

    #[error("not yet implemented")]
    NotImplemented,
}
