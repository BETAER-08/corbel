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
    let user_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    match user_version {
        0 => migrate_v0_to_v1(conn),
        v if v == CURRENT_SCHEMA_VERSION => Ok(()),
        v if v > CURRENT_SCHEMA_VERSION => Err(Error::Migration {
            expected: CURRENT_SCHEMA_VERSION as i64,
            found: v as i64,
        }),
        v => Err(Error::Migration {
            expected: CURRENT_SCHEMA_VERSION as i64,
            found: v as i64,
        }),
    }
}

fn migrate_v0_to_v1(conn: &Connection) -> Result<()> {
    create_schema(conn)
}
