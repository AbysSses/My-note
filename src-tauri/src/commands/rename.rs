//! File rename / move with automatic reference rewriting.
//!
//! `file_move` is the dumb version — it only relocates the file. `file_move_with_refs`
//! additionally scans every note that links to the source via the `links` index,
//! patches the raw `[[...]]` / `![alt](...)` text to point at the new location,
//! and re-indexes every touched file so `dst_resolved` stays correct.
//!
//! Scope (Phase 2 Task 4):
//! - Handles *path* renames only. Frontmatter `title:` changes are a separate
//!   operation — this command does not touch title-form wiki references
//!   (`[[OldTitle]]`) even if it happens to encounter them.
//! - Wiki forms rewritten: `[[stem]]`, `[[path/stem]]`, `[[path/stem.md]]`,
//!   plus their alias variants `[[... | alias]]`.
//! - Embed form rewritten: `![alt](path)` where `path` equals the moved file.
//! - Directory renames: out of scope. Only single-file renames.
//!
//! Failure semantics:
//! - We rewrite all referrers first, then move the file. If an individual
//!   referrer rewrite fails (read / parse / write), we log + push a warning
//!   and continue with the rest; we still move the file at the end.
//! - If the source doesn't exist or the destination already does, we bail
//!   before any rewrites run.

use std::collections::{BTreeMap, HashSet};

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::map_sql_err;
use crate::error::{AppError, AppResult};
use crate::services::scanner;
use crate::AppState;

use super::file::{atomic_write, resolve_write_target_in_vault};
use super::vault::resolve_in_vault;

/// Result returned to the frontend after a rename-with-refs.
#[derive(Debug, Clone, Serialize)]
pub struct RenameResult {
    pub old_path: String,
    pub new_path: String,
    /// Vault-relative paths of every note whose body we edited.
    pub rewritten_files: Vec<String>,
    /// Total raw-link-text replacements made across all rewritten files.
    pub rewritten_links: usize,
    /// Non-fatal issues (e.g. a referring file we couldn't read). The file
    /// move still proceeded; callers should surface these to the user.
    pub warnings: Vec<String>,
}

const PREVIEW_LIMIT: usize = 100;

/// Advisory summary for a file rename. Pure dry-run: no writes, no move, no
/// reindex. The preview list is truncated to `PREVIEW_LIMIT` entries so a
/// highly-linked note doesn't flood the modal.
#[derive(Debug, Clone, Serialize)]
pub struct FileRenamePreview {
    pub old_path: String,
    pub new_path: String,
    pub rewritten_files_total: usize,
    pub rewritten_files_preview: Vec<String>,
    pub rewritten_links: usize,
}

/// Advisory summary for a directory rename. `rewritten_files_*` only reports
/// external referrers because in-tree notes travel with the moved directory,
/// matching the existing execute-path banner semantics.
#[derive(Debug, Clone, Serialize)]
pub struct DirRenamePreview {
    pub old_path: String,
    pub new_path: String,
    pub moved_files_total: usize,
    pub moved_markdown_files: usize,
    pub moved_other_files: usize,
    pub moved_files_preview: Vec<String>,
    pub rewritten_files_total: usize,
    pub rewritten_files_preview: Vec<String>,
    pub rewritten_links: usize,
}

#[derive(Debug, Clone)]
struct RewriteImpactSummary {
    rewritten_files_total: usize,
    rewritten_files_preview: Vec<String>,
    rewritten_links: usize,
}

#[tauri::command]
pub fn file_move_with_refs_preview(
    from: String,
    to: String,
    state: State<AppState>,
) -> AppResult<FileRenamePreview> {
    if from == to {
        return Ok(FileRenamePreview {
            old_path: from,
            new_path: to,
            rewritten_files_total: 0,
            rewritten_files_preview: Vec::new(),
            rewritten_links: 0,
        });
    }

    let active = state.active_vault.lock().unwrap().clone();
    let src = resolve_in_vault(&active, &from)?;
    if !src.exists() {
        return Err(AppError::Other(format!("source does not exist: {from}")));
    }
    if std::fs::metadata(&src)?.is_dir() {
        return Err(AppError::Other(format!(
            "file_move_with_refs: directory renames are not supported in Phase 2: {from}"
        )));
    }
    let dst = resolve_write_target_in_vault(&active, &to)?;
    if dst.exists() {
        return Err(AppError::Other(format!("destination already exists: {to}")));
    }

    let plan = RewritePlan::from_paths(&from, &to);
    let handle = state.index_handle().ok_or_else(|| {
        AppError::Other("file_move_with_refs: index not initialized; open a vault first".into())
    })?;

    let referring = {
        let conn = handle.lock().unwrap();
        query_referring(&conn, &from)?
    };
    let per_file = group_referring_rows(referring);
    let summary = summarize_preview_rewrites(&active, &per_file, &plan, None);

    Ok(FileRenamePreview {
        old_path: from,
        new_path: to,
        rewritten_files_total: summary.rewritten_files_total,
        rewritten_files_preview: summary.rewritten_files_preview,
        rewritten_links: summary.rewritten_links,
    })
}

