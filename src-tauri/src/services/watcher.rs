//! Live filesystem → index updates.
//!
//! On top of `notify` + `notify-debouncer-full` we fire `reindex_one` /
//! `delete_one` whenever a `.md` file under the vault is touched. The
//! debouncer coalesces bursty events (editor save + external touch +
//! iCloud sync-down) into one re-index per file per ~200ms window.
//!
//! Phase 3-D2a.3b extends this path with a **second**, slower queue for AI
//! embeddings:
//!
//! - `create/modify` of an eligible `.md` note → enqueue note path
//! - queue is coalesced for 30 s per note
//! - when a note matures, run `embed_service::embed_note`
//! - `remove` skips the network path and synchronously prunes stale chunks via
//!   `EmbeddingStore::delete_by_note`
//!
//! The AI queue only accepts `create/modify` when auto-embed is enabled
//! (`ai_enabled != false` + provider config complete). Delete cleanup still
//! runs even when AI is disabled so stale vectors do not survive until the
//! user re-enables AI later.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// `Watcher` trait must be in scope to call `.watch()` on the underlying
// notify watcher returned by the debouncer.
use notify::{EventKind, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::services::ai::embed_service::{self, SkipReason};
use crate::services::ai::embedding_store::EmbeddingStore;
use crate::services::ai::runtime;
use crate::services::ai::secrets::KeyringSecretStore;
use crate::services::config::ConfigStore;
use crate::services::scanner;

/// Holds onto the debouncer so its OS watch + internal threads stay alive.
/// Drop to stop watching (this also closes the event channel, and the
/// consumer thread exits naturally on `recv` error).
pub struct WatcherHandle {
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
}

const INDEX_DEBOUNCE: Duration = Duration::from_millis(200);
const AI_EMBED_DEBOUNCE: Duration = Duration::from_secs(30);
const AI_IDLE_WAIT: Duration = Duration::from_secs(300);

#[derive(Debug)]
enum AiWatchMsg {
    Upsert(String),
    Delete(String),
}

#[derive(Debug, Default)]
struct AiDebounceQueue {
    deadlines: HashMap<String, Instant>,
}

impl AiDebounceQueue {
    fn queue_upsert(&mut self, rel: String, now: Instant) {
        self.deadlines.insert(rel, now + AI_EMBED_DEBOUNCE);
    }

    fn queue_delete(&mut self, rel: &str) {
        self.deadlines.remove(rel);
    }

    fn pop_due(&mut self, now: Instant) -> Vec<String> {
        let mut due = Vec::new();
        self.deadlines.retain(|rel, deadline| {
            if *deadline <= now {
                due.push(rel.clone());
                false
            } else {
                true
            }
        });
        due.sort();
        due
    }

    fn next_wait(&self, now: Instant) -> Duration {
        self.deadlines
            .values()
            .min()
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(AI_IDLE_WAIT)
    }
}

pub fn start_watcher(
    conn: Arc<Mutex<Connection>>,
    vault: PathBuf,
    config: Arc<Mutex<ConfigStore>>,
    embeddings: Option<Arc<Mutex<EmbeddingStore>>>,
) -> AppResult<WatcherHandle> {
    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(INDEX_DEBOUNCE, None, tx)
        .map_err(|e| AppError::Other(format!("notify init: {e}")))?;
    debouncer
        .watcher()
        .watch(&vault, RecursiveMode::Recursive)
        .map_err(|e| AppError::Other(format!("notify watch {}: {e}", vault.display())))?;

    let ai_tx = embeddings.map(|store| start_ai_worker(vault.clone(), config.clone(), store));
    let vault_for_thread = vault.clone();
    let config_for_thread = config.clone();
    std::thread::spawn(move || {
        for res in rx {
            match res {
                Ok(events) => {
                    for ev in events {
                        handle_event(
                            &conn,
                            &vault_for_thread,
                            &config_for_thread,
                            ai_tx.as_ref(),
                            &ev,
                        );
                    }
                }
                Err(errs) => {
                    for e in errs {
                        tracing::warn!(error = %e, "watcher error");
                    }
                }
            }
        }
        tracing::debug!(vault = %vault_for_thread.display(), "watcher thread exit");
    });

    Ok(WatcherHandle {
        _debouncer: debouncer,
    })
}

fn handle_event(
    conn: &Arc<Mutex<Connection>>,
    vault: &Path,
    config: &Arc<Mutex<ConfigStore>>,
    ai_tx: Option<&std::sync::mpsc::Sender<AiWatchMsg>>,
    ev: &notify_debouncer_full::DebouncedEvent,
) {
    for abs in &ev.event.paths {
        let Some(rel) = watched_markdown_rel(vault, abs) else {
            continue;
        };

        let result = match ev.event.kind {
            EventKind::Remove(_) => scanner::delete_one(conn, &rel),
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any => {
                scanner::reindex_one(conn, vault, &rel)
            }
            EventKind::Access(_) | EventKind::Other => continue,
        };
        if let Err(e) = result {
            tracing::warn!(rel = %rel, error = %e, "watcher: reindex failed");
        }

        let Some(ai_tx) = ai_tx else {
            continue;
        };
        match ev.event.kind {
            EventKind::Remove(_) => {
                let _ = ai_tx.send(AiWatchMsg::Delete(rel.clone()));
            }
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any => {
                if !abs.exists() {
                    let _ = ai_tx.send(AiWatchMsg::Delete(rel.clone()));
                    continue;
                }
                let enabled = {
                    let cfg = config.lock().unwrap();
                    runtime::auto_embed_enabled(&cfg)
                };
                if enabled {
                    let _ = ai_tx.send(AiWatchMsg::Upsert(rel.clone()));
                }
            }
            EventKind::Access(_) | EventKind::Other => {}
        }
    }
}

fn watched_markdown_rel(vault: &Path, abs: &Path) -> Option<String> {
    let rel = abs
        .strip_prefix(vault)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    if !rel.ends_with(".md") {
        return None;
    }
    // Skip hidden + attachments/ — same filter as the initial scan.
    if rel.split('/').any(|seg| seg.starts_with('.')) {
        return None;
    }
    if rel.starts_with("attachments/") {
        return None;
    }
    Some(rel)
}

fn start_ai_worker(
    vault: PathBuf,
    config: Arc<Mutex<ConfigStore>>,
    store: Arc<Mutex<EmbeddingStore>>,
) -> std::sync::mpsc::Sender<AiWatchMsg> {
    let (tx, rx) = std::sync::mpsc::channel::<AiWatchMsg>();
    let vault_for_thread = vault.clone();
    std::thread::spawn(move || {
        ai_worker_loop(rx, &vault_for_thread, config, store);
        tracing::debug!(vault = %vault_for_thread.display(), "watcher ai thread exit");
    });
    tx
}

fn ai_worker_loop(
    rx: std::sync::mpsc::Receiver<AiWatchMsg>,
    vault: &Path,
    config: Arc<Mutex<ConfigStore>>,
    store: Arc<Mutex<EmbeddingStore>>,
) {
    let mut queue = AiDebounceQueue::default();

    loop {
        let timeout = queue.next_wait(Instant::now());
        match rx.recv_timeout(timeout) {
            Ok(AiWatchMsg::Upsert(rel)) => queue.queue_upsert(rel, Instant::now()),
            Ok(AiWatchMsg::Delete(rel)) => {
                queue.queue_delete(&rel);
                match store.lock().unwrap().delete_by_note(&rel) {
                    Ok(removed) => {
                        if removed > 0 {
                            tracing::debug!(rel = %rel, removed, "watcher ai: deleted stale chunks");
                        }
                    }
                    Err(e) => tracing::warn!(rel = %rel, error = %e, "watcher ai: delete failed"),
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        flush_due_embeddings(&mut queue, vault, &config, store.clone());
    }
}

fn flush_due_embeddings(
    queue: &mut AiDebounceQueue,
    vault: &Path,
    config: &Arc<Mutex<ConfigStore>>,
    store: Arc<Mutex<EmbeddingStore>>,
) {
    let due = queue.pop_due(Instant::now());
    if due.is_empty() {
        return;
    }

    let maybe_provider = {
        let cfg = config.lock().unwrap();
        if !runtime::auto_embed_enabled(&cfg) {
            None
        } else {
            match runtime::build_configured_provider(&cfg, &KeyringSecretStore::new()) {
                Ok(built) => Some(built),
                Err(e) => {
                    tracing::warn!(error = %e, count = due.len(), "watcher ai: provider unavailable");
                    None
                }
            }
        }
    };

    let Some((provider, model)) = maybe_provider else {
        return;
    };

    for rel in due {
        match tauri::async_runtime::block_on(embed_service::embed_note(
            store.clone(),
            &provider,
            &model,
            vault,
            &rel,
        )) {
            Ok(out) => match out.skipped {
                Some(SkipReason::UpToDate) => {
                    tracing::debug!(rel = %rel, "watcher ai: up to date");
                }
                Some(SkipReason::Empty) => {
                    tracing::debug!(rel = %rel, "watcher ai: empty note");
                }
                None => {
                    tracing::info!(
                        rel = %rel,
                        chunks = out.chunks_embedded,
                        tokens = out.tokens_used,
                        "watcher ai: embedded"
                    );
                }
            },
            Err(e) => tracing::warn!(rel = %rel, error = %e, "watcher ai: embed failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_upsert_is_debounced_and_deduped() {
        let mut q = AiDebounceQueue::default();
        let now = Instant::now();
        q.queue_upsert("a.md".into(), now);
        q.queue_upsert("a.md".into(), now + Duration::from_secs(1));
        assert_eq!(q.deadlines.len(), 1);
        assert_eq!(
            q.pop_due(now + AI_EMBED_DEBOUNCE + Duration::from_secs(2)),
            vec!["a.md"]
        );
    }

    #[test]
    fn queue_delete_cancels_pending_upsert() {
        let mut q = AiDebounceQueue::default();
        let now = Instant::now();
        q.queue_upsert("a.md".into(), now);
        q.queue_delete("a.md");
        assert!(q
            .pop_due(now + AI_EMBED_DEBOUNCE + Duration::from_secs(1))
            .is_empty());
    }

    #[test]
    fn watched_markdown_rel_filters_hidden_and_attachments() {
        let vault = Path::new("/tmp/vault");
        assert_eq!(
            watched_markdown_rel(vault, &vault.join("1-notes/a.md")),
            Some("1-notes/a.md".into())
        );
        assert_eq!(
            watched_markdown_rel(vault, &vault.join(".mynotes/a.md")),
            None
        );
        assert_eq!(
            watched_markdown_rel(vault, &vault.join("attachments/img.md")),
            None
        );
        assert_eq!(
            watched_markdown_rel(vault, &vault.join("1-notes/a.txt")),
            None
        );
    }
}
