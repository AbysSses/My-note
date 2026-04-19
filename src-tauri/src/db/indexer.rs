//! Markdown → index record conversion.
//!
//! One file in, one [`ParsedNote`] out. No DB concerns here — callers decide
//! when to upsert what.

use std::path::Path;
use std::sync::OnceLock;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{AppError, AppResult};

use super::map_sql_err;

/// Everything we extract from one `.md` file. Ready to be inserted into the
/// `notes`, `tags`, `links`, `tasks`, and `notes_fts` tables.
#[derive(Debug, Clone, Serialize)]
pub struct ParsedNote {
    pub title: Option<String>,
    pub note_type: Option<String>,
    pub status: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub project_slug: Option<String>,
    /// Round-tripped frontmatter as JSON. `{}` when absent.
    pub frontmatter_json: String,
    pub tags: Vec<String>,
    pub links: Vec<ParsedLink>,
    pub tasks: Vec<ParsedTask>,
    /// Body stripped of frontmatter, for FTS indexing.
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedLink {
    /// The raw `[[target]]` as the user typed it (no brackets, no alias).
    pub dst: String,
    /// Always "wiki" for now; "markdown" / "embed" come later.
    pub link_type: &'static str,
    /// Byte offset within the **whole file** of the `[[`.
    pub position: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedTask {
    /// 1-based line number within the whole file.
    pub line: usize,
    pub text: String,
    pub done: bool,
}

/// Parse one note. Pure — does not touch the filesystem or DB.
pub fn parse_note(rel_path: &str, contents: &str) -> ParsedNote {
    let (fm_raw, body_start) = split_frontmatter(contents);
    let body = &contents[body_start..];

    let (fm_fields, fm_json) = parse_frontmatter_fields(fm_raw);

    // Gather tags from frontmatter first, then augment with inline #tag.
    let mut tags = fm_fields.tags.clone();
    let (inline_tags, links, tasks) = scan_body(body, body_start);
    for t in inline_tags {
        if !tags.iter().any(|x| x == &t) {
            tags.push(t);
        }
    }

    // Pick a title: frontmatter > first H1 > stem of filename.
    let title = fm_fields
        .title
        .clone()
        .or_else(|| first_h1(body))
        .or_else(|| title_from_path(rel_path));

    // Infer type / project_slug from path when frontmatter doesn't say.
    let note_type = fm_fields
        .note_type
        .clone()
        .or_else(|| infer_type_from_path(rel_path, body).map(str::to_string));
    let project_slug = fm_fields
        .project_slug
        .clone()
        .or_else(|| infer_project_slug(rel_path));

    ParsedNote {
        title,
        note_type,
        status: fm_fields.status,
        created: fm_fields.created,
        updated: fm_fields.updated,
        project_slug,
        frontmatter_json: fm_json,
        tags,
        links,
        tasks,
        body: body.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Frontmatter: YAML between leading `---` fences.

fn split_frontmatter(contents: &str) -> (&str, usize) {
    // Must start with "---\n" or "---\r\n".
    let rest = if let Some(r) = contents.strip_prefix("---\n") {
        r
    } else if let Some(r) = contents.strip_prefix("---\r\n") {
        r
    } else {
        return ("", 0);
    };
    let prefix_len = contents.len() - rest.len();

    // Find closing "---" on its own line.
    let mut cursor = 0;
    while cursor < rest.len() {
        let line_end = rest[cursor..]
            .find('\n')
            .map(|i| cursor + i)
            .unwrap_or(rest.len());
        let line = rest[cursor..line_end].trim_end_matches('\r');
        if line == "---" {
            let fm = &rest[..cursor];
            let body_start = prefix_len + (line_end + 1).min(rest.len());
            return (fm, body_start);
        }
        cursor = line_end + 1;
    }
    // Unterminated frontmatter — treat whole file as body so we don't lose it.
    ("", 0)
}

#[derive(Debug, Default)]
struct FmFields {
    title: Option<String>,
    note_type: Option<String>,
    status: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    project_slug: Option<String>,
    tags: Vec<String>,
}

fn parse_frontmatter_fields(fm_raw: &str) -> (FmFields, String) {
    if fm_raw.trim().is_empty() {
        return (FmFields::default(), "{}".to_string());
    }
    match serde_yaml::from_str::<serde_yaml::Value>(fm_raw) {
        Ok(val) => {
            let fields = extract_fm_fields(&val);
            // Serialize the whole YAML as JSON so unknown fields round-trip.
            let json = serde_json::to_string(&val).unwrap_or_else(|_| "{}".to_string());
            (fields, json)
        }
        Err(e) => {
            tracing::warn!(error = %e, "frontmatter YAML parse failed; keeping body only");
            (FmFields::default(), "{}".to_string())
        }
    }
}

fn extract_fm_fields(val: &serde_yaml::Value) -> FmFields {
    let map = match val.as_mapping() {
        Some(m) => m,
        None => return FmFields::default(),
    };

    let get = |key: &str| -> Option<String> {
        map.get(serde_yaml::Value::String(key.to_string()))
            .and_then(yaml_to_string)
    };

    let tags = map
        .get(serde_yaml::Value::String("tags".to_string()))
        .map(yaml_to_string_vec)
        .unwrap_or_default();

    FmFields {
        title: get("title"),
        note_type: get("type"),
        status: get("status"),
        created: get("created"),
        updated: get("updated"),
        project_slug: get("project").or_else(|| get("project_slug")),
        tags,
    }
}

fn yaml_to_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn yaml_to_string_vec(v: &serde_yaml::Value) -> Vec<String> {
    match v {
        serde_yaml::Value::Sequence(items) => items.iter().filter_map(yaml_to_string).collect(),
        serde_yaml::Value::String(s) => {
            // Support "tags: a, b, c" single-line form.
            s.split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('#').to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Body scan: inline tags, wiki-links, tasks. One pass so we can honor code
// fences consistently.

fn scan_body(body: &str, body_offset: usize) -> (Vec<String>, Vec<ParsedLink>, Vec<ParsedTask>) {
    let mut tags: Vec<String> = Vec::new();
    let mut links: Vec<ParsedLink> = Vec::new();
    let mut tasks: Vec<ParsedTask> = Vec::new();

    let mut in_code = false;
    let mut line_no = 0usize;
    let mut cursor = 0usize;

    for line in body.split_inclusive('\n') {
        line_no += 1;
        let line_start_abs = body_offset + cursor;
        let trimmed = line.trim_end_matches(|c| c == '\n' || c == '\r');

        // Fenced code block toggle. ``` at line start (any length >= 3).
        let leading = trimmed.trim_start();
        if leading.starts_with("```") || leading.starts_with("~~~") {
            in_code = !in_code;
            cursor += line.len();
            continue;
        }
        if in_code {
            cursor += line.len();
            continue;
        }

        // --- task lines ---
        if let Some(task) = parse_task_line(trimmed) {
            tasks.push(ParsedTask {
                line: line_no,
                text: task.text,
                done: task.done,
            });
        }

        // --- wiki links ---
        for (abs_pos, raw_target) in find_wiki_links(trimmed, line_start_abs) {
            links.push(ParsedLink {
                dst: raw_target,
                link_type: "wiki",
                position: abs_pos,
            });
        }

        // --- inline tags ---
        //
        // We mask out `[[...]]` and inline code spans first, otherwise
        // `#tag` inside a wiki-link target or `` `snippet` `` would bleed in.
        let masked = mask_noise(trimmed);
        for t in find_inline_tags(&masked) {
            if !tags.iter().any(|x| x == &t) {
                tags.push(t);
            }
        }

        cursor += line.len();
    }

    (tags, links, tasks)
}

struct TaskHit {
    text: String,
    done: bool,
}

fn parse_task_line(line: &str) -> Option<TaskHit> {
    // `- [ ]`, `* [x]`, `+ [X]`, `1. [ ]` (ordered list with checkbox too).
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();

    // Walk past the list marker. Return (marker_len, rest).
    let after_marker = if trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
    {
        &trimmed[2..]
    } else {
        // Ordered: "1. " / "12. "
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == 0 || i >= bytes.len() || bytes[i] != b'.' {
            return None;
        }
        let rest = &trimmed[i + 1..];
        rest.strip_prefix(' ')?
    };

    // Expect `[X]` where X is space, x, or X.
    let ab = after_marker.as_bytes();
    if ab.len() < 3 || ab[0] != b'[' || ab[2] != b']' {
        return None;
    }
    let done = match ab[1] {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    // Need a space after `]` (or EOL).
    let rest = &after_marker[3..];
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    Some(TaskHit {
        text: rest.to_string(),
        done,
    })
}

/// Find `[[target]]` or `[[target|alias]]` occurrences in a single line.
/// Returns (absolute file offset of `[[`, target). Ignores escaped `\[[`.
fn find_wiki_links(line: &str, line_offset_abs: usize) -> Vec<(usize, String)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // Escaped?
            if i > 0 && bytes[i - 1] == b'\\' {
                i += 2;
                continue;
            }
            // Scan for "]]" on the same line.
            let start = i + 2;
            let mut j = start;
            while j + 1 < bytes.len() && !(bytes[j] == b']' && bytes[j + 1] == b']') {
                j += 1;
            }
            if j + 1 < bytes.len() && bytes[j] == b']' && bytes[j + 1] == b']' {
                let inner = &line[start..j];
                // Strip alias segment.
                let target = match inner.find('|') {
                    Some(p) => &inner[..p],
                    None => inner,
                };
                let target = target.trim();
                if !target.is_empty() {
                    out.push((line_offset_abs + i, target.to_string()));
                }
                i = j + 2;
                continue;
            } else {
                break;
            }
        }
        i += 1;
    }
    out
}

/// Mask `[[…]]` and `` `…` `` runs with spaces so downstream tag-matching
/// doesn't catch `#fragments` inside them.
fn mask_noise(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // Copy until matching ]] (or EOL).
            out.push_str("  ");
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b']' && bytes[j + 1] == b']') {
                out.push(' ');
                j += 1;
            }
            if j + 1 < bytes.len() {
                out.push_str("  ");
                j += 2;
            }
            i = j;
            continue;
        }
        if bytes[i] == b'`' {
            out.push(' ');
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'`' {
                out.push(' ');
                j += 1;
            }
            if j < bytes.len() {
                out.push(' ');
                j += 1;
            }
            i = j;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_inline_tags(masked_line: &str) -> Vec<String> {
    // #tag at start of line or after whitespace; stops at space or punctuation
    // other than `-`, `_`, `/`. Supports CJK.
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(?:^|\s)#([A-Za-z0-9\u{4e00}-\u{9fa5}_\-/]+)").unwrap()
    });
    let mut out = Vec::new();
    for cap in re.captures_iter(masked_line) {
        if let Some(m) = cap.get(1) {
            let tag = m.as_str().to_string();
            // Skip pure-digits (likely `#1234` issue refs, not tags).
            if !tag.chars().all(|c| c.is_ascii_digit()) && !out.contains(&tag) {
                out.push(tag);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Path / title helpers.

fn first_h1(body: &str) -> Option<String> {
    let mut in_code = false;
    for line in body.lines() {
        let leading = line.trim_start();
        if leading.starts_with("```") || leading.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            let t = rest.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn title_from_path(rel_path: &str) -> Option<String> {
    let stem = Path::new(rel_path).file_stem().and_then(|s| s.to_str())?;
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

fn infer_type_from_path(rel_path: &str, _body: &str) -> Option<&'static str> {
    // Normalize separators. We store everything with '/'.
    let p = rel_path.replace('\\', "/");
    let segs: Vec<&str> = p.split('/').collect();
    let top = *segs.first()?;
    match top {
        "0-inbox" => Some("inbox"),
        "1-notes" => Some("note"),
        "2-moc" => Some("moc"),
        "3-journal" => {
            let stem = Path::new(&p).file_stem()?.to_str()?;
            if is_daily_slug(stem) {
                Some("daily")
            } else if is_weekly_slug(stem) {
                Some("weekly")
            } else {
                Some("note")
            }
        }
        "4-projects" => {
            // 4-projects/{slug}/index.md → project; else project-note
            if segs.len() >= 3 && segs.last().map(|s| *s == "index.md").unwrap_or(false) {
                Some("project")
            } else {
                Some("project-note")
            }
        }
        _ => None,
    }
}

fn infer_project_slug(rel_path: &str) -> Option<String> {
    let p = rel_path.replace('\\', "/");
    let segs: Vec<&str> = p.split('/').collect();
    if segs.len() >= 3 && segs[0] == "4-projects" {
        Some(segs[1].to_string())
    } else {
        None
    }
}

fn is_daily_slug(s: &str) -> bool {
    // YYYY-MM-DD
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

fn is_weekly_slug(s: &str) -> bool {
    // YYYY-Www (upper or lower W)
    let b = s.as_bytes();
    b.len() == 8
        && b[4] == b'-'
        && (b[5] == b'W' || b[5] == b'w')
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[6..8].iter().all(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// DB write path.

/// Upsert one parsed note (and its tags / links / tasks / fts row) into the DB.
/// Caller wraps this in a transaction for bulk operations.
pub fn upsert_note(
    conn: &Connection,
    rel_path: &str,
    parsed: &ParsedNote,
    size: i64,
    mtime: i64,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO notes (path, title, type, status, created, updated, size, mtime, project_slug, frontmatter_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(path) DO UPDATE SET
           title = excluded.title,
           type = excluded.type,
           status = excluded.status,
           created = excluded.created,
           updated = excluded.updated,
           size = excluded.size,
           mtime = excluded.mtime,
           project_slug = excluded.project_slug,
           frontmatter_json = excluded.frontmatter_json",
        rusqlite::params![
            rel_path,
            parsed.title,
            parsed.note_type,
            parsed.status,
            parsed.created,
            parsed.updated,
            size,
            mtime,
            parsed.project_slug,
            parsed.frontmatter_json,
        ],
    )
    .map_err(map_sql_err)?;

    // Replace tags / links / tasks wholesale — simpler than diffing.
    conn.execute("DELETE FROM tags WHERE note_path = ?1", [rel_path])
        .map_err(map_sql_err)?;
    for tag in &parsed.tags {
        conn.execute(
            "INSERT OR IGNORE INTO tags (note_path, tag) VALUES (?1, ?2)",
            rusqlite::params![rel_path, tag],
        )
        .map_err(map_sql_err)?;
    }

    conn.execute("DELETE FROM links WHERE src = ?1", [rel_path])
        .map_err(map_sql_err)?;
    {
        let mut stmt = conn
            .prepare_cached(
                "INSERT INTO links (src, dst, dst_resolved, link_type, position)
                 VALUES (?1, ?2, NULL, ?3, ?4)",
            )
            .map_err(map_sql_err)?;
        for l in &parsed.links {
            stmt.execute(rusqlite::params![rel_path, l.dst, l.link_type, l.position as i64])
                .map_err(map_sql_err)?;
        }
    }

    conn.execute("DELETE FROM tasks WHERE note_path = ?1", [rel_path])
        .map_err(map_sql_err)?;
    {
        let mut stmt = conn
            .prepare_cached(
                "INSERT INTO tasks (note_path, line, text, done) VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(map_sql_err)?;
        for t in &parsed.tasks {
            stmt.execute(rusqlite::params![
                rel_path,
                t.line as i64,
                t.text,
                if t.done { 1i64 } else { 0i64 }
            ])
            .map_err(map_sql_err)?;
        }
    }

    // FTS: contentless external table — delete then insert.
    conn.execute("DELETE FROM notes_fts WHERE path = ?1", [rel_path])
        .map_err(map_sql_err)?;
    conn.execute(
        "INSERT INTO notes_fts (path, title, body) VALUES (?1, ?2, ?3)",
        rusqlite::params![rel_path, parsed.title.clone().unwrap_or_default(), parsed.body],
    )
    .map_err(map_sql_err)?;

    Ok(())
}

/// Drop everything for a single note (path gone / renamed).
pub fn delete_note(conn: &Connection, rel_path: &str) -> AppResult<()> {
    // ON DELETE CASCADE handles tags/links/tasks.
    conn.execute("DELETE FROM notes WHERE path = ?1", [rel_path])
        .map_err(map_sql_err)?;
    conn.execute("DELETE FROM notes_fts WHERE path = ?1", [rel_path])
        .map_err(map_sql_err)?;
    Ok(())
}

/// Resolve all unresolved wiki-links by looking them up against current notes.
/// Cheap enough to run on every batch (a full scan resolves the whole set once).
pub fn resolve_links(conn: &Connection) -> AppResult<()> {
    // Precedence: exact title match → filename stem match.
    // Done in two update passes; SQLite subquery picks ANY match when there
    // are collisions, which is an acceptable limitation for now.
    conn.execute(
        "UPDATE links SET dst_resolved = (
             SELECT path FROM notes WHERE notes.title = links.dst LIMIT 1
         ) WHERE dst_resolved IS NULL",
        [],
    )
    .map_err(map_sql_err)?;
    conn.execute(
        "UPDATE links SET dst_resolved = (
             SELECT path FROM notes
             WHERE
               -- match stem of path (strip dir + .md). rfind would be nice
               -- but we settle for simple equality after a LIKE narrowing.
               notes.path LIKE '%/' || links.dst || '.md'
                OR notes.path = links.dst || '.md'
             LIMIT 1
         ) WHERE dst_resolved IS NULL",
        [],
    )
    .map_err(map_sql_err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Error shim.

#[allow(dead_code)]
fn _bind_app_error(_e: AppError) {}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter() {
        let src = "---\ntitle: A\n---\n# H1\nbody\n";
        let (fm, offset) = split_frontmatter(src);
        assert_eq!(fm, "title: A\n");
        assert_eq!(&src[offset..], "# H1\nbody\n");
    }

    #[test]
    fn no_frontmatter() {
        let src = "# H1\nbody\n";
        let (fm, offset) = split_frontmatter(src);
        assert!(fm.is_empty());
        assert_eq!(offset, 0);
    }

    #[test]
    fn parses_tags_and_links() {
        let src = "---\ntags: [knowledge]\n---\n# Hello\n\nBody with [[Some Note]] and #tag1 and `#inside-code` should skip.\n\n- [ ] a task\n- [x] done task\n";
        let parsed = parse_note("1-notes/hello.md", src);
        assert!(parsed.tags.contains(&"knowledge".to_string()));
        assert!(parsed.tags.contains(&"tag1".to_string()));
        assert!(!parsed.tags.contains(&"inside-code".to_string()));
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(parsed.links[0].dst, "Some Note");
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].done, false);
        assert_eq!(parsed.tasks[1].done, true);
        assert_eq!(parsed.note_type.as_deref(), Some("note"));
        assert_eq!(parsed.title.as_deref(), Some("Hello"));
    }

    #[test]
    fn infers_types() {
        assert_eq!(
            infer_type_from_path("0-inbox/abc.md", ""),
            Some("inbox")
        );
        assert_eq!(
            infer_type_from_path("3-journal/2026-04-19.md", ""),
            Some("daily")
        );
        assert_eq!(
            infer_type_from_path("3-journal/2026-W16.md", ""),
            Some("weekly")
        );
        assert_eq!(
            infer_type_from_path("4-projects/foo/index.md", ""),
            Some("project")
        );
        assert_eq!(
            infer_type_from_path("4-projects/foo/note.md", ""),
            Some("project-note")
        );
    }

    #[test]
    fn wiki_link_with_alias() {
        let src = "See [[real-note|Friendly Name]].\n";
        let parsed = parse_note("1-notes/x.md", src);
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(parsed.links[0].dst, "real-note");
    }
}
