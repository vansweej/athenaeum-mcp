//! athenaeum-mcp-server — MCP server exposing the personal library search tool.
//!
//! Current state: scaffold only. The single `search(query, k)` tool (brief step 2)
//! will be registered where marked below.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO(brief-step-2): register the search(query, k) tool and bind to an
    // rmcp transport. Replace this placeholder once athenaeum-core::search is
    // implemented.
    eprintln!("athenaeum-mcp-server: scaffold only — no tools registered yet");
    Ok(())
}
