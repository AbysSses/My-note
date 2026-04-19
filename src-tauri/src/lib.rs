mod commands;
mod db;
mod error;
mod services;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use services::config::ConfigStore;
use services::watcher::WatcherHandle;
use tauri::Manager;

/// Global app state.
///
/// - `config` — persisted across launches (recent vaults, future prefs).
/// - `active_vault` — path of the currently open vault, or None.
/// - `index` — per-vault SQLite connection. Set by vault_open / vault_init
///   after a full scan. Wrapped in `Arc<Mutex<_>>` so background tasks
///   (file watcher) can share it without holding the outer Mutex.
/// - `app_support_dir` — captured at setup-time because `app.path()` isn't
///   reachable from plain `#[tauri::command]` fns.
pub struct AppState {
    pub config: Mutex<ConfigStore>,
    pub active_vault: Mutex<Option<PathBuf>>,
    pub index: Mutex<Option<Arc<Mutex<Connection>>>>,
    pub watcher: Mutex<Option<WatcherHandle>>,
    pub app_support_dir: PathBuf,
}

impl AppState {
    /// Cheap clone-of-Arc — caller can Mutex::lock it without holding our outer lock.
    pub fn index_handle(&self) -> Option<Arc<Mutex<Connection>>> {
        self.index.lock().unwrap().as_ref().cloned()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("failed to resolve app_config_dir");
            let app_support_dir = app
                .path()
                .app_data_dir()
                .or_else(|_| app.path().app_config_dir())
                .expect("failed to resolve app_data_dir");
            let config = ConfigStore::load_or_init(&config_dir)?;
            app.manage(AppState {
                config: Mutex::new(config),
                active_vault: Mutex::new(None),
                index: Mutex::new(None),
                watcher: Mutex::new(None),
                app_support_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_init,
            commands::vault::vault_open,
            commands::vault::vault_recent,
            commands::vault::vault_is_initialized,
            commands::vault::vault_reseed_templates,
            commands::file::file_read,
            commands::file::file_write,
            commands::file::file_list,
            commands::file::file_exists,
            commands::file::file_move,
            commands::file::file_delete,
            commands::index::index_backlinks,
            commands::index::index_outgoing,
            commands::index::index_unresolved,
            commands::index::index_tags,
            commands::index::index_notes_by_tag,
            commands::index::index_all_notes,
            commands::index::index_inbox_list,
            commands::index::index_unresolved_count,
            commands::index::index_projects_by_status,
            commands::index::index_project_notes,
            commands::index::index_search,
            commands::project::project_set_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
