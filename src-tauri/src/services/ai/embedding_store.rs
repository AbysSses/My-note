//! Embedding storage — Phase 3-D2a.
//!
//! Stores chunk-level embeddings in a **dedicated SQLite file** at
//! `<vault>/.mynotes/ai/embeddings.sqlite`, separate from `index.sqlite`.
//!
//! ## Why a separate file
//!
//! - **Non-disruptive migration**: users of older versions can delete the
//!   whole `.mynotes/ai/` directory and "factory-reset" AI state without
//!   losing the primary index.
//! - **No schema coupling**: `index.sqlite` stays a pure derivative of the
//!   markdown tree; the embedding DB is a derivative of the AI pipeline and
//!   may churn its schema independently.
//! - **WAL isolation**: a long embed batch cannot block frontend writes to
//!   the main index.
//!
//! ## Vector storage format
//!
//! Vectors are stored as **little-endian packed `f32` BLOBs** (`dim * 4`
//! bytes). Search is a full-table cosine-similarity scan in memory — fine
//! for vaults under ~50 k chunks (< 50 ms on a modern laptop). When vaults
//! grow past that threshold, the search() implementation can be swapped
//! for sqlite-vec or an ANN index without changing the schema.
//!
//! ## Dimension invariant
//!
//! All chunks with the same `model` must have the same `dim`. `search()`
//! rejects queries whose dimension doesn't match the requested model.
//!
//! ## Consumer status
//!
//! As of D2a.2 this module is still library-only — consumers land in D2a.3
//! (embed-note IPC + filewatcher increment). Module-scoped `allow(dead_code)`
//! suppresses the "no users yet" warnings until then.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{AppError, AppResult};

// ── Schema ────────────────────────────────────────────────────────────────────

const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS embedding_chunks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    note_rel_path TEXT    NOT NULL,
    chunk_index   INTEGER NOT NULL,
    offset_start  INTEGER NOT NULL,
    offset_end    INTEGER NOT NULL,
    text          TEXT    NOT NULL,
    model         TEXT    NOT NULL,
    dim           INTEGER NOT NULL,
    vector        BLOB    NOT NULL,  -- little-endian f32 × dim
    note_mtime    INTEGER NOT NULL,  -- seconds since epoch; used for incremental
    created_at    INTEGER NOT NULL,  -- seconds since epoch
    UNIQUE(note_rel_path, chunk_index, model)
);
CREATE INDEX IF NOT EXISTS idx_emb_note  ON embedding_chunks(note_rel_path);
CREATE INDEX IF NOT EXISTS idx_emb_model ON embedding_chunks(model);

-- Keyed schema / metadata; future migrations read `schema_version` here.
CREATE TABLE IF NOT EXISTS embedding_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT OR IGNORE INTO embedding_meta(key, value) VALUES('schema_version', '1');
"#;

// ── Public types ──────────────────────────────────────────────────────────────

/// A chunk ready to be persisted — vector comes from the provider, everything
/// else from [`super::chunker::Chunk`].
#[derive(Debug, Clone)]
pub struct StoredChunk {
    pub note_rel_path: String,
    pub chunk_index: u32,
    pub offset_start: u32,
    pub offset_end: u32,
    pub text: String,
    pub model: String,
    pub vector: Vec<f32>,
    pub note_mtime: i64,
}

/// One search hit — the caller receives chunks ranked by descending cosine.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub note_rel_path: String,
    pub chunk_index: u32,
    pub offset_start: u32,
    pub offset_end: u32,
    pub text: String,
    pub score: f32,
}

/// Aggregate counters surfaced by [`EmbeddingStore::stats`]. Used by the
/// Settings "AI 辅助" section to show "N chunks across M notes".
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingStats {
    pub chunk_count: u64,
    pub note_count: u64,
    pub model_count: u64,
}

// ── Store ─────────────────────────────────────────────────────────────────────

/// Thin wrapper over `rusqlite::Connection` with all schema bootstrapping
/// and vector packing/unpacking inside.
pub struct EmbeddingStore {
    conn: Connection,
}

