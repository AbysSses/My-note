//! Attachment commands — image/file paste, drop, preview, orphan cleanup.
//!
//! Storage layout: `vault/attachments/YYYY/MM/<stem>.<ext>`
//! Naming: `YYYYMMDD-HHmmss-<slug|rand6hex>.<ext>`
//!
//! See design_V2.md §6.12 for the full rationale.

use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;

use chrono::{Datelike, Local, Timelike};
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::map_sql_err;
use crate::error::{AppError, AppResult};
use crate::AppState;

use super::file::atomic_write;
use super::vault::resolve_in_vault;

/// One entry in `attachment_list` / `attachment_unreferenced` responses.
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentInfo {
    /// Vault-relative path with forward slashes (e.g. `attachments/2026/04/foo.png`).
    pub rel_path: String,
    pub size: u64,
    /// Unix seconds.
    pub mtime: i64,
}

// ---------------------------------------------------------------------------
// attachment_save
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn attachment_save(
    bytes: Vec<u8>,
    original_name: Option<String>,
    ext: String,
    state: State<AppState>,
) -> AppResult<String> {
    if bytes.is_empty() {
        return Err(AppError::Other("attachment_save: empty bytes".to_string()));
    }
    let ext = sanitize_ext(&ext)
        .ok_or_else(|| AppError::Other(format!("attachment_save: bad ext: {ext}")))?;

    let now = Local::now();
    let year = format!("{:04}", now.year());
    let month = format!("{:02}", now.month());
    let ts = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    );

    let suffix = match original_name
        .as_deref()
        .map(strip_ext)
        .filter(|s| !s.trim().is_empty())
    {
        Some(stem) => slugify(&stem),
        None => rand_hex6(),
    };
    let suffix = if suffix.is_empty() {
        rand_hex6()
    } else {
        suffix
    };

    // Guarantee uniqueness: if somehow the file exists (second paste within a
    // second + same slug), append a numeric counter.
    let active = state.active_vault.lock().unwrap().clone();
    let mut rel = format!("attachments/{}/{}/{}-{}.{}", year, month, ts, suffix, ext);
    let mut counter = 1;
    while resolve_in_vault(&active, &rel)
        .map(|p| p.exists())
        .unwrap_or(false)
    {
        counter += 1;
        rel = format!(
            "attachments/{}/{}/{}-{}-{}.{}",
            year, month, ts, suffix, counter, ext
        );
        if counter > 64 {
            return Err(AppError::Other(
                "attachment_save: failed to find a non-colliding filename".into(),
            ));
        }
    }

    // resolve_in_vault on a brand new path returns the canonicalized parent
    // joined with the new leaf — perfect for writing.
    let target = resolve_in_vault(&active, &rel)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&target, &bytes)?;
    tracing::info!(rel = %rel, size = bytes.len(), "attachment_save");
    Ok(rel)
}

// ---------------------------------------------------------------------------
// attachment_read_bytes
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn attachment_read_bytes(rel_path: String, state: State<AppState>) -> AppResult<Vec<u8>> {
    // Gate: only paths under `attachments/` allowed — this IPC is NOT a
    // general-purpose binary file reader.
    let norm = rel_path.replace('\\', "/");
    if !norm.starts_with("attachments/") && norm != "attachments" {
        return Err(AppError::Other(format!(
            "attachment_read_bytes: only attachments/... allowed, got: {rel_path}"
        )));
    }
    let active = state.active_vault.lock().unwrap().clone();
    let p = resolve_in_vault(&active, &norm)?;
    Ok(std::fs::read(p)?)
}

