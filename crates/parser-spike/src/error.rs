use thiserror::Error;

/// Errors produced by the parser spike.
#[derive(Debug, Error)]
pub enum SpikeError {
    #[error("pdf extraction failed: {0}")]
    PdfFailed(String),

    #[error("epub extraction failed: {0}")]
    EpubFailed(String),
}
