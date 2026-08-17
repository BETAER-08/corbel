use crate::support::{ImportKind, LanguageSupport, ScopeEntry, ScopeTable};

const SYMBOL_QUERY: &str = r#"
(function_definition
  name: (identifier) @name) @definition

(class_definition
  name: (identifier) @name) @definition
"#;

const REFERENCE_QUERY: &str = r#"
(call
  function: (identifier) @callee)

(call
  function: (attribute
    attribute: (identifier) @callee))
"#;

fn join_from_source(module_text: &str, name_text: &str) -> String {
    if module_text.ends_with('.') {
        format!("{module_text}{name_text}")
    } else {
        format!("{module_text}.{name_text}")
    }
}

fn collect_import_statement(node: tree_sitter::Node, src: &str, entries: &mut Vec<ScopeEntry>) {
    let mut cursor = node.walk();
    for name_node in node.children_by_field_name("name", &mut cursor) {
        match name_node.kind() {
            "dotted_name" => {
                if let (Some(first), Ok(full)) = (
                    name_node.named_child(0),
                    name_node.utf8_text(src.as_bytes()),
                ) && let Ok(local) = first.utf8_text(src.as_bytes())
                {
                    entries.push(ScopeEntry {
                        local_name: Some(local.to_string()),
                        source_path: full.to_string(),
                        kind: ImportKind::Direct { aliased: false },
                    });
                }
            }
            "aliased_import" => {
                if let (Some(path_node), Some(alias_node)) = (
                    name_node.child_by_field_name("name"),
                    name_node.child_by_field_name("alias"),
                ) && let (Ok(source_path), Ok(local_name)) = (
                    path_node.utf8_text(src.as_bytes()),
                    alias_node.utf8_text(src.as_bytes()),
                ) {
                    entries.push(ScopeEntry {
                        local_name: Some(local_name.to_string()),
                        source_path: source_path.to_string(),
                        kind: ImportKind::Direct { aliased: true },
                    });
                }
            }
            _ => {}
        }
    }
}

fn collect_import_from_statement(
    node: tree_sitter::Node,
    src: &str,
    entries: &mut Vec<ScopeEntry>,
) {
    let Some(module_node) = node.child_by_field_name("module_name") else {
        return;
    };
    let Ok(module_text) = module_node.utf8_text(src.as_bytes()) else {
        return;
    };

    let mut wildcard_cursor = node.walk();
    for child in node.children(&mut wildcard_cursor) {
        if child.kind() == "wildcard_import" {
            entries.push(ScopeEntry {
                local_name: None,
                source_path: module_text.to_string(),
                kind: ImportKind::Wildcard,
            });
        }
    }

    let mut cursor = node.walk();
    for name_node in node.children_by_field_name("name", &mut cursor) {
        match name_node.kind() {
            "dotted_name" => {
                if let Ok(name_text) = name_node.utf8_text(src.as_bytes()) {
                    entries.push(ScopeEntry {
                        local_name: Some(name_text.to_string()),
                        source_path: join_from_source(module_text, name_text),
                        kind: ImportKind::Direct { aliased: false },
                    });
                }
            }
            "aliased_import" => {
                if let (Some(name_field), Some(alias_field)) = (
                    name_node.child_by_field_name("name"),
                    name_node.child_by_field_name("alias"),
                ) && let (Ok(name_text), Ok(alias_text)) = (
                    name_field.utf8_text(src.as_bytes()),
                    alias_field.utf8_text(src.as_bytes()),
                ) {
                    entries.push(ScopeEntry {
                        local_name: Some(alias_text.to_string()),
                        source_path: join_from_source(module_text, name_text),
                        kind: ImportKind::Direct { aliased: true },
                    });
                }
            }
            _ => {}
        }
    }
}

fn walk_imports(node: tree_sitter::Node, src: &str, entries: &mut Vec<ScopeEntry>) {
    match node.kind() {
        "import_statement" => collect_import_statement(node, src, entries),
        "import_from_statement" => collect_import_from_statement(node, src, entries),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_imports(child, src, entries);
    }
}

pub struct PythonSupport;

impl LanguageSupport for PythonSupport {
    fn extensions(&self) -> &'static [&'static str] {
        &["py"]
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn symbol_query(&self) -> &'static str {
        SYMBOL_QUERY
    }

    fn reference_query(&self) -> &'static str {
        REFERENCE_QUERY
    }

    fn symbol_kind(&self, node_kind: &str) -> String {
        match node_kind {
            "function_definition" => "function",
            "class_definition" => "class",
            other => unreachable!("symbol_query captured unexpected node kind: {other}"),
        }
        .to_string()
    }

    fn extract_signature(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        let end_byte = node
            .child_by_field_name("body")
            .map(|body| body.start_byte())
            .unwrap_or_else(|| node.end_byte());
        let start_byte = node.start_byte();
        if end_byte <= start_byte {
            return None;
        }

        let raw = &src[start_byte..end_byte];
        let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    fn is_public(&self, node: tree_sitter::Node, src: &str) -> bool {
        let Some(name_node) = node.child_by_field_name("name") else {
            return true;
        };
        let Ok(name) = name_node.utf8_text(src.as_bytes()) else {
            return true;
        };

        if name.len() >= 5 && name.starts_with("__") && name.ends_with("__") {
            true
        } else {
            !name.starts_with('_')
        }
    }

    fn enclosing_definition_name(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        let mut current = node.parent();
        while let Some(ancestor) = current {
            if ancestor.kind() == "function_definition" {
                let name_node = ancestor.child_by_field_name("name")?;
                return name_node.utf8_text(src.as_bytes()).ok().map(str::to_string);
            }
            current = ancestor.parent();
        }
        None
    }

    fn build_scope(&self, tree: &tree_sitter::Tree, src: &str) -> ScopeTable {
        let mut entries = Vec::new();
        walk_imports(tree.root_node(), src, &mut entries);
        ScopeTable { entries }
    }
}
