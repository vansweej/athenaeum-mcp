# Feature: Minimal MCP server — search(query, k) over LanceDB + Ollama (build step 2)

This plan implements build step 2 from `docs/decision-brief.md`: a runnable
`athenaeum-mcp-server` that exposes a single `search(query, k)` MCP tool.
End-to-end: query text → embed via Ollama → vector search in LanceDB → top-k
cited `SearchHit` passages.

## Decisions settled before writing this plan

- **A — Metric**: LanceDB cosine distance; `SearchHit.score = (1.0 - distance).clamp(0.0, 1.0)`;
  768-dim vectors (nomic-embed-text output dimension).
- **B — Embed API**: Ollama batch endpoint `POST /api/embed`
  (`{ model, input: [...] }` → `{ embeddings: [[...]] }`). Supports batching
  now so the step-3 ingestion pipeline has no migration cost.
- **C — Config**: A `Config` struct with hardcoded default values, threaded into
  `Store` and `Engine`. Tests override `db_path` via `tempfile::tempdir()` —
  no env-var overrides in this build step.
- **D — rmcp 1.7**: Macro-based tool registration (`#[tool_router]` / `#[tool]`
  / `#[tool_handler]`). Requires the `macros` and `schemars` features on the
  `rmcp` dep, and a direct `schemars = "1"` dependency in the mcp-server crate.
- **Coverage**: `Embedder` trait + `FakeEmbedder` enables ≥ 90 % unit coverage.
  The real `OllamaEmbedder` HTTP path is covered only by an `#[ignore]`d
  integration test — acceptable.

---

## Phase 1: Core crate foundation — dependencies, config, errors

Commit message: feat(core): add config, error variants, and runtime dependencies

### Step 1: Add runtime dependencies to athenaeum-core

Modify `crates/core/Cargo.toml`. Under `[dependencies]` add, all referencing
the workspace: `tokio`, `serde_json`, `reqwest`, `lancedb`, `arrow-array`, and
`futures` (add `futures = "0.3"` to the root `Cargo.toml`
`[workspace.dependencies]` first if not present, then reference it from the
core crate with `{ workspace = true }`). Keep the existing `thiserror` and
`serde` entries. Under `[dev-dependencies]` keep `tokio` and add
`tempfile = { workspace = true }`. Do not remove any existing entry.

### Step 2: Create the Config struct with hardcoded defaults

Create `crates/core/src/config.rs`. Define a public struct `Config` with
fields: `db_path: std::path::PathBuf`, `table_name: String`,
`ollama_url: String`, `embed_model: String`, `embed_dim: usize`. Implement
`Default` for `Config` with hardcoded values:

- `db_path` = `PathBuf::from("./data/athenaeum")`
- `table_name` = `"passages"`
- `ollama_url` = `"http://localhost:11434"`
- `embed_model` = `"nomic-embed-text"`
- `embed_dim` = `768`

Add a rustdoc comment on the struct explaining these are fixed defaults for the
single-user local build and that fields may be overridden by constructing the
struct directly (e.g. in tests). Add a unit test confirming
`Config::default().embed_dim == 768` and `table_name == "passages"`.

### Step 3: Extend CoreError with the variants the engine needs

Modify `crates/core/src/error.rs`. Keep the existing `EmbeddingFailed`,
`StoreFailed`, and `NotImplemented` variants. Add new variants:

- `Http(String)` — message: `"ollama request failed: {0}"`
- `DimensionMismatch { expected: usize, actual: usize }` — message:
  `"embedding dimension mismatch: expected {expected}, got {actual}"`
- `EmptyInput` — message: `"input text was empty"`

Keep `NotImplemented` for now (other crates still reference it). Update the
doc comment to note these cover the embedding and storage paths.

### Step 4: Wire new modules into the core crate root

Modify `crates/core/src/lib.rs`. Add `pub mod config;` and keep
`pub mod error;` and `pub mod search;`. Add `pub use config::Config;` alongside
the existing `pub use error::CoreError;`. Do not remove the existing `search`
re-export yet (it is replaced in Phase 4). Add a crate-level `//!` doc comment
summarising that this crate provides embedding, LanceDB storage, and the
`search(query, k)` core used by the MCP server.

---

## Phase 2: Embedding — Embedder trait and Ollama client

