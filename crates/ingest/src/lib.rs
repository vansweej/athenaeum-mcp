pub mod chunking;
pub mod discovery;
pub mod error;
pub mod extract;
pub mod ingest;

pub use chunking::{chunk_text, ChunkingConfig, TextChunk};
pub use discovery::discover_documents;
pub use error::IngestError;
pub use extract::{EpubSection, ExtractedDocument, ExtractedPage};
pub use ingest::{ingest, Chunk, IngestSummary};
