use std::path::Path;

use athenaeum_core::{embed::FakeEmbedder, Engine, Store};
use athenaeum_ingest::ingest;

#[tokio::test]
async fn ingest_markdown_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let engine = Engine::with_parts(FakeEmbedder { dim: 768 }, store, 768);

    let summary = ingest(&engine, Path::new("tests/fixtures/sample.md"))
        .await
        .expect("ingest should succeed");

    assert_eq!(summary.documents, 1);
    assert!(summary.chunks > 0, "should produce at least one chunk");

    // Verify the data is searchable and cited under the "sample" title.
    let hits = engine
        .search("chapter", 5)
        .await
        .expect("search should succeed");
    assert!(!hits.is_empty(), "ingested data should be searchable");
    assert!(
        hits.iter().any(|h| h.source == "sample"),
        "expected a hit sourced from 'sample', got: {:?}",
        hits
    );
}
