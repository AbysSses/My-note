//! IPC command that exposes the full note-graph (nodes + resolved edges) to
//! the frontend so it can run a force-directed layout and render a canvas
//! visualization.
//!
//! We return the *entire* graph in one shot — for a personal vault (few k
//! notes, tens of k edges) the payload is comfortably <1 MB and trivially
//! serializable as JSON. The frontend is responsible for:
//!   * local-mode pruning (BFS from a seed path, N hops)
//!   * type/tag filtering
//!   * force simulation + canvas draw
//!
//! Unresolved links (`dst_resolved IS NULL`) are *not* returned — they have
//! no destination node and would just clutter the view. Backlinks panel +
//! "Unresolved links" command already surface them separately.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::map_sql_err;
use crate::error::{AppError, AppResult};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct GraphNode {
    pub path: String,
    pub title: Option<String>,
    /// Matches `notes.type` — one of `note / moc / daily / weekly / project /
    /// project-note / inbox`. Frontend colours nodes by this.
    pub note_type: Option<String>,
    /// Number of resolved links pointing *at* this note.
    pub in_degree: i64,
    /// Number of resolved links leaving this note.
    pub out_degree: i64,
}

#[derive(Debug, Serialize)]
pub struct GraphEdge {
    pub src: String,
    /// Always set — we only return resolved edges.
    pub dst: String,
    /// `wiki` / `markdown` / `embed`.
    pub link_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

fn with_conn<F, R>(state: &State<AppState>, f: F) -> AppResult<R>
where
    F: FnOnce(&Connection) -> AppResult<R>,
{
    let handle = state.index_handle().ok_or(AppError::NoActiveVault)?;
    let conn = handle.lock().unwrap();
    f(&conn)
}

/// Build and return the complete resolved note-graph.
///
/// Three reads against the index, all cheap:
///   1. `SELECT path, title, type FROM notes` — one row per node.
///   2. `SELECT src, dst_resolved, link_type FROM links WHERE dst_resolved IS NOT NULL`
///      — one row per resolved edge; also drives the degree counts.
///
/// Degree counts are computed in Rust rather than in SQL so we only touch
/// the `links` table once.
#[tauri::command]
pub fn index_graph(state: State<AppState>) -> AppResult<GraphData> {
    with_conn(&state, |conn| build_graph(conn))
}

fn build_graph(conn: &Connection) -> AppResult<GraphData> {
    // 1) Nodes.
    let mut node_stmt = conn
        .prepare("SELECT path, title, type FROM notes")
        .map_err(map_sql_err)?;
    let node_rows = node_stmt
        .query_map([], |row| {
            let path: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            let note_type: Option<String> = row.get(2)?;
            Ok((path, title, note_type))
        })
        .map_err(map_sql_err)?;

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut node_idx: HashMap<String, usize> = HashMap::new();
    for row in node_rows {
        let (path, title, note_type) = row.map_err(map_sql_err)?;
        node_idx.insert(path.clone(), nodes.len());
        nodes.push(GraphNode {
            path,
            title,
            note_type,
            in_degree: 0,
            out_degree: 0,
        });
    }

    // 2) Resolved edges + degree accumulation.
    let mut edge_stmt = conn
        .prepare(
            "SELECT src, dst_resolved, link_type
             FROM links
             WHERE dst_resolved IS NOT NULL",
        )
        .map_err(map_sql_err)?;
    let edge_rows = edge_stmt
        .query_map([], |row| {
            let src: String = row.get(0)?;
            let dst: String = row.get(1)?;
            let link_type: Option<String> = row.get(2)?;
            Ok((src, dst, link_type))
        })
        .map_err(map_sql_err)?;

    let mut edges: Vec<GraphEdge> = Vec::new();
    for row in edge_rows {
        let (src, dst, link_type) = row.map_err(map_sql_err)?;
        // Skip the rare case where `src` or `dst_resolved` points at a path
        // we don't have a node for (e.g. a link whose target was just
        // renamed but the incremental index hasn't caught up yet). Without
        // the node the frontend can't render the endpoint anyway.
        //
        // Resolve BOTH endpoints before mutating any degree counter —
        // otherwise a dangling destination would still bump src's
        // out_degree, corrupting the visible node metadata.
        let src_idx = match node_idx.get(&src) {
            Some(&i) => i,
            None => continue,
        };
        let dst_idx = match node_idx.get(&dst) {
            Some(&i) => i,
            None => continue,
        };
        nodes[src_idx].out_degree += 1;
        nodes[dst_idx].in_degree += 1;
        edges.push(GraphEdge {
            src,
            dst,
            link_type,
        });
    }

    Ok(GraphData { nodes, edges })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Helper — spin up an in-memory DB with the real schema and a tiny
    /// fixture graph: A → B, A → C, B → C. No dangling edges.
    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../db/schema.sql"))
            .unwrap();
        for (path, title, typ) in [
            ("1-notes/a.md", "A", "note"),
            ("1-notes/b.md", "B", "note"),
            ("2-moc/c.md", "C", "moc"),
        ] {
            conn.execute(
                "INSERT INTO notes(path, title, type) VALUES(?1, ?2, ?3)",
                rusqlite::params![path, title, typ],
            )
            .unwrap();
        }
        for (src, dst, ltype) in [
            ("1-notes/a.md", "1-notes/b.md", "wiki"),
            ("1-notes/a.md", "2-moc/c.md", "wiki"),
            ("1-notes/b.md", "2-moc/c.md", "embed"),
        ] {
            conn.execute(
                "INSERT INTO links(src, dst, dst_resolved, link_type, position)
                 VALUES(?1, ?2, ?2, ?3, 0)",
                rusqlite::params![src, dst, ltype],
            )
            .unwrap();
        }
        // One unresolved link — should NOT appear in edges.
        conn.execute(
            "INSERT INTO links(src, dst, dst_resolved, link_type, position)
             VALUES('1-notes/a.md', 'ghost', NULL, 'wiki', 0)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn graph_contains_all_nodes_with_correct_degrees() {
        let conn = fixture();
        let g = build_graph(&conn).unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 3);

        let by_path: HashMap<_, _> = g.nodes.iter().map(|n| (n.path.as_str(), n)).collect();
        assert_eq!(by_path["1-notes/a.md"].out_degree, 2);
        assert_eq!(by_path["1-notes/a.md"].in_degree, 0);
        assert_eq!(by_path["1-notes/b.md"].out_degree, 1);
        assert_eq!(by_path["1-notes/b.md"].in_degree, 1);
        assert_eq!(by_path["2-moc/c.md"].out_degree, 0);
        assert_eq!(by_path["2-moc/c.md"].in_degree, 2);
    }

    #[test]
    fn unresolved_edges_are_dropped() {
        let conn = fixture();
        let g = build_graph(&conn).unwrap();
        assert!(g.edges.iter().all(|e| e.dst != "ghost"));
    }

    #[test]
    fn dangling_edge_to_missing_node_is_skipped() {
        let conn = fixture();
        // Edge whose src/dst isn't in `notes` — simulates index lag.
        conn.execute(
            "INSERT INTO notes(path, title, type) VALUES('stale', 'Stale', 'note')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO links(src, dst, dst_resolved, link_type, position)
             VALUES('stale', 'vanished', 'vanished', 'wiki', 0)",
            [],
        )
        .unwrap();
        let g = build_graph(&conn).unwrap();
        // "stale" → "vanished" should be skipped (no node for `vanished`).
        assert!(g
            .edges
            .iter()
            .all(|e| !(e.src == "stale" && e.dst == "vanished")));
        // And "stale" itself has out_degree 0 because the edge was dropped.
        let stale = g.nodes.iter().find(|n| n.path == "stale").unwrap();
        assert_eq!(stale.out_degree, 0);
    }
}
