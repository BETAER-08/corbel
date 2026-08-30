use corbel_core::index::index_repo;
use corbel_core::path::RepoRoot;
use corbel_core::store::migrate::open_connection;
use corbel_lang::langs::javascript::JavaScriptSupport;
use corbel_lang::langs::python::PythonSupport;
use corbel_lang::langs::rust::RustSupport;
use corbel_lang::langs::typescript::TypeScriptSupport;
use corbel_lang::registry::LanguageRegistry;
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Box::new(RustSupport)).unwrap();
    registry
}

fn db() -> (Connection, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let conn = open_connection(dir.path().join("index.db")).unwrap();
    (conn, dir)
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn indexes_files_and_stores_symbols() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn add(a: i32, b: i32) -> i32 { a + b }\nfn helper() {}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("b.rs"),
        b"pub struct Point { pub x: i32 }\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 2);
    assert_eq!(stats.files_skipped_unchanged, 0);
    assert_eq!(stats.symbols_stored, 3);

    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 3);
}

#[test]
fn symbol_fields_match_parsed_source() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    index_repo(&root, &conn, &parser).unwrap();

    let (name, kind, line, signature, is_public): (String, String, i64, Option<String>, i64) = conn
        .query_row(
            "SELECT name, kind, line, signature, is_public FROM symbols",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(name, "add");
    assert_eq!(kind, "function");
    assert_eq!(line, 1);
    assert_eq!(
        signature.as_deref(),
        Some("pub fn add(a: i32, b: i32) -> i32")
    );
    assert_eq!(is_public, 1);
}

#[test]
fn stores_imports_from_use_declarations() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"use std::collections::HashMap;\n\nfn main() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.imports_stored, 1);
    let (local_name, source_path, kind): (String, String, String) = conn
        .query_row(
            "SELECT local_name, source_path, kind FROM imports",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(local_name, "HashMap");
    assert_eq!(source_path, "std::collections::HashMap");
    assert_eq!(kind, "direct");
}

#[test]
fn second_index_run_skips_unchanged_files() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() {}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let first = index_repo(&root, &conn, &parser).unwrap();
    assert_eq!(first.files_indexed, 2);
    assert_eq!(first.files_skipped_unchanged, 0);

    let second = index_repo(&root, &conn, &parser).unwrap();
    assert_eq!(second.files_indexed, 0);
    assert_eq!(second.files_skipped_unchanged, 2);

    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 2);
}

#[test]
fn changed_file_replaces_old_symbols_without_duplicating() {
    let repo_dir = tempdir().unwrap();
    let file_path = repo_dir.path().join("a.rs");
    fs::write(&file_path, b"fn a() {}\nfn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    index_repo(&root, &conn, &parser).unwrap();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 2);

    fs::write(&file_path, b"fn c() {}\n").unwrap();
    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 1);

    let name: String = conn
        .query_row("SELECT name FROM symbols", [], |row| row.get(0))
        .unwrap();
    assert_eq!(name, "c");
}

#[test]
fn deleted_file_is_removed_along_with_its_symbols() {
    let repo_dir = tempdir().unwrap();
    let file_path = repo_dir.path().join("a.rs");
    fs::write(&file_path, b"fn a() {}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    index_repo(&root, &conn, &parser).unwrap();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 2);

    fs::remove_file(&file_path).unwrap();
    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 1);
    assert_eq!(stats.files_indexed, 0);
    assert_eq!(stats.files_skipped_unchanged, 1);

    let path: String = conn
        .query_row("SELECT path FROM files", [], |row| row.get(0))
        .unwrap();
    assert_eq!(path, "b.rs");
}

