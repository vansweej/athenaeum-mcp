use thiserror::Error;
use athenaeum_core::CoreError;

/// Errors produced by the athenaeum-ingest crate.
#[derive(Debug, Error)]
pub enum IngestError {
    #[error("unsupported file type: {0}")]
    UnsupportedFileType(String),

    #[error("parse failed: {0}")]
    ParseFailed(String),

    #[error("io error: {0}")]
    IoFailed(String),

    #[error("embedding failed: {0}")]
    EmbedFailed(String),

    #[error("store operation failed: {0}")]
    StoreFailed(String),

    #[error("not yet implemented")]
    NotImplemented,
}

impl From<athenaeum_core::CoreError> for IngestError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Http(msg) => IngestError::EmbedFailed(msg),
            CoreError::EmptyInput => IngestError::ParseFailed("empty input".to_string()),
            CoreError::StoreFailed(msg) => IngestError::StoreFailed(msg),
            CoreError::EmbeddingFailed(msg) => IngestError::EmbedFailed(msg),
            CoreError::DimensionMismatch { expected, actual } => {
                IngestError::EmbedFailed(format!(
                    "embedding dimension mismatch: expected {}, got {}",
                    expected, actual
                ))
            }
            CoreError::NotImplemented => IngestError::NotImplemented,
        }
    }
}
