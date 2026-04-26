//! Markdown chunker — Phase 3-D2a.
//!
//! Splits a note's body into **embeddable chunks** with stable byte offsets
//! back into the source file. Offsets let the chat UI (D2b) highlight the
//! exact source region when rendering `[[note-title]]` citations.
//!
//! ## Strategy (v1)
//!
//! 1. Strip the frontmatter block (`---\n…\n---\n`) — it is metadata, not
//!    content the user wants to semantically search.
//! 2. Split the remaining body by **paragraph boundaries** (`\n\n+`).
//! 3. For each paragraph whose estimated token count exceeds
//!    [`MAX_CHUNK_TOKENS`], split further by **sentence boundaries**
//!    (period / question / exclamation, ASCII or CJK).
//! 4. Drop any chunk whose trimmed body is empty.
//!
//! ## Why not semantic / markdown-aware chunking?
//!
//! A paragraph + sentence heuristic is good enough for the v1 RAG retrieval
//! quality target and keeps the logic purely byte-based (no parser state).
//! D2+ can swap in a smarter chunker (code-fence aware, heading-aware) by
//! replacing this module alone — the output shape stays stable.
//!
//! ## Consumer status
//!
//! As of D2a.2 this module is still **library-only** — consumers land in
//! D2a.3 when we wire the embed-note IPC + filewatcher. Module-scoped
//! `allow(dead_code)` suppresses the "no users yet" warnings until then.
//!
//! ## Offset contract
//!
//! `offset_start` and `offset_end` are **byte offsets into the original
//! input string** (the full note body including frontmatter). They are
//! inclusive-exclusive and always satisfy `0 ≤ start ≤ end ≤ input.len()`.
//! Callers may safely do `&input[start..end]` to recover the raw slice.

#![allow(dead_code)]

// ── Public types ──────────────────────────────────────────────────────────────

/// One embeddable slice of a note's body with its source offsets preserved.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// 0-based ordinal within the note — used as the stable SQL primary
    /// key component so re-chunking can diff-upsert without touching
    /// unchanged chunks.
    pub chunk_index: u32,
    /// Byte offset of the first character of this chunk in the source string.
    pub offset_start: u32,
    /// Byte offset *past the last character* of this chunk.
    pub offset_end: u32,
    /// The chunk body — already trimmed of surrounding whitespace.
    pub text: String,
    /// Rough token count estimate (chars / 4). Used by the dry-run cost
    /// estimator; real providers may report authoritative counts.
    pub est_tokens: u32,
}

/// Upper bound (in estimated tokens) before a paragraph is split at sentence
/// boundaries. Chosen so a typical OpenAI `text-embedding-3-small` batch
/// (context 8192) can hold ~10 chunks comfortably.
pub const MAX_CHUNK_TOKENS: u32 = 800;

// ── Public API ────────────────────────────────────────────────────────────────

/// Split a note body into chunks. Always returns in `chunk_index` order.
///
/// Empty input / frontmatter-only input yields an empty vector.
pub fn chunk_markdown(body: &str) -> Vec<Chunk> {
    let (body_start, stripped) = strip_frontmatter(body);
    let mut out = Vec::new();
    let mut idx: u32 = 0;

    for (para_start, para_end) in split_paragraphs(stripped) {
        // Offsets relative to the original input.
        let abs_start = body_start + para_start;
        let abs_end = body_start + para_end;

        let text = &body[abs_start..abs_end];
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let est = est_tokens(trimmed);
        if est <= MAX_CHUNK_TOKENS {
            out.push(Chunk {
                chunk_index: idx,
                offset_start: abs_start as u32,
                offset_end: abs_end as u32,
                text: trimmed.to_string(),
                est_tokens: est,
            });
            idx += 1;
        } else {
            // Split this paragraph at sentence boundaries.
            for (s_rel_start, s_rel_end) in split_sentences(text) {
                let s_abs_start = abs_start + s_rel_start;
                let s_abs_end = abs_start + s_rel_end;
                let sub = body[s_abs_start..s_abs_end].trim();
                if sub.is_empty() {
                    continue;
                }
                out.push(Chunk {
                    chunk_index: idx,
                    offset_start: s_abs_start as u32,
                    offset_end: s_abs_end as u32,
                    text: sub.to_string(),
                    est_tokens: est_tokens(sub),
                });
                idx += 1;
            }
        }
    }

    out
}