#[test]
fn call_inside_function_is_attributed_to_caller() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { b(); }\nfn b() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.relationships_stored, 1);
    assert_eq!(stats.references_skipped_no_caller, 0);

    let (caller_name, callee_name, resolution): (String, String, String) = conn
        .query_row(
            "SELECT symbols.name, relationships.callee_name, relationships.resolution
             FROM relationships JOIN symbols ON symbols.id = relationships.caller_symbol_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(caller_name, "a");
    assert_eq!(callee_name, "b");
    assert_eq!(resolution, "same-file");
}

#[test]
fn every_relationship_row_carries_a_resolution_value() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { b(); c(); }\nfn b() {}\nfn c() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    index_repo(&root, &conn, &parser).unwrap();

    let valid_resolution_count = count(
        &conn,
        "SELECT COUNT(*) FROM relationships
         WHERE resolution IN ('same-file', 'scoped', 'global-unique', 'external', 'unresolved')",
    );
    let total_count = count(&conn, "SELECT COUNT(*) FROM relationships");
    assert_eq!(valid_resolution_count, total_count);
    assert_eq!(total_count, 2);
}

#[test]
fn top_level_call_is_skipped_and_counted() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn b() {}\nstatic X: i32 = b();\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.relationships_stored, 0);
    assert_eq!(stats.references_skipped_no_caller, 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM relationships"), 0);
}

#[test]
fn reindexing_does_not_duplicate_relationships() {
    let repo_dir = tempdir().unwrap();
    let file_path = repo_dir.path().join("a.rs");
    fs::write(&file_path, b"fn a() { b(); }\nfn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    index_repo(&root, &conn, &parser).unwrap();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM relationships"), 1);

    fs::write(&file_path, b"fn a() { b(); b(); }\nfn b() {}\n").unwrap();
    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.relationships_stored, 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM relationships"), 2);
}

#[test]
fn multiple_callers_in_same_file_attribute_correctly() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { x(); }\nfn b() { y(); }\nfn x() {}\nfn y() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();
    assert_eq!(stats.relationships_stored, 2);

    let mut stmt = conn
        .prepare(
            "SELECT symbols.name, relationships.callee_name
             FROM relationships JOIN symbols ON symbols.id = relationships.caller_symbol_id
             ORDER BY symbols.name",
        )
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(
        rows,
        vec![
            ("a".to_string(), "x".to_string()),
            ("b".to_string(), "y".to_string()),
        ]
    );
}

#[test]
fn index_stats_relationships_stored_matches_table_count() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { b(); c(); }\nfn b() { c(); }\nfn c() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(
        stats.relationships_stored as i64,
        count(&conn, "SELECT COUNT(*) FROM relationships")
    );
    assert_eq!(stats.relationships_stored, 3);
}

