use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

const MAX_RECENT: usize = 10;
const CONFIG_FILENAME: &str = "app-config.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiToolPermissions {
    #[serde(default = "default_true")]
    pub allow_readonly: bool,
    #[serde(default = "default_true")]
    pub allow_writeback: bool,
    #[serde(default)]
    pub allow_destructive: bool,
}

impl Default for AiToolPermissions {
    fn default() -> Self {
        Self {
            allow_readonly: true,
            allow_writeback: true,
            allow_destructive: false,
        }
    }
}

const fn default_true() -> bool {
    true
}

/// Non-secret half of an AI provider config. **The API key is NEVER written
/// here** — it lives in the OS keystore (see `services::ai::secrets`).
/// What's persisted is only the shape needed to re-construct an
/// `OpenAiProvider` on next launch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiProviderConfig {
    /// Provider kind identifier (`"openai"` for now; future values may be
    /// `"anthropic"`, etc.). Also doubles as the keyring account name.
    #[serde(default)]
    pub kind: String,
    /// HTTP base URL (no trailing slash). Empty = use provider default.
    #[serde(default)]
    pub base_url: String,
    /// Embedding model id (e.g. `text-embedding-3-small`, `nomic-embed-text`).
    /// Empty = caller must choose.
    #[serde(default)]
    pub embed_model: String,
    /// Chat model id (e.g. `gpt-4o-mini`, `llama3.1`). Independent from
    /// `embed_model` because most deployments mix a small/cheap embedder
    /// with a larger chat model. Empty = chat disabled (embeddings may
    /// still work). Defaulted on deserialise so upgrading from a pre-D2b
    /// config file is forward-compatible.
    #[serde(default)]
    pub chat_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppPreferences {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub autosave_ms: Option<u32>,
    #[serde(default)]
    pub shortcuts: BTreeMap<String, String>,
    /// Whether the AI-assist panel (related-notes, future RAG) is shown.
    /// `None` means "user hasn't set this yet" → defaults to `true` at the
    /// frontend layer so the panel appears on first launch without requiring
    /// an explicit opt-in.
    #[serde(default)]
    pub ai_enabled: Option<bool>,
    /// AI provider configuration (D2a.2). `None` = no provider configured
    /// yet; IPC commands requiring a provider will return a typed error.
    #[serde(default)]
    pub ai_provider: Option<AiProviderConfig>,
    /// Permission matrix for agentic chat tools (D5.6).
    #[serde(default)]
    pub ai_tool_permissions: AiToolPermissions,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppConfigSnapshot {
    pub recent_vaults: Vec<String>,
    pub theme: Option<String>,
    pub autosave_ms: Option<u32>,
    pub shortcuts: BTreeMap<String, String>,
    /// `None` = not yet persisted, treat as `true` on the frontend.
    pub ai_enabled: Option<bool>,
    /// `None` = no provider configured yet; Settings UI shows "new provider"
    /// form. The `has_api_key` side of the state is served by a separate
    /// IPC (`ai_provider_has_api_key`) so `snapshot()` stays synchronous
    /// and never touches the OS keystore.
    pub ai_provider: Option<AiProviderConfig>,
    pub ai_tool_permissions: AiToolPermissions,
}

/// App-wide config stored in the OS app-config dir (not inside any vault).
/// Holds the recent-vault list and future global preferences (theme, hotkeys, etc.).
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ConfigStore {
    #[serde(default)]
    pub recent_vaults: Vec<String>,
    #[serde(default)]
    pub prefs: AppPreferences,
    #[serde(skip)]
    config_path: PathBuf,
}

impl ConfigStore {
    pub fn load_or_init(config_dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(config_dir)?;
        let path = config_dir.join(CONFIG_FILENAME);

        let mut store = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<Self>(&raw).unwrap_or_default()
        } else {
            Self::default()
        };
        store.config_path = path;
        Ok(store)
    }

    pub fn recent_vaults(&self) -> Vec<String> {
        self.recent_vaults.clone()
    }

    pub fn snapshot(&self) -> AppConfigSnapshot {
        AppConfigSnapshot {
            recent_vaults: self.recent_vaults.clone(),
            theme: self.prefs.theme.clone(),
            autosave_ms: self.prefs.autosave_ms,
            shortcuts: self.prefs.shortcuts.clone(),
            ai_enabled: self.prefs.ai_enabled,
            ai_provider: self.prefs.ai_provider.clone(),
            ai_tool_permissions: self.prefs.ai_tool_permissions.clone(),
        }
    }

    pub fn ai_provider_kind(&self) -> Option<String> {
        self.prefs.ai_provider.as_ref().map(|p| p.kind.clone())
    }

    pub fn record_recent_vault(&mut self, path: &Path) -> AppResult<()> {
        let s = path.to_string_lossy().to_string();
        self.recent_vaults.retain(|p| p != &s);
        self.recent_vaults.insert(0, s);
        if self.recent_vaults.len() > MAX_RECENT {
            self.recent_vaults.truncate(MAX_RECENT);
        }
        self.persist()
    }

    pub fn set_theme(&mut self, theme: String) -> AppResult<()> {
        self.prefs.theme = Some(theme);
        self.persist()
    }

    pub fn set_autosave_ms(&mut self, autosave_ms: u32) -> AppResult<()> {
        self.prefs.autosave_ms = Some(autosave_ms);
        self.persist()
    }

    pub fn set_shortcuts(&mut self, shortcuts: BTreeMap<String, String>) -> AppResult<()> {
        self.prefs.shortcuts = shortcuts;
        self.persist()
    }

    pub fn set_ai_enabled(&mut self, enabled: bool) -> AppResult<()> {
        self.prefs.ai_enabled = Some(enabled);
        self.persist()
    }

    /// Replace the AI provider config. `api_key` intentionally lives outside
    /// this struct — store it via `services::ai::secrets` separately.
    pub fn set_ai_provider(&mut self, provider: AiProviderConfig) -> AppResult<()> {
        self.prefs.ai_provider = Some(provider);
        self.persist()
    }

    /// Drop the AI provider config entirely. Callers should also wipe the
    /// keychain entry in the same code path — this fn doesn't touch secrets.
    pub fn clear_ai_provider(&mut self) -> AppResult<()> {
        self.prefs.ai_provider = None;
        self.persist()
    }

    pub fn set_ai_tool_permissions(&mut self, permissions: AiToolPermissions) -> AppResult<()> {
        self.prefs.ai_tool_permissions = permissions;
        self.persist()
    }

    fn persist(&self) -> AppResult<()> {
        let body = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.config_path, body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_tool_permissions_default_shape_is_expected() {
        let perms = AiToolPermissions::default();
        assert!(perms.allow_readonly);
        assert!(perms.allow_writeback);
        assert!(!perms.allow_destructive);
    }

    #[test]
    fn snapshot_includes_tool_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = ConfigStore::load_or_init(tmp.path()).unwrap();
        store
            .set_ai_tool_permissions(AiToolPermissions {
                allow_readonly: true,
                allow_writeback: false,
                allow_destructive: true,
            })
            .unwrap();
        let snap = store.snapshot();
        assert!(snap.ai_tool_permissions.allow_readonly);
        assert!(!snap.ai_tool_permissions.allow_writeback);
        assert!(snap.ai_tool_permissions.allow_destructive);
    }

    #[test]
    fn persisted_permissions_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = ConfigStore::load_or_init(tmp.path()).unwrap();
        let perms = AiToolPermissions {
            allow_readonly: false,
            allow_writeback: true,
            allow_destructive: true,
        };
        store.set_ai_tool_permissions(perms.clone()).unwrap();

        let loaded = ConfigStore::load_or_init(tmp.path()).unwrap();
        assert_eq!(loaded.snapshot().ai_tool_permissions, perms);
    }
}
