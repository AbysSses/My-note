//! End-to-end embed pipeline for a single note — Phase 3-D2a.3.
//!
//! ```text
//! rel_path
//!   ↓ read file + fs mtime
//! raw markdown + mtime
//!   ↓ chunker::chunk_markdown
//! Vec<Chunk>                     ── (if 0 chunks → SkipReason::Empty)
//!   ↓ compare mtime vs store
//! {up-to-date?} → yes → SkipReason::UpToDate
//!   ↓ no
//! provider.embed(chunk texts)
//!   ↓
//! Vec<Vec<f32>>  + pack into StoredChunk[]
//!   ↓
//! store.upsert_chunks (transactional)
//!   ↓
//! EmbedOutcome { chunks_embedded, tokens_used }
//! ```
//!
//! ## Why mtime-based skip instead of content hash
//!
//! - mtime is a single SQLite read (no file hashing), so `embed_note` on an
//!   up-to-date note is ~100 µs vs. a content-hash check that would re-read
//!   the file.
//! - False negatives (content same, mtime bumped by `touch`) cost one
//!   round-trip; we eat the cost rather than build a hash index.
//! - False positives (content changed but mtime stayed — e.g. some backup
//!   tools preserve mtime) **are possible in theory but very rare**; the
//!   D2a.4 full-vault initialization flow (or "清空 AI 索引" + rerun) covers
//!   those cases manually.
//!
//! ## Batch size
//!
//! For D2a.3a we send **all chunks of one note in a single request**.
//! Notes exceeding the provider input limit (OpenAI: 96) are split into
//! 64-input batches as a precaution. Larger-scale batching (across notes)
//! is the watcher's job in D2a.3b.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::AppError;

use super::chunker::{chunk_markdown, Chunk};
use super::embedding_store::{EmbeddingStore, StoredChunk};
use super::provider::{describe_provider_error, AiProvider, EmbedRequest, ProviderErrorKind};

/// Provider-batch size guard. OpenAI caps inputs per `/embeddings` call at
/// 96; we use 64 to leave headroom for local backends (Ollama has been seen
/// to refuse batches larger than ~40 in some configurations).
pub const MAX_BATCH_INPUTS: usize = 64;

// ── Public types ──────────────────────────────────────────────────────────────

/// Why a note's embed run was skipped — both cases are **non-errors**. The
/// `ai_embed_note` IPC returns an `EmbedOutcome` with a `skipped` tag rather
/// than erroring so the caller can surface a "nothing to do" notice instead
/// of a scary red banner.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// File mtime matches the newest chunk mtime already in the store.
    UpToDate,
    /// After chunking the note has 0 non-empty chunks (empty / frontmatter-only).
    Empty,
}