#[test]
fn python_repository_indexes_and_resolves_through_the_same_pipeline() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.py"),
        b"from b import helper\n\n\ndef caller():\n    helper()\n    local()\n\n\ndef local():\n    pass\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.py"), b"def helper():\n    pass\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let mut parser = LanguageRegistry::new();
    parser.register(Box::new(PythonSupport)).unwrap();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 3);

    let (local_resolution, local_callee_path): (String, Option<String>) = conn
        .query_row(
            "SELECT relationships.resolution, files.path
             FROM relationships
             JOIN symbols ON symbols.id = relationships.caller_symbol_id
             LEFT JOIN files ON files.id = relationships.callee_file_id
             WHERE relationships.callee_name = 'local'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(local_resolution, "same-file");
    assert_eq!(local_callee_path.as_deref(), Some("a.py"));

    let (helper_resolution, helper_callee_path): (String, Option<String>) = conn
        .query_row(
            "SELECT relationships.resolution, files.path
             FROM relationships
             JOIN symbols ON symbols.id = relationships.caller_symbol_id
             LEFT JOIN files ON files.id = relationships.callee_file_id
             WHERE relationships.callee_name = 'helper'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(helper_resolution, "scoped");
    assert_eq!(helper_callee_path.as_deref(), Some("b.py"));

    assert_eq!(stats.resolution.same_file, 1);
    assert_eq!(stats.resolution.scoped, 1);
}

#[test]
fn javascript_repository_indexes_and_resolves_through_the_same_pipeline() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.js"),
        b"import { helper } from \"./b\";\n\nexport function caller() {\n    helper();\n    local();\n}\n\nfunction local() {\n    return 1;\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("b.js"),
        b"export function helper() {\n    return 1;\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let mut parser = LanguageRegistry::new();
    parser.register(Box::new(JavaScriptSupport)).unwrap();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 3);

    let (local_resolution, local_callee_path): (String, Option<String>) = conn
        .query_row(
            "SELECT relationships.resolution, files.path
             FROM relationships
             JOIN symbols ON symbols.id = relationships.caller_symbol_id
             LEFT JOIN files ON files.id = relationships.callee_file_id
             WHERE relationships.callee_name = 'local'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(local_resolution, "same-file");
    assert_eq!(local_callee_path.as_deref(), Some("a.js"));

    let (helper_resolution, helper_callee_path): (String, Option<String>) = conn
        .query_row(
            "SELECT relationships.resolution, files.path
             FROM relationships
             JOIN symbols ON symbols.id = relationships.caller_symbol_id
             LEFT JOIN files ON files.id = relationships.callee_file_id
             WHERE relationships.callee_name = 'helper'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(helper_resolution, "scoped");
    assert_eq!(helper_callee_path.as_deref(), Some("b.js"));

    assert_eq!(stats.resolution.same_file, 1);
    assert_eq!(stats.resolution.scoped, 1);
}

#[test]
fn typescript_repository_indexes_and_resolves_through_the_same_pipeline() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.ts"),
        b"import { helper } from \"./b\";\n\nexport function caller() {\n    helper();\n    local();\n}\n\nfunction local() {\n    return 1;\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("b.ts"),
        b"export function helper() {\n    return 1;\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let mut parser = LanguageRegistry::new();
    parser.register(Box::new(TypeScriptSupport)).unwrap();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 2);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 3);

    let (local_resolution, local_callee_path): (String, Option<String>) = conn
        .query_row(
            "SELECT relationships.resolution, files.path
             FROM relationships
             JOIN symbols ON symbols.id = relationships.caller_symbol_id
             LEFT JOIN files ON files.id = relationships.callee_file_id
             WHERE relationships.callee_name = 'local'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(local_resolution, "same-file");
    assert_eq!(local_callee_path.as_deref(), Some("a.ts"));

    let (helper_resolution, helper_callee_path): (String, Option<String>) = conn
        .query_row(
            "SELECT relationships.resolution, files.path
             FROM relationships
             JOIN symbols ON symbols.id = relationships.caller_symbol_id
             LEFT JOIN files ON files.id = relationships.callee_file_id
             WHERE relationships.callee_name = 'helper'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(helper_resolution, "scoped");
    assert_eq!(helper_callee_path.as_deref(), Some("b.ts"));

    assert_eq!(stats.resolution.same_file, 1);
    assert_eq!(stats.resolution.scoped, 1);
}

#[test]
fn binary_file_with_recognized_extension_is_skipped_not_indexed() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("good.rs"), b"pub fn good() {}\n").unwrap();
    fs::write(
        repo_dir.path().join("binary.rs"),
        [0xff_u8, 0xfe, 0x00, 0x01, 0x02, 0xff, 0xff, 0xfe],
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    assert_eq!(stats.skipped.len(), 1);
    assert_eq!(stats.skipped[0].path.as_deref(), Some("binary.rs"));
    assert_eq!(
        stats.skipped[0].reason,
        corbel_core::walk::SkipReason::NotUtf8
    );
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 1);
}

