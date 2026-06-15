//! athenaeum-mcp-server — MCP server exposing search(query, k) over the personal library.

use std::sync::Arc;

use athenaeum_core::{Config, Embedder, Engine};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
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

        let json =
            serde_json::to_string(&hits).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl<E: Embedder + 'static> ServerHandler for AthenaeumServer<E> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Personal library semantic search over CS, FP, and computer-graphics books and papers.",
            )
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let engine = Engine::new(Config::default()).await?;
    let server = AthenaeumServer::new(engine);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
