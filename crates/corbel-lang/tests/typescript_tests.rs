use corbel_lang::langs::typescript::TypeScriptSupport;
use corbel_lang::support::{ImportKind, LanguageSupport};
use std::fs;

fn fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/typescript_basic.ts"
    ))
    .expect("fixture reads")
}

fn imports_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/typescript_imports.ts"
    ))
    .expect("fixture reads")
}

fn scope_entries(support: &TypeScriptSupport, src: &str) -> Vec<corbel_lang::support::ScopeEntry> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(src, None).unwrap();
    support.build_scope(&tree, src).entries
}

#[test]
fn extract_symbols_finds_expected_count() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(symbols.len(), 12);
}

#[test]
fn export_based_visibility_is_detected() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let pub_fn = symbols.iter().find(|s| s.name == "pub").unwrap();
    assert!(pub_fn.is_public);

    let priv_fn = symbols.iter().find(|s| s.name == "priv").unwrap();
    assert!(!priv_fn.is_public);

    let arrow_fn = symbols.iter().find(|s| s.name == "arrowFn").unwrap();
    assert!(arrow_fn.is_public);

    let not_exported = symbols.iter().find(|s| s.name == "notExported").unwrap();
    assert!(!not_exported.is_public);

    let foo = symbols.iter().find(|s| s.name == "Foo").unwrap();
    assert!(foo.is_public);
}

#[test]
fn class_member_accessibility_modifier_is_detected() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let constructor = symbols.iter().find(|s| s.name == "constructor").unwrap();
    assert!(constructor.is_public);

    let pub_method = symbols.iter().find(|s| s.name == "pubMethod").unwrap();
    assert!(pub_method.is_public);

    let priv_method = symbols.iter().find(|s| s.name == "privMethod").unwrap();
    assert!(!priv_method.is_public);
}

#[test]
fn function_signature_excludes_body() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let pub_fn = symbols.iter().find(|s| s.name == "pub").unwrap();
    let signature = pub_fn.signature.as_ref().unwrap();

    assert!(!signature.contains("return"));
    assert_eq!(signature, "function pub(a: number, b: string): number");
}

#[test]
fn arrow_function_const_signature_excludes_body() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let arrow_fn = symbols.iter().find(|s| s.name == "arrowFn").unwrap();
    let signature = arrow_fn.signature.as_ref().unwrap();

    assert!(!signature.contains("return"));
    assert_eq!(signature, "const arrowFn = (a: number): number =>");
}

#[test]
fn interface_signature_includes_full_body() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let shape = symbols.iter().find(|s| s.name == "Shape").unwrap();
    let signature = shape.signature.as_ref().unwrap();

    assert!(signature.contains("area(): number"));
    assert!(signature.contains("name: string"));
}

#[test]
fn type_alias_signature_includes_full_value() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let point = symbols.iter().find(|s| s.name == "Point").unwrap();
    let signature = point.signature.as_ref().unwrap();

    assert!(signature.contains("x: number"));
    assert!(signature.contains("y: number"));
}

#[test]
fn arrow_const_is_captured_as_function_symbol() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let arrow_fn = symbols.iter().find(|s| s.name == "arrowFn").unwrap();
    assert_eq!(arrow_fn.kind, "function");
}

#[test]
fn interface_and_type_and_enum_kinds_are_distinct() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(
        symbols.iter().find(|s| s.name == "Shape").unwrap().kind,
        "interface"
    );
    assert_eq!(
        symbols.iter().find(|s| s.name == "Point").unwrap().kind,
        "type"
    );
    assert_eq!(
        symbols.iter().find(|s| s.name == "Color").unwrap().kind,
        "enum"
    );
    assert_eq!(
        symbols.iter().find(|s| s.name == "Foo").unwrap().kind,
        "class"
    );
    assert_eq!(
        symbols.iter().find(|s| s.name == "pubMethod").unwrap().kind,
        "method"
    );
}

#[test]
fn line_numbers_match_fixture() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let expected: &[(&str, u32)] = &[
        ("pub", 1),
        ("priv", 5),
        ("arrowFn", 9),
        ("notExported", 13),
        ("Foo", 15),
        ("constructor", 19),
        ("pubMethod", 23),
        ("privMethod", 27),
        ("Shape", 32),
        ("Point", 37),
        ("Color", 42),
        ("caller", 48),
    ];

    for (name, line) in expected {
        let symbol = symbols.iter().find(|s| &s.name == name).unwrap();
        assert_eq!(symbol.line, *line, "line mismatch for {name}");
    }
}

#[test]
fn reference_query_captures_plain_member_and_constructor_calls() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    assert!(references.iter().any(|r| r.callee_name == "pub"));
    assert!(references.iter().any(|r| r.callee_name == "Foo"));
    assert!(references.iter().any(|r| r.callee_name == "pubMethod"));
    assert!(references.iter().any(|r| r.callee_name == "arrowFn"));
}

#[test]
fn calls_are_attributed_to_enclosing_function_or_method() {
    let support = TypeScriptSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    let priv_call = references.iter().find(|r| r.callee_name == "priv").unwrap();
    assert_eq!(priv_call.caller_name.as_deref(), Some("pubMethod"));

    let pub_call = references.iter().find(|r| r.callee_name == "pub").unwrap();
    assert_eq!(pub_call.caller_name.as_deref(), Some("caller"));

    let arrow_call = references
        .iter()
        .find(|r| r.callee_name == "arrowFn")
        .unwrap();
    assert_eq!(arrow_call.caller_name.as_deref(), Some("caller"));
}

#[test]
fn default_import_maps_to_import_default() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| {
        e.local_name.as_deref() == Some("defaultExport")
            && e.source_path == "moduleA"
            && e.kind == ImportKind::Direct { aliased: false }
    }));
}

#[test]
fn named_import_maps_local_and_source() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("a")
        && e.source_path == "moduleB.a"
        && e.kind == ImportKind::Direct { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("b")
        && e.source_path == "moduleB.b"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn aliased_named_import_binds_alias() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("b2")
        && e.source_path == "moduleC.a"
        && e.kind == ImportKind::Direct { aliased: true }));
}

#[test]
fn namespace_import_maps_to_import_namespace() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| {
        e.local_name.as_deref() == Some("ns")
            && e.source_path == "moduleD"
            && e.kind == ImportKind::Namespace
    }));
}

#[test]
fn named_re_export_maps_to_re_export() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("x")
        && e.source_path == "moduleE.x"
        && e.kind == ImportKind::Reexport { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("y")
        && e.source_path == "moduleE.y"
        && e.kind == ImportKind::Reexport { aliased: false }));
}

#[test]
fn aliased_re_export_binds_alias() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("q")
        && e.source_path == "moduleH.p"
        && e.kind == ImportKind::Reexport { aliased: true }));
}

#[test]
fn barrel_re_export_maps_to_re_export_all() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| {
        e.local_name.is_none() && e.source_path == "moduleF" && e.kind == ImportKind::Wildcard
    }));
}

#[test]
fn side_effect_import_has_no_local_name() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(entries.iter().any(|e| {
        e.local_name.is_none() && e.source_path == "moduleG" && e.kind == ImportKind::SideEffect
    }));
}

#[test]
fn import_entry_count_matches_fixture() {
    let support = TypeScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert_eq!(entries.len(), 10);
}
