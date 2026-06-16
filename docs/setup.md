# Setup & Installation — athenaeum-mcp

Get the system running from scratch on a fresh machine.

---

## Prerequisites

### System Requirements

- **Nix** with flakes enabled (see [nixos.org](https://nixos.org/))
- **Ollama** running locally with the `nomic-embed-text` model pulled

### Install Ollama & Pull the Model

1. Install Ollama from [ollama.com](https://ollama.com/)
2. Pull the embedding model:
   ```bash
   ollama pull nomic-embed-text
   ```
3. Verify Ollama is running:
   ```bash
   curl http://localhost:11434/api/tags
   ```
   You should see `nomic-embed-text` in the response.

**Note:** The Rust toolchain, pdfium, and all other system dependencies are provided by the Nix dev shell (`flake.nix`). Do not install them on the host.

---

## Clone & Enter the Dev Shell

```bash
git clone https://github.com/yourusername/athenaeum-mcp.git
cd athenaeum-mcp
nix develop
```

The dev shell provides:
- Rust toolchain (1.96+)
- pdfium (for PDF parsing)
- All Cargo dependencies

**Important:** All build commands must run inside the dev shell. Per [AGENTS.md](../AGENTS.md), use `nix develop --command <cmd>` for one-off commands.

---

## Build, Test, Lint, Format, Coverage

All commands run inside the dev shell:

| Task | Command |
|------|---------|
| **Build** | `nix develop --command cargo build` |
| **Test** | `nix develop --command cargo test` |
| **Lint** | `nix develop --command cargo clippy -- -D warnings` |
| **Format** | `nix develop --command cargo fmt` |
| **Coverage** | `nix develop --command cargo tarpaulin` |
| **Run MCP server** | `nix develop --command cargo run -p athenaeum-mcp-server` |
| **Run CLI** | `nix develop --command cargo run -p athenaeum-ingest -- <DIR>` |

**Coverage target:** ≥ 90% (UI and GPU functions excluded with `#[cfg(not(tarpaulin_include))]`).

---

## Configuration Reference

All configuration is hardcoded in `Config::default()` (see `crates/core/src/config.rs`):

| Field | Default | Description |
|-------|---------|-------------|
| `db_path` | `./data/athenaeum` | LanceDB database directory |
| `table_name` | `passages` | LanceDB table name |
| `ollama_url` | `http://localhost:11434` | Ollama base URL (no trailing slash) |
| `embed_model` | `nomic-embed-text` | Ollama embedding model |
| `embed_dim` | `768` | Embedding vector dimension |

### Overriding Configuration

**No environment-variable overrides exist.** To override defaults:

1. **In tests:** Construct `Config` directly with custom values (e.g., `db_path` set to `tempfile::tempdir()`).
2. **In production:** Modify `Config::default()` in `crates/core/src/config.rs` and rebuild.

### Important: Working Directory Coupling

The default `db_path` is **relative**: `./data/athenaeum`. This means:

- The MCP server and the CLI must be launched from the **same working directory** (repo root) to share one store.
- If you run the server from `/path/to/athenaeum-mcp` and the CLI from `/path/to/athenaeum-mcp/crates/ingest`, they will create separate databases.
- **Always run both from the repo root.**

---

## First Smoke Test

### 1. Start the MCP Server

```bash
nix develop --command cargo run -p athenaeum-mcp-server
```

You should see:
```
[listening on stdio]
```

The server is now ready to receive MCP requests. Leave it running in a terminal.

### 2. Verify Ollama Connectivity

In another terminal, run the Ollama integration test (marked `#[ignore]` by default):

```bash
nix develop --command cargo test ollama_embedder_returns_768_dim_vectors -- --ignored
```

If this passes, Ollama is reachable and the model is loaded. If it fails:
- Check that `ollama serve` is running.
- Check that `nomic-embed-text` is pulled: `ollama list`.
- Check the Ollama logs for errors.

### 3. Ingest a Test File

See [ingestion.md](ingestion.md) for detailed instructions. For a quick test:

```bash
# In a third terminal, from the repo root:
nix develop --command cargo run -p athenaeum-ingest -- /path/to/a/test/document.pdf
```

You should see:
```
Found 1 file(s) to ingest in /path/to/a/test
[1/1] Ingesting document.pdf... ✓ (N chunks)

============================================================
Ingestion Summary
============================================================
Total files processed: 1
Successful: 1
Failed: 0
Total documents: 1
Total chunks: N
```

### 4. Query the Server

With the server still running, send a search request (via MCP client or manually):

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "search",
    "arguments": {
      "query": "machine learning",
      "k": 5
    }
  }
}
```

You should receive a JSON array of `SearchHit` objects with `source`, `location`, `text`, and `score` fields.

---

## Troubleshooting

### Ollama Not Running

**Error:** `CoreError::Http("connection refused")`

**Fix:**
```bash
ollama serve
```

### Model Not Pulled

**Error:** `CoreError::Http("404: model not found")`

**Fix:**
```bash
ollama pull nomic-embed-text
```

### Dimension Mismatch

**Error:** `CoreError::DimensionMismatch { expected: 768, actual: N }`

**Cause:** The embedding model was changed to something other than `nomic-embed-text` (which produces 768-dim vectors).

**Fix:** Either:
- Revert to `nomic-embed-text`, or
- Update `Config::embed_dim` to match the new model's dimension.

### Empty Database

**Behavior:** `search(query, k)` returns `[]` (empty array).

**Cause:** No documents have been ingested yet.

**Fix:** Run `athenaeum-ingest` to load documents (see [ingestion.md](ingestion.md)).

### Working Directory Mismatch

**Symptom:** CLI ingests files, but the server's search returns no results.

**Cause:** CLI and server are running from different directories, creating separate `./data/athenaeum` stores.

**Fix:** Always run both from the repo root.

---

## Next Steps

- **[ingestion.md](ingestion.md)** — Load your corpus at scale.
- **[integration.md](integration.md)** — Wire the server into opencode agents.
- **[architecture.md](architecture.md)** — Understand the system design.
