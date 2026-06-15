pub mod error;
pub mod ingest;

pub use error::IngestError;
pub use ingest::{Chunk, IngestSummary, ingest};
