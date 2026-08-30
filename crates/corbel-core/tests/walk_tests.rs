use corbel_core::path::RepoRoot;
use corbel_core::walk::{WalkConfig, walk_repo};
use std::fs;
use tempfile::tempdir;

fn config(extensions: &[&str]) -> WalkConfig {
    WalkConfig {
        extensions: extensions.iter().map(|s| s.to_string()).collect(),
        follow_symlinks: false,
    }
}

#[test]
fn includes_configured_extensions_and_excludes_others() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), b"fn main() {}").unwrap();
    fs::write(dir.path().join("script.py"), b"print(1)").unwrap();
    fs::write(dir.path().join("notes.txt"), b"hello").unwrap();
    fs::write(dir.path().join("README.md"), b"# readme").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let files = walk_repo(&root, &config(&["rs", "py"])).unwrap();

    let names: Vec<String> = files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["main.rs".to_string(), "script.py".to_string()]);
}

#[test]
fn respects_gitignore() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".gitignore"), b"ignored.rs\nbuild/\n").unwrap();
    fs::write(dir.path().join("ignored.rs"), b"fn a() {}").unwrap();
    fs::write(dir.path().join("kept.rs"), b"fn b() {}").unwrap();
    fs::create_dir_all(dir.path().join("build")).unwrap();
    fs::write(dir.path().join("build").join("gen.rs"), b"fn c() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let files = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["kept.rs".to_string()]);
}

#[test]
fn excludes_git_directory_even_if_extension_matches() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".git").join("config.rs"), b"fn a() {}").unwrap();
    fs::write(dir.path().join("kept.rs"), b"fn b() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let files = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["kept.rs".to_string()]);
}

#[test]
fn finds_files_in_nested_directories_with_correct_rel_path() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("deep.rs"), b"fn a() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let files = walk_repo(&root, &config(&["rs"])).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path.to_string(), "a/b/deep.rs");
    assert_eq!(files[0].extension, "rs");
}

#[test]
fn empty_repo_returns_empty_vec() {
    let dir = tempdir().unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let files = walk_repo(&root, &config(&["rs"])).unwrap();

    assert!(files.is_empty());
}

#[test]
fn walk_repo_is_deterministic_across_calls() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("b.rs"), b"fn b() {}").unwrap();
    fs::write(dir.path().join("a.rs"), b"fn a() {}").unwrap();
    fs::write(dir.path().join("c.rs"), b"fn c() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let first = walk_repo(&root, &config(&["rs"])).unwrap();
    let second = walk_repo(&root, &config(&["rs"])).unwrap();

    assert_eq!(first, second);
}

#[cfg(unix)]
#[test]
fn symlink_escaping_root_does_not_panic_walk() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let outside_dir = tempdir().unwrap();
    let outside_file = outside_dir.path().join("secret.rs");
    fs::write(&outside_file, b"fn secret() {}").unwrap();

    symlink(&outside_file, dir.path().join("escape.rs")).unwrap();
    fs::write(dir.path().join("normal.rs"), b"fn normal() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let files = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["normal.rs".to_string()]);
}
