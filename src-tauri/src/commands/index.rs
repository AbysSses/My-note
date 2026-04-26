//! IPC commands that expose the SQLite index to the frontend.
//!
//! Every command locks the per-vault `Arc<Mutex<Connection>>` just for the
//! duration of a single query — we don't hold it across awaits, and we
//! clone the `Arc` out of AppState first so IPC threads never contend on
//! the outer state mutex.

use std::collections::HashSet;

use rusqlite::{params, params_from_iter, Connection};
use serde::Serialize;
use tauri::State;

use crate::db::map_sql_err;
use crate::error::{AppError, AppResult};
use crate::services::scanner;
use crate::AppState;

use super::file::{atomic_write, resolve_write_target_in_vault};
use super::vault::resolve_in_vault;

#[derive(Debug, Serialize)]
pub struct NoteRef {
    pub path: String,
    pub title: Option<String>,
    pub updated: Option<String>,
    pub note_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BacklinkItem {
    pub src_path: String,
    pub src_title: Option<String>,
    pub link_text: String,
}

#[derive(Debug, Serialize)]
pub struct OutgoingLink {
    pub dst: String,
    pub dst_resolved: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub title: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct TaskRow {
    pub id: i64,
    pub note_path: String,
    pub note_title: Option<String>,
    pub line: i64,
    pub text: String,
    pub done: bool,
    pub due: Option<String>,
    pub priority: Option<String>,
}

fn with_conn<F, R>(state: &State<AppState>, f: F) -> AppResult<R>
where
    F: FnOnce(&Connection) -> AppResult<R>,
{
    let handle = state.index_handle().ok_or(AppError::NoActiveVault)?;
    let conn = handle.lock().unwrap();
    f(&conn)
}

#[tauri::command]
pub fn index_backlinks(rel_path: String, state: State<AppState>) -> AppResult<Vec<BacklinkItem>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT l.src, n.title, l.dst
                 FROM links l
                 LEFT JOIN notes n ON n.path = l.src
                 WHERE l.dst_resolved = ?1
                 ORDER BY COALESCE(n.updated, '') DESC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![rel_path], |row| {
                Ok(BacklinkItem {
                    src_path: row.get(0)?,
                    src_title: row.get(1)?,
                    link_text: row.get(2)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

#[tauri::command]
pub fn index_outgoing(rel_path: String, state: State<AppState>) -> AppResult<Vec<OutgoingLink>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT l.dst, l.dst_resolved, n.title
                 FROM links l
                 LEFT JOIN notes n ON n.path = l.dst_resolved
                 WHERE l.src = ?1
                 ORDER BY l.position",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![rel_path], |row| {
                Ok(OutgoingLink {
                    dst: row.get(0)?,
                    dst_resolved: row.get(1)?,
                    title: row.get(2)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

#[tauri::command]
pub fn index_unresolved(rel_path: String, state: State<AppState>) -> AppResult<Vec<String>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT dst FROM links
                 WHERE src = ?1 AND dst_resolved IS NULL
                 ORDER BY dst",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![rel_path], |row| row.get::<_, String>(0))
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

#[tauri::command]
pub fn index_tags(state: State<AppState>) -> AppResult<Vec<TagCount>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT tag, COUNT(*) as c FROM tags
                 GROUP BY tag
                 ORDER BY c DESC, tag ASC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TagCount {
                    tag: row.get(0)?,
                    count: row.get(1)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

#[tauri::command]
pub fn index_notes_by_tag(tag: String, state: State<AppState>) -> AppResult<Vec<NoteRef>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT n.path, n.title, n.updated, n.type
                 FROM tags t
                 JOIN notes n ON n.path = t.note_path
                 WHERE t.tag = ?1
                 ORDER BY COALESCE(n.updated, '') DESC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![tag], |row| {
                Ok(NoteRef {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    updated: row.get(2)?,
                    note_type: row.get(3)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// Notes carrying one or more tags.
///
/// - `match_all = false`: set-union (notes carrying *any* selected tag).
/// - `match_all = true`: set-intersection (notes carrying *every* selected tag).
///
/// Ordering intentionally stays "recent first" — the frontend may apply an
/// extra presentation sort, but the query result itself should already be
/// useful when consumed raw.
#[tauri::command]
pub fn index_notes_by_tags(
    tags: Vec<String>,
    match_all: bool,
    state: State<AppState>,
) -> AppResult<Vec<NoteRef>> {
    let tags = normalize_tag_filters(tags);
    if tags.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; tags.len()].join(", ");
    let having = if match_all {
        format!("HAVING COUNT(DISTINCT t.tag) = {}", tags.len())
    } else {
        String::new()
    };
    let sql = format!(
        "SELECT n.path, n.title, n.updated, n.type
         FROM tags t
         JOIN notes n ON n.path = t.note_path
         WHERE t.tag IN ({placeholders})
         GROUP BY n.path, n.title, n.updated, n.type
         {having}
         ORDER BY COALESCE(n.updated, '') DESC, n.path ASC"
    );

    with_conn(&state, |conn| {
        let mut stmt = conn.prepare(&sql).map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params_from_iter(tags.iter()), |row| {
                Ok(NoteRef {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    updated: row.get(2)?,
                    note_type: row.get(3)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// All notes, ordered by updated desc. Used by wiki-link autocomplete and
/// command-palette file fuzzy-find. Cheap enough even for 10k notes because
/// we only return lightweight NoteRef rows.
#[tauri::command]
pub fn index_all_notes(state: State<AppState>) -> AppResult<Vec<NoteRef>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT path, title, updated, type FROM notes
                 ORDER BY COALESCE(updated, '') DESC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(NoteRef {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    updated: row.get(2)?,
                    note_type: row.get(3)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// All notes sitting in `0-inbox/`, newest first. Used by the Inbox Review view.
/// Reads straight from `notes` — no extra table needed. Size is bounded by how
/// many captures the user has queued, so no LIMIT necessary.
#[tauri::command]
pub fn index_inbox_list(state: State<AppState>) -> AppResult<Vec<NoteRef>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT path, title, updated, type FROM notes
                 WHERE path LIKE '0-inbox/%'
                 ORDER BY COALESCE(mtime, 0) DESC, path ASC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(NoteRef {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    updated: row.get(2)?,
                    note_type: row.get(3)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// Projects bucketed by `status` frontmatter field.
///
/// Query shape: `4-projects/{slug}/index.md` rows, filtered by
/// `LOWER(TRIM(status)) = LOWER(TRIM(?1))` when a status is supplied.
/// `None` returns every project regardless of status.
///
/// Tolerance by design (see design_V2.md §0.1 / Week 5 Task 1 gap note):
/// status values are lower-/trim-compared, so a user-typed `Active` or
/// trailing whitespace still sorts into the right bucket. Typoed values
/// (`in-progress`) don't match any canonical bucket, which is intended —
/// md is SSOT, the user fixes the typo in-place.
#[tauri::command]
pub fn index_projects_by_status(
    status: Option<String>,
    state: State<AppState>,
) -> AppResult<Vec<NoteRef>> {
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT path, title, updated, type FROM notes
                 WHERE path LIKE '4-projects/%/index.md'
                   AND (?1 IS NULL
                        OR LOWER(TRIM(COALESCE(status, ''))) = LOWER(TRIM(?1)))
                 ORDER BY COALESCE(updated, '') DESC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![status], |row| {
                Ok(NoteRef {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    updated: row.get(2)?,
                    note_type: row.get(3)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// All notes that live inside `4-projects/<slug>/` **except** the project's
/// own `index.md`. Populates the "项目笔记" section of the right-hand Panel
/// when the user is viewing a project's index.md. Path-based SSOT per V2 —
/// we don't consult frontmatter `project_slug` (deprecated field) at all.
///
/// Slug is passed verbatim; callers are expected to derive it from a path
/// that the index already produced (so it's ASCII / URL-safe and doesn't
/// need LIKE escaping). Belt-and-suspenders we still use `substr` equality
/// on the prefix rather than LIKE so future slugs containing `%` or `_`
/// don't misbehave if the sanitization ever relaxes.
///
/// Ordering: `updated DESC` so the freshest project-note is at the top —
/// matches the vibe of "what did I touch last" when glancing at a project.
#[tauri::command]
pub fn index_project_notes(slug: String, state: State<AppState>) -> AppResult<Vec<NoteRef>> {
    // Empty slug → empty list rather than a runaway prefix match on "4-projects/".
    // (Would otherwise return every project-note in the vault, which is not
    // what any caller wants.)
    if slug.trim().is_empty() {
        return Ok(Vec::new());
    }
    let prefix = format!("4-projects/{}/", slug);
    let index_md = format!("{}index.md", prefix);
    let prefix_len = prefix.len() as i64;

    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT path, title, updated, type FROM notes
                 WHERE substr(path, 1, ?1) = ?2
                   AND path != ?3
                 ORDER BY COALESCE(updated, '') DESC",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![prefix_len, prefix, index_md], |row| {
                Ok(NoteRef {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    updated: row.get(2)?,
                    note_type: row.get(3)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// Total count of distinct unresolved wiki-link targets across the vault.
/// Shown on the Home view so the user sees at a glance how many dangling
/// references the knowledge graph carries.
///
/// We count DISTINCT dst so a broken link repeated in 10 notes doesn't
/// inflate the number. Cheap: just one COUNT over `links`.
#[tauri::command]
pub fn index_unresolved_count(state: State<AppState>) -> AppResult<i64> {
    with_conn(&state, |conn| {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT dst) FROM links WHERE dst_resolved IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(map_sql_err)?;
        Ok(n)
    })
}

/// FTS5 search. `query` is forwarded to `MATCH` — callers should escape
/// special chars if they want plain-string behavior (quote the term).
#[tauri::command]
pub fn index_search(
    query: String,
    limit: Option<i64>,
    state: State<AppState>,
) -> AppResult<Vec<SearchHit>> {
    let limit = limit.unwrap_or(50).clamp(1, 500);
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT f.path, f.title,
                        snippet(notes_fts, 2, '<mark>', '</mark>', '…', 12) as snip
                 FROM notes_fts f
                 WHERE notes_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![fts_sanitize(&query), limit], |row| {
                Ok(SearchHit {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get(2)?,
                })
            })
            .map_err(map_sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_sql_err)?);
        }
        Ok(out)
    })
}

/// Wrap the user's query in a single quoted phrase so FTS5 treats `+`, `-`,
/// `"`, etc. as literal characters rather than operators. For power-user
/// queries we can relax this later via an opt-in flag.
pub(crate) fn fts_sanitize(q: &str) -> String {
    let escaped = q.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

/// Resolve a `[[wiki-link]]` target (title text or filename stem) to a
/// concrete vault-relative path, if one exists. Mirrors the two-pass
/// precedence of [`crate::db::indexer::resolve_links`] for consistency:
///
/// 1. exact frontmatter-title match (`notes.title = target`);
/// 2. filename stem match (strip dir + `.md`).
///
/// Returns `None` when the link is unresolved — the frontend then
/// renders the chip as plain text rather than a clickable link. Used
/// by `ChatPanel` to turn `[[Some Note]]` in AI replies into a
/// clickable span that calls `onOpenNote` (D2b.5).
#[tauri::command]
pub fn index_resolve_wiki_link(target: String, state: State<AppState>) -> AppResult<Option<NoteRef>> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let t = trimmed.to_string();
    with_conn(&state, |conn| {
        if let Some(r) = query_first_note(conn, "notes.title = ?1", &[&t])? {
            return Ok(Some(r));
        }
        // Filename-stem match: same LIKE shape as the indexer.
        let like1 = format!("%/{t}.md");
        let eq1 = format!("{t}.md");
        if let Some(r) =
            query_first_note(conn, "notes.path LIKE ?1 OR notes.path = ?2", &[&like1, &eq1])?
        {
            return Ok(Some(r));
        }
        Ok(None)
    })
}

/// Tiny helper for the two-pass wiki-link resolver. Kept file-local
/// because the other index queries are structured enough that they
/// don't share this shape.
fn query_first_note(
    conn: &Connection,
    where_clause: &str,
    params: &[&dyn rusqlite::ToSql],
) -> AppResult<Option<NoteRef>> {
    let sql = format!(
        "SELECT path, title, updated, type FROM notes WHERE {where_clause} LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql).map_err(map_sql_err)?;
    let mut rows = stmt.query(params).map_err(map_sql_err)?;
    if let Some(row) = rows.next().map_err(map_sql_err)? {
        Ok(Some(NoteRef {
            path: row.get(0).map_err(map_sql_err)?,
            title: row.get(1).map_err(map_sql_err)?,
            updated: row.get(2).map_err(map_sql_err)?,
            note_type: row.get(3).map_err(map_sql_err)?,
        }))
    } else {
        Ok(None)
    }
}

/// Open tasks whose `due` equals `today` OR that live inside a daily note
/// named `today` (`3-journal/YYYY-MM-DD.md` — per indexer's convention).
///
/// Sorted urgent → low, then by due ascending. Hard-capped at 50 so a
/// runaway daily note can't flood the UI. `today` is `YYYY-MM-DD`.
#[tauri::command]
pub fn index_tasks_today(today: String, state: State<AppState>) -> AppResult<Vec<TaskRow>> {
    let daily_like = format!("3-journal/{today}.md");
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.note_path, n.title, t.line, t.text, t.done, t.due, t.priority
                 FROM tasks t
                 LEFT JOIN notes n ON n.path = t.note_path
                 WHERE t.done = 0
                   AND (t.due = ?1 OR t.note_path = ?2)
                 ORDER BY
                   CASE t.priority
                     WHEN 'urgent' THEN 0
                     WHEN 'high' THEN 1
                     WHEN 'med' THEN 2
                     WHEN 'low' THEN 3
                     ELSE 4
                   END,
                   COALESCE(t.due, '9999-99-99') ASC,
                   t.note_path, t.line
                 LIMIT 50",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![today, daily_like], map_task_row)
            .map_err(map_sql_err)?;
        collect(rows)
    })
}

/// Open tasks whose `due` is strictly after `today`, newest-due first,
/// limited to `limit` (defaults to 30).
#[tauri::command]
pub fn index_tasks_upcoming(
    today: String,
    limit: Option<i64>,
    state: State<AppState>,
) -> AppResult<Vec<TaskRow>> {
    let limit = limit.unwrap_or(30).clamp(1, 200);
    with_conn(&state, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.note_path, n.title, t.line, t.text, t.done, t.due, t.priority
                 FROM tasks t
                 LEFT JOIN notes n ON n.path = t.note_path
                 WHERE t.done = 0 AND t.due IS NOT NULL AND t.due > ?1
                 ORDER BY t.due ASC, t.note_path, t.line
                 LIMIT ?2",
            )
            .map_err(map_sql_err)?;
        let rows = stmt
            .query_map(params![today, limit], map_task_row)
            .map_err(map_sql_err)?;
        collect(rows)
    })
}

/// Total number of open (`done = 0`) tasks. Used for count badges.
#[tauri::command]
pub fn index_tasks_count(state: State<AppState>) -> AppResult<i64> {
    with_conn(&state, |conn| {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE done = 0", [], |row| row.get(0))
            .map_err(map_sql_err)?;
        Ok(n)
    })
}

/// Toggle a single task's `[ ]` ↔ `[x]` in place on disk, then re-index
/// the note so counts and filters reflect the change on the next query.
///
/// `note_path` is vault-relative; `line` is 1-based (matches `tasks.line`).
/// `done` is the desired new state.
#[tauri::command]
pub fn toggle_task_done(
    note_path: String,
    line: i64,
    done: bool,
    state: State<AppState>,
) -> AppResult<()> {
    let active = state.active_vault.lock().unwrap().clone();
    let read_path = resolve_in_vault(&active, &note_path)?;
    let write_path = resolve_write_target_in_vault(&active, &note_path)?;

    let content = std::fs::read_to_string(&read_path)?;
    let mut out = String::with_capacity(content.len());
    let line_target = line.max(1) as usize;
    let mut current = 0usize;
    let mut found = false;
    for raw_line in content.split_inclusive('\n') {
        current += 1;
        if current == line_target {
            out.push_str(&toggle_checkbox(raw_line, done));
            found = true;
        } else {
            out.push_str(raw_line);
        }
    }
    if !found {
        return Err(AppError::Other(format!(
            "toggle_task_done: line {line} out of range for {note_path}"
        )));
    }
    atomic_write(&write_path, out.as_bytes())?;

    if let Some(handle) = state.index_handle() {
        if let Some(vault) = active.as_ref() {
            if let Err(e) = scanner::reindex_one(&handle, vault, &note_path) {
                tracing::warn!(rel = %note_path, error = %e, "toggle_task_done: reindex failed");
            }
        }
    }
    Ok(())
}

/// Rewrite the `[ ]` / `[x]` in a single task line to reflect `done`.
/// Preserves everything else, including the trailing newline.
fn toggle_checkbox(line: &str, done: bool) -> String {
    // Find "[ ]" / "[x]" / "[X]" anywhere on the line (list-marker already
    // matched by the parser; here we only need to flip the first checkbox
    // we see). Falls back to returning the input unchanged if the pattern
    // isn't present — callers get a DB refresh but the file stays honest.
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'['
            && (bytes[i + 1] == b' ' || bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
            && bytes[i + 2] == b']'
        {
            let replacement = if done { "[x]" } else { "[ ]" };
            let mut out = String::with_capacity(line.len());
            out.push_str(&line[..i]);
            out.push_str(replacement);
            out.push_str(&line[i + 3..]);
            return out;
        }
        i += 1;
    }
    line.to_string()
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        id: row.get(0)?,
        note_path: row.get(1)?,
        note_title: row.get(2)?,
        line: row.get(3)?,
        text: row.get(4)?,
        done: row.get::<_, i64>(5)? != 0,
        due: row.get(6)?,
        priority: row.get(7)?,
    })
}

fn collect<I, T, E>(iter: I) -> AppResult<Vec<T>>
where
    I: Iterator<Item = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut out = Vec::new();
    for row in iter {
        out.push(row.map_err(|e| AppError::Other(format!("sqlite: {e}")))?);
    }
    Ok(out)
}

fn normalize_tag_filters(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in tags {
        let tag = raw.trim().trim_start_matches('#').trim().to_string();
        if tag.is_empty() {
            continue;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize_tag_filters;

    #[test]
    fn normalize_tag_filters_trims_hashes_and_dedupes() {
        let got = normalize_tag_filters(vec![
            " foo ".into(),
            "#bar".into(),
            "foo".into(),
            "".into(),
            "   ".into(),
            " #baz ".into(),
        ]);
        assert_eq!(got, vec!["foo", "bar", "baz"]);
    }
}
