use crate::error::{Error, Result};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoRoot(PathBuf);

impl RepoRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let canonical = std::fs::canonicalize(path).map_err(|e| Error::io(path, e))?;
        if !canonical.is_dir() {
            return Err(Error::InvalidRepoRoot { path: canonical });
        }
        Ok(RepoRoot(canonical))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn relativize(&self, absolute: impl AsRef<Path>) -> Result<RelPath> {
        let absolute = absolute.as_ref();
        let canonical = std::fs::canonicalize(absolute).map_err(|e| Error::io(absolute, e))?;
        let stripped = canonical
            .strip_prefix(&self.0)
            .map_err(|_| Error::PathEscapesRoot {
                path: canonical.clone(),
            })?;
        if stripped.as_os_str().is_empty() {
            return Err(Error::PathEscapesRoot { path: canonical });
        }

        let mut normalized = String::new();
        for (i, component) in stripped.components().enumerate() {
            if i > 0 {
                normalized.push('/');
            }
            normalized.push_str(&component.as_os_str().to_string_lossy());
        }

        Ok(RelPath(normalized))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelPath(String);

impl RelPath {
    #[allow(dead_code)]
    pub(crate) fn from_stored(s: impl Into<String>) -> Self {
        RelPath(s.into())
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RelPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_stored_preserves_input_without_normalizing() {
        let rel = RelPath::from_stored("a/b/c.txt");
        assert_eq!(rel.as_ref(), "a/b/c.txt");
        assert_eq!(rel.to_string(), "a/b/c.txt");
    }
}