Commit message: feat(core): add Embedder trait and Ollama batch embedder

### Step 1: Define the Embedder trait and a deterministic fake for tests

Create `crates/core/src/embed.rs`. Define a public `#[async_trait::async_trait]`
trait `Embedder` with one method:

```rust
async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, CoreError>;
```

Returns one vector per input, in order. Add `async-trait = "0.1"` to the root
`Cargo.toml` `[workspace.dependencies]` and reference it from
`crates/core/Cargo.toml` as `{ workspace = true }`.

In the same file, behind `#[cfg(test)]`, define `FakeEmbedder { dim: usize }`
implementing `Embedder`. It returns, for each input, a deterministic vector of
length `dim` derived from the input bytes (e.g. hashing so equal strings embed
identically and different strings differ). It returns `CoreError::EmptyInput`
if any input is empty.

Add a rustdoc comment on the trait describing the ordering and length contract.
Add a unit test confirming the fake returns one vector of the expected dim per
input and is deterministic across two calls with the same input.

Add `pub mod embed;` and `pub use embed::{Embedder, OllamaEmbedder};` to
`crates/core/src/lib.rs`.

### Step 2: Implement OllamaEmbedder against /api/embed

In `crates/core/src/embed.rs`, add a public struct `OllamaEmbedder` holding:
`client: reqwest::Client`, `url: String`, `model: String`, `dim: usize`. Add
`OllamaEmbedder::new(url: impl Into<String>, model: impl Into<String>, dim: usize) -> Self`.

Implement `Embedder` for `OllamaEmbedder`:

1. Return `CoreError::EmptyInput` if `inputs` is empty or any entry is empty.
2. POST to `{url}/api/embed` with JSON body `{ "model": model, "input": inputs }`.
3. Deserialize the response: `{ "embeddings": Vec<Vec<f32>> }`.
4. Validate: returned count == `inputs.len()`; every vector has length `dim`.
   Return `CoreError::DimensionMismatch` on violation.
5. Map reqwest errors to `CoreError::Http`.

Define private `#[derive(Serialize)]` / `#[derive(Deserialize)]` request and
response structs. Add a rustdoc comment noting this targets the batch
`/api/embed` endpoint. Add an `#[ignore]`d `#[tokio::test]` (documented as
requiring a running Ollama with `nomic-embed-text`) that embeds two short
strings and asserts two vectors of length 768.

---

## Phase 3: Storage — LanceDB Store

Commit message: feat(core): add LanceDB passage store with vector search

### Step 1: Define the stored row type and Arrow schema

Create `crates/core/src/store.rs`. Define a public struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Passage {
    pub source:   String,
    pub location: String,
    pub text:     String,
}
```

Add a private function that builds the Arrow `Schema` for the table given `dim`:
fields `vector` as `FixedSizeList(Float32; dim)`, `source` Utf8, `location`
Utf8, `text` Utf8. Add a rustdoc comment documenting that the raw `text` is
always stored alongside the vector (the "raw source text" mandate from the
decision brief).

### Step 2: Implement Store — open and ensure-table

In `crates/core/src/store.rs`, define:

```rust
pub struct Store {
    table_name: String,
    dim:        usize,
    conn:       lancedb::Connection,
}
```

Implement `async fn open(db_path: &Path, table_name: &str, dim: usize) -> Result<Store, CoreError>`:
connect to LanceDB at `db_path`; if the table does not exist create it empty
with the Phase-3 schema; otherwise open it. Map all LanceDB errors to
`CoreError::StoreFailed`. Verify the exact lancedb 0.30 builder method names
(`connect(...).execute()`, `create_empty_table` / `open_table`) at compile time.

### Step 3: Implement batch insert and cosine vector search

In `crates/core/src/store.rs`, add to `Store`:

**Insert:**
```rust
pub async fn add(&self, vectors: &[Vec<f32>], passages: &[Passage]) -> Result<(), CoreError>
```
Build a single Arrow `RecordBatch` from the parallel slices. Return
`CoreError::DimensionMismatch` if any vector length != `dim`, or
`CoreError::StoreFailed` if the slice lengths differ. Append to the table.

**Search:**
```rust
pub async fn search(&self, vector: &[f32], k: usize) -> Result<Vec<(Passage, f32)>, CoreError>
```
Run a nearest-neighbour query using the **cosine** distance metric, limited to
`k`. Read back `source`, `location`, `text`, and the distance column. An empty
table must yield `Ok(vec![])`, never an error. Map LanceDB errors to
`CoreError::StoreFailed`.

Add `pub mod store;` and `pub use store::{Passage, Store};` to
`crates/core/src/lib.rs`.

**Test:** Add a `#[tokio::test]` using `tempfile::tempdir()` that:

