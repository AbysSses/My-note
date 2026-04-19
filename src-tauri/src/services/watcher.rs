//! Live filesystem → index updates.
//!
//! On top of `notify` + `notify-debouncer-full` we fire `reindex_one` /
//! `delete_one` whenever a `.md` file under the vault is touched. The
//! debouncer coalesces bursty events (editor save + external touch +
//! iCloud sync-down) into one re-index per file per ~200ms window.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// `Watcher` trait must be in scope to call `.watch()` on the underlying
// notify watcher returned by the debouncer.
use notify::{EventKind, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::services::scanner;

/// Holds onto the debouncer so its OS watch + internal threads stay alive.
/// Drop to stop watching (this also closes the event channel, and the
/// consumer thread exits naturally on `recv` error).
pub struct WatcherHandle {
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
}

pub fn start_watcher(
    conn: Arc<Mutex<Connection>>,
    vault: PathBuf,
) -> AppResult<WatcherHandle> {
    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(Duration::from_millis(200), None, tx)
        .map_err(|e| AppError::Other(format!("notify init: {e}")))?;
    debouncer
        .watcher()
        .watch(&vault, RecursiveMode::Recursive)
        .map_err(|e| AppError::Other(format!("notify watch {}: {e}", vault.display())))?;

    let vault_for_thread = vault.clone();
    std::thread::spawn(move || {
        for res in rx {
            match res {
                Ok(events) => {
                    for ev in events {
                        handle_event(&conn, &vault_for_thread, &ev);
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
    ev: &notify_debouncer_full::DebouncedEvent,
) {
    for abs in &ev.event.paths {
        let rel = match abs.strip_prefix(vault) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if !rel.ends_with(".md") {
            continue;
        }
        // Skip hidden + attachments/ — same filter as the initial scan.
        if rel.split('/').any(|seg| seg.starts_with('.')) {
            continue;
        }
        if rel.starts_with("attachments/") {
            continue;
        }

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
    }
}
