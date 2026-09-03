use tree_sitter::{Node, StreamingIterator};

use crate::support::{ImportKind, LanguageSupport, ScopeEntry, ScopeTable};

const SYMBOL_QUERY: &str = r#"
(function_item
  name: (identifier) @name) @definition

(struct_item
  name: (type_identifier) @name) @definition

(enum_item
  name: (type_identifier) @name) @definition

(trait_item
  name: (type_identifier) @name) @definition
"#;

const REFERENCE_QUERY: &str = r#"
(call_expression
  function: (identifier) @callee)

(call_expression
  function: (field_expression
    field: (field_identifier) @callee))
"#;

const USE_DECLARATION_QUERY: &str = "(use_declaration) @use";

fn path_text(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "crate" | "self" | "super" => {
            node.utf8_text(src.as_bytes()).ok().map(str::to_string)
        }
        "scoped_identifier" => {
            let path = node.child_by_field_name("path")?;
            let name = node.child_by_field_name("name")?;
            let path_str = path_text(path, src)?;
            let name_str = path_text(name, src)?;
            Some(format!("{path_str}::{name_str}"))
        }
        _ => None,
    }
}

fn last_segment(path: &str) -> String {
    path.rsplit("::").next().unwrap_or(path).to_string()
}

fn rust_type_name(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "type_identifier" => node.utf8_text(src.as_bytes()).ok().map(str::to_string),
        "generic_type" => rust_type_name(node.child_by_field_name("type")?, src),
        "reference_type" => rust_type_name(node.child_by_field_name("type")?, src),
        "scoped_type_identifier" => rust_type_name(node.child_by_field_name("name")?, src),
        _ => None,
    }
}

fn join_path(prefix: Option<&str>, rel: &str) -> String {
    match prefix {
        Some(p) if !rel.is_empty() => format!("{p}::{rel}"),
        Some(p) => p.to_string(),
        None => rel.to_string(),
    }
}

fn direct_kind(is_reexport: bool, aliased: bool) -> ImportKind {
    if is_reexport {
        ImportKind::Reexport { aliased }
    } else {
        ImportKind::Direct { aliased }
    }
}

fn collect_use_tree(
    node: Node,
    prefix: Option<&str>,
    is_reexport: bool,
    src: &str,
    entries: &mut Vec<ScopeEntry>,
) {
    match node.kind() {
        "identifier" => {
            if let Ok(name) = node.utf8_text(src.as_bytes()) {
                let source_path = join_path(prefix, name);
                entries.push(ScopeEntry {
                    local_name: Some(name.to_string()),
                    source_path,
                    kind: direct_kind(is_reexport, false),
                });
            }
        }
        "self" => {
            if let Some(prefix) = prefix {
                entries.push(ScopeEntry {
                    local_name: Some(last_segment(prefix)),
                    source_path: prefix.to_string(),
                    kind: direct_kind(is_reexport, false),
                });
            }
        }
        "scoped_identifier" => {
            if let Some(rel) = path_text(node, src) {
                let source_path = join_path(prefix, &rel);
                entries.push(ScopeEntry {
                    local_name: Some(last_segment(&rel)),
                    source_path,
                    kind: direct_kind(is_reexport, false),
                });
            }
        }
        "use_as_clause" => {
            let path_node = node.child_by_field_name("path");
            let alias_node = node.child_by_field_name("alias");
            if let (Some(path_node), Some(alias_node)) = (path_node, alias_node) {
                if let (Some(rel), Ok(alias)) = (
                    path_text(path_node, src),
                    alias_node.utf8_text(src.as_bytes()),
                ) {
                    let source_path = join_path(prefix, &rel);
                    entries.push(ScopeEntry {
                        local_name: Some(alias.to_string()),
                        source_path,
                        kind: direct_kind(is_reexport, true),
                    });
                }
            }
        }
        "scoped_use_list" => {
            let combined_prefix = match node
                .child_by_field_name("path")
                .and_then(|path_node| path_text(path_node, src))
            {
                Some(rel) => join_path(prefix, &rel),
                None => prefix.unwrap_or("").to_string(),
            };
            if let Some(list) = node.child_by_field_name("list") {
                let mut cursor = list.walk();
                for item in list.named_children(&mut cursor) {
                    collect_use_tree(
                        item,
                        Some(combined_prefix.as_str()),
                        is_reexport,
                        src,
                        entries,
                    );
                }
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                collect_use_tree(item, prefix, is_reexport, src, entries);
            }
        }
        "use_wildcard" => {
            let mut cursor = node.walk();
            let rel = node
                .named_children(&mut cursor)
                .next()
                .and_then(|prefix_node| path_text(prefix_node, src));
            let source_path = match rel {
                Some(rel) => join_path(prefix, &rel),
                None => prefix.unwrap_or("").to_string(),
            };
            entries.push(ScopeEntry {
                local_name: None,
                source_path,
                kind: ImportKind::Wildcard,
            });
        }
        _ => {}
    }
}

pub struct RustSupport;

impl LanguageSupport for RustSupport {
    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn symbol_query(&self) -> &'static str {
        SYMBOL_QUERY
    }

    fn reference_query(&self) -> &'static str {
        REFERENCE_QUERY
    }

    fn symbol_kind(&self, node_kind: &str) -> String {
        match node_kind {
            "function_item" => "function",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
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

    fn is_public(&self, node: tree_sitter::Node, _src: &str) -> bool {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .any(|child| child.kind() == "visibility_modifier")
    }

    fn enclosing_definition_name(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        let mut current = node.parent();
        while let Some(ancestor) = current {
            if ancestor.kind() == "function_item" {
                let name_node = ancestor.child_by_field_name("name")?;
                return name_node.utf8_text(src.as_bytes()).ok().map(str::to_string);
            }
            current = ancestor.parent();
        }
        None
    }

    fn owner_of_definition(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        let mut current = node.parent();
        while let Some(ancestor) = current {
            if ancestor.kind() == "impl_item" {
                let type_field = ancestor.child_by_field_name("type")?;
                return rust_type_name(type_field, src);
            }
            current = ancestor.parent();
        }
        None
    }

    fn build_scope(&self, tree: &tree_sitter::Tree, src: &str) -> ScopeTable {
        let query =
            tree_sitter::Query::new(&self.grammar(), USE_DECLARATION_QUERY).expect("valid query");
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut entries = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let declaration = capture.node;
                let is_reexport = self.is_public(declaration, src);
                if let Some(argument) = declaration.child_by_field_name("argument") {
                    collect_use_tree(argument, None, is_reexport, src, &mut entries);
                }
            }
        }

        ScopeTable { entries }
    }
}
