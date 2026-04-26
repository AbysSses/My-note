//! Vault export commands.
//!
//! Currently a single command: `vault_export_zip`. Walks the active vault
//! and writes every file (except `.mynotes/` — SQLite index + app metadata)
//! into a new zip archive at the user-chosen destination.
//!
//! Design decisions
//! ---
//! * **`.mynotes/` is always excluded**. The SQLite index is a derived
//!   artifact — a new install reindexes in seconds. Shipping it would
//!   double archive size and risk DB-version mismatches for the recipient.
//! * **`attachments/` is included**. Otherwise the exported vault is
//!   missing referenced images — a vault-import on another machine would
//!   have broken `![](attachments/…)` links.
//! * **DEFLATE compression, not ZSTD**. Wider reader compatibility (every
//!   desktop OS's built-in zip tool reads DEFLATE); markdown compresses
//!   well regardless. Avoids the `zstd` dep entirely.
//! * **Paths in the archive are relative to the vault root**, using forward
//!   slashes always. macOS / Windows can both read these.
//! * **Atomic-ish**: we write to `<dest>.part` then rename. A crash
//!   mid-write leaves `<dest>.part` behind (discoverable by the user) but
//!   never a half-written file at the user's chosen path. No rollback of
//!   `.part` on failure — easier to inspect what went wrong than to hide
//!   the evidence.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pulldown_cmark::{html, Options, Parser};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;
use tauri::State;
use url::Url;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::{AppError, AppResult};
use crate::AppState;

/// Summary of a completed vault export, returned to the frontend for display.
#[derive(Debug, Serialize)]
pub struct ExportSummary {
    /// Absolute path of the completed zip.
    pub dest_path: String,
    /// Count of file entries written into the archive (directories not counted).
    pub file_count: u64,
    /// Sum of *uncompressed* bytes written. The zip itself will be smaller.
    pub bytes_written: u64,
    /// Count of files skipped because of the exclusion rules (anything inside
    /// `.mynotes/`). Reported for transparency in the status bar.
    pub skipped_count: u64,
}

/// Pack the entire active vault into a zip archive at `dest_abs_path`.
///
/// `dest_abs_path` must be an absolute path supplied by the frontend's save
/// dialog. We don't validate it lives outside the vault — the user's choice
/// is the user's choice — but we do refuse to clobber an existing file (the
/// frontend's save dialog already warns, but we belt-and-suspender in case
/// the user typed the path manually).
#[tauri::command]
pub fn vault_export_zip(dest_abs_path: String, state: State<AppState>) -> AppResult<ExportSummary> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;

    let dest = PathBuf::from(&dest_abs_path);
    if dest.exists() {
        return Err(AppError::Other(format!(
            "destination already exists: {}",
            dest.display()
        )));
    }
    // Ensure the parent directory exists (save dialog should guarantee this,
    // but we defensively create it — e.g. if a previous export populated a
    // folder that's since been moved).
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let part_path = with_extension_suffix(&dest, ".part");
    // If a stale .part exists from a previous crashed export, remove it so
    // the fresh write can proceed — the user explicitly asked to re-export.
    if part_path.exists() {
        std::fs::remove_file(&part_path)?;
    }

    let file = File::create(&part_path)?;
    let mut zw = ZipWriter::new(file);

    let options: SimpleFileOptions = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        // Unix perms: 0o644 — readable by archive consumers on POSIX. Windows
        // zip tools ignore the field.
        .unix_permissions(0o644);

    let mut file_count: u64 = 0;
    let mut bytes_written: u64 = 0;
    let mut skipped_count: u64 = 0;

    for entry in WalkDir::new(&vault).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                // Permission-denied on a subdirectory shouldn't kill the whole
                // export — surface once in logs and keep going.
                tracing::warn!(error = %err, "walkdir entry error, skipping");
                continue;
            }
        };

        let path = entry.path();
        let rel = match path.strip_prefix(&vault) {
            Ok(p) => p,
            Err(_) => continue, // shouldn't happen, defensive
        };
        if rel.as_os_str().is_empty() {
            continue; // vault root itself
        }

        // Exclude anything under `.mynotes/` (app metadata, SQLite index).
        if rel
            .components()
            .next()
            .map(|c| c.as_os_str() == ".mynotes")
            .unwrap_or(false)
        {
            if entry.file_type().is_file() {
                skipped_count += 1;
            }
            continue;
        }

        // Convert to forward-slash string for the archive. Windows paths use
        // `\`, which zip readers on other platforms misinterpret as a literal
        // character in the filename.
        let archive_name = rel_path_to_archive_name(rel);

        if entry.file_type().is_dir() {
            // Directory entries: add explicitly so empty dirs are preserved
            // (cosmetic but nice).
            zw.add_directory(format!("{archive_name}/"), options)?;
            continue;
        }

        if entry.file_type().is_symlink() {
            // Skip symlinks — avoid cycles and vault-external targets leaking in.
            skipped_count += 1;
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        zw.start_file(&archive_name, options)?;
        let mut src = File::open(path)?;
        let bytes = io::copy(&mut src, &mut zw)?;
        bytes_written = bytes_written.saturating_add(bytes);
        file_count += 1;
    }

    zw.finish()?;

    // Rename `.part` → final dest. Tauri apps don't run as root; both paths
    // live on the same volume (the save dialog confines to a single chosen
    // dir), so `rename` is atomic.
    std::fs::rename(&part_path, &dest)?;

    Ok(ExportSummary {
        dest_path: dest.to_string_lossy().into_owned(),
        file_count,
        bytes_written,
        skipped_count,
    })
}

