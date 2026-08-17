use crate::langs::typescript;
use crate::support::{LanguageSupport, RawReference, ScopeTable};

const REFERENCE_QUERY: &str = r#"
(call_expression
  function: (identifier) @callee)

(call_expression
  function: (member_expression
    property: (property_identifier) @callee))

(new_expression
  constructor: (identifier) @callee)

(jsx_opening_element
  name: (identifier) @jsx_callee)

(jsx_self_closing_element
  name: (identifier) @jsx_callee)

(jsx_opening_element
  name: (member_expression
    property: (property_identifier) @jsx_callee))

(jsx_self_closing_element
  name: (member_expression
    property: (property_identifier) @jsx_callee))
"#;

fn is_component_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

pub struct TsxSupport;

impl LanguageSupport for TsxSupport {
    fn extensions(&self) -> &'static [&'static str] {
        &["tsx"]
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    }

    fn symbol_query(&self) -> &'static str {
        typescript::SYMBOL_QUERY
    }

    fn reference_query(&self) -> &'static str {
        REFERENCE_QUERY
    }

    fn symbol_kind(&self, node_kind: &str) -> String {
        typescript::symbol_kind(node_kind)
    }

    fn extract_signature(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        typescript::extract_signature(node, src)
    }

    fn is_public(&self, node: tree_sitter::Node, src: &str) -> bool {
        typescript::is_public(node, src)
    }

    fn enclosing_definition_name(&self, node: tree_sitter::Node, src: &str) -> Option<String> {
        typescript::enclosing_definition_name(node, src)
    }

    fn build_scope(&self, tree: &tree_sitter::Tree, src: &str) -> ScopeTable {
        typescript::build_scope(tree, src)
    }

    fn extract_references(&self, src: &str) -> Vec<RawReference> {
        use tree_sitter::StreamingIterator;

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.grammar()).expect("grammar loads");
        let tree = parser.parse(src, None).expect("source parses");

        let query =
            tree_sitter::Query::new(&self.grammar(), self.reference_query()).expect("valid query");
        let callee_index = query
            .capture_index_for_name("callee")
            .expect("query defines @callee capture");
        let jsx_callee_index = query
            .capture_index_for_name("jsx_callee")
            .expect("query defines @jsx_callee capture");

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut references = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let callee = capture.node;
                let callee_name = callee
                    .utf8_text(src.as_bytes())
                    .expect("callee node is valid utf8")
                    .to_string();

                if capture.index == jsx_callee_index && !is_component_name(&callee_name) {
                    continue;
                }

                if capture.index != callee_index && capture.index != jsx_callee_index {
                    continue;
                }

                references.push(RawReference {
                    callee_name,
                    line: callee.start_position().row as u32 + 1,
                    caller_name: self.enclosing_definition_name(callee, src),
                });
            }
        }

        references
    }
}
