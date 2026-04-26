use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::AppResult;

fn ai_dir(vault_root: &Path) -> PathBuf {
    vault_root.join(".mynotes").join("ai")
}

fn append_jsonl(path: &Path, value: &impl Serialize) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(value)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

pub fn append_usage_log(vault_root: &Path, value: &impl Serialize) -> AppResult<()> {
    append_jsonl(&ai_dir(vault_root).join("usage.log"), value)
}

pub fn append_audit_log(vault_root: &Path, value: &impl Serialize) -> AppResult<()> {
    append_jsonl(&ai_dir(vault_root).join("audit.log"), value)
}