#[tauri::command]
pub fn file_move_with_refs(
    from: String,
    to: String,
    state: State<AppState>,
) -> AppResult<RenameResult> {
    if from == to {
        return Ok(RenameResult {
            old_path: from,
            new_path: to,
            rewritten_files: Vec::new(),
            rewritten_links: 0,
            warnings: Vec::new(),
        });
    }

    let active = state.active_vault.lock().unwrap().clone();
    let vault = active.as_ref().ok_or(AppError::NoActiveVault)?.clone();

    let src = resolve_in_vault(&active, &from)?;
    if !src.exists() {
        return Err(AppError::Other(format!("source does not exist: {from}")));
    }
    // Refuse directory renames — scope limit.
    if std::fs::metadata(&src)?.is_dir() {
        return Err(AppError::Other(format!(
            "file_move_with_refs: directory renames are not supported in Phase 2: {from}"
        )));
    }
    let dst = resolve_write_target_in_vault(&active, &to)?;
    if dst.exists() {
        return Err(AppError::Other(format!("destination already exists: {to}")));
    }

    let plan = RewritePlan::from_paths(&from, &to);

    let handle = state.index_handle().ok_or_else(|| {
        AppError::Other("file_move_with_refs: index not initialized; open a vault first".into())
    })?;

    // ---- 1. Discover every note that links to `from`. ----
    let referring: Vec<(String, String, String)> = {
        let conn = handle.lock().unwrap();
        query_referring(&conn, &from)?
    };

    // Group by src note so we open each referrer only once.
    let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (src_note, dst_raw, lt) in referring {
        per_file.entry(src_note).or_default().push((dst_raw, lt));
    }

    let mut result = RenameResult {
        old_path: from.clone(),
        new_path: to.clone(),
        rewritten_files: Vec::new(),
        rewritten_links: 0,
        warnings: Vec::new(),
    };

    // ---- 2. Rewrite each referring file. ----
    for (ref_path, links) in &per_file {
        let abs = match resolve_in_vault(&active, ref_path) {
            Ok(p) => p,
            Err(e) => {
                result
                    .warnings
                    .push(format!("{ref_path}: resolve failed: {e}"));
                continue;
            }
        };
        let body = match std::fs::read_to_string(&abs) {
            Ok(b) => b,
            Err(e) => {
                result
                    .warnings
                    .push(format!("{ref_path}: read failed: {e}"));
                continue;
            }
        };
        let (new_body, hits) = plan.apply(&body, links);
        if hits == 0 {
            // The indexer said there were links, but the raw text no longer
            // contains them — the file was edited since last index. Not fatal.
            continue;
        }
        if let Err(e) = atomic_write(&abs, new_body.as_bytes()) {
            result
                .warnings
                .push(format!("{ref_path}: write failed: {e}"));
            continue;
        }
        result.rewritten_files.push(ref_path.clone());
        result.rewritten_links += hits;
    }

    // ---- 3. Move the actual file. ----
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
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

    // ---- 4. Re-index: drop the old path, reindex the new path, reindex every
    //      referrer we touched (their `links.dst` strings changed). ----
    if let Err(e) = scanner::delete_one(&handle, &from) {
        tracing::warn!(from = %from, error = %e, "rename: delete_one failed");
    }
    // NB: reindex_one also re-runs resolve_links, so a single pass at the end
    // suffices — but we still call it once per touched file to get their own
    // `links.src` rows updated.
    if let Err(e) = scanner::reindex_one(&handle, &vault, &to) {
        tracing::warn!(rel = %to, error = %e, "rename: reindex_one(new) failed");
    }
    for rel in &result.rewritten_files {
        if let Err(e) = scanner::reindex_one(&handle, &vault, rel) {
            tracing::warn!(rel = %rel, error = %e, "rename: reindex_one(referrer) failed");
        }
    }

    tracing::info!(
        from = %from,
        to = %to,
        files = result.rewritten_files.len(),
        links = result.rewritten_links,
        warnings = result.warnings.len(),
        "file_move_with_refs done"
    );

    Ok(result)
}

#[tauri::command]
pub fn dir_move_with_refs_preview(
    from: String,
    to: String,
    state: State<AppState>,
) -> AppResult<DirRenamePreview> {
    let from_norm = normalize_dir(&from);
    let to_norm = normalize_dir(&to);

    if from_norm.is_empty() || to_norm.is_empty() {
        return Err(AppError::Other(
            "dir_move_with_refs: vault root cannot be renamed".into(),
        ));
    }
    if from_norm == to_norm {
        return Ok(DirRenamePreview {
            old_path: from_norm,
            new_path: to_norm,
            moved_files_total: 0,
            moved_markdown_files: 0,
            moved_other_files: 0,
            moved_files_preview: Vec::new(),
            rewritten_files_total: 0,
            rewritten_files_preview: Vec::new(),
            rewritten_links: 0,
        });
    }
    if from_norm.starts_with(".mynotes") || to_norm.starts_with(".mynotes") {
        return Err(AppError::Other(
            "dir_move_with_refs: .mynotes/ is off-limits".into(),
        ));
    }
    if to_norm == from_norm || to_norm.starts_with(&format!("{from_norm}/")) {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: target '{to_norm}' is inside source '{from_norm}'"
        )));
    }

    let active = state.active_vault.lock().unwrap().clone();
    let src_abs = resolve_in_vault(&active, &from_norm)?;
    if !src_abs.exists() {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: source does not exist: {from_norm}"
        )));
    }
    if !std::fs::metadata(&src_abs)?.is_dir() {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: source is not a directory: {from_norm}"
        )));
    }
    let dst_abs = resolve_write_target_in_vault(&active, &to_norm)?;
    if dst_abs.exists() {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: destination already exists: {to_norm}"
        )));
    }

    let mut files: Vec<FileMove> = Vec::new();
    walk_dir_all(&src_abs, &src_abs, &from_norm, &to_norm, &mut files)?;

    let handle = state.index_handle().ok_or_else(|| {
        AppError::Other("dir_move_with_refs: index not initialized; open a vault first".into())
    })?;
    let plan = build_dir_plan(&files);
    let like_prefix = like_escape(&from_norm);
    let referring = {
        let conn = handle.lock().unwrap();
        query_referring_dir(&conn, &like_prefix)?
    };
    let per_file = group_referring_rows(referring);
    let from_prefix = format!("{from_norm}/");
    let rewrite_summary =
        summarize_preview_rewrites(&active, &per_file, &plan, Some(from_prefix.as_str()));
    let (moved_files_total, moved_markdown_files, moved_other_files, moved_files_preview) =
        summarize_moved_files(&files);

    Ok(DirRenamePreview {
        old_path: from_norm,
        new_path: to_norm,
        moved_files_total,
        moved_markdown_files,
        moved_other_files,
        moved_files_preview,
        rewritten_files_total: rewrite_summary.rewritten_files_total,
        rewritten_files_preview: rewrite_summary.rewritten_files_preview,
        rewritten_links: rewrite_summary.rewritten_links,
    })
}

