use std::collections::HashMap;
use std::path::Path;

use crate::support::LanguageSupport;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("extension \"{extension}\" is already registered")]
    ExtensionAlreadyRegistered { extension: String },
}

#[derive(Default)]
pub struct LanguageRegistry {
    languages: Vec<Box<dyn LanguageSupport>>,
    by_extension: HashMap<String, usize>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, support: Box<dyn LanguageSupport>) -> Result<(), RegistryError> {
        for extension in support.extensions() {
            let normalized = extension.to_ascii_lowercase();
            if self.by_extension.contains_key(&normalized) {
                return Err(RegistryError::ExtensionAlreadyRegistered {
                    extension: normalized,
                });
            }
        }

        let index = self.languages.len();
        for extension in support.extensions() {
            self.by_extension
                .insert(extension.to_ascii_lowercase(), index);
        }
        self.languages.push(support);
        Ok(())
    }

    pub fn for_extension(&self, ext: &str) -> Option<&dyn LanguageSupport> {
        let normalized = ext.to_ascii_lowercase();
        self.by_extension
            .get(&normalized)
            .map(|&index| self.languages[index].as_ref())
    }

    pub fn for_path(&self, path: &Path) -> Option<&dyn LanguageSupport> {
        let ext = path.extension()?.to_str()?;
        self.for_extension(ext)
    }

    pub fn supported_extensions(&self) -> Vec<&str> {
        self.by_extension.keys().map(String::as_str).collect()
    }
}
