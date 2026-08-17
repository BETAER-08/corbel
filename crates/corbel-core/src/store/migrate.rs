use crate::error::{Error, Result};
use crate::store::schema::{CURRENT_SCHEMA_VERSION, create_schema};
use rusqlite::Connection;
use std::path::Path;

pub fn open_connection(db_path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(db_path.as_ref()).map_err(Error::Sqlite)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(&conn)?;
    Ok(conn)
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