/// Pull every link row that currently resolves to `target_path`. Excludes the
/// source itself (a note can't link to itself in a way that needs rewriting
/// for this operation — the file is about to move intact).
fn query_referring(
    conn: &Connection,
    target_path: &str,
) -> AppResult<Vec<(String, String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT src, dst, link_type FROM links
               WHERE dst_resolved = ?1 AND src != ?1",
        )
        .map_err(map_sql_err)?;
    let rows = stmt
        .query_map([target_path], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(map_sql_err)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_sql_err)?);
    }
    Ok(out)
}

fn group_referring_rows(
    rows: Vec<(String, String, String)>,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (src_note, dst_raw, lt) in rows {
        per_file.entry(src_note).or_default().push((dst_raw, lt));
    }
    per_file
}

fn preview_paths(mut paths: Vec<String>) -> (usize, Vec<String>) {
    paths.sort();
    paths.dedup();
    let total = paths.len();
    if paths.len() > PREVIEW_LIMIT {
        paths.truncate(PREVIEW_LIMIT);
    }
    (total, paths)
}

fn summarize_preview_rewrites(
    active_vault: &Option<std::path::PathBuf>,
    per_file: &BTreeMap<String, Vec<(String, String)>>,
    plan: &RewritePlan,
    external_only_prefix: Option<&str>,
) -> RewriteImpactSummary {
    let mut rewritten_files: Vec<String> = Vec::new();
    let mut rewritten_links = 0usize;

    for (ref_path, links) in per_file {
        let abs = match resolve_in_vault(active_vault, ref_path) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let body = match std::fs::read_to_string(&abs) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let (_, hits) = plan.apply(&body, links);
        if hits == 0 {
            continue;
        }
        rewritten_links += hits;
        let should_report = match external_only_prefix {
            Some(prefix) => !ref_path.starts_with(prefix),
            None => true,
        };
        if should_report {
            rewritten_files.push(ref_path.clone());
        }
    }

    let (rewritten_files_total, rewritten_files_preview) = preview_paths(rewritten_files);
    RewriteImpactSummary {
        rewritten_files_total,
        rewritten_files_preview,
        rewritten_links,
    }
}

// ---------------------------------------------------------------------------
// Rewrite plan: encodes the (old_raw, new_raw) pairs we're prepared to
// substitute inside referring files.
// ---------------------------------------------------------------------------

struct RewritePlan {
    /// Wiki-link inner targets that should be rewritten if a referrer wrote
    /// the link in exactly this raw form.
    wiki_pairs: Vec<(String, String)>,
    /// Embed `![alt](path)` paths.
    embed_pairs: Vec<(String, String)>,
}

impl RewritePlan {
    fn from_paths(from: &str, to: &str) -> Self {
        let from_norm = from.replace('\\', "/");
        let to_norm = to.replace('\\', "/");

        let from_no_ext = strip_md(&from_norm);
        let to_no_ext = strip_md(&to_norm);

        let from_stem = stem_of(&from_norm);
        let to_stem = stem_of(&to_norm);

        let mut wiki_pairs: Vec<(String, String)> = Vec::new();

        // Order matters: apply longer forms *first* so a bare-stem rewrite
        // doesn't eat into `[[dir/stem]]` by accident. Our regex is bracket-
        // anchored so collisions are unlikely, but being explicit is cheap.
        if from_norm != to_norm {
            wiki_pairs.push((from_norm.clone(), to_norm.clone()));
        }
        if from_no_ext != to_no_ext
            && from_no_ext != from_norm
            && !from_no_ext.is_empty()
            && !to_no_ext.is_empty()
        {
            wiki_pairs.push((from_no_ext.clone(), to_no_ext.clone()));
        }
        if from_stem != to_stem && !from_stem.is_empty() && !to_stem.is_empty() {
            wiki_pairs.push((from_stem.clone(), to_stem.clone()));
        }

        let embed_pairs = if from_norm != to_norm {
            vec![(from_norm, to_norm)]
        } else {
            Vec::new()
        };

        Self {
            wiki_pairs,
            embed_pairs,
        }
    }

    /// Rewrite every applicable occurrence in `body`. `links` is the set of
    /// (raw_dst, link_type) rows the indexer reported for this referrer —
    /// we only activate a plan pair if the referrer actually wrote the link
    /// in that raw form. Keeps us from rewriting title-form refs.
    fn apply(&self, body: &str, links: &[(String, String)]) -> (String, usize) {
        let mut raw_wiki: HashSet<&str> = HashSet::new();
        let mut raw_embed: HashSet<&str> = HashSet::new();
        for (raw, lt) in links {
            match lt.as_str() {
                "wiki" => {
                    raw_wiki.insert(raw.as_str());
                }
                "embed" => {
                    raw_embed.insert(raw.as_str());
                }
                _ => {}
            }
        }

        let mut out = body.to_string();
        let mut hits = 0usize;

        for (old, new) in &self.wiki_pairs {
            if !raw_wiki.contains(old.as_str()) {
                continue;
            }
            let (next, n) = replace_wiki(&out, old, new);
            out = next;
            hits += n;
        }

        for (old, new) in &self.embed_pairs {
            if !raw_embed.contains(old.as_str()) {
                continue;
            }
            let (next, n) = replace_embed(&out, old, new);
            out = next;
            hits += n;
        }

        (out, hits)
    }
}

fn strip_md(p: &str) -> String {
    p.strip_suffix(".md").unwrap_or(p).to_string()
}

fn stem_of(p: &str) -> String {
    let p = p.replace('\\', "/");
    let last = p.rsplit('/').next().unwrap_or("");
    strip_md(last)
}

/// Replace `[[old]]` / `[[old|alias]]` with `[[new]]` / `[[new|alias]]`.
/// Alias text is preserved verbatim (including internal whitespace).
fn replace_wiki(body: &str, old: &str, new: &str) -> (String, usize) {
    // Bracket-anchored; allows optional whitespace around the inner target
    // (Obsidian-tolerant). Alias group is captured so we can reconstruct it.
    let pattern = format!(r"\[\[\s*{}\s*(\|[^\]]*)?\s*\]\]", regex::escape(old));
    let re = match regex::Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return (body.to_string(), 0),
    };
    let mut n = 0usize;
    let new_body = re
        .replace_all(body, |caps: &regex::Captures| {
            n += 1;
            match caps.get(1) {
                Some(alias) => format!("[[{}{}]]", new, alias.as_str()),
                None => format!("[[{}]]", new),
            }
        })
        .into_owned();
    (new_body, n)
}

