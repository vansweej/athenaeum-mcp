//! athenaeum-parser-spike
//!
//! Permanent version-compatibility canary for pdfium-render and the epub crate.
//! Per the build sequence in docs/decision-brief.md (step 1), this crate exists
//! solely to fail loudly when a Rust toolchain, nixpkgs, or crate update breaks
//! document parsing. It is NOT part of the ingestion pipeline.

pub mod error;

use std::path::Path;

use error::SpikeError;

/// Extract all text from a PDF file using pdfium.
///
/// Requires the pdfium dynamic library to be locatable via
/// `PDFIUM_DYNAMIC_LIB_PATH` (set by the nix dev shell).
///
/// # Errors
/// Returns [`SpikeError::PdfFailed`] on any extraction error.
pub fn extract_pdf_text(path: &Path) -> Result<String, SpikeError> {
    use pdfium_render::prelude::*;

    let pdfium = Pdfium::default();
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| SpikeError::PdfFailed(e.to_string()))?;

    // page.text() borrows `page`; extract the string before the page is dropped.
    let text = doc
        .pages()
        .iter()
        .filter_map(|page| page.text().ok().map(|t| t.all()))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text)
}

/// Extract all text from an EPUB file.
///
/// # Errors
/// Returns [`SpikeError::EpubFailed`] on any extraction error.
pub fn extract_epub_text(path: &Path) -> Result<String, SpikeError> {
    use epub::doc::EpubDoc;

    let mut doc = EpubDoc::new(path).map_err(|e| SpikeError::EpubFailed(e.to_string()))?;

    let mut pages: Vec<String> = Vec::new();
    // EpubDoc starts positioned at the first item; read it before advancing.
    loop {
        if let Some((content, _mime)) = doc.get_current_str() {
            pages.push(content);
        }
        if !doc.go_next() {
            break;
        }
    }

    Ok(pages.join("\n"))
}
