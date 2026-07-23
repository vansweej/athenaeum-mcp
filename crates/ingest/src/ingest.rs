use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::chunking::{chunk_text, ChunkingConfig};
use crate::error::IngestError;
use crate::extract::{extract_epub, extract_md, extract_pdf};
use athenaeum_core::embed::Embedder;
use athenaeum_core::Engine;

/// A text chunk extracted from a document, with full citation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub title: String,
    pub chapter: Option<String>,
    pub section: Option<String>,
    pub page: Option<u32>,
    pub text: String,
}

/// Summary returned after a successful ingestion run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSummary {
    pub documents: usize,
    pub chunks: usize,
}

/// Ingest the document at `path`, chunk it, and write chunks into the vector store.
///
/// This function:
/// 1. Determines file type (PDF or EPUB) from the file extension
/// 2. Extracts text and metadata using the appropriate extractor
/// 3. Chunks the extracted text at sentence boundaries with overlap
/// 4. Converts chunks to (source, location, text) tuples for insertion
/// 5. Calls `engine.upsert_passages()` to insert into LanceDB (dedup-aware upsert
///    keyed on the canonicalized absolute path as `doc_id`; re-ingesting a file
///    replaces prior chunks)
///
/// # Errors
/// Returns [`IngestError`] if file type is unsupported, extraction fails, the document is empty,
/// or the vector store operation fails.
pub async fn ingest<E: Embedder>(
    engine: &Engine<E>,
    path: &Path,
) -> Result<IngestSummary, IngestError> {
    let doc_id = path
        .canonicalize()
        .map_err(|e| IngestError::IoFailed(e.to_string()))?
        .to_string_lossy()
        .into_owned();

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .ok_or_else(|| IngestError::UnsupportedFileType("no file extension".to_string()))?;

    let chunks = match extension.as_str() {
        "pdf" => ingest_pdf(path).await?,
        "epub" => ingest_epub(path).await?,
        "md" | "markdown" => ingest_md(path).await?,
        _ => {
            return Err(IngestError::UnsupportedFileType(format!(
                "unsupported file type: {}",
                extension
            )))
        }
    };

    // Convert chunks to (source, location, text) tuples for batch insertion
    let passages: Vec<(String, String, String)> = chunks
        .iter()
        .map(|chunk| {
            let location = format_location(chunk);

            (chunk.title.clone(), location, chunk.text.clone())
        })
        .collect();

    let chunk_count = passages.len();

    // Insert chunks into the vector store (dedup-aware upsert by doc_id)
    engine
        .upsert_passages(&doc_id, &passages)
        .await
        .map_err(IngestError::from)?;

    Ok(IngestSummary {
        documents: 1,
        chunks: chunk_count,
    })
}

/// Ingest a PDF file
async fn ingest_pdf(path: &Path) -> Result<Vec<Chunk>, IngestError> {
    let doc = extract_pdf(path).await?;
    let mut chunks = Vec::new();

    for page in doc.pages {
        let text_chunks = chunk_text(&page.text, ChunkingConfig::default());

        for text_chunk in text_chunks {
            chunks.push(Chunk {
                title: doc.title.clone(),
                chapter: None,
                section: None,
                page: Some(page.page_number),
                text: text_chunk.text,
            });
        }
    }

    Ok(chunks)
}

/// Ingest an EPUB file
async fn ingest_epub(path: &Path) -> Result<Vec<Chunk>, IngestError> {
    let (title, sections) = extract_epub(path).await?;
    let mut chunks = Vec::new();

    for section in sections {
        let text_chunks = chunk_text(&section.text, ChunkingConfig::default());

        for text_chunk in text_chunks {
            chunks.push(Chunk {
                title: title.clone(),
                chapter: section.chapter.clone(),
                section: section.section.clone(),
                page: None,
                text: text_chunk.text,
            });
        }
    }

    Ok(chunks)
}

/// Ingest a Markdown file
async fn ingest_md(path: &Path) -> Result<Vec<Chunk>, IngestError> {
    let (title, sections) = extract_md(path).await?;
    let mut chunks = Vec::new();

    for section in sections {
        let text_chunks = chunk_text(&section.text, ChunkingConfig::default());

        for text_chunk in text_chunks {
            chunks.push(Chunk {
                title: title.clone(),
                chapter: section.chapter.clone(),
                section: section.section.clone(),
                page: None,
                text: text_chunk.text,
            });
        }
    }

    Ok(chunks)
}

/// Format a human-readable location string from a chunk's metadata.
///
/// Returns `"page N"` when `page` is set; otherwise formats
/// `"chapter > section"`, `"chapter"`, `"section"`, or `"unknown"`.
fn format_location(chunk: &Chunk) -> String {
    if let Some(page) = chunk.page {
        format!("page {}", page)
    } else {
        let mut loc = String::new();
        if let Some(ref chapter) = chunk.chapter {
            loc.push_str(chapter);
        }
        if let Some(ref section) = chunk.section {
            if !loc.is_empty() {
                loc.push_str(" > ");
            }
            loc.push_str(section);
        }
        if loc.is_empty() {
            "unknown".to_string()
        } else {
            loc
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn ingest_unsupported_file_type() {
        // This test can't actually call ingest without an Engine,
        // but we can test the file type detection logic separately
        let path = Path::new("test.txt");
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase());

        assert_eq!(extension, Some("txt".to_string()));
    }

    #[tokio::test]
    async fn ingest_no_extension() {
        let path = Path::new("testfile");
        let extension = path.extension();
        assert!(extension.is_none());
    }

    #[test]
    fn format_location_page_only() {
        let chunk = Chunk {
            title: "Test".into(),
            chapter: None,
            section: None,
            page: Some(42),
            text: "content".into(),
        };
        assert_eq!(format_location(&chunk), "page 42");
    }

    #[test]
    fn format_location_chapter_and_section() {
        let chunk = Chunk {
            title: "Test".into(),
            chapter: Some("Chapter 1".into()),
            section: Some("Section A".into()),
            page: None,
            text: "content".into(),
        };
        assert_eq!(format_location(&chunk), "Chapter 1 > Section A");
    }

    #[test]
    fn format_location_chapter_only() {
        let chunk = Chunk {
            title: "Test".into(),
            chapter: Some("Chapter 1".into()),
            section: None,
            page: None,
            text: "content".into(),
        };
        assert_eq!(format_location(&chunk), "Chapter 1");
    }

    #[test]
    fn format_location_section_only() {
        let chunk = Chunk {
            title: "Test".into(),
            chapter: None,
            section: Some("Appendix".into()),
            page: None,
            text: "content".into(),
        };
        assert_eq!(format_location(&chunk), "Appendix");
    }

    #[test]
    fn format_location_none_returns_unknown() {
        let chunk = Chunk {
            title: "Test".into(),
            chapter: None,
            section: None,
            page: None,
            text: "content".into(),
        };
        assert_eq!(format_location(&chunk), "unknown");
    }
}