/// Replace the path portion of `![alt](path)` when `path == old`.
fn replace_embed(body: &str, old: &str, new: &str) -> (String, usize) {
    let pattern = format!(r"(!\[[^\]]*\]\()\s*{}\s*(\))", regex::escape(old));
    let re = match regex::Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return (body.to_string(), 0),
    };
    let mut n = 0usize;
    let new_body = re
        .replace_all(body, |caps: &regex::Captures| {
            n += 1;
            format!("{}{}{}", &caps[1], new, &caps[2])
        })
        .into_owned();
    (new_body, n)
}

// ===========================================================================
// Directory-level rename.
//
// `dir_move_with_refs` is `file_move_with_refs`'s bigger sibling. The semantic
// target is a whole subtree: `1-notes/` → `notes/`, or `4-projects/old/` →
// `4-projects/new/`. We walk the source directory, compute the new relative
// path for every file inside it (md + non-md — the latter for attachments /
// arbitrary user files), build one aggregated `RewritePlan`, and run a single
// rewrite pass per external referrer. Then we `fs::rename` the directory
// itself and re-index the touched files.
//
// This avoids:
// - Re-opening each referrer once per moved file (O(files × referrers) IO);
// - Partial-state nightmares from running per-file move in a loop mid-tree.
//
// Hard scope limits (rejected in pre-check):
// - `from` must be a directory.
// - `to` must not exist.
// - `to` must not be a strict subpath of `from` (no self-nesting).
// - Neither side may be empty string nor under `.mynotes/`.
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct DirRenameResult {
    pub old_path: String,
    pub new_path: String,
    /// Total number of files we re-parented (md + non-md, inside the tree).
    pub moved_files: usize,
    /// Vault-relative paths of every *external* referrer whose body was edited.
    /// (Referrers that live inside the moved tree are rewritten too, but by
    /// construction their final rel-path is their new in-tree rel-path, not
    /// the pre-move one.)
    pub rewritten_files: Vec<String>,
    /// Total raw-link-text replacements across all rewritten referrers.
    pub rewritten_links: usize,
    /// Non-fatal issues (read/write failures on individual referrers).
    pub warnings: Vec<String>,
}

#[tauri::command]
pub fn dir_move_with_refs(
    from: String,
    to: String,
    state: State<AppState>,
) -> AppResult<DirRenameResult> {
    let from_norm = normalize_dir(&from);
    let to_norm = normalize_dir(&to);

    if from_norm.is_empty() || to_norm.is_empty() {
        return Err(AppError::Other(
            "dir_move_with_refs: vault root cannot be renamed".into(),
        ));
    }
    if from_norm == to_norm {
        return Ok(DirRenameResult {
            old_path: from_norm.clone(),
            new_path: to_norm,
            moved_files: 0,
            rewritten_files: Vec::new(),
            rewritten_links: 0,
            warnings: Vec::new(),
        });
    }
    if from_norm.starts_with(".mynotes") || to_norm.starts_with(".mynotes") {
        return Err(AppError::Other(
            "dir_move_with_refs: .mynotes/ is off-limits".into(),
        ));
    }
    // Prevent "move a dir into itself" — e.g. 1-notes → 1-notes/archive.
    // Plain `.starts_with(&from_norm)` is not enough (foo vs foo-bar). Require
    // a `/` boundary so foo/bar counts but foo-bar doesn't.
    if to_norm == from_norm || to_norm.starts_with(&format!("{from_norm}/")) {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: target '{to_norm}' is inside source '{from_norm}'"
        )));
    }

    let active = state.active_vault.lock().unwrap().clone();
    let vault = active.as_ref().ok_or(AppError::NoActiveVault)?.clone();

    let src_abs = resolve_in_vault(&active, &from_norm)?;
    if !src_abs.exists() {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: source does not exist: {from_norm}"
        )));
    }
    if !std::fs::metadata(&src_abs)?.is_dir() {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: source is not a directory: {from_norm}"
        )));
    }
    let dst_abs = resolve_in_vault(&active, &to_norm)?;
    if dst_abs.exists() {
        return Err(AppError::Other(format!(
            "dir_move_with_refs: destination already exists: {to_norm}"
        )));
    }

    // ---- 1. Walk source → list of (old_rel, new_rel, is_md). ----
    let mut files: Vec<FileMove> = Vec::new();
    walk_dir_all(&src_abs, &src_abs, &from_norm, &to_norm, &mut files)?;

    let handle = state.index_handle().ok_or_else(|| {
        AppError::Other("dir_move_with_refs: index not initialized; open a vault first".into())
    })?;

    // ---- 2. Build aggregated rewrite plan across every file in the tree. ----
    let plan = build_dir_plan(&files);

    // ---- 3. Query every referring row whose resolved target is inside the
    //         tree. LIKE-escape `%` / `_` in the prefix (unusual, but possible
    //         in vault paths if someone names a folder `100%done`). ----
    let like_prefix = like_escape(&from_norm);
    let referring: Vec<(String, String, String)> = {
        let conn = handle.lock().unwrap();
        query_referring_dir(&conn, &like_prefix)?
    };

    // Build { src_rel → [(raw_dst, link_type)] } grouped map.
    let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (src_note, dst_raw, lt) in referring {
        per_file.entry(src_note).or_default().push((dst_raw, lt));
    }

    let mut result = DirRenameResult {
        old_path: from_norm.clone(),
        new_path: to_norm.clone(),
        moved_files: files.len(),
        rewritten_files: Vec::new(),
        rewritten_links: 0,
        warnings: Vec::new(),
    };

    // ---- 4. Rewrite each referrer. If the referrer lives INSIDE the moved
    //      tree we rewrite it in place (at its pre-move rel) — then the
    //      directory rename carries the updated body along. ----
    let from_prefix = format!("{from_norm}/");
    for (ref_path, links) in &per_file {
        let abs = match resolve_in_vault(&active, ref_path) {
            Ok(p) => p,
            Err(e) => {
                result
                    .warnings
                    .push(format!("{ref_path}: resolve failed: {e}"));
                continue;
            }
        };
        let body = match std::fs::read_to_string(&abs) {
            Ok(b) => b,
            Err(e) => {
                result
                    .warnings
                    .push(format!("{ref_path}: read failed: {e}"));
                continue;
            }
        };
        let (new_body, hits) = plan.apply(&body, links);
        if hits == 0 {
            continue;
        }
        if let Err(e) = atomic_write(&abs, new_body.as_bytes()) {
            result
                .warnings
                .push(format!("{ref_path}: write failed: {e}"));
            continue;
        }
        // Report external referrers only — in-tree referrers will be indexed
        // via step 6 (delete_one + reindex_one on their new path). Reporting
        // them as `ref_path` would be misleading after the move.
        if !ref_path.starts_with(&from_prefix) {
            result.rewritten_files.push(ref_path.clone());
        }
        result.rewritten_links += hits;
    }

    // ---- 5. Move the directory. ----
    if let Some(parent) = dst_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Err(e) = std::fs::rename(&src_abs, &dst_abs) {
        tracing::warn!(
            from = %src_abs.display(),
            to = %dst_abs.display(),
            error = %e,
            "dir rename failed, falling back to recursive copy + remove"
        );
        copy_dir_recursive(&src_abs, &dst_abs)?;
        std::fs::remove_dir_all(&src_abs)?;
    }

    // ---- 6. Reindex: drop every old md rel, reindex every new md rel,
    //      plus reindex external referrers (in-tree referrers are already
    //      covered by the md-reindex loop). ----
    for fm in &files {
        if !fm.is_md {
            continue;
        }
        if let Err(e) = scanner::delete_one(&handle, &fm.old_rel) {
            tracing::warn!(rel = %fm.old_rel, error = %e, "dir rename: delete_one failed");
        }
        if let Err(e) = scanner::reindex_one(&handle, &vault, &fm.new_rel) {
            tracing::warn!(rel = %fm.new_rel, error = %e, "dir rename: reindex_one(new) failed");
        }
    }
    for rel in &result.rewritten_files {
        if let Err(e) = scanner::reindex_one(&handle, &vault, rel) {
            tracing::warn!(rel = %rel, error = %e, "dir rename: reindex_one(referrer) failed");
        }
    }

    tracing::info!(
        from = %from_norm,
        to = %to_norm,
        files = result.moved_files,
        rewritten = result.rewritten_files.len(),
        links = result.rewritten_links,
        warnings = result.warnings.len(),
        "dir_move_with_refs done"
    );

    Ok(result)
}