impl EmbeddingStore {
    /// Open or create the store at `path`. Parent directories are created if
    /// missing. Idempotent — existing tables are left alone.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { conn })
    }

    /// Open an in-memory store. Primarily for unit tests — the path-based
    /// constructor is what production code uses.
    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(Self { conn })
    }

    /// Insert or replace a batch of chunks atomically.
    ///
    /// Returns the number of rows written. The `(note_rel_path, chunk_index,
    /// model)` tuple is the unique key — re-indexing a note with the same
    /// chunk layout overwrites rows in place.
    pub fn upsert_chunks(&mut self, chunks: &[StoredChunk]) -> AppResult<usize> {
        if chunks.is_empty() {
            return Ok(0);
        }
        let now = unix_secs_now();
        let tx = self.conn.transaction()?;
        let written = {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO embedding_chunks
                 (note_rel_path, chunk_index, offset_start, offset_end,
                  text, model, dim, vector, note_mtime, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(note_rel_path, chunk_index, model) DO UPDATE SET
                   offset_start = excluded.offset_start,
                   offset_end   = excluded.offset_end,
                   text         = excluded.text,
                   dim          = excluded.dim,
                   vector       = excluded.vector,
                   note_mtime   = excluded.note_mtime,
                   created_at   = excluded.created_at",
            )?;

            let mut n = 0usize;
            for c in chunks {
                let blob = pack_f32(&c.vector);
                stmt.execute(params![
                    c.note_rel_path,
                    c.chunk_index,
                    c.offset_start,
                    c.offset_end,
                    c.text,
                    c.model,
                    c.vector.len() as i64,
                    blob,
                    c.note_mtime,
                    now,
                ])?;
                n += 1;
            }
            n
        };
        tx.commit()?;
        Ok(written)
    }

    /// Atomically replace every chunk for one note with `chunks`.
    ///
    /// The delete + insert happen inside one SQLite transaction so a write
    /// failure cannot leave the note half-cleared.
    pub fn replace_note_chunks(
        &mut self,
        note_rel_path: &str,
        chunks: &[StoredChunk],
    ) -> AppResult<usize> {
        let now = unix_secs_now();
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM embedding_chunks WHERE note_rel_path = ?1",
            params![note_rel_path],
        )?;

        if chunks.is_empty() {
            tx.commit()?;
            return Ok(0);
        }

        let written = {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO embedding_chunks
                 (note_rel_path, chunk_index, offset_start, offset_end,
                  text, model, dim, vector, note_mtime, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(note_rel_path, chunk_index, model) DO UPDATE SET
                   offset_start = excluded.offset_start,
                   offset_end   = excluded.offset_end,
                   text         = excluded.text,
                   dim          = excluded.dim,
                   vector       = excluded.vector,
                   note_mtime   = excluded.note_mtime,
                   created_at   = excluded.created_at",
            )?;

            let mut n = 0usize;
            for c in chunks {
                let blob = pack_f32(&c.vector);
                stmt.execute(params![
                    c.note_rel_path,
                    c.chunk_index,
                    c.offset_start,
                    c.offset_end,
                    c.text,
                    c.model,
                    c.vector.len() as i64,
                    blob,
                    c.note_mtime,
                    now,
                ])?;
                n += 1;
            }
            n
        };
        tx.commit()?;
        Ok(written)
    }

    /// Delete every chunk belonging to a note. Returns the row count.
    /// Called when a note is deleted or moved (before re-indexing).
    pub fn delete_by_note(&self, note_rel_path: &str) -> AppResult<usize> {
        let n = self.conn.execute(
            "DELETE FROM embedding_chunks WHERE note_rel_path = ?1",
            params![note_rel_path],
        )?;
        Ok(n)
    }

    /// Wipe every chunk. Used by the Settings "清空 AI 索引" action and by
    /// migration flows that need to force a full re-embed. Non-transactional
    /// on purpose — a single `DELETE` without a `WHERE` clause is atomic
    /// at the SQLite engine level.
    pub fn clear_all(&self) -> AppResult<()> {
        self.conn.execute("DELETE FROM embedding_chunks", [])?;
        Ok(())
    }

    /// Return the most recent `note_mtime` stored for `note_rel_path`, or
    /// `None` if no chunks exist yet. The watcher compares this against
    /// filesystem mtime to decide whether to re-embed.
    pub fn note_mtime(&self, note_rel_path: &str) -> AppResult<Option<i64>> {
        let row = self
            .conn
            .query_row(
                "SELECT MAX(note_mtime) FROM embedding_chunks WHERE note_rel_path = ?1",
                params![note_rel_path],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?;
        Ok(row.flatten())
    }

    /// Return the most recent `note_mtime` for one specific `model`.
    ///
    /// This is the correct predicate for "is the current provider/model
    /// already up to date?" — otherwise switching models could incorrectly
    /// short-circuit because another model wrote the same note at the same
    /// filesystem mtime.
    pub fn note_mtime_for_model(&self, note_rel_path: &str, model: &str) -> AppResult<Option<i64>> {
        let row = self
            .conn
            .query_row(
                "SELECT MAX(note_mtime) FROM embedding_chunks
                 WHERE note_rel_path = ?1 AND model = ?2",
                params![note_rel_path, model],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?;
        Ok(row.flatten())
    }

    /// Return the sole distinct model stored in the DB, or `None` when the
    /// store is empty or currently holds multiple model namespaces.
    pub fn only_model_name(&self) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT model FROM embedding_chunks LIMIT 2")?;
        let rows: Vec<rusqlite::Result<String>> =
            stmt.query_map([], |r| r.get::<_, String>(0))?.collect();
        let mut models: Vec<String> = rows.into_iter().filter_map(|r| r.ok()).collect();
        if models.len() == 1 {
            Ok(models.pop())
        } else {
            Ok(None)
        }
    }

    /// Compute note-level cosine similarity scores under one `model`.
    ///
    /// Each note is represented by the sum of all its chunk vectors under
    /// that model. Cosines are clamped at 0 so the scoring signal stays in
    /// the same [0, 1] range as the earlier title-similarity heuristic.
    pub fn note_cosine_scores(
        &self,
        note_rel_path: &str,
        model: &str,
    ) -> AppResult<HashMap<String, f32>> {
        let mut per_note = self.note_vectors_by_note(model)?;
        let Some(src_vec) = per_note.remove(note_rel_path) else {
            return Ok(HashMap::new());
        };
        if norm(&src_vec) == 0.0 {
            return Ok(HashMap::new());
        }

        let mut scores = HashMap::with_capacity(per_note.len());
        for (path, cand_vec) in per_note {
            let score = cosine(&src_vec, &cand_vec).max(0.0);
            if score > 0.0 {
                scores.insert(path, score);
            }
        }
        Ok(scores)
    }

    /// Find the `limit` most similar chunks to `query` under the given
    /// `model` namespace. Cosine-similarity scan over every matching row.
    ///
    /// Returns hits in descending score order. An empty DB returns `Ok(vec![])`.
    pub fn search(&self, query: &[f32], model: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        if query.is_empty() {
            return Err(AppError::Other(
                "search called with empty query vector".into(),
            ));
        }
        let q_norm = norm(query);
        if q_norm == 0.0 {
            return Ok(vec![]);
        }

        let mut stmt = self.conn.prepare_cached(
            "SELECT note_rel_path, chunk_index, offset_start, offset_end,
                    text, dim, vector
             FROM embedding_chunks
             WHERE model = ?1",
        )?;
        let rows: Vec<rusqlite::Result<(String, u32, u32, u32, String, i64, Vec<u8>)>> = stmt
            .query_map(params![model], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?
            .collect();

        // Score every row in memory; use a bounded min-heap if `limit` is
        // small. For simplicity we score all rows, then partial-sort.
        let mut scored: Vec<SearchHit> = Vec::with_capacity(rows.len());
        for row in rows.into_iter().flatten() {
            let (note_rel_path, chunk_index, start, end, text, dim, blob) = row;
            if dim as usize != query.len() {
                // Dim mismatch — skip (not an error; allows mixing provider
                // namespaces under distinct `model` strings).
                continue;
            }
            let v = unpack_f32(&blob, dim as usize);
            let v_norm = norm(&v);
            if v_norm == 0.0 {
                continue;
            }
            let dot: f32 = query.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
            let score = dot / (q_norm * v_norm);
            scored.push(SearchHit {
                note_rel_path,
                chunk_index,
                offset_start: start,
                offset_end: end,
                text,
                score,
            });
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    /// Cheap aggregate stats — used by the Settings panel and by D2a.4's
    /// dry-run estimator.
    pub fn stats(&self) -> AppResult<EmbeddingStats> {
        let chunk_count: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM embedding_chunks", [], |r| r.get(0))?;
        let note_count: u64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT note_rel_path) FROM embedding_chunks",
            [],
            |r| r.get(0),
        )?;
        let model_count: u64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT model) FROM embedding_chunks",
            [],
            |r| r.get(0),
        )?;
        Ok(EmbeddingStats {
            chunk_count,
            note_count,
            model_count,
        })
    }

    fn note_vectors_by_note(&self, model: &str) -> AppResult<HashMap<String, Vec<f32>>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT note_rel_path, dim, vector
             FROM embedding_chunks
             WHERE model = ?1
             ORDER BY note_rel_path, chunk_index",
        )?;
        let rows: Vec<rusqlite::Result<(String, i64, Vec<u8>)>> = stmt
            .query_map(params![model], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect();

        let mut per_note: HashMap<String, Vec<f32>> = HashMap::new();
        for row in rows.into_iter().flatten() {
            let (note_rel_path, dim, blob) = row;
            let dim = dim.max(0) as usize;
            if dim == 0 {
                continue;
            }
            let vec = unpack_f32(&blob, dim);
            if vec.len() != dim {
                continue;
            }
            let acc = per_note
                .entry(note_rel_path)
                .or_insert_with(|| vec![0.0; dim]);
            if acc.len() != dim {
                continue;
            }
            for (dst, src) in acc.iter_mut().zip(vec.iter()) {
                *dst += *src;
            }
        }
        Ok(per_note)
    }
}

