//! Embedding abstractions and the Ollama batch embedder.
//!
//! The `Embedder` trait lets tests inject a deterministic `FakeEmbedder`
//! without a live Ollama instance.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Produce dense embeddings for a slice of input strings.
///
/// Returns one embedding vector per input, in the same order as the inputs.
/// Every returned vector has the same length (the model's embedding dimension).
/// Implementations must return `CoreError::EmptyInput` if any input string is
/// empty (or the slice itself is empty).
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, CoreError>;
}

// ─── Ollama embedder ──────────────────────────────────────────────────────────

/// Request body for `POST /api/embed`.
#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

/// Response body from `POST /api/embed`.
#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Embeds text using Ollama's batch `POST /api/embed` endpoint.
///
/// Targets the Ollama batch API so the step-3 ingestion pipeline can call
/// this with multiple chunks per request without API migration cost.
pub struct OllamaEmbedder {
    client: reqwest::Client,
    url: String,
    model: String,
    dim: usize,
}

impl OllamaEmbedder {
    /// Create a new embedder pointing at the given Ollama `url` (no trailing
    /// slash), using the specified `model` and expected vector `dim`.
    pub fn new(url: impl Into<String>, model: impl Into<String>, dim: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.into(),
            model: model.into(),
            dim,
        }
    }
}

#[async_trait]
impl Embedder for OllamaEmbedder {
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        if inputs.is_empty() || inputs.iter().any(|s| s.is_empty()) {
            return Err(CoreError::EmptyInput);
        }

        let endpoint = format!("{}/api/embed", self.url);
        let body = EmbedRequest {
            model: &self.model,
            input: inputs,
        };

        let response = self
            .client
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(CoreError::Http(format!("{status}: {text}")));
        }

        let parsed: EmbedResponse = response
            .json()
            .await
            .map_err(|e| CoreError::Http(e.to_string()))?;

        if parsed.embeddings.len() != inputs.len() {
            return Err(CoreError::DimensionMismatch {
                expected: inputs.len(),
                actual: parsed.embeddings.len(),
            });
        }

        for vec in &parsed.embeddings {
            if vec.len() != self.dim {
                return Err(CoreError::DimensionMismatch {
                    expected: self.dim,
                    actual: vec.len(),
                });
            }
        }

        Ok(parsed.embeddings)
    }
}

// ─── Test utilities ───────────────────────────────────────────────────────────

#[cfg(any(test, feature = "test-support"))]
pub struct FakeEmbedder {
    pub dim: usize,
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl Embedder for FakeEmbedder {
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        if inputs.is_empty() || inputs.iter().any(|s| s.is_empty()) {
            return Err(CoreError::EmptyInput);
        }

        let vecs = inputs
            .iter()
            .map(|s| {
                let sum: u64 = s.bytes().map(u64::from).sum();
                (0..self.dim)
                    .map(|i| ((sum + i as u64) % 256) as f32 / 255.0)
                    .collect()
            })
            .collect();

        Ok(vecs)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_embedder_returns_correct_dim() {
        let embedder = FakeEmbedder { dim: 4 };
        let result = embedder
            .embed(&["hello".to_string(), "world".to_string()])
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 4);
        assert_eq!(result[1].len(), 4);
    }

    #[tokio::test]
    async fn fake_embedder_is_deterministic() {
        let embedder = FakeEmbedder { dim: 8 };
        let a = embedder.embed(&["test".to_string()]).await.unwrap();
        let b = embedder.embed(&["test".to_string()]).await.unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn fake_embedder_different_strings_differ() {
        let embedder = FakeEmbedder { dim: 8 };
        let a = embedder.embed(&["hello".to_string()]).await.unwrap();
        let b = embedder.embed(&["world".to_string()]).await.unwrap();
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn fake_embedder_empty_input_errors() {
        let embedder = FakeEmbedder { dim: 4 };
        let result = embedder.embed(&["".to_string()]).await;
        assert!(matches!(result, Err(CoreError::EmptyInput)));
    }

    #[tokio::test]
    async fn fake_embedder_empty_slice_errors() {
        let embedder = FakeEmbedder { dim: 4 };
        let result = embedder.embed(&[]).await;
        assert!(matches!(result, Err(CoreError::EmptyInput)));
    }

    /// Integration test — requires a running Ollama with `nomic-embed-text`.
    #[tokio::test]
    #[ignore]
    async fn ollama_embedder_returns_768_dim_vectors() {
        let embedder = OllamaEmbedder::new("http://localhost:11434", "nomic-embed-text", 768);
        let result = embedder
            .embed(&["hello world".to_string(), "foo bar".to_string()])
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 768);
        assert_eq!(result[1].len(), 768);
    }
}