/// Normalize: strip trailing slashes, normalize path separators, trim.
fn normalize_dir(p: &str) -> String {
    let normalized = p.trim().replace('\\', "/");
    normalized.trim_end_matches('/').to_string()
}

/// Escape LIKE metacharacters so a path containing `%` or `_` matches
/// literally. Uses `\` as escape (see SQL `ESCAPE '\\'` clause).
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[derive(Debug, Clone)]
struct FileMove {
    old_rel: String,
    new_rel: String,
    is_md: bool,
}

fn summarize_moved_files(files: &[FileMove]) -> (usize, usize, usize, Vec<String>) {
    let moved_files_total = files.len();
    let moved_markdown_files = files.iter().filter(|fm| fm.is_md).count();
    let moved_other_files = moved_files_total.saturating_sub(moved_markdown_files);
    let (_, moved_files_preview) =
        preview_paths(files.iter().map(|fm| fm.new_rel.clone()).collect());
    (
        moved_files_total,
        moved_markdown_files,
        moved_other_files,
        moved_files_preview,
    )
}

/// Recursively walk `dir`, collecting `(old_rel, new_rel, is_md)` for every
/// file. Mirrors `services::scanner::walk_inner`'s skip rules (`.` prefixed
/// dirs are skipped — they're never user content). Non-md files ARE included
/// so embed refs to attachments inside the tree get rewritten too.
fn walk_dir_all(
    root: &std::path::Path,
    dir: &std::path::Path,
    from_rel: &str,
    to_rel: &str,
    out: &mut Vec<FileMove>,
) -> AppResult<()> {
    let iter = std::fs::read_dir(dir)?;
    for entry in iter {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk_dir_all(root, &path, from_rel, to_rel, out)?;
        } else if ft.is_file() {
            // Compute rel to the vault root is hard from here — we only have
            // `root` = src_abs. Compute rel-within-src, then splice it onto
            // from_rel / to_rel.
            let rel_inside = path
                .strip_prefix(root)
                .map_err(|_| AppError::Other(format!("strip_prefix {}", path.display())))?
                .to_string_lossy()
                .replace('\\', "/");
            let old_rel = format!("{from_rel}/{rel_inside}");
            let new_rel = format!("{to_rel}/{rel_inside}");
            let is_md = name_str.to_lowercase().ends_with(".md");
            out.push(FileMove {
                old_rel,
                new_rel,
                is_md,
            });
        }
    }
    Ok(())
}

/// Aggregate the wiki + embed pairs for every file in the tree into a single
/// `RewritePlan`. Md files contribute up to 3 wiki pairs + 1 embed pair each;
/// non-md files contribute 1 embed pair.
fn build_dir_plan(files: &[FileMove]) -> RewritePlan {
    let mut wiki_pairs: Vec<(String, String)> = Vec::new();
    let mut embed_pairs: Vec<(String, String)> = Vec::new();
    for fm in files {
        if fm.is_md {
            let per_file = RewritePlan::from_paths(&fm.old_rel, &fm.new_rel);
            for p in per_file.wiki_pairs {
                wiki_pairs.push(p);
            }
            for p in per_file.embed_pairs {
                embed_pairs.push(p);
            }
        } else if fm.old_rel != fm.new_rel {
            embed_pairs.push((fm.old_rel.clone(), fm.new_rel.clone()));
        }
    }
    RewritePlan {
        wiki_pairs,
        embed_pairs,
    }
}

