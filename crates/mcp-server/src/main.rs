//! athenaeum-mcp-server — MCP server exposing search(query, k) over the personal library.

use std::path::PathBuf;
use std::sync::Arc;

use athenaeum_core::{Config, Embedder, Engine};
use athenaeum_ingest::ingest;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;

// ─── Tool input schema ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchArgs {
    /// Natural-language query sent to the embedding model.
    query: String,
    /// Maximum number of passages to return.
    k: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct IngestArgs {
    /// Path to the PDF, EPUB, or Markdown file to ingest.
    path: String,
}

// ─── Server struct ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AthenaeumServer<E: Embedder + 'static> {
    engine: Arc<Engine<E>>,
    // The router is accessed by the rmcp macro-generated code, not directly.
    #[allow(dead_code)]
    tool_router: ToolRouter<AthenaeumServer<E>>,
}

#[tool_router]
impl<E: Embedder + 'static> AthenaeumServer<E> {
    fn new(engine: Engine<E>) -> Self {
        Self {
            engine: Arc::new(engine),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Search the personal library for cited passages")]
    async fn search(
        &self,
        Parameters(SearchArgs { query, k }): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let hits = self
            .engine
            .search(&query, k)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string(&hits)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Ingest a PDF, EPUB, or Markdown file into the personal library")]
    async fn ingest_file(
        &self,
        Parameters(IngestArgs { path }): Parameters<IngestArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let path_buf = PathBuf::from(&path);
        let summary = ingest(&self.engine, &path_buf)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string(&summary)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl<E: Embedder + 'static> ServerHandler for AthenaeumServer<E> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Personal library semantic search over CS, FP, and computer-graphics books and papers.",
        )
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use athenaeum_core::{embed::FakeEmbedder, SearchHit, Store};
    use rmcp::model::RawContent;

    async fn seed(server: &AthenaeumServer<FakeEmbedder>) {
        server
            .engine
            .upsert_passages(
                "book-a.epub",
                &[(
                    "book-a.epub".to_string(),
                    "p. 1".to_string(),
                    "the quick brown fox".to_string(),
                )],
            )
            .await
            .unwrap();
        server
            .engine
            .upsert_passages(
                "book-b.epub",
                &[(
                    "book-b.epub".to_string(),
                    "p. 2".to_string(),
                    "pack my box with five dozen liquor jugs".to_string(),
                )],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn search_tool_returns_hits_as_json() {
        // Keep `dir` alive for the entire test so LanceDB can access the path.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let server = AthenaeumServer::new(engine);

        seed(&server).await;

        let result = server
            .search(Parameters(SearchArgs {
                query: "the quick brown fox".to_string(),
                k: 2,
            }))
            .await;

        let ok = result.expect("search should succeed");
        let text = match &ok.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("expected text content"),
        };
        let hits: Vec<SearchHit> = serde_json::from_str(text).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].source, "book-a.epub");
        assert!(
            hits[0].score >= 0.0 && hits[0].score <= 1.0,
            "score out of range: {}",
            hits[0].score
        );
    }

    #[tokio::test]
    async fn search_tool_empty_query_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let server = AthenaeumServer::new(engine);

        let result = server
            .search(Parameters(SearchArgs {
                query: "".to_string(),
                k: 5,
            }))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ingest_file_tool_returns_summary_json() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let server = AthenaeumServer::new(engine);

        let result = server
            .ingest_file(Parameters(IngestArgs {
                path: "../parser-spike/tests/fixtures/sample.epub".to_string(),
            }))
            .await;

        let ok = result.expect("ingest_file should succeed");
        let text = match &ok.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("expected text content"),
        };
        let summary: athenaeum_ingest::IngestSummary = serde_json::from_str(text).unwrap();
        assert_eq!(summary.documents, 1);
        assert!(summary.chunks > 0);
    }

    #[tokio::test]
    async fn ingest_file_tool_error_on_bad_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let server = AthenaeumServer::new(engine);

        let result = server
            .ingest_file(Parameters(IngestArgs {
                path: "/nonexistent/file.epub".to_string(),
            }))
            .await;

        assert!(result.is_err(), "expected error for non-existent path");
    }

    #[tokio::test]
    async fn get_info_advertises_tools() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "passages", 768).await.unwrap();
        let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);
        let server = AthenaeumServer::new(engine);

        let info = server.get_info();
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be advertised"
        );
        assert!(
            info.instructions.as_ref().is_some_and(|s| !s.is_empty()),
            "instructions must be present and non-empty"
        );
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[cfg(not(tarpaulin_include))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let engine = Engine::new(Config::default()).await?;
    let server = AthenaeumServer::new(engine);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
