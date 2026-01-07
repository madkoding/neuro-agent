use std::cmp::min;

/// Find the nearest valid char boundary at or before the given byte index
fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Find the nearest valid char boundary at or after the given byte index
fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Simple tokenizer-lite chunker based on characters/words.
/// Keeps chunks <= `max_chars` and overlaps by `overlap_chars` to preserve context.
pub fn chunk_text(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    if text.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut i = 0;
    let len = text.len();

    while i < len {
        // Calculate end position, ensuring it's a valid char boundary
        let raw_end = min(len, i + max_chars);
        let end = floor_char_boundary(text, raw_end);

        // Try to cut at last newline or space before end to avoid breaking words
        let mut cut = end;
        if cut < len && cut > i {
            let slice = &text[i..cut];
            if let Some(idx) = slice.rfind('\n') {
                cut = i + idx + 1;
            } else if let Some(idx) = slice.rfind(' ') {
                cut = i + idx + 1;
            }
        }

        // Ensure cut is a valid char boundary
        cut = ceil_char_boundary(text, cut);

        // Ensure we always make progress
        if cut <= i {
            cut = ceil_char_boundary(text, min(i + 1, len));
        }

        // Safety check: ensure cut doesn't exceed len
        cut = min(cut, len);

        if cut > i {
            let chunk = text[i..cut].trim().to_string();
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
        }

        // Move forward, ensuring we always advance
        let next_i = if overlap_chars < (cut.saturating_sub(i)) {
            cut.saturating_sub(overlap_chars)
        } else {
            cut
        };

        // Ensure next_i is a valid char boundary
        let next_i = floor_char_boundary(text, next_i);

        // Safety: always advance at least by 1 char to prevent infinite loop
        if next_i <= i {
            i = ceil_char_boundary(text, cut.max(i + 1));
        } else {
            i = next_i;
        }

        // Exit if we've reached the end
        if cut >= len || i >= len {
            break;
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_basic() {
        let text = "a b c d e f g h i j k l m n o p q r s t";
        let chunks = chunk_text(text, 10, 3);
        assert!(!chunks.is_empty());
        for c in chunks.iter() {
            assert!(c.len() <= 13); // max_chars + overlap
        }
    }
}
