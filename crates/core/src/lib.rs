//! `athenaeum-core` — Ollama embedding, LanceDB storage, and the
//! `search(query, k)` / `add_passage` core used by the MCP server.

pub mod config;
pub mod embed;
pub mod error;
pub mod search;

pub use config::Config;
pub use embed::{Embedder, OllamaEmbedder};
pub use error::CoreError;
pub use search::{SearchHit, embed, search};
