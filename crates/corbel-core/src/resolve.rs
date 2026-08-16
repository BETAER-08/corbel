use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::error::Result;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionStats {
    pub same_file: usize,
    pub scoped: usize,
    pub global_unique: usize,
    pub unresolved: usize,
}

struct ImportEntry {
    local_name: String,
    source_path: String,
    kind: String,
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

pub fn resolve_all(conn: &Connection) -> Result<ResolutionStats> {
    let mut symbols_by_name: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    let mut symbol_file: HashMap<i64, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, file_id, name FROM symbols")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let symbol_id: i64 = row.get(0)?;
            let file_id: i64 = row.get(1)?;
            let name: String = row.get(2)?;
            symbol_file.insert(symbol_id, file_id);
            symbols_by_name
                .entry(name)
                .or_default()
                .push((file_id, symbol_id));
        }
    }

    let mut imports_by_file: HashMap<i64, Vec<ImportEntry>> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT file_id, local_name, source_path, kind FROM imports")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let file_id: i64 = row.get(0)?;
            imports_by_file
                .entry(file_id)
                .or_default()
                .push(ImportEntry {
                    local_name: row.get(1)?,
                    source_path: row.get(2)?,
                    kind: row.get(3)?,
                });
        }
    }

    let relationships: Vec<(i64, i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, caller_symbol_id, callee_name FROM relationships")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    let mut stats = ResolutionStats::default();
    let tx = conn.unchecked_transaction()?;

    for (relationship_id, caller_symbol_id, callee_name) in relationships {
        let Some(&caller_file_id) = symbol_file.get(&caller_symbol_id) else {
            tx.execute(
                "UPDATE relationships SET callee_file_id = NULL, resolution = 'unresolved' WHERE id = ?1",
                params![relationship_id],
            )?;
            stats.unresolved += 1;
            continue;
        };

        let candidates = symbols_by_name
            .get(&callee_name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let same_file = candidates
            .iter()
            .any(|(file_id, _)| *file_id == caller_file_id);

        let (resolution, callee_file_id): (&'static str, Option<i64>) = if same_file {
            ("same-file", Some(caller_file_id))
        } else {
            let unique_file = if candidates.len() == 1 {
                Some(candidates[0].0)
            } else {
                None
            };

            let has_scoped_import = imports_by_file.get(&caller_file_id).is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.kind != "glob"
                        && (entry.local_name == callee_name
                            || last_segment(&entry.source_path) == callee_name)
                })
            });

            match (unique_file, has_scoped_import) {
                (Some(file_id), true) => ("scoped", Some(file_id)),
                (Some(file_id), false) => ("global-unique", Some(file_id)),
                (None, _) => ("unresolved", None),
            }
        };

        tx.execute(
            "UPDATE relationships SET callee_file_id = ?1, resolution = ?2 WHERE id = ?3",
            params![callee_file_id, resolution, relationship_id],
        )?;

        match resolution {
            "same-file" => stats.same_file += 1,
            "scoped" => stats.scoped += 1,
            "global-unique" => stats.global_unique += 1,
            _ => stats.unresolved += 1,
        }
    }

    tx.commit()?;

    Ok(stats)
}
