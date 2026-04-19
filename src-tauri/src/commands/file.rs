use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::services::scanner;
use crate::AppState;

use super::vault::resolve_in_vault;

#[derive(Debug, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
}

#[tauri::command]
pub fn file_read(rel_path: String, state: State<AppState>) -> AppResult<String> {
    let active = state.active_vault.lock().unwrap().clone();
    let p = resolve_in_vault(&active, &rel_path)?;
    Ok(std::fs::read_to_string(p)?)
}

#[tauri::command]
pub fn file_write(rel_path: String, content: String, state: State<AppState>) -> AppResult<()> {
    let active = state.active_vault.lock().unwrap().clone();
    let p = resolve_write_target_in_vault(&active, &rel_path)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic write: tmp file -> rename.
    atomic_write(&p, content.as_bytes())?;
    Ok(())
}

/// Delete a file inside the vault, and its index row. Used by Inbox Review
/// "Delete" and any future trash-style flows. Directory deletion is explicitly
/// not supported — too easy to nuke the whole vault.
#[tauri::command]
pub fn file_delete(rel_path: String, state: State<AppState>) -> AppResult<()> {
    let active = state.active_vault.lock().unwrap().clone();
    let target = resolve_in_vault(&active, &rel_path)?;
    let meta = std::fs::metadata(&target)?;
    if meta.is_dir() {
        return Err(AppError::Other(format!(
            "refusing to delete a directory: {rel_path}"
        )));
    }
    std::fs::remove_file(&target)?;
    if let Some(handle) = state.index_handle() {
        if let Err(e) = scanner::delete_one(&handle, &rel_path) {
            tracing::warn!(rel = %rel_path, error = %e, "file_delete: delete_one failed");
        }
    }
    Ok(())
}

/// Move a file from one vault-relative path to another. Used primarily by the
/// Promote flow (inbox → 1-notes) but also fine for generic rename.
///
/// Guarantees: target parent dir is auto-created; refuses to clobber an
/// existing destination; after a successful rename we synchronously patch the
/// index (delete old row, reindex new row) so downstream UI never observes
/// a window where both paths appear to be missing. The file watcher will
/// issue a redundant reindex a moment later — that's fine, it's idempotent.
#[tauri::command]
pub fn file_move(from: String, to: String, state: State<AppState>) -> AppResult<()> {
    if from == to {
        return Ok(());
    }
    let active = state.active_vault.lock().unwrap().clone();
    let vault = active
        .as_ref()
        .ok_or(AppError::NoActiveVault)?
        .clone();

    let src = resolve_in_vault(&active, &from)?;
    if !src.exists() {
        return Err(AppError::Other(format!("source does not exist: {from}")));
    }
    let dst = resolve_write_target_in_vault(&active, &to)?;
    if dst.exists() {
        return Err(AppError::Other(format!("destination already exists: {to}")));
    }

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Prefer atomic rename; fall back to copy + remove for cross-device moves
    // (not expected within a single vault, but cheap insurance).
    if let Err(e) = std::fs::rename(&src, &dst) {
        tracing::warn!(
            from = %src.display(),
            to = %dst.display(),
            error = %e,
            "rename failed, falling back to copy + remove",
        );
        std::fs::copy(&src, &dst)?;
        std::fs::remove_file(&src)?;
    }

    // Index upkeep: do not hold the outer index Mutex across scanner calls.
    if let Some(handle) = state.index_handle() {
        if let Err(e) = scanner::delete_one(&handle, &from) {
            tracing::warn!(rel = %from, error = %e, "file_move: delete_one failed");
        }
        if let Err(e) = scanner::reindex_one(&handle, &vault, &to) {
            tracing::warn!(rel = %to, error = %e, "file_move: reindex_one failed");
        }
    }

    Ok(())
}

#[tauri::command]
pub fn file_exists(rel_path: String, state: State<AppState>) -> AppResult<bool> {
    let active = state.active_vault.lock().unwrap().clone();
    match resolve_in_vault(&active, &rel_path) {
        Ok(p) => Ok(p.exists()),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
pub fn file_list(rel_dir: String, state: State<AppState>) -> AppResult<Vec<DirEntry>> {
    let active = state.active_vault.lock().unwrap().clone();
    let base = resolve_in_vault(&active, &rel_dir)?;
    // Use canonicalized vault root for prefix stripping, so it matches
    // `read_dir` output (which returns paths based on the dir passed in).
    let vault_canon = std::fs::canonicalize(active.as_ref().ok_or(
        crate::error::AppError::NoActiveVault,
    )?)?;

    let mut entries: Vec<DirEntry> = Vec::new();
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files and the app metadata dir in listings.
        if name.starts_with('.') {
            continue;
        }

        let is_dir = entry.file_type()?.is_dir();
        let path = entry.path();
        let rel = path
            .strip_prefix(&vault_canon)
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());

        entries.push(DirEntry {
            name,
            rel_path: rel,
            is_dir,
        });
    }

    // Directories first, then files; each group alphabetical.
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

pub(crate) fn resolve_write_target_in_vault(
    active_vault: &Option<std::path::PathBuf>,
    rel_path: &str,
) -> AppResult<std::path::PathBuf> {
    let rel = std::path::Path::new(rel_path);
    let file_name = rel
        .file_name()
        .ok_or_else(|| AppError::PathEscape(rel_path.to_string()))?;
    let parent_rel = rel.parent().unwrap_or_else(|| std::path::Path::new(""));
    let parent = resolve_in_vault(active_vault, &parent_rel.to_string_lossy())?;
    Ok(parent.join(file_name))
}

/// Replace `target` with `bytes`. Prefers an atomic tempfile + rename dance,
/// but falls back to a plain `fs::write` when rename fails.
///
/// Atomicity is nice-to-have for a personal notes app — on a crash the worst
/// case is a partially-written file — but being unable to save at all is much
/// worse. On macOS we observed `rename(2)` returning `ENOTDIR` in some
/// environments despite the target and its parent both being regular
/// directories; the fallback keeps the user's edits flowing while we log rich
/// context (via `tracing`) for anyone diagnosing the underlying cause.
pub(crate) fn atomic_write(target: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = target.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent directory")
    })?;

    match try_atomic_write(dir, target, bytes) {
        Ok(()) => return Ok(()),
        Err(e) => {
            tracing::warn!(
                target = %target.display(),
                parent = %dir.display(),
                parent_is_dir = dir.is_dir(),
                error = %e,
                "atomic_write: tempfile+rename failed, falling back to fs::write",
            );
        }
    }

    // Fallback: direct write. Not atomic, but preserves the user's edits.
    std::fs::write(target, bytes)
}

fn try_atomic_write(
    dir: &std::path::Path,
    target: &std::path::Path,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    use std::io::Write;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    // Include the tmp path in the error so logs can correlate with filesystem state.
    tmp.persist(target).map_err(|e| {
        let tmp_path = e.file.path().display().to_string();
        std::io::Error::other(format!(
            "persist {} -> {}: {}",
            tmp_path,
            target.display(),
            e.error
        ))
    })?;
    Ok(())
}
