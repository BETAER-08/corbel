use corbel_core::path::RepoRoot;
use corbel_core::walk::{SkipReason, WalkConfig, walk_repo};
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
    let result = walk_repo(&root, &config(&["rs", "py"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["main.rs".to_string(), "script.py".to_string()]);
    assert!(result.skipped.is_empty());
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
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["kept.rs".to_string()]);
}

#[test]
fn excludes_git_directory_even_if_extension_matches() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".git").join("config.rs"), b"fn a() {}").unwrap();
    fs::write(dir.path().join("kept.rs"), b"fn b() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["kept.rs".to_string()]);
}

#[test]
fn finds_files_in_nested_directories_with_correct_rel_path() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("deep.rs"), b"fn a() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].path.to_string(), "a/b/deep.rs");
    assert_eq!(result.files[0].extension, "rs");
}

#[test]
fn empty_repo_returns_empty_vec() {
    let dir = tempdir().unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    assert!(result.files.is_empty());
    assert!(result.skipped.is_empty());
}

#[test]
fn repo_without_git_directory_walks_normally() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.rs"), b"fn a() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["a.rs".to_string()]);
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

#[test]
fn non_ascii_utf8_path_is_walked_correctly() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("café_한글.rs"), b"fn a() {}").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["café_한글.rs".to_string()]);
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
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["normal.rs".to_string()]);

    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path.as_deref(), Some("escape.rs"));
    assert_eq!(result.skipped[0].reason, SkipReason::Symlink);
}

#[cfg(unix)]
#[test]
fn symlink_within_root_is_skipped_and_reported_not_silently_dropped() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    fs::write(dir.path().join("real.rs"), b"fn real() {}").unwrap();
    symlink(dir.path().join("real.rs"), dir.path().join("link.rs")).unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"])).unwrap();

    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["real.rs".to_string()]);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path.as_deref(), Some("link.rs"));
    assert_eq!(result.skipped[0].reason, SkipReason::Symlink);
}

#[cfg(unix)]
#[test]
fn unreadable_directory_is_reported_as_walk_error() {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("hidden.rs"), b"fn hidden() {}").unwrap();
    fs::write(dir.path().join("visible.rs"), b"fn visible() {}").unwrap();

    fs::set_permissions(&locked, Permissions::from_mode(0o000)).unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = walk_repo(&root, &config(&["rs"]));

    fs::set_permissions(&locked, Permissions::from_mode(0o755)).unwrap();

    let result = result.unwrap();
    let names: Vec<String> = result.files.iter().map(|f| f.path.to_string()).collect();
    assert_eq!(names, vec!["visible.rs".to_string()]);
    assert!(
        result
            .skipped
            .iter()
            .any(|s| matches!(s.reason, SkipReason::WalkError(_)))
    );
}
