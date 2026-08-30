use corbel_core::budget::{TokenBudget, estimate_node_tokens};
use corbel_core::index::index_repo;
use corbel_core::path::RepoRoot;
use corbel_core::query::impact;
use corbel_core::store::migrate::open_connection;
use corbel_lang::langs::rust::RustSupport;
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

fn generous_budget() -> TokenBudget {
    TokenBudget::new(8000)
}

#[test]
fn direct_caller_is_captured_at_depth_one() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    t();\n}\n").unwrap();
    fs::write(repo_dir.path().join("t.rs"), b"pub fn t() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "t", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert_eq!(result.target.name, "t");
    assert_eq!(result.affected.len(), 1);
    assert_eq!(result.affected[0].name, "a");
    assert_eq!(result.affected[0].file, "a.rs");
    assert_eq!(result.affected[0].depth, 1);
    assert!(!result.truncated);
    assert_eq!(result.truncated_count, 0);
    assert_eq!(result.max_depth_reached, 1);
}

#[test]
fn multi_level_chain_is_captured_with_increasing_depth() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {\n    t();\n}\n").unwrap();
    fs::write(repo_dir.path().join("t.rs"), b"pub fn t() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "t", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert_eq!(result.affected.len(), 2);

    let b_node = result.affected.iter().find(|n| n.name == "b").unwrap();
    assert_eq!(b_node.depth, 1);

    let a_node = result.affected.iter().find(|n| n.name == "a").unwrap();
    assert_eq!(a_node.depth, 2);

    assert!(!result.truncated);
    assert_eq!(result.max_depth_reached, 2);
}

#[test]
fn cycle_terminates_without_infinite_loop_and_visits_each_symbol_once() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn a() {\n    t();\n    b();\n}\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {\n    a();\n}\n").unwrap();
    fs::write(repo_dir.path().join("t.rs"), b"pub fn t() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "t", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert_eq!(result.affected.len(), 2);
    let names: Vec<&str> = result.affected.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names.iter().filter(|&&n| n == "a").count(), 1);
    assert_eq!(names.iter().filter(|&&n| n == "b").count(), 1);

    let a_node = result.affected.iter().find(|n| n.name == "a").unwrap();
    assert_eq!(a_node.depth, 1);
    let b_node = result.affected.iter().find(|n| n.name == "b").unwrap();
    assert_eq!(b_node.depth, 2);
}

#[test]
fn symbol_reached_via_multiple_paths_keeps_shortest_depth() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("t.rs"), b"pub fn t() {}\n").unwrap();
    fs::write(repo_dir.path().join("y.rs"), b"pub fn y() {\n    t();\n}\n").unwrap();
    fs::write(
        repo_dir.path().join("x.rs"),
        b"pub fn x() {\n    t();\n    y();\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "t", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert_eq!(result.affected.len(), 2);

    let x_node = result.affected.iter().find(|n| n.name == "x").unwrap();
    assert_eq!(x_node.depth, 1);

    let y_node = result.affected.iter().find(|n| n.name == "y").unwrap();
    assert_eq!(y_node.depth, 1);
}

#[test]
fn tight_budget_truncates_and_keeps_closest_nodes() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("caller_a.rs"),
        b"pub fn caller_a() {\n    t();\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("caller_b.rs"),
        b"pub fn caller_b() {\n    t();\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("caller_c.rs"),
        b"pub fn caller_c() {\n    t();\n}\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("t.rs"), b"pub fn t() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let tight_limit = estimate_node_tokens("caller_a", "caller_a.rs", 1, "global-unique")
        + estimate_node_tokens("caller_b", "caller_b.rs", 1, "global-unique");
    let budget = TokenBudget::new(tight_limit);

    let results = impact(&conn, "t", None, budget).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert!(result.truncated);
    assert_eq!(result.truncated_count, 1);
    assert_eq!(result.affected.len(), 2);
    assert_eq!(result.affected[0].name, "caller_a");
    assert_eq!(result.affected[1].name, "caller_b");
    assert!(result.affected.iter().all(|n| n.depth == 1));
}

#[test]
fn generous_budget_captures_full_graph_without_truncation() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {\n    t();\n}\n").unwrap();
    fs::write(repo_dir.path().join("t.rs"), b"pub fn t() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "t", None, generous_budget()).unwrap();
    let result = &results[0];

    assert!(!result.truncated);
    assert_eq!(result.truncated_count, 0);
    assert_eq!(result.affected.len(), 2);
}

#[test]
fn outgoing_external_call_from_target_is_irrelevant_to_impact() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("t.rs"),
        b"pub fn t() {\n    nonexistent_external();\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "t", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].affected.is_empty());
}

#[test]
fn symbol_with_no_callers_has_empty_affected() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("lonely.rs"), b"pub fn lonely() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "lonely", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].affected.is_empty());
    assert!(!results[0].truncated);
    assert_eq!(results[0].max_depth_reached, 0);
}

#[test]
fn duplicate_name_across_files_produces_independent_impact_graphs() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("p.rs"),
        b"pub fn shared() {}\n\npub fn call_p() {\n    shared();\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("q.rs"),
        b"pub fn shared() {}\n\npub fn call_q() {\n    shared();\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = impact(&conn, "shared", None, generous_budget()).unwrap();
    assert_eq!(results.len(), 2);

    let p_result = results.iter().find(|r| r.target.file == "p.rs").unwrap();
    assert_eq!(p_result.affected.len(), 1);
    assert_eq!(p_result.affected[0].name, "call_p");

    let q_result = results.iter().find(|r| r.target.file == "q.rs").unwrap();
    assert_eq!(q_result.affected.len(), 1);
    assert_eq!(q_result.affected[0].name, "call_q");
}
