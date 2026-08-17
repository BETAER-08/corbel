use crate::error::Result;
use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i32 = 3;

const SCHEMA_DDL: &str = "
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    lang TEXT NOT NULL,
    hash TEXT NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    line INTEGER NOT NULL,
    signature TEXT,
    is_public INTEGER NOT NULL
);

CREATE TABLE relationships (
    id INTEGER PRIMARY KEY,
    caller_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    callee_name TEXT NOT NULL,
    callee_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
    resolution TEXT NOT NULL CHECK (resolution IN ('same-file', 'scoped', 'global-unique', 'external', 'unresolved'))
);

CREATE TABLE imports (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    local_name TEXT,
    source_path TEXT NOT NULL,
    kind TEXT NOT NULL
);

CREATE TABLE embeddings (
    symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
    vector BLOB NOT NULL
);

CREATE INDEX idx_symbols_name ON symbols(name);
CREATE INDEX idx_relationships_callee_file ON relationships(callee_file_id);
CREATE INDEX idx_symbols_file ON symbols(file_id);
CREATE INDEX idx_imports_file ON imports(file_id);
";

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_DDL)?;
    conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
    Ok(())
}
