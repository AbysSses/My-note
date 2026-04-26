//! Chat session storage — Phase 3-D2b.1.
//!
//! Persists AI chat sessions as append-only JSONL files under
//! `<vault>/.mynotes/ai/chats/<session-id>.jsonl`. Each file is
//! self-contained: the first line carries [`ChatMeta`] (title, creation
//! timestamp, optional linked note) and every subsequent line is a
//! [`ChatMessage`]. This layout gives four guarantees without needing a
//! second SQLite table:
//!
//! 1. **Append-first writes** — new messages are `O_APPEND`ed + `sync_data`'d,
//!    so a crash mid-write can only lose the in-flight line, never earlier
//!    turns.
//! 2. **Human-inspectable** — users can open a session file in any text
//!    editor to audit what was sent to the model.
//! 3. **Trivially deletable** — `rm chats/<id>.jsonl` drops one session and
//!    nothing else; aligns with the broader "everything AI lives under
//!    `.mynotes/ai/`" reset story established in D2a.
//! 4. **Vault-portable** — copying a vault brings the chat history along,
//!    same as the embedding store and the primary index.
//!
//! This module is **storage-only**. It makes no provider calls; D2b.2
//! wires the streaming chat pipeline on top, D2b.3 renders the UI.

// D2b.1 ships storage; the consumers (commands + frontend) arrive in the same
// PR, so the `#[allow(dead_code)]` that covers the rest of the AI module is
// not needed here — every item below is exercised immediately.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

// ── Public types ─────────────────────────────────────────────────────────────

/// Current on-disk schema version written by `create` / `append`.
///
/// Bumped in P3-D5.1 from 1 → 2 to carry the optional
/// `tool_calls` + `tool_call_id` fields on assistant / tool messages.
/// **Load is lenient** — see [`ChatStore::load`] — so pre-D5.1 files
/// (v=1 lines with no tool fields) mix freely with newly-appended v=2
/// lines inside the same `.jsonl`. No migration script is required;
/// the next append quietly stamps v=2 onto the new line.
pub const SCHEMA_VERSION: u32 = 2;

/// Re-exports from [`crate::services::ai::provider`] so storage and the
/// streaming chat pipeline share one set of primitives + one on-the-wire
/// representation.
pub use super::provider::{ChatRole, ToolCall};

/// First line of a `*.jsonl` session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMeta {
    pub v: u32,
    pub session_id: String,
    pub title: String,
    pub created_at: i64,
    /// Vault-relative path of the note this session is "about", if any.
    /// Used by D2b.5 to seed RAG context with the linked note's chunks.
    /// Optional because "scratch" sessions are valid and common.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_note: Option<String>,
}

/// One chat turn. Persisted as a single line in the session file.
///
/// Tool-calling fields were added in P3-D5.1 as **optional**. On load,
/// pre-D5.1 files (v=1 lines) deserialize with `tool_calls = None` and
/// `tool_call_id = None`; on append, new lines are stamped with
/// [`SCHEMA_VERSION`] (= 2) and carry the fields when relevant. The
/// serde `skip_serializing_if = "Option::is_none"` ensures unused
/// tool fields never bloat plain-text turns — v=2 append of a normal
/// user/assistant turn stays byte-for-byte indistinguishable from
/// v=1 shape other than the bumped `v`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub v: u32,
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Header-only view used by the sidebar — avoids parsing the whole file
/// when we only need title + counts. Message count and last-message
/// timestamp are still computed by scanning the jsonl, but no message
/// bodies are copied into memory.
#[derive(Debug, Clone, Serialize)]
pub struct ChatSessionSummary {
    pub session_id: String,
    pub title: String,
    pub created_at: i64,
    pub message_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_note: Option<String>,
}

/// Full session payload returned by [`ChatStore::load`].
#[derive(Debug, Clone, Serialize)]
pub struct ChatSessionFull {
    pub meta: ChatMeta,
    pub messages: Vec<ChatMessage>,
}

