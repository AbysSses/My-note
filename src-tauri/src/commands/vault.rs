use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::services::{scanner, watcher};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct VaultInfo {
    pub path: String,
    pub initialized_at: String,
}

/// Result of `vault_reseed_templates`.
///
/// Buckets are disjoint — each bundled template lands in exactly one list.
/// `added` covers templates that didn't exist; `updated` covers templates
/// whose on-disk content differed from the bundled bytes; `unchanged` covers
/// exact matches (no write). Files in `templates/` that aren't in our bundle
/// are left alone (e.g. user's custom templates).
#[derive(Debug, Serialize)]
pub struct ReseedSummary {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
}

const MYNOTES_DIR: &str = ".mynotes";
const CONFIG_FILE: &str = "config.json";

/// Standard LYT directory layout. Created on init (see §4.1 of design.md).
const LYT_DIRS: &[&str] = &[
    "0-inbox",
    "1-notes",
    "2-moc",
    "3-journal",
    "4-projects",
    "attachments",
    "templates",
];

/// Default templates baked into the binary (`src-tauri/templates/*.md`).
/// Each entry is (filename, contents).
const BUNDLED_TEMPLATES: &[(&str, &str)] = &[
    ("inbox.md", include_str!("../../templates/inbox.md")),
    ("note.md", include_str!("../../templates/note.md")),
    ("moc.md", include_str!("../../templates/moc.md")),
    ("daily.md", include_str!("../../templates/daily.md")),
    ("weekly.md", include_str!("../../templates/weekly.md")),
    ("project.md", include_str!("../../templates/project.md")),
    (
        "project-note.md",
        include_str!("../../templates/project-note.md"),
    ),
];

#[tauri::command]
pub fn vault_is_initialized(path: String) -> bool {
    Path::new(&path).join(MYNOTES_DIR).join(CONFIG_FILE).exists()
}

#[tauri::command]
pub fn vault_open(path: String, state: State<AppState>) -> AppResult<VaultInfo> {
    let p = PathBuf::from(&path);
    if !p.join(MYNOTES_DIR).join(CONFIG_FILE).exists() {
        return Err(AppError::NotAVault(path));
    }

    *state.active_vault.lock().unwrap() = Some(p.clone());
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.record_recent_vault(&p)?;
    }
    attach_index(&state, &p)?;

    Ok(VaultInfo {
        path,
        initialized_at: chrono::Local::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn vault_init(path: String, state: State<AppState>) -> AppResult<VaultInfo> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        std::fs::create_dir_all(&root)?;
    }

    // Create LYT directories (idempotent).
    for dir in LYT_DIRS {
        std::fs::create_dir_all(root.join(dir))?;
    }

    // Create .mynotes/ app metadata dir.
    let app_dir = root.join(MYNOTES_DIR);
    std::fs::create_dir_all(&app_dir)?;
    std::fs::create_dir_all(app_dir.join("logs"))?;

    // Write config.json (skeleton for now).
    let cfg_path = app_dir.join(CONFIG_FILE);
    if !cfg_path.exists() {
        let payload = serde_json::json!({
            "schema_version": 1,
            "created_at": chrono::Local::now().to_rfc3339(),
            "vault_name": root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("vault"),
        });
        std::fs::write(&cfg_path, serde_json::to_string_pretty(&payload)?)?;
    }

    // Write bundled templates if missing (never overwrite user edits).
    let tpl_dir = root.join("templates");
    for (name, body) in BUNDLED_TEMPLATES {
        let dst = tpl_dir.join(name);
        if !dst.exists() {
            std::fs::write(&dst, body)?;
        }
    }

    *state.active_vault.lock().unwrap() = Some(root.clone());
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.record_recent_vault(&root)?;
    }
    attach_index(&state, &root)?;

    Ok(VaultInfo {
        path,
        initialized_at: chrono::Local::now().to_rfc3339(),
    })
}

