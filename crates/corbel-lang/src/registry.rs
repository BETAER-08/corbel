use std::collections::HashMap;
use std::path::Path;

use corbel_core::path::RelPath;
use corbel_core::symbol::{FileParser, Import, ParsedFile, Reference, Symbol};

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

    pub fn with_all_languages() -> Self {
        let mut registry = Self::new();
        registry
            .register(Box::new(crate::langs::rust::RustSupport))
            .expect("built-in languages register without extension conflicts");
        registry
            .register(Box::new(crate::langs::python::PythonSupport))
            .expect("built-in languages register without extension conflicts");
        registry
            .register(Box::new(crate::langs::typescript::TypeScriptSupport))
            .expect("built-in languages register without extension conflicts");
        registry
            .register(Box::new(crate::langs::tsx::TsxSupport))
            .expect("built-in languages register without extension conflicts");
        registry
            .register(Box::new(crate::langs::javascript::JavaScriptSupport))
            .expect("built-in languages register without extension conflicts");
        registry
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

impl FileParser for LanguageRegistry {
    fn parse(&self, path: &RelPath, source: &str) -> ParsedFile {
        let extension = match Path::new(path.as_ref())
            .extension()
            .and_then(|e| e.to_str())
        {
            Some(extension) => extension,
            None => return ParsedFile::default(),
        };

        let support = match self.for_extension(extension) {
            Some(support) => support,
            None => return ParsedFile::default(),
        };

        let symbols = support
            .extract_symbols(source)
            .into_iter()
            .map(|raw| Symbol {
                name: raw.name,
                kind: raw.kind,
                line: raw.line,
                signature: raw.signature,
                is_public: raw.is_public,
                owner: raw.owner,
            })
            .collect();

        let references = support
            .extract_references(source)
            .into_iter()
            .map(|raw| Reference {
                callee_name: raw.callee_name,
                line: raw.line,
                caller_name: raw.caller_name,
            })
            .collect();

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&support.grammar())
            .expect("grammar loads");
        let tree = parser.parse(source, None).expect("source parses");
        let imports = support
            .build_scope(&tree, source)
            .entries
            .into_iter()
            .map(|entry| Import {
                local_name: entry.local_name,
                source_path: entry.source_path,
                kind: entry.kind,
            })
            .collect();

        ParsedFile {
            symbols,
            references,
            imports,
        }
    }

    fn extensions(&self) -> Vec<String> {
        self.supported_extensions()
            .into_iter()
            .map(str::to_string)
            .collect()
    }
}
