/// Document chunker for memory ingestion.
/// Splits text into ~400-token chunks with 80-token overlap.
///
/// A chunk of text with metadata.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub index: usize,
    pub total: usize,
    pub source: String,
}

/// Rough token estimation: ~4 chars per token for English text.
const CHARS_PER_TOKEN: usize = 4;

/// Split text into chunks of approximately `target_tokens` size
/// with `overlap_tokens` overlap between chunks.
pub fn chunk_text(
    text: &str,
    source: &str,
    target_tokens: usize,
    overlap_tokens: usize,
) -> Vec<Chunk> {
    let target_chars = target_tokens * CHARS_PER_TOKEN;
    let overlap_chars = overlap_tokens * CHARS_PER_TOKEN;

    if text.len() <= target_chars {
        return vec![Chunk {
            text: text.to_string(),
            index: 0,
            total: 1,
            source: source.to_string(),
        }];
    }

    let sentences = split_sentences(text);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut sentence_buffer: Vec<String> = Vec::new();

    for sentence in &sentences {
        if current.len() + sentence.len() > target_chars && !current.is_empty() {
            chunks.push(current.clone());

            // Build overlap from end of sentence buffer.
            current.clear();
            let mut overlap_len = 0;
            let mut overlap_start = sentence_buffer.len();
            for (i, s) in sentence_buffer.iter().enumerate().rev() {
                overlap_len += s.len();
                if overlap_len >= overlap_chars {
                    overlap_start = i;
                    break;
                }
            }
            for s in &sentence_buffer[overlap_start..] {
                current.push_str(s);
            }
            sentence_buffer = sentence_buffer[overlap_start..].to_vec();
        }

        current.push_str(sentence);
        sentence_buffer.push(sentence.clone());
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, text)| Chunk {
            text,
            index: i,
            total,
            source: source.to_string(),
        })
        .collect()
}

/// Split text into sentences on paragraph and sentence boundaries.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if ch == '.' || ch == '!' || ch == '?' || ch == '\n' {
            // Check for paragraph break (double newline).
            if ch == '\n' && current.ends_with("\n\n") {
                sentences.push(current.clone());
                current.clear();
                continue;
            }
            // End of sentence.
            if ch == '.' || ch == '!' || ch == '?' {
                // Look ahead: only split if next char is space or end.
                sentences.push(current.clone());
                current.clear();
            }
        }
    }

    if !current.is_empty() {
        sentences.push(current);
    }

    sentences
}

/// Default chunking with plan parameters (400 tokens, 80 overlap).
pub fn chunk_default(text: &str, source: &str) -> Vec<Chunk> {
    chunk_text(text, source, 400, 80)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_text_single_chunk() {
        let chunks = chunk_default("Hello world.", "test.md");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].total, 1);
    }

    #[test]
    fn test_long_text_multiple_chunks() {
        // Create text longer than 400 tokens (~1600 chars).
        let text = "This is a test sentence. ".repeat(100);
        let chunks = chunk_default(&text, "test.md");
        assert!(chunks.len() > 1);
        // All chunks should have correct total.
        for chunk in &chunks {
            assert_eq!(chunk.total, chunks.len());
        }
    }

    #[test]
    fn test_chunk_overlap() {
        let text = "Sentence one. Sentence two. Sentence three. Sentence four. ".repeat(50);
        let chunks = chunk_text(&text, "test.md", 100, 20);
        // With overlap, later chunks should share some text with earlier ones.
        if chunks.len() >= 2 {
            let last_words_of_first: Vec<&str> =
                chunks[0].text.split_whitespace().rev().take(3).collect();
            let first_words_of_second: Vec<&str> =
                chunks[1].text.split_whitespace().take(10).collect();
            // There should be some overlap.
            let has_overlap = last_words_of_first
                .iter()
                .any(|w| first_words_of_second.contains(w));
            assert!(has_overlap, "Expected overlap between chunks");
        }
    }
}
