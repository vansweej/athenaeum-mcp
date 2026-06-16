use std::path::Path;

use athenaeum_ingest::extract::extract_epub;

// Regression coverage for the EPUB content/mime tuple bug: `get_current_str()`
// returns `(content, mime)`, so the body must be bound from the first element.
// The fixture is the parser-spike toolchain-canary EPUB, referenced by relative
// path because integration tests run with the crate root as the working dir.
#[tokio::test]
async fn extracts_real_text_from_epub() {
    let path = Path::new("../parser-spike/tests/fixtures/sample.epub");

    let (title, sections) = extract_epub(path)
        .await
        .expect("epub extraction should succeed");

    assert!(!title.trim().is_empty(), "title must be non-empty");
    assert!(!sections.is_empty(), "epub must yield at least one section");

    let combined: String = sections
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    // With the tuple bug each section's text was the MIME string
    // (e.g. "application/xhtml+xml") — no spaces, prefixed "application/".
    // Real extracted prose contains whitespace; this is the discriminator.
    assert!(
        combined.contains(' '),
        "extracted text should be prose, not a MIME type: {combined:?}"
    );
    assert!(
        !combined.starts_with("application/"),
        "extracted text must not be a MIME type"
    );
}