/// Cheap token count estimate — 4 chars/token, a widely-used rule of thumb
/// that is within ±20 % for English and ~2x off for CJK (conservative side,
/// so it tends to over-estimate cost, not under). Rounded up.
pub fn est_tokens(s: &str) -> u32 {
    let chars = s.chars().count();
    ((chars + 3) / 4) as u32
}

// ── Internals ─────────────────────────────────────────────────────────────────

/// If `input` begins with a YAML frontmatter block (`---\n…\n---\n`), return
/// `(body_start, &input[body_start..])`. Otherwise `(0, input)`.
fn strip_frontmatter(input: &str) -> (usize, &str) {
    // Frontmatter must begin at byte 0 with "---\n" or "---\r\n".
    let rest = input
        .strip_prefix("---\n")
        .or_else(|| input.strip_prefix("---\r\n"));
    let Some(after_open) = rest else {
        return (0, input);
    };
    let after_open_offset = input.len() - after_open.len();

    // Find the closing "\n---\n" (or EOF variants).
    // We search for "\n---" followed by newline or end-of-input.
    let mut search_from = 0usize;
    while let Some(rel) = after_open[search_from..].find("\n---") {
        let abs = search_from + rel;
        let after_close = &after_open[abs + 4..]; // past "\n---"
                                                  // Accept "\n---\n", "\n---\r\n", or "\n---" at EOF.
        if after_close.is_empty() {
            let body_start = after_open_offset + after_open.len();
            return (body_start, "");
        }
        if let Some(stripped) = after_close.strip_prefix('\n') {
            let body_start = after_open_offset + abs + 4 + 1;
            return (body_start, stripped);
        }
        if let Some(stripped) = after_close.strip_prefix("\r\n") {
            let body_start = after_open_offset + abs + 4 + 2;
            return (body_start, stripped);
        }
        // "---" embedded inside a line of dashes — skip past this hit.
        search_from = abs + 4;
    }

    // Unterminated frontmatter — treat whole input as body, conservatively.
    (0, input)
}

/// Return `(start_byte, end_byte)` pairs for every paragraph in `body`.
///
/// A paragraph is maximal run of non-empty lines, separated by one or more
/// blank lines (`\n\n+`). Byte offsets are relative to `body`.
fn split_paragraphs(body: &str) -> Vec<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let len = bytes.len();

    while i < len {
        // Skip leading blank lines (runs of "\n" / "\r\n" / whitespace-only).
        while i < len && is_blank_line_start(bytes, i) {
            i = advance_line(bytes, i);
        }
        if i >= len {
            break;
        }

        let start = i;
        // Consume lines until we hit a blank line or EOF.
        while i < len && !is_blank_line_start(bytes, i) {
            i = advance_line(bytes, i);
        }
        let end = i;
        out.push((start, end));
    }

    out
}

/// Does position `i` (start of a line) begin a blank line? A blank line is
/// either `\n`, `\r\n`, or EOF right here.
fn is_blank_line_start(bytes: &[u8], i: usize) -> bool {
    if i >= bytes.len() {
        return false; // EOF is not a blank line — caller handles EOF.
    }
    match bytes[i] {
        b'\n' => true,
        b'\r' if bytes.get(i + 1) == Some(&b'\n') => true,
        _ => false,
    }
}

/// Advance past the line starting at `i`, returning the position of the
/// next line's start (or `bytes.len()` at EOF).
fn advance_line(bytes: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < bytes.len() && bytes[j] != b'\n' {
        j += 1;
    }
    if j < bytes.len() {
        j + 1 // consume the \n
    } else {
        j
    }
}

