//! Test that `upsert_passages` handles embed errors gracefully.
//!
//! Non-negotiable #2: one bad file must not abort the run. This test asserts
//! that an embed failure returns an `Err` (which the CLI loop converts to a
//! `failed_files` entry) rather than panicking or deadlocking. Only the
//! FailingEmbedder can provide this test — live-Ollama tests cannot simulate
//! the failure mode.

use athenaeum_core::{CoreError, Engine, Store};

#[tokio::test]
async fn embed_failure_returns_error_does_not_abort() {
    use athenaeum_core::embed::FailingEmbedder;

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let embedder = FailingEmbedder::new(768, 1); // fails on first call
    let engine = Engine::with_parts(embedder, store, 768);

    let result = engine
        .upsert_passages(
            "will-fail",
            &[(
                "test.epub".to_string(),
                "p. 1".to_string(),
                "this will fail to embed".to_string(),
            )],
        )
        .await;

    assert!(
        result.is_err(),
        "upsert_passages should propagate the embed error"
    );
    match result {
        Err(CoreError::EmbeddingFailed(_)) | Err(CoreError::Http(_)) => { /* expected */ }
        _ => panic!("unexpected error variant"),
    }
}

#[tokio::test]
async fn non_failing_embed_still_succeeds() {
    use athenaeum_core::embed::FakeEmbedder;

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "passages", 768).await.unwrap();
    let embedder = FakeEmbedder { dim: 768 };
    let engine = Engine::with_parts(embedder, store, 768);

    let result = engine
        .upsert_passages(
            "will-succeed",
            &[(
                "test.epub".to_string(),
                "p. 1".to_string(),
                "this will embed fine".to_string(),
            )],
        )
        .await;

    assert!(
        result.is_ok(),
        "normal embedder should succeed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), 1);
}
