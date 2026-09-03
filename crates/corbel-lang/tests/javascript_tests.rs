use corbel_lang::langs::javascript::JavaScriptSupport;
use corbel_lang::support::{ImportKind, LanguageSupport};
use std::fs;

fn fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/javascript_basic.js"
    ))
    .expect("fixture reads")
}

fn imports_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/javascript_imports.js"
    ))
    .expect("fixture reads")
}

fn jsx_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/javascript_jsx.js"
    ))
    .expect("fixture reads")
}

fn scope_entries(support: &JavaScriptSupport, src: &str) -> Vec<corbel_lang::support::ScopeEntry> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(src, None).unwrap();
    support.build_scope(&tree, src).entries
}

#[test]
fn extract_symbols_finds_expected_count() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(symbols.len(), 9);
}

#[test]
fn export_based_visibility_is_detected() {
    let support = JavaScriptSupport;
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
fn class_members_default_public_and_private_field_method_is_private() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let constructor = symbols.iter().find(|s| s.name == "constructor").unwrap();
    assert!(constructor.is_public);

    let pub_method = symbols.iter().find(|s| s.name == "pubMethod").unwrap();
    assert!(pub_method.is_public);

    let priv_method = symbols.iter().find(|s| s.name == "#privMethod").unwrap();
    assert!(!priv_method.is_public);
}

#[test]
fn function_signature_excludes_body_and_types() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let pub_fn = symbols.iter().find(|s| s.name == "pub").unwrap();
    let signature = pub_fn.signature.as_ref().unwrap();

    assert!(!signature.contains("return"));
    assert_eq!(signature, "function pub(a, b)");
}

#[test]
fn arrow_function_const_signature_excludes_body() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let arrow_fn = symbols.iter().find(|s| s.name == "arrowFn").unwrap();
    let signature = arrow_fn.signature.as_ref().unwrap();

    assert!(!signature.contains("return"));
    assert_eq!(signature, "const arrowFn = (a) =>");
}

#[test]
fn class_signature_includes_extends_header() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let foo = symbols.iter().find(|s| s.name == "Foo").unwrap();
    let signature = foo.signature.as_ref().unwrap();

    assert!(signature.contains("class Foo extends Base"));
    assert!(!signature.contains("constructor"));
}

#[test]
fn arrow_const_is_captured_as_function_symbol() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let arrow_fn = symbols.iter().find(|s| s.name == "arrowFn").unwrap();
    assert_eq!(arrow_fn.kind, "function");
}

#[test]
fn class_and_method_kinds_are_distinct() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(
        symbols.iter().find(|s| s.name == "Foo").unwrap().kind,
        "class"
    );
    assert_eq!(
        symbols.iter().find(|s| s.name == "pubMethod").unwrap().kind,
        "method"
    );
    assert_eq!(
        symbols.iter().find(|s| s.name == "pub").unwrap().kind,
        "function"
    );
}

#[test]
fn reference_query_captures_plain_and_member_and_constructor_calls() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    assert!(references.iter().any(|r| r.callee_name == "pub"));
    assert!(references.iter().any(|r| r.callee_name == "priv"));
    assert!(references.iter().any(|r| r.callee_name == "arrowFn"));
    assert!(references.iter().any(|r| r.callee_name == "Foo"));
}

#[test]
fn calls_are_attributed_to_enclosing_function_or_method() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    let priv_call = references.iter().find(|r| r.callee_name == "priv").unwrap();
    assert_eq!(priv_call.caller_name.as_deref(), Some("pubMethod"));

    let pub_call = references
        .iter()
        .find(|r| r.callee_name == "pub" && r.caller_name.as_deref() == Some("caller"));
    assert!(pub_call.is_some());
}

#[test]
fn imports_map_to_import_kind_without_new_variants() {
    let support = JavaScriptSupport;
    let src = imports_fixture_src();
    let entries = scope_entries(&support, &src);

    assert!(
        entries
            .iter()
            .any(|e| e.local_name.as_deref() == Some("defaultExport")
                && e.source_path == "moduleA"
                && e.kind == ImportKind::Direct { aliased: false })
    );
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("a")
        && e.source_path == "moduleB.a"
        && e.kind == ImportKind::Direct { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("b2")
        && e.source_path == "moduleC.a"
        && e.kind == ImportKind::Direct { aliased: true }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("ns")
        && e.source_path == "moduleD"
        && e.kind == ImportKind::Namespace));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("x")
        && e.source_path == "moduleE.x"
        && e.kind == ImportKind::Reexport { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.is_none()
        && e.source_path == "moduleF"
        && e.kind == ImportKind::Wildcard));
    assert!(entries.iter().any(|e| e.local_name.is_none()
        && e.source_path == "moduleG"
        && e.kind == ImportKind::SideEffect));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("q")
        && e.source_path == "moduleH.p"
        && e.kind == ImportKind::Reexport { aliased: true }));

    assert_eq!(entries.len(), 10);
}

#[test]
fn commonjs_require_does_not_produce_an_import_entry() {
    let support = JavaScriptSupport;
    let src = "const x = require(\"commonjs-mod\");\n";
    let entries = scope_entries(&support, src);

    assert!(entries.is_empty());
}

#[test]
fn jsx_component_usage_is_captured_as_reference() {
    let support = JavaScriptSupport;
    let src = jsx_fixture_src();
    let references = support.extract_references(&src);

    let child_refs: Vec<_> = references
        .iter()
        .filter(|r| r.callee_name == "Child")
        .collect();
    assert_eq!(child_refs.len(), 1);
}

#[test]
fn lowercase_html_elements_are_not_captured_as_references() {
    let support = JavaScriptSupport;
    let src = jsx_fixture_src();
    let references = support.extract_references(&src);

    assert!(!references.iter().any(|r| r.callee_name == "div"));
    assert!(!references.iter().any(|r| r.callee_name == "span"));
}

#[test]
fn member_expression_component_captures_property_name() {
    let support = JavaScriptSupport;
    let src = jsx_fixture_src();
    let references = support.extract_references(&src);

    assert!(references.iter().any(|r| r.callee_name == "Item"));
}

#[test]
fn reference_query_still_compiles_and_captures_calls() {
    let support = JavaScriptSupport;
    let src = fixture_src();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(&src, None).unwrap();

    let query = tree_sitter::Query::new(&support.grammar(), support.reference_query())
        .expect("reference query compiles");
    let mut cursor = tree_sitter::QueryCursor::new();

    use tree_sitter::StreamingIterator;
    let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());

    let mut count = 0;
    while matches.next().is_some() {
        count += 1;
    }

    assert!(count >= 1);
}

#[test]
fn free_functions_have_no_owner() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let pub_fn = symbols.iter().find(|s| s.name == "pub").unwrap();
    assert_eq!(pub_fn.owner, None);
}

#[test]
fn class_methods_are_owner_qualified_by_the_class_name() {
    let support = JavaScriptSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let pub_method = symbols.iter().find(|s| s.name == "pubMethod").unwrap();
    assert_eq!(pub_method.owner.as_deref(), Some("Foo"));
}
