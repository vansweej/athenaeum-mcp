//! LanceDB-backed passage store with cosine-distance vector search.
//!
//! The raw `text` of every passage is stored alongside its embedding vector,
//! satisfying the "raw source text" mandate: retrieved hits always carry the
//! original passage, not a lossy reconstruction.

use std::path::Path;
use std::sync::Arc;

use arrow_array::cast::AsArray;
use arrow_array::types::Float32Type;
use arrow_array::{
    array::ArrayRef,
    builder::{FixedSizeListBuilder, Float32Builder, StringBuilder},
    RecordBatch,
};
use arrow_schema::{DataType, Field, Fields, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, DistanceType, Table};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

// ─── SQL helpers ──────────────────────────────────────────────────────────────

/// Escape single quotes for safe insertion in a LanceDB SQL filter predicate.
///
/// Replaces every `'` with `''` and wraps the result in single quotes, making
/// the string safe for use in a `DELETE WHERE doc_id = …` filter. Required
/// because `doc_id` is a file path and filenames like `O'Reilly - SICP.pdf`
/// will otherwise break the predicate at runtime.
fn sql_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

// ─── Row type ─────────────────────────────────────────────────────────────────

/// A text passage stored in the vector database.
///
/// The raw `text` is always stored alongside the vector so that search results
/// carry the original passage without further retrieval. The `doc_id` identifies
/// the source document for dedup/upsert — all chunks from one file share it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Passage {
    /// Absolute path of the source document (the dedup key).
    pub doc_id: String,
    /// Title or path of the source document.
    pub source: String,
    /// Human-readable location within the source (e.g. "p. 42", "§3.2").
    pub location: String,
    /// The raw passage text that was embedded.
    pub text: String,
}

// ─── Schema ───────────────────────────────────────────────────────────────────

