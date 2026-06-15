//! `Engine` — composes an `Embedder` with a `Store` to provide the public
//! `add_passage` / `search` API consumed by the MCP server.
//!
//! # Write path
//!
//! [`Engine::add_passage`] is the minimal seeding / write path for build step 2.
//! The full EPUB/PDF ingestion pipeline (`athenaeum-ingest`, build step 3) will
//! call this method (or a batch form) per extracted chunk. No changes to the
//! `Engine` API are expected when that pipeline lands.
//!
//! # Constructors
//!
//! - [`Engine::new`] — builds an [`OllamaEmbedder`] from a [`Config`] and
//!   opens the production [`Store`]. Use in the real binary.
//! - [`Engine::with_parts`] — injects any `E: Embedder` and a pre-opened
//!   `Store`. Use in tests with [`FakeEmbedder`](crate::embed::FakeEmbedder).

use crate::config::Config;
use crate::embed::{Embedder, OllamaEmbedder};
use crate::error::CoreError;
use crate::search::SearchHit;
use crate::store::{Passage, Store};

/// Combines an embedder and a vector store into the core search interface.
pub struct Engine<E: Embedder> {
    embedder: E,
    store: Store,
    /// The embedding dimension. Stored for future batch-insert validation and
    /// for the build-step-3 ingestion pipeline.
    #[allow(dead_code)]
    dim: usize,
}

impl Engine<OllamaEmbedder> {
    /// Build an `Engine` from the given `Config`, connecting to the default
    /// Ollama instance and the LanceDB path specified in the config.
    pub async fn new(config: Config) -> Result<Self, CoreError> {
        let embedder = OllamaEmbedder::new(&config.ollama_url, &config.embed_model, config.embed_dim);
        let store = Store::open(&config.db_path, &config.table_name, config.embed_dim).await?;
        Ok(Self {
            embedder,
            store,
            dim: config.embed_dim,
        })
    }
}

impl<E: Embedder> Engine<E> {
    /// Construct an `Engine` from pre-built parts.
    ///
    /// Intended for test injection of `FakeEmbedder` with a `tempdir` store.
    pub fn with_parts(embedder: E, store: Store, dim: usize) -> Self {
        Self { embedder, store, dim }
    }

    /// Embed a single passage and insert it into the store.
    ///
    /// Returns `CoreError::EmptyInput` if `text` is empty (propagated from the
    /// embedder).
    ///
    /// This is the minimal write path for build step 2. The full ingestion
    /// pipeline (build step 3) will call this or a batch variant per chunk.
    pub async fn add_passage(
        &self,
        source: &str,
        location: &str,
        text: &str,
    ) -> Result<(), CoreError> {
        let vectors = self.embedder.embed(&[text.to_string()]).await?;
        let passage = Passage {
            source: source.to_string(),
            location: location.to_string(),
            text: text.to_string(),
        };
        self.store.add(&vectors, &[passage]).await
    }

    /// Embed `query` and return the top-`k` nearest passages as `SearchHit`s.
    ///
    /// The `score` field is `(1.0 - cosine_distance).clamp(0.0, 1.0)`.
    pub async fn search(&self, query: &str, k: usize) -> Result<Vec<SearchHit>, CoreError> {
        let query_vecs = self.embedder.embed(&[query.to_string()]).await?;
        let query_vec = &query_vecs[0];

        let raw = self.store.search(query_vec, k).await?;

        let hits = raw
            .into_iter()
            .map(|(passage, distance)| SearchHit {
                source: passage.source,
                location: passage.location,
                text: passage.text,
                score: (1.0_f32 - distance).clamp(0.0, 1.0),
            })
            .collect();

        Ok(hits)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::FakeEmbedder;

    #[tokio::test]
    async fn empty_engine_search_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let results = engine.search("anything", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn add_and_search_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

        engine
            .add_passage("book-a.epub", "p. 10", "the quick brown fox")
            .await
            .unwrap();
        engine
            .add_passage("book-b.epub", "p. 20", "pack my box with five dozen liquor jugs")
            .await
            .unwrap();

        // Search with text equal to the first passage — FakeEmbedder is deterministic,
        // so the embedding will be identical and it should rank first.
        let hits = engine
            .search("the quick brown fox", 2)
            .await
            .unwrap();

        assert!(!hits.is_empty());
        assert_eq!(hits[0].source, "book-a.epub");
        assert_eq!(hits[0].location, "p. 10");
        assert_eq!(hits[0].text, "the quick brown fox");
        assert!(
            hits[0].score >= 0.0 && hits[0].score <= 1.0,
            "score out of range: {}",
            hits[0].score
        );
    }

    #[tokio::test]
    async fn add_empty_text_returns_empty_input_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let result = engine.add_passage("src", "loc", "").await;
        assert!(matches!(result, Err(CoreError::EmptyInput)));
    }
}
