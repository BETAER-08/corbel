use std::collections::{HashSet, VecDeque};

use rusqlite::{Connection, params};

use crate::budget::{TokenBudget, estimate_node_tokens};
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

struct CallerRow {
    symbol_id: i64,
    file_id: i64,
    info: CallerInfo,
}

fn find_caller_rows(
    conn: &Connection,
    callee_file_id: i64,
    callee_name: &str,
) -> Result<Vec<CallerRow>> {
    let mut stmt = conn.prepare(
        "SELECT caller_symbols.id, caller_symbols.file_id, caller_symbols.name,
                caller_files.path, caller_symbols.line, relationships.resolution
         FROM relationships
         JOIN symbols AS caller_symbols ON caller_symbols.id = relationships.caller_symbol_id
         JOIN files AS caller_files ON caller_files.id = caller_symbols.file_id
         WHERE relationships.callee_file_id = ?1 AND relationships.callee_name = ?2
         ORDER BY caller_files.path, caller_symbols.line",
    )?;
    let rows = stmt.query_map(params![callee_file_id, callee_name], |row| {
        Ok(CallerRow {
            symbol_id: row.get(0)?,
            file_id: row.get(1)?,
            info: CallerInfo {
                name: row.get(2)?,
                file: row.get(3)?,
                line: row.get(4)?,
                resolution: row.get(5)?,
            },
        })
    })?;
    rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
}

fn find_callers(conn: &Connection, file_id: i64, name: &str) -> Result<Vec<CallerInfo>> {
    Ok(find_caller_rows(conn, file_id, name)?
        .into_iter()
        .map(|row| row.info)
        .collect())
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

const MAX_IMPACT_DEPTH: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactNode {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub resolution: String,
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactResult {
    pub target: SymbolInfo,
    pub affected: Vec<ImpactNode>,
    pub truncated: bool,
    pub truncated_count: usize,
    pub max_depth_reached: u32,
}

fn impact_for_symbol(
    conn: &Connection,
    symbol_row: &SymbolRow,
    budget: &mut TokenBudget,
) -> Result<ImpactResult> {
    let mut visited: HashSet<i64> = HashSet::new();
    visited.insert(symbol_row.id);
    let mut rejected: HashSet<i64> = HashSet::new();

    let mut queue: VecDeque<(i64, String, u32)> = VecDeque::new();
    queue.push_back((symbol_row.file_id, symbol_row.info.name.clone(), 0));

    let mut affected: Vec<ImpactNode> = Vec::new();
    let mut truncated = false;
    let mut truncated_count = 0usize;
    let mut max_depth_reached = 0u32;

    while let Some((current_file_id, current_name, depth)) = queue.pop_front() {
        if depth >= MAX_IMPACT_DEPTH {
            continue;
        }

        let caller_rows = find_caller_rows(conn, current_file_id, &current_name)?;
        for caller_row in caller_rows {
            if visited.contains(&caller_row.symbol_id) {
                continue;
            }

            let next_depth = depth + 1;
            let token_estimate = estimate_node_tokens(
                &caller_row.info.name,
                &caller_row.info.file,
                caller_row.info.line,
                &caller_row.info.resolution,
            );

            if !budget.try_consume(token_estimate) {
                truncated = true;
                if rejected.insert(caller_row.symbol_id) {
                    truncated_count += 1;
                }
                continue;
            }

            visited.insert(caller_row.symbol_id);
            max_depth_reached = max_depth_reached.max(next_depth);

            affected.push(ImpactNode {
                name: caller_row.info.name.clone(),
                file: caller_row.info.file.clone(),
                line: caller_row.info.line,
                resolution: caller_row.info.resolution.clone(),
                depth: next_depth,
            });

            queue.push_back((caller_row.file_id, caller_row.info.name, next_depth));
        }
    }

    affected.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });

    Ok(ImpactResult {
        target: symbol_row.info.clone(),
        affected,
        truncated,
        truncated_count,
        max_depth_reached,
    })
}

pub fn impact(
    conn: &Connection,
    name: &str,
    file: Option<&str>,
    mut budget: TokenBudget,
) -> Result<Vec<ImpactResult>> {
    let symbol_rows = find_symbol_rows(conn, name, file)?;

    let mut results = Vec::with_capacity(symbol_rows.len());
    for symbol_row in &symbol_rows {
        results.push(impact_for_symbol(conn, symbol_row, &mut budget)?);
    }

    Ok(results)
}
