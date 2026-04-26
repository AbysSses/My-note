//! Project-scope commands.
//!
//! Per design_V2.md §0.4 / §6.11: projects are identified solely by path
//! (`4-projects/{slug}/index.md`) — the slug is never stored in frontmatter.
//! The one piece of project metadata we *do* keep in frontmatter is
//! `status` (`active` / `paused` / `done` / `archived`), because it is
//! user-set intent that can't be derived from the path.
//!
//! `project_set_status` is the single entry point for changing that field.
//! It reads the index.md, rewrites the `status:` frontmatter line in place,
//! atomic-writes, and synchronously reindexes so the caller never sees a
//! window where Home / the Projects sidebar shows the old bucket.

use tauri::State;

use crate::commands::file::{atomic_write, resolve_write_target_in_vault};
use crate::commands::vault::resolve_in_vault;
use crate::error::{AppError, AppResult};
use crate::services::scanner;
use crate::AppState;

/// Set a project's `status` frontmatter field.
///
/// Parameters:
/// - `slug`  — vault-relative project slug (`"deep-work"`, not a path).
/// - `status` — any string; the caller (palette) enforces the canonical
///              set (`active/paused/done/archived`), but we don't reject
///              unknown values — md is SSOT.
///
/// Errors:
/// - `AppError::Other("invalid project slug: …")` when slug is empty or
///   contains path separators.
/// - `AppError::Other("project does not exist: …")` when the index.md is
///   missing. We deliberately don't auto-create it — that's the job of
///   `> New Project…`.
/// - Propagates IO / index errors.
#[tauri::command]
pub fn project_set_status(slug: String, status: String, state: State<AppState>) -> AppResult<()> {
    if slug.is_empty() || slug.contains('/') || slug.contains('\\') || slug == "." || slug == ".." {
        return Err(AppError::Other(format!("invalid project slug: {slug}")));
    }
    let rel_path = format!("4-projects/{slug}/index.md");

    let active = state.active_vault.lock().unwrap().clone();
    let vault = active.as_ref().ok_or(AppError::NoActiveVault)?.clone();

    // Must exist — we do not create projects here.
    let abs = resolve_in_vault(&active, &rel_path)?;
    if !abs.exists() {
        return Err(AppError::Other(format!(
            "project does not exist: {rel_path}"
        )));
    }

    let original = std::fs::read_to_string(&abs)?;
    let updated = rewrite_frontmatter_status(&original, &status);

    // Nothing to write if the file already had exactly this value.
    if updated == original {
        return Ok(());
    }

    // `resolve_write_target_in_vault` handles path validation the same way
    // file_write does — we don't want project_set_status to become a
    // back-door for writing outside the vault.
    let target = resolve_write_target_in_vault(&active, &rel_path)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&target, updated.as_bytes())?;

    // Synchronous reindex so Home/Projects panels reflect the new bucket
    // without the 200ms watcher lag (same pattern as file_move).
    if let Some(handle) = state.index_handle() {
        if let Err(e) = scanner::reindex_one(&handle, &vault, &rel_path) {
            tracing::warn!(
                rel = %rel_path,
                error = %e,
                "project_set_status: reindex_one failed (watcher will retry)",
            );
        }
    }

    Ok(())
}

/// Rewrite (or insert) the `status:` frontmatter line on a markdown body.
///
/// Mirrors the behavior of `src/lib/commands.ts::rewriteFrontmatter` for the
/// single-key `status` case: line-based, preserves unrelated YAML formatting,
/// appends the key before the closing `---` if missing, prepends a new
/// block if no frontmatter is present.
///
/// Only scalar values — list / block values are not represented by this
/// command. `status` is always a short enum-like scalar.
fn rewrite_frontmatter_status(body: &str, value: &str) -> String {
    let formatted = format_yaml_scalar(value);

    // Match a leading `---\n...\n---\n` (or \r\n variants). We only touch
    // leading frontmatter — mid-document `---` is left alone.
    let Some((fm_raw, rest)) = split_leading_frontmatter(body) else {
        // No frontmatter: prepend a minimal one with just `status:`.
        let head = format!("---\nstatus: {formatted}\n---\n\n");
        return format!("{head}{}", body.trim_start());
    };

    let mut out_lines: Vec<String> = Vec::new();
    let mut seen = false;
    // Split on \n — we drop any trailing \r below. This preserves blank lines
    // and comments (which YAML allows) between keys.
    for line in fm_raw.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !seen && is_status_line(line) {
            out_lines.push(format!("status: {formatted}"));
            seen = true;
        } else {
            out_lines.push(line.to_string());
        }
    }
    if !seen {
        out_lines.push(format!("status: {formatted}"));
    }

    let joined = out_lines.join("\n");
    // Normalize: single \n after the closing `---`, then the body, stripped
    // of any leading \r\n so we don't double the separator.
    let trimmed_rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .unwrap_or(rest);
    format!("---\n{joined}\n---\n\n{trimmed_rest}")
}

