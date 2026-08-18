use rusqlite::{Connection, params};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub kind: String,
    pub signature: Option<String>,
    pub is_public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub resolution: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalleeInfo {
    pub name: String,
    pub file: Option<String>,
    pub resolution: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolResult {
    pub symbol: SymbolInfo,
    pub callers: Vec<CallerInfo>,
    pub callees: Vec<CalleeInfo>,
}

struct SymbolRow {
    id: i64,
    file_id: i64,
    info: SymbolInfo,
}

fn find_symbol_rows(conn: &Connection, name: &str, file: Option<&str>) -> Result<Vec<SymbolRow>> {
    let sql = match file {
        Some(_) => {
            "SELECT symbols.id, symbols.file_id, symbols.name, symbols.kind, symbols.line,
                    symbols.signature, symbols.is_public, files.path
             FROM symbols
             JOIN files ON files.id = symbols.file_id
             WHERE symbols.name = ?1 AND files.path = ?2
             ORDER BY files.path, symbols.line"
        }
        None => {
            "SELECT symbols.id, symbols.file_id, symbols.name, symbols.kind, symbols.line,
                    symbols.signature, symbols.is_public, files.path
             FROM symbols
             JOIN files ON files.id = symbols.file_id
             WHERE symbols.name = ?1
             ORDER BY files.path, symbols.line"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = match file {
        Some(file) => stmt.query_map(params![name, file], map_symbol_row)?,
        None => stmt.query_map(params![name], map_symbol_row)?,
    };
    rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
}

fn map_symbol_row(row: &rusqlite::Row) -> rusqlite::Result<SymbolRow> {
    Ok(SymbolRow {
        id: row.get(0)?,
        file_id: row.get(1)?,
        info: SymbolInfo {
            name: row.get(2)?,
            kind: row.get(3)?,
            line: row.get(4)?,
            signature: row.get(5)?,
            is_public: row.get(6)?,
            file: row.get(7)?,
        },
    })
}

fn find_callees(conn: &Connection, symbol_id: i64) -> Result<Vec<CalleeInfo>> {
    let mut stmt = conn.prepare(
        "SELECT relationships.callee_name, relationships.resolution, callee_files.path
         FROM relationships
         LEFT JOIN files AS callee_files ON callee_files.id = relationships.callee_file_id
         WHERE relationships.caller_symbol_id = ?1
         ORDER BY (callee_files.path IS NULL), callee_files.path, relationships.callee_name, relationships.id",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| {
        Ok(CalleeInfo {
            name: row.get(0)?,
            resolution: row.get(1)?,
            file: row.get(2)?,
        })
    })?;
    rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
}

fn find_callers(conn: &Connection, file_id: i64, name: &str) -> Result<Vec<CallerInfo>> {
    let mut stmt = conn.prepare(
        "SELECT caller_symbols.name, caller_files.path, caller_symbols.line, relationships.resolution
         FROM relationships
         JOIN symbols AS caller_symbols ON caller_symbols.id = relationships.caller_symbol_id
         JOIN files AS caller_files ON caller_files.id = caller_symbols.file_id
         WHERE relationships.callee_file_id = ?1 AND relationships.callee_name = ?2
         ORDER BY caller_files.path, caller_symbols.line",
    )?;
    let rows = stmt.query_map(params![file_id, name], |row| {
        Ok(CallerInfo {
            name: row.get(0)?,
            file: row.get(1)?,
            line: row.get(2)?,
            resolution: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
}

pub fn get_symbol(conn: &Connection, name: &str, file: Option<&str>) -> Result<Vec<SymbolResult>> {
    let symbol_rows = find_symbol_rows(conn, name, file)?;

    let mut results = Vec::with_capacity(symbol_rows.len());
    for symbol_row in symbol_rows {
        let callees = find_callees(conn, symbol_row.id)?;
        let callers = find_callers(conn, symbol_row.file_id, &symbol_row.info.name)?;

        results.push(SymbolResult {
            symbol: symbol_row.info,
            callers,
            callees,
        });
    }

    Ok(results)
}
