use corbel_core::index::index_repo;
use corbel_core::path::RepoRoot;
use corbel_core::store::migrate::open_connection;
use corbel_lang::langs::rust::RustSupport;
use corbel_lang::langs::tsx::TsxSupport;
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

fn ts_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Box::new(TypeScriptSupport)).unwrap();
    registry
}

fn tsx_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Box::new(TsxSupport)).unwrap();
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

fn relationship_for_named_callee(conn: &Connection, callee_name: &str) -> (String, Option<String>) {
    conn.query_row(
        "SELECT relationships.resolution, files.path
         FROM relationships
         LEFT JOIN files ON files.id = relationships.callee_file_id
         WHERE relationships.callee_name = ?1",
        [callee_name],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

fn relationship_for_caller(conn: &Connection, caller_name: &str) -> (String, Option<String>) {
    conn.query_row(
        "SELECT relationships.resolution, files.path
         FROM relationships
         JOIN symbols ON symbols.id = relationships.caller_symbol_id
         LEFT JOIN files ON files.id = relationships.callee_file_id
         WHERE symbols.name = ?1",
        [caller_name],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

#[test]
fn same_file_call_resolves_to_same_file() {
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

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "same-file");
    assert_eq!(callee_path.as_deref(), Some("a.rs"));
    assert_eq!(stats.resolution.same_file, 1);
}

#[test]
fn cross_file_unique_definition_resolves_global_unique() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() { b(); }\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "global-unique");
    assert_eq!(callee_path.as_deref(), Some("b.rs"));
    assert_eq!(stats.resolution.global_unique, 1);
}

#[test]
fn imported_unique_definition_resolves_scoped() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"use other::b;\nfn a() { b(); }\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "scoped");
    assert_eq!(callee_path.as_deref(), Some("b.rs"));
    assert_eq!(stats.resolution.scoped, 1);
}

#[test]
fn ambiguous_definitions_without_import_stay_unresolved() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() { b(); }\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "unresolved");
    assert_eq!(callee_path, None);
    assert_eq!(stats.resolution.unresolved, 1);
}

#[test]
fn call_to_undefined_symbol_resolves_external() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() { ghost(); }\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "external");
    assert_eq!(callee_path, None);
    assert_eq!(stats.resolution.external, 1);
    assert_eq!(stats.resolution.unresolved, 0);
}

#[test]
fn external_and_ambiguous_are_distinguished() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { ghost(); dup(); }\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn dup() {}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"fn dup() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT relationships.callee_name, relationships.resolution
             FROM relationships JOIN symbols ON symbols.id = relationships.caller_symbol_id
             WHERE symbols.name = 'a'
             ORDER BY relationships.callee_name",
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
            ("dup".to_string(), "unresolved".to_string()),
            ("ghost".to_string(), "external".to_string()),
        ]
    );
    assert_eq!(stats.resolution.external, 1);
    assert_eq!(stats.resolution.unresolved, 1);
}

#[test]
fn same_file_definition_wins_over_other_file_definition() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { b(); }\nfn b() {}\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "same-file");
    assert_eq!(callee_path.as_deref(), Some("a.rs"));
    assert_eq!(stats.resolution.same_file, 1);
    assert_eq!(stats.resolution.global_unique, 0);
}

#[test]
fn resolution_stats_match_actual_relationship_counts() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"use other::c;\nfn a() { b(); c(); ghost(); }\nfn b() {}\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"fn c() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM relationships WHERE resolution = 'same-file'"
        ) as usize,
        stats.resolution.same_file
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM relationships WHERE resolution = 'scoped'"
        ) as usize,
        stats.resolution.scoped
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM relationships WHERE resolution = 'global-unique'"
        ) as usize,
        stats.resolution.global_unique
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM relationships WHERE resolution = 'external'"
        ) as usize,
        stats.resolution.external
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM relationships WHERE resolution = 'unresolved'"
        ) as usize,
        stats.resolution.unresolved
    );

    assert_eq!(stats.resolution.same_file, 1);
    assert_eq!(stats.resolution.scoped, 1);
    assert_eq!(stats.resolution.external, 1);
    assert_eq!(stats.resolution.unresolved, 0);
}

#[test]
fn reindexing_recomputes_resolution_without_stale_labels() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() { b(); }\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();

    let first = index_repo(&root, &conn, &parser).unwrap();
    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "external");
    assert_eq!(callee_path, None);
    assert_eq!(first.resolution.external, 1);

    fs::write(repo_dir.path().join("b.rs"), b"fn b() {}\n").unwrap();
    let second = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "global-unique");
    assert_eq!(callee_path.as_deref(), Some("b.rs"));
    assert_eq!(second.resolution.global_unique, 1);
    assert_eq!(second.resolution.external, 0);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM relationships"), 1);
}

#[test]
fn typescript_wildcard_reexport_is_not_treated_as_scoped_import() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.ts"),
        b"export * from \"dup\";\nfunction a() { dup(); }\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.ts"), b"function dup() {}\n").unwrap();
    fs::write(repo_dir.path().join("c.ts"), b"function dup() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = ts_registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "unresolved");
    assert_eq!(callee_path, None);
    assert_eq!(stats.resolution.scoped, 0);
    assert_eq!(stats.resolution.unresolved, 1);
}

#[test]
fn typescript_namespace_import_is_treated_as_scoped_import() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.ts"),
        b"import * as dup from \"m\";\nfunction a() { dup(); }\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.ts"), b"function dup() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = ts_registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (resolution, callee_path) = relationship_for_caller(&conn, "a");
    assert_eq!(resolution, "scoped");
    assert_eq!(callee_path.as_deref(), Some("b.ts"));
    assert_eq!(stats.resolution.scoped, 1);
}

#[test]
fn tsx_component_dependency_graph_resolves_through_the_pipeline() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("page.tsx"),
        b"export function Local() {\n    return <span />;\n}\n\nexport function Page() {\n    return (\n        <div>\n            <Local />\n            <Header />\n        </div>\n    );\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("header.tsx"),
        b"export function Header() {\n    return <div>Header</div>;\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = tsx_registry();

    let stats = index_repo(&root, &conn, &parser).unwrap();

    let (local_resolution, local_callee_path) = relationship_for_named_callee(&conn, "Local");
    assert_eq!(local_resolution, "same-file");
    assert_eq!(local_callee_path.as_deref(), Some("page.tsx"));

    let (header_resolution, header_callee_path) = relationship_for_named_callee(&conn, "Header");
    assert_eq!(header_resolution, "global-unique");
    assert_eq!(header_callee_path.as_deref(), Some("header.tsx"));

    assert_eq!(stats.resolution.same_file, 1);
    assert_eq!(stats.resolution.global_unique, 1);
}