/// Return (frontmatter_raw_without_fences_or_trailing_newline,
///         rest_of_body_starting_after_closing_fence).
/// `None` if the document doesn't begin with a `---` fence.
fn split_leading_frontmatter(body: &str) -> Option<(&str, &str)> {
    let rest = body
        .strip_prefix("---\n")
        .or_else(|| body.strip_prefix("---\r\n"))?;
    // Scan for a line that is exactly `---` (optionally \r-terminated).
    let mut cursor = 0;
    while cursor < rest.len() {
        let next_nl = rest[cursor..]
            .find('\n')
            .map(|i| cursor + i)
            .unwrap_or(rest.len());
        let line = rest[cursor..next_nl].trim_end_matches('\r');
        if line == "---" {
            // `cursor` is the start of the closing fence line; the FM chunk
            // is everything before it, minus the trailing newline that
            // separates it from the closing fence.
            let fm = rest[..cursor].strip_suffix('\n').unwrap_or(&rest[..cursor]);
            let fm = fm.strip_suffix('\r').unwrap_or(fm);
            let after_start = (next_nl + 1).min(rest.len());
            return Some((fm, &rest[after_start..]));
        }
        cursor = next_nl + 1;
    }
    None
}

fn is_status_line(line: &str) -> bool {
    // Matches `status:` at the start, tolerating leading spaces and
    // whatever follows the colon.
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("status") {
        let rest = rest.trim_start();
        return rest.starts_with(':');
    }
    false
}

/// Quote a scalar if it contains YAML-significant chars; otherwise leave
/// bare. Matches the `formatYamlScalar` helper in `commands.ts`.
fn format_yaml_scalar(value: &str) -> String {
    let needs_quote = value.is_empty()
        || value.starts_with(|c: char| c == ' ' || c == '\t' || c == '-')
        || value
            .chars()
            .any(|c| matches!(c, ':' | '#' | '"' | '[' | ']' | '{' | '}'));
    if needs_quote {
        let escaped = value.replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_existing_status() {
        let src = "---\ntitle: Deep Work\nstatus: active\ntype: project\n---\n\n# body\n";
        let out = rewrite_frontmatter_status(src, "paused");
        assert!(out.contains("status: paused\n"));
        assert!(!out.contains("status: active"));
        assert!(out.contains("title: Deep Work"));
        assert!(out.contains("type: project"));
        assert!(out.ends_with("# body\n"));
    }

    #[test]
    fn appends_when_missing_from_block() {
        let src = "---\ntitle: X\n---\n\nhi\n";
        let out = rewrite_frontmatter_status(src, "active");
        assert!(out.contains("title: X"));
        assert!(out.contains("status: active"));
        // Status should be appended inside the block, not outside.
        let between = out.split("---").nth(1).unwrap();
        assert!(between.contains("status: active"));
    }

    #[test]
    fn prepends_when_no_frontmatter() {
        let src = "# already written\n\nbody\n";
        let out = rewrite_frontmatter_status(src, "done");
        assert!(out.starts_with("---\nstatus: done\n---\n\n"));
        assert!(out.contains("# already written"));
    }

    #[test]
    fn quotes_values_with_yaml_specials() {
        assert_eq!(format_yaml_scalar("active"), "active");
        assert_eq!(format_yaml_scalar("on hold: maybe"), "\"on hold: maybe\"");
        assert_eq!(format_yaml_scalar(""), "\"\"");
    }

    #[test]
    fn noop_when_value_unchanged() {
        let src = "---\nstatus: active\n---\n\nbody\n";
        let out = rewrite_frontmatter_status(src, "active");
        // Either byte-identical or semantically identical — our check is that
        // the status line still exists and there's no duplication.
        let count = out.matches("status: active").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn tolerates_crlf_line_endings() {
        let src = "---\r\ntitle: X\r\nstatus: active\r\n---\r\nbody\r\n";
        let out = rewrite_frontmatter_status(src, "paused");
        assert!(out.contains("status: paused\n"));
        assert!(!out.contains("status: active"));
    }
}