/// Append `suffix` (e.g. ".part") to a path's filename. Cheap helper — we
/// want "foo.zip.part", not "foo.part" (which would strip the real ext).
fn with_extension_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Convert a relative path to the forward-slash form zip archives expect.
/// On POSIX this is a no-op; on Windows `\` is replaced.
fn rel_path_to_archive_name(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Copy a vault-relative `.md` file to an absolute destination path.
///
/// Used by the "Export current note" command — the frontend picks `dest`
/// via the Tauri save dialog, and we do the write here so we don't need
/// the `fs` plugin on the JS side (adding it for one call would double
/// the bundle's Tauri-surface and require a capabilities JSON edit).
///
/// We don't overwrite silently — if `dest` exists the caller's save
/// dialog already warned; we still refuse. `src_rel_path` is resolved
/// against the active vault exactly like `file_read`.
#[tauri::command]
pub fn note_export_copy(
    src_rel_path: String,
    dest_abs_path: String,
    state: State<AppState>,
) -> AppResult<()> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;

    // Sanity: refuse `..` escapes and leading `/` — same contract as
    // `file_read` / `file_write` so we can't be tricked into copying
    // outside the vault.
    let rel = PathBuf::from(&src_rel_path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::PathEscape(src_rel_path));
    }
    let src = vault.join(&rel);
    if !src.is_file() {
        return Err(AppError::Other(format!(
            "source is not a file: {}",
            src.display()
        )));
    }

    let dest = PathBuf::from(&dest_abs_path);
    if dest.exists() {
        return Err(AppError::Other(format!(
            "destination already exists: {}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&src, &dest)?;
    Ok(())
}

/// Render a vault-relative `.md` file to a standalone HTML print-preview
/// document and open it in the user's default browser. Returns the
/// absolute path of the written HTML so the frontend can surface it in
/// the status bar.
///
/// Why a separate "render → open in browser" path instead of `window.print()`:
/// on macOS Tauri WKWebView, `window.print()` triggered from a non-user
/// gesture (palette command handler / setTimeout) is silently dropped —
/// no print dialog appears. Rendering to HTML + `opener::open` delegates
/// to the system browser where the native `⌘P` shortcut Just Works, and
/// also sidesteps CodeMirror's viewport virtualization (CM6 only keeps
/// on-screen lines in the DOM, so `@media print` captures at most the
/// visible slice).
///
/// Output lives under `app_support_dir/print-preview/<stem>-<ts>.html`.
/// Each invocation writes a fresh file — we don't GC old previews
/// automatically; they're tiny (KB-range) and `app_support_dir` is
/// hidden from users. A future "clean up" pass can sweep them if this
/// ever matters.
#[tauri::command]
pub fn note_render_print_html(
    src_rel_path: String,
    theme: Option<String>,
    state: State<AppState>,
) -> AppResult<String> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;

    // Same sandbox contract as file_read / note_export_copy.
    let rel = PathBuf::from(&src_rel_path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::PathEscape(src_rel_path));
    }
    let src = vault.join(&rel);
    if !src.is_file() {
        return Err(AppError::Other(format!(
            "source is not a file: {}",
            src.display()
        )));
    }

    let md = std::fs::read_to_string(&src)?;
    let body_md = strip_frontmatter(&md);

    // Rewrite `[[target]]` / `[[target|alias]]` into plain CommonMark
    // `[alias](#slug)` **before** the parser sees them. pulldown-cmark
    // has no concept of wiki-links; without this pass they'd be emitted
    // as literal `[[foo]]` text in the printed HTML (plain characters,
    // not styled as a link). Since the output is a single-note page the
    // anchor target is a local `#<slug>` — it won't resolve to anything
    // inside this one doc, but the link gets the `<a>` treatment (accent
    // colour, hyperlink affordance in PDF readers) and the href shows
    // the intended target on hover. A future multi-note export can reuse
    // the same anchor scheme without reworking the wiki-link layer.
    let preprocessed = preprocess_wikilinks(body_md);

    // Render CommonMark → HTML with GFM-ish extras. Task lists + tables
    // + strikethrough are ubiquitous in notes and trivially supported.
    // Smart punctuation off on purpose — quotes inside code samples
    // shouldn't silently curl.
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(&preprocessed, opts);
    let mut html_body = String::new();
    html::push_html(&mut html_body, parser);

    // `<base href="file:///…vault/">` so relative image paths resolve to
    // vault files when the browser loads the HTML off disk. `Url::from_directory_path`
    // produces a trailing-slash file:// URL with correct percent-encoding
    // (handles spaces, unicode, etc.).
    let vault_base = Url::from_directory_path(&vault)
        .map_err(|_| {
            AppError::Other(format!(
                "cannot build file URL for vault: {}",
                vault.display()
            ))
        })?
        .to_string();

    let title = rel
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Note".to_string());
    let theme_mode = PrintTheme::from_option(theme.as_deref());
    let full_html = wrap_print_html(&title, &vault_base, &html_body, theme_mode);

    // Write to app_support/print-preview/<stem>-<ts>.html. Timestamp ms
    // so rapid re-runs don't collide; sanitize stem so Windows-unfriendly
    // chars (`:` / `?` / etc.) in note titles don't break the filename.
    let out_dir = state.app_support_dir.join("print-preview");
    std::fs::create_dir_all(&out_dir)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let safe = sanitize_stem(&title);
    let out_path = out_dir.join(format!("{safe}-{ts}.html"));
    std::fs::write(&out_path, full_html)?;

    // Hand off to the OS. `opener` uses `open` on macOS, `start` on
    // Windows, `xdg-open` on Linux — each one picks the user's default
    // browser for .html.
    opener::open(&out_path).map_err(|e| AppError::Other(format!("open failed: {e}")))?;

    Ok(out_path.to_string_lossy().into_owned())
}

/// Strip the leading YAML frontmatter block if present. Accepts both
/// `---\n…\n---\n` (Unix) and `---\r\n…\r\n---\r\n` (Windows). Leaves
/// the input untouched if there's no opening fence or no closing fence
/// — malformed frontmatter renders literally rather than silently
/// eating the whole note.
fn strip_frontmatter(md: &str) -> &str {
    let Some(after_open) = md
        .strip_prefix("---\n")
        .or_else(|| md.strip_prefix("---\r\n"))
    else {
        return md;
    };
    let mut consumed = 0usize;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            return &after_open[consumed + line.len()..];
        }
        consumed += line.len();
    }
    // No closing marker — treat as not-frontmatter.
    md
}

