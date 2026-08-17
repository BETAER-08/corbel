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
    pub caller_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    Direct { aliased: bool },
    Reexport { aliased: bool },
    Wildcard,
    Namespace,
    SideEffect,
}

impl ImportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportKind::Direct { aliased: false } => "direct",
            ImportKind::Direct { aliased: true } => "direct-aliased",
            ImportKind::Reexport { aliased: false } => "reexport",
            ImportKind::Reexport { aliased: true } => "reexport-aliased",
            ImportKind::Wildcard => "wildcard",
            ImportKind::Namespace => "namespace",
            ImportKind::SideEffect => "sideeffect",
        }
    }
}

impl std::fmt::Display for ImportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown import kind: {0}")]
pub struct UnknownImportKind(pub String);

impl std::str::FromStr for ImportKind {
    type Err = UnknownImportKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "direct" => Ok(ImportKind::Direct { aliased: false }),
            "direct-aliased" => Ok(ImportKind::Direct { aliased: true }),
            "reexport" => Ok(ImportKind::Reexport { aliased: false }),
            "reexport-aliased" => Ok(ImportKind::Reexport { aliased: true }),
            "wildcard" => Ok(ImportKind::Wildcard),
            "namespace" => Ok(ImportKind::Namespace),
            "sideeffect" => Ok(ImportKind::SideEffect),
            other => Err(UnknownImportKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub local_name: Option<String>,
    pub source_path: String,
    pub kind: ImportKind,
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
