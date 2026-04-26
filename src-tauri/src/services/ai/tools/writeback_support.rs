use std::fs;
use std::path::{Component, Path};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::provider::{complete_text, CompleteTextRequest};
use super::super::tool_registry::ToolContext;
use super::common;

const BUNDLED_MOC_TEMPLATE: &str = include_str!("../../../../templates/moc.md");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalPayload {
    pub proposal_kind: String,
    pub target_rel_path: String,
    pub original_content: String,
    pub proposed_content: String,
    pub summary: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct NoteRef {
    pub path: String,
    pub title: Option<String>,
}

pub fn proposal_ok(payload: &ProposalPayload) -> super::super::provider::ToolResult {
    common::ok(payload)
}

pub async fn complete_via_tool(
    ctx: &ToolContext,
    system_prompt: &str,
    user_prompt: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<String, super::super::provider::ToolResult> {
    let provider = ctx
        .provider
        .as_deref()
        .ok_or_else(|| common::err("no chat provider available"))?;
    let model = ctx
        .chat_model
        .as_deref()
        .ok_or_else(|| common::err("no chat model configured"))?;

    let out = complete_text(
        provider,
        CompleteTextRequest {
            model,
            system_prompt: Some(system_prompt),
            user_prompt: &user_prompt,
            temperature,
            max_tokens,
            cancel: Some(ctx.cancel.as_ref()),
        },
    )
    .await
    .map_err(|e| common::err(format!("provider error: {e}")))?;

    if ctx.cancel.load(Ordering::Relaxed) || out.cancelled {
        return Err(common::err("cancelled"));
    }
    if out.reply.trim().is_empty() {
        return Err(common::err("provider returned an empty reply"));
    }
    Ok(out.reply)
}

pub fn read_note_body(
    ctx: &ToolContext,
    rel_path: &str,
) -> Result<String, super::super::provider::ToolResult> {
    validate_rel_path(rel_path)?;
    let vault = ctx
        .vault_root
        .as_ref()
        .ok_or_else(|| common::err("no vault available (vault not opened?)"))?;
    let abs = vault.join(rel_path);
    fs::read_to_string(&abs)
        .map_err(|e| common::err(format!("failed to read '{rel_path}': {e}")))
}

pub fn write_target_exists(
    ctx: &ToolContext,
    rel_path: &str,
) -> Result<bool, super::super::provider::ToolResult> {
    validate_rel_path(rel_path)?;
    let vault = ctx
        .vault_root
        .as_ref()
        .ok_or_else(|| common::err("no vault available (vault not opened?)"))?;
    Ok(vault.join(rel_path).exists())
}

pub fn validate_rel_path(
    rel_path: &str,
) -> Result<(), super::super::provider::ToolResult> {
    if rel_path.trim().is_empty() {
        return Err(common::err("path must be non-empty"));
    }
    let path = Path::new(rel_path);
    if path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(common::err(format!("path escape rejected: {rel_path}")));
    }
    Ok(())
}

pub fn split_leading_frontmatter(body: &str) -> Option<(&str, &str)> {
    let rest = body
        .strip_prefix("---\n")
        .or_else(|| body.strip_prefix("---\r\n"))?;
    let mut cursor = 0;
    while cursor < rest.len() {
        let next_nl = rest[cursor..]
            .find('\n')
            .map(|i| cursor + i)
            .unwrap_or(rest.len());
        let line = rest[cursor..next_nl].trim_end_matches('\r');
        if line == "---" {
            let fm = rest[..cursor].strip_suffix('\n').unwrap_or(&rest[..cursor]);
            let fm = fm.strip_suffix('\r').unwrap_or(fm);
            let after_start = (next_nl + 1).min(rest.len());
            return Some((fm, &rest[after_start..]));
        }
        cursor = next_nl + 1;
    }
    None
}

pub fn format_yaml_scalar(value: &str) -> String {
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

pub fn rewrite_frontmatter_scalar(body: &str, key: &str, value: &str) -> String {
    let formatted = format_yaml_scalar(value);
    let Some((fm_raw, rest)) = split_leading_frontmatter(body) else {
        let head = format!("---\n{key}: {formatted}\n---\n\n");
        return format!("{head}{}", body.trim_start());
    };

    let mut out_lines = Vec::new();
    let mut seen = false;
    for line in fm_raw.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !seen && is_key_line(line, key) {
            out_lines.push(format!("{key}: {formatted}"));
            seen = true;
        } else {
            out_lines.push(line.to_string());
        }
    }
    if !seen {
        out_lines.push(format!("{key}: {formatted}"));
    }

    let trimmed_rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .unwrap_or(rest);
    format!("---\n{}\n---\n\n{trimmed_rest}", out_lines.join("\n"))
}

fn is_key_line(line: &str, key: &str) -> bool {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix(key) {
        let rest = rest.trim_start();
        return rest.starts_with(':');
    }
    false
}

pub fn insert_tldr_at_top(body: &str, summary: &str) -> String {
    let block = format!("> **TL;DR** {}\n\n", summary.trim().replace('\n', " "));
    if let Some((fm_raw, rest)) = split_leading_frontmatter(body) {
        let rest = rest.trim_start_matches(['\n', '\r']);
        return format!("---\n{fm_raw}\n---\n\n{block}{rest}");
    }
    format!("{block}{}", body.trim_start_matches(['\n', '\r']))
}

pub fn parse_existing_tags(body: &str) -> Vec<String> {
    let Some((fm_raw, _)) = split_leading_frontmatter(body) else {
        return Vec::new();
    };
    if let Some(flow) = fm_raw
        .lines()
        .find_map(|line| line.trim().strip_prefix("tags: [").map(|rest| rest.to_string()))
    {
        let flow = flow.trim_end_matches(']').trim();
        return flow
            .split(',')
            .filter_map(normalize_tag)
            .collect();
    }

    let lines: Vec<&str> = fm_raw.lines().collect();
    if let Some(idx) = lines.iter().position(|line| line.trim() == "tags:") {
        let mut out = Vec::new();
        for line in &lines[(idx + 1)..] {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("- ") {
                if let Some(tag) = normalize_tag(rest.trim_matches('"').trim_matches('\'')) {
                    out.push(tag);
                }
            } else {
                break;
            }
        }
        if !out.is_empty() {
            return out;
        }
    }

    if let Some(line) = lines.iter().find(|line| line.trim_start().starts_with("tags:")) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("tags:") {
            return rest
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter_map(normalize_tag)
                .collect();
        }
    }

    Vec::new()
}

