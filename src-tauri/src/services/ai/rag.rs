//! Retrieval-augmented generation helper (P3-D2b.5).
//!
//! Bridges [`embedding_store`](super::embedding_store) → [`provider`](super::provider)
//! into a single "given the user's current prompt, what context should
//! we prepend as a system message" function.
//!
//! Design constraints:
//!
//! 1. **Best-effort, never fatal.** If embedding is unconfigured, if the
//!    embedding store is empty, if the query embed call fails — we just
//!    return no context and let the chat continue. RAG is a quality
//!    bonus, not a required pre-condition.
//! 2. **Model-aware.** Search is namespaced by the embedding model name,
//!    mirroring what we do in `ai_related_notes`. Mixing two embedding
//!    models in the same vault already confused retrieval at D2a.5;
//!    D2b.5 inherits the same "latest model wins" heuristic.
//! 3. **Cheap when inapplicable.** The short-circuit checks live at the
//!    top of [`embed_query`] / [`search_and_format`] so the hot path
//!    (no embeddings configured) adds just a handful of guards per send.
//!
//! ## Not in scope for v1 (D2b.5)
//!
//! - **Filter by `related_note`.** We currently do a global top-K over the
//!    whole embedding store. Filtering to just the chunks from the related
//!    note would be a ~10-line addition (new `search_in_note` method),
//!    but empirically a global search surfaces related notes naturally
//!    via cosine similarity and frees the user from "correct note
//!    selection" being a prerequisite for good retrieval. Revisit if
//!    users report irrelevant context leaking from unrelated notes.
//! - **Per-chunk boosting by same-note proximity.** Would need joining
//!    against the index DB; the extra plumbing is not worth it at
//!    `K = 5`.
//! - **Token-aware context truncation.** We cap at `MAX_CONTEXT_CHARS`
//!    characters (combined across all chunks), leaving the downstream
//!    history truncator to do the token-level squeeze. Good enough for
//!    8k–32k ctx windows; revisit when supporting larger models.

use serde::Serialize;

use crate::services::ai::embedding_store::{EmbeddingStore, SearchHit};
use crate::services::ai::provider::{AiProvider, ChatRole, ChatTurn, EmbedRequest};

/// Default number of chunks retrieved for a chat turn's RAG context.
/// Chosen to fit 5 × ~700-char chunks ≈ 3500 chars ≈ 1k tokens,
/// leaving ample room for conversation history in an 8k ctx window.
pub const DEFAULT_TOP_K: usize = 5;

/// Aggregate hard cap on combined context length, in characters. Acts
/// as a second-line defence if chunks are unusually long (e.g. a note
/// with few but huge paragraphs). We prefer "truncate mid-chunk" to
/// "silently drop a chunk" because partial context is still signal.
const MAX_CONTEXT_CHARS: usize = 6_000;

/// A single cited chunk surfaced back to the frontend so the UI can
/// later render a "sources used" footer or a sidebar. We deliberately
/// carry the same shape as [`SearchHit`] minus the raw vector data so
/// the IPC payload stays small.
#[derive(Debug, Clone, Serialize)]
pub struct RagCitation {
    pub note_rel_path: String,
    pub chunk_index: u32,
    pub offset_start: u32,
    pub offset_end: u32,
    /// Cosine similarity in `[0, 1]`. Higher = more similar.
    pub score: f32,
    /// First ~160 chars of the chunk text, for UI preview. The full
    /// chunk text goes only to the provider.
    pub preview: String,
}

/// Aggregate return of the RAG pipeline: the synthetic system
/// message (when non-empty) plus the structured list of what was fed
/// in. Both are `None` when retrieval produced nothing usable.
#[derive(Debug, Clone)]
pub struct RagContext {
    pub system_turn: ChatTurn,
    pub citations: Vec<RagCitation>,
}

/// Embed the user's current prompt with `embed_provider`. Async so
/// callers can run this without holding any mutex across the await
/// (the [`EmbeddingStore`] lock is only needed for the later sync
/// search call, see [`search_and_format`]).
///
/// Returns `None` for empty input or any provider error — both are
/// treated as "no context available", never propagated up.
pub async fn embed_query(
    query: &str,
    embed_provider: &dyn AiProvider,
    embed_model: &str,
) -> Option<Vec<f32>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let resp = embed_provider
        .embed(EmbedRequest {
            model: embed_model.to_string(),
            inputs: vec![trimmed.to_string()],
        })
        .await
        .ok()?;
    let v = resp.vectors.into_iter().next()?;
    if v.is_empty() { None } else { Some(v) }
}

