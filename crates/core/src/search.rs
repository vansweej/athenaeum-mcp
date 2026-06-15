use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// A single cited passage returned by a search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Human-readable source identifier (e.g. book title, paper DOI).
    pub source: String,
    /// Location within the source (e.g. "Chapter 3 § 2, p. 47").
    pub location: String,
    /// The raw passage text.
    pub text: String,
    /// Cosine similarity score in [0, 1].
    pub score: f32,
}

/// Embed `text` into a vector using the configured embedding model.
///
/// # Errors
/// Returns [`CoreError::NotImplemented`] until the Ollama client is wired.
pub async fn embed(text: &str) -> Result<Vec<f32>, CoreError> {
    let _ = text;
    Err(CoreError::NotImplemented)
}

/// Search the vector store for the `k` nearest passages to `query`.
///
/// # Errors
/// Returns [`CoreError::NotImplemented`] until LanceDB is wired.
pub async fn search(query: &str, k: usize) -> Result<Vec<SearchHit>, CoreError> {
    let _ = (query, k);
    Err(CoreError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn embed_returns_not_implemented() {
        let result = embed("hello").await;
        assert!(matches!(result, Err(CoreError::NotImplemented)));
    }

    #[tokio::test]
    async fn search_returns_not_implemented() {
        let result = search("hello", 5).await;
        assert!(matches!(result, Err(CoreError::NotImplemented)));
    }
}
