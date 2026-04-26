use std::collections::BTreeMap;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::services::config::{AiToolPermissions, AppConfigSnapshot};
use crate::AppState;

#[tauri::command]
pub fn app_config_get(state: State<AppState>) -> AppResult<AppConfigSnapshot> {
    let cfg = state.config.lock().unwrap();
    Ok(cfg.snapshot())
}

#[tauri::command]
pub fn app_config_set_theme(theme: String, state: State<AppState>) -> AppResult<AppConfigSnapshot> {
    if !matches!(theme.as_str(), "system" | "light" | "dark") {
        return Err(AppError::Other(format!("invalid theme: {theme}")));
    }

    let mut cfg = state.config.lock().unwrap();
    cfg.set_theme(theme)?;
    Ok(cfg.snapshot())
}

#[tauri::command]
pub fn app_config_set_autosave_ms(
    autosave_ms: u32,
    state: State<AppState>,
) -> AppResult<AppConfigSnapshot> {
    if !(100..=5000).contains(&autosave_ms) {
        return Err(AppError::Other(format!(
            "autosave_ms out of range: {autosave_ms}"
        )));
    }

    let mut cfg = state.config.lock().unwrap();
    cfg.set_autosave_ms(autosave_ms)?;
    Ok(cfg.snapshot())
}

#[tauri::command]
pub fn app_config_set_shortcuts(
    shortcuts: BTreeMap<String, String>,
    state: State<AppState>,
) -> AppResult<AppConfigSnapshot> {
    let mut cfg = state.config.lock().unwrap();
    cfg.set_shortcuts(shortcuts)?;
    Ok(cfg.snapshot())
}

#[tauri::command]
pub fn app_config_set_ai_enabled(
    enabled: bool,
    state: State<AppState>,
) -> AppResult<AppConfigSnapshot> {
    let mut cfg = state.config.lock().unwrap();
    cfg.set_ai_enabled(enabled)?;
    Ok(cfg.snapshot())
}

#[tauri::command]
pub fn app_config_set_ai_tool_permissions(
    allow_readonly: bool,
    allow_writeback: bool,
    allow_destructive: bool,
    state: State<AppState>,
) -> AppResult<AppConfigSnapshot> {
    let mut cfg = state.config.lock().unwrap();
    cfg.set_ai_tool_permissions(AiToolPermissions {
        allow_readonly,
        allow_writeback,
        allow_destructive,
    })?;
    Ok(cfg.snapshot())
}