// ── Internal line shape ──────────────────────────────────────────────────────

/// On-disk tagged enum. Exists purely so we can parse either line type
/// through one `serde_json::from_str` call without a manual peek.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatLogLine {
    Meta(ChatMeta),
    Message(ChatMessage),
}

// ── Store ────────────────────────────────────────────────────────────────────

/// Filesystem-backed chat session store rooted at `<vault>/.mynotes/ai/chats/`.
///
/// Construction is cheap (`PathBuf` clone only); there is no long-lived
/// connection. The directory is created lazily on the first write so a
/// read-only vault doesn't spuriously grow a `chats/` folder.
pub struct ChatStore {
    root: PathBuf,
}

impl ChatStore {
    /// Open a store rooted at `<vault>/.mynotes/ai/chats/`. Does **not**
    /// touch the filesystem.
    pub fn new(vault: &Path) -> Self {
        Self {
            root: vault.join(".mynotes").join("ai").join("chats"),
        }
    }

    fn ensure_root(&self) -> AppResult<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    fn session_path(&self, session_id: &str) -> AppResult<PathBuf> {
        validate_session_id(session_id)?;
        Ok(self.root.join(format!("{session_id}.jsonl")))
    }

    /// Create a new empty session with a freshly-minted id. The id is
    /// backend-generated so a hostile frontend can't aim the write at an
    /// arbitrary path. Rejects title > 500 chars; the frontend is expected
    /// to truncate before calling.
    pub fn create(
        &self,
        title: &str,
        related_note: Option<String>,
    ) -> AppResult<ChatSessionSummary> {
        if title.chars().count() > 500 {
            return Err(AppError::Other(
                "chat title too long (max 500 chars)".into(),
            ));
        }
        let title = if title.trim().is_empty() {
            "Untitled".to_string()
        } else {
            title.to_string()
        };
        self.ensure_root()?;

        let session_id = new_session_id();
        let now = unix_now();
        let meta = ChatMeta {
            v: SCHEMA_VERSION,
            session_id: session_id.clone(),
            title: title.clone(),
            created_at: now,
            related_note: related_note.clone(),
        };

        let path = self.session_path(&session_id)?;
        // `create_new` guards against the (vanishingly rare) id collision;
        // we'd rather surface the conflict than silently overwrite an
        // existing session.
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        write_line(&mut file, &ChatLogLine::Meta(meta))?;
        file.sync_data()?;

        Ok(ChatSessionSummary {
            session_id,
            title,
            created_at: now,
            message_count: 0,
            last_message_at: None,
            related_note,
        })
    }

    /// Append one plain-text message. Thin wrapper around
    /// [`ChatStore::append_rich`] that passes `tool_calls = None` +
    /// `tool_call_id = None`; kept for the overwhelming majority of
    /// call sites (user / assistant text turns) that don't need
    /// tool-calling plumbing.
    ///
    /// Durability: `O_APPEND` + `sync_data` on every call — a partial
    /// write during a crash can lose at most the line in flight,
    /// never earlier turns.
    pub fn append(
        &self,
        session_id: &str,
        role: ChatRole,
        content: &str,
    ) -> AppResult<ChatMessage> {
        self.append_rich(session_id, role, content, None, None)
    }

    /// Append a message with optional tool-calling metadata. Used by the
    /// D5.1 multi-turn loop to persist assistant-with-tool_calls turns
    /// (non-empty `tool_calls`) and tool-result turns (non-None
    /// `tool_call_id`, `role = Tool`).
    pub fn append_rich(
        &self,
        session_id: &str,
        role: ChatRole,
        content: &str,
        tool_calls: Option<Vec<ToolCall>>,
        tool_call_id: Option<String>,
    ) -> AppResult<ChatMessage> {
        let path = self.session_path(session_id)?;
        if !path.exists() {
            return Err(AppError::Other(format!("session not found: {session_id}")));
        }
        let msg = ChatMessage {
            v: SCHEMA_VERSION,
            id: new_message_id(),
            role,
            content: content.to_string(),
            created_at: unix_now(),
            tool_calls,
            tool_call_id,
        };
        let mut file = OpenOptions::new().append(true).open(&path)?;
        write_line(&mut file, &ChatLogLine::Message(msg.clone()))?;
        file.sync_data()?;
        Ok(msg)
    }

