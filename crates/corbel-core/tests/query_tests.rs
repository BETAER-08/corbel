use corbel_core::budget::TokenBudget;
use corbel_core::index::index_repo;
use corbel_core::path::RepoRoot;
use corbel_core::query::{find, get_symbol};
use corbel_core::store::migrate::open_connection;
use corbel_lang::langs::rust::RustSupport;
use corbel_lang::registry::LanguageRegistry;
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

const AMPLE_BUDGET: usize = 100_000;

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

#[test]
fn call_chain_populates_callers_and_callees() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {\n    c();\n}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"pub fn c() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "b", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert_eq!(result.symbol.name, "b");
    assert_eq!(result.symbol.file, "b.rs");

    assert_eq!(result.callers.len(), 1);
    assert_eq!(result.callers[0].name, "a");
    assert_eq!(result.callers[0].file, "a.rs");
    assert_eq!(result.callers[0].line, 1);
    assert_eq!(result.callers[0].resolution, "global-unique");

    assert_eq!(result.callees.len(), 1);
    assert_eq!(result.callees[0].name, "c");
    assert_eq!(result.callees[0].file.as_deref(), Some("c.rs"));
    assert_eq!(result.callees[0].resolution, "global-unique");
}

#[test]
fn unknown_symbol_returns_empty_vector() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(
        &conn,
        "does_not_exist",
        None,
        None,
        TokenBudget::new(AMPLE_BUDGET),
    )
    .unwrap();
    assert!(results.is_empty());
}

#[test]
fn duplicate_name_across_files_yields_two_results_narrowed_by_file() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("p.rs"),
        b"pub fn shared() -> i32 {\n    1\n}\n\npub fn call_p() -> i32 {\n    shared()\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("q.rs"),
        b"pub fn shared() -> i32 {\n    2\n}\n\npub fn call_q() -> i32 {\n    shared()\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "shared", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].symbol.file, "p.rs");
    assert_eq!(results[1].symbol.file, "q.rs");

    let narrowed = get_symbol(
        &conn,
        "shared",
        Some("q.rs"),
        None,
        TokenBudget::new(AMPLE_BUDGET),
    )
    .unwrap();
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].symbol.file, "q.rs");
}

#[test]
fn caller_is_attributed_only_to_the_definition_in_the_same_file() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("p.rs"),
        b"pub fn shared() -> i32 {\n    1\n}\n\npub fn call_p() -> i32 {\n    shared()\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("q.rs"),
        b"pub fn shared() -> i32 {\n    2\n}\n\npub fn call_q() -> i32 {\n    shared()\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "shared", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    let p_result = results.iter().find(|r| r.symbol.file == "p.rs").unwrap();
    let q_result = results.iter().find(|r| r.symbol.file == "q.rs").unwrap();

    assert_eq!(p_result.callers.len(), 1);
    assert_eq!(p_result.callers[0].name, "call_p");
    assert_eq!(p_result.callers[0].file, "p.rs");
    assert_eq!(p_result.callers[0].resolution, "same-file");

    assert_eq!(q_result.callers.len(), 1);
    assert_eq!(q_result.callers[0].name, "call_q");
    assert_eq!(q_result.callers[0].file, "q.rs");
    assert_eq!(q_result.callers[0].resolution, "same-file");
}

#[test]
fn external_call_has_no_file_and_external_resolution() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn a() {\n    nonexistent_external();\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "a", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].callees.len(), 1);
    assert_eq!(results[0].callees[0].name, "nonexistent_external");
    assert_eq!(results[0].callees[0].file, None);
    assert_eq!(results[0].callees[0].resolution, "external");
}

#[test]
fn ambiguous_call_from_third_file_is_unresolved() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("dup1.rs"), b"pub fn dup() {}\n").unwrap();
    fs::write(repo_dir.path().join("dup2.rs"), b"pub fn dup() {}\n").unwrap();
    fs::write(
        repo_dir.path().join("caller.rs"),
        b"pub fn ambiguous_caller() {\n    dup();\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(
        &conn,
        "ambiguous_caller",
        None,
        None,
        TokenBudget::new(AMPLE_BUDGET),
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].callees.len(), 1);
    assert_eq!(results[0].callees[0].name, "dup");
    assert_eq!(results[0].callees[0].file, None);
    assert_eq!(results[0].callees[0].resolution, "unresolved");
}

