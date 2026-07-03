use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Approximate character-to-token ratio for chunking estimation
const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

/// Marker returned when normalization results in empty content
const EMPTY_CONTENT_MARKER: &str = "EMPTY_CONTENT";

pub struct TextNormalizer;

static PERMISSIONS_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[d\-][rwx\-]{9}").unwrap());
static FILE_SIZE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d+[BKMGT]?\b").unwrap());
static TIMESTAMP_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2}\s+\d{1,2}:\d{2}\b")
        .unwrap()
});

impl TextNormalizer {
    /// Reduce classifier input to its distinctive content: strips filesystem
    /// metadata noise (permission bits, sizes, timestamps commonly seen in
    /// `ls`/`find` output), then drops repeated words/bigrams/trigrams so a
    /// verbose directory listing or repeated tool output doesn't dominate the
    /// classifier's limited token budget with duplication.
    pub fn normalize(text: &str) -> String {
        if text.trim().is_empty() {
            return EMPTY_CONTENT_MARKER.to_string();
        }
        let cleaned = Self::remove_filesystem_metadata(text);
        let cleaned_lower = cleaned.to_lowercase();
        let words: Vec<&str> = cleaned_lower.split_whitespace().collect();
        let deduplicated = Self::deduplicate_words(&words);
        let bigram_deduped = Self::deduplicate_ngrams(&deduplicated, 2);
        let trigram_deduped = Self::deduplicate_ngrams(&bigram_deduped, 3);
        let final_result = trigram_deduped.replace(' ', "");
        if final_result.is_empty() {
            EMPTY_CONTENT_MARKER.to_string()
        } else {
            final_result
        }
    }

    /// Split `text` into chunks no larger than `max_tokens` (estimated via
    /// [`CHARS_PER_TOKEN_ESTIMATE`]) for classifiers with a fixed input
    /// window. Splits on `char` boundaries so multi-byte UTF-8 sequences are
    /// never cut in half.
    pub fn chunk_for_classification(text: &str, max_tokens: usize) -> Vec<String> {
        let max_chars = max_tokens * CHARS_PER_TOKEN_ESTIMATE;
        if text.chars().count() <= max_chars {
            return vec![text.to_string()];
        }
        text.chars()
            .collect::<Vec<char>>()
            .chunks(max_chars.max(1))
            .map(|chunk| chunk.iter().collect())
            .collect()
    }

    fn remove_filesystem_metadata(text: &str) -> String {
        let no_perms = PERMISSIONS_PATTERN.replace_all(text, " ");
        let no_sizes = FILE_SIZE_PATTERN.replace_all(&no_perms, " ");
        let no_timestamps = TIMESTAMP_PATTERN.replace_all(&no_sizes, " ");
        let alpha_only: String = no_timestamps
            .chars()
            .map(|c| if c.is_alphabetic() { c } else { ' ' })
            .collect();
        alpha_only.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn deduplicate_words(words: &[&str]) -> String {
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for word in words {
            if seen.insert(word.to_lowercase()) {
                result.push(*word);
            }
        }
        result.join(" ")
    }

    fn deduplicate_ngrams(text: &str, n: usize) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < n {
            return text.to_string();
        }
        let mut seen_ngrams: HashSet<Vec<String>> = HashSet::new();
        let mut keep_positions: HashSet<usize> = (0..words.len()).collect();
        for i in 0..=words.len().saturating_sub(n) {
            let ngram: Vec<String> = words[i..i + n].iter().map(|s| s.to_string()).collect();
            if !seen_ngrams.insert(ngram) {
                for pos in i..i + n {
                    keep_positions.remove(&pos);
                }
            }
        }

        words
            .iter()
            .enumerate()
            .filter(|(i, _)| keep_positions.contains(i))
            .map(|(_, word)| *word)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_marker() {
        assert_eq!(TextNormalizer::normalize(""), EMPTY_CONTENT_MARKER);
        assert_eq!(TextNormalizer::normalize("   \n\t "), EMPTY_CONTENT_MARKER);
    }

    #[test]
    fn strips_filesystem_metadata_noise() {
        let input = "-rw-r--r-- 1 user staff 4096 Jan 12 09:41 report.txt";
        let normalized = TextNormalizer::normalize(input);
        assert!(!normalized.contains("4096"));
        assert!(normalized.contains("user"));
        assert!(normalized.contains("staff"));
        assert!(normalized.contains("report"));
    }

    #[test]
    fn deduplicates_repeated_words_and_ngrams() {
        let input = "delete the file delete the file now";
        let normalized = TextNormalizer::normalize(input);
        // "delete the file" collapses to one occurrence; only the trailing
        // "now" (not part of a repeated trigram) survives alongside it.
        assert_eq!(normalized, "deletethefilenow");
    }

    #[test]
    fn chunk_for_classification_returns_single_chunk_when_under_limit() {
        let chunks = TextNormalizer::chunk_for_classification("short text", 512);
        assert_eq!(chunks, vec!["short text".to_string()]);
    }

    #[test]
    fn chunk_for_classification_splits_long_text() {
        let text = "a".repeat(100);
        let chunks = TextNormalizer::chunk_for_classification(&text, 10); // max_chars = 40
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 40);
        assert_eq!(chunks[1].len(), 40);
        assert_eq!(chunks[2].len(), 20);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn chunk_for_classification_never_splits_a_multibyte_char() {
        // Each "é" is 2 UTF-8 bytes; a byte-index-based splitter would panic
        // or corrupt output here. Force a chunk boundary in the middle.
        let text = "é".repeat(50);
        let chunks = TextNormalizer::chunk_for_classification(&text, 10); // max_chars = 40
        assert_eq!(chunks.len(), 2);
        for chunk in &chunks {
            assert!(chunk.chars().all(|c| c == 'é'));
        }
        assert_eq!(chunks.concat(), text);
    }
}
