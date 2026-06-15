use serde::{Deserialize, Serialize};

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
