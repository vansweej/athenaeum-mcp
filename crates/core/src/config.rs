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
            db_path: default_data_dir(),
            table_name: "passages".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            embed_model: "nomic-embed-text".to_string(),
            embed_dim: 768,
            embed_timeout: DEFAULT_EMBED_TIMEOUT,
            embed_connect_timeout: DEFAULT_EMBED_CONNECT_TIMEOUT,
        }
    }
}

/// Return the platform-appropriate data directory for the `athenaeum` application.
///
/// The server and CLI now self-locate regardless of the working directory —
/// there is no longer a coupling to `./data/athenaeum` relative to the
/// invocation directory. On a typical Linux/macOS machine this resolves to
/// `~/.local/share/athenaeum/data/athenaeum`, which is functionally identical
/// to the previous `./data/athenaeum` when the repo root was the working
/// directory (zero migration required for existing stores placed there).
///
/// Precedence (first match wins):
/// 1. `$XDG_DATA_HOME/<app>/data/<app>` — when `XDG_DATA_HOME` is set and non-empty.
/// 2. `%LOCALAPPDATA%/<app>/data/<app>` — Windows only, when `LOCALAPPDATA` is set and non-empty.
/// 3. `$HOME/.local/share/<app>/data/<app>` — when `HOME` is set and non-empty.
/// 4. `./data/<app>` — relative fallback (same as the previous default).
pub fn default_data_dir() -> PathBuf {
    resolve_data_dir("athenaeum", |k| std::env::var(k).ok())
}

/// Pure, testable implementation of the data-directory resolution logic.
///
/// `get_env` is called with environment variable names and returns `Some(value)`
/// when the variable is set, or `None` when it is absent. Passing a closure
/// rather than reading `std::env` directly makes the function fully testable
/// without mutating the process environment.
///
/// The final path for every branch is `<base>/<app>/data/<app>`.
fn resolve_data_dir(app: &str, get_env: impl Fn(&str) -> Option<String>) -> PathBuf {
    // 1. XDG_DATA_HOME (all platforms)
    if let Some(xdg) = get_env("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg).join(app).join("data").join(app);
    }

    // 2. LOCALAPPDATA (Windows only)
    #[cfg(windows)]
    if let Some(local) = get_env("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(local).join(app).join("data").join(app);
    }

    // 3. HOME → ~/.local/share/<app>/data/<app>
    if let Some(home) = get_env("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(app)
            .join("data")
            .join(app);
    }

    // 4. Relative fallback
    PathBuf::from(".").join("data").join(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Config defaults ───────────────────────────────────────────────────────

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

    // ─── resolve_data_dir tests (injected closures, no env mutation) ───────────

    #[test]
    fn xdg_data_home_set_and_non_empty_uses_xdg() {
        let path = resolve_data_dir("athenaeum", |k| match k {
            "XDG_DATA_HOME" => Some("/custom/xdg".to_string()),
            _ => None,
        });
        assert_eq!(path, PathBuf::from("/custom/xdg/athenaeum/data/athenaeum"));
    }

    #[test]
    fn xdg_data_home_empty_string_falls_through_to_home() {
        let path = resolve_data_dir("athenaeum", |k| match k {
            "XDG_DATA_HOME" => Some(String::new()), // empty → skip
            "HOME" => Some("/home/user".to_string()),
            _ => None,
        });
        assert_eq!(
            path,
            PathBuf::from("/home/user/.local/share/athenaeum/data/athenaeum")
        );
    }

    #[test]
    fn home_set_produces_correct_path() {
        let path = resolve_data_dir("athenaeum", |k| match k {
            "XDG_DATA_HOME" => None,
            "HOME" => Some("/home/alice".to_string()),
            _ => None,
        });
        // Must end with athenaeum/data/athenaeum under <HOME>/.local/share
        assert_eq!(
            path,
            PathBuf::from("/home/alice/.local/share/athenaeum/data/athenaeum")
        );
        // Verify the suffix explicitly
        assert!(
            path.ends_with("athenaeum/data/athenaeum"),
            "path should end with athenaeum/data/athenaeum, got: {path:?}"
        );
    }

    #[test]
    fn no_env_vars_falls_back_to_relative_path() {
        let path = resolve_data_dir("athenaeum", |_| None);
        assert_eq!(path, PathBuf::from("./data/athenaeum"));
    }

    #[test]
    fn default_data_dir_final_component_is_app_name() {
        let path = default_data_dir();
        let last = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("path must have a final component");
        assert_eq!(last, "athenaeum");
    }

    #[test]
    fn custom_app_name_is_threaded_through_all_segments() {
        let path = resolve_data_dir("myapp", |k| match k {
            "XDG_DATA_HOME" => Some("/xdg".to_string()),
            _ => None,
        });
        assert_eq!(path, PathBuf::from("/xdg/myapp/data/myapp"));
    }
}