    /// Read a full session into memory. Malformed or unknown-schema lines
    /// produce a structured error so the UI can tell the user to reset the
    /// session rather than hand back a half-parsed transcript.
    pub fn load(&self, session_id: &str) -> AppResult<ChatSessionFull> {
        let path = self.session_path(session_id)?;
        if !path.exists() {
            return Err(AppError::Other(format!("session not found: {session_id}")));
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);

        let mut meta: Option<ChatMeta> = None;
        let mut messages: Vec<ChatMessage> = Vec::new();
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let parsed: ChatLogLine = serde_json::from_str(&line).map_err(|e| {
                AppError::Other(format!(
                    "corrupt chat session {session_id} line {}: {e}",
                    idx + 1
                ))
            })?;
            match parsed {
                ChatLogLine::Meta(m) => {
                    // Forward-compatible: accept any schema version we
                    // know about. A file stamped with a newer-than-us
                    // version is rejected — running an older build
                    // against a future vault should fail loudly rather
                    // than silently drop unknown fields.
                    if m.v > SCHEMA_VERSION {
                        return Err(AppError::Other(format!(
                            "chat session {session_id} uses newer schema v{} (max supported: v{SCHEMA_VERSION})",
                            m.v
                        )));
                    }
                    if meta.is_some() {
                        return Err(AppError::Other(format!(
                            "chat session {session_id} has more than one meta line"
                        )));
                    }
                    meta = Some(m);
                }
                ChatLogLine::Message(msg) => {
                    if meta.is_none() {
                        return Err(AppError::Other(format!(
                            "chat session {session_id} message precedes meta"
                        )));
                    }
                    // Message-level v also forward-compatible. v1 + v2
                    // lines coexist in the same file — both deserialize
                    // cleanly since tool fields are optional on v1.
                    if msg.v > SCHEMA_VERSION {
                        return Err(AppError::Other(format!(
                            "chat session {session_id} has message with newer schema v{} (max supported: v{SCHEMA_VERSION})",
                            msg.v
                        )));
                    }
                    messages.push(msg);
                }
            }
        }

        let meta = meta
            .ok_or_else(|| AppError::Other(format!("chat session {session_id} is empty")))?;
        Ok(ChatSessionFull { meta, messages })
    }

    /// Enumerate all sessions under the root, newest first. Directories
    /// and non-`.jsonl` entries are skipped silently so a user dropping
    /// an unrelated file into `chats/` doesn't crash the sidebar.
    /// Corrupt files are logged and skipped rather than aborting the list.
    pub fn list(&self) -> AppResult<Vec<ChatSessionSummary>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out: Vec<ChatSessionSummary> = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".jsonl") {
                continue;
            }
            let path = entry.path();
            match summary_from_path(&path) {
                Ok(s) => out.push(s),
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(), error = %e,
                        "skipping corrupt chat session"
                    );
                }
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        Ok(out)
    }

    /// Delete one session file. Returns `Ok(false)` when the file was
    /// already gone — mirrors the idempotent delete semantics we use for
    /// notes elsewhere (command palette "undo last delete" still works
    /// because the tombstone is at the note-level, not here).
    pub fn delete(&self, session_id: &str) -> AppResult<bool> {
        let path = self.session_path(session_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn write_line<W: Write>(w: &mut W, line: &ChatLogLine) -> AppResult<()> {
    let json = serde_json::to_string(line)?;
    w.write_all(json.as_bytes())?;
    w.write_all(b"\n")?;
    Ok(())
}

fn summary_from_path(path: &Path) -> AppResult<ChatSessionSummary> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut meta: Option<ChatMeta> = None;
    let mut message_count: u32 = 0;
    let mut last_message_at: Option<i64> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: ChatLogLine = serde_json::from_str(&line)
            .map_err(|e| AppError::Other(format!("corrupt line in {}: {e}", path.display())))?;
        match parsed {
            ChatLogLine::Meta(m) if meta.is_none() => {
                meta = Some(m);
            }
            // Subsequent meta lines are tolerated by `list` (logged only)
            // so one broken session doesn't blank the sidebar; `load`
            // rejects them stricter.
            ChatLogLine::Meta(_) => {}
            ChatLogLine::Message(m) => {
                message_count += 1;
                last_message_at = Some(m.created_at);
            }
        }
    }
    let meta = meta.ok_or_else(|| AppError::Other(format!("no meta in {}", path.display())))?;
    Ok(ChatSessionSummary {
        session_id: meta.session_id,
        title: meta.title,
        created_at: meta.created_at,
        message_count,
        last_message_at,
        related_note: meta.related_note,
    })
}

