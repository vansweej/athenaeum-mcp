pub mod error;
pub mod ingest;
pub mod extract;
pub mod chunking;

pub use error::IngestError;
pub use ingest::{Chunk, IngestSummary, ingest};
pub use extract::{ExtractedDocument, ExtractedPage, EpubSection};
pub use chunking::{ChunkingConfig, TextChunk, chunk_text};

