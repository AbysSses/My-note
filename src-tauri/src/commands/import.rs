//! Sidebar file import — copy an external file into the vault.
//!
//! Triggered by DOM `drop` on the sidebar tree (Finder drag-drop after
//! `tauri.conf.json` sets `dragDropEnabled: false`). Semantics:
//!
//! - Source: absolute path on the host filesystem, already parsed out of
//!   `text/uri-list` / `text/plain` on the frontend (see `imageEmbed.ts` for
//!   the reference path of the same shape).
//! - Destination: vault-relative directory (the row the user dropped onto,
//!   or the parent dir when dropping on a file row, or `0-inbox/` when
//!   dropping on the empty tree area — the frontend decides, we just copy).
//! - Naming: keep the source basename; on collision append `-1`, `-2`, …
//!   up to 64 attempts before giving up.
//!
//! Why not reuse `attachment_save`? That one is specifically for "file the
//! bytes into `attachments/YYYY/MM/<timestamp>-<slug>.<ext>`" — a different
//! lifecycle. Sidebar drop is "drop this file **as-is** into the folder I
//! pointed at".
//!
//! See design_V2.md §6.13.9.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::AppState;

use super::vault::resolve_in_vault;

/// Result returned to the frontend after a single-file import.
#[derive(Debug, Clone, Serialize)]
pub struct ImportedFile {
    /// Vault-relative path of the written file (forward-slash separated).
    pub rel_path: String,
    /// Basename of the source, for user-facing notice copy.
    pub original_name: String,
    /// True when a `-N` suffix was appended because the original name clashed.
    pub was_renamed: bool,
    pub bytes_copied: u64,
}

#[tauri::command]
pub fn file_import(
    src_abs: String,
    dst_dir: String,
    state: State<AppState>,
) -> AppResult<ImportedFile> {
    let src = PathBuf::from(&src_abs);
    if !src.is_absolute() {
        return Err(AppError::Other(format!(
            "file_import: source must be an absolute path: {src_abs}"
        )));
    }

    let src_meta = std::fs::metadata(&src)
        .map_err(|e| AppError::Other(format!("file_import: cannot stat source {src_abs}: {e}")))?;
    if src_meta.is_dir() {
        return Err(AppError::Other(format!(
            "file_import: refusing to import a directory (drop individual files instead): {src_abs}"
        )));
    }
    if !src_meta.is_file() {
        return Err(AppError::Other(format!(
            "file_import: source is not a regular file: {src_abs}"
        )));
    }

    let original_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AppError::Other(format!(
                "file_import: source has no usable filename: {src_abs}"
            ))
        })?;
    if original_name.starts_with('.') || original_name.contains('/') || original_name.contains('\\')
    {
        return Err(AppError::Other(format!(
            "file_import: bad source basename: {original_name}"
        )));
    }

    let active = state.active_vault.lock().unwrap().clone();

    // Reject importing a file that already lives inside this vault — that's
    // a move/rename, not an import, and we don't want to silently duplicate
    // a vault-internal note.
    if let Some(vault) = active.as_ref() {
        if let Ok(vault_canon) = std::fs::canonicalize(vault) {
            if let Ok(src_canon) = std::fs::canonicalize(&src) {
                if src_canon.starts_with(&vault_canon) {
                    return Err(AppError::Other(
                        "file_import: source is already inside this vault".to_string(),
                    ));
                }
            }
        }
    }

    // Normalize dst_dir — allow empty (= vault root) and forward-slash form.
    let dst_dir_norm = dst_dir.trim_matches('/').replace('\\', "/");

    // Resolve the target directory first (must be an existing dir inside the
    // vault). `resolve_in_vault("")` returns the vault root, which we also
    // allow.
    let dst_dir_abs = resolve_in_vault(&active, &dst_dir_norm)?;
    let dst_dir_meta = std::fs::metadata(&dst_dir_abs).map_err(|e| {
        AppError::Other(format!(
            "file_import: target dir does not exist: {dst_dir_norm} ({e})"
        ))
    })?;
    if !dst_dir_meta.is_dir() {
        return Err(AppError::Other(format!(
            "file_import: target is not a directory: {dst_dir_norm}"
        )));
    }

    let (stem, ext) = split_name(&original_name);
    let (rel_path, was_renamed, target_abs) =
        pick_free_slot(&active, &dst_dir_norm, &stem, ext.as_deref())?;

    if let Some(parent) = target_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes_copied = std::fs::copy(&src, &target_abs).map_err(|e| {
        AppError::Other(format!(
            "file_import: copy {} -> {}: {}",
            src.display(),
            target_abs.display(),
            e
        ))
    })?;

    tracing::info!(
        src = %src.display(),
        rel = %rel_path,
        renamed = was_renamed,
        bytes = bytes_copied,
        "file_import",
    );

    Ok(ImportedFile {
        rel_path,
        original_name,
        was_renamed,
        bytes_copied,
    })
}

