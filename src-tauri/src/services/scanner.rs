//! Vault → SQLite full / incremental scanner.
//!
//! `full_scan` walks the vault, diffs against the current `notes` rows by
//! mtime, and upserts / deletes as needed. Runs everything inside a single
//! transaction so even a 1000-note cold start completes in well under a
//! second.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use rusqlite::Connection;

use crate::db::indexer::{delete_note, parse_note, resolve_links, upsert_note};
use crate::db::map_sql_err;
use crate::error::{AppError, AppResult};

/// Directories whose contents we intentionally skip during indexing.
/// `.mynotes/` and `.git/` are hidden by the leading-dot rule already; this
/// list is for visible names we want to exclude.
const SKIP_DIRS: &[&str] = &["attachments"];

#[derive(Debug, Default)]
pub struct ScanSummary {
    pub scanned: usize,
    pub updated: usize,
    pub deleted: usize,
}

pub fn full_scan(conn: &Mutex<Connection>, vault: &Path) -> AppResult<ScanSummary> {
    let files = walk_vault_md(vault)?;
    let present: HashSet<String> = files.iter().map(|(_, rel)| rel.clone()).collect();

    let mut conn = conn.lock().unwrap();

    // Snapshot existing rows' mtime so we only re-parse changed files.
    let existing = snapshot_existing(&conn)?;

    let mut summary = ScanSummary {
        scanned: files.len(),
        ..Default::default()
    };

    let tx = conn.transaction().map_err(map_sql_err)?;
    {
        // rusqlite's Transaction derefs to Connection; take &*tx so we pass
        // the expected &Connection type without an extra shim.
        let cn: &Connection = &tx;
        for (abs, rel) in &files {
            let meta = match std::fs::metadata(abs) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(path = %abs.display(), error = %e, "stat failed, skipping");
                    continue;
                }
            };
            let mtime = mtime_secs(&meta);
            if let Some(prev) = existing.get(rel) {
                if *prev == mtime {
                    continue;
                }
            }

            let content = match std::fs::read_to_string(abs) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(path = %abs.display(), error = %e, "read failed, skipping");
                    continue;
                }
            };
            let parsed = parse_note(rel, &content);
            upsert_note(cn, rel, &parsed, meta.len() as i64, mtime)?;
            summary.updated += 1;
        }

        for old_path in existing.keys() {
            if !present.contains(old_path) {
                delete_note(cn, old_path)?;
                summary.deleted += 1;
            }
        }

        resolve_links(cn)?;
    }
    tx.commit().map_err(map_sql_err)?;

    tracing::info!(
        scanned = summary.scanned,
        updated = summary.updated,
        deleted = summary.deleted,
        "full_scan complete"
    );
    Ok(summary)
}

/// Incremental single-file re-index. Used by the notify watcher.
pub fn reindex_one(conn: &Mutex<Connection>, vault: &Path, rel_path: &str) -> AppResult<()> {
    let abs = vault.join(rel_path);
    if !abs.exists() {
        let conn = conn.lock().unwrap();
        delete_note(&*conn, rel_path)?;
        return Ok(());
    }
    let meta = std::fs::metadata(&abs)?;
    let mtime = mtime_secs(&meta);
    let content = std::fs::read_to_string(&abs).unwrap_or_default();
    let parsed = parse_note(rel_path, &content);
    let conn = conn.lock().unwrap();
    upsert_note(&*conn, rel_path, &parsed, meta.len() as i64, mtime)?;
    resolve_links(&*conn)?;
    Ok(())
}

pub fn delete_one(conn: &Mutex<Connection>, rel_path: &str) -> AppResult<()> {
    let conn = conn.lock().unwrap();
    delete_note(&*conn, rel_path)?;
    Ok(())
}

fn snapshot_existing(conn: &Connection) -> AppResult<HashMap<String, i64>> {
    let mut stmt = conn
        .prepare("SELECT path, mtime FROM notes")
        .map_err(map_sql_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(map_sql_err)?;
    let mut out = HashMap::new();
    for row in rows {
        let (p, m) = row.map_err(map_sql_err)?;
        out.insert(p, m);
    }
    Ok(out)
}

fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn walk_vault_md(vault: &Path) -> AppResult<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    walk_inner(vault, vault, &mut out)?;
    Ok(out)
}

fn walk_inner(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) -> AppResult<()> {
    let iter = match std::fs::read_dir(dir) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "read_dir failed");
            return Ok(());
        }
    };
    for entry in iter {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "dir entry read failed");
                continue;
            }
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
            if SKIP_DIRS.contains(&name_str.as_ref()) {
                continue;
            }
            walk_inner(root, &path, out)?;
        } else if ft.is_file() && name_str.ends_with(".md") {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| AppError::Other(format!("strip_prefix {}", path.display())))?;
            // Normalize to forward slashes for DB storage.
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            out.push((path, rel_str));
        }
    }
    Ok(())
}