fn schema(dim: usize) -> Arc<Schema> {
    let vector_field = Field::new("item", DataType::Float32, true);
    // doc_id must remain first to match RecordBatch column order in add()
    let fields = vec![
        Field::new("doc_id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(Arc::new(vector_field), dim as i32),
            false,
        ),
        Field::new("source", DataType::Utf8, false),
        Field::new("location", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
    ];
    Arc::new(Schema::new(Fields::from(fields)))
}

// ─── Store ────────────────────────────────────────────────────────────────────

/// LanceDB connection wrapper providing passage insert and vector search.
pub struct Store {
    table_name: String,
    dim: usize,
    conn: Connection,
}

impl Store {
    /// Open (or create) the passage table at `db_path`.
    ///
    /// If the table does not yet exist it is created empty with the correct
    /// schema. All LanceDB errors are mapped to `CoreError::StoreFailed`.
    pub async fn open(db_path: &Path, table_name: &str, dim: usize) -> Result<Self, CoreError> {
        let path = db_path
            .to_str()
            .ok_or_else(|| CoreError::StoreFailed("non-UTF-8 db_path".to_string()))?;

        let conn = lancedb::connect(path)
            .execute()
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        let existing = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        if !existing.contains(&table_name.to_string()) {
            conn.create_empty_table(table_name, schema(dim))
                .execute()
                .await
                .map_err(|e| CoreError::StoreFailed(e.to_string()))?;
        }

        Ok(Self {
            table_name: table_name.to_string(),
            dim,
            conn,
        })
    }

    async fn table(&self) -> Result<Table, CoreError> {
        self.conn
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))
    }

    /// Insert a batch of passages with their embedding vectors.
    ///
    /// Both slices must have equal length. Every vector must have length `dim`.
    pub async fn add(&self, vectors: &[Vec<f32>], passages: &[Passage]) -> Result<(), CoreError> {
        if vectors.len() != passages.len() {
            return Err(CoreError::StoreFailed(
                "vectors and passages slices have different lengths".to_string(),
            ));
        }

        for v in vectors {
            if v.len() != self.dim {
                return Err(CoreError::DimensionMismatch {
                    expected: self.dim,
                    actual: v.len(),
                });
            }
        }

        let schema = schema(self.dim);

        // Build vector column (FixedSizeList<Float32>)
        let mut vector_builder = FixedSizeListBuilder::new(Float32Builder::new(), self.dim as i32);
        for v in vectors {
            for &f in v {
                vector_builder.values().append_value(f);
            }
            vector_builder.append(true);
        }
        let vector_array = Arc::new(vector_builder.finish()) as ArrayRef;

        // Build string columns
        let mut doc_id_builder = StringBuilder::new();
        let mut source_builder = StringBuilder::new();
        let mut location_builder = StringBuilder::new();
        let mut text_builder = StringBuilder::new();
        for p in passages {
            doc_id_builder.append_value(&p.doc_id);
            source_builder.append_value(&p.source);
            location_builder.append_value(&p.location);
            text_builder.append_value(&p.text);
        }

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(doc_id_builder.finish()) as ArrayRef,
                vector_array,
                Arc::new(source_builder.finish()) as ArrayRef,
                Arc::new(location_builder.finish()) as ArrayRef,
                Arc::new(text_builder.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        let table = self.table().await?;
        table
            .add(vec![batch])
            .execute()
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        Ok(())
    }

    /// Replace all stored passages for `doc_id` with new ones (delete-then-add upsert).
    ///
    /// Removes any existing rows with matching `doc_id`, then inserts the new
    /// `passages` with their `vectors`. All passages MUST carry the same `doc_id`
    /// as the first argument. The delete predicate is SQL-quoted so paths
    /// containing apostrophes (e.g. `O'Reilly - SICP.pdf`) are safe.
    pub async fn upsert_doc(
        &self,
        doc_id: &str,
        vectors: &[Vec<f32>],
        passages: &[Passage],
    ) -> Result<(), CoreError> {
        let table = self.table().await?;

        // Delete existing rows for this doc_id
        let predicate = format!("doc_id = {}", sql_quote(doc_id));
        table
            .delete(&predicate)
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        // Insert new rows
        self.add(vectors, passages).await
    }

    /// Search for the `k` passages nearest to `vector` using cosine distance.
    ///
    /// An empty table returns `Ok(vec![])` rather than an error.
    pub async fn search(&self, vector: &[f32], k: usize) -> Result<Vec<(Passage, f32)>, CoreError> {
        let table = self.table().await?;

        let row_count = table
            .count_rows(None)
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        if row_count == 0 {
            return Ok(vec![]);
        }

        let stream = table
            .query()
            .nearest_to(vector)
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?
            .distance_type(DistanceType::Cosine)
            .limit(k)
            .execute()
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| CoreError::StoreFailed(e.to_string()))?;

        let mut results = Vec::new();
        for batch in batches {
            let n = batch.num_rows();
            if n == 0 {
                continue;
            }

            let doc_id_col = batch
                .column_by_name("doc_id")
                .ok_or_else(|| CoreError::StoreFailed("missing 'doc_id' column".to_string()))?
                .as_string::<i32>();

            let source_col = batch
                .column_by_name("source")
                .ok_or_else(|| CoreError::StoreFailed("missing 'source' column".to_string()))?
                .as_string::<i32>();

            let location_col = batch
                .column_by_name("location")
                .ok_or_else(|| CoreError::StoreFailed("missing 'location' column".to_string()))?
                .as_string::<i32>();

            let text_col = batch
                .column_by_name("text")
                .ok_or_else(|| CoreError::StoreFailed("missing 'text' column".to_string()))?
                .as_string::<i32>();

            let distance_col = batch
                .column_by_name("_distance")
                .ok_or_else(|| CoreError::StoreFailed("missing '_distance' column".to_string()))?
                .as_primitive::<Float32Type>();

            for i in 0..n {
                let passage = Passage {
                    doc_id: doc_id_col.value(i).to_string(),
                    source: source_col.value(i).to_string(),
                    location: location_col.value(i).to_string(),
                    text: text_col.value(i).to_string(),
                };
                let distance = distance_col.value(i);
                results.push((passage, distance));
            }
        }

        Ok(results)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── sql_quote tests ───────────────────────────────────────────────────────

    #[test]
    fn test_sql_quote_plain_string() {
        assert_eq!(sql_quote("abc"), "'abc'");
    }

    #[test]
    fn test_sql_quote_with_apostrophe() {
        assert_eq!(sql_quote("O'Reilly"), "'O''Reilly'");
    }

    #[test]
    fn test_sql_quote_multiple_apostrophes() {
        assert_eq!(sql_quote("a'b'c"), "'a''b''c'");
    }

    // ─── Store tests ───────────────────────────────────────────────────────────

    fn make_vector(base: f32, dim: usize) -> Vec<f32> {
        (0..dim).map(|i| (base + i as f32).sin()).collect()
    }

    #[tokio::test]
    async fn empty_store_search_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 8).await.unwrap();
        let q = make_vector(0.0, 8);
        let results = store.search(&q, 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn add_and_search_returns_nearest() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();

        // passage A: mostly 1.0 values
        let vec_a: Vec<f32> = (0..768).map(|i| if i < 700 { 1.0 } else { 0.0 }).collect();
        // passage B: mostly 0.0 values
        let vec_b: Vec<f32> = (0..768).map(|i| if i < 700 { 0.0 } else { 1.0 }).collect();

        let passage_a = Passage {
            doc_id: "book-a.epub".to_string(),
            source: "book-a.epub".to_string(),
            location: "p. 1".to_string(),
            text: "passage a text".to_string(),
        };
        let passage_b = Passage {
            doc_id: "book-b.epub".to_string(),
            source: "book-b.epub".to_string(),
            location: "p. 2".to_string(),
            text: "passage b text".to_string(),
        };

        store
            .add(&[vec_a.clone(), vec_b], &[passage_a, passage_b])
            .await
            .unwrap();

        // query close to vec_a
        let query: Vec<f32> = (0..768).map(|i| if i < 700 { 0.99 } else { 0.0 }).collect();
        let results = store.search(&query, 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.source, "book-a.epub");
        let score = (1.0_f32 - results[0].1).clamp(0.0, 1.0);
        assert!(score >= 0.0 && score <= 1.0, "score out of range: {score}");
    }

    #[tokio::test]
    async fn upsert_doc_replaces_rows() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 8).await.unwrap();

        // Add two passages for docX
        let vec_a = make_vector(0.0, 8);
        let vec_b = make_vector(1.0, 8);
        let passages = vec![
            Passage {
                doc_id: "docX".into(),
                source: "a".into(),
                location: "p.1".into(),
                text: "original a".into(),
            },
            Passage {
                doc_id: "docX".into(),
                source: "b".into(),
                location: "p.2".into(),
                text: "original b".into(),
            },
        ];
        store
            .upsert_doc("docX", &[vec_a, vec_b], &passages)
            .await
            .unwrap();

        // Upsert again — one replacement passage
        let vec_c = make_vector(2.0, 8);
        let replacement = vec![Passage {
            doc_id: "docX".into(),
            source: "c".into(),
            location: "p.3".into(),
            text: "replacement".into(),
        }];
        store
            .upsert_doc("docX", &[vec_c], &replacement)
            .await
            .unwrap();

        // Only one row should remain
        let results = store.search(&make_vector(2.0, 8), 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.text, "replacement");
    }

    #[tokio::test]
    async fn add_vectors_passages_length_mismatch_errors() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 8).await.unwrap();

        // 2 passages but only 1 vector
        let vecs = vec![make_vector(0.0, 8)];
        let passages = vec![
            Passage {
                doc_id: "a".into(),
                source: "a".into(),
                location: "p1".into(),
                text: "a".into(),
            },
            Passage {
                doc_id: "b".into(),
                source: "b".into(),
                location: "p2".into(),
                text: "b".into(),
            },
        ];

        let result = store.add(&vecs, &passages).await;
        assert!(matches!(result, Err(CoreError::StoreFailed(_))));
    }

    #[tokio::test]
    async fn add_vector_wrong_dim_errors() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 8).await.unwrap();

        // Vector dim 4 but store expects 8
        let vecs = vec![make_vector(0.0, 4)];
        let passages = vec![Passage {
            doc_id: "a".into(),
            source: "a".into(),
            location: "p1".into(),
            text: "a".into(),
        }];

        let result = store.add(&vecs, &passages).await;
        assert!(matches!(result, Err(CoreError::DimensionMismatch { .. })));
    }

    #[tokio::test]
    async fn upsert_doc_quotes_apostrophe_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 8).await.unwrap();

        // Use a doc_id with an apostrophe (non-negotiable #1)
        let apostrophe_doc = "O'Reilly - SICP.pdf";
        let vec_a = make_vector(0.0, 8);
        let passages_a = vec![Passage {
            doc_id: apostrophe_doc.into(),
            source: "book".into(),
            location: "p.1".into(),
            text: "original".into(),
        }];
        store
            .upsert_doc(apostrophe_doc, &[vec_a], &passages_a)
            .await
            .unwrap();

        // Upsert again with the same apostrophe doc_id
        let vec_b = make_vector(1.0, 8);
        let passages_b = vec![Passage {
            doc_id: apostrophe_doc.into(),
            source: "book".into(),
            location: "p.2".into(),
            text: "replacement".into(),
        }];
        store
            .upsert_doc(apostrophe_doc, &[vec_b], &passages_b)
            .await
            .unwrap();

        // Only the replacement row
        let results = store.search(&make_vector(1.0, 8), 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.text, "replacement");
    }
}