#[test]
fn symbol_info_matches_definition() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn a(x: i32) -> i32 {\n    x\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "a", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(results.len(), 1);
    let symbol = &results[0].symbol;

    assert_eq!(symbol.name, "a");
    assert_eq!(symbol.file, "a.rs");
    assert_eq!(symbol.line, 1);
    assert_eq!(symbol.kind, "function");
    assert_eq!(symbol.signature.as_deref(), Some("pub fn a(x: i32) -> i32"));
    assert!(symbol.is_public);
}

#[test]
fn repeated_queries_return_identical_order() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {\n    c();\n}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"pub fn c() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let first = get_symbol(&conn, "b", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    let second = get_symbol(&conn, "b", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(first, second);
}

#[test]
fn duplicate_name_within_same_file_is_narrowed_by_line() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("overload.rs"),
        b"pub fn widget(x: i32) -> i32 {\n    x\n}\n\npub fn widget(x: i32, y: i32) -> i32 {\n    x + y\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let both = get_symbol(
        &conn,
        "widget",
        Some("overload.rs"),
        None,
        TokenBudget::new(AMPLE_BUDGET),
    )
    .unwrap();
    assert_eq!(both.len(), 2);

    let narrowed = get_symbol(
        &conn,
        "widget",
        Some("overload.rs"),
        Some(5),
        TokenBudget::new(AMPLE_BUDGET),
    )
    .unwrap();
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].symbol.line, 5);
    assert_eq!(
        narrowed[0].symbol.signature.as_deref(),
        Some("pub fn widget(x: i32, y: i32) -> i32")
    );
}

#[test]
fn find_matches_are_case_insensitive_for_ascii() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn Widget() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "widget", 20, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].name, "Widget");
}

#[test]
fn find_treats_percent_and_underscore_as_literal_characters() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn a_b() {}\n\npub fn acb() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "a_b", 20, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    let names: Vec<&str> = result.matches.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["a_b"]);
}

#[test]
fn find_ranks_exact_then_prefix_then_substring_matches() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn old_widget() {}\n\npub fn widget_factory() {}\n\npub fn widget() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "widget", 20, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    let names: Vec<&str> = result.matches.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["widget", "widget_factory", "old_widget"]);
}

#[test]
fn find_orders_ties_within_a_rank_by_name_then_file_then_line() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("p.rs"), b"pub fn widget_p() {}\n").unwrap();
    fs::write(repo_dir.path().join("q.rs"), b"pub fn widget_q() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "widget", 20, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    let files: Vec<&str> = result.matches.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(files, vec!["p.rs", "q.rs"]);
}

#[test]
fn find_truncates_by_limit_and_reports_total_matches() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn widget_a() {}\n\npub fn widget_b() {}\n\npub fn widget_c() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "widget", 2, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.total_matches, 3);
    assert!(result.truncated);
    assert_eq!(result.truncated_count, 1);
}

#[test]
fn find_truncates_by_token_budget() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"pub fn widget_a() {}\n\npub fn widget_b() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "widget", 20, TokenBudget::new(1)).unwrap();
    assert!(result.matches.is_empty());
    assert_eq!(result.total_matches, 2);
    assert!(result.truncated);
    assert_eq!(result.truncated_count, 2);
}

#[test]
fn find_with_no_matches_returns_empty_result() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "ghost", 20, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert!(result.matches.is_empty());
    assert_eq!(result.total_matches, 0);
    assert!(!result.truncated);
    assert_eq!(result.truncated_count, 0);
}

