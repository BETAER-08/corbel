#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSymbol {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub signature: Option<String>,
    pub is_public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawReference {
    pub callee_name: String,
    pub line: u32,
    pub caller_name: Option<String>,
}

pub use corbel_core::symbol::ImportKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeEntry {
    pub local_name: Option<String>,
    pub source_path: String,
    pub kind: ImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeTable {
    pub entries: Vec<ScopeEntry>,
}

pub trait LanguageSupport {
    fn extensions(&self) -> &'static [&'static str];

    fn grammar(&self) -> tree_sitter::Language;

    fn symbol_query(&self) -> &'static str;

    fn reference_query(&self) -> &'static str;

    fn symbol_kind(&self, node_kind: &str) -> String;

    fn extract_signature(&self, node: tree_sitter::Node, src: &str) -> Option<String>;

    fn is_public(&self, node: tree_sitter::Node, src: &str) -> bool;

    fn build_scope(&self, tree: &tree_sitter::Tree, src: &str) -> ScopeTable;

    fn enclosing_definition_name(&self, node: tree_sitter::Node, src: &str) -> Option<String>;

    fn extract_symbols(&self, src: &str) -> Vec<RawSymbol> {
        use tree_sitter::StreamingIterator;

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.grammar()).expect("grammar loads");
        let tree = parser.parse(src, None).expect("source parses");

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
                kind: self.symbol_kind(definition.kind()),
                line: definition.start_position().row as u32 + 1,
                signature: self.extract_signature(definition, src),
                is_public: self.is_public(definition, src),
            });
        }

        symbols
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

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

        let mut references = Vec::new();
        while let Some(m) = matches.next() {
            let callee = m
                .captures
                .iter()
                .find(|c| c.index == callee_index)
                .expect("match has a callee capture")
                .node;

            let callee_name = callee
                .utf8_text(src.as_bytes())
                .expect("callee node is valid utf8")
                .to_string();

            references.push(RawReference {
                callee_name,
                line: callee.start_position().row as u32 + 1,
                caller_name: self.enclosing_definition_name(callee, src),
            });
        }

        references
    }
}