/// Force-refresh bundled templates into the open vault's `templates/` dir.
///
/// Unlike `vault_init` — which only writes a template when the destination
/// doesn't exist — this command reads each bundled template and compares
/// byte-for-byte with the on-disk copy. If they differ, the bundled version
/// wins; if the file doesn't exist, it's created. Files unrelated to the
/// bundle (user's own custom templates) are left alone — we never delete.
///
/// Motivated by the scenario where a bundled template was edited in the repo
/// after the vault was first initialized (e.g. Week 5 Task 2's
/// `project_status` → `status` fix). `vault_init` can't migrate those because
/// of the existence guard; this command is the user-facing migration knob.
///
/// Does NOT touch user notes. Does NOT reindex — templates/ changes land
/// via the running watcher within ~200ms; synchronous reindex isn't worth
/// the extra code path.
#[tauri::command]
pub fn vault_reseed_templates(state: State<AppState>) -> AppResult<ReseedSummary> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;

    let tpl_dir = vault.join("templates");
    std::fs::create_dir_all(&tpl_dir)?;

    let mut summary = ReseedSummary {
        added: Vec::new(),
        updated: Vec::new(),
        unchanged: Vec::new(),
    };

    for (name, body) in BUNDLED_TEMPLATES {
        let dst = tpl_dir.join(name);
        if !dst.exists() {
            std::fs::write(&dst, body)?;
            summary.added.push((*name).to_string());
            continue;
        }
        // Compare raw bytes — CRLF vs LF differences count as a diff,
        // which is what we want: rewriting unifies line endings with the
        // bundled (LF) form.
        let existing = std::fs::read(&dst)?;
        if existing.as_slice() == body.as_bytes() {
            summary.unchanged.push((*name).to_string());
        } else {
            std::fs::write(&dst, body)?;
            summary.updated.push((*name).to_string());
        }
    }

    Ok(summary)
}

/// Open the per-vault SQLite index, run a full scan, and stash it in AppState.
///
/// Any previous index connection is dropped as a side effect of the
/// `state.index` slot being overwritten. Full scan runs synchronously — for
/// an empty vault it's microseconds; for 1000 notes the design target is
/// under 2 s (§5.4). If we outgrow that we'll move this onto a background
/// thread and emit an event when done.
fn attach_index(state: &State<AppState>, vault: &Path) -> AppResult<()> {
    // Stop any existing watcher before swapping the DB — otherwise a trailing
    // event from the old vault could land on the new connection.
    *state.watcher.lock().unwrap() = None;

    let conn = db::open_for_vault(&state.app_support_dir, vault)?;
    let handle = Arc::new(Mutex::new(conn));
    match scanner::full_scan(&handle, vault) {
        Ok(summary) => tracing::info!(
            scanned = summary.scanned,
            updated = summary.updated,
            deleted = summary.deleted,
            "index ready"
        ),
        Err(e) => tracing::warn!(error = %e, "full_scan failed; index may be stale"),
    }
    *state.index.lock().unwrap() = Some(handle.clone());

    match watcher::start_watcher(handle, vault.to_path_buf()) {
        Ok(w) => *state.watcher.lock().unwrap() = Some(w),
        Err(e) => tracing::warn!(error = %e, "watcher failed to start — live updates disabled"),
    }
    Ok(())
}

#[tauri::command]
pub fn vault_recent(state: State<AppState>) -> AppResult<Vec<String>> {
    let cfg = state.config.lock().unwrap();
    Ok(cfg.recent_vaults())
}

/// Resolve a safe absolute path inside the active vault, rejecting traversal.
pub fn resolve_in_vault(active_vault: &Option<PathBuf>, rel: &str) -> AppResult<PathBuf> {
    let vault = active_vault
        .as_ref()
        .ok_or(AppError::NoActiveVault)?
        .clone();
    let joined = if rel.is_empty() {
        vault.clone()
    } else {
        vault.join(rel)
    };

    // Canonicalize both and verify the resolved path stays within the vault.
    // Allow non-existent leaves (for file_write to new paths).
    let vault_canon = std::fs::canonicalize(&vault)?;
    // Canonicalize the longest existing prefix to resolve symlinks and checking '..',
    // and then ensure it stays within the vault.
    let mut current = joined.clone();
    let mut suffix = PathBuf::new();

    while !current.exists() {
        if let Some(name) = current.file_name() {
            let mut new_suffix = PathBuf::from(name);
            new_suffix.push(suffix);
            suffix = new_suffix;
        } else {
            // e.g. ".." or other things that aren't normal file names
            return Err(AppError::PathEscape(rel.to_string()));
        }
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            return Err(AppError::PathEscape(rel.to_string()));
        }
    }

    let canon = std::fs::canonicalize(&current)?;
    if !canon.starts_with(&vault_canon) {
        return Err(AppError::PathEscape(rel.to_string()));
    }

    // Now check that the non-existent suffix doesn't contain any escape sequences.
    // `file_name()` above implicitly shields this, but just to be exhaustive:
    for comp in suffix.components() {
        if matches!(
            comp,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return Err(AppError::PathEscape(rel.to_string()));
        }
    }

    // IMPORTANT: `Path::join("")` appends a trailing separator to `self`, which
    // turns `/…/file.md` into `/…/file.md/`. POSIX treats a trailing slash on a
    // non-directory as `ENOTDIR`, breaking every subsequent `read_to_string` /
    // `rename` on an existing file. Skip the join when the suffix is empty
    // (the common case: the target already exists).
    Ok(if suffix.as_os_str().is_empty() {
        canon
    } else {
        canon.join(suffix)
    })
}

