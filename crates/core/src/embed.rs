//! Embedding abstractions and the Ollama batch embedder.
//!
//! The `Embedder` trait lets tests inject a deterministic `FakeEmbedder`
//! without a live Ollama instance.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::{DEFAULT_EMBED_CONNECT_TIMEOUT, DEFAULT_EMBED_TIMEOUT};
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
    ///
    /// Uses the crate default timeouts (`DEFAULT_EMBED_TIMEOUT` /
    /// `DEFAULT_EMBED_CONNECT_TIMEOUT`), which also back `Config`.
    pub fn new(url: impl Into<String>, model: impl Into<String>, dim: usize) -> Self {
        Self::with_timeouts(
            url,
            model,
            dim,
            DEFAULT_EMBED_TIMEOUT,
            DEFAULT_EMBED_CONNECT_TIMEOUT,
        )
    }

    /// Create a new embedder with explicit request and connect timeouts.
    ///
    /// `timeout` is the total deadline for the HTTP request (including the
    /// response body). `connect_timeout` is the TCP connect deadline.
    ///
    /// # Panics
    ///
    /// Panics if `reqwest` fails to initialise its TLS backend — this is an
    /// environment fault, not a runtime condition.
    pub fn with_timeouts(
        url: impl Into<String>,
        model: impl Into<String>,
        dim: usize,
        timeout: Duration,
        connect_timeout: Duration,
    ) -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(connect_timeout)
                .timeout(timeout)
                .build()
                .expect("failed to build reqwest client"),
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

/// An embedder that fails on a configurable call, for testing error-recovery.
///
/// Behaves exactly like [`FakeEmbedder`] except that the Nth call to `embed()`
/// returns `Err(CoreError::Http("simulated embed failure".to_string()))`.
/// Use this to prove that the ingestion loop records a failure and continues
/// rather than aborting (non-negotiable #2).
#[cfg(any(test, feature = "test-support"))]
pub struct FailingEmbedder {
    pub dim: usize,
    pub fail_on_call: usize,
    pub counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(any(test, feature = "test-support"))]
impl FailingEmbedder {
    /// Create a new `FailingEmbedder` that fails on the `fail_on_call`-th
    /// `embed()` call (1-indexed).
    pub fn new(dim: usize, fail_on_call: usize) -> Self {
        Self {
            dim,
            fail_on_call,
            counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl Embedder for FailingEmbedder {
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        if inputs.is_empty() || inputs.iter().any(|s| s.is_empty()) {
            return Err(CoreError::EmptyInput);
        }

        let call_count = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        if call_count == self.fail_on_call {
            return Err(CoreError::Http("simulated embed failure".to_string()));
        }

        // Same deterministic logic as FakeEmbedder
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

    // ─── FailingEmbedder tests ─────────────────────────────────────────────────

    // ─── OllamaEmbedder wiremock tests ─────────────────────────────────────────

    #[tokio::test]
    async fn ollama_embedder_success() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let mock_server = MockServer::start().await;

        // Build a 4-dimensional embedding response
        let response_body = serde_json::json!({
            "embeddings": [[0.1, 0.2, 0.3, 0.4]]
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let embedder = OllamaEmbedder::new(mock_server.uri(), "test-model", 4);
        let result = embedder.embed(&["hello".to_string()]).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 4);
        assert!((result[0][0] - 0.1).abs() < 1e-6);
    }

    #[tokio::test]
    async fn ollama_embedder_http_error() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&mock_server)
            .await;

        let embedder = OllamaEmbedder::new(mock_server.uri(), "test-model", 4);
        let result = embedder.embed(&["hello".to_string()]).await;
        assert!(matches!(result, Err(CoreError::Http(_))));
    }

    #[tokio::test]
    async fn ollama_embedder_count_mismatch() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "embeddings": [[0.1, 0.2, 0.3, 0.4]]  // only 1, but we sent 2
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let embedder = OllamaEmbedder::new(mock_server.uri(), "test-model", 4);
        let result = embedder
            .embed(&["hello".to_string(), "world".to_string()])
            .await;
        assert!(matches!(result, Err(CoreError::DimensionMismatch { .. })));
    }

    #[tokio::test]
    async fn ollama_embedder_dim_mismatch() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "embeddings": [[0.1, 0.2, 0.3]]  // dim 3, but expected dim 4
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let embedder = OllamaEmbedder::new(mock_server.uri(), "test-model", 4);
        let result = embedder.embed(&["hello".to_string()]).await;
        assert!(matches!(result, Err(CoreError::DimensionMismatch { .. })));
    }

    #[tokio::test]
    async fn ollama_embedder_empty_input() {
        use wiremock::MockServer;

        let mock_server = MockServer::start().await;
        let embedder = OllamaEmbedder::new(mock_server.uri(), "test-model", 4);

        let result = embedder.embed(&[] as &[String]).await;
        assert!(matches!(result, Err(CoreError::EmptyInput)));

        let result = embedder.embed(&["".to_string()]).await;
        assert!(matches!(result, Err(CoreError::EmptyInput)));
    }

    #[tokio::test]
    async fn ollama_embedder_times_out_on_slow_response() {
        use std::time::Duration;
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let mock_server = MockServer::start().await;

        // Valid 4-dimensional body, but delayed by 2 s.
        let response_body = serde_json::json!({
            "embeddings": [[0.1, 0.2, 0.3, 0.4]]
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&response_body)
                    .set_delay(Duration::from_secs(2)),
            )
            .mount(&mock_server)
            .await;

        let embedder = OllamaEmbedder::with_timeouts(
            mock_server.uri(),
            "test-model",
            4,
            Duration::from_millis(200), // request timeout well under 2 s delay
            Duration::from_secs(5),
        );
        let result = embedder.embed(&["hello".to_string()]).await;
        assert!(
            matches!(result, Err(CoreError::Http(_))),
            "expected Http error, got {result:?}"
        );
    }

    #[tokio::test]
    async fn failing_embedder_fails_on_nth_call() {
        let embedder = FailingEmbedder::new(4, 2);

        // First call succeeds
        let result = embedder.embed(&["hello".to_string()]).await;
        assert!(result.is_ok());

        // Second call fails
        let result = embedder.embed(&["world".to_string()]).await;
        assert!(matches!(result, Err(CoreError::Http(_))));

        // Third call succeeds again
        let result = embedder.embed(&["third".to_string()]).await;
        assert!(result.is_ok());
    }
}