// ── Binary helpers ────────────────────────────────────────────────────────────

/// Pack `f32` vector into little-endian bytes. `dim * 4` bytes output.
fn pack_f32(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// Inverse of [`pack_f32`]. On partial blob returns as many values as fit.
fn unpack_f32(bytes: &[u8], dim: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(dim);
    for chunk in bytes.chunks_exact(4).take(dim) {
        let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
        out.push(f32::from_le_bytes(arr));
    }
    out
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let a_norm = norm(a);
    let b_norm = norm(b);
    if a_norm == 0.0 || b_norm == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    dot / (a_norm * b_norm)
}

fn unix_secs_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_chunk(note: &str, idx: u32, text: &str, model: &str, vec: Vec<f32>) -> StoredChunk {
        StoredChunk {
            note_rel_path: note.to_string(),
            chunk_index: idx,
            offset_start: 0,
            offset_end: text.len() as u32,
            text: text.to_string(),
            model: model.to_string(),
            vector: vec,
            note_mtime: 1_700_000_000,
        }
    }

    #[test]
    fn open_in_memory_creates_schema() {
        let store = EmbeddingStore::open_in_memory().unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.chunk_count, 0);
        assert_eq!(stats.note_count, 0);
        assert_eq!(stats.model_count, 0);
    }

    #[test]
    fn upsert_and_stats() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        let chunks = vec![
            sample_chunk("a.md", 0, "hello", "m1", vec![1.0, 0.0, 0.0]),
            sample_chunk("a.md", 1, "world", "m1", vec![0.0, 1.0, 0.0]),
            sample_chunk("b.md", 0, "foo", "m1", vec![0.0, 0.0, 1.0]),
        ];
        let n = store.upsert_chunks(&chunks).unwrap();
        assert_eq!(n, 3);
        let stats = store.stats().unwrap();
        assert_eq!(stats.chunk_count, 3);
        assert_eq!(stats.note_count, 2);
        assert_eq!(stats.model_count, 1);
    }

    #[test]
    fn upsert_is_idempotent_on_duplicate_key() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        let first = sample_chunk("a.md", 0, "v1", "m1", vec![1.0, 0.0]);
        let updated = sample_chunk("a.md", 0, "v2", "m1", vec![0.0, 1.0]);
        store.upsert_chunks(&[first]).unwrap();
        store.upsert_chunks(&[updated]).unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.chunk_count, 1);
        // Search for [0,1] should now score 1.0 (the updated vector).
        let hits = store.search(&[0.0, 1.0], "m1", 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert!((hits[0].score - 1.0).abs() < 1e-5);
        assert_eq!(hits[0].text, "v2");
    }

    #[test]
    fn delete_by_note_removes_all_chunks() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        store
            .upsert_chunks(&[
                sample_chunk("a.md", 0, "x", "m1", vec![1.0, 0.0]),
                sample_chunk("a.md", 1, "y", "m1", vec![0.0, 1.0]),
                sample_chunk("b.md", 0, "z", "m1", vec![0.5, 0.5]),
            ])
            .unwrap();
        let removed = store.delete_by_note("a.md").unwrap();
        assert_eq!(removed, 2);
        let stats = store.stats().unwrap();
        assert_eq!(stats.chunk_count, 1);
        assert_eq!(stats.note_count, 1);
    }

    #[test]
    fn note_mtime_missing_returns_none() {
        let store = EmbeddingStore::open_in_memory().unwrap();
        assert_eq!(store.note_mtime("does-not-exist.md").unwrap(), None);
    }

    #[test]
    fn note_mtime_reflects_latest_upsert() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        let c = StoredChunk {
            note_mtime: 1_700_000_000,
            ..sample_chunk("a.md", 0, "x", "m1", vec![1.0, 0.0])
        };
        store.upsert_chunks(&[c]).unwrap();
        assert_eq!(store.note_mtime("a.md").unwrap(), Some(1_700_000_000));
    }

    #[test]
    fn note_mtime_for_model_is_scoped() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        store
            .upsert_chunks(&[
                StoredChunk {
                    note_mtime: 11,
                    ..sample_chunk("a.md", 0, "x", "old-model", vec![1.0, 0.0])
                },
                StoredChunk {
                    note_mtime: 42,
                    ..sample_chunk("a.md", 0, "x", "new-model", vec![0.0, 1.0])
                },
            ])
            .unwrap();
        assert_eq!(
            store.note_mtime_for_model("a.md", "old-model").unwrap(),
            Some(11)
        );
        assert_eq!(
            store.note_mtime_for_model("a.md", "new-model").unwrap(),
            Some(42)
        );
        assert_eq!(
            store.note_mtime_for_model("a.md", "missing-model").unwrap(),
            None
        );
    }

    #[test]
    fn only_model_name_requires_exactly_one_distinct_model() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        assert_eq!(store.only_model_name().unwrap(), None);

        store
            .upsert_chunks(&[sample_chunk("a.md", 0, "x", "m1", vec![1.0, 0.0])])
            .unwrap();
        assert_eq!(store.only_model_name().unwrap().as_deref(), Some("m1"));

        store
            .upsert_chunks(&[sample_chunk("b.md", 0, "y", "m2", vec![0.0, 1.0])])
            .unwrap();
        assert_eq!(store.only_model_name().unwrap(), None);
    }

    #[test]
    fn note_cosine_scores_aggregate_chunks_by_note() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        store
            .upsert_chunks(&[
                sample_chunk("src.md", 0, "src-1", "m1", vec![1.0, 0.0]),
                sample_chunk("src.md", 1, "src-2", "m1", vec![1.0, 0.0]),
                sample_chunk("near.md", 0, "near", "m1", vec![1.0, 0.0]),
                sample_chunk("mid.md", 0, "mid-1", "m1", vec![1.0, 0.0]),
                sample_chunk("mid.md", 1, "mid-2", "m1", vec![0.0, 1.0]),
                sample_chunk("far.md", 0, "far", "m1", vec![0.0, 1.0]),
                sample_chunk("neg.md", 0, "neg", "m1", vec![-1.0, 0.0]),
            ])
            .unwrap();

        let scores = store.note_cosine_scores("src.md", "m1").unwrap();
        assert!(!scores.contains_key("src.md"));
        assert!(scores["near.md"] > scores["mid.md"]);
        assert!(scores["mid.md"] > 0.0);
        assert!(!scores.contains_key("far.md"));
        assert!(!scores.contains_key("neg.md"));
    }

    #[test]
    fn note_cosine_scores_empty_when_source_missing() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        store
            .upsert_chunks(&[sample_chunk("a.md", 0, "x", "m1", vec![1.0, 0.0])])
            .unwrap();
        assert!(store
            .note_cosine_scores("missing.md", "m1")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn search_returns_hits_in_descending_order() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        // Three orthogonal unit vectors.
        store
            .upsert_chunks(&[
                sample_chunk("a.md", 0, "x-axis", "m1", vec![1.0, 0.0, 0.0]),
                sample_chunk("a.md", 1, "y-axis", "m1", vec![0.0, 1.0, 0.0]),
                sample_chunk("a.md", 2, "z-axis", "m1", vec![0.0, 0.0, 1.0]),
            ])
            .unwrap();
        // Query aligned with "x-axis" → perfect match; then slight y tilt.
        let hits = store.search(&[0.9, 0.1, 0.0], "m1", 3).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].text, "x-axis");
        assert_eq!(hits[1].text, "y-axis");
        assert_eq!(hits[2].text, "z-axis");
        // z-axis is orthogonal → ~0.0.
        assert!(hits[2].score.abs() < 1e-5);
    }

    #[test]
    fn search_filters_by_model() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        store
            .upsert_chunks(&[
                sample_chunk("a.md", 0, "from m1", "m1", vec![1.0, 0.0]),
                sample_chunk("a.md", 0, "from m2", "m2", vec![1.0, 0.0]),
            ])
            .unwrap();
        let hits_m1 = store.search(&[1.0, 0.0], "m1", 10).unwrap();
        assert_eq!(hits_m1.len(), 1);
        assert_eq!(hits_m1[0].text, "from m1");
    }

    #[test]
    fn search_dim_mismatch_is_skipped_not_errored() {
        let mut store = EmbeddingStore::open_in_memory().unwrap();
        store
            .upsert_chunks(&[
                sample_chunk("a.md", 0, "three dims", "m1", vec![1.0, 0.0, 0.0]),
                sample_chunk("b.md", 0, "two dims", "m1", vec![1.0, 0.0]),
            ])
            .unwrap();
        // Query is 2-dim; 3-dim row is silently skipped.
        let hits = store.search(&[1.0, 0.0], "m1", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "two dims");
    }

    #[test]
    fn search_empty_query_errors() {
        let store = EmbeddingStore::open_in_memory().unwrap();
        assert!(store.search(&[], "m1", 1).is_err());
    }

    #[test]
    fn search_on_empty_store_is_ok() {
        let store = EmbeddingStore::open_in_memory().unwrap();
        let hits = store.search(&[1.0, 0.0], "m1", 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let original: Vec<f32> = vec![0.1, -0.2, 3.14, -42.0, f32::EPSILON];
        let bytes = pack_f32(&original);
        let decoded = unpack_f32(&bytes, original.len());
        assert_eq!(decoded, original);
    }

    #[test]
    fn norm_zero_for_zero_vector() {
        assert_eq!(norm(&[0.0, 0.0, 0.0]), 0.0);
    }
}
