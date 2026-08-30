use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

fn corbel_cmd() -> Command {
    Command::cargo_bin("corbel").unwrap()
}

#[test]
fn index_succeeds_and_prints_summary() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() {}\n").unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"))
        .stdout(predicate::str::contains("symbols"))
        .stdout(predicate::str::contains("Internal calls:"))
        .stdout(predicate::str::contains("External calls:"));
}

#[test]
fn index_creates_database_file() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() {}\n").unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();

    let db_path = repo_dir.path().join(".corbel").join("index.db");
    assert!(db_path.exists());

    let conn = Connection::open(&db_path).unwrap();
    let file_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .unwrap();
    assert_eq!(file_count, 1);
}

#[test]
fn summary_reports_file_and_symbol_counts() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\nfn b() {}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"struct C;\n").unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 2 files (0 unchanged)"))
        .stdout(predicate::str::contains("3 symbols"));
}

#[test]
fn second_run_reports_unchanged_files() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() {}\n").unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 0 files (1 unchanged)"));
}

#[test]
fn nonexistent_path_fails_with_nonzero_exit() {
    let repo_dir = tempdir().unwrap();
    let missing = repo_dir.path().join("does-not-exist");

    corbel_cmd()
        .arg("index")
        .arg(&missing)
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());
}

#[test]
fn resolution_counts_sum_to_relationships_stored() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { b(); c(); }\nfn b() {}\n",
    )
    .unwrap();
    fs::write(repo_dir.path().join("d.rs"), b"fn c() {}\n").unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();

    let db_path = repo_dir.path().join(".corbel").join("index.db");
    let conn = Connection::open(&db_path).unwrap();

    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))
        .unwrap();
    let summed: i64 = conn
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM relationships WHERE resolution = 'same-file') +
                (SELECT COUNT(*) FROM relationships WHERE resolution = 'scoped') +
                (SELECT COUNT(*) FROM relationships WHERE resolution = 'global-unique') +
                (SELECT COUNT(*) FROM relationships WHERE resolution = 'external') +
                (SELECT COUNT(*) FROM relationships WHERE resolution = 'unresolved')",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(total, summed);
    assert!(total > 0);
}

#[test]
fn external_calls_are_excluded_from_internal_resolution_percentage() {
    let repo_dir = tempdir().unwrap();
    fs::write(
        repo_dir.path().join("a.rs"),
        b"fn a() { b(); unwrap(); }\nfn b() {}\n",
    )
    .unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Internal calls: 1 resolved / 1 total (100.0%)",
        ))
        .stdout(predicate::str::contains("External calls: 1"));
}

#[test]
fn verbose_flag_keeps_stdout_summary_and_adds_stderr_logs() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() {}\n").unwrap();

    corbel_cmd()
        .arg("-v")
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"))
        .stdout(predicate::str::contains("Internal calls:"))
        .stderr(predicate::str::is_empty().not());
}
