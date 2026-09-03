use corbel_lang::support::{ImportKind, RawReference, RawSymbol, ScopeEntry, ScopeTable};

#[test]
fn raw_symbol_is_debug_clone_partial_eq() {
    let a = RawSymbol {
        name: "foo".to_string(),
        kind: "function".to_string(),
        line: 1,
        signature: Some("fn foo()".to_string()),
        is_public: true,
        owner: None,
    };
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn raw_reference_is_debug_clone_partial_eq() {
    let a = RawReference {
        callee_name: "bar".to_string(),
        line: 2,
        caller_name: Some("foo".to_string()),
    };
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn scope_entry_is_debug_clone_partial_eq() {
    let a = ScopeEntry {
        local_name: Some("baz".to_string()),
        source_path: "std::baz".to_string(),
        kind: ImportKind::Direct { aliased: false },
    };
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn scope_table_is_debug_clone_partial_eq() {
    let a = ScopeTable {
        entries: vec![ScopeEntry {
            local_name: Some("baz".to_string()),
            source_path: "std::baz".to_string(),
            kind: ImportKind::Direct { aliased: false },
        }],
    };
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}
