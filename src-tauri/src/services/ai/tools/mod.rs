//! Built-in tools for the agentic chat loop (P3-D5.2).
//!
//! This module ships the first batch of **🟢 read-only** tools the chat
//! loop exposes to the model. Each tool:
//!
//! - Lives in its own file (`search_by_tag.rs`, `search_fulltext.rs`, …).
//! - Implements [`super::tool_registry::Tool`].
//! - Returns a stable JSON string in `ToolResult.content` — the frontend
//!   and model both consume it.
//! - Never mutates the vault or index. 🟡 `propose_*` and 🔴 `delete_*`
//!   tools land in D5.4.
//!
//! ## Naming convention
//!
//! - Read-only: `search_*` / `list_*` / `read_*` / `get_*`.
//! - Write-back (D5.4): `propose_*`.
//! - Destructive (D5.7): `delete_*` / `rename_*`.
//!
//! ## Registration
//!
//! `setup()` in `lib.rs` calls [`register_readonly_tools`] on a fresh
//! `ToolRegistry` **before** wrapping it in `Arc` and stashing it in
//! `AppState`. Tools are zero-sized unit structs, so this is cheap.
//!
//! ## What each tool needs from [`ToolContext`]
//!
//! | tool | vault_root | index | embeddings | embed_model |
//! |---|---|---|---|---|
//! | search_by_tag      | –    | ✓   | –   | –   |
//! | search_fulltext    | –    | ✓   | –   | –   |
//! | list_tags          | –    | ✓   | –   | –   |
//! | read_note          | ✓    | –   | –   | –   |
//! | get_related_notes  | –    | ✓   | opt | opt |
//!
//! Tools gracefully degrade: e.g. `get_related_notes` runs without an
//! embedding store, just with embedding_cosine = 0 for every candidate.

pub(crate) mod common;

pub mod delete_note;
pub mod get_related_notes;
pub mod list_tags;
pub mod propose_moc;
pub mod propose_note_edit;
pub mod propose_summary;
pub mod propose_tag_update;
pub mod read_note;
pub mod rename_note;
pub mod search_by_tag;
pub mod search_fulltext;
pub mod writeback_support;

pub use delete_note::DeleteNoteTool;
pub use get_related_notes::GetRelatedNotesTool;
pub use list_tags::ListTagsTool;
pub use propose_moc::ProposeMocTool;
pub use propose_note_edit::ProposeNoteEditTool;
pub use propose_summary::ProposeSummaryTool;
pub use propose_tag_update::ProposeTagUpdateTool;
pub use read_note::ReadNoteTool;
pub use rename_note::RenameNoteTool;
pub use search_by_tag::SearchByTagTool;
pub use search_fulltext::SearchFulltextTool;

use std::sync::Arc;

use super::tool_registry::ToolRegistry;

/// Register the D5.2 read-only tool set on an empty registry. Called
/// once in `lib.rs::setup()` before the registry is wrapped in `Arc`
/// and stashed in `AppState`.
pub fn register_readonly_tools(reg: &mut ToolRegistry) {
    reg.register(Arc::new(SearchByTagTool));
    reg.register(Arc::new(SearchFulltextTool));
    reg.register(Arc::new(ListTagsTool));
    reg.register(Arc::new(ReadNoteTool));
    reg.register(Arc::new(GetRelatedNotesTool));
}

pub fn register_writeback_tools(reg: &mut ToolRegistry) {
    reg.register(Arc::new(ProposeSummaryTool));
    reg.register(Arc::new(ProposeTagUpdateTool));
    reg.register(Arc::new(ProposeMocTool));
    reg.register(Arc::new(ProposeNoteEditTool));
}

pub fn register_destructive_tools(reg: &mut ToolRegistry) {
    reg.register(Arc::new(DeleteNoteTool));
    reg.register(Arc::new(RenameNoteTool));
}