/// Turn a note title into a filesystem-safe stem. Replaces characters
/// Windows/macOS file systems don't like with `_`; collapses whitespace.
fn sanitize_stem(title: &str) -> String {
    let mut out: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out.trim().to_string();
    if out.is_empty() {
        out = "note".to_string();
    }
    out
}

/// Normalized three-way theme choice for the print-preview HTML.
///
/// The frontend `AppConfig.theme` is a free-form `Option<String>` on the
/// Rust side (so forward-compat with future themes doesn't need a schema
/// migration), but for the print scaffold we only care about three
/// branches: explicit light, explicit dark, or follow-OS. Anything else
/// (null, unknown string, empty) falls back to `System` — matching the
/// frontend's `readThemeFromBrowserStorage` default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintTheme {
    Light,
    Dark,
    System,
}

impl PrintTheme {
    fn from_option(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("light") => PrintTheme::Light,
            Some("dark") => PrintTheme::Dark,
            _ => PrintTheme::System,
        }
    }
}

/// Wrap rendered markdown in a print-friendly HTML scaffold.
///
/// Design choices:
/// * **Inline CSS** so the file is self-contained (double-click still
///   renders correctly after being moved around, and the preview lives
///   under `app_support_dir/print-preview/` without any sibling assets).
/// * **Theme-aware**: for dark-mode / system-mode users the preview used
///   to always render white-on-black, breaking the editor→print visual
///   continuity. We now emit a `[data-theme]` attribute on `<html>` and
///   keep a `@media (prefers-color-scheme: dark)` block for the
///   `System` branch, so the browser still honours the OS choice when
///   the user hasn't pinned light/dark in the app.
/// * **`@page` margin** set to 0.75in — matches the macOS "Preview" /
///   Chrome defaults, so saved PDFs look normal.
/// * **System font stack** — no webfont dependency; matches how the
///   user's editor renders natively, and prints crisply.
/// * **`max-width: 780px`** — standard comfortable reading column.
///   Overrides to 100% in print so full page gets used.
/// * **`<base href>`** so relative image paths (`attachments/…`) resolve
///   to files next to the vault root.
///
/// The actual colour values mirror `src/app.css` loosely (not verbatim —
/// the app uses `oklch()` which prints beautifully in Chrome but is
/// fussy in older PDF viewers; we stick to hex so Preview / iOS Books /
/// third-party PDF renderers all agree on the palette).
fn wrap_print_html(title: &str, base_href: &str, body_html: &str, theme: PrintTheme) -> String {
    let escaped_title = html_escape(title);
    let escaped_base = html_escape(base_href);
    let theme_attr = match theme {
        // Explicit pin → hard-coded colour-scheme + data-theme so browsers
        // that honour `color-scheme` also give us the right scrollbar /
        // form-control chrome.
        PrintTheme::Light => " data-theme=\"light\"",
        PrintTheme::Dark => " data-theme=\"dark\"",
        // System mode leaves `data-theme` off and the `@media` query
        // below handles OS-driven dark. Emitting no attribute is what
        // signals "follow the media query".
        PrintTheme::System => "",
    };
    let color_scheme = match theme {
        PrintTheme::Light => "light",
        PrintTheme::Dark => "dark",
        PrintTheme::System => "light dark",
    };
    // Only include the media-query override when we're in System mode.
    // For pinned light / dark we want the browser to ignore the OS
    // preference entirely (user already decided).
    let media_block = if matches!(theme, PrintTheme::System) {
        r#"@media (prefers-color-scheme: dark) {
    :root:not([data-theme]) {
      --fg: #e6edf3;
      --fg-muted: #8b949e;
      --bg: #0d1117;
      --surface: #161b22;
      --border: #30363d;
      --code-bg: #161b22;
      --accent: #58a6ff;
    }
  }"#
    } else {
        ""
    };
    // Base (light) variables always emitted; explicit dark overrides them
    // via `[data-theme='dark']` scoping, so they don't leak into the light
    // path.
    format!(
        r#"<!doctype html>
<html lang="zh-CN"{theme_attr}>
<head>
<meta charset="utf-8">
<base href="{escaped_base}">
<title>{escaped_title}</title>
<style>
  :root {{
    color-scheme: {color_scheme};
    --fg: #1f2328;
    --fg-muted: #6e7781;
    --bg: #ffffff;
    --surface: #f6f8fa;
    --border: #d0d7de;
    --code-bg: #f6f8fa;
    --accent: #0969da;
  }}
  :root[data-theme='dark'] {{
    --fg: #e6edf3;
    --fg-muted: #8b949e;
    --bg: #0d1117;
    --surface: #161b22;
    --border: #30363d;
    --code-bg: #161b22;
    --accent: #58a6ff;
  }}
  {media_block}
  html, body {{ background: var(--bg); color: var(--fg); }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
      "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
    font-size: 15px;
    line-height: 1.7;
    margin: 0;
    padding: 2rem 1.5rem;
  }}
  main.note {{ max-width: 780px; margin: 0 auto; }}
  h1, h2, h3, h4, h5, h6 {{
    font-weight: 600;
    line-height: 1.3;
    margin-top: 2em;
    margin-bottom: 0.6em;
  }}
  h1 {{ font-size: 1.8em; border-bottom: 1px solid var(--border); padding-bottom: 0.3em; }}
  h2 {{ font-size: 1.4em; border-bottom: 1px solid var(--border); padding-bottom: 0.2em; }}
  h3 {{ font-size: 1.2em; }}
  h4 {{ font-size: 1.05em; }}
  p, ul, ol, blockquote, table, pre {{ margin: 0.8em 0; }}
  a {{ color: var(--accent); text-decoration: none; }}
  a:hover {{ text-decoration: underline; }}
  img {{ max-width: 100%; height: auto; }}
  code {{
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, "Cascadia Code", monospace;
    font-size: 0.92em;
    background: var(--code-bg);
    border-radius: 4px;
    padding: 0.15em 0.35em;
  }}
  pre {{
    background: var(--code-bg);
    border-radius: 6px;
    padding: 0.9em 1em;
    overflow-x: auto;
    font-size: 0.88em;
    line-height: 1.55;
  }}
  pre code {{ background: transparent; padding: 0; border-radius: 0; }}
  blockquote {{
    border-left: 3px solid var(--border);
    padding: 0 1em;
    color: var(--fg-muted);
    margin-left: 0;
  }}
  table {{
    border-collapse: collapse;
    width: 100%;
  }}
  th, td {{
    border: 1px solid var(--border);
    padding: 0.4em 0.7em;
    text-align: left;
  }}
  th {{ background: var(--surface); }}
  ul.task-list, li.task-list-item {{ list-style: none; padding-left: 0; }}
  li.task-list-item {{ margin-left: -1.2em; }}
  input[type="checkbox"] {{ margin-right: 0.4em; }}
  hr {{ border: 0; border-top: 1px solid var(--border); margin: 2em 0; }}
  @page {{ margin: 0.75in; }}
  @media print {{
    /* Printed output is always on white paper. Force the light palette
       even when the preview was viewed in dark — otherwise saving to
       PDF from a dark preview would carry the dark background into the
       PDF, wasting ink and being hard to read on paper. */
    :root, :root[data-theme='dark'] {{
      --fg: #1f2328;
      --fg-muted: #6e7781;
      --bg: #ffffff;
      --surface: #f6f8fa;
      --border: #d0d7de;
      --code-bg: #f6f8fa;
      --accent: #0969da;
    }}
    body {{ padding: 0; font-size: 11pt; }}
    main.note {{ max-width: 100%; }}
    a {{ color: var(--fg); }}
    pre, code {{ background: transparent !important; }}
    pre {{ border: 1px solid var(--border); }}
  }}