// ---------------------------------------------------------------------------
// attachment_read_external_bytes
// ---------------------------------------------------------------------------
//
// Used by the editor when the user pastes / drops / types an image reference
// that lives *outside* the vault (e.g. `/Users/…/Desktop/foo.png` or a
// `file://…` URI from WeChat's clipboard). WKWebView refuses to load
// `file://` into `<img>` from a `http://localhost` origin, so we have to
// proxy the bytes through IPC.
//
// Hardening:
// - Only absolute paths accepted.
// - Only an allow-list of image extensions (prevents surprise reads of
//   arbitrary large binaries).
// - Hard cap at 50 MB (an image bigger than that is almost certainly a
//   mis-selection, and we don't want to OOM by accident).
// - Rejects symlinks? No — following them is fine; users drop files from
//   aliases all the time on macOS.

const EXTERNAL_READ_MAX_BYTES: u64 = 50 * 1024 * 1024;

#[tauri::command]
pub fn attachment_read_external_bytes(abs_path: String) -> AppResult<Vec<u8>> {
    let p = std::path::PathBuf::from(&abs_path);
    if !p.is_absolute() {
        return Err(AppError::Other(format!(
            "attachment_read_external_bytes: not an absolute path: {abs_path}"
        )));
    }
    if !is_allowed_image_ext(&p) {
        return Err(AppError::Other(format!(
            "attachment_read_external_bytes: unsupported extension: {abs_path}"
        )));
    }
    let meta = std::fs::metadata(&p)?;
    if !meta.is_file() {
        return Err(AppError::Other(format!(
            "attachment_read_external_bytes: not a file: {abs_path}"
        )));
    }
    if meta.len() > EXTERNAL_READ_MAX_BYTES {
        return Err(AppError::Other(format!(
            "attachment_read_external_bytes: file too large ({} bytes, cap {}): {abs_path}",
            meta.len(),
            EXTERNAL_READ_MAX_BYTES
        )));
    }
    Ok(std::fs::read(&p)?)
}

fn is_allowed_image_ext(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif" | "heic" | "heif"
        ),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// attachment_list
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn attachment_list(state: State<AppState>) -> AppResult<Vec<AttachmentInfo>> {
    let active = state.active_vault.lock().unwrap().clone();
    list_all_attachments(&active)
}

