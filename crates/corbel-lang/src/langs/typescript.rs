use tree_sitter::Node;

use crate::support::{ImportKind, LanguageSupport, ScopeEntry, ScopeTable};

pub(crate) const SYMBOL_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @definition

(class_declaration
  name: (type_identifier) @name) @definition

(method_definition
  name: (property_identifier) @name) @definition

(interface_declaration
  name: (type_identifier) @name) @definition

(type_alias_declaration
  name: (type_identifier) @name) @definition

(enum_declaration
  name: (identifier) @name) @definition

(variable_declarator
  name: (identifier) @name
  value: (arrow_function)) @definition
"#;

const REFERENCE_QUERY: &str = r#"
(call_expression
  function: (identifier) @callee)

(call_expression
  function: (member_expression
    property: (property_identifier) @callee))

(new_expression
  constructor: (identifier) @callee)
"#;

fn module_text(source_node: Node, src: &str) -> Option<String> {
    source_node
        .named_child(0)
        .and_then(|fragment| fragment.utf8_text(src.as_bytes()).ok())
        .map(str::to_string)
}

fn alias_or_name(specifier: Node, src: &str) -> Option<(String, String, bool)> {
    let name_node = specifier.child_by_field_name("name")?;
    let name_text = name_node.utf8_text(src.as_bytes()).ok()?.to_string();
    let (local_name, aliased) = match specifier.child_by_field_name("alias") {
        Some(alias_node) => (alias_node.utf8_text(src.as_bytes()).ok()?.to_string(), true),
        None => (name_text.clone(), false),
    };
    Some((local_name, name_text, aliased))
}

fn collect_import_statement(node: Node, src: &str, entries: &mut Vec<ScopeEntry>) {
    let Some(source_node) = node.child_by_field_name("source") else {
        return;
    };
    let Some(module) = module_text(source_node, src) else {
        return;
    };

    let clause = node
        .named_child(0)
        .filter(|child| child.kind() == "import_clause");

    let Some(clause) = clause else {
        entries.push(ScopeEntry {
            local_name: None,
            source_path: module,
            kind: ImportKind::SideEffect,
        });
        return;
    };

    let Some(binding) = clause.named_child(0) else {
        return;
    };

    match binding.kind() {
        "identifier" => {
            if let Ok(local_name) = binding.utf8_text(src.as_bytes()) {
                entries.push(ScopeEntry {
                    local_name: Some(local_name.to_string()),
                    source_path: module,
                    kind: ImportKind::Direct { aliased: false },
                });
            }
        }
        "namespace_import" => {
            if let Some(identifier) = binding.named_child(0)
                && let Ok(local_name) = identifier.utf8_text(src.as_bytes())
            {
                entries.push(ScopeEntry {
                    local_name: Some(local_name.to_string()),
                    source_path: module,
                    kind: ImportKind::Namespace,
                });
            }
        }
        "named_imports" => {
            let mut cursor = binding.walk();
            for specifier in binding.named_children(&mut cursor) {
                if specifier.kind() != "import_specifier" {
                    continue;
                }
                if let Some((local_name, name_text, aliased)) = alias_or_name(specifier, src) {
                    entries.push(ScopeEntry {
                        local_name: Some(local_name),
                        source_path: format!("{module}.{name_text}"),
                        kind: ImportKind::Direct { aliased },
                    });
                }
            }
        }
        _ => {}
    }
}

fn collect_export_statement(node: Node, src: &str, entries: &mut Vec<ScopeEntry>) {
    let Some(source_node) = node.child_by_field_name("source") else {
        return;
    };
    let Some(module) = module_text(source_node, src) else {
        return;
    };

    let clause = node
        .named_child(0)
        .filter(|child| child.kind() == "export_clause");

    let Some(clause) = clause else {
        entries.push(ScopeEntry {
            local_name: None,
            source_path: module,
            kind: ImportKind::Wildcard,
        });
        return;
    };

    let mut cursor = clause.walk();
    for specifier in clause.named_children(&mut cursor) {
        if specifier.kind() != "export_specifier" {
            continue;
        }
        if let Some((local_name, name_text, aliased)) = alias_or_name(specifier, src) {
            entries.push(ScopeEntry {
                local_name: Some(local_name),
                source_path: format!("{module}.{name_text}"),
                kind: ImportKind::Reexport { aliased },
            });
        }
    }
}

fn walk_imports(node: Node, src: &str, entries: &mut Vec<ScopeEntry>) {
    match node.kind() {
        "import_statement" => collect_import_statement(node, src, entries),
        "export_statement" => collect_export_statement(node, src, entries),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_imports(child, src, entries);
    }
}

pub(crate) fn symbol_kind(node_kind: &str) -> String {
    match node_kind {
        "function_declaration" => "function",
        "variable_declarator" => "function",
        "method_definition" => "method",
        "class_declaration" => "class",
        "interface_declaration" => "interface",
        "type_alias_declaration" => "type",
        "enum_declaration" => "enum",
        other => unreachable!("symbol_query captured unexpected node kind: {other}"),
    }
    .to_string()
}