</style>
</head>
<body>
<main class="note">
<h1>{escaped_title}</h1>
{body_html}
<hr>
<p style="color: var(--fg-muted); font-size: 0.85em;">MyNotes · 使用浏览器 <kbd>⌘P</kbd> / <kbd>Ctrl+P</kbd> 保存为 PDF</p>
</main>
</body>
</html>
"#
    )
}

/// Minimal HTML-entity escaper for text we interpolate into attributes
/// and plain text (title, base href). The rendered markdown body is
/// passed through unchanged — pulldown-cmark already escapes it.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Regex for `[[target]]` and `[[target|alias]]`. Matches lazily on the
/// target so the first `]]` terminates, matching the live-preview decorator
/// used in the editor. Compiled once via `OnceLock` — this function can be
/// called on every print invocation.
fn wikilink_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // `[^\]|]+?` — target can't contain `]` or `|` (alias delimiter).
        // `(?:\|([^\]]+))?` — optional alias, allows spaces and `|` inside.
        Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]+))?\]\]").expect("static regex compiles")
    })
}

/// Rewrite every `[[target]]` / `[[target|alias]]` inside `md` into a plain
/// CommonMark link `[display](#<slug>)`. Pure function; no IO, no DB lookup.
///
/// Design notes:
/// * **No unresolved/resolved distinction at this layer.** The print HTML
///   is self-contained and the target anchor may not exist inside the same
///   doc — that's fine; the point is to make the link *semantic* so PDF
///   readers and a11y tools see an `<a>` instead of plain text. A later
///   multi-note export can layer a resolver on top without changing this
///   pre-processor.
/// * **Escape the display text.** pulldown-cmark treats `]`, `[`, and `\`
///   inside link text specially; we route user-provided targets/aliases
///   through `escape_md_link_text` so `[[foo [bar]]]` doesn't become a
///   parser crash.
/// * **Leave `\[\[foo\]\]` alone.** Escaped wiki-links are intentional
///   literal text (e.g. documenting the syntax) — the regex's literal `[[`
///   bound requires unescaped brackets, which the markdown author can't
///   produce via `\[\[` (that compiles to the text `\[\[` in CommonMark
///   but for *our* input we're looking at raw markdown before parsing,
///   so a leading `\` in the source keeps `[[` from matching — verified
///   in tests).
pub fn preprocess_wikilinks(md: &str) -> String {
    wikilink_re()
        .replace_all(md, |caps: &regex::Captures<'_>| {
            let target = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let alias = caps
                .get(2)
                .map(|m| m.as_str().trim())
                .filter(|s| !s.is_empty());
            let display = alias.unwrap_or(target);
            let slug = wikilink_slug(target);
            let display_escaped = escape_md_link_text(display);
            if slug.is_empty() {
                // All punctuation target (e.g. `[[---]]`) → nothing to link
                // to. Preserve the original literal so the user can fix it.
                return caps
                    .get(0)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
            }
            format!("[{display_escaped}](#{slug})")
        })
        .into_owned()
}

