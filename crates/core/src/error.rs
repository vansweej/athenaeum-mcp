use thiserror::Error;

/// Errors produced by the athenaeum-core crate.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("embedding failed: {0}")]
    EmbeddingFailed(String),

    #[error("store operation failed: {0}")]
    StoreFailed(String),

    #[error("not yet implemented")]
    NotImplemented,
}
