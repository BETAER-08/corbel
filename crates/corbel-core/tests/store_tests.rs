use corbel_core::error::Error;
use corbel_core::store::migrate::{migrate, open_connection};
use corbel_core::store::schema::CURRENT_SCHEMA_VERSION;
use tempfile::tempdir;

#[test]
fn open_connection_creates_schema_at_current_version() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");

    let conn = open_connection(&db_path).unwrap();

    let user_version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();

    assert_eq!(user_version, CURRENT_SCHEMA_VERSION);
}

#[test]
fn schema_creates_all_tables_and_indexes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = open_connection(&db_path).unwrap();

    let mut table_stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = table_stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(
        tables,
        vec!["embeddings", "files", "imports", "relationships", "symbols"]
    );

    let mut index_stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_%' ORDER BY name",
        )
        .unwrap();
    let indexes: Vec<String> = index_stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(
        indexes,
        vec![
            "idx_imports_file",
            "idx_relationships_callee_file",
            "idx_symbols_file",
            "idx_symbols_name",
        ]
    );
}

#[test]
fn deleting_file_cascades_to_symbols() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = open_connection(&db_path).unwrap();

    conn.execute(
        "INSERT INTO files (id, path, lang, hash, indexed_at) VALUES (1, 'a.rs', 'rust', 'deadbeef', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO symbols (id, file_id, name, kind, line, signature, is_public) VALUES (1, 1, 'foo', 'function', 1, NULL, 1)",
        [],
    )
    .unwrap();

    let count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count_before, 1);

    conn.execute("DELETE FROM files WHERE id = 1", []).unwrap();

    let count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count_after, 0);
}

#[test]
fn relationships_resolution_check_constraint_rejects_invalid_value() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = open_connection(&db_path).unwrap();

    conn.execute(
        "INSERT INTO files (id, path, lang, hash, indexed_at) VALUES (1, 'a.rs', 'rust', 'deadbeef', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO symbols (id, file_id, name, kind, line, signature, is_public) VALUES (1, 1, 'foo', 'function', 1, NULL, 1)",
        [],
    )
    .unwrap();

    let result = conn.execute(
        "INSERT INTO relationships (caller_symbol_id, callee_name, callee_file_id, resolution) VALUES (1, 'bar', NULL, 'maybe')",
        [],
    );

    assert!(result.is_err());
}

#[test]
fn reopening_migrated_db_does_not_duplicate_schema() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");

    let conn1 = open_connection(&db_path).unwrap();
    drop(conn1);

    let conn2 = open_connection(&db_path).unwrap();

    let table_count: i64 = conn2
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'files'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(table_count, 1);
}

#[test]
fn migrate_v1_to_v2_allows_external_resolution_and_invalidates_hashes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    conn.execute_batch(
        "
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
            resolution TEXT NOT NULL CHECK (resolution IN ('same-file', 'scoped', 'global-unique', 'unresolved'))
        );

        CREATE TABLE imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            local_name TEXT NOT NULL,
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

        INSERT INTO files (id, path, lang, hash, indexed_at) VALUES (1, 'a.rs', 'rust', 'deadbeefcafebabe', 0);
        INSERT INTO symbols (id, file_id, name, kind, line, signature, is_public) VALUES (1, 1, 'a', 'function', 1, NULL, 0);
        INSERT INTO relationships (caller_symbol_id, callee_name, callee_file_id, resolution) VALUES (1, 'b', NULL, 'unresolved');
        ",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    drop(conn);

    let conn = open_connection(&db_path).unwrap();

    let user_version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(user_version, CURRENT_SCHEMA_VERSION);

    let relationship_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))
        .unwrap();
    assert_eq!(relationship_count, 0);

    let hash: String = conn
        .query_row("SELECT hash FROM files WHERE id = 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(hash, "");

    conn.execute(
        "INSERT INTO relationships (caller_symbol_id, callee_name, callee_file_id, resolution) VALUES (1, 'c', NULL, 'external')",
        [],
    )
    .unwrap();

    let external_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM relationships WHERE resolution = 'external'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(external_count, 1);
}

#[test]
fn migrate_v2_to_v3_empties_imports_and_invalidates_hashes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    conn.execute_batch(
        "
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
            local_name TEXT NOT NULL,
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

        INSERT INTO files (id, path, lang, hash, indexed_at) VALUES (1, 'a.ts', 'typescript', 'deadbeefcafebabe', 0);
        INSERT INTO imports (id, file_id, local_name, source_path, kind) VALUES (1, 1, '*', 'moduleF', 're-export-all');
        ",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    drop(conn);

    let conn = open_connection(&db_path).unwrap();

    let user_version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(user_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(user_version, 3);

    let import_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM imports", [], |row| row.get(0))
        .unwrap();
    assert_eq!(import_count, 0);

    let hash: String = conn
        .query_row("SELECT hash FROM files WHERE id = 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(hash, "");

    conn.execute(
        "INSERT INTO imports (file_id, local_name, source_path, kind) VALUES (1, NULL, 'moduleF', 'wildcard')",
        [],
    )
    .unwrap();

    let local_name: Option<String> = conn
        .query_row(
            "SELECT local_name FROM imports WHERE file_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(local_name, None);
}

#[test]
fn migrate_rejects_future_schema_version() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = open_connection(&db_path).unwrap();

    let future_version = CURRENT_SCHEMA_VERSION + 1;
    conn.pragma_update(None, "user_version", future_version)
        .unwrap();

    let result = migrate(&conn);

    assert!(matches!(result, Err(Error::Migration { .. })));
}