pub fn parse_suggested_tags(reply: &str) -> Vec<String> {
    let trimmed = reply.trim();
    let mut candidates: Vec<String> = Vec::new();
    if trimmed.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Vec<Value>>(trimmed) {
            candidates = parsed.into_iter().map(|v| v.to_string()).collect();
        }
    }
    if candidates.is_empty() {
        let stripped = trimmed
            .lines()
            .map(|line| line.trim_start_matches(['-', '*', '•']).trim())
            .collect::<Vec<_>>()
            .join("\n");
        if stripped.contains('#') {
            candidates = stripped
                .split(|c| matches!(c, '#' | ',' | ';' | '\n'))
                .map(|s| s.to_string())
                .collect();
        } else {
            candidates = stripped
                .split(|c| matches!(c, ',' | ';' | '\n'))
                .map(|s| s.to_string())
                .collect();
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for raw in candidates {
        let raw = raw.trim_matches('"').trim_matches('\'');
        if let Some(tag) = normalize_tag(raw) {
            if seen.insert(tag.clone()) {
                out.push(tag);
            }
        }
    }
    out
}

pub fn merge_tags_into_frontmatter(body: &str, new_tags: &[String]) -> String {
    let tags_line = format!("tags: [{}]", new_tags.join(", "));
    let Some((fm_raw, rest)) = split_leading_frontmatter(body) else {
        let prefix = format!("---\n{tags_line}\n---\n\n");
        return prefix + body.trim_start();
    };

    let src_lines: Vec<&str> = fm_raw.split('\n').collect();
    let mut out_lines = Vec::new();
    let mut replaced = false;
    let mut i = 0;
    while i < src_lines.len() {
        let line = src_lines[i].trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.starts_with("tags: [") || (trimmed.starts_with("tags:") && trimmed != "tags:") {
            if !replaced {
                out_lines.push(tags_line.clone());
                replaced = true;
            }
            i += 1;
            continue;
        }
        if trimmed == "tags:" {
            if !replaced {
                out_lines.push(tags_line.clone());
                replaced = true;
            }
            i += 1;
            while i < src_lines.len() && src_lines[i].trim_start().starts_with("- ") {
                i += 1;
            }
            continue;
        }
        out_lines.push(line.to_string());
        i += 1;
    }
    if !replaced {
        out_lines.push(tags_line);
    }

    let trimmed_rest = rest.trim_start_matches(['\n', '\r']);
    format!("---\n{}\n---\n\n{trimmed_rest}", out_lines.join("\n"))
}

fn normalize_tag(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('#').to_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    let dashed = trimmed.split_whitespace().collect::<Vec<_>>().join("-");
    let cleaned: String = dashed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || is_cjk(*c))
        .collect();
    if cleaned.is_empty() || cleaned.len() > 40 || cleaned.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(cleaned)
}

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

