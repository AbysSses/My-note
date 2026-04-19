//! SQLite index layer.
//!
//! Design decisions (see design_V2.md §5.3 / §8.2):
//!   - The DB is **purely derived data** — dropping it triggers a full rescan.
//!   - It lives in the OS app-support dir, NOT inside the vault. This keeps
//!     iCloud/Syncthing from fighting with WAL locks.
//!   - One SQLite file per vault, keyed by a short hash of the vault path.
//!   - Single writer / readers, so a single `Connection` behind a `Mutex`
//!     is fine for this personal-notes workload.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

pub mod indexer;

/// Bump this whenever schema.sql changes incompatibly; the DB will be wiped
/// and rebuilt on next open.
pub const SCHEMA_VERSION: &str = "1";

/// Resolve the on-disk path for a vault's index DB. Creates parent dirs.
pub fn index_path_for_vault(app_support: &Path, vault_path: &Path) -> AppResult<PathBuf> {
    let mut hasher = Sha256::new();
    // Canonicalize so different-but-equivalent spellings of the same vault
    // share one index. Fall back to the literal path if canonicalize fails
    // (e.g. vault was just moved but we still want to open *something*).
    let canon = std::fs::canonicalize(vault_path).unwrap_or_else(|_| vault_path.to_path_buf());
    hasher.update(canon.to_string_lossy().as_bytes());
    let hex = hex_short(&hasher.finalize());

    let dir = app_support.join("index");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{hex}.sqlite")))
}

fn hex_short(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(16);
    for b in bytes.iter().take(8) {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Open (or create) the index DB for `vault_path`, applying schema + pragmas.
///
/// If `schema_meta.version` doesn't match [`SCHEMA_VERSION`] or `vault_path`
/// has drifted from what the DB was built for, the DB is dropped and
/// rebuilt empty. Either way, the returned connection is ready to use.
pub fn open_for_vault(app_support: &Path, vault_path: &Path) -> AppResult<Connection> {
    let db_path = index_path_for_vault(app_support, vault_path)?;
    let conn = Connection::open(&db_path).map_err(map_sql_err)?;
    apply_pragmas(&conn)?;
    apply_schema(&conn)?;

    let vault_str = vault_path.to_string_lossy().to_string();
    let needs_rebuild = match (
        read_meta(&conn, "schema_version")?,
        read_meta(&conn, "vault_path")?,
    ) {
        (Some(sv), Some(vp)) => sv != SCHEMA_VERSION || vp != vault_str,
        _ => true,
    };

    if needs_rebuild {
        drop(conn);
        // Safer than TRUNCATEing tables: the FTS virtual table is finicky
        // and rebuilding the file guarantees a clean slate.
        let _ = std::fs::remove_file(&db_path);
        let conn = Connection::open(&db_path).map_err(map_sql_err)?;
        apply_pragmas(&conn)?;
        apply_schema(&conn)?;
        write_meta(&conn, "schema_version", SCHEMA_VERSION)?;
        write_meta(&conn, "vault_path", &vault_str)?;
        tracing::info!(db = %db_path.display(), "rebuilt index (schema or vault drift)");
        return Ok(conn);
    }

    Ok(conn)
}

fn apply_pragmas(conn: &Connection) -> AppResult<()> {
    // `journal_mode = WAL` returns a row (the resulting mode), so `pragma_update`
    // is not usable here — we go through `pragma` which tolerates rows.
    // WAL is the whole point of §5.3's perf note.
    conn.pragma(None, "journal_mode", "WAL", |_row| Ok(()))
        .map_err(map_sql_err)?;
    conn.pragma(None, "synchronous", "NORMAL", |_row| Ok(()))
        .map_err(map_sql_err)?;
    conn.pragma(None, "foreign_keys", "ON", |_row| Ok(()))
        .map_err(map_sql_err)?;
    Ok(())
}

fn apply_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(include_str!("schema.sql"))
        .map_err(map_sql_err)?;
    Ok(())
}

fn read_meta(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let mut stmt = conn
        .prepare("SELECT value FROM schema_meta WHERE key = ?1")
        .map_err(map_sql_err)?;
    let mut rows = stmt.query([key]).map_err(map_sql_err)?;
    if let Some(row) = rows.next().map_err(map_sql_err)? {
        Ok(Some(row.get::<_, String>(0).map_err(map_sql_err)?))
    } else {
        Ok(None)
    }
}

fn write_meta(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO schema_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map_err(map_sql_err)?;
    Ok(())
}

pub(crate) fn map_sql_err(e: rusqlite::Error) -> AppError {
    AppError::Other(format!("sqlite: {e}"))
}
