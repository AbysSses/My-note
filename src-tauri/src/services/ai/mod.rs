//! AI services — Phase 3-D2a.
//!
//! Module layout:
//!
//! - [`provider`] — `AiProvider` trait + `MockProvider`. Real HTTP impls live
//!   in dedicated modules (`openai`).
//! - [`openai`] — OpenAI-compatible HTTP `AiProvider`. Covers OpenAI /
//!   OpenRouter / Ollama / LM Studio / vLLM / Together.ai under the same
//!   `/v1/embeddings` wire protocol.
//! - [`secrets`] — `SecretStore` trait + `KeyringSecretStore` (prod) +
//!   `MockSecretStore` (tests). Bridges OS-native keystores behind one
//!   narrow API so the command layer never touches `keyring::Entry`.
//! - [`chunker`] — pure-function markdown → chunk splitter.
//! - [`embedding_store`] — `embeddings.sqlite` wrapper. Separate from
//!   `index.sqlite` so AI state can be reset by deleting `.mynotes/ai/`.
//!
//! ## Current consumer surface
//!
//! As of D2a.2, the `commands/ai.rs` layer consumes:
//!
//! - `provider::{AiProvider, EmbedRequest, ProviderError}` + `openai::OpenAiProvider`
//!   inside `ai_provider_test_connection`.
//! - `secrets::{SecretStore, KeyringSecretStore}` inside every provider-config
//!   command.
//!
//! The `chunker` and `embedding_store` modules are still pending consumers —
//! they land in D2a.3 (embed-note IPC + watcher integration). Their public
//! items are exercised by unit tests only, so we keep a **module-scoped**
//! `#[allow(dead_code)]` on each of those two files rather than a blanket
//! allow on this module root.

pub mod chat_store;
pub mod chunker;
pub mod embed_service;
pub mod embedding_store;
pub mod init_service;
pub mod openai;
pub mod provider;
pub mod rag;
pub mod runtime;
pub mod secrets;
pub mod system_prompt;
/// Agentic-chat tool registry (P3-D5.1). Empty by default; real tools
/// (search_notes, propose_edit_note, …) land in D5.2+.
pub mod tool_registry;
/// Built-in tools the registry ships with (P3-D5.2). First batch is
/// the read-only 🟢 set — search/list/read. 🟡 write-back tools land in
/// D5.4.
pub mod tools;
pub mod usage_log;
