use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::tempdir;

fn corbel_cmd() -> Command {
    Command::cargo_bin("corbel").unwrap()
}

fn git(root: &std::path::Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .expect("git command runs");
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo_with_commit(root: &std::path::Path, file: &str, content: &[u8]) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    fs::write(root.join(file), content).unwrap();
    git(root, &["add", file]);
    git(root, &["commit", "-q", "-m", "init"]);
}

const LIB_RS: &[u8] = b"fn a() {\n    println!(\"a\");\n}\n\nfn b() {\n    a();\n}\n";

/// Regression test for the bug this session fixed: indexing right after a
/// commit (so the index matches HEAD), then editing a file's body without
/// re-indexing, must NOT trigger a stale-index warning and must still
/// produce a normal coverage report. The prior (working-tree-hash-based)
/// staleness check flagged every edited file unconditionally, since editing
/// a file always changes its hash relative to the last index — making
/// `audit` unusable for its primary purpose.
#[test]
fn edit_after_index_without_reindexing_produces_normal_report_not_a_stale_warning() {
    let repo_dir = tempdir().unwrap();
    init_repo_with_commit(repo_dir.path(), "lib.rs", LIB_RS);

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();

    // Edit inside fn b()'s body without touching the index.
    let edited = b"fn a() {\n    println!(\"a\");\n}\n\nfn b() {\n    let _x = 1;\n    a();\n}\n";
    fs::write(repo_dir.path().join("lib.rs"), edited).unwrap();

    corbel_cmd()
        .arg("audit")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("stale").not())
        .stdout(predicate::str::contains("out of sync").not())
        .stdout(predicate::str::contains("Changed symbols:"))
        .stdout(predicate::str::contains("b (lib.rs:5)"));
}

/// When the index reflects neither HEAD nor a state audit can otherwise
/// verify (e.g. it was built mid-edit, before that edit was committed),
/// `audit` must warn and exclude the file rather than silently mapping
/// hunks to the wrong symbols.
#[test]
fn index_built_from_a_third_state_triggers_out_of_sync_warning() {
    let repo_dir = tempdir().unwrap();
    init_repo_with_commit(repo_dir.path(), "lib.rs", LIB_RS);

    // Edit before indexing, so the index reflects neither HEAD nor (after
    // the next edit) the working tree.
    let mid_edit = b"// shifted\nfn a() {\n    println!(\"a\");\n}\n\nfn b() {\n    a();\n}\n";
    fs::write(repo_dir.path().join("lib.rs"), mid_edit).unwrap();
    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();

    // Edit again without re-indexing.
    let edited_again =
        b"// shifted\nfn a() {\n    println!(\"a\");\n}\n\nfn b() {\n    let _x = 1;\n    a();\n}\n";
    fs::write(repo_dir.path().join("lib.rs"), edited_again).unwrap();

    corbel_cmd()
        .arg("audit")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("out of sync with HEAD"))
        .stdout(predicate::str::contains("lib.rs"))
        .stdout(predicate::str::contains("Skipping coverage analysis"));
}

/// A file changed on disk but never indexed at all (e.g. newly created)
/// should be reported distinctly from a stale/out-of-sync index — there's
/// no coordinate risk to warn about, just nothing to check coverage
/// against yet.
#[test]
fn unindexed_new_file_is_reported_separately_from_stale_index() {
    let repo_dir = tempdir().unwrap();
    init_repo_with_commit(repo_dir.path(), "lib.rs", LIB_RS);

    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();

    fs::write(repo_dir.path().join("new.rs"), b"fn c() {}\n").unwrap();
    // Intent-to-add so `git diff HEAD` reports it as a new file without
    // fully staging it.
    git(repo_dir.path(), &["add", "-N", "new.rs"]);

    corbel_cmd()
        .arg("audit")
        .arg(repo_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("not yet indexed"))
        .stdout(predicate::str::contains("new.rs"))
        .stdout(predicate::str::contains("out of sync").not());
}
