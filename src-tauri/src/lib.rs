mod commands;
mod db;
mod error;
mod services;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use services::ai::embedding_store::EmbeddingStore;
use services::ai::tool_registry::ToolRegistry;
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
    pub config: Arc<Mutex<ConfigStore>>,
    pub active_vault: Mutex<Option<PathBuf>>,
    pub index: Mutex<Option<Arc<Mutex<Connection>>>>,
    pub watcher: Mutex<Option<WatcherHandle>>,
    /// Per-vault AI embedding store at `.mynotes/ai/embeddings.sqlite`.
    /// `None` until a vault is opened; swapped out on vault switch.
    /// Separate from `index` so wiping AI state never touches the
    /// primary SQLite index.
    pub embeddings: Mutex<Option<Arc<Mutex<EmbeddingStore>>>>,
    /// Registry of in-flight streaming chat IPCs (D2b.4). Keyed by the
    /// frontend-generated `stream_id`; each entry is a cancel flag the
    /// spawned streaming task polls between token deltas. Removed on
    /// terminal event (done / cancelled / error). Wrapped in `Arc` so
    /// the spawned task can own a clone rather than borrow through
    /// `State<AppState>` across awaits (which would need the state to
    /// live for `'static`).
    pub chat_streams: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// Registry of in-flight single-shot `ai_complete` requests (D3.1).
    /// Same shape as [`chat_streams`] but semantically distinct: these
    /// back the write-back commands (summarize / suggest tags / MOC AI
    /// draft) whose results never hit the chat-session jsonl store.
    /// Separate map so a cancel for one side can never accidentally
    /// abort an in-flight request on the other.
    pub complete_requests: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// Agentic-chat tool registry (P3-D5.1). Shared `Arc` so the
    /// spawned streaming task in `ai_chat_stream_start` can own a
    /// clone without borrowing through `State<AppState>` across
    /// awaits. D5.1 bootstraps it empty — the multi-turn chat loop
    /// exercises the end-to-end "tool not registered" path. D5.2+
    /// registers real tools at app setup.
    pub tool_registry: Arc<ToolRegistry>,
    pub app_support_dir: PathBuf,
}

impl AppState {
    /// Cheap clone-of-Arc — caller can Mutex::lock it without holding our outer lock.
    pub fn index_handle(&self) -> Option<Arc<Mutex<Connection>>> {
        self.index.lock().unwrap().as_ref().cloned()
    }

    /// Cheap clone-of-Arc for the embedding store. `None` when no vault
    /// is open or when the store failed to initialize.
    pub fn embeddings_handle(&self) -> Option<Arc<Mutex<EmbeddingStore>>> {
        self.embeddings.lock().unwrap().as_ref().cloned()
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
            // Pre-build the registry with the D5.2 read-only tool set.
            // Tools are zero-sized unit structs; registration is a
            // one-shot boot-time HashMap insert. We do this *before*
            // `app.manage(..)` to dodge the chicken-and-egg of mutating
            // the state after it's been moved into the handle.
            let tool_registry = {
                let mut reg = ToolRegistry::new();
                services::ai::tools::register_readonly_tools(&mut reg);
                services::ai::tools::register_writeback_tools(&mut reg);
                services::ai::tools::register_destructive_tools(&mut reg);
                Arc::new(reg)
            };
            app.manage(AppState {
                config: Arc::new(Mutex::new(config)),
                active_vault: Mutex::new(None),
                index: Mutex::new(None),
                watcher: Mutex::new(None),
                embeddings: Mutex::new(None),
                chat_streams: Arc::new(Mutex::new(HashMap::new())),
                complete_requests: Arc::new(Mutex::new(HashMap::new())),
                tool_registry,
                app_support_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::app_config_get,
            commands::config::app_config_set_theme,
            commands::config::app_config_set_autosave_ms,
            commands::config::app_config_set_shortcuts,
            commands::config::app_config_set_ai_enabled,
            commands::config::app_config_set_ai_tool_permissions,
            commands::ai::ai_related_notes,
            commands::ai::ai_provider_set_config,
            commands::ai::ai_provider_clear_config,
            commands::ai::ai_provider_has_api_key,
            commands::ai::ai_provider_test_connection,
            commands::ai::ai_provider_test_chat_connection,
            commands::ai::ai_embed_note,
            commands::ai::ai_embed_stats,
            commands::ai::ai_embed_delete_note,
            commands::ai::ai_embed_clear_all,
            commands::ai::ai_embed_vault_preview,
            commands::ai::ai_embed_vault_run,
            commands::ai::ai_chat_session_list,
            commands::ai::ai_chat_session_create,
            commands::ai::ai_chat_session_load,
            commands::ai::ai_chat_session_append,
            commands::ai::ai_chat_session_delete,
            commands::ai::ai_chat_send,
            commands::ai::ai_chat_stream_start,
            commands::ai::ai_chat_stream_cancel,
            commands::ai::ai_complete,
            commands::ai::ai_complete_cancel,
            commands::ai::ai_record_proposal_resolution,
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
            commands::file::path_reveal,
            commands::index::index_backlinks,
            commands::index::index_outgoing,
            commands::index::index_unresolved,
            commands::index::index_tags,
            commands::index::index_notes_by_tag,
            commands::index::index_notes_by_tags,
            commands::index::index_all_notes,
            commands::index::index_inbox_list,
            commands::index::index_unresolved_count,
            commands::index::index_projects_by_status,
            commands::index::index_project_notes,
            commands::index::index_search,
            commands::index::index_resolve_wiki_link,
            commands::index::index_tasks_today,
            commands::index::index_tasks_upcoming,
            commands::index::index_tasks_count,
            commands::index::toggle_task_done,
            commands::project::project_set_status,
            commands::attachment::attachment_save,
            commands::attachment::attachment_read_bytes,
            commands::attachment::attachment_read_external_bytes,
            commands::attachment::attachment_list,
            commands::attachment::attachment_unreferenced,
            commands::attachment::attachment_delete_batch,
            commands::rename::file_move_with_refs_preview,
            commands::rename::file_move_with_refs,
            commands::rename::dir_move_with_refs_preview,
            commands::rename::dir_move_with_refs,
            commands::graph::index_graph,
            commands::import::file_import,
            commands::export::vault_export_zip,
            commands::export::note_export_copy,
            commands::export::note_render_print_html,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