1. Opens a `Store` on the tempdir.
2. Searches before any inserts — asserts `Ok(vec![])`.
3. Adds two passages with hand-written, distinct 768-dim vectors.
4. Searches with a query vector close to the first passage's vector.
5. Asserts the top result is the expected passage.

---

## Phase 4: Engine — compose embedder + store, replace stub API

Commit message: feat(core): add Engine with add_passage and search, remove stub functions

### Step 1: Reduce search.rs to the SearchHit result type

Modify `crates/core/src/search.rs`. Remove the stub `embed` and `search` free
functions and their `NotImplemented` tests. Keep only the public `SearchHit`
struct (existing fields and doc comments unchanged). Update
`crates/core/src/lib.rs`: change `pub use search::{SearchHit, embed, search};`
to `pub use search::SearchHit;`.

### Step 2: Implement Engine

Create `crates/core/src/engine.rs`. Define:

```rust
pub struct Engine<E: Embedder> {
    embedder: E,
    store:    Store,
    dim:      usize,
}
```

Provide two constructors:

- `pub async fn new(config: Config) -> Result<Engine<OllamaEmbedder>, CoreError>` —
  builds `OllamaEmbedder` from the config and opens `Store`.
- `pub fn with_parts(embedder: E, store: Store, dim: usize) -> Engine<E>` —
  for test injection of `FakeEmbedder`.

Add:

```rust
pub async fn add_passage(&self, source: &str, location: &str, text: &str) -> Result<(), CoreError>
```
Embed the single `text` (via `self.embedder.embed(&[text.to_string()])`) then
call `self.store.add` with one vector and one `Passage`. Return
`CoreError::EmptyInput` if `text` is empty (propagated from the embedder).

```rust
pub async fn search(&self, query: &str, k: usize) -> Result<Vec<SearchHit>, CoreError>
```
Embed the query, call `self.store.search`, map each `(Passage, distance)` into
a `SearchHit` with `score = (1.0_f32 - distance).clamp(0.0, 1.0)`.

Add rustdoc comments noting `add_passage` is the minimal seeding/write path for
build step 2, and that the full EPUB/PDF ingestion pipeline (`athenaeum-ingest`)
is build step 3 and will call this API (or a batch form) per chunk.

Add `pub mod engine;` and `pub use engine::Engine;` to
`crates/core/src/lib.rs`.

### Step 3: End-to-end Engine unit test with FakeEmbedder

In `crates/core/src/engine.rs`, add a `#[cfg(test)]` module with a
`#[tokio::test]` that:

1. Builds an `Engine` via `with_parts(FakeEmbedder { dim: 768 }, store, 768)`
   where `store` opens on a `tempfile::tempdir()`.
2. Asserts that `engine.search("anything", 5)` on an empty engine returns
   `Ok(vec![])`.
3. Adds two passages with distinct text via `add_passage`.
4. Searches with a query equal to one passage's text.
5. Asserts the top `SearchHit` has the matching `source`/`location`/`text` and
   `score` in `[0.0, 1.0]`.

---

## Phase 5: MCP server — register the search tool over stdio

Commit message: feat(mcp-server): expose search(query, k) tool via rmcp stdio

### Step 1: Enable the required rmcp features and add schemars

Modify the root `Cargo.toml` `[workspace.dependencies]`:

- Change the `rmcp` entry to:
  `rmcp = { version = "1.7", features = ["server", "transport-io", "macros", "schemars"] }`
- Add `schemars = "1"`.

Modify `crates/mcp-server/Cargo.toml`: add `serde = { workspace = true }`,
`serde_json = { workspace = true }`, and `schemars = { workspace = true }`.
Keep all existing entries.

### Step 2: Implement the server struct, search tool, and ServerHandler

Rewrite `crates/mcp-server/src/main.rs`.

Define the input schema type:

```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    /// Natural-language query sent to the embedding model.
    query: String,
    /// Maximum number of passages to return.
    k: usize,
}
```