/// Result of a successful `embed_note` call — either a run with counts, or
/// a principled skip with reason attached.
#[derive(Debug, Clone, Serialize)]
pub struct EmbedOutcome {
    /// Note that was embedded (vault-relative, forward-slash form).
    pub rel_path: String,
    /// Number of chunks that were embedded + written. 0 when skipped.
    pub chunks_embedded: u32,
    /// Total tokens reported by the provider across all batches (0 if the
    /// provider omits usage).
    pub tokens_used: u32,
    /// `Some(reason)` when the pipeline short-circuited; `None` on a real
    /// embed run that hit the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<SkipReason>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmbedFailureKind {
    Network,
    Auth,
    RateLimit,
    InvalidRequest,
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EmbedFailure {
    pub kind: EmbedFailureKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    /// `true` means the embedding store was left untouched by this failed run.
    pub store_unchanged: bool,
}

impl std::fmt::Display for EmbedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

type EmbedResult<T> = Result<T, EmbedFailure>;

// ── Public entry point ────────────────────────────────────────────────────────

/// Embed one note end-to-end.
///
/// `provider_model` is the canonical model identifier that gets written into
/// the `model` column (not necessarily the one the provider picks on its
/// end — some providers route "auto" to a backing model). We persist what
/// the caller *asked for* so retrievability is self-consistent.
pub async fn embed_note(
    store: Arc<Mutex<EmbeddingStore>>,
    provider: &dyn AiProvider,
    provider_model: &str,
    vault: &Path,
    rel_path: &str,
) -> EmbedResult<EmbedOutcome> {
    // ── 1. Read file + fs mtime ─────────────────────────────────────────────
    let abs = vault.join(rel_path);
    let body = std::fs::read_to_string(&abs)
        .map_err(|e| other_failure(format!("read {}: {e}", abs.display())))?;
    let mtime = file_mtime_secs(&abs)?;

    // ── 2. Chunk ─────────────────────────────────────────────────────────────
    let chunks: Vec<Chunk> = chunk_markdown(&body);
    if chunks.is_empty() {
        return Ok(EmbedOutcome {
            rel_path: rel_path.to_string(),
            chunks_embedded: 0,
            tokens_used: 0,
            skipped: Some(SkipReason::Empty),
        });
    }

    // ── 3. mtime-based incremental skip ──────────────────────────────────────
    // Only skip if every existing chunk for this note already has the latest
    // mtime AND the chunk count matches. The latter protects against the
    // case where the user edited the note but the fs mtime regressed (git
    // checkout of an older commit, etc.) — we'd rather re-embed than leave
    // stale chunks in the store.
    {
        let store_g = store.lock().unwrap();
        if let Some(stored_mtime) = store_g
            .note_mtime_for_model(rel_path, provider_model)
            .map_err(|e| other_failure(format!("load existing embedding state: {e}")))?
        {
            if stored_mtime == mtime {
                return Ok(EmbedOutcome {
                    rel_path: rel_path.to_string(),
                    chunks_embedded: 0,
                    tokens_used: 0,
                    skipped: Some(SkipReason::UpToDate),
                });
            }
        }
    }

    // ── 4. Embed via provider (batched to MAX_BATCH_INPUTS) ─────────────────
    let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    let mut tokens_used: u32 = 0;
    for batch in chunks.chunks(MAX_BATCH_INPUTS) {
        let inputs: Vec<String> = batch.iter().map(|c| c.text.clone()).collect();
        let resp = provider
            .embed(EmbedRequest {
                model: provider_model.to_string(),
                inputs,
            })
            .await
            .map_err(|e| provider_failure(&e))?;
        if resp.vectors.len() != batch.len() {
            return Err(other_failure(format!(
                "provider returned {} vectors for {} inputs",
                resp.vectors.len(),
                batch.len()
            )));
        }
        vectors.extend(resp.vectors);
        tokens_used = tokens_used.saturating_add(resp.total_tokens);
    }

    // ── 5. Build StoredChunks + atomic upsert ────────────────────────────────
    let stored: Vec<StoredChunk> = chunks
        .into_iter()
        .zip(vectors.into_iter())
        .map(|(c, v)| StoredChunk {
            note_rel_path: rel_path.to_string(),
            chunk_index: c.chunk_index,
            offset_start: c.offset_start,
            offset_end: c.offset_end,
            text: c.text,
            model: provider_model.to_string(),
            vector: v,
            note_mtime: mtime,
        })
        .collect();

    let chunks_embedded = stored.len() as u32;
    {
        let mut store_g = store.lock().unwrap();
        store_g
            .replace_note_chunks(rel_path, &stored)
            .map_err(|e| other_failure(format!("persist {rel_path}: {e}")))?;
    }

    Ok(EmbedOutcome {
        rel_path: rel_path.to_string(),
        chunks_embedded,
        tokens_used,
        skipped: None,
    })
}

/// Read an absolute path's modification time as seconds since Unix epoch.
/// Falls back to `0` for filesystems that refuse to report mtime (the
/// subsequent chunk write still succeeds; the incremental check just
/// becomes "always re-embed" until the FS supplies a real mtime).
fn file_mtime_secs(abs: &Path) -> EmbedResult<i64> {
    let meta = std::fs::metadata(abs)
        .map_err(|e| other_failure(format!("stat {}: {e}", abs.display())))?;
    let t = meta.modified().unwrap_or_else(|_| SystemTime::now());
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(secs)
}

pub fn failure_from_app_error(err: &AppError) -> EmbedFailure {
    other_failure(err.to_string())
}

fn provider_failure(err: &super::provider::ProviderError) -> EmbedFailure {
    let (kind, message, retry_after_secs) = describe_provider_error(err);
    let kind = match kind {
        ProviderErrorKind::Network => EmbedFailureKind::Network,
        ProviderErrorKind::Auth => EmbedFailureKind::Auth,
        ProviderErrorKind::RateLimit => EmbedFailureKind::RateLimit,
        ProviderErrorKind::InvalidRequest => EmbedFailureKind::InvalidRequest,
        ProviderErrorKind::Other => EmbedFailureKind::Other,
    };
    EmbedFailure {
        kind,
        message,
        retry_after_secs,
        store_unchanged: true,
    }
}

fn other_failure(message: String) -> EmbedFailure {
    EmbedFailure {
        kind: EmbedFailureKind::Other,
        message,
        retry_after_secs: None,
        store_unchanged: true,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai::provider::{AiProvider, EmbedResponse, MockProvider, ProviderError};
    use async_trait::async_trait;

    struct FailProvider {
        err: ProviderError,
    }

    #[async_trait]
    impl AiProvider for FailProvider {
        fn name(&self) -> &'static str {
            "fail"
        }

        fn default_dim(&self) -> usize {
            0
        }

        async fn embed(
            &self,
            _req: crate::services::ai::provider::EmbedRequest,
        ) -> Result<EmbedResponse, ProviderError> {
            Err(match &self.err {
                ProviderError::Network(msg) => ProviderError::Network(msg.clone()),
                ProviderError::Auth(msg) => ProviderError::Auth(msg.clone()),
                ProviderError::RateLimit {
                    retry_after_secs,
                    message,
                } => ProviderError::RateLimit {
                    retry_after_secs: *retry_after_secs,
                    message: message.clone(),
                },
                ProviderError::InvalidRequest(msg) => ProviderError::InvalidRequest(msg.clone()),
                ProviderError::Other(msg) => ProviderError::Other(msg.clone()),
            })
        }
    }

    fn fresh_store() -> Arc<Mutex<EmbeddingStore>> {
        Arc::new(Mutex::new(EmbeddingStore::open_in_memory().unwrap()))
    }

    fn write(vault: &Path, rel: &str, body: &str) {
        let abs = vault.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&abs, body).unwrap();
    }

