//! IPC commands that expose the SQLite index to the frontend.
//!
//! Every command locks the per-vault `Arc<Mutex<Connection>>` just for the
//! duration of a single query — we don't hold it across awaits, and we
//! clone the `Arc` out of AppState first so IPC threads never contend on
//! the outer state mutex.

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::db::map_sql_err;
use crate::error::{AppError, AppResult};
use crate::AppState;

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
fn fts_sanitize(q: &str) -> String {
    let escaped = q.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
