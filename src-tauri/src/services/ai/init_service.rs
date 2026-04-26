//! Full-vault embedding preview + execution helpers — Phase 3-D2a.4.
//!
//! The Settings "初始化索引" flow needs two backend capabilities:
//!
//! 1. A **dry-run preview** that walks every markdown note, estimates how many
//!    chunks / tokens are pending for the *current* model, and optionally
//!    derives a rough dollar estimate.
//! 2. A **confirmed execution** path that iterates the whole vault and reuses
//!    `embed_service::embed_note` per note, collecting a summary instead of
//!    failing the entire run on the first broken file.
//!
//! This module keeps those concerns out of `commands/ai.rs` so both the IPC
//! layer and future background jobs can share the same traversal logic.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use url::Url;

use crate::error::AppResult;
use crate::services::config::AiProviderConfig;
use crate::services::scanner;

use super::chunker::chunk_markdown;
use super::embed_service::{self, EmbedFailure, EmbedFailureKind, SkipReason};
use super::embedding_store::EmbeddingStore;
use super::provider::AiProvider;

const PREVIEW_LIMIT: usize = 100;
const FAILURE_PREVIEW_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostEstimateKind {
    Local,
    OpenAiPublicPricing,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultEmbedPreview {
    pub note_count_total: u64,
    pub note_count_to_embed: u64,
    pub note_count_up_to_date: u64,
    pub note_count_empty: u64,
    pub chunk_count_to_embed: u64,
    pub token_count_estimated: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cost_estimate_kind: CostEstimateKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd_estimate: Option<f64>,
    pub notes_preview: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultEmbedRunResult {
    pub note_count_total: u64,
    pub note_count_embedded: u64,
    pub note_count_up_to_date: u64,
    pub note_count_empty: u64,
    pub note_count_failed: u64,
    pub note_count_not_attempted: u64,
    pub chunk_count_embedded: u64,
    pub token_count_used: u64,
    pub aborted_early: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aborted_error_kind: Option<EmbedFailureKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aborted_error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aborted_retry_after_secs: Option<u64>,
    pub failure_preview: Vec<String>,
}

pub fn preview_vault_embed(
    vault: &Path,
    store: &EmbeddingStore,
    provider_cfg: Option<&AiProviderConfig>,
) -> AppResult<VaultEmbedPreview> {
    let model = provider_cfg
        .map(|cfg| cfg.embed_model.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut files = scanner::walk_vault_md(vault)?;
    files.sort_by(|a, b| a.1.cmp(&b.1));

    let mut preview = VaultEmbedPreview {
        note_count_total: files.len() as u64,
        note_count_to_embed: 0,
        note_count_up_to_date: 0,
        note_count_empty: 0,
        chunk_count_to_embed: 0,
        token_count_estimated: 0,
        model,
        cost_estimate_kind: CostEstimateKind::Unknown,
        cost_usd_estimate: None,
        notes_preview: Vec::new(),
    };

    for (abs, rel) in files {
        let body = match std::fs::read_to_string(&abs) {
            Ok(body) => body,
            Err(err) => {
                tracing::warn!(path = %abs.display(), error = %err, "vault embed preview: read failed");
                continue;
            }
        };
        let chunks = chunk_markdown(&body);
        if chunks.is_empty() {
            preview.note_count_empty += 1;
            continue;
        }

        let is_up_to_date = match preview.model.as_deref() {
            Some(model) => {
                let mtime = match file_mtime_secs(&abs) {
                    Ok(mtime) => mtime,
                    Err(err) => {
                        tracing::warn!(path = %abs.display(), error = %err, "vault embed preview: stat failed");
                        continue;
                    }
                };
                matches!(store.note_mtime_for_model(&rel, model)?, Some(stored) if stored == mtime)
            }
            None => false,
        };

        if is_up_to_date {
            preview.note_count_up_to_date += 1;
            continue;
        }

        preview.note_count_to_embed += 1;
        preview.chunk_count_to_embed += chunks.len() as u64;
        preview.token_count_estimated += chunks
            .iter()
            .map(|chunk| chunk.est_tokens as u64)
            .sum::<u64>();
        if preview.notes_preview.len() < PREVIEW_LIMIT {
            preview.notes_preview.push(rel);
        }
    }

    let (cost_estimate_kind, cost_usd_estimate) =
        estimate_cost(provider_cfg, preview.token_count_estimated);
    preview.cost_estimate_kind = cost_estimate_kind;
    preview.cost_usd_estimate = cost_usd_estimate;
    Ok(preview)
}

pub async fn embed_vault(
    store: Arc<Mutex<EmbeddingStore>>,
    provider: &dyn AiProvider,
    provider_model: &str,
    vault: &Path,
) -> AppResult<VaultEmbedRunResult> {
    let mut files = scanner::walk_vault_md(vault)?;
    files.sort_by(|a, b| a.1.cmp(&b.1));

    let mut out = VaultEmbedRunResult {
        note_count_total: files.len() as u64,
        note_count_embedded: 0,
        note_count_up_to_date: 0,
        note_count_empty: 0,
        note_count_failed: 0,
        note_count_not_attempted: 0,
        chunk_count_embedded: 0,
        token_count_used: 0,
        aborted_early: false,
        aborted_error_kind: None,
        aborted_error_message: None,
        aborted_retry_after_secs: None,
        failure_preview: Vec::new(),
    };

    for (idx, (_, rel)) in files.into_iter().enumerate() {
        match embed_service::embed_note(store.clone(), provider, provider_model, vault, &rel).await
        {
            Ok(result) => match result.skipped {
                Some(SkipReason::UpToDate) => out.note_count_up_to_date += 1,
                Some(SkipReason::Empty) => out.note_count_empty += 1,
                None => {
                    out.note_count_embedded += 1;
                    out.chunk_count_embedded += result.chunks_embedded as u64;
                    out.token_count_used += result.tokens_used as u64;
                }
            },
            Err(err) => {
                out.note_count_failed += 1;
                if out.failure_preview.len() < FAILURE_PREVIEW_LIMIT {
                    out.failure_preview
                        .push(format!("{rel} — {}", format_failure(&err)));
                }
                if should_abort_after_failure(&err) {
                    out.aborted_early = true;
                    out.aborted_error_kind = Some(err.kind);
                    out.aborted_error_message = Some(err.message.clone());
                    out.aborted_retry_after_secs = err.retry_after_secs;
                    out.note_count_not_attempted =
                        (out.note_count_total as usize).saturating_sub(idx + 1) as u64;
                    break;
                }
            }
        }
    }

    Ok(out)
}

fn should_abort_after_failure(failure: &EmbedFailure) -> bool {
    matches!(
        failure.kind,
        EmbedFailureKind::Network
            | EmbedFailureKind::Auth
            | EmbedFailureKind::RateLimit
            | EmbedFailureKind::InvalidRequest
    )
}

fn format_failure(failure: &EmbedFailure) -> String {
    match failure.retry_after_secs {
        Some(secs) => format!("{}（建议 {secs}s 后重试）", failure.message),
        None => failure.message.clone(),
    }
}

fn estimate_cost(
    provider_cfg: Option<&AiProviderConfig>,
    token_count: u64,
) -> (CostEstimateKind, Option<f64>) {
    let Some(cfg) = provider_cfg else {
        return (CostEstimateKind::Unknown, None);
    };

    if is_local_base_url(&cfg.base_url) {
        return (CostEstimateKind::Local, Some(0.0));
    }

    if !is_official_openai_base_url(&cfg.base_url) {
        return (CostEstimateKind::Unknown, None);
    }

    let rate_per_million = match cfg.embed_model.trim() {
        // OpenAI official embeddings pricing. Source:
        // https://platform.openai.com/docs/pricing/
        "text-embedding-3-small" => 0.02_f64,
        "text-embedding-3-large" => 0.13_f64,
        "text-embedding-ada-002" => 0.10_f64,
        _ => return (CostEstimateKind::Unknown, None),
    };
    let usd = (token_count as f64 / 1_000_000_f64) * rate_per_million;
    (CostEstimateKind::OpenAiPublicPricing, Some(usd))
}

fn is_local_base_url(base_url: &str) -> bool {
    match Url::parse(base_url) {
        Ok(url) => matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1")
        ),
        Err(_) => base_url.contains("localhost") || base_url.contains("127.0.0.1"),
    }
}

fn is_official_openai_base_url(base_url: &str) -> bool {
    match Url::parse(base_url) {
        Ok(url) => matches!(
            url.host_str(),
            Some("api.openai.com") | Some("platform.openai.com")
        ),
        Err(_) => false,
    }
}

fn file_mtime_secs(abs: &Path) -> AppResult<i64> {
    let meta = std::fs::metadata(abs)?;
    let t = meta.modified().unwrap_or_else(|_| SystemTime::now());
    Ok(t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai::embedding_store::{EmbeddingStats, StoredChunk};
    use crate::services::ai::provider::{
        AiProvider, EmbedRequest, EmbedResponse, MockProvider, ProviderError,
    };
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

        async fn embed(&self, _req: EmbedRequest) -> Result<EmbedResponse, ProviderError> {
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

    fn write_note(root: &Path, rel: &str, body: &str) {
        let abs = root.join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(abs, body).unwrap();
    }

    fn fake_cfg(base_url: &str, model: &str) -> AiProviderConfig {
        AiProviderConfig {
            kind: "openai".into(),
            base_url: base_url.into(),
            embed_model: model.into(),
            chat_model: String::new(),
        }
    }

    fn seed_embedded_note(store: &mut EmbeddingStore, root: &Path, rel: &str, model: &str) {
        let abs = root.join(rel);
        let mtime = file_mtime_secs(&abs).unwrap();
        let chunk = StoredChunk {
            note_rel_path: rel.into(),
            chunk_index: 0,
            offset_start: 0,
            offset_end: 4,
            text: "seed".into(),
            model: model.into(),
            vector: vec![1.0, 0.0],
            note_mtime: mtime,
        };
        store.upsert_chunks(&[chunk]).unwrap();
    }

    #[test]
    fn preview_counts_pending_up_to_date_and_empty() {
        let vault = tempfile::tempdir().unwrap();
        write_note(vault.path(), "1-notes/a.md", "hello\n\nworld");
        write_note(vault.path(), "1-notes/b.md", "body");
        write_note(vault.path(), "1-notes/empty.md", "---\ntitle: x\n---\n");

        let mut store = EmbeddingStore::open_in_memory().unwrap();
        seed_embedded_note(
            &mut store,
            vault.path(),
            "1-notes/a.md",
            "text-embedding-3-small",
        );

        let preview = preview_vault_embed(
            vault.path(),
            &store,
            Some(&fake_cfg(
                "https://api.openai.com/v1",
                "text-embedding-3-small",
            )),
        )
        .unwrap();

        assert_eq!(preview.note_count_total, 3);
        assert_eq!(preview.note_count_up_to_date, 1);
        assert_eq!(preview.note_count_to_embed, 1);
        assert_eq!(preview.note_count_empty, 1);
        assert_eq!(preview.notes_preview, vec!["1-notes/b.md"]);
        assert!(preview.chunk_count_to_embed >= 1);
        assert!(preview.token_count_estimated >= 1);
    }

    #[test]
    fn preview_is_model_scoped_not_any_model_scoped() {
        let vault = tempfile::tempdir().unwrap();
        write_note(vault.path(), "1-notes/a.md", "hello");

        let mut store = EmbeddingStore::open_in_memory().unwrap();
        seed_embedded_note(&mut store, vault.path(), "1-notes/a.md", "old-model");

        let preview = preview_vault_embed(
            vault.path(),
            &store,
            Some(&fake_cfg("https://api.openai.com/v1", "new-model")),
        )
        .unwrap();

        assert_eq!(preview.note_count_to_embed, 1);
        assert_eq!(preview.note_count_up_to_date, 0);
    }

    #[test]
    fn preview_list_truncates_and_sorts() {
        let vault = tempfile::tempdir().unwrap();
        for i in (0..105).rev() {
            write_note(vault.path(), &format!("1-notes/{i:03}.md"), "body");
        }
        let store = EmbeddingStore::open_in_memory().unwrap();

        let preview = preview_vault_embed(
            vault.path(),
            &store,
            Some(&fake_cfg("http://localhost:11434/v1", "nomic-embed-text")),
        )
        .unwrap();

        assert_eq!(preview.note_count_total, 105);
        assert_eq!(preview.note_count_to_embed, 105);
        assert_eq!(preview.notes_preview.len(), PREVIEW_LIMIT);
        assert_eq!(preview.notes_preview.first().unwrap(), "1-notes/000.md");
        assert_eq!(preview.notes_preview.last().unwrap(), "1-notes/099.md");
    }

    #[test]
    fn local_provider_cost_estimate_is_zero() {
        let (kind, usd) = estimate_cost(
            Some(&fake_cfg("http://localhost:11434/v1", "nomic-embed-text")),
            12_345,
        );
        assert_eq!(kind, CostEstimateKind::Local);
        assert_eq!(usd, Some(0.0));
    }

    #[test]
    fn openai_public_pricing_is_applied_for_known_models() {
        let (kind, usd) = estimate_cost(
            Some(&fake_cfg(
                "https://api.openai.com/v1",
                "text-embedding-3-small",
            )),
            1_000_000,
        );
        assert_eq!(kind, CostEstimateKind::OpenAiPublicPricing);
        assert_eq!(usd, Some(0.02));
    }

    #[tokio::test]
    async fn embed_vault_summarizes_success_skip_and_failure() {
        let vault = tempfile::tempdir().unwrap();
        write_note(vault.path(), "1-notes/a.md", "hello");
        write_note(vault.path(), "1-notes/b.md", "---\ntitle: x\n---\n");

        let store = Arc::new(Mutex::new(EmbeddingStore::open_in_memory().unwrap()));
        let provider = MockProvider::with_dim(8);

        let first = embed_vault(store.clone(), &provider, "mock", vault.path())
            .await
            .unwrap();
        assert_eq!(first.note_count_total, 2);
        assert_eq!(first.note_count_embedded, 1);
        assert_eq!(first.note_count_empty, 1);
        assert_eq!(first.note_count_failed, 0);
        assert!(!first.aborted_early);
        assert_eq!(first.note_count_not_attempted, 0);

        let second = embed_vault(store.clone(), &provider, "mock", vault.path())
            .await
            .unwrap();
        assert_eq!(second.note_count_up_to_date, 1);
        assert_eq!(second.note_count_empty, 1);
        assert_eq!(second.note_count_embedded, 0);
        assert!(!second.aborted_early);

        let stats: EmbeddingStats = store.lock().unwrap().stats().unwrap();
        assert_eq!(stats.note_count, 1);
    }

    #[tokio::test]
    async fn embed_vault_aborts_early_on_provider_failures() {
        let vault = tempfile::tempdir().unwrap();
        write_note(vault.path(), "1-notes/a.md", "hello");
        write_note(vault.path(), "1-notes/b.md", "world");

        let store = Arc::new(Mutex::new(EmbeddingStore::open_in_memory().unwrap()));
        let provider = FailProvider {
            err: ProviderError::Auth("bad key".into()),
        };

        let out = embed_vault(store, &provider, "mock", vault.path())
            .await
            .unwrap();
        assert_eq!(out.note_count_failed, 1);
        assert!(out.aborted_early);
        assert_eq!(out.aborted_error_kind, Some(EmbedFailureKind::Auth));
        assert!(out
            .aborted_error_message
            .as_deref()
            .unwrap_or("")
            .contains("bad key"));
        assert_eq!(out.note_count_not_attempted, 1);
    }
}
