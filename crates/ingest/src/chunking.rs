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
/// 1. Splits text into sentences using simple string matching
/// 2. Groups sentences into chunks targeting min_tokens to max_tokens
/// 3. Adds overlap between chunks
/// 4. Uses whitespace word count (1 word ≈ 1.3 tokens)
pub fn chunk_text(text: &str, config: ChunkingConfig) -> Vec<TextChunk> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    // Split into sentences using simple string matching
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

/// Split text into sentences using simple string matching
/// Splits on sentence-ending punctuation (. ! ?) followed by whitespace and uppercase letter,
/// or at the end of the string.
fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current_sentence = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        current_sentence.push(ch);

        // Check if this is a sentence-ending punctuation
        if ch == '.' || ch == '!' || ch == '?' {
            // Look ahead to see if we should end the sentence
            let mut should_end = false;

            if chars.peek().is_none() {
                // End of text
                should_end = true;
            } else if chars.peek() == Some(&' ') {
                // Peek further to see if there's an uppercase letter after the space
                let mut temp_chars = chars.clone();
                temp_chars.next(); // skip the space

                if let Some(&next_ch) = temp_chars.peek() {
                    if next_ch.is_uppercase() || next_ch == '\n' {
                        should_end = true;
                        // Consume the space
                        chars.next();
                    }
                }
            }

            if should_end {
                let trimmed = current_sentence.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current_sentence.clear();
            }
        }
    }

    // Add any remaining text as a sentence
    let trimmed = current_sentence.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
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

    #[test]
    fn test_split_into_sentences() {
        let text = "First sentence. Second sentence! Third sentence?";
        let sentences = split_into_sentences(text);
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0], "First sentence.");
        assert_eq!(sentences[1], "Second sentence!");
        assert_eq!(sentences[2], "Third sentence?");
    }
}
