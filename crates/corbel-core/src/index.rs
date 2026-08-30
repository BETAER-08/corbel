use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::hash::hash_bytes;
use crate::path::RepoRoot;
use crate::resolve::{ResolutionStats, resolve_all};
use crate::symbol::FileParser;
use crate::walk::{WalkConfig, walk_repo};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub files_skipped_unchanged: usize,
    pub symbols_stored: usize,
    pub imports_stored: usize,
    pub relationships_stored: usize,
    pub references_skipped_no_caller: usize,
    pub resolution: ResolutionStats,
}

pub fn index_repo(
    root: &RepoRoot,
    conn: &Connection,
    parser: &dyn FileParser,
) -> Result<IndexStats> {
    let config = WalkConfig {
        extensions: parser.extensions(),
        follow_symlinks: false,
    };
    let walked = walk_repo(root, &config)?;

    let tx = conn.unchecked_transaction()?;
    let mut stats = IndexStats::default();
    let mut seen_paths: HashSet<String> = HashSet::new();

    for file in &walked {
        let path_str = file.path.as_ref();
        seen_paths.insert(path_str.to_string());

        let abs_path = root.as_path().join(path_str);
        let content = fs::read(&abs_path).map_err(|e| Error::io(&abs_path, e))?;
        let hash_str = hash_bytes(&content).to_string();

        let existing_hash: Option<String> = tx
            .query_row(
                "SELECT hash FROM files WHERE path = ?1",
                params![path_str],
                |row| row.get(0),
            )
            .optional()?;

        if existing_hash.as_deref() == Some(hash_str.as_str()) {
            tracing::debug!(path = %path_str, "file unchanged, skipping");
            stats.files_skipped_unchanged += 1;
            continue;
        }

        let source = String::from_utf8_lossy(&content).into_owned();
        let indexed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after unix epoch")
            .as_secs() as i64;

        tx.execute(
            "INSERT INTO files (path, lang, hash, indexed_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
                 lang = excluded.lang,
                 hash = excluded.hash,
                 indexed_at = excluded.indexed_at",
            params![path_str, file.extension, hash_str, indexed_at],
        )?;

        let file_id: i64 = tx.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![path_str],
            |row| row.get(0),
        )?;

        tx.execute("DELETE FROM symbols WHERE file_id = ?1", params![file_id])?;
        tx.execute("DELETE FROM imports WHERE file_id = ?1", params![file_id])?;

        let parsed = parser.parse(&file.path, &source);

        let mut symbol_ids_by_name: HashMap<String, Vec<(i64, u32)>> = HashMap::new();

        for symbol in &parsed.symbols {
            tx.execute(
                "INSERT INTO symbols (file_id, name, kind, line, signature, is_public)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    file_id,
                    symbol.name,
                    symbol.kind,
                    symbol.line,
                    symbol.signature,
                    symbol.is_public,
                ],
            )?;
            let symbol_id = tx.last_insert_rowid();
            symbol_ids_by_name
                .entry(symbol.name.clone())
                .or_default()
                .push((symbol_id, symbol.line));
            stats.symbols_stored += 1;
        }

        for import in &parsed.imports {
            tx.execute(
                "INSERT INTO imports (file_id, local_name, source_path, kind)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    file_id,
                    import.local_name,
                    import.source_path,
                    import.kind.as_str()
                ],
            )?;
            stats.imports_stored += 1;
        }

        for reference in &parsed.references {
            let caller_symbol_id = reference.caller_name.as_ref().and_then(|caller_name| {
                let candidates = symbol_ids_by_name.get(caller_name)?;
                candidates
                    .iter()
                    .filter(|(_, line)| *line <= reference.line)
                    .max_by_key(|(_, line)| *line)
                    .or_else(|| candidates.iter().min_by_key(|(_, line)| *line))
                    .map(|(symbol_id, _)| *symbol_id)
            });

            let Some(caller_symbol_id) = caller_symbol_id else {
                stats.references_skipped_no_caller += 1;
                continue;
            };

            tx.execute(
                "INSERT INTO relationships (caller_symbol_id, callee_name, callee_file_id, resolution)
                 VALUES (?1, ?2, NULL, 'unresolved')",
                params![caller_symbol_id, reference.callee_name],
            )?;
            stats.relationships_stored += 1;
        }

        tracing::debug!(
            path = %path_str,
            symbols = parsed.symbols.len(),
            imports = parsed.imports.len(),
            references = parsed.references.len(),
            "indexed file"
        );
        stats.files_indexed += 1;
    }

    let existing_paths: Vec<String> = {
        let mut stmt = tx.prepare("SELECT path FROM files")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    for path in existing_paths {
        if !seen_paths.contains(&path) {
            tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
        }
    }

    tx.commit()?;

    stats.resolution = resolve_all(conn)?;

    Ok(stats)
}