#[test]
fn find_works_across_all_five_registered_languages() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn widget_rs() {}\n").unwrap();
    fs::write(
        repo_dir.path().join("b.py"),
        b"def widget_py():\n    pass\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("c.ts"),
        b"export function widget_ts() {}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("d.tsx"),
        b"export function widget_tsx() {\n    return null;\n}\n",
    )
    .unwrap();
    fs::write(
        repo_dir.path().join("e.js"),
        b"export function widget_js() {}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = LanguageRegistry::with_all_languages();
    index_repo(&root, &conn, &parser).unwrap();

    let result = find(&conn, "widget", 20, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    let mut names: Vec<&str> = result.matches.iter().map(|m| m.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "widget_js",
            "widget_py",
            "widget_rs",
            "widget_ts",
            "widget_tsx",
        ]
    );
}

#[test]
fn get_symbol_with_ample_budget_is_not_truncated() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {}\n").unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "a", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(results.len(), 1);
    assert!(!results[0].truncated);
    assert_eq!(results[0].truncated_count, 0);
}

#[test]
fn get_symbol_with_zero_budget_truncates_both_nonempty_callers_and_callees() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {}\n").unwrap();
    fs::write(
        repo_dir.path().join("caller_of_a.rs"),
        b"pub fn calls_a() {\n    a();\n}\n",
    )
    .unwrap();

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let ample = get_symbol(&conn, "a", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(
        ample[0].callers.len(),
        1,
        "fixture must give a a real caller"
    );
    assert_eq!(
        ample[0].callees.len(),
        1,
        "fixture must give a a real callee"
    );

    let results = get_symbol(&conn, "a", None, None, TokenBudget::new(0)).unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].callers.is_empty(),
        "the one real caller must have been cut by the zero budget, not merely absent"
    );
    assert!(
        results[0].callees.is_empty(),
        "the one real callee must have been cut by the zero budget, not merely absent"
    );
    assert!(results[0].truncated);
    assert_eq!(results[0].truncated_count, 2);
}

#[test]
fn get_symbol_splits_budget_evenly_across_ambiguous_symbol_rows() {
    let repo_dir = tempdir().unwrap();
    for prefix in ["p", "q"] {
        let mut src = String::from("pub fn shared() {}\n");
        for i in 0..3 {
            src.push_str(&format!(
                "pub fn {prefix}_caller_{i}() {{\n    shared();\n}}\n"
            ));
        }
        fs::write(repo_dir.path().join(format!("{prefix}.rs")), src).unwrap();
    }

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let ample = get_symbol(&conn, "shared", None, None, TokenBudget::new(AMPLE_BUDGET)).unwrap();
    assert_eq!(ample.len(), 2, "fixture must produce two ambiguous rows");
    assert_eq!(ample[0].callers.len(), 3);
    assert_eq!(ample[1].callers.len(), 3);

    let results = get_symbol(&conn, "shared", None, None, TokenBudget::new(160)).unwrap();
    assert_eq!(results.len(), 2);

    for (i, result) in results.iter().enumerate() {
        assert!(
            !result.callers.is_empty(),
            "row {i} must not be starved to zero just because it was processed second"
        );
        assert!(
            result.callers.len() < 3,
            "row {i} should still be truncated given only a fraction of the total budget"
        );
    }
}

#[test]
fn get_symbol_splits_budget_so_many_callers_do_not_starve_callees() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("hot.rs"),
        b"pub fn hot() {\n    helper();\n}\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("helper.rs"), b"pub fn helper() {}\n").unwrap();
    for i in 0..5 {
        fs::write(
            repo_dir.path().join(format!("caller_{i}.rs")),
            format!("pub fn caller_{i}() {{\n    hot();\n}}\n").as_bytes(),
        )
        .unwrap();
    }

    let root = RepoRoot::new(repo_dir.path()).unwrap();
    let (conn, _db_dir) = db();
    let parser = registry();
    index_repo(&root, &conn, &parser).unwrap();

    let results = get_symbol(&conn, "hot", None, None, TokenBudget::new(100)).unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];

    assert!(
        result.callers.len() < 5,
        "expected the tiny budget to truncate at least one of the 5 callers"
    );
    assert_eq!(
        result.callees.len(),
        1,
        "the single callee must not be starved by the much larger callers list"
    );
    assert!(result.truncated);
    assert!(result.truncated_count > 0);
}
