//! OS-level secret storage for AI provider API keys — Phase 3-D2a.2.
//!
//! API keys never touch `app-config.json`. Instead we route them through the
//! platform-native secret store (macOS Keychain / Windows Credential Manager /
//! Linux secret-service) via the `keyring` crate. The rest of the codebase
//! only sees a narrow trait so tests can inject an in-memory mock and the
//! UI flow isn't coupled to any specific OS mechanism.
//!
//! ## Service / account namespacing
//!
//! One **service** string per MyNotes installation (`SERVICE_NAME`). Each
//! provider kind (`openai` / `anthropic` / …) becomes a distinct **account**
//! string, so users who experiment with multiple providers don't clobber
//! each other's keys.
//!
//! ## What this module does NOT do
//!
//! - **Encryption at rest inside the app**: the OS keystore already handles
//!   that; layering our own encryption would just add a key-management
//!   problem. See `plan_P3.md §6` for the "no end-to-end encryption" stance.
//! - **Multi-account per provider**: one key per provider kind. If the user
//!   wants a personal + work key, they swap them in Settings. Surfacing this
//!   as a feature is out of scope for D2a.
//! - **Key rotation / expiry**: providers may expire keys on their side;
//!   we surface this as a Network / Auth error at call time, not proactively.

// ── Public trait ──────────────────────────────────────────────────────────────

/// Narrow contract for storing and retrieving API keys. Implementations must
/// be `Send + Sync` so `AppState` can hold one behind `Arc<dyn SecretStore>`.
///
/// All operations are synchronous — `keyring` itself is sync, and the
/// in-flight surface is small (one set / one get per provider change).
pub trait SecretStore: Send + Sync {
    /// Store (or overwrite) the API key for `provider`.
    fn set_api_key(&self, provider: &str, secret: &str) -> Result<(), SecretError>;

    /// Return the stored key, or `None` if no entry exists for this provider.
    /// A returned `Err` means the store itself failed (permission denied,
    /// keyring daemon not running, etc.) — not the same as "key absent".
    fn get_api_key(&self, provider: &str) -> Result<Option<String>, SecretError>;

    /// Remove any stored entry. Idempotent — removing an absent entry is
    /// `Ok(())`, not an error.
    fn delete_api_key(&self, provider: &str) -> Result<(), SecretError>;

    /// Cheap presence check. Default implementation just calls `get_api_key`
    /// and checks `Option`, but impls may override if they can short-circuit
    /// without decrypting (e.g. Windows CredRead(…, SECRET_DOESNT_EXIST)).
    fn has_api_key(&self, provider: &str) -> Result<bool, SecretError> {
        Ok(self.get_api_key(provider)?.is_some())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Permission denied / daemon not running / user denied prompt.
    #[error("secret store unavailable: {0}")]
    Unavailable(String),
    /// Invalid input (empty provider, empty secret).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Keyring backend-specific failure we can't categorise further.
    #[error("{0}")]
    Other(String),
}

// Bridge `SecretError` into `AppError::Other` so commands can use `?` without
// an explicit map_err at every site.
impl From<SecretError> for crate::error::AppError {
    fn from(e: SecretError) -> Self {
        crate::error::AppError::Other(format!("secret: {e}"))
    }
}

// ── Production impl: keyring ──────────────────────────────────────────────────

/// Single shared service name under which every MyNotes secret is stored.
/// Changing this string is a **breaking change** — existing installations
/// will appear to have "lost" their saved keys.
const SERVICE_NAME: &str = "com.mynotes.ai";

/// Thin wrapper over [`keyring::Entry`]. Constructs a fresh Entry per call
/// — `keyring::Entry` is cheap to build and carries no state we'd benefit
/// from caching. Avoiding caching keeps the struct `Clone` + tiny.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, provider: &str) -> Result<keyring::Entry, SecretError> {
        if provider.trim().is_empty() {
            return Err(SecretError::InvalidArgument("provider is empty".into()));
        }
        keyring::Entry::new(SERVICE_NAME, provider)
            .map_err(|e| SecretError::Unavailable(e.to_string()))
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_api_key(&self, provider: &str, secret: &str) -> Result<(), SecretError> {
        if secret.is_empty() {
            return Err(SecretError::InvalidArgument("secret is empty".into()));
        }
        let entry = self.entry(provider)?;
        entry
            .set_password(secret)
            .map_err(|e| SecretError::Other(e.to_string()))
    }