fn list_all_attachments(
    active_vault: &Option<std::path::PathBuf>,
) -> AppResult<Vec<AttachmentInfo>> {
    let vault = active_vault
        .as_ref()
        .ok_or(AppError::NoActiveVault)?
        .clone();
    let dir = vault.join("attachments");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk_attachments(&dir, &dir, &mut out)?;
    // Stable order: by mtime desc (newest first), then rel_path.
    out.sort_by(|a, b| {
        b.mtime
            .cmp(&a.mtime)
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    Ok(out)
}

fn walk_attachments(root: &Path, dir: &Path, out: &mut Vec<AttachmentInfo>) -> AppResult<()> {
    let iter = match std::fs::read_dir(dir) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "attachments read_dir failed");
            return Ok(());
        }
    };
    for entry in iter {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk_attachments(root, &path, out)?;
        } else if ft.is_file() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Strip prefix to the *vault* root (one up from `root=attachments/`)
            // so `rel_path` is `attachments/...`.
            let from_attachments_root = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let rel = format!("attachments/{}", from_attachments_root);
            out.push(AttachmentInfo {
                rel_path: rel,
                size: meta.len(),
                mtime: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// attachment_unreferenced
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn attachment_unreferenced(state: State<AppState>) -> AppResult<Vec<AttachmentInfo>> {
    let active = state.active_vault.lock().unwrap().clone();
    let all = list_all_attachments(&active)?;

    let referenced = match state.index_handle() {
        Some(handle) => {
            let conn = handle.lock().unwrap();
            embed_dst_set(&conn)?
        }
        None => HashSet::new(),
    };

    let orphans: Vec<AttachmentInfo> = all
        .into_iter()
        .filter(|a| !referenced.contains(&a.rel_path))
        .collect();
    Ok(orphans)
}

fn embed_dst_set(conn: &Connection) -> AppResult<HashSet<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT dst_resolved FROM links
             WHERE link_type = 'embed' AND dst_resolved IS NOT NULL",
        )
        .map_err(map_sql_err)?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(map_sql_err)?;
    let mut out = HashSet::new();
    for row in rows {
        let v = row.map_err(map_sql_err)?;
        out.insert(v.replace('\\', "/"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// attachment_delete_batch
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn attachment_delete_batch(
    rel_paths: Vec<String>,
    state: State<AppState>,
) -> AppResult<Vec<String>> {
    let active = state.active_vault.lock().unwrap().clone();
    let mut deleted = Vec::new();
    for rel in rel_paths {
        let norm = rel.replace('\\', "/");
        if !norm.starts_with("attachments/") {
            tracing::warn!(rel = %rel, "attachment_delete_batch: skip non-attachment path");
            continue;
        }
        let p = match resolve_in_vault(&active, &norm) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(rel = %rel, error = %e, "attachment_delete_batch: resolve failed");
                continue;
            }
        };
        match std::fs::metadata(&p) {
            Ok(m) if m.is_file() => {}
            Ok(_) => {
                tracing::warn!(rel = %rel, "attachment_delete_batch: not a file, skipping");
                continue;
            }
            Err(e) => {
                tracing::warn!(rel = %rel, error = %e, "attachment_delete_batch: metadata failed");
                continue;
            }
        }
        if let Err(e) = std::fs::remove_file(&p) {
            tracing::warn!(rel = %rel, error = %e, "attachment_delete_batch: remove_file failed");
            continue;
        }
        deleted.push(norm);
    }
    tracing::info!(count = deleted.len(), "attachment_delete_batch done");
    Ok(deleted)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn sanitize_ext(ext: &str) -> Option<String> {
    let e = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if e.is_empty() || e.len() > 12 {
        return None;
    }
    if e.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(e)
    } else {
        None
    }
}

fn strip_ext(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

/// Basic slugify — keep ASCII alphanumerics and CJK, replace runs of the rest
/// with a single dash. Trim leading/trailing dashes. Truncate to 48 chars.
/// Not perfect (doesn't transliterate), but good enough for filenames.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true; // leading
    for ch in s.chars() {
        let keep = ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch); // CJK unified ideographs
        if keep {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    // Trim trailing dashes.
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().count() > 48 {
        // Truncate by char count, not bytes.
        let truncated: String = out.chars().take(48).collect();
        // Don't leave a trailing dash.
        truncated.trim_end_matches('-').to_string()
    } else {
        out
    }
}

/// Low-entropy 6-hex-char id from SystemTime nanoseconds. Collisions possible
/// in the same millisecond but `attachment_save` retries with a counter when
/// the file already exists, so this is fine for single-user workflows.
fn rand_hex6() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    let v = (nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) & 0xFF_FFFF;
    format!("{:06x}", v)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_ext_basics() {
        assert_eq!(sanitize_ext("png").as_deref(), Some("png"));
        assert_eq!(sanitize_ext(".PNG").as_deref(), Some("png"));
        assert_eq!(sanitize_ext("  jpeg  ").as_deref(), Some("jpeg"));
        assert_eq!(sanitize_ext("../evil").as_deref(), None);
        assert_eq!(sanitize_ext("").as_deref(), None);
        assert_eq!(sanitize_ext("png/"), None);
        assert_eq!(sanitize_ext("thisiswaytoolongext"), None);
    }

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("foo___bar"), "foo-bar");
        assert_eq!(slugify("  trailing  "), "trailing");
        assert_eq!(slugify("架构 discussion 图"), "架构-discussion-图");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn strip_ext_basics() {
        assert_eq!(strip_ext("foo.png"), "foo");
        assert_eq!(strip_ext("foo.bar.baz"), "foo.bar");
        assert_eq!(strip_ext("noext"), "noext");
        assert_eq!(strip_ext(".hidden"), ".hidden");
    }

    #[test]
    fn rand_hex6_is_6_hex() {
        let s = rand_hex6();
        assert_eq!(s.len(), 6);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
