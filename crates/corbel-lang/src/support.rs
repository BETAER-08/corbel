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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeEntry {
    pub local_name: String,
    pub source_path: String,
    pub kind: String,
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

    fn extract_signature(&self, node: tree_sitter::Node, src: &str) -> Option<String>;

    fn is_public(&self, node: tree_sitter::Node, src: &str) -> bool;

    fn build_scope(&self, tree: &tree_sitter::Tree, src: &str) -> ScopeTable;
}
