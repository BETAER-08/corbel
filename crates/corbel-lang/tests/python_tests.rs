use corbel_lang::langs::python::PythonSupport;
use corbel_lang::support::{ImportKind, LanguageSupport};
use std::fs;

fn fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/python_basic.py"
    ))
    .expect("fixture reads")
}

fn imports_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/python_imports.py"
    ))
    .expect("fixture reads")
}

#[test]
fn extract_symbols_finds_expected_count() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(symbols.len(), 8);
}

#[test]
fn visibility_follows_underscore_convention() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    assert!(add.is_public);

    let helper = symbols.iter().find(|s| s.name == "_helper").unwrap();
    assert!(!helper.is_public);

    let private = symbols.iter().find(|s| s.name == "_private").unwrap();
    assert!(!private.is_public);

    let init = symbols.iter().find(|s| s.name == "__init__").unwrap();
    assert!(init.is_public);

    let repr = symbols.iter().find(|s| s.name == "__repr__").unwrap();
    assert!(repr.is_public);

    let hidden = symbols.iter().find(|s| s.name == "__hidden").unwrap();
    assert!(!hidden.is_public);
}

#[test]
fn function_signature_excludes_body() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    let signature = add.signature.as_ref().unwrap();

    assert!(!signature.contains("return"));
    assert_eq!(signature, "def add(a, b):");
}

#[test]
fn class_signature_includes_bases_excludes_body() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let point = symbols.iter().find(|s| s.name == "Point").unwrap();
    let signature = point.signature.as_ref().unwrap();

    assert!(!signature.contains("__init__"));
    assert_eq!(signature, "class Point:");
}

#[test]
fn class_kind_is_class_not_struct() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let point = symbols.iter().find(|s| s.name == "Point").unwrap();
    assert_eq!(point.kind, "class");

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.kind, "function");
}

#[test]
fn line_numbers_match_fixture() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let expected: &[(&str, u32)] = &[
        ("add", 1),
        ("_helper", 5),
        ("Point", 9),
        ("__init__", 10),
        ("_private", 14),
        ("__repr__", 17),
        ("__hidden", 20),
        ("caller", 24),
    ];

    for (name, line) in expected {
        let symbol = symbols.iter().find(|s| &s.name == name).unwrap();
        assert_eq!(symbol.line, *line, "line mismatch for {name}");
    }
}

#[test]
fn reference_query_captures_calls() {
    let support = PythonSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    assert!(references.iter().any(|r| r.callee_name == "add"));
    assert!(references.iter().any(|r| r.callee_name == "Point"));
    assert!(references.iter().any(|r| r.callee_name == "_private"));
}

#[test]
fn calls_are_attributed_to_enclosing_function() {
    let support = PythonSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    for name in ["add", "Point", "_private"] {
        let reference = references.iter().find(|r| r.callee_name == name).unwrap();
        assert_eq!(reference.caller_name.as_deref(), Some("caller"));
    }
}

#[test]
fn simple_import_maps_module_to_itself() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(
        entries
            .iter()
            .any(|e| e.local_name.as_deref() == Some("module")
                && e.source_path == "module"
                && e.kind == ImportKind::Direct { aliased: false })
    );
}

#[test]
fn dotted_import_binds_leftmost_segment() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("a")
        && e.source_path == "a.b.c"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn aliased_dotted_import_binds_alias() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("d")
        && e.source_path == "a.b.c"
        && e.kind == ImportKind::Direct { aliased: true }));
}

#[test]
fn from_import_source_path_joins_module_and_name() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(entries.iter().any(|e| {
        e.local_name.as_deref() == Some("name")
            && e.source_path == "module.name"
            && e.kind == ImportKind::Direct { aliased: false }
    }));
}

#[test]
fn from_import_alias_binds_alias_to_module_and_name() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(entries.iter().any(|e| {
        e.local_name.as_deref() == Some("alias")
            && e.source_path == "module.name"
            && e.kind == ImportKind::Direct { aliased: true }
    }));
}

#[test]
fn parenthesized_from_import_expands_to_individual_entries() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    for (local, source) in [("a", "module.a"), ("b", "module.b"), ("c", "module.c")] {
        assert!(
            entries
                .iter()
                .any(|e| e.local_name.as_deref() == Some(local)
                    && e.source_path == source
                    && e.kind == ImportKind::Direct { aliased: false }),
            "missing entry for {local}"
        );
    }
}

#[test]
fn glob_import_has_no_specific_local_name() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(entries.iter().any(|e| e.local_name.is_none()
        && e.source_path == "module"
        && e.kind == ImportKind::Wildcard));
}

#[test]
fn relative_imports_preserve_dot_prefix_verbatim() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("x")
        && e.source_path == ".x"
        && e.kind == ImportKind::Direct { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("y")
        && e.source_path == ".module.y"
        && e.kind == ImportKind::Direct { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("z")
        && e.source_path == "..pkg.z"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn import_entry_count_matches_fixture() {
    let support = PythonSupport;
    let src = imports_fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();
    let entries = support.build_scope(&tree, &src).entries;

    assert_eq!(entries.len(), 12);
}

fn nested_class_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/python_nested_class.py"
    ))
    .expect("fixture reads")
}

#[test]
fn free_functions_have_no_owner() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.owner, None);

    let caller = symbols.iter().find(|s| s.name == "caller").unwrap();
    assert_eq!(caller.owner, None);
}

#[test]
fn methods_inside_class_are_owner_qualified_by_the_class_name() {
    let support = PythonSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let init = symbols.iter().find(|s| s.name == "__init__").unwrap();
    assert_eq!(init.owner.as_deref(), Some("Point"));

    let private = symbols.iter().find(|s| s.name == "_private").unwrap();
    assert_eq!(private.owner.as_deref(), Some("Point"));
}

#[test]
fn nested_class_method_owner_is_the_full_dot_joined_qualname_chain() {
    let support = PythonSupport;
    let src = nested_class_fixture_src();
    let symbols = support.extract_symbols(&src);

    let method = symbols.iter().find(|s| s.name == "method").unwrap();
    assert_eq!(method.owner.as_deref(), Some("Outer.Inner"));
}