pub(crate) fn extract_signature(node: tree_sitter::Node, src: &str) -> Option<String> {
    let (start_byte, end_byte) = match node.kind() {
        "interface_declaration" | "type_alias_declaration" => (node.start_byte(), node.end_byte()),
        "variable_declarator" => {
            let start_byte = node
                .parent()
                .map(|parent| parent.start_byte())
                .unwrap_or_else(|| node.start_byte());
            let end_byte = node
                .child_by_field_name("value")
                .and_then(|value| value.child_by_field_name("body"))
                .map(|body| body.start_byte())
                .unwrap_or_else(|| node.end_byte());
            (start_byte, end_byte)
        }
        _ => {
            let end_byte = node
                .child_by_field_name("body")
                .map(|body| body.start_byte())
                .unwrap_or_else(|| node.end_byte());
            (node.start_byte(), end_byte)
        }
    };

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

pub(crate) fn is_public(node: tree_sitter::Node, src: &str) -> bool {
    match node.kind() {
        "method_definition" => {
            let mut cursor = node.walk();
            let modifier = node
                .children(&mut cursor)
                .find(|child| child.kind() == "accessibility_modifier");
            match modifier {
                Some(modifier) => modifier
                    .utf8_text(src.as_bytes())
                    .map(|text| text == "public")
                    .unwrap_or(true),
                None => true,
            }
        }
        "variable_declarator" => node
            .parent()
            .and_then(|parent| parent.parent())
            .map(|grandparent| grandparent.kind() == "export_statement")
            .unwrap_or(false),
        _ => node
            .parent()
            .map(|parent| parent.kind() == "export_statement")
            .unwrap_or(false),
    }
}

pub(crate) fn enclosing_definition_name(node: tree_sitter::Node, src: &str) -> Option<String> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "function_declaration" | "method_definition" => {
                let name_node = ancestor.child_by_field_name("name")?;
                return name_node.utf8_text(src.as_bytes()).ok().map(str::to_string);
            }
            "variable_declarator"
                if ancestor
                    .child_by_field_name("value")
                    .is_some_and(|value| value.kind() == "arrow_function") =>
            {
                let name_node = ancestor.child_by_field_name("name")?;
                return name_node.utf8_text(src.as_bytes()).ok().map(str::to_string);
            }
            _ => {}
        }
        current = ancestor.parent();
    }
    None
}

fn class_chain_name(mut class_node: tree_sitter::Node, src: &str) -> Option<String> {
    let mut names = Vec::new();
    loop {
        if let Some(name_node) = class_node.child_by_field_name("name") {
            if let Ok(name) = name_node.utf8_text(src.as_bytes()) {
                names.push(name.to_string());
            }
        }
        let mut current = class_node.parent();
        let mut found_outer = None;
        while let Some(ancestor) = current {
            if matches!(ancestor.kind(), "class_declaration" | "class") {
                found_outer = Some(ancestor);
                break;
            }
            current = ancestor.parent();
        }
        match found_outer {
            Some(outer) => class_node = outer,
            None => break,
        }
    }
    if names.is_empty() {
        None
    } else {
        names.reverse();
        Some(names.join("."))
    }
}

fn object_owner_name(object_node: tree_sitter::Node, src: &str) -> Option<String> {
    let parent = object_node.parent()?;
    match parent.kind() {
        "variable_declarator" => {
            let value = parent.child_by_field_name("value")?;
            if value.id() != object_node.id() {
                return None;
            }
            let name_node = parent.child_by_field_name("name")?;
            name_node.utf8_text(src.as_bytes()).ok().map(str::to_string)
        }
        "pair" => {
            let value = parent.child_by_field_name("value")?;
            if value.id() != object_node.id() {
                return None;
            }
            let key_node = parent.child_by_field_name("key")?;
            let key_text = key_node.utf8_text(src.as_bytes()).ok()?;
            Some(key_text.trim_matches(|c| c == '"' || c == '\'').to_string())
        }
        _ => None,
    }
}

pub(crate) fn owner_of_definition(node: tree_sitter::Node, src: &str) -> Option<String> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "class_declaration" | "class" => return class_chain_name(ancestor, src),
            "object" => return object_owner_name(ancestor, src),
            _ => {}
        }
        current = ancestor.parent();
    }
    None
}

pub(crate) fn build_scope(tree: &tree_sitter::Tree, src: &str) -> ScopeTable {
    let mut entries = Vec::new();
    walk_imports(tree.root_node(), src, &mut entries);
    ScopeTable { entries }
}

pub struct TypeScriptSupport;

impl LanguageSupport for TypeScriptSupport {
    fn extensions(&self) -> &'static [&'static str] {
        &["ts"]
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn symbol_query(&self) -> &'static str {
        SYMBOL_QUERY
    }

    fn reference_query(&self) -> &'static str {
        REFERENCE_QUERY
    }

    fn symbol_kind(&self, node_kind: &str) -> String {
        symbol_kind(node_kind)
    }

    fn extract_signature(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        extract_signature(node, src)
    }

    fn is_public(&self, node: tree_sitter::Node, src: &str) -> bool {
        is_public(node, src)
    }

    fn enclosing_definition_name(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        enclosing_definition_name(node, src)
    }

    fn owner_of_definition(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        owner_of_definition(node, src)
    }

    fn build_scope(&self, tree: &tree_sitter::Tree, src: &str) -> ScopeTable {
        build_scope(tree, src)
    }
}