/// Run the top-K search against an open store and format the result
/// into a [`RagContext`]. Synchronous — meant to be called inside a
/// short critical section while holding the store mutex.
///
/// Returns `None` when the search has zero hits under `embed_model`.
pub fn search_and_format(
    query_vec: &[f32],
    embed_model: &str,
    store: &EmbeddingStore,
    top_k: usize,
) -> Option<RagContext> {
    let hits = store.search(query_vec, embed_model, top_k).ok()?;
    if hits.is_empty() {
        return None;
    }
    let (body, citations) = format_context(&hits);
    if body.is_empty() {
        return None;
    }
    Some(RagContext {
        system_turn: ChatTurn::text(ChatRole::System, body),
        citations,
    })
}

/// Turn a list of search hits into the system-prompt body + the
/// preview-sized citation structs. Hits are already in descending
/// score order; we number them so the model can reference `[1]..[k]`
/// inline. Truncation is per-chunk soft-cap then combined hard cap.
fn format_context(hits: &[SearchHit]) -> (String, Vec<RagCitation>) {
    let mut body = String::new();
    body.push_str(
        "以下是从用户笔记库中检索到的相关片段，按相似度降序排列。\
回答时可直接引用，必要时用 `[编号]` 标注出处；若片段与问题无关请忽略。\n\n",
    );
    let mut citations = Vec::with_capacity(hits.len());
    let mut total = body.len();

    for (i, hit) in hits.iter().enumerate() {
        let preview = truncate_chars(&hit.text, 160);
        citations.push(RagCitation {
            note_rel_path: hit.note_rel_path.clone(),
            chunk_index: hit.chunk_index,
            offset_start: hit.offset_start,
            offset_end: hit.offset_end,
            score: hit.score,
            preview,
        });

        let header = format!("[{}] 来源：{}\n", i + 1, hit.note_rel_path);
        let chunk_body = truncate_chars(&hit.text, 1_500);
        let chunk_block = format!("{header}{chunk_body}\n\n");

        if total.saturating_add(chunk_block.len()) > MAX_CONTEXT_CHARS {
            // Try to fit a shortened version; if even a header+160 chars
            // won't fit, stop accumulating. We still keep the `citation`
            // entry so the UI can tell the user "retrieved but omitted
            // due to budget" if it wants to.
            let remaining = MAX_CONTEXT_CHARS.saturating_sub(total);
            if remaining > header.len() + 40 {
                let shortened = truncate_chars(&hit.text, remaining - header.len() - 4);
                let _ = write(&mut body, &header);
                let _ = write(&mut body, &shortened);
                body.push_str("\n\n");
            }
            break;
        }

        body.push_str(&chunk_block);
        total = body.len();
    }

    (body, citations)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

fn write(buf: &mut String, piece: &str) -> std::fmt::Result {
    use std::fmt::Write;
    write!(buf, "{}", piece)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str, idx: u32, text: &str, score: f32) -> SearchHit {
        SearchHit {
            note_rel_path: path.into(),
            chunk_index: idx,
            offset_start: 0,
            offset_end: text.len() as u32,
            text: text.into(),
            score,
        }
    }

    #[test]
    fn format_includes_all_hits_under_budget() {
        let hits = vec![
            hit("a.md", 0, "Chunk A contents.", 0.9),
            hit("b.md", 0, "Chunk B contents.", 0.8),
        ];
        let (body, cites) = format_context(&hits);
        assert!(body.contains("[1]"));
        assert!(body.contains("[2]"));
        assert!(body.contains("Chunk A"));
        assert!(body.contains("Chunk B"));
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].note_rel_path, "a.md");
    }

    #[test]
    fn format_truncates_previews_to_160_chars() {
        let long = "x".repeat(500);
        let hits = vec![hit("a.md", 0, &long, 0.9)];
        let (_body, cites) = format_context(&hits);
        // 160 + '…'
        assert!(cites[0].preview.chars().count() <= 161);
    }

    #[test]
    fn format_respects_overall_budget() {
        // 10 chunks × 1500 chars = 15000 > 6000 cap; should stop early.
        let big = "x".repeat(1500);
        let hits: Vec<SearchHit> = (0..10)
            .map(|i| hit(&format!("n{i}.md"), 0, &big, 0.9 - i as f32 * 0.05))
            .collect();
        let (body, _cites) = format_context(&hits);
        assert!(body.len() <= MAX_CONTEXT_CHARS + 200, "body = {}", body.len());
        // At least one chunk must have landed.
        assert!(body.contains("n0.md"));
    }

    #[test]
    fn truncate_chars_handles_multibyte() {
        // Each CJK char is 3 bytes in UTF-8; ensure we count chars, not bytes.
        let s = "一二三四五六七八九十";
        let t = truncate_chars(s, 3);
        assert_eq!(t.chars().filter(|c| *c != '…').count(), 3);
    }
}
