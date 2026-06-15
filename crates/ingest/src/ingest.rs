use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::IngestError;

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
#[derive(Debug, Clone)]
pub struct IngestSummary {
    pub documents: usize,
    pub chunks:    usize,
}

/// Ingest the document at `path`, chunk it, and write chunks into the vector store.
///
/// # Errors
/// Returns [`IngestError::NotImplemented`] until parsing and storage are wired.
pub async fn ingest(path: &Path) -> Result<IngestSummary, IngestError> {
    let _ = path;
    Err(IngestError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn ingest_returns_not_implemented() {
        let result = ingest(Path::new("/dev/null")).await;
        assert!(matches!(result, Err(IngestError::NotImplemented)));
    }
}
