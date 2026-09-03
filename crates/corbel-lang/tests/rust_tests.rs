use corbel_lang::langs::rust::RustSupport;
use corbel_lang::support::{ImportKind, LanguageSupport, ScopeEntry};
use std::fs;

fn fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rust_basic.rs"
    ))
    .expect("fixture reads")
}

fn use_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rust_use.rs"
    ))
    .expect("fixture reads")
}

fn build_scope_entries(support: &RustSupport, src: &str) -> Vec<ScopeEntry> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&support.grammar()).unwrap();
    let tree = parser.parse(src, None).unwrap();
    support.build_scope(&tree, src).entries
}

#[test]
fn extract_symbols_finds_expected_count() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    assert_eq!(symbols.len(), 6);
}

#[test]
fn visibility_is_detected_correctly() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    assert!(add.is_public);

    let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
    assert!(!helper.is_public);

    let new_fn = symbols.iter().find(|s| s.name == "new").unwrap();
    assert!(new_fn.is_public);

    let distance = symbols
        .iter()
        .find(|s| s.name == "distance_from_origin")
        .unwrap();
    assert!(!distance.is_public);
}

#[test]
fn function_signature_excludes_body() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    let signature = add.signature.as_ref().unwrap();

    assert!(!signature.contains('{'));
    assert_eq!(signature, "pub fn add(a: i32, b: i32) -> i32");
}

#[test]
fn struct_signature_excludes_fields() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let point = symbols.iter().find(|s| s.name == "Point").unwrap();
    let signature = point.signature.as_ref().unwrap();

    assert!(!signature.contains('{'));
    assert!(!signature.contains("pub x"));
    assert_eq!(signature, "pub struct Point");
}

#[test]
fn line_numbers_match_fixture() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let expected: &[(&str, u32)] = &[
        ("add", 1),
        ("helper", 5),
        ("Point", 9),
        ("Shape", 14),
        ("new", 19),
        ("distance_from_origin", 23),
    ];

    for (name, line) in expected {
        let symbol = symbols.iter().find(|s| &s.name == name).unwrap();
        assert_eq!(symbol.line, *line, "line mismatch for {name}");
    }
}

#[test]
fn reference_query_captures_call_expressions() {
    let support = RustSupport;
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
fn build_scope_simple_path_maps_local_name_to_source_path() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("c")
        && e.source_path == "a::b::c"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn build_scope_as_alias_maps_alias_to_original_path() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("d")
        && e.source_path == "a::b::c"
        && e.kind == ImportKind::Direct { aliased: true }));
}

#[test]
fn build_scope_group_expands_into_individual_entries() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("d")
        && e.source_path == "a::b::d"
        && e.kind == ImportKind::Direct { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("e")
        && e.source_path == "a::b::e"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn build_scope_nested_group_expands_fully() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("e")
        && e.source_path == "a::d::e"
        && e.kind == ImportKind::Direct { aliased: false }));
    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("f")
        && e.source_path == "a::d::f"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn build_scope_self_in_group_binds_to_parent_path() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.as_deref() == Some("b")
        && e.source_path == "a::b"
        && e.kind == ImportKind::Direct { aliased: false }));
}

#[test]
fn build_scope_glob_has_no_specific_local_name() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(entries.iter().any(|e| e.local_name.is_none()
        && e.source_path == "a::b"
        && e.kind == ImportKind::Wildcard));
}

#[test]
fn build_scope_pub_use_is_marked_distinctly() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    let pub_use_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.kind == ImportKind::Reexport { aliased: false })
        .collect();
    assert_eq!(pub_use_entries.len(), 1);
    assert_eq!(pub_use_entries[0].local_name.as_deref(), Some("c"));
    assert_eq!(pub_use_entries[0].source_path, "a::b::c");
}

#[test]
fn build_scope_preserves_crate_and_super_prefixes() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(
        entries
            .iter()
            .any(|e| e.local_name.as_deref() == Some("Bar") && e.source_path == "crate::foo::Bar")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.local_name.as_deref() == Some("Qux") && e.source_path == "super::baz::Qux")
    );
}

#[test]
fn build_scope_finds_use_declarations_nested_in_mod_blocks() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert!(
        entries
            .iter()
            .any(|e| e.local_name.as_deref() == Some("z") && e.source_path == "x::y::z")
    );
}

#[test]
fn build_scope_entry_count_matches_fixture() {
    let support = RustSupport;
    let src = use_fixture_src();
    let entries = build_scope_entries(&support, &src);

    assert_eq!(entries.len(), 16);
}

fn impl_owner_variants_fixture_src() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rust_impl_owner_variants.rs"
    ))
    .expect("fixture reads")
}

#[test]
fn free_functions_have_no_owner() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let add = symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.owner, None);

    let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
    assert_eq!(helper.owner, None);
}

#[test]
fn methods_inside_impl_block_are_owner_qualified_by_the_impl_type() {
    let support = RustSupport;
    let src = fixture_src();
    let symbols = support.extract_symbols(&src);

    let new_fn = symbols.iter().find(|s| s.name == "new").unwrap();
    assert_eq!(new_fn.owner.as_deref(), Some("Point"));

    let distance = symbols
        .iter()
        .find(|s| s.name == "distance_from_origin")
        .unwrap();
    assert_eq!(distance.owner.as_deref(), Some("Point"));
}

#[test]
fn generic_impl_type_owner_strips_type_parameters() {
    let support = RustSupport;
    let src = impl_owner_variants_fixture_src();
    let symbols = support.extract_symbols(&src);

    let get = symbols.iter().find(|s| s.name == "get").unwrap();
    assert_eq!(get.owner.as_deref(), Some("Container"));
}

#[test]
fn trait_impl_owner_uses_the_concrete_type_not_the_trait() {
    let support = RustSupport;
    let src = impl_owner_variants_fixture_src();
    let symbols = support.extract_symbols(&src);

    let foo_method = symbols.iter().find(|s| s.name == "foo_method").unwrap();
    assert_eq!(foo_method.owner.as_deref(), Some("Bar"));
}

#[test]
fn scoped_type_identifier_impl_owner_uses_the_short_type_name() {
    let support = RustSupport;
    let src = impl_owner_variants_fixture_src();
    let symbols = support.extract_symbols(&src);

    let baz_method = symbols.iter().find(|s| s.name == "baz_method").unwrap();
    assert_eq!(baz_method.owner.as_deref(), Some("Baz"));
}