/// Validate a caller-supplied session id. Frontend-generated ids are
/// never trusted — we enforce the charset and cap the length to keep the
/// filesystem-join from escaping the `chats/` root on hostile input.
pub(crate) fn validate_session_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 64 {
        return Err(AppError::Other(
            "invalid session id (empty or too long)".into(),
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Other(
            "invalid session id (only [A-Za-z0-9_-] allowed)".into(),
        ));
    }
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Hash `input` with SHA-256 and return the first 8 hex chars. Used as a
/// compact collision-resistant suffix for ids — we don't need cryptographic
/// guarantees, just "extremely unlikely to collide within one vault".
fn short_hex(input: &str) -> String {
    let hash = Sha256::digest(input.as_bytes());
    let mut s = String::with_capacity(8);
    for b in &hash[..4] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Per-process monotonic counter folded into id salts so back-to-back
/// `create` / `append` calls within the same OS tick still produce
/// distinct ids. Not exposed — clients should treat ids as opaque.
static ID_SEQ: AtomicU64 = AtomicU64::new(0);

fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let suffix = short_hex(&format!("session-{nanos}-{pid}-{seq}"));
    format!("chat-{ts}-{suffix}")
}

fn new_message_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("msg-{}", short_hex(&format!("msg-{nanos}-{pid}-{seq}")))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store(dir: &Path) -> ChatStore {
        ChatStore::new(dir)
    }

    #[test]
    fn create_then_load_returns_meta_and_empty_messages() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let summary = s.create("hello", Some("notes/a.md".into())).unwrap();
        assert_eq!(summary.title, "hello");
        assert_eq!(summary.message_count, 0);
        assert_eq!(summary.related_note.as_deref(), Some("notes/a.md"));
        assert!(summary.session_id.starts_with("chat-"));

        let full = s.load(&summary.session_id).unwrap();
        assert_eq!(full.meta.title, "hello");
        assert_eq!(full.meta.session_id, summary.session_id);
        assert!(full.messages.is_empty());
    }

    #[test]
    fn empty_title_falls_back_to_untitled() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let summary = s.create("   ", None).unwrap();
        assert_eq!(summary.title, "Untitled");
    }

    #[test]
    fn append_then_load_preserves_order_and_roles() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let summary = s.create("chat", None).unwrap();
        let id = &summary.session_id;
        s.append(id, ChatRole::User, "hi").unwrap();
        s.append(id, ChatRole::Assistant, "hello there").unwrap();
        s.append(id, ChatRole::User, "bye").unwrap();

        let full = s.load(id).unwrap();
        assert_eq!(full.messages.len(), 3);
        assert_eq!(full.messages[0].role, ChatRole::User);
        assert_eq!(full.messages[0].content, "hi");
        assert_eq!(full.messages[1].role, ChatRole::Assistant);
        assert_eq!(full.messages[2].content, "bye");
        // Ids must be unique even when appended back-to-back.
        let ids: std::collections::HashSet<_> =
            full.messages.iter().map(|m| &m.id).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn list_sorts_newest_first_and_aggregates_counts() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let a = s.create("first", None).unwrap();
        // `created_at` has 1-second resolution, so sleep past that boundary
        // to guarantee a distinct ordering key for `b`.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = s.create("second", None).unwrap();
        s.append(&b.session_id, ChatRole::User, "hello").unwrap();

        let list = s.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].session_id, b.session_id);
        assert_eq!(list[0].message_count, 1);
        assert!(list[0].last_message_at.is_some());
        assert_eq!(list[1].session_id, a.session_id);
        assert_eq!(list[1].message_count, 0);
        assert!(list[1].last_message_at.is_none());
    }

    #[test]
    fn load_missing_session_returns_error() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let err = s.load("chat-20260101T000000-deadbeef").unwrap_err();
        assert!(
            err.to_string().contains("session not found"),
            "got {err}"
        );
    }

    #[test]
    fn delete_removes_file_and_is_idempotent() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let sum = s.create("x", None).unwrap();
        assert!(s.delete(&sum.session_id).unwrap());
        assert!(!s.delete(&sum.session_id).unwrap());
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn invalid_session_id_is_rejected_before_touching_fs() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let long = "x".repeat(65);
        for bad in [
            "",
            "../escape",
            "a/b",
            "has space",
            "has.dot",
            long.as_str(),
        ] {
            let e = s.load(bad).unwrap_err();
            assert!(
                e.to_string().contains("invalid session id"),
                "{bad:?} → {e}"
            );
        }
    }

    #[test]
    fn corrupt_line_surfaces_error_in_load() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let sum = s.create("x", None).unwrap();
        let path = s.session_path(&sum.session_id).unwrap();
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "{{not json").unwrap();
        drop(f);

        let err = s.load(&sum.session_id).unwrap_err();
        assert!(err.to_string().contains("corrupt"), "got {err}");
    }

    #[test]
    fn list_on_empty_root_returns_empty_vec() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn list_ignores_non_jsonl_files() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        s.ensure_root().unwrap();
        fs::write(s.root.join("README.md"), b"hello").unwrap();
        fs::write(s.root.join("stray.txt"), b"nope").unwrap();
        s.create("only", None).unwrap();

        let list = s.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "only");
    }

    // ── P3-D5.1 schema forward-compat + tool-calling append ────────────────

    /// Write a synthetic v=1 `.jsonl` file by hand (simulating a file
    /// created by a pre-D5.1 build) then load it via the current store
    /// and confirm all fields survive — tool fields default to None.
    #[test]
    fn load_v1_only_file_defaults_tool_fields_to_none() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        s.ensure_root().unwrap();
        let path = s.root.join("chat-20260101T000000-deadbeef.jsonl");
        let meta = r#"{"type":"meta","v":1,"session_id":"chat-20260101T000000-deadbeef","title":"legacy","created_at":100}"#;
        let msg1 = r#"{"type":"message","v":1,"id":"msg-aaaa","role":"user","content":"hi","created_at":101}"#;
        let msg2 = r#"{"type":"message","v":1,"id":"msg-bbbb","role":"assistant","content":"hello","created_at":102}"#;
        fs::write(&path, format!("{meta}\n{msg1}\n{msg2}\n")).unwrap();

        let full = s.load("chat-20260101T000000-deadbeef").unwrap();
        assert_eq!(full.meta.v, 1);
        assert_eq!(full.messages.len(), 2);
        for m in &full.messages {
            assert!(
                m.tool_calls.is_none(),
                "v=1 message must load with tool_calls = None"
            );
            assert!(m.tool_call_id.is_none());
        }
    }

    /// Same file but with one v=2 line appended — exercises the mixed
    /// case the D5.1 store will produce in practice (old file + new
    /// append).
    #[test]
    fn load_v1_and_v2_mixed_file_parses_cleanly() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        s.ensure_root().unwrap();
        let path = s.root.join("chat-20260101T000001-mixedfil.jsonl");
        let meta = r#"{"type":"meta","v":1,"session_id":"chat-20260101T000001-mixedfil","title":"mixed","created_at":200}"#;
        let v1_msg = r#"{"type":"message","v":1,"id":"msg-1","role":"user","content":"hi","created_at":201}"#;
        let v2_msg_tool = r#"{"type":"message","v":2,"id":"msg-2","role":"assistant","content":"","created_at":202,"tool_calls":[{"id":"call_1","name":"search","arguments":"{\"q\":\"x\"}"}]}"#;
        let v2_msg_result = r#"{"type":"message","v":2,"id":"msg-3","role":"tool","content":"no results","created_at":203,"tool_call_id":"call_1"}"#;
        fs::write(
            &path,
            format!("{meta}\n{v1_msg}\n{v2_msg_tool}\n{v2_msg_result}\n"),
        )
        .unwrap();

        let full = s.load("chat-20260101T000001-mixedfil").unwrap();
        assert_eq!(full.messages.len(), 3);
        assert_eq!(full.messages[0].v, 1);
        assert!(full.messages[0].tool_calls.is_none());
        assert_eq!(full.messages[1].v, 2);
        let tcs = full.messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].name, "search");
        assert_eq!(full.messages[2].role, ChatRole::Tool);
        assert_eq!(
            full.messages[2].tool_call_id.as_deref(),
            Some("call_1")
        );
    }

    /// Newer-than-us schema is rejected — running a stale build against
    /// a v=3 file must fail loudly.
    #[test]
    fn load_newer_schema_is_rejected() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        s.ensure_root().unwrap();
        let path = s.root.join("chat-20260101T000002-future00.jsonl");
        // Cast to u32 so a future SCHEMA_VERSION bump to e.g. 3 still
        // exercises the "v > SCHEMA_VERSION" path.
        let next = SCHEMA_VERSION + 1;
        let meta = format!(
            r#"{{"type":"meta","v":{next},"session_id":"chat-20260101T000002-future00","title":"future","created_at":300}}"#
        );
        fs::write(&path, format!("{meta}\n")).unwrap();

        let err = s.load("chat-20260101T000002-future00").unwrap_err();
        assert!(err.to_string().contains("newer schema"), "got {err}");
    }

    #[test]
    fn append_rich_persists_tool_calls_as_v2() {
        let d = tempdir().unwrap();
        let s = store(d.path());
        let sum = s.create("x", None).unwrap();
        let id = &sum.session_id;
        s.append(id, ChatRole::User, "hi").unwrap();
        let assistant = s
            .append_rich(
                id,
                ChatRole::Assistant,
                "",
                Some(vec![ToolCall {
                    id: "call_x".into(),
                    name: "fake_tool".into(),
                    arguments: r#"{"k":1}"#.into(),
                }]),
                None,
            )
            .unwrap();
        s.append_rich(
            id,
            ChatRole::Tool,
            "tool result here",
            None,
            Some("call_x".into()),
        )
        .unwrap();

        assert_eq!(assistant.v, SCHEMA_VERSION);
        let full = s.load(id).unwrap();
        assert_eq!(full.messages.len(), 3);
        // Raw text check — the wire shape of the v=2 line must carry
        // the tool fields verbatim.
        let raw = fs::read_to_string(s.session_path(id).unwrap()).unwrap();
        assert!(raw.contains("\"v\":2"), "expected v=2 in raw: {raw}");
        assert!(raw.contains("\"tool_calls\""), "raw: {raw}");
        assert!(raw.contains("\"tool_call_id\":\"call_x\""), "raw: {raw}");
    }
}
