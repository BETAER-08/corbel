use crate::path::RelPath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub signature: Option<String>,
    pub is_public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub callee_name: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub local_name: String,
    pub source_path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedFile {
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub imports: Vec<Import>,
}

pub trait FileParser {
    fn parse(&self, path: &RelPath, source: &str) -> ParsedFile;

    fn extensions(&self) -> Vec<String>;
}
