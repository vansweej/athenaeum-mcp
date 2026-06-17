//! Hardcoded configuration defaults for the single-user local build.
//!
//! `Config` holds all runtime parameters for the embedding and storage paths.
//! These are compile-time defaults — override any field by constructing the
//! struct directly (e.g. in tests, set `db_path` to a `tempdir()` path).

use std::path::PathBuf;
use std::time::Duration;

/// Total deadline for the Ollama embed request (generous enough to tolerate a
/// cold model load).
pub const DEFAULT_EMBED_TIMEOUT: Duration = Duration::from_secs(60);

/// TCP connect deadline for the Ollama embed request (short because "cannot
/// connect" is unambiguous).
pub const DEFAULT_EMBED_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Configuration for `athenaeum-core`.
///
/// All fields have hardcoded defaults suitable for a single-user local
/// deployment. Tests override `db_path` via `tempfile::tempdir()` to avoid
/// touching the production store.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the LanceDB database directory.
    pub db_path: PathBuf,
    /// Name of the LanceDB table that holds passages.
    pub table_name: String,
    /// Base URL of the local Ollama instance (no trailing slash).
    pub ollama_url: String,
    /// Name of the Ollama embedding model to use.
    pub embed_model: String,
    /// Expected dimension of the embedding vectors produced by `embed_model`.
    pub embed_dim: usize,
    /// Total deadline for the Ollama embed request (connect + response).
    pub embed_timeout: Duration,
    /// TCP connect deadline for the Ollama embed request.
    pub embed_connect_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./data/athenaeum"),
            table_name: "passages".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            embed_model: "nomic-embed-text".to_string(),
            embed_dim: 768,
            embed_timeout: DEFAULT_EMBED_TIMEOUT,
            embed_connect_timeout: DEFAULT_EMBED_CONNECT_TIMEOUT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_embed_dim_is_768() {
        assert_eq!(Config::default().embed_dim, 768);
    }

    #[test]
    fn default_table_name_is_passages() {
        assert_eq!(Config::default().table_name, "passages");
    }

    #[test]
    fn default_embed_timeout_is_60_seconds() {
        assert_eq!(Config::default().embed_timeout, Duration::from_secs(60));
    }

    #[test]
    fn default_embed_connect_timeout_is_5_seconds() {
        assert_eq!(
            Config::default().embed_connect_timeout,
            Duration::from_secs(5)
        );
    }
}
