use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::IngestError;
use crate::extract::{extract_pdf, extract_epub};
use crate::chunking::{chunk_text, ChunkingConfig};
use athenaeum_core::Engine;
use athenaeum_core::embed::Embedder;

/// A text chunk extracted from a document, with full citation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub title:   String,
    pub chapter: Option<String>,
    pub section: Option<String>,
    pub page:    Option<u32>,
    pub text:    String,
}

/// Summary returned after a successful ingestion run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSummary {
    pub documents: usize,
    pub chunks:    usize,
}

/// Ingest the document at `path`, chunk it, and write chunks into the vector store.
///
/// This function:
/// 1. Determines file type (PDF or EPUB) from the file extension
/// 2. Extracts text and metadata using the appropriate extractor
/// 3. Chunks the extracted text at sentence boundaries with overlap
/// 4. Converts chunks to (source, location, text) tuples for batch insertion
/// 5. Calls `engine.add_passages()` to insert into LanceDB
///
/// # Errors
/// Returns [`IngestError`] if file type is unsupported, extraction fails, the document is empty,
/// or the vector store operation fails.
pub async fn ingest<E: Embedder>(
    engine: &Engine<E>,
    path: &Path,
) -> Result<IngestSummary, IngestError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .ok_or_else(|| IngestError::UnsupportedFileType("no file extension".to_string()))?;

    let chunks = match extension.as_str() {
        "pdf" => ingest_pdf(path).await?,
        "epub" => ingest_epub(path).await?,
        _ => return Err(IngestError::UnsupportedFileType(format!(
            "unsupported file type: {}",
            extension
        ))),
    };

    // Convert chunks to (source, location, text) tuples for batch insertion
    let passages: Vec<(String, String, String)> = chunks
        .iter()
        .map(|chunk| {
            let location = if let Some(page) = chunk.page {
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
            };

            (chunk.title.clone(), location, chunk.text.clone())
        })
        .collect();

    let chunk_count = passages.len();
    
    // Insert chunks into the vector store
    engine
        .add_passages(&passages)
        .await
        .map_err(|e| IngestError::from(e))?;

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

#[cfg(test)]
mod tests {
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
}