/// Pull every link row whose `dst_resolved` is inside the moved tree. Uses
/// SQL `LIKE ? ESCAPE '\\'` so user-paths containing `%` / `_` literal match.
fn query_referring_dir(
    conn: &Connection,
    like_prefix_escaped: &str,
) -> AppResult<Vec<(String, String, String)>> {
    let pattern = format!("{like_prefix_escaped}/%");
    let mut stmt = conn
        .prepare(
            "SELECT src, dst, link_type FROM links
               WHERE dst_resolved LIKE ?1 ESCAPE '\\'",
        )
        .map_err(map_sql_err)?;
    let rows = stmt
        .query_map([pattern], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(map_sql_err)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_sql_err)?);
    }
    Ok(out)
}

/// Recursive `cp -R`. Only used as fallback when `fs::rename` fails (typically
/// cross-filesystem moves). Ignores .-prefixed entries for consistency with
/// walk_dir_all.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let src_child = entry.path();
        let dst_child = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&src_child, &dst_child)?;
        } else if ft.is_file() {
            std::fs::copy(&src_child, &dst_child)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use tempfile::TempDir;

    fn write_rel(root: &Path, rel: &str, body: &str) {
        let abs = root.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(abs, body).unwrap();
    }

    #[test]
    fn plan_stem_change_only() {
        let p = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");
        // Should include path-with-ext, path-no-ext, and stem.
        assert!(p
            .wiki_pairs
            .contains(&("1-notes/foo.md".into(), "1-notes/bar.md".into())));
        assert!(p
            .wiki_pairs
            .contains(&("1-notes/foo".into(), "1-notes/bar".into())));
        assert!(p.wiki_pairs.contains(&("foo".into(), "bar".into())));
        assert_eq!(p.embed_pairs.len(), 1);
    }

    #[test]
    fn plan_dir_change_only() {
        // Stem unchanged; we should NOT propose a `foo -> foo` pair.
        let p = RewritePlan::from_paths("0-inbox/foo.md", "1-notes/foo.md");
        assert!(p
            .wiki_pairs
            .contains(&("0-inbox/foo.md".into(), "1-notes/foo.md".into())));
        assert!(p
            .wiki_pairs
            .contains(&("0-inbox/foo".into(), "1-notes/foo".into())));
        assert!(!p.wiki_pairs.iter().any(|(o, _)| o == "foo"));
    }

    #[test]
    fn plan_noop_when_identical() {
        let p = RewritePlan::from_paths("1-notes/foo.md", "1-notes/foo.md");
        assert!(p.wiki_pairs.is_empty());
        assert!(p.embed_pairs.is_empty());
    }

    #[test]
    fn wiki_replace_basic() {
        let (out, n) = replace_wiki("See [[foo]] and [[foo|alias]].", "foo", "bar");
        assert_eq!(out, "See [[bar]] and [[bar|alias]].");
        assert_eq!(n, 2);
    }

    #[test]
    fn wiki_replace_preserves_unrelated() {
        let (out, n) = replace_wiki("[[foobar]] [[foo]]", "foo", "bar");
        assert_eq!(out, "[[foobar]] [[bar]]");
        assert_eq!(n, 1);
    }

    #[test]
    fn wiki_replace_path_form() {
        let (out, n) = replace_wiki("ref [[1-notes/foo]].", "1-notes/foo", "2-moc/foo");
        assert_eq!(out, "ref [[2-moc/foo]].");
        assert_eq!(n, 1);
    }

    #[test]
    fn embed_replace_basic() {
        let (out, n) = replace_embed(
            "before ![pic](attachments/2026/04/old.png) after",
            "attachments/2026/04/old.png",
            "attachments/2026/04/new.png",
        );
        assert_eq!(out, "before ![pic](attachments/2026/04/new.png) after");
        assert_eq!(n, 1);
    }

    #[test]
    fn embed_replace_preserves_alt() {
        let (out, n) = replace_embed(
            "![架构 discussion](attachments/a.png)",
            "attachments/a.png",
            "attachments/b.png",
        );
        assert_eq!(out, "![架构 discussion](attachments/b.png)");
        assert_eq!(n, 1);
    }

    #[test]
    fn plan_apply_respects_raw_form() {
        // Two referring links: one wrote `[[foo]]` (stem), one wrote
        // `[[Some Title]]` (title-form). Only the stem one should rewrite.
        let p = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");
        let body = "See [[foo]] and [[Some Title]].";
        let links = vec![
            ("foo".into(), "wiki".into()),
            ("Some Title".into(), "wiki".into()),
        ];
        let (out, hits) = p.apply(body, &links);
        assert_eq!(out, "See [[bar]] and [[Some Title]].");
        assert_eq!(hits, 1);
    }

    #[test]
    fn plan_apply_skips_when_indexer_has_no_record() {
        // Body contains `[[foo]]` but the indexer row list doesn't mention
        // `foo` — don't rewrite. This protects against the rare case where a
        // stale index says one thing and the current file says another.
        let p = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");
        let body = "Standalone [[foo]] with no index row.";
        let links: Vec<(String, String)> = vec![];
        let (out, hits) = p.apply(body, &links);
        assert_eq!(out, body);
        assert_eq!(hits, 0);
    }

    #[test]
    fn plan_apply_embed() {
        let p =
            RewritePlan::from_paths("attachments/2026/04/foo.png", "attachments/2026/04/bar.png");
        let body = "![](attachments/2026/04/foo.png)";
        let links = vec![("attachments/2026/04/foo.png".into(), "embed".into())];
        let (out, hits) = p.apply(body, &links);
        assert_eq!(out, "![](attachments/2026/04/bar.png)");
        assert_eq!(hits, 1);
    }

    #[test]
    fn file_preview_no_referrers() {
        let tmp = TempDir::new().unwrap();
        let active = Some(tmp.path().to_path_buf());
        let per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        let plan = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");

        let summary = summarize_preview_rewrites(&active, &per_file, &plan, None);

        assert_eq!(summary.rewritten_files_total, 0);
        assert!(summary.rewritten_files_preview.is_empty());
        assert_eq!(summary.rewritten_links, 0);
    }

    #[test]
    fn file_preview_counts_multiple_referrers() {
        let tmp = TempDir::new().unwrap();
        let active = Some(tmp.path().to_path_buf());
        write_rel(tmp.path(), "1-notes/ref-a.md", "See [[foo]].");
        write_rel(
            tmp.path(),
            "2-moc/ref-b.md",
            "[[1-notes/foo]] and [[1-notes/foo|alias]].",
        );

        let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        per_file.insert(
            "1-notes/ref-a.md".into(),
            vec![("foo".into(), "wiki".into())],
        );
        per_file.insert(
            "2-moc/ref-b.md".into(),
            vec![("1-notes/foo".into(), "wiki".into())],
        );
        let plan = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");

        let summary = summarize_preview_rewrites(&active, &per_file, &plan, None);

        assert_eq!(summary.rewritten_files_total, 2);
        assert_eq!(
            summary.rewritten_files_preview,
            vec!["1-notes/ref-a.md".to_string(), "2-moc/ref-b.md".to_string()]
        );
        assert_eq!(summary.rewritten_links, 3);
    }

    #[test]
    fn file_preview_skips_title_form_refs() {
        let tmp = TempDir::new().unwrap();
        let active = Some(tmp.path().to_path_buf());
        write_rel(tmp.path(), "1-notes/ref.md", "See [[Some Title]].");

        let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        per_file.insert(
            "1-notes/ref.md".into(),
            vec![("Some Title".into(), "wiki".into())],
        );
        let plan = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");

        let summary = summarize_preview_rewrites(&active, &per_file, &plan, None);

        assert_eq!(summary.rewritten_files_total, 0);
        assert!(summary.rewritten_files_preview.is_empty());
        assert_eq!(summary.rewritten_links, 0);
    }

    #[test]
    fn file_preview_list_truncates_to_limit() {
        let tmp = TempDir::new().unwrap();
        let active = Some(tmp.path().to_path_buf());
        let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for i in 0..(PREVIEW_LIMIT + 5) {
            let rel = format!("1-notes/ref-{i:03}.md");
            write_rel(tmp.path(), &rel, "[[foo]]");
            per_file.insert(rel, vec![("foo".into(), "wiki".into())]);
        }
        let plan = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");

        let summary = summarize_preview_rewrites(&active, &per_file, &plan, None);

        assert_eq!(summary.rewritten_files_total, PREVIEW_LIMIT + 5);
        assert_eq!(summary.rewritten_files_preview.len(), PREVIEW_LIMIT);
        assert_eq!(summary.rewritten_files_preview[0], "1-notes/ref-000.md");
        assert_eq!(
            summary.rewritten_files_preview[PREVIEW_LIMIT - 1],
            format!("1-notes/ref-{:03}.md", PREVIEW_LIMIT - 1)
        );
        assert_eq!(summary.rewritten_links, PREVIEW_LIMIT + 5);
    }

    #[test]
    fn preview_summary_does_not_mutate_referrers() {
        let tmp = TempDir::new().unwrap();
        let active = Some(tmp.path().to_path_buf());
        let rel = "1-notes/ref.md";
        let original = "See [[foo]].";
        write_rel(tmp.path(), rel, original);

        let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        per_file.insert(rel.into(), vec![("foo".into(), "wiki".into())]);
        let plan = RewritePlan::from_paths("1-notes/foo.md", "1-notes/bar.md");

        let summary = summarize_preview_rewrites(&active, &per_file, &plan, None);

        assert_eq!(summary.rewritten_files_total, 1);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join(rel)).unwrap(),
            original
        );
    }

    // -----------------------------------------------------------------
    // dir_move_with_refs helpers
    // -----------------------------------------------------------------

    #[test]
    fn normalize_dir_strips_trailing_slashes_and_normalizes_separators() {
        assert_eq!(normalize_dir("1-notes/"), "1-notes");
        assert_eq!(normalize_dir("1-notes///"), "1-notes");
        assert_eq!(normalize_dir("  1-notes/sub/  "), "1-notes/sub");
        // Backslash on Windows-ish input gets normalized to forward.
        assert_eq!(normalize_dir("a\\b\\c"), "a/b/c");
        assert_eq!(normalize_dir(""), "");
    }

    #[test]
    fn like_escape_handles_percent_underscore_and_backslash() {
        // Plain string: no-op.
        assert_eq!(like_escape("1-notes/foo"), "1-notes/foo");
        // `%` and `_` are LIKE wildcards → escaped with `\`.
        assert_eq!(like_escape("100%done"), "100\\%done");
        assert_eq!(like_escape("foo_bar"), "foo\\_bar");
        // Existing backslashes doubled first, so escape metachar stays un-
        // ambiguous.
        assert_eq!(like_escape("a\\b"), "a\\\\b");
        assert_eq!(like_escape("weird_%dir"), "weird\\_\\%dir");
    }

    #[test]
    fn build_dir_plan_aggregates_md_and_non_md_entries() {
        // Two md notes + one attachment moving from `old-dir` → `new-dir`.
        let files = vec![
            FileMove {
                old_rel: "old-dir/note-a.md".into(),
                new_rel: "new-dir/note-a.md".into(),
                is_md: true,
            },
            FileMove {
                old_rel: "old-dir/sub/note-b.md".into(),
                new_rel: "new-dir/sub/note-b.md".into(),
                is_md: true,
            },
            FileMove {
                old_rel: "old-dir/pic.png".into(),
                new_rel: "new-dir/pic.png".into(),
                is_md: false,
            },
        ];
        let plan = build_dir_plan(&files);

        // Per md file: path-with-ext + path-without-ext wiki pairs. Stem
        // pairs (`note-a` → `note-a`) are filtered out because stem didn't
        // change. So 2 md files × 2 dir-form pairs = 4 wiki pairs.
        assert!(plan
            .wiki_pairs
            .contains(&("old-dir/note-a.md".into(), "new-dir/note-a.md".into())));
        assert!(plan
            .wiki_pairs
            .contains(&("old-dir/note-a".into(), "new-dir/note-a".into())));
        assert!(plan.wiki_pairs.contains(&(
            "old-dir/sub/note-b.md".into(),
            "new-dir/sub/note-b.md".into()
        )));
        assert!(plan
            .wiki_pairs
            .contains(&("old-dir/sub/note-b".into(), "new-dir/sub/note-b".into())));
        // Stems unchanged — no bare-stem pair should leak through.
        assert!(!plan
            .wiki_pairs
            .iter()
            .any(|(o, _)| o == "note-a" || o == "note-b"));

        // Embed pairs: md files contribute 1 each (path-with-ext), attach-
        // ment contributes 1 → 3 total.
        assert!(plan
            .embed_pairs
            .contains(&("old-dir/note-a.md".into(), "new-dir/note-a.md".into())));
        assert!(plan.embed_pairs.contains(&(
            "old-dir/sub/note-b.md".into(),
            "new-dir/sub/note-b.md".into()
        )));
        assert!(plan
            .embed_pairs
            .contains(&("old-dir/pic.png".into(), "new-dir/pic.png".into())));
        assert_eq!(plan.embed_pairs.len(), 3);
    }

    #[test]
    fn summarize_moved_files_counts_markdown_and_other() {
        let files = vec![
            FileMove {
                old_rel: "old/z.png".into(),
                new_rel: "new/z.png".into(),
                is_md: false,
            },
            FileMove {
                old_rel: "old/a.md".into(),
                new_rel: "new/a.md".into(),
                is_md: true,
            },
            FileMove {
                old_rel: "old/sub/b.md".into(),
                new_rel: "new/sub/b.md".into(),
                is_md: true,
            },
        ];

        let (total, markdown, other, preview) = summarize_moved_files(&files);

        assert_eq!(total, 3);
        assert_eq!(markdown, 2);
        assert_eq!(other, 1);
        assert_eq!(
            preview,
            vec![
                "new/a.md".to_string(),
                "new/sub/b.md".to_string(),
                "new/z.png".to_string()
            ]
        );
    }

    #[test]
    fn dir_preview_reports_external_referrers_only() {
        let tmp = TempDir::new().unwrap();
        let active = Some(tmp.path().to_path_buf());
        write_rel(tmp.path(), "notes/ref-out.md", "[[old/a]]");
        write_rel(tmp.path(), "old/inner.md", "[[old/b]]");

        let files = vec![
            FileMove {
                old_rel: "old/a.md".into(),
                new_rel: "new/a.md".into(),
                is_md: true,
            },
            FileMove {
                old_rel: "old/b.md".into(),
                new_rel: "new/b.md".into(),
                is_md: true,
            },
        ];
        let plan = build_dir_plan(&files);
        let mut per_file: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        per_file.insert(
            "notes/ref-out.md".into(),
            vec![("old/a".into(), "wiki".into())],
        );
        per_file.insert("old/inner.md".into(), vec![("old/b".into(), "wiki".into())]);

        let summary = summarize_preview_rewrites(&active, &per_file, &plan, Some("old/"));

        assert_eq!(summary.rewritten_files_total, 1);
        assert_eq!(
            summary.rewritten_files_preview,
            vec!["notes/ref-out.md".to_string()]
        );
        assert_eq!(summary.rewritten_links, 2);
    }

    #[test]
    fn dir_preview_list_truncates_to_limit() {
        let files: Vec<FileMove> = (0..(PREVIEW_LIMIT + 8))
            .map(|i| FileMove {
                old_rel: format!("old/ref-{i:03}.md"),
                new_rel: format!("new/ref-{i:03}.md"),
                is_md: true,
            })
            .collect();

        let (total, markdown, other, preview) = summarize_moved_files(&files);

        assert_eq!(total, PREVIEW_LIMIT + 8);
        assert_eq!(markdown, PREVIEW_LIMIT + 8);
        assert_eq!(other, 0);
        assert_eq!(preview.len(), PREVIEW_LIMIT);
        assert_eq!(preview[0], "new/ref-000.md");
        assert_eq!(
            preview[PREVIEW_LIMIT - 1],
            format!("new/ref-{:03}.md", PREVIEW_LIMIT - 1)
        );
    }

    #[test]
    fn build_dir_plan_noop_when_no_move() {
        // Degenerate case: a FileMove whose old == new. Shouldn't contribute
        // any rewrite pairs (protects against accidental self-rewrites).
        let files = vec![FileMove {
            old_rel: "dir/foo.png".into(),
            new_rel: "dir/foo.png".into(),
            is_md: false,
        }];
        let plan = build_dir_plan(&files);
        assert!(plan.wiki_pairs.is_empty());
        assert!(plan.embed_pairs.is_empty());
    }

    #[test]
    fn build_dir_plan_applies_across_tree_on_external_referrer() {
        // Simulate a referrer *outside* the moved tree whose body points at
        // two md notes inside. A single aggregate plan should rewrite both
        // in one pass.
        let files = vec![
            FileMove {
                old_rel: "old/a.md".into(),
                new_rel: "new/a.md".into(),
                is_md: true,
            },
            FileMove {
                old_rel: "old/b.md".into(),
                new_rel: "new/b.md".into(),
                is_md: true,
            },
        ];
        let plan = build_dir_plan(&files);
        let body = "Linking [[old/a]] and [[old/b|b-alias]].";
        let links = vec![
            ("old/a".into(), "wiki".into()),
            ("old/b".into(), "wiki".into()),
        ];
        let (out, hits) = plan.apply(body, &links);
        assert_eq!(out, "Linking [[new/a]] and [[new/b|b-alias]].");
        assert_eq!(hits, 2);
    }

    #[test]
    fn dir_self_nesting_prefix_check_distinguishes_boundary() {
        // Helper for the prefix-based self-nesting guard used in
        // dir_move_with_refs. This test pins the semantics so a future
        // refactor can't regress it into `starts_with(&from_norm)`.
        fn is_self_nesting(from: &str, to: &str) -> bool {
            to == from || to.starts_with(&format!("{from}/"))
        }
        // Same path → nesting (caller also short-circuits with "no-op" but
        // the check here errs on the side of rejection).
        assert!(is_self_nesting("foo", "foo"));
        // Target inside source → rejected.
        assert!(is_self_nesting("foo", "foo/bar"));
        assert!(is_self_nesting("1-notes", "1-notes/archive"));
        // Sibling with same prefix → allowed.
        assert!(!is_self_nesting("foo", "foo-bar"));
        assert!(!is_self_nesting("1-notes", "1-notes-old"));
        // Totally unrelated → allowed.
        assert!(!is_self_nesting("foo", "bar"));
        assert!(!is_self_nesting("1-notes", "notes"));
    }
}
