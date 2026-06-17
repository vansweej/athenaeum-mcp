use athenaeum_core::CoreError;
use thiserror::Error;

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
            CoreError::DimensionMismatch { expected, actual } => IngestError::EmbedFailed(format!(
                "embedding dimension mismatch: expected {}, got {}",
                expected, actual
            )),
            CoreError::NotImplemented => IngestError::NotImplemented,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use athenaeum_core::CoreError;

    #[test]
    fn from_core_error_maps_all_variants() {
        // CoreError::Http → IngestError::EmbedFailed
        let err: IngestError = CoreError::Http("timeout".into()).into();
        assert!(matches!(err, IngestError::EmbedFailed(_)));

        // CoreError::EmptyInput → IngestError::ParseFailed
        let err: IngestError = CoreError::EmptyInput.into();
        assert!(matches!(err, IngestError::ParseFailed(_)));

        // CoreError::StoreFailed → IngestError::StoreFailed
        let err: IngestError = CoreError::StoreFailed("disk full".into()).into();
        assert!(matches!(err, IngestError::StoreFailed(_)));

        // CoreError::EmbeddingFailed → IngestError::EmbedFailed
        let err: IngestError = CoreError::EmbeddingFailed("model error".into()).into();
        assert!(matches!(err, IngestError::EmbedFailed(_)));

        // CoreError::DimensionMismatch → IngestError::EmbedFailed with formatted message
        let err: IngestError = CoreError::DimensionMismatch {
            expected: 768,
            actual: 4,
        }
        .into();
        assert!(matches!(err, IngestError::EmbedFailed(_)));
        assert!(err.to_string().contains("768"));

        // CoreError::NotImplemented → IngestError::NotImplemented
        let err: IngestError = CoreError::NotImplemented.into();
        assert!(matches!(err, IngestError::NotImplemented));
    }
}