/// Find the first non-colliding vault-relative slot for `<stem>[.ext]` under
/// `dst_dir`. Tries the bare name first, then `<stem>-1`, `<stem>-2`, … up to
/// 64 attempts.
fn pick_free_slot(
    active: &Option<PathBuf>,
    dst_dir_norm: &str,
    stem: &str,
    ext: Option<&str>,
) -> AppResult<(String, bool, PathBuf)> {
    let mut counter: u32 = 0;
    loop {
        let leaf = match (counter, ext) {
            (0, Some(e)) => format!("{stem}.{e}"),
            (0, None) => stem.to_string(),
            (n, Some(e)) => format!("{stem}-{n}.{e}"),
            (n, None) => format!("{stem}-{n}"),
        };
        let rel = if dst_dir_norm.is_empty() {
            leaf.clone()
        } else {
            format!("{dst_dir_norm}/{leaf}")
        };
        let abs = resolve_in_vault(active, &rel)?;
        if !abs.exists() {
            return Ok((rel, counter > 0, abs));
        }
        counter += 1;
        if counter > 64 {
            return Err(AppError::Other(format!(
                "file_import: too many name collisions for {stem} under {dst_dir_norm}"
            )));
        }
    }
}

/// Split a basename into `(stem, ext)`. Hidden files (`.foo`) keep the dot
/// in the stem so we don't accidentally create `.foo-1` collisions across
/// dotfile variants.
fn split_name(name: &str) -> (String, Option<String>) {
    let p = Path::new(name);
    match (
        p.file_stem().and_then(|s| s.to_str()),
        p.extension().and_then(|s| s.to_str()),
    ) {
        (Some(stem), Some(ext)) if !stem.is_empty() => (stem.to_string(), Some(ext.to_string())),
        _ => (name.to_string(), None),
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_name_basic_extension() {
        assert_eq!(
            split_name("foo.md"),
            ("foo".to_string(), Some("md".to_string()))
        );
        assert_eq!(
            split_name("IMG_1234.PNG"),
            ("IMG_1234".to_string(), Some("PNG".to_string()))
        );
    }

    #[test]
    fn split_name_no_extension() {
        assert_eq!(split_name("README"), ("README".to_string(), None));
    }

    #[test]
    fn split_name_double_extension_keeps_rightmost() {
        // `foo.tar.gz` → ("foo.tar", "gz"). That's fine — conflict resolution
        // only cares about the final stem, and `foo.tar-1.gz` is a sensible
        // disambiguation.
        assert_eq!(
            split_name("foo.tar.gz"),
            ("foo.tar".to_string(), Some("gz".to_string()))
        );
    }

    #[test]
    fn split_name_dotfile_has_no_extension() {
        // `.gitignore` has no stem per Path::file_stem + extension rules on
        // Unix: file_stem = ".gitignore", extension = None.
        assert_eq!(split_name(".gitignore"), (".gitignore".to_string(), None));
    }

    #[test]
    fn pick_free_slot_uses_bare_name_when_available() {
        let tmp = tempfile::tempdir().unwrap();
        let vault_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault_path.join("1-notes")).unwrap();
        let active = Some(vault_path.clone());

        let (rel, renamed, abs) = pick_free_slot(&active, "1-notes", "foo", Some("md")).unwrap();
        assert_eq!(rel, "1-notes/foo.md");
        assert!(!renamed);
        assert!(abs.ends_with("1-notes/foo.md"));
    }

    #[test]
    fn pick_free_slot_increments_on_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let vault_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault_path.join("1-notes")).unwrap();
        std::fs::write(vault_path.join("1-notes/foo.md"), b"existing").unwrap();
        std::fs::write(vault_path.join("1-notes/foo-1.md"), b"existing").unwrap();
        let active = Some(vault_path.clone());

        let (rel, renamed, _abs) = pick_free_slot(&active, "1-notes", "foo", Some("md")).unwrap();
        assert_eq!(rel, "1-notes/foo-2.md");
        assert!(renamed);
    }

    #[test]
    fn pick_free_slot_handles_no_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let vault_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(vault_path.join("attachments")).unwrap();
        std::fs::write(vault_path.join("attachments/README"), b"x").unwrap();
        let active = Some(vault_path.clone());

        let (rel, renamed, _abs) = pick_free_slot(&active, "attachments", "README", None).unwrap();
        assert_eq!(rel, "attachments/README-1");
        assert!(renamed);
    }

    #[test]
    fn pick_free_slot_allows_vault_root_target() {
        let tmp = tempfile::tempdir().unwrap();
        let vault_path = tmp.path().to_path_buf();
        let active = Some(vault_path.clone());

        let (rel, renamed, abs) = pick_free_slot(&active, "", "hello", Some("md")).unwrap();
        assert_eq!(rel, "hello.md");
        assert!(!renamed);
        assert!(abs.ends_with("hello.md"));
    }
}