/// Return `(start, end)` byte ranges for sentences within `para` (relative
/// to `para`'s start).
///
/// Sentence boundary = one of `. ! ? 。 ！ ？` followed by whitespace or EOF.
/// The terminator character is **included** in the returned range.
fn split_sentences(para: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = para.as_bytes();
    let len = bytes.len();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < len {
        let c = bytes[i];
        // ASCII terminators.
        let is_ascii_term = matches!(c, b'.' | b'!' | b'?');
        // CJK terminators ('。' = 0xE3 0x80 0x82, '！' = 0xEF 0xBC 0x81,
        // '？' = 0xEF 0xBC 0x9F). Detected by their multi-byte prefix.
        let cjk_len = cjk_terminator_len(&bytes[i..]);

        if is_ascii_term || cjk_len > 0 {
            let term_end = if cjk_len > 0 { i + cjk_len } else { i + 1 };
            // Boundary confirmed when next byte is whitespace or EOF.
            let next_is_ws =
                term_end >= len || matches!(bytes[term_end], b' ' | b'\t' | b'\n' | b'\r');
            if next_is_ws {
                out.push((start, term_end));
                // Skip whitespace to start next sentence.
                let mut k = term_end;
                while k < len && matches!(bytes[k], b' ' | b'\t' | b'\n' | b'\r') {
                    k += 1;
                }
                start = k;
                i = k;
                continue;
            }
        }
        // Advance by one UTF-8 codepoint.
        i += utf8_len(bytes[i]);
    }

    if start < len {
        out.push((start, len));
    }
    out
}

/// Byte length of the UTF-8 codepoint starting with `first`.
fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1, // invalid — treat as 1 so we make forward progress
    }
}

