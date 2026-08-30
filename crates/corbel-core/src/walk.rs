use std::fmt;
use std::path::Path;

use crate::error::Result;
use crate::path::{RelPath, RepoRoot};
use ignore::WalkBuilder;

#[derive(Clone, Debug, PartialEq)]
pub struct WalkedFile {
    pub path: RelPath,
    pub extension: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    WalkError(String),
    Symlink,
    Io(String),
    OutsideRoot,
    NotUtf8,
    TooLarge { size: u64, limit: u64 },
}

impl SkipReason {
    pub fn category(&self) -> &'static str {
        match self {
            SkipReason::WalkError(_) => "walk error",
            SkipReason::Symlink => "symlink",
            SkipReason::Io(_) => "I/O error",
            SkipReason::OutsideRoot => "outside repository root",
            SkipReason::NotUtf8 => "not valid UTF-8",
            SkipReason::TooLarge { .. } => "too large",
        }
    }
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkipReason::WalkError(msg) => write!(f, "could not read directory entry: {msg}"),
            SkipReason::Symlink => write!(f, "symbolic link, not followed"),
            SkipReason::Io(msg) => write!(f, "could not read file: {msg}"),
            SkipReason::OutsideRoot => write!(f, "resolves outside the repository root"),
            SkipReason::NotUtf8 => write!(f, "not valid UTF-8 (likely a binary file)"),
            SkipReason::TooLarge { size, limit } => {
                write!(
                    f,
                    "file is {size} bytes, exceeds the {limit} byte indexing limit"
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub path: Option<String>,
    pub reason: SkipReason,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalkConfig {
    pub extensions: Vec<String>,
    pub follow_symlinks: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WalkResult {
    pub files: Vec<WalkedFile>,
    pub skipped: Vec<SkippedFile>,
}

fn display_path(root: &RepoRoot, path: &Path) -> String {
    path.strip_prefix(root.as_path())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub fn walk_repo(root: &RepoRoot, config: &WalkConfig) -> Result<WalkResult> {
    let mut walker = WalkBuilder::new(root.as_path());
    walker.follow_links(config.follow_symlinks);
    walker.standard_filters(true);

    let mut files = Vec::new();
    let mut skipped = Vec::new();

    for entry in walker.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                skipped.push(SkippedFile {
                    path: None,
                    reason: SkipReason::WalkError(err.to_string()),
                });
                continue;
            }
        };

        if entry
            .path()
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            continue;
        }

        let file_type = match entry.file_type() {
            Some(file_type) => file_type,
            None => continue,
        };

        if file_type.is_symlink() {
            skipped.push(SkippedFile {
                path: Some(display_path(root, entry.path())),
                reason: SkipReason::Symlink,
            });
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let extension = match entry.path().extension().and_then(|ext| ext.to_str()) {
            Some(extension) => extension.to_string(),
            None => continue,
        };

        if !config
            .extensions
            .iter()
            .any(|allowed| allowed == &extension)
        {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                skipped.push(SkippedFile {
                    path: Some(display_path(root, entry.path())),
                    reason: SkipReason::Io(err.to_string()),
                });
                continue;
            }
        };

        let rel_path = match root.relativize(entry.path()) {
            Ok(rel_path) => rel_path,
            Err(_) => {
                skipped.push(SkippedFile {
                    path: Some(display_path(root, entry.path())),
                    reason: SkipReason::OutsideRoot,
                });
                continue;
            }
        };

        files.push(WalkedFile {
            path: rel_path,
            extension,
            size: metadata.len(),
        });
    }

    files.sort_by(|a, b| a.path.as_ref().cmp(b.path.as_ref()));
    skipped.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(WalkResult { files, skipped })
}
