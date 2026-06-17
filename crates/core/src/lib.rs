//! `athenaeum-core` — Ollama embedding, LanceDB storage, and the
//! `search(query, k)` / `upsert_passages` core used by the MCP server and
//! the ingestion pipeline.

pub mod config;
pub mod embed;
pub mod engine;
pub mod error;
pub mod search;
pub mod store;

pub use config::Config;
pub use embed::{Embedder, OllamaEmbedder};
pub use engine::Engine;
pub use error::CoreError;
pub use search::SearchHit;
pub use store::{Passage, Store};
