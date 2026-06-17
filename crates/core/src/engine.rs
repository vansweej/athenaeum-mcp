//! `Engine` — composes an `Embedder` with a `Store` to provide the public
//! `upsert_passages` / `search` API consumed by the MCP server.
//!
//! # Write path
//!
//! [`Engine::upsert_passages`] is the dedup-aware write path for ingestion.
//! It embeds passages, then performs a document-level upsert (delete old +
//! insert new) keyed on `doc_id`. The [`ingest`] function in the `athenaeum-ingest`
//! crate calls this method per file. Tests may call it directly.
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

#[cfg(not(tarpaulin_include))]
impl Engine<OllamaEmbedder> {
    /// Build an `Engine` from the given `Config`, connecting to the default
    /// Ollama instance and the LanceDB path specified in the config.
    pub async fn new(config: Config) -> Result<Self, CoreError> {
        let embedder =
            OllamaEmbedder::new(&config.ollama_url, &config.embed_model, config.embed_dim);
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
        Self {
            embedder,
            store,
            dim,
        }
    }

    /// Batch embed and upsert passages for a single document.
    ///
    /// This is the dedup-aware primary write path. It replaces all prior stored
    /// chunks for `doc_id` with the newly embedded `passages`. If `passages` is
    /// empty, it still performs the delete (clearing any existing rows for this
    /// doc) and returns `Ok(0)`.
    ///
    /// # Errors
    /// Returns `CoreError::EmptyInput` if any text field is empty (propagated
    /// from the embedder).
    pub async fn upsert_passages(
        &self,
        doc_id: &str,
        passages: &[(String, String, String)], // (source, location, text)
    ) -> Result<usize, CoreError> {
        if passages.is_empty() {
            // Still call upsert_doc with empty data to clear any prior rows.
            self.store.upsert_doc(doc_id, &[], &[]).await?;
            return Ok(0);
        }

        let texts: Vec<String> = passages.iter().map(|(_, _, text)| text.clone()).collect();
        let vectors = self.embedder.embed(&texts).await?;

        let passages_vec: Vec<Passage> = passages
            .iter()
            .map(|(source, location, text)| Passage {
                doc_id: doc_id.to_string(),
                source: source.clone(),
                location: location.clone(),
                text: text.clone(),
            })
            .collect();

        self.store
            .upsert_doc(doc_id, &vectors, &passages_vec)
            .await?;
        Ok(passages.len())
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
            .upsert_passages(
                "book-a.epub",
                &[(
                    "book-a.epub".to_string(),
                    "p. 10".to_string(),
                    "the quick brown fox".to_string(),
                )],
            )
            .await
            .unwrap();
        engine
            .upsert_passages(
                "book-b.epub",
                &[(
                    "book-b.epub".to_string(),
                    "p. 20".to_string(),
                    "pack my box with five dozen liquor jugs".to_string(),
                )],
            )
            .await
            .unwrap();

        // Search with text equal to the first passage — FakeEmbedder is deterministic,
        // so the embedding will be identical and it should rank first.
        let hits = engine.search("the quick brown fox", 2).await.unwrap();

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
    async fn batch_add_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

        let passages = vec![
            (
                "book-a.epub".to_string(),
                "p. 10".to_string(),
                "the quick brown fox".to_string(),
            ),
            (
                "book-b.epub".to_string(),
                "p. 20".to_string(),
                "pack my box with five dozen liquor jugs".to_string(),
            ),
            (
                "book-c.epub".to_string(),
                "p. 30".to_string(),
                "lazy dog".to_string(),
            ),
        ];

        let count = engine
            .upsert_passages("book-c-doc", &passages)
            .await
            .unwrap();
        assert_eq!(count, 3);

        let hits = engine.search("the quick brown fox", 1).await.unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].text, "the quick brown fox");

        let empty_res = engine.upsert_passages("empty-doc", &[]).await.unwrap();
        assert_eq!(empty_res, 0);
    }

    #[tokio::test]
    async fn upsert_replaces_prior_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

        // First upsert: two passages for doc "d1"
        let first = vec![
            (
                "book-a.epub".to_string(),
                "p. 1".to_string(),
                "first chunk".to_string(),
            ),
            (
                "book-a.epub".to_string(),
                "p. 2".to_string(),
                "second chunk".to_string(),
            ),
        ];
        engine.upsert_passages("d1", &first).await.unwrap();

        // Second upsert: one different passage for same doc "d1"
        let second = vec![(
            "book-a.epub".to_string(),
            "p. 3".to_string(),
            "replacement chunk".to_string(),
        )];
        engine.upsert_passages("d1", &second).await.unwrap();

        // Search should only find the replacement chunk (old ones were deleted)
        let hits = engine.search("replacement chunk", 10).await.unwrap();
        assert_eq!(hits.len(), 1, "should have exactly 1 passage after upsert");
        assert_eq!(hits[0].text, "replacement chunk");

        // The old chunks should not appear — with FakeEmbedder a non-empty table
        // always returns the closest row, so verify the old texts are gone, not
        // that no results are returned.
        let old_hits = engine.search("first chunk", 10).await.unwrap();
        assert_eq!(old_hits.len(), 1, "table has 1 row total after upsert");
        assert_eq!(
            old_hits[0].text, "replacement chunk",
            "returned passage must be the replacement, not the old chunk"
        );
    }
}
