use corbel_core::error::Error;
use corbel_core::path::RepoRoot;
use std::fs;
use tempfile::tempdir;

#[test]
fn relativize_simple_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("foo.txt");
    fs::write(&file_path, b"x").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let rel = root.relativize(&file_path).unwrap();

    assert_eq!(rel.as_ref(), "foo.txt");
    assert_eq!(rel.to_string(), "foo.txt");
}

#[test]
fn relativize_nested_directories() {
    let dir = tempdir().unwrap();
    let nested_dir = dir.path().join("a").join("b");
    fs::create_dir_all(&nested_dir).unwrap();
    let file_path = nested_dir.join("c.txt");
    fs::write(&file_path, b"x").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let rel = root.relativize(&file_path).unwrap();

    assert_eq!(rel.as_ref(), "a/b/c.txt");
}

#[test]
fn relativize_root_itself_is_error() {
    let dir = tempdir().unwrap();
    let root = RepoRoot::new(dir.path()).unwrap();

    let result = root.relativize(dir.path());

    assert!(matches!(result, Err(Error::PathEscapesRoot { .. })));
}

#[test]
fn relativize_path_outside_root_is_error() {
    let dir = tempdir().unwrap();
    let outside_dir = tempdir().unwrap();
    let outside_file = outside_dir.path().join("out.txt");
    fs::write(&outside_file, b"x").unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = root.relativize(&outside_file);

    assert!(matches!(result, Err(Error::PathEscapesRoot { .. })));
}

#[test]
fn repo_root_new_nonexistent_path_is_error() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");

    let result = RepoRoot::new(&missing);

    assert!(result.is_err());
}

#[test]
fn repo_root_new_file_path_is_error() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, b"x").unwrap();

    let result = RepoRoot::new(&file_path);

    assert!(matches!(result, Err(Error::InvalidRepoRoot { .. })));
}

#[cfg(unix)]
#[test]
fn relativize_symlink_escaping_root_is_error() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let outside_dir = tempdir().unwrap();
    let outside_file = outside_dir.path().join("secret.txt");
    fs::write(&outside_file, b"x").unwrap();

    let link_path = dir.path().join("escape_link");
    symlink(&outside_file, &link_path).unwrap();

    let root = RepoRoot::new(dir.path()).unwrap();
    let result = root.relativize(&link_path);

    assert!(matches!(result, Err(Error::PathEscapesRoot { .. })));
}