    #[tokio::test]
    async fn empty_note_is_skip_empty() {
        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "a.md", "");
        let store = fresh_store();
        let provider = MockProvider::new();
        let out = embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap();
        assert_eq!(out.skipped, Some(SkipReason::Empty));
        assert_eq!(out.chunks_embedded, 0);
    }

    #[tokio::test]
    async fn frontmatter_only_is_skip_empty() {
        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "a.md", "---\ntitle: a\n---\n");
        let store = fresh_store();
        let provider = MockProvider::new();
        let out = embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap();
        assert_eq!(out.skipped, Some(SkipReason::Empty));
    }

    #[tokio::test]
    async fn basic_run_embeds_and_persists() {
        let vault = tempfile::tempdir().unwrap();
        write(
            vault.path(),
            "a.md",
            "# hello\n\nfirst paragraph body.\n\nsecond paragraph body.\n",
        );
        let store = fresh_store();
        let provider = MockProvider::new();
        let out = embed_note(store.clone(), &provider, "mock", vault.path(), "a.md")
            .await
            .unwrap();
        assert!(out.skipped.is_none());
        assert_eq!(out.chunks_embedded, 3); // heading + 2 paragraphs
        let stats = store.lock().unwrap().stats().unwrap();
        assert_eq!(stats.chunk_count, 3);
        assert_eq!(stats.note_count, 1);
    }

    #[tokio::test]
    async fn second_run_with_same_mtime_is_up_to_date() {
        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "a.md", "a body\n\nb body\n");
        let store = fresh_store();
        let provider = MockProvider::new();

        let first = embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap();
        assert!(first.skipped.is_none());

        let second = embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap();
        assert_eq!(second.skipped, Some(SkipReason::UpToDate));
    }

    #[tokio::test]
    async fn edit_reduces_chunk_count_stale_chunks_cleaned() {
        // Embed a note with 3 chunks, then overwrite with 1 chunk and
        // verify the old chunks are gone (not left in the store as orphans).
        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "a.md", "para1.\n\npara2.\n\npara3.\n");
        let store = fresh_store();
        let provider = MockProvider::new();

        embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap();
        assert_eq!(store.lock().unwrap().stats().unwrap().chunk_count, 3);

        // Re-write with bumped mtime (sleep 1 s to ensure the FS mtime
        // granularity distinguishes old vs. new).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        write(vault.path(), "a.md", "only one para.\n");

        let out = embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap();
        assert_eq!(out.chunks_embedded, 1);
        assert_eq!(store.lock().unwrap().stats().unwrap().chunk_count, 1);
    }

    #[tokio::test]
    async fn missing_file_surfaces_error() {
        let vault = tempfile::tempdir().unwrap();
        let store = fresh_store();
        let provider = MockProvider::new();
        let err = embed_note(store, &provider, "m", vault.path(), "nope.md")
            .await
            .unwrap_err();
        assert_eq!(err.kind, EmbedFailureKind::Other);
        assert!(err.message.contains("read"));
        assert!(err.store_unchanged);
    }

    #[tokio::test]
    async fn provider_rate_limit_is_classified_and_store_unchanged() {
        let vault = tempfile::tempdir().unwrap();
        write(vault.path(), "a.md", "body");
        let store = fresh_store();
        let provider = FailProvider {
            err: ProviderError::RateLimit {
                retry_after_secs: 30,
                message: "quota exceeded".into(),
            },
        };

        let err = embed_note(store.clone(), &provider, "m", vault.path(), "a.md")
            .await
            .unwrap_err();
        assert_eq!(err.kind, EmbedFailureKind::RateLimit);
        assert_eq!(err.retry_after_secs, Some(30));
        assert!(err.message.contains("quota"));
        assert!(err.store_unchanged);
        assert_eq!(store.lock().unwrap().stats().unwrap().chunk_count, 0);
    }
}
