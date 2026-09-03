use corbel_lang::langs::tsx::TsxSupport;
use corbel_lang::support::{ImportKind, LanguageSupport};
use std::fs;

fn fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tsx_basic.tsx"
    ))
    .expect("fixture reads")
}

fn imports_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tsx_imports.tsx"
    ))
    .expect("fixture reads")
}

fn scope_entries(support: &TsxSupport, src: &str) -> Vec<corbel_lang::support::ScopeEntry> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(src, None).unwrap();
    support.build_scope(&tree, src).entries
}

#[test]
fn component_function_is_captured_as_symbol() {
    let support = TsxSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Comp" && s.kind == "function")
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Arrow" && s.kind == "function")
    );
}

#[test]
fn extract_symbols_finds_expected_count() {
    let support = TsxSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(symbols.len(), 5);
}

#[test]
fn export_based_visibility_matches_typescript_behavior() {
    let support = TsxSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let comp = symbols.iter().find(|s| s.name == "Comp").unwrap();
    assert!(comp.is_public);

    let arrow = symbols.iter().find(|s| s.name == "Arrow").unwrap();
    assert!(arrow.is_public);

    let plain = symbols.iter().find(|s| s.name == "plain").unwrap();
    assert!(!plain.is_public);

    let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
    assert!(!helper.is_public);

    let caller = symbols.iter().find(|s| s.name == "caller").unwrap();
    assert!(caller.is_public);
}

#[test]
fn function_signature_excludes_jsx_body() {
    let support = TsxSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let comp = symbols.iter().find(|s| s.name == "Comp").unwrap();
    let signature = comp.signature.as_ref().unwrap();

    assert!(!signature.contains("<div"));
    assert!(signature.contains("Comp"));
}

#[test]
fn jsx_component_usage_is_captured_as_reference() {
    let support = TsxSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    let child_refs: Vec<_> = references
        .iter()
        .filter(|r| r.callee_name == "Child")
        .collect();
    assert_eq!(child_refs.len(), 1);
}

#[test]
fn lowercase_html_elements_are_not_captured_as_references() {
    let support = TsxSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    assert!(!references.iter().any(|r| r.callee_name == "div"));
    assert!(!references.iter().any(|r| r.callee_name == "span"));
}

#[test]
fn member_expression_component_captures_property_name() {
    let support = TsxSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    assert!(references.iter().any(|r| r.callee_name == "Item"));
}

#[test]
fn jsx_component_usage_is_attributed_to_enclosing_function_component() {
    let support = TsxSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    let child_ref = references
        .iter()
        .find(|r| r.callee_name == "Child")
        .unwrap();
    assert_eq!(child_ref.caller_name.as_deref(), Some("Comp"));

    let item_ref = references.iter().find(|r| r.callee_name == "Item").unwrap();
    assert_eq!(item_ref.caller_name.as_deref(), Some("Comp"));

    let comp_refs: Vec<_> = references
        .iter()
        .filter(|r| r.callee_name == "Comp")
        .collect();
    assert_eq!(comp_refs.len(), 2);
    assert!(
        comp_refs
            .iter()
            .any(|r| r.caller_name.as_deref() == Some("Arrow"))
    );
    assert!(
        comp_refs
            .iter()
            .any(|r| r.caller_name.as_deref() == Some("caller"))
    );
}

#[test]
fn plain_function_calls_are_still_captured() {
    let support = TsxSupport;
    let src = fixture_src();
    let references = support.extract_references(&src);

    let helper_ref = references
        .iter()
        .find(|r| r.callee_name == "helper")
        .unwrap();
    assert_eq!(helper_ref.caller_name.as_deref(), Some("plain"));

    let plain_ref = references
        .iter()
        .find(|r| r.callee_name == "plain")
        .unwrap();
    assert_eq!(plain_ref.caller_name.as_deref(), Some("caller"));
}

#[test]
fn imports_map_to_import_kind_without_new_variants() {
    let support = TsxSupport;
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
fn reference_query_still_compiles_and_captures_calls() {
    let support = TsxSupport;
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

fn class_owner_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tsx_class_owner.tsx"
    ))
    .expect("fixture reads")
}

#[test]
fn free_functions_have_no_owner() {
    let support = TsxSupport;
    let src = class_owner_fixture_src();
    let symbols = support.extract_symbols(&src);

    let free_fn = symbols.iter().find(|s| s.name == "freeFn").unwrap();
    assert_eq!(free_fn.owner, None);
}

#[test]
fn class_methods_are_owner_qualified_by_the_class_name() {
    let support = TsxSupport;
    let src = class_owner_fixture_src();
    let symbols = support.extract_symbols(&src);

    let render = symbols.iter().find(|s| s.name == "render").unwrap();
    assert_eq!(render.owner.as_deref(), Some("Widget"));
}
