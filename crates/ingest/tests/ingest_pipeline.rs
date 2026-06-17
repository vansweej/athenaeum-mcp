use std::path::Path;

use athenaeum_core::{embed::FakeEmbedder, Engine, Store};
use athenaeum_ingest::extract::extract_epub;
use athenaeum_ingest::{ingest, IngestError};

#[tokio::test]
async fn ingest_epub_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

    let summary = ingest(
        &engine,
        Path::new("../parser-spike/tests/fixtures/sample.epub"),
    )
    .await
    .expect("ingest should succeed");

    assert_eq!(summary.documents, 1);
    assert!(summary.chunks > 0, "should produce at least one chunk");

    // Verify the data is searchable
    let hits = engine
        .search("the", 5)
        .await
        .expect("search should succeed");
    assert!(!hits.is_empty(), "ingested data should be searchable");
}

#[tokio::test]
async fn ingest_pdf_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

    let summary = ingest(
        &engine,
        Path::new("../parser-spike/tests/fixtures/sample.pdf"),
    )
    .await
    .expect("ingest should succeed");

    assert_eq!(summary.documents, 1);
    assert!(summary.chunks > 0, "should produce at least one chunk");

    // Verify the data is searchable
    let hits = engine
        .search("the", 5)
        .await
        .expect("search should succeed");
    assert!(!hits.is_empty(), "ingested data should be searchable");
}

#[tokio::test]
async fn ingest_unsupported_extension_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

    // Create a temp file with .txt extension
    let txt_path = dir.path().join("test.txt");
    std::fs::write(&txt_path, "hello").unwrap();

    let result = ingest(&engine, &txt_path).await;
    assert!(
        matches!(result, Err(IngestError::UnsupportedFileType(_))),
        "expected UnsupportedFileType error, got {:?}",
        result
    );
}

#[tokio::test]
async fn ingest_missing_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

    let result = ingest(&engine, &dir.path().join("nonexistent.epub")).await;

    assert!(
        matches!(result, Err(IngestError::IoFailed(_))),
        "expected IoFailed error, got {:?}",
        result
    );
}

#[tokio::test]
async fn extract_epub_invalid_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let bad_epub = dir.path().join("not-an-epub.epub");
    std::fs::write(&bad_epub, "this is not an epub file").unwrap();

    let result = extract_epub(&bad_epub).await;
    assert!(
        matches!(result, Err(IngestError::ParseFailed(_))),
        "expected ParseFailed error for invalid EPUB, got {:?}",
        result
    );
}

#[tokio::test]
async fn extract_epub_nonexistent_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let result = extract_epub(&dir.path().join("nonexistent.epub")).await;
    assert!(
        matches!(result, Err(IngestError::ParseFailed(_))),
        "expected ParseFailed error for nonexistent EPUB, got {:?}",
        result
    );
}

#[tokio::test]
async fn ingest_dedup_replaces_on_reingest() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

    let path = Path::new("../parser-spike/tests/fixtures/sample.epub");

    // First ingest
    let first = ingest(&engine, path).await.expect("first ingest");
    let first_chunks = first.chunks;

    // Second ingest — should replace, not append
    let second = ingest(&engine, path).await.expect("second ingest");
    assert_eq!(second.chunks, first_chunks, "chunk count should match");

    // Search should return the same number of hits as from one ingest
    let hits = engine.search("the", 50).await.expect("search");
    assert_eq!(
        hits.len(),
        first_chunks.min(50),
        "search results should not double after re-ingest"
    );
}
