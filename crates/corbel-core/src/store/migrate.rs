use crate::error::{Error, Result, SchemaCondition};
use crate::store::schema::{CURRENT_SCHEMA_VERSION, create_schema};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

fn read_schema_version(conn: &Connection) -> Option<i32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .ok()
}

fn sibling_with_suffix(db_path: &Path, suffix: &str) -> PathBuf {
    let mut os_string = db_path.as_os_str().to_os_string();
    os_string.push(suffix);
    PathBuf::from(os_string)
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(Error::io(path, err)),
    }
}

fn rebuild_db_files(db_path: &Path) -> Result<()> {
    remove_file_if_exists(&sibling_with_suffix(db_path, "-wal"))?;
    remove_file_if_exists(&sibling_with_suffix(db_path, "-shm"))?;
    remove_file_if_exists(db_path)?;
    Ok(())
}

pub fn open_connection(db_path: impl AsRef<Path>) -> Result<Connection> {
    let db_path = db_path.as_ref();
    let conn = Connection::open(db_path).map_err(Error::Sqlite)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    if read_schema_version(&conn).is_none() {
        drop(conn);
        rebuild_db_files(db_path)?;
        let conn = Connection::open(db_path).map_err(Error::Sqlite)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        create_schema(&conn)?;
        return Ok(conn);
    }

    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_for_serve(db_path: impl AsRef<Path>) -> Result<Connection> {
    let db_path = db_path.as_ref();
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(Error::Sqlite)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "query_only", "ON")?;

    match read_schema_version(&conn) {
        Some(version) if version == CURRENT_SCHEMA_VERSION => Ok(conn),
        Some(version) => Err(Error::IncompatibleSchema {
            expected: CURRENT_SCHEMA_VERSION as i64,
            condition: SchemaCondition::VersionMismatch(version as i64),
        }),
        None => Err(Error::IncompatibleSchema {
            expected: CURRENT_SCHEMA_VERSION as i64,
            condition: SchemaCondition::Unreadable,
        }),
    }
}

pub fn migrate(conn: &Connection) -> Result<()> {
    loop {
        let user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

        match user_version {
            v if v == CURRENT_SCHEMA_VERSION => return Ok(()),
            v if v > CURRENT_SCHEMA_VERSION => {
                return Err(Error::Migration {
                    expected: CURRENT_SCHEMA_VERSION as i64,
                    found: v as i64,
                });
            }
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            2 => migrate_v2_to_v3(conn)?,
            3 => migrate_v3_to_v4(conn)?,
            v => {
                return Err(Error::Migration {
                    expected: CURRENT_SCHEMA_VERSION as i64,
                    found: v as i64,
                });
            }
        }
    }
}

fn migrate_v0_to_v1(conn: &Connection) -> Result<()> {
    create_schema(conn)
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE relationships;

        CREATE TABLE relationships (
            id INTEGER PRIMARY KEY,
            caller_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
            callee_name TEXT NOT NULL,
            callee_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
            resolution TEXT NOT NULL CHECK (resolution IN ('same-file', 'scoped', 'global-unique', 'external', 'unresolved'))
        );

        CREATE INDEX idx_relationships_callee_file ON relationships(callee_file_id);

        UPDATE files SET hash = '';
        ",
    )?;
    conn.pragma_update(None, "user_version", 2)?;
    Ok(())
}

fn migrate_v2_to_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE imports;

        CREATE TABLE imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            local_name TEXT,
            source_path TEXT NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE INDEX idx_imports_file ON imports(file_id);

        UPDATE files SET hash = '';
        ",
    )?;
    conn.pragma_update(None, "user_version", 3)?;
    Ok(())
}

fn migrate_v3_to_v4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        ALTER TABLE symbols ADD COLUMN owner TEXT;

        UPDATE files SET hash = '';
        ",
    )?;
    conn.pragma_update(None, "user_version", 4)?;
    Ok(())
}
