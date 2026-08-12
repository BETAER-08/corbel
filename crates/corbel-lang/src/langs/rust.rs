use tree_sitter::StreamingIterator;

use crate::support::{LanguageSupport, RawSymbol, ScopeTable};

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

pub struct RustSupport;

impl RustSupport {
    fn symbol_kind(node_kind: &str) -> &'static str {
        match node_kind {
            "function_item" => "function",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
            other => unreachable!("symbol_query captured unexpected node kind: {other}"),
        }
    }

    pub fn extract_symbols(&self, src: &str) -> Vec<RawSymbol> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.grammar())
            .expect("rust grammar loads");
        let tree = parser.parse(src, None).expect("rust source parses");

        let query =
            tree_sitter::Query::new(&self.grammar(), self.symbol_query()).expect("valid query");
        let definition_index = query
            .capture_index_for_name("definition")
            .expect("query defines @definition capture");
        let name_index = query
            .capture_index_for_name("name")
            .expect("query defines @name capture");

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut symbols = Vec::new();
        while let Some(m) = matches.next() {
            let definition = m
                .captures
                .iter()
                .find(|c| c.index == definition_index)
                .expect("match has a definition capture")
                .node;
            let name_node = m
                .captures
                .iter()
                .find(|c| c.index == name_index)
                .expect("match has a name capture")
                .node;

            let name = name_node
                .utf8_text(src.as_bytes())
                .expect("name node is valid utf8")
                .to_string();

            symbols.push(RawSymbol {
                name,
                kind: Self::symbol_kind(definition.kind()).to_string(),
                line: definition.start_position().row as u32 + 1,
                signature: self.extract_signature(definition, src),
                is_public: self.is_public(definition, src),
            });
        }

        symbols
    }
}

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

    fn build_scope(&self, _tree: &tree_sitter::Tree, _src: &str) -> ScopeTable {
        ScopeTable { entries: vec![] }
    }
}