/// If `bytes` starts with a CJK sentence terminator (`。！？`), return its
/// byte length; else 0.
fn cjk_terminator_len(bytes: &[u8]) -> usize {
    // 。 = E3 80 82
    if bytes.starts_with(&[0xE3, 0x80, 0x82]) {
        return 3;
    }
    // ！ = EF BC 81
    if bytes.starts_with(&[0xEF, 0xBC, 0x81]) {
        return 3;
    }
    // ？ = EF BC 9F
    if bytes.starts_with(&[0xEF, 0xBC, 0x9F]) {
        return 3;
    }
    0
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── est_tokens ──

    #[test]
    fn est_tokens_rounds_up() {
        assert_eq!(est_tokens(""), 0);
        assert_eq!(est_tokens("abc"), 1);
        assert_eq!(est_tokens("abcd"), 1);
        assert_eq!(est_tokens("abcde"), 2);
    }

    #[test]
    fn est_tokens_counts_codepoints_not_bytes() {
        // "你好" = 2 chars / 6 bytes → 1 token rounded up.
        assert_eq!(est_tokens("你好"), 1);
        // 5 CJK chars → 2 tokens.
        assert_eq!(est_tokens("你好世界人"), 2);
    }

    // ── strip_frontmatter ──

    #[test]
    fn strip_frontmatter_absent() {
        let (start, body) = strip_frontmatter("hello\n\nworld");
        assert_eq!(start, 0);
        assert_eq!(body, "hello\n\nworld");
    }

    #[test]
    fn strip_frontmatter_present() {
        let input = "---\ntitle: foo\ntype: note\n---\nhello body\n";
        let (start, body) = strip_frontmatter(input);
        assert_eq!(&input[start..], body);
        assert_eq!(body, "hello body\n");
    }

    #[test]
    fn strip_frontmatter_unterminated_treats_as_body() {
        let input = "---\ntitle: foo\nno closer here";
        let (start, body) = strip_frontmatter(input);
        assert_eq!(start, 0);
        assert_eq!(body, input);
    }

    #[test]
    fn strip_frontmatter_with_crlf() {
        let input = "---\r\ntitle: foo\r\n---\r\nbody";
        let (_start, body) = strip_frontmatter(input);
        assert_eq!(body, "body");
    }

    // ── chunk_markdown ──

    #[test]
    fn chunk_empty_yields_empty() {
        assert!(chunk_markdown("").is_empty());
        assert!(chunk_markdown("\n\n\n").is_empty());
    }

    #[test]
    fn chunk_frontmatter_only_yields_empty() {
        assert!(chunk_markdown("---\ntitle: x\n---\n").is_empty());
    }

    #[test]
    fn chunk_single_paragraph() {
        let input = "hello world";
        let chunks = chunk_markdown(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].offset_start, 0);
        assert_eq!(chunks[0].offset_end, 11);
        assert_eq!(chunks[0].text, "hello world");
    }

    #[test]
    fn chunk_multiple_paragraphs() {
        let input = "first para\n\nsecond para\n\nthird";
        let chunks = chunk_markdown(input);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].text, "first para");
        assert_eq!(chunks[1].text, "second para");
        assert_eq!(chunks[2].text, "third");
        // chunk_index is sequential.
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].chunk_index, 1);
        assert_eq!(chunks[2].chunk_index, 2);
    }

    #[test]
    fn chunk_offsets_slice_correctly() {
        let input = "aaa\n\nbbbb\n\ncccccc";
        let chunks = chunk_markdown(input);
        for c in &chunks {
            let slice = &input[c.offset_start as usize..c.offset_end as usize];
            assert!(slice.contains(c.text.as_str()));
        }
    }

    #[test]
    fn chunk_frontmatter_offsets_are_absolute() {
        // Body starts at byte 24 (after "---\ntitle: x\n---\n" = 17 bytes? let's compute)
        let fm = "---\ntitle: foo\n---\n";
        let body = "hello";
        let input = format!("{fm}{body}");
        let chunks = chunk_markdown(&input);
        assert_eq!(chunks.len(), 1);
        // The chunk should slice back to "hello".
        let slice = &input[chunks[0].offset_start as usize..chunks[0].offset_end as usize];
        assert_eq!(slice.trim(), "hello");
    }

    #[test]
    fn chunk_large_para_splits_by_sentence() {
        // Build a paragraph of many short English sentences to exceed
        // MAX_CHUNK_TOKENS without hitting a paragraph break.
        let sentence = "This is a moderately long sentence used in testing. ";
        let body = sentence.repeat(200); // ~10_000 chars → ~2500 tokens
        let chunks = chunk_markdown(&body);
        // Expect multiple chunks because sentence splitting kicked in.
        assert!(
            chunks.len() > 1,
            "expected sentence-level split, got {} chunk(s)",
            chunks.len()
        );
        // Each chunk must be below the limit.
        for c in &chunks {
            assert!(
                c.est_tokens <= MAX_CHUNK_TOKENS,
                "chunk {} exceeds limit with {} tokens",
                c.chunk_index,
                c.est_tokens
            );
        }
    }

    #[test]
    fn chunk_cjk_sentence_split() {
        let para = "第一句话测试内容。第二句话也是测试。第三句完整。";
        // Single paragraph, small enough — should be ONE chunk (no split).
        let chunks = chunk_markdown(para);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("第一句"));
        assert!(chunks[0].text.contains("第三句"));
    }

    #[test]
    fn chunk_preserves_inline_markdown() {
        let input = "This has **bold** and _italic_ and `code`.\n\nAnd [[wiki]] links.";
        let chunks = chunk_markdown(input);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].text.contains("**bold**"));
        assert!(chunks[1].text.contains("[[wiki]]"));
    }

    // ── split_sentences (internal) ──

    #[test]
    fn split_sentences_basic_ascii() {
        let ranges = split_sentences("One. Two! Three? Four");
        assert_eq!(ranges.len(), 4);
    }

    #[test]
    fn split_sentences_decimal_point_is_not_boundary() {
        // "3.14" — the '.' is not followed by whitespace, so no split.
        let ranges = split_sentences("Pi is 3.14 approximately.");
        assert_eq!(ranges.len(), 1);
    }
}