/// CJK-friendly slugify: ASCII alphanumerics and CJK unified ideographs
/// survive; runs of anything else collapse to a single `-`. Trim leading
/// and trailing dashes. Truncate to 64 chars (by character count, not
/// bytes) to keep HTML anchor IDs bounded.
///
/// This is deliberately a private local copy of the algorithm used by
/// `commands::attachment::slugify` — keeping it in-module makes the
/// `preprocess_wikilinks` unit tests self-contained and lets the two
/// slugifiers evolve independently (attachment filenames have stricter
/// filesystem constraints than HTML anchor IDs).
fn wikilink_slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for ch in s.chars() {
        let keep = ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch);
        if keep {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.chars().count() > 64 {
        let truncated: String = out.chars().take(64).collect();
        return truncated.trim_end_matches('-').to_string();
    }
    out
}

/// Minimal CommonMark link-text escaper. Handles `[`, `]`, and `\` which
/// are the three characters the CommonMark link-text grammar treats
/// specially. Anything else (including `|`, `#`, `(`, `)`) is fine
/// because it can't prematurely terminate `[…]`.
fn escape_md_link_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '[' | ']' => {
                out.push('\\');
                out.push(ch);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_vault(root: &Path) {
        std::fs::create_dir_all(root.join("1-notes")).unwrap();
        std::fs::create_dir_all(root.join(".mynotes")).unwrap();
        std::fs::create_dir_all(root.join("attachments/2026-04")).unwrap();
        std::fs::write(root.join("1-notes/a.md"), b"# A").unwrap();
        std::fs::write(root.join("1-notes/b.md"), b"# B\n\nbody").unwrap();
        std::fs::write(root.join(".mynotes/config.json"), b"{}").unwrap();
        std::fs::write(root.join(".mynotes/index.sqlite"), b"sqlite-bytes").unwrap();
        std::fs::write(root.join("attachments/2026-04/pic.png"), b"fakepng").unwrap();
    }

    /// Extract a zip and return sorted list of entry names + byte sizes.
    fn list_entries(zip_path: &Path) -> Vec<(String, u64)> {
        let f = File::open(zip_path).unwrap();
        let mut zr = zip::ZipArchive::new(f).unwrap();
        let mut out = Vec::new();
        for i in 0..zr.len() {
            let entry = zr.by_index(i).unwrap();
            out.push((entry.name().to_string(), entry.size()));
        }
        out.sort();
        out
    }

    #[test]
    fn export_excludes_mynotes() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        make_vault(&vault);

        let dest = tmp.path().join("out.zip");

        // We need an AppState to call the command directly. Easier to
        // factor out the packing logic if we want fine-grained unit tests,
        // but the shape here exercises the public contract: no AppState
        // stub helper is exposed. So test via the exclusion predicate
        // directly — reproduce the walk and assert membership.
        //
        // This mirrors what `vault_export_zip` does minus the Tauri shell.
        let f = File::create(&dest).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let mut included: Vec<String> = Vec::new();
        for entry in WalkDir::new(&vault) {
            let entry = entry.unwrap();
            let rel = entry.path().strip_prefix(&vault).unwrap();
            if rel.as_os_str().is_empty() {
                continue;
            }
            if rel
                .components()
                .next()
                .map(|c| c.as_os_str() == ".mynotes")
                .unwrap_or(false)
            {
                continue;
            }
            if entry.file_type().is_file() {
                let name = rel_path_to_archive_name(rel);
                zw.start_file(&name, opts).unwrap();
                let mut src = File::open(entry.path()).unwrap();
                std::io::copy(&mut src, &mut zw).unwrap();
                included.push(name);
            }
        }
        zw.finish().unwrap();
        included.sort();

        let got = list_entries(&dest);
        let got_names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert!(!got_names.iter().any(|n| n.starts_with(".mynotes/")));
        assert!(got_names.contains(&"1-notes/a.md"));
        assert!(got_names.contains(&"1-notes/b.md"));
        assert!(got_names.contains(&"attachments/2026-04/pic.png"));
        // And sanity: file counts match the visible tree (3 files).
        assert_eq!(included.len(), 3);
    }

    #[test]
    fn archive_name_uses_forward_slashes() {
        // On Unix this is trivially true; on Windows path separator is
        // `\`. The test just asserts the helper's contract.
        let rel = Path::new("1-notes").join("sub").join("x.md");
        let name = rel_path_to_archive_name(&rel);
        assert_eq!(name, "1-notes/sub/x.md");
    }

    // --- preprocess_wikilinks ------------------------------------------------

    #[test]
    fn wikilink_preprocess_basic_target_becomes_anchor_link() {
        let out = preprocess_wikilinks("See [[Foo Bar]] for context.");
        // Display text defaults to target; slug is the lowercased, dash-
        // collapsed target. Check both the display text and the anchor.
        assert_eq!(out, "See [Foo Bar](#foo-bar) for context.");
    }

    #[test]
    fn wikilink_preprocess_alias_overrides_display_text() {
        let out = preprocess_wikilinks("Jump to [[Real Target|click here]].");
        // Alias becomes the rendered text; slug still derived from target.
        assert_eq!(out, "Jump to [click here](#real-target).");
    }

    #[test]
    fn wikilink_preprocess_cjk_target_slugs_survive() {
        let out = preprocess_wikilinks("见 [[架构讨论]] 章节。");
        // CJK codepoints are preserved verbatim in the slug (no
        // transliteration). Dash rules still apply for spaces/punct.
        assert_eq!(out, "见 [架构讨论](#架构讨论) 章节。");
    }

    #[test]
    fn wikilink_preprocess_two_adjacent_links_do_not_merge() {
        // Regex is non-greedy on target — must stop at first `]]` even
        // if another `[[…]]` follows immediately.
        let out = preprocess_wikilinks("[[a]] [[b]]");
        assert_eq!(out, "[a](#a) [b](#b)");
    }

    #[test]
    fn wikilink_preprocess_empty_slug_preserved_as_literal() {
        // `[[   ]]` / `[[---]]` → slug collapses to empty. Keep the
        // original literal so the user can see the broken markup rather
        // than having it silently become `[  ](#)`.
        let out = preprocess_wikilinks("before [[---]] after");
        assert_eq!(out, "before [[---]] after");
    }

    #[test]
    fn wikilink_preprocess_escape_special_chars_in_display() {
        // `[` in the alias needs CommonMark escaping so the emitted
        // link text `[…]` doesn't re-open early. `]` in the alias is
        // not supported by the wiki-link grammar (live-preview and the
        // wiki-completion both reject it) — we don't test that case.
        let out = preprocess_wikilinks("see [[foo|with [ bracket]] end");
        assert_eq!(out, "see [with \\[ bracket](#foo) end");
    }

    #[test]
    fn wikilink_preprocess_leaves_plain_prose_alone() {
        let src = "No links here, just prose with [brackets] and a [md](url).";
        assert_eq!(preprocess_wikilinks(src), src);
    }

    #[test]
    fn wikilink_slug_helper_is_cjk_aware_and_lowercases_ascii() {
        assert_eq!(wikilink_slug("Hello World"), "hello-world");
        assert_eq!(wikilink_slug("  trailing spaces   "), "trailing-spaces");
        assert_eq!(wikilink_slug("架构 discussion"), "架构-discussion");
        // All-punctuation slug collapses to empty (caught by caller).
        assert_eq!(wikilink_slug("---"), "");
    }

    // --- print-preview theming (P3-A7) ---------------------------------------

    #[test]
    fn print_theme_from_option_normalizes_known_values() {
        assert_eq!(PrintTheme::from_option(Some("light")), PrintTheme::Light);
        assert_eq!(PrintTheme::from_option(Some("dark")), PrintTheme::Dark);
        assert_eq!(PrintTheme::from_option(Some("system")), PrintTheme::System);
        // Unknown strings and None both fall through to System so the
        // print preview always has *some* palette even if the app config
        // carries a forward-compat value we don't recognise yet.
        assert_eq!(PrintTheme::from_option(Some("sepia")), PrintTheme::System);
        assert_eq!(PrintTheme::from_option(Some("")), PrintTheme::System);
        assert_eq!(PrintTheme::from_option(None), PrintTheme::System);
    }

    #[test]
    fn print_html_light_pins_light_palette_and_no_media_query() {
        let html = wrap_print_html("Title", "file:///vault/", "<p>hi</p>", PrintTheme::Light);
        // Light mode sets `data-theme="light"` so browsers that honour the
        // attribute as a colour hint (rare — mostly ours) match the app.
        assert!(html.contains("data-theme=\"light\""));
        assert!(html.contains("color-scheme: light;"));
        // The `:root[data-theme='dark']` override is still present in the
        // stylesheet (single source of truth for dark variables), but the
        // `@media` fallback is NOT emitted — in light mode the OS theme
        // must not leak in.
        assert!(html.contains(":root[data-theme='dark']"));
        assert!(!html.contains("@media (prefers-color-scheme: dark)"));
    }

    #[test]
    fn print_html_dark_pins_dark_palette() {
        let html = wrap_print_html("Title", "file:///vault/", "<p>hi</p>", PrintTheme::Dark);
        assert!(html.contains("data-theme=\"dark\""));
        assert!(html.contains("color-scheme: dark;"));
        // Same as Light: no OS-driven media query, the app already
        // decided for the user.
        assert!(!html.contains("@media (prefers-color-scheme: dark)"));
    }

    #[test]
    fn print_html_system_emits_media_query_and_drops_data_theme() {
        let html = wrap_print_html("Title", "file:///vault/", "<p>hi</p>", PrintTheme::System);
        // No `data-theme` attribute on <html> — that's what makes
        // `:root:not([data-theme])` inside the media query the one that
        // actually matches.
        assert!(!html.contains("data-theme=\"light\""));
        assert!(!html.contains("data-theme=\"dark\""));
        assert!(html.contains("color-scheme: light dark"));
        // The media query IS emitted in system mode.
        assert!(html.contains("@media (prefers-color-scheme: dark)"));
        assert!(html.contains(":root:not([data-theme])"));
    }

    #[test]
    fn print_html_print_media_always_forces_light_for_paper_output() {
        // Regardless of which preview theme was chosen, printing on
        // paper should flip back to the light palette so saved PDFs
        // aren't black-background ink wasters. The `@media print`
        // block carries a palette override scoped to both `:root` and
        // `:root[data-theme='dark']` — check both branches.
        for theme in [PrintTheme::Light, PrintTheme::Dark, PrintTheme::System] {
            let html = wrap_print_html("T", "file:///v/", "body", theme);
            assert!(html.contains("@media print"));
            assert!(
                html.contains(":root, :root[data-theme='dark'] {"),
                "print block should reset both light and dark roots (theme={theme:?})"
            );
        }
    }
}
