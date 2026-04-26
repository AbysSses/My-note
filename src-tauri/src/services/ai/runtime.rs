//! Shared runtime helpers for AI commands + watcher integration.
//!
//! The command layer (`commands/ai.rs`) and the background watcher both need
//! the same two decisions:
//!
//! 1. Is auto-embed currently enabled?
//! 2. Given the persisted config + keychain, how do we build a live provider?
//!
//! Keep those answers in one place so D2a.3b doesn't fork provider bootstrap
//! logic from the interactive IPC path.

use crate::error::{AppError, AppResult};
use crate::services::config::{AiProviderConfig, ConfigStore};

use super::openai::OpenAiProvider;
use super::secrets::SecretStore;

fn provider_config_complete(cfg: &AiProviderConfig) -> bool {
    !cfg.kind.trim().is_empty()
        && !cfg.base_url.trim().is_empty()
        && !cfg.embed_model.trim().is_empty()
}

/// Whether watcher-driven auto-embed should run right now.
///
/// `ai_enabled: None` inherits the frontend default (`true`). A provider with
/// an empty base URL / model is treated as "not ready" even if a struct is
/// present in config.
pub fn auto_embed_enabled(cfg: &ConfigStore) -> bool {
    let snap = cfg.snapshot();
    if snap.ai_enabled == Some(false) {
        return false;
    }
    snap.ai_provider
        .as_ref()
        .is_some_and(provider_config_complete)
}

/// Build a live provider from a persisted config payload + secret store.
///
/// API keys are optional so local OpenAI-compatible backends such as Ollama
/// can run anonymously.
pub fn build_provider_from_config(
    cfg: &AiProviderConfig,
    secrets: &dyn SecretStore,
) -> AppResult<(OpenAiProvider, String)> {
    if cfg.kind.trim().is_empty() {
        return Err(AppError::Other("AI provider kind is empty".into()));
    }
    if cfg.base_url.trim().is_empty() {
        return Err(AppError::Other("AI provider base_url is empty".into()));
    }
    if cfg.embed_model.trim().is_empty() {
        return Err(AppError::Other("AI provider embed_model is empty".into()));
    }

    let api_key = secrets.get_api_key(&cfg.kind)?.unwrap_or_default();
    let provider = OpenAiProvider::new(&cfg.base_url, &cfg.embed_model, api_key);
    Ok((provider, cfg.embed_model.clone()))
}

/// Build a provider from the persisted app config store.
pub fn build_configured_provider(
    cfg: &ConfigStore,
    secrets: &dyn SecretStore,
) -> AppResult<(OpenAiProvider, String)> {
    if cfg.snapshot().ai_enabled == Some(false) {
        return Err(AppError::Other("AI is disabled in Settings".into()));
    }
    let provider_cfg = cfg
        .snapshot()
        .ai_provider
        .ok_or_else(|| AppError::Other("no AI provider configured".into()))?;
    build_provider_from_config(&provider_cfg, secrets)
}

/// Build a chat-flavoured provider: same transport as the embedding
/// provider, but preloaded with `chat_model` instead of `embed_model`.
/// Split on `chat_model` specifically because D2b.2 onward needs chat
/// plumbing even when the user leaves embeddings disabled (and vice
/// versa).
///
/// Consumed from D2b.3 by the non-streaming `ai_chat_send` command and
/// in D2b.4 by the streaming `ai_chat_stream` equivalent.
pub fn build_configured_chat_provider(
    cfg: &ConfigStore,
    secrets: &dyn SecretStore,
) -> AppResult<(OpenAiProvider, String)> {
    if cfg.snapshot().ai_enabled == Some(false) {
        return Err(AppError::Other("AI is disabled in Settings".into()));
    }
    let provider_cfg = cfg
        .snapshot()
        .ai_provider
        .ok_or_else(|| AppError::Other("no AI provider configured".into()))?;
    build_chat_provider_from_config(&provider_cfg, secrets)
}

/// Chat-specific variant of [`build_provider_from_config`]. Validates
/// `chat_model` instead of `embed_model`.
pub fn build_chat_provider_from_config(
    cfg: &AiProviderConfig,
    secrets: &dyn SecretStore,
) -> AppResult<(OpenAiProvider, String)> {
    if cfg.kind.trim().is_empty() {
        return Err(AppError::Other("AI provider kind is empty".into()));
    }
    if cfg.base_url.trim().is_empty() {
        return Err(AppError::Other("AI provider base_url is empty".into()));
    }
    if cfg.chat_model.trim().is_empty() {
        return Err(AppError::Other("AI provider chat_model is empty".into()));
    }
    let api_key = secrets.get_api_key(&cfg.kind)?.unwrap_or_default();
    let provider = OpenAiProvider::new(&cfg.base_url, &cfg.chat_model, api_key);
    Ok((provider, cfg.chat_model.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai::provider::AiProvider;
    use crate::services::ai::secrets::{MockSecretStore, SecretStore};
    use tempfile::TempDir;

    fn configured_store(ai_enabled: Option<bool>) -> (TempDir, ConfigStore) {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = ConfigStore::load_or_init(tmp.path()).unwrap();
        cfg.prefs.ai_enabled = ai_enabled;
        cfg.prefs.ai_provider = Some(AiProviderConfig {
            kind: "openai".into(),
            base_url: "http://localhost:11434/v1".into(),
            embed_model: "nomic-embed-text".into(),
            chat_model: "gpt-4o-mini".into(),
        });
        (tmp, cfg)
    }

    #[test]
    fn auto_embed_enabled_defaults_true_when_flag_missing() {
        let (_tmp, cfg) = configured_store(None);
        assert!(auto_embed_enabled(&cfg));
    }

    #[test]
    fn auto_embed_enabled_respects_explicit_false() {
        let (_tmp, cfg) = configured_store(Some(false));
        assert!(!auto_embed_enabled(&cfg));
    }

    #[test]
    fn auto_embed_enabled_rejects_incomplete_provider() {
        let (_tmp, mut cfg) = configured_store(Some(true));
        cfg.prefs.ai_provider.as_mut().unwrap().embed_model.clear();
        assert!(!auto_embed_enabled(&cfg));
    }

    #[test]
    fn build_provider_reads_secret_store_but_allows_missing_key() {
        let (_tmp, cfg) = configured_store(Some(true));
        let secrets = MockSecretStore::new();
        let (provider, model) = build_configured_provider(&cfg, &secrets).unwrap();
        assert_eq!(provider.name(), "openai");
        assert_eq!(model, "nomic-embed-text");
    }

    #[test]
    fn build_provider_rejects_empty_kind() {
        let (_tmp, mut cfg) = configured_store(Some(true));
        cfg.prefs.ai_provider.as_mut().unwrap().kind.clear();
        let secrets = MockSecretStore::new();
        match build_configured_provider(&cfg, &secrets) {
            Err(AppError::Other(msg)) => assert!(msg.contains("kind")),
            Err(err) => panic!("expected AppError::Other, got {err:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn build_provider_reads_saved_key_when_present() {
        let (_tmp, cfg) = configured_store(Some(true));
        let secrets = MockSecretStore::new();
        secrets.set_api_key("openai", "sk-test").unwrap();
        let (provider, _) = build_configured_provider(&cfg, &secrets).unwrap();
        assert_eq!(provider.name(), "openai");
    }
}
