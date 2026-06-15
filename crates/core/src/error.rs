use thiserror::Error;

/// Errors produced by the athenaeum-core crate.
///
/// Variants cover the embedding path (Ollama HTTP, dimension validation,
/// empty input), the storage path (LanceDB operations), and a stub for
/// not-yet-implemented functionality.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("embedding failed: {0}")]
    EmbeddingFailed(String),

    #[error("store operation failed: {0}")]
    StoreFailed(String),

    #[error("not yet implemented")]
    NotImplemented,

    #[error("ollama request failed: {0}")]
    Http(String),

    #[error("embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("input text was empty")]
    EmptyInput,
}