pub fn load_moc_template(ctx: &ToolContext) -> Result<String, super::super::provider::ToolResult> {
    let vault = ctx
        .vault_root
        .as_ref()
        .ok_or_else(|| common::err("no vault available (vault not opened?)"))?;
    let candidate = vault.join("templates").join("moc.md");
    match fs::read_to_string(&candidate) {
        Ok(content) => Ok(content),
        Err(_) => Ok(BUNDLED_MOC_TEMPLATE.to_string()),
    }
}

pub fn build_flat_entries_markdown(notes: &[NoteRef]) -> String {
    notes.iter()
        .map(|note| format!("- [[{}]]", note.title.clone().unwrap_or_else(|| stem_from_path(&note.path))))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn inject_moc_entries(body: &str, entries_markdown: &str) -> (String, String) {
    if entries_markdown.trim().is_empty() {
        return (body.to_string(), "none".into());
    }
    const SENTINEL: &str = "<!-- moc:entries-insertion-point -->";
    if body.contains(SENTINEL) {
        return (
            body.replacen(SENTINEL, &format!("{entries_markdown}\n"), 1),
            "sentinel".into(),
        );
    }
    const LEGACY: &str = "## 核心笔记\n\n- [[]]";
    if body.contains(LEGACY) {
        return (
            body.replacen(LEGACY, &format!("## 核心笔记\n\n{entries_markdown}"), 1),
            "legacy".into(),
        );
    }

    let mut out = body.trim_end().to_string();
    out.push_str("\n\n## 核心笔记\n\n");
    out.push_str(entries_markdown);
    out.push('\n');
    (out, "appended".into())
}

pub fn sanitize_moc_reply(reply: &str, allowed_titles: &[String]) -> String {
    let allowed_set: std::collections::BTreeSet<String> =
        allowed_titles.iter().cloned().collect();
    let mut text = reply.trim().to_string();
    if let Some(stripped) = strip_fenced_block(&text) {
        text = stripped;
    }
    if !text.starts_with("## ") {
        if let Some(idx) = text.find("## ") {
            text = text[idx..].to_string();
        }
    }

    let mut linked_titles = std::collections::BTreeSet::new();
    let mut out_lines = Vec::new();
    for line in text.lines().take(200) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out_lines.push(String::new());
            continue;
        }
        if trimmed.starts_with("## ") {
            out_lines.push(trimmed.to_string());
            continue;
        }
        if let Some(title) = extract_bullet_title(trimmed) {
            if allowed_set.contains(&title) {
                linked_titles.insert(title.clone());
                out_lines.push(format!("- [[{title}]]"));
            } else {
                out_lines.push(format!("- {title} <!-- AI 生成，非选中笔记 -->"));
            }
        }
    }

    let missing: Vec<String> = allowed_titles
        .iter()
        .filter(|title| !linked_titles.contains(*title))
        .cloned()
        .collect();
    if !missing.is_empty() {
        if !out_lines.is_empty() && !out_lines.last().is_some_and(|line| line.is_empty()) {
            out_lines.push(String::new());
        }
        out_lines.push("## 其余笔记".into());
        out_lines.push(String::new());
        for title in missing {
            out_lines.push(format!("- [[{title}]]"));
        }
    }

    dedupe_blank_lines(out_lines).join("\n").trim().to_string()
}

fn strip_fenced_block(text: &str) -> Option<String> {
    if !text.starts_with("```") || !text.ends_with("```") {
        return None;
    }
    let body = text
        .trim_start_matches("```markdown")
        .trim_start_matches("```md")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();
    Some(body)
}

fn extract_bullet_title(line: &str) -> Option<String> {
    let bullet = line.strip_prefix("- ")?;
    let inner = bullet
        .strip_prefix("[[")?
        .strip_suffix("]]")?
        .split('|')
        .next()?
        .trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

fn dedupe_blank_lines(lines: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut blank_run = 0;
    for line in lines {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push(String::new());
            }
        } else {
            blank_run = 0;
            out.push(line);
        }
    }
    out
}

pub fn stem_from_path(path: &str) -> String {
    path.trim_end_matches(".md")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

pub fn slugify_title(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in input.trim().chars() {
        let mapped = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if is_cjk(c) {
            Some(c)
        } else {
            None
        };

        match mapped {
            Some(ch) => {
                out.push(ch);
                last_dash = false;
            }
            None if !last_dash && !out.is_empty() => {
                out.push('-');
                last_dash = true;
            }
            None => {}
        }
    }
    out.trim_matches('-').to_string()
}

pub fn note_edit_too_large(body: &str) -> bool {
    body.len() > 28_000
}

pub fn summary_target_from_args(args: &Value) -> String {
    args.get("target")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| matches!(s.as_str(), "frontmatter" | "top"))
        .unwrap_or_else(|| "frontmatter".into())
}
