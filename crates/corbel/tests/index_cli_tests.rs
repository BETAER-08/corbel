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

#[test]
fn binary_file_is_skipped_and_summarized() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("good.rs"), b"fn good() {}\n").unwrap();
    fs::write(
        repo_dir.path().join("bad.rs"),
        [0xff_u8, 0xfe, 0x00, 0x01, 0xff, 0xff],
    )
    .unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 1 files (0 unchanged)"))
        .stdout(predicate::str::contains("Skipped 1 file(s):"))
        .stdout(predicate::str::contains("not valid UTF-8"));
}

#[test]
fn oversized_file_is_skipped_and_summarized() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("small.rs"), b"fn small() {}\n").unwrap();
    let huge = vec![b'a'; 6 * 1024 * 1024];
    fs::write(repo_dir.path().join("huge.rs"), &huge).unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 1 files (0 unchanged)"))
        .stdout(predicate::str::contains("Skipped 1 file(s):"))
        .stdout(predicate::str::contains("too large"));
}

#[test]
fn bom_prefixed_file_is_indexed_without_being_skipped() {
    let repo_dir = tempdir().unwrap();
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(b"fn with_bom() {}\n");
    fs::write(repo_dir.path().join("a.rs"), &content).unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 1 files (0 unchanged)"))
        .stdout(predicate::str::contains("1 symbols"))
        .stdout(predicate::str::contains("Skipped").not());
}

#[test]
fn no_skip_line_when_nothing_was_skipped() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"fn a() {}\n").unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped").not());
}

#[test]
fn verbose_flag_reports_skipped_file_path_and_reason_on_stderr() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("bad.rs"), [0xff_u8, 0xfe, 0x00, 0x01]).unwrap();

    corbel_cmd()
        .arg("-v")
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("bad.rs"))
        .stderr(predicate::str::contains("not valid UTF-8"));
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_skipped_summarized_and_does_not_fail_the_command() {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("readable.rs"), b"fn ok() {}\n").unwrap();
    let locked_path = repo_dir.path().join("locked.rs");
    fs::write(&locked_path, b"fn locked() {}\n").unwrap();
    fs::set_permissions(&locked_path, Permissions::from_mode(0o000)).unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 1 files (0 unchanged)"))
        .stdout(predicate::str::contains("Skipped 1 file(s):"))
        .stdout(predicate::str::contains("I/O error"));

    fs::set_permissions(&locked_path, Permissions::from_mode(0o644)).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_file_is_skipped_and_summarized() {
    use std::os::unix::fs::symlink;

    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("real.rs"), b"fn real() {}\n").unwrap();
    symlink(
        repo_dir.path().join("real.rs"),
        repo_dir.path().join("link.rs"),
    )
    .unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 1 files (0 unchanged)"))
        .stdout(predicate::str::contains("Skipped 1 file(s):"))
        .stdout(predicate::str::contains("symlink"));
}

#[test]
fn empty_repository_indexes_successfully() {
    let repo_dir = tempdir().unwrap();

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 0 files (0 unchanged)"));
}