    fn get_api_key(&self, provider: &str) -> Result<Option<String>, SecretError> {
        let entry = self.entry(provider)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            // keyring 3.x: `NoEntry` is the sentinel for "no such credential".
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Other(e.to_string())),
        }
    }

    fn delete_api_key(&self, provider: &str) -> Result<(), SecretError> {
        let entry = self.entry(provider)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Deleting a missing entry is a no-op from the caller's POV.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Other(e.to_string())),
        }
    }
}

// ── Test impl: in-memory ──────────────────────────────────────────────────────

/// Unit-test-only `SecretStore` backed by a `HashMap` guarded by `Mutex`.
/// Used by [`crate::services::ai::openai::tests`] and anywhere else we want
/// to verify plumbing without triggering the real OS Keychain dialog.
#[cfg(test)]
pub struct MockSecretStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl MockSecretStore {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(test)]
impl Default for MockSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl SecretStore for MockSecretStore {
    fn set_api_key(&self, provider: &str, secret: &str) -> Result<(), SecretError> {
        if provider.trim().is_empty() {
            return Err(SecretError::InvalidArgument("provider is empty".into()));
        }
        if secret.is_empty() {
            return Err(SecretError::InvalidArgument("secret is empty".into()));
        }
        self.inner
            .lock()
            .unwrap()
            .insert(provider.to_string(), secret.to_string());
        Ok(())
    }

    fn get_api_key(&self, provider: &str) -> Result<Option<String>, SecretError> {
        Ok(self.inner.lock().unwrap().get(provider).cloned())
    }

    fn delete_api_key(&self, provider: &str) -> Result<(), SecretError> {
        self.inner.lock().unwrap().remove(provider);
        Ok(())
    }
}

// ── Unit tests (mock only — real Keychain not exercised in CI) ────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_roundtrip_set_get() {
        let s = MockSecretStore::new();
        s.set_api_key("openai", "sk-xyz").unwrap();
        assert_eq!(s.get_api_key("openai").unwrap().as_deref(), Some("sk-xyz"));
    }

    #[test]
    fn mock_overwrite() {
        let s = MockSecretStore::new();
        s.set_api_key("openai", "one").unwrap();
        s.set_api_key("openai", "two").unwrap();
        assert_eq!(s.get_api_key("openai").unwrap().as_deref(), Some("two"));
    }

    #[test]
    fn mock_get_missing_is_none_not_err() {
        let s = MockSecretStore::new();
        assert_eq!(s.get_api_key("nope").unwrap(), None);
    }

    #[test]
    fn mock_delete_is_idempotent() {
        let s = MockSecretStore::new();
        s.delete_api_key("never-set").unwrap();
        s.set_api_key("openai", "sk").unwrap();
        s.delete_api_key("openai").unwrap();
        s.delete_api_key("openai").unwrap();
        assert_eq!(s.get_api_key("openai").unwrap(), None);
    }

    #[test]
    fn mock_has_api_key_matches_get() {
        let s = MockSecretStore::new();
        assert!(!s.has_api_key("openai").unwrap());
        s.set_api_key("openai", "sk").unwrap();
        assert!(s.has_api_key("openai").unwrap());
    }

    #[test]
    fn mock_rejects_empty_provider() {
        let s = MockSecretStore::new();
        let err = s.set_api_key("", "sk").unwrap_err();
        assert!(matches!(err, SecretError::InvalidArgument(_)));
    }

    #[test]
    fn mock_rejects_empty_secret() {
        let s = MockSecretStore::new();
        let err = s.set_api_key("openai", "").unwrap_err();
        assert!(matches!(err, SecretError::InvalidArgument(_)));
    }

    #[test]
    fn mock_isolates_providers() {
        let s = MockSecretStore::new();
        s.set_api_key("openai", "one").unwrap();
        s.set_api_key("anthropic", "two").unwrap();
        assert_eq!(s.get_api_key("openai").unwrap().as_deref(), Some("one"));
        assert_eq!(s.get_api_key("anthropic").unwrap().as_deref(), Some("two"));
    }
}
