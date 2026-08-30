use corbel_lang::registry::{LanguageRegistry, RegistryError};
use corbel_lang::support::{LanguageSupport, ScopeTable};
use std::path::Path;

struct StubLanguage {
    extensions: &'static [&'static str],
}

impl LanguageSupport for StubLanguage {
    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn grammar(&self) -> tree_sitter::Language {
        unimplemented!("grammar() is not exercised by registry tests")
    }

    fn symbol_query(&self) -> &'static str {
        ""
    }

    fn reference_query(&self) -> &'static str {
        ""
    }

    fn symbol_kind(&self, node_kind: &str) -> String {
        node_kind.to_string()
    }

    fn extract_signature(&self, _node: tree_sitter::Node, _src: &str) -> Option<String> {
        None
    }

    fn is_public(&self, _node: tree_sitter::Node, _src: &str) -> bool {
        false
    }

    fn build_scope(&self, _tree: &tree_sitter::Tree, _src: &str) -> ScopeTable {
        ScopeTable::default()
    }

    fn enclosing_definition_name(&self, _node: tree_sitter::Node, _src: &str) -> Option<String> {
        None
    }
}

#[test]
fn for_extension_finds_registered_language() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["rs"],
        }))
        .unwrap();

    assert!(registry.for_extension("rs").is_some());
}

#[test]
fn for_extension_returns_none_for_unregistered() {
    let registry = LanguageRegistry::new();
    assert!(registry.for_extension("rs").is_none());
}

#[test]
fn for_extension_is_case_insensitive() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["rs"],
        }))
        .unwrap();

    assert!(registry.for_extension("RS").is_some());
    assert!(registry.for_extension("Rs").is_some());
}

#[test]
fn multi_extension_language_resolves_all_extensions() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["ts", "tsx"],
        }))
        .unwrap();

    assert!(registry.for_extension("ts").is_some());
    assert!(registry.for_extension("tsx").is_some());
}

#[test]
fn registering_duplicate_extension_returns_error() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["rs"],
        }))
        .unwrap();

    let result = registry.register(Box::new(StubLanguage {
        extensions: &["rs"],
    }));

    assert!(matches!(
        result,
        Err(RegistryError::ExtensionAlreadyRegistered { extension }) if extension == "rs"
    ));
}

#[test]
fn for_path_returns_none_without_extension() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["rs"],
        }))
        .unwrap();

    assert!(registry.for_path(Path::new("Makefile")).is_none());
}

#[test]
fn for_path_resolves_extension() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["rs"],
        }))
        .unwrap();

    assert!(registry.for_path(Path::new("src/main.rs")).is_some());
}

#[test]
fn supported_extensions_lists_all_registered() {
    let mut registry = LanguageRegistry::new();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["rs"],
        }))
        .unwrap();
    registry
        .register(Box::new(StubLanguage {
            extensions: &["ts", "tsx"],
        }))
        .unwrap();

    let mut extensions = registry.supported_extensions();
    extensions.sort_unstable();

    assert_eq!(extensions, vec!["rs", "ts", "tsx"]);
}