#[test]
fn oversized_file_is_skipped_and_not_read_as_content() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("small.rs"), b"pub fn small() {}\n").unwrap();
    let huge = vec![b'a'; (corbel_core::index::MAX_INDEXABLE_FILE_SIZE_BYTES + 1) as usize];
    fs::write(repo_dir.path().join("huge.rs"), &huge).unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    assert_eq!(stats.skipped.len(), 1);
    assert_eq!(stats.skipped[0].path.as_deref(), Some("huge.rs"));
    match &stats.skipped[0].reason {
        corbel_core::walk::SkipReason::TooLarge { size, limit } => {
            assert_eq!(*size, huge.len() as u64);
            assert_eq!(*limit, corbel_core::index::MAX_INDEXABLE_FILE_SIZE_BYTES);
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 1);
}

#[test]
fn utf8_bom_is_stripped_before_parsing() {
    let repo_dir = tempdir().unwrap();
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(b"pub fn with_bom() {}\n");
    fs::write(repo_dir.path().join("a.rs"), &content).unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    assert!(stats.skipped.is_empty());

    let (name, line): (String, i64) = conn
        .query_row(
            "SELECT name, line FROM symbols WHERE name = 'with_bom'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(name, "with_bom");
    assert_eq!(line, 1);
}

#[test]
fn skipped_file_does_not_abort_indexing_of_other_files() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("before.rs"), b"pub fn before() {}\n").unwrap();
    fs::write(repo_dir.path().join("bad.rs"), [0xff_u8, 0xfe, 0x00]).unwrap();
    fs::write(repo_dir.path().join("after.rs"), b"pub fn after() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 2);
    assert_eq!(stats.skipped.len(), 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 2);
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_skipped_and_does_not_abort_indexing() {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("readable.rs"), b"pub fn ok() {}\n").unwrap();
    let locked_path = repo_dir.path().join("locked.rs");
    fs::write(&locked_path, b"pub fn locked() {}\n").unwrap();
    fs::set_permissions(&locked_path, Permissions::from_mode(0o000)).unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser);

    fs::set_permissions(&locked_path, Permissions::from_mode(0o644)).unwrap();

    let stats = stats.unwrap();
    assert_eq!(stats.files_indexed, 1);
    assert_eq!(stats.skipped.len(), 1);
    assert_eq!(stats.skipped[0].path.as_deref(), Some("locked.rs"));
    assert!(matches!(
        stats.skipped[0].reason,
        corbel_core::walk::SkipReason::Io(_)
    ));
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 1);
}

#[test]
fn empty_repository_indexes_without_error() {
    let repo_dir = tempdir().unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 0);
    assert!(stats.skipped.is_empty());
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM files"), 0);
}

#[test]
fn repository_without_git_directory_indexes_normally() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    assert!(stats.skipped.is_empty());
}

#[test]
fn non_ascii_path_is_indexed_correctly() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("café_한글.rs"), b"pub fn a() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    let path: String = conn
        .query_row("SELECT path FROM files", [], |row| row.get(0))
        .unwrap();
    assert_eq!(path, "café_한글.rs");
}

#[test]
fn old_schema_version_forces_full_reindex_not_incremental() {
    let repo_dir = tempdir().unwrap();
    let file_content = b"pub fn a() {}\n";
    fs::write(repo_dir.path().join("a.rs"), file_content).unwrap();

    let db_dir = tempdir().unwrap();
    let db_path = db_dir.path().join("index.db");
    let real_hash = corbel_core::hash::hash_bytes(file_content).to_string();

    let raw = rusqlite::Connection::open(&db_path).unwrap();
    raw.execute_batch(&format!(
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

        INSERT INTO files (id, path, lang, hash, indexed_at) VALUES (1, 'a.rs', 'rust', '{real_hash}', 0);
        "
    ))
    .unwrap();
    raw.pragma_update(None, "user_version", 2).unwrap();
    drop(raw);

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let conn = open_connection(&db_path).unwrap();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(stats.files_indexed, 1);
    assert_eq!(stats.files_skipped_unchanged, 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM symbols"), 1);
}
