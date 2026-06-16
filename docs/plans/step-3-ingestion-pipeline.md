# Feature: Ingestion Pipeline — EPUB + PDF chunker with `ingest(path)` MCP tool and CLI (build step 3)

## Goal restated

Implement the ingestion pipeline as defined in the decision brief (build step 3): extract text from EPUB and text-based PDFs, chunk it at sentence boundaries targeting 500–1000 tokens with overlap, preserve citation metadata (title/chapter/section/page), and store chunks into the LanceDB vector store via a batch embedding + insert path. Expose ingestion as both a single-file `ingest(path)` MCP tool on the existing server and a separate CLI binary for bulk directory ingestion.

## Decisions settled before writing this plan

| Decision | Choice |
|---|---|
| Delivery model | MCP tool for single-file + separate CLI binary for bulk |
| Parsing ownership | `athenaeum-ingest` owns extraction + chunking; parser-spike remains canary only |
| Overlap strategy | Sentence-boundary overlap |
| Token counting | Whitespace word count (1 word ≈ 1.3 tokens) |
| Engine batch API | New `Engine::add_passages` that batches embed + insert |
| EPUB chapter detection | HTML `<h1>`/`<h2>` heading tag extraction |
| CLI style | Separate `athenaeum-ingest` binary target |

---

## Phase 1: Engine batch API

Commit message: feat(core): add batch add_passages method to Engine

### Step 1: Add batch add_passages to Engine

Modify `crates/core/src/engine.rs`. Add a new public method to `impl<E: Embedder> Engine<E>`:

```rust
pub async fn add_passages(
    &self,
    passages: &[(String, String, String)], // (source, location, text)
) -> Result<usize, CoreError>
```

This method:
1. Returns `Ok(0)` immediately if `passages` is empty.
2. Extracts all `text` fields into a `Vec<String>` and passes them to `self.embedder.embed(...)` in a single batch call (leveraging Ollama's batch `/api/embed` endpoint).
3. Constructs a `Vec<Passage>` from the `(source, location, text)` tuples.
4. Calls `self.store.add(&vectors, &passages_vec)`.
5. Returns `Ok(passages.len())` on success.
6. Returns `CoreError::EmptyInput` if any text field is empty (propagated from the embedder).

Add a rustdoc comment noting this is the batch write path for the ingestion pipeline (build step 3). Keep the existing `add_passage` single-item method unchanged (it remains useful for tests and ad-hoc seeding).

### Step 2: Unit test for batch add_passages

In the `#[cfg(test)]` module of `crates/core/src/engine.rs`, add a `#[tokio::test]` named `batch_add_and_search`:

1. Build an `Engine` via `with_parts(FakeEmbedder { dim: 768 }, store, 768)` on a `tempfile::tempdir()`.
2. Call `add_passages` with 3 tuples of distinct text.
3. Assert the return value is `Ok(3)`.
4. Search for text equal to one of the passages and assert it appears in top results.
5. Also test that calling `add_passages` with an empty slice returns `Ok(0)`.

---

## Phase 2: Text extraction in the ingest crate

Commit message: feat(ingest): add EPUB and PDF text extraction with metadata

### Step 1: Add parsing dependencies to athenaeum-ingest

Modify `crates/ingest/Cargo.toml`. Under `[dependencies]` add:
- `pdfium-render = { workspace = true }`
- `epub = { workspace = true }`
- `tokio = { workspace = true }`

Keep all existing entries (`athenaeum-core`, `thiserror`, `serde`).

### Step 2: Create the extract module with PDF extraction

Create `crates/ingest/src/extract.rs`. Define:

```rust
pub struct ExtractedDocument {
    pub title: String,
    pub pages: Vec<ExtractedPage>,
}

pub struct ExtractedPage {
    pub page_number: u32,
    pub text: String,
}
```

Implement a public function:

```rust
pub fn extract_pdf(path: &Path) -> Result<ExtractedDocument, IngestError>
```

This function:
1. Opens the PDF via `Pdfium::default()` and `load_pdf_from_file`.
2. Derives `title` from the filename (stem, without extension).
3. Iterates pages, extracting text via `page.text().all()`, creating an `ExtractedPage` per PDF page with 1-based `page_number`.
4. Maps all pdfium errors to `IngestError::ParseFailed`.
5. Returns `IngestError::ParseFailed("empty document")` if no text is extracted.

Add a rustdoc comment explaining the function extracts all text content page-by-page and that the `page_number` field is used for citation location metadata.

### Step 3: Add EPUB extraction with heading detection

In `crates/ingest/src/extract.rs`, implement:

```rust
pub struct EpubSection {
    pub chapter: Option<String>,
    pub section: Orption<String>,
    pub text: String,
}

pub fn extract_epub(path: &Path) -> Result<(String, Vec<EpubSection>), IngestError>
```

This function:
1. Opens the EPUB via `epub::doc::EpubDoc::new(path)`.
2. Extracts the title from EPUB metadata (`doc.mdata("title")`) or falls back to the filename stem.
3. Iterates spine items via `get_current_str()` / `go_next()` loop.
4. For each spine item's HTML content, detects `<h1 />` tags to set the current chapter name, and `<h2 />` tags to set the current section name. Uses simple string searching (e.g. regex or `find`/`split` on `<h1 />` and `</h1 />` — does NOT require an HTML parser dependency).
5. Strips all HTML tags from the content to get plain text (simple regex: remove everything between `<` and `>`).
6. Produces one `EpubSection` per spine item, carrying the last-seen chapter/section names.
7. Maps errors to `IngestError::ParseFailed`.

Return type is `(title, sections)`.

### Step 4: Wire extract module into lib.rs

Modify `crates/ingest/src/lib.rs`. Add `pub mod extract;` and add a `pub use extract::{ExtractedDocument, ExtractedPage, EpubSection, extract_pdf, extract_epub};`.

### Step 5: Add IngestError variants for extraction

Modify `crates/ingest/src/error.rs`. Keep existing variants. Add:
- `IoFailed(String)` — message: `"io error: {0}"`
- `EmbedFailed(String)` — message: `"embedding failed: {0}"`
- `StoreFailed(String)` — message: `"store operation failed: {0}"`

Add an `impl From<athenaeum_core::CoreError> for IngestError` that maps `CoreError` variants to the corresponding `IngestError` variants (e.g. `CoreError::Http(..)` → `IngestError::EmbedFailed`, `Core

[... truncated for brevity in thought block ...]
