use regex::Regex;

/// Configuration for text chunking
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Target minimum tokens per chunk (default: 500)
    pub min_tokens: usize,
    /// Target maximum tokens per chunk (default: 1000)
    pub max_tokens: usize,
    /// Overlap tokens between chunks (default: 100)
    pub overlap_tokens: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            min_tokens: 500,
            max_tokens: 1000,
            overlap_tokens: 100,
        }
    }
}

/// A chunk of text with metadata
#[derive(Debug, Clone)]
pub struct TextChunk {
    pub text: String,
    pub token_count: usize,
}

/// Chunk text at sentence boundaries with overlap
///
/// This function:
/// 1. Splits text into sentences using regex
/// 2. Groups sentences into chunks targeting min_tokens to max_tokens
/// 3. Adds overlap between chunks
/// 4. Uses whitespace word count (1 word ≈ 1.3 tokens)
pub fn chunk_text(text: &str, config: ChunkingConfig) -> Vec<TextChunk> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    // Split into sentences using regex
    let sentences = split_into_sentences(text);
    if sentences.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut current_tokens = 0;

    for sentence in sentences {
        let sentence_tokens = estimate_tokens(&sentence);

        // If adding this sentence would exceed max_tokens and we have content, start a new chunk
        if current_tokens + sentence_tokens > config.max_tokens && !current_chunk.is_empty() {
            chunks.push(TextChunk {
                text: current_chunk.clone(),
                token_count: current_tokens,
            });

            // Add overlap: keep the last part of the previous chunk
            let overlap_text = get_overlap_text(&current_chunk, config.overlap_tokens);
            current_chunk = overlap_text;
        }

        // Add sentence to current chunk
        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(&sentence);
        current_tokens = estimate_tokens(&current_chunk);

        // If we've reached min_tokens, we can start a new chunk on the next sentence
        // (but we'll wait until we exceed max_tokens to actually do so)
    }

    // Add the final chunk
    if !current_chunk.is_empty() {
        chunks.push(TextChunk {
            text: current_chunk,
            token_count: current_tokens,
        });
    }

    chunks
}

/// Split text into sentences using regex
fn split_into_sentences(text: &str) -> Vec<String> {
    // Match sentence boundaries: period, question mark, exclamation mark followed by space or end of string
    let sentence_regex = Regex::new(r"(?<=[.!?])\s+(?=[A-Z])|(?<=[.!?])$").unwrap();

    let mut sentences = Vec::new();
    let mut current_sentence = String::new();

    for part in sentence_regex.split(text) {
        if !part.is_empty() {
            if !current_sentence.is_empty() {
                current_sentence.push(' ');
            }
            current_sentence.push_str(part);

            // Check if this part ends with sentence-ending punctuation
            if part.ends_with('.') || part.ends_with('!') || part.ends_with('?') {
                sentences.push(current_sentence.trim().to_string());
                current_sentence.clear();
            }
        }
    }

    // Add any remaining text as a sentence
    if !current_sentence.is_empty() {
        sentences.push(current_sentence.trim().to_string());
    }

    sentences
}

/// Estimate token count using whitespace word count (1 word ≈ 1.3 tokens)
fn estimate_tokens(text: &str) -> usize {
    let word_count = text.split_whitespace().count();
    ((word_count as f64) * 1.3).ceil() as usize
}

/// Get the last N tokens of text for overlap
fn get_overlap_text(text: &str, overlap_tokens: usize) -> String {
    let overlap_words = ((overlap_tokens as f64) / 1.3).ceil() as usize;
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.len() <= overlap_words {
        return text.to_string();
    }

    let start_idx = words.len() - overlap_words;
    words[start_idx..].join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_basic() {
        let text = "This is a test. This is another sentence. And a third one.";
        let config = ChunkingConfig {
            min_tokens: 5,
            max_tokens: 20,
            overlap_tokens: 2,
        };

        let chunks = chunk_text(text, config);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| !c.text.is_empty()));
    }

    #[test]
    fn test_estimate_tokens() {
        let text = "This is a test";
        let tokens = estimate_tokens(text);
        // 4 words * 1.3 = 5.2, rounded up to 6
        assert_eq!(tokens, 6);
    }

    #[test]
    fn test_empty_text() {
        let chunks = chunk_text("", ChunkingConfig::default());
        assert!(chunks.is_empty());
    }
}
