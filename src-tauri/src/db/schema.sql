-- MyNotes index schema. See design_V2.md §5.3.
-- This DB is purely derived data — deleting it triggers a full rescan on next open.
-- Always open with `PRAGMA journal_mode = WAL;` for performance.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS notes (
  path TEXT PRIMARY KEY,         -- vault-relative path, forward-slash-normalized
  title TEXT,
  type TEXT,                     -- inbox / note / moc / daily / weekly / project / project-note
  status TEXT,                   -- draft / evergreen / archived / active / paused / done
  created TEXT,
  updated TEXT,
  size INTEGER,
  mtime INTEGER,                 -- seconds since epoch; used for incremental scan
  project_slug TEXT,             -- non-NULL only for type=project or project-note
  frontmatter_json TEXT          -- raw parsed frontmatter, for round-tripping unknown fields
);
CREATE INDEX IF NOT EXISTS idx_notes_type ON notes(type);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated);
CREATE INDEX IF NOT EXISTS idx_notes_status ON notes(status);
CREATE INDEX IF NOT EXISTS idx_notes_project ON notes(project_slug);

CREATE TABLE IF NOT EXISTS tags (
  note_path TEXT NOT NULL REFERENCES notes(path) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  PRIMARY KEY (note_path, tag)
);
CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);

CREATE TABLE IF NOT EXISTS links (
  src TEXT NOT NULL REFERENCES notes(path) ON DELETE CASCADE,
  dst TEXT NOT NULL,             -- raw [[target]] as typed by user
  dst_resolved TEXT,             -- resolved vault-relative path, NULL if unresolved
  link_type TEXT,                -- wiki / markdown / embed
  position INTEGER               -- byte offset in body, for jump-to-link
);
CREATE INDEX IF NOT EXISTS idx_links_dst ON links(dst_resolved);
CREATE INDEX IF NOT EXISTS idx_links_src ON links(src);
CREATE INDEX IF NOT EXISTS idx_links_dst_unresolved ON links(dst) WHERE dst_resolved IS NULL;

CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  note_path TEXT NOT NULL REFERENCES notes(path) ON DELETE CASCADE,
  line INTEGER NOT NULL,
  text TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0,  -- 0/1
  due TEXT,                          -- ISO YYYY-MM-DD, extracted from task body
  priority TEXT,                     -- 'urgent' / 'high' / 'med' / 'low', extracted from task body
  completed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_tasks_done ON tasks(done);
CREATE INDEX IF NOT EXISTS idx_tasks_note ON tasks(note_path);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due);
CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority);

CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
  path UNINDEXED,
  title,
  body,
  content=''
);

-- schema_meta tracks the indexer schema version and the vault path the DB
-- was built for. A mismatch on either triggers a fresh rebuild.
CREATE TABLE IF NOT EXISTS schema_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
