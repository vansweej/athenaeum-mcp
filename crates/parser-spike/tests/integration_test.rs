use std::path::Path;

use athenaeum_parser_spike::{extract_epub_text, extract_pdf_text};

// not corpus — self-authored throwaway fixtures for toolchain validation only

#[test]
fn extracts_text_from_sample_pdf() {
    let path = Path::new("tests/fixtures/sample.pdf");
    let result = extract_pdf_text(path).expect("pdf extraction should succeed");
    assert!(
        !result.trim().is_empty(),
        "extracted text must be non-empty"
    );
}

#[test]
fn extracts_text_from_sample_epub() {
    let path = Path::new("tests/fixtures/sample.epub");
    let result = extract_epub_text(path).expect("epub extraction should succeed");
    assert!(
        !result.trim().is_empty(),
        "extracted text must be non-empty"
    );
}
