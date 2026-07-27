use crate::error::Result;
use crate::path::{RelPath, RepoRoot};
use ignore::WalkBuilder;

#[derive(Clone, Debug, PartialEq)]
pub struct WalkedFile {
    pub path: RelPath,
    pub extension: String,
    pub size: u64,
}

#[derive(Clone, Debug, Default)]
pub struct WalkConfig {
    pub extensions: Vec<String>,
    pub follow_symlinks: bool,
}

pub fn walk_repo(root: &RepoRoot, config: &WalkConfig) -> Result<Vec<WalkedFile>> {
    let mut walker = WalkBuilder::new(root.as_path());
    walker.follow_links(config.follow_symlinks);
    walker.standard_filters(true);

    let mut files = Vec::new();

    for entry in walker.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                tracing::warn!(error = %err, "skipping walk entry due to error");
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
                tracing::warn!(path = %entry.path().display(), error = %err, "skipping file due to metadata error");
                continue;
            }
        };

        let rel_path = match root.relativize(entry.path()) {
            Ok(rel_path) => rel_path,
            Err(err) => {
                tracing::warn!(path = %entry.path().display(), error = %err, "skipping file outside repository root");
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

    Ok(files)
}