Define the server struct:

```rust
#[derive(Clone)]
struct AthenaeumServer {
    engine:      Arc<Engine<OllamaEmbedder>>,
    tool_router: ToolRouter<AthenaeumServer>,
}
```

In a `#[tool_router]` impl block:

- `fn new(engine: Engine<OllamaEmbedder>) -> Self` — wraps in `Arc`, calls
  `Self::tool_router()`.
- `#[tool(description = "Search the personal library for cited passages")]`
  `async fn search(&self, Parameters(SearchArgs { query, k }): Parameters<SearchArgs>) -> Result<CallToolResult, McpError>`:
  calls `self.engine.search(&query, k).await`, maps `CoreError` to
  `McpError::internal_error(e.to_string(), None)`, serialises `Vec<SearchHit>`
  to a JSON string, returns `CallToolResult::success(vec![Content::text(json)])`.

In a separate `#[tool_handler] impl ServerHandler for AthenaeumServer` block:

- `fn get_info(&self) -> ServerInfo` — returns `ServerInfo` with
  `ServerCapabilities::builder().enable_tools().build()`,
  `Implementation::from_build_env()`, and instructions string
  `"Personal library semantic search over CS, FP, and computer-graphics books and papers."`.

Import all needed items from `rmcp` per the 1.7 public API. Verify exact
builder/constructor method names (`ServerInfo::new`, `ServerCapabilities::builder`,
`McpError::internal_error`, etc.) against the pinned crate at compile time.

### Step 3: Wire main() to build the Engine and serve over stdio

At the bottom of `crates/mcp-server/src/main.rs`, implement:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let engine = Engine::new(Config::default()).await?;
    let server = AthenaeumServer::new(engine);
    let service = server.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

Remove the old scaffold `eprintln!` placeholder and the `TODO(brief-step-2)`
comment. Keep a `//!` crate-level doc comment:
`"athenaeum-mcp-server — MCP server exposing search(query, k) over the personal library."`

---

## Phase 6: Documentation

Commit message: docs: document the search tool, config defaults, and seeding

### Step 1: Update the README for the now-functional server

Modify `README.md`:

- Update the `crates/core` row of the workspace table to: "Ollama embedding +
  LanceDB storage + `search(query, k)` + `add_passage` seeding".
- Replace the "scaffold — no tools registered yet" note in the run section with:
  the server now exposes a `search(query, k)` MCP tool over stdio; requires
  Ollama running with `nomic-embed-text`; LanceDB data is stored under
  `./data/athenaeum` by default.
- Add a short **Configuration** subsection listing the hardcoded `Config`
  defaults (db path, table name, Ollama URL, model, embed dim) and noting they
  are compile-time defaults for the single-user build. Do not document env-var
  overrides (none exist yet).

### Step 2: Add a module-level doc note on the deferred write path

Ensure `crates/core/src/engine.rs` makes clear in rustdoc that `add_passage`
is the minimal seeding/write path for build step 2, and that the full EPUB/PDF
ingestion pipeline (`athenaeum-ingest`, build step 3) will call this API (or a
batch form) per chunk. No code changes — doc only.

---

## Risks to keep visible during execution

| Risk | Mitigation |
|---|---|
| LanceDB 0.30 builder API method names | Verify with `cargo check` in Phase 3, not from memory |
| rmcp 1.7 constructor/builder names (`ServerInfo`, `McpError`, etc.) | Verify with `cargo check` in Phase 5 |
| Arrow `FixedSizeList` RecordBatch construction | Budget extra time; schema/insert code is fiddly |
| Empty-table search returning error instead of `Ok(vec![])` | Explicitly tested in Phases 3 and 4 |
| OllamaEmbedder HTTP path not covered in CI | Accepted; `#[ignore]`d integration test documents this |

## Build strategy (one paragraph)

Build the core bottom-up — config/errors, then a trait-abstracted embedder,
then the LanceDB store, then an `Engine` that composes them — so every layer
except the live HTTP call is unit-testable with a `FakeEmbedder` and a tempdir
database. The MCP server stays a thin rmcp shell that deserialises args,
delegates to `Engine::search`, and serialises `SearchHit`s, keeping all logic
in the tested core. A minimal `add_passage` write path makes search
demonstrable now while leaving the real document pipeline to build step 3.
