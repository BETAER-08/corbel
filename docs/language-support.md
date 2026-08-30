# Language support

## Status

| Language   | Level | Notes |
| ---------- | ----- | ----- |
| Rust       | full  | `langs/rust.rs` — own `symbol_query`/`reference_query`, `use`-tree scope walker, all five resolution stages exercised. |
| Python     | full  | `langs/python.rs` — own `symbol_query`/`reference_query`, `import`/`from ... import` scope walker, all five resolution stages exercised. |
| TypeScript | full  | `langs/typescript.rs` — own `symbol_query`/`reference_query`, ES-module scope walker, all five resolution stages exercised. |
| TSX        | full  | `langs/tsx.rs` — reuses TypeScript's `symbol_kind`/`extract_signature`/`build_scope`/`enclosing_definition_name`, overrides `extract_references` to add JSX-tag references. |
| JavaScript | full  | `langs/javascript.rs` — reuses TypeScript's `symbol_kind`/`extract_signature`/`build_scope`/`enclosing_definition_name`, overrides `extract_references` to add JSX-tag references, same as TSX. Known gap: `build_scope` only walks `import_statement`/`export_statement`, so CommonJS `require(...)` calls produce no import entry (verified by `commonjs_require_does_not_produce_an_import_entry` in `tests/javascript_tests.rs`); this doesn't lower the tier because the same ES-module-only scope walker is shared verbatim with TypeScript/TSX. |

The five-stage resolution chain (same-file, scoped, global-unique, external,
unresolved) is implemented once in `corbel-core/src/resolve.rs`
(`resolve_all`) and is language-agnostic: it reads only the generic
`symbols`/`imports`/`relationships` tables and never branches on source
language, so all five stages apply identically to every registered
language above.

`LanguageSupport` is frozen based on five languages clearing it end to end.
JavaScript confirmed the contract rather than changing it, since it shares a
grammar family with TypeScript/TSX: it reuses `typescript::build_scope`,
`typescript::symbol_kind`, `typescript::extract_signature`, and
`typescript::enclosing_definition_name` verbatim, and defines its own
`reference_query`/`extract_references` override to add JSX-tag references —
the same override TSX uses. The trait was not reopened; JavaScript turned up
no requirement the other four didn't already cover.

## The `LanguageSupport` contract

Every method on the trait falls into one of two categories, per the policy
in `CONTRIBUTING.md`:

### Required — decides a language-specific fact

These have no default body. Each language must answer for itself:

- `extensions` — file extensions this implementation claims.
- `grammar` — the tree-sitter `Language` to parse with.
- `symbol_query` / `reference_query` — tree-sitter query strings, written
  against that language's own grammar; capture names (`@definition`,
  `@name`, `@callee`) are the only thing the orchestration layer depends on.
- `symbol_kind` — maps a captured node's tree-sitter kind to corbel's
  symbol kind string.
- `extract_signature` — the display signature for a symbol, built from
  wherever that language's grammar puts a body to exclude.
- `is_public` — whether a symbol is externally visible.
- `build_scope` — the file's import/re-export table, as `ScopeEntry` values.
- `enclosing_definition_name` — walks a reference node's ancestors to find
  the symbol that contains it.

### Default — orchestrates, invents nothing

- `extract_symbols` — runs `symbol_query`, then calls `symbol_kind`,
  `extract_signature`, and `is_public` on each match. Every language gets
  this for free because it only combines results the required methods
  already computed.
- `extract_references` — runs `reference_query`, then calls
  `enclosing_definition_name` on each `@callee` capture. TSX and JavaScript
  override this one: their query captures two disjoint groups (`@callee` for
  ordinary calls, `@jsx_callee` for JSX tags), and the default only knows
  about `@callee`. Overriding to dispatch on capture name isn't a language
  fabricating data — it still gets every fact from the same required
  methods — so it stays within the policy.

## What five languages confirmed

- **`is_public` covers three different visibility models without changing
  shape.** Rust reads a `pub` keyword. Python reads a leading-underscore
  naming convention. TypeScript/TSX read `export` plus, for class members,
  an explicit accessibility modifier (`public`/`private`/`protected`).
  Three different sources of truth, one `bool` return — the trait didn't
  need to know which model a language uses.

- **`symbol_kind` can return an open string set per language and the
  pipeline still works.** Rust emits `function`/`struct`/`enum`/`trait`.
  Python emits `function`/`class`. TypeScript/TSX add
  `method`/`interface`/`type`/`enum`/`class` on top. Nothing downstream
  (storage, resolution) switches on a closed set of kind values — kind is
  stored and surfaced as opaque data, so a language is free to define
  whatever kinds make sense for it.

- **`build_scope` tolerates completely different traversal strategies.**
  Rust walks a single tree-sitter query over `use_declaration` nodes and
  recurses into the matched subtree by hand for nested groups. Python
  recursively walks the whole tree looking for `import_statement` and
  `import_from_statement` nodes with no query involved. TypeScript/TSX walk
  the tree for `import_statement`/`export_statement` nodes and dispatch on
  child node kind. The contract only asks for a `ScopeTable` back — how a
  language gets there (query, recursion, hybrid) is invisible to every
  caller.

- **`ImportKind` covers all five languages' import forms with no new
  variant.** `Direct { aliased }`, `Reexport { aliased }`, `Wildcard`,
  `Namespace`, `SideEffect` were designed against Rust/Python/TypeScript
  and held unchanged through TSX and JavaScript, both of which reuse
  TypeScript's `build_scope` verbatim. The one enum distinction that mattered in practice:
  `import * as ns` binds a concrete name and is `Namespace`, while
  `export * from "m"` / `from m import *` / `use a::*` bind no name and are
  all `Wildcard` — collapsing that distinction was the bug the enum was
  introduced to prevent (a wildcard re-export was previously
  string-compared against `"glob"` and slipped through as if it were a
  scoped import).

- **`extract_signature` needing a per-language, sometimes per-node-kind,
  override is the intended design, not a gap.** Rust and Python share the
  same "text up to the body" shape, but where "the body" starts differs by
  grammar. TypeScript/TSX need three separate cases in one implementation
  (`interface_declaration`/`type_alias_declaration` keep their full text,
  `variable_declarator` walks up to the enclosing declaration and down into
  the arrow function's body, everything else falls back to the
  Rust/Python shape). This is exactly what "required, no default" is
  for: signature formatting is a language-specific fact, and the contract
  never pretended otherwise.

## Awaiting promotion

These languages are candidates and are promoted only after they clear the
gates described in `CONTRIBUTING.md`:

Go, Java, C#, C, C++, Ruby, PHP.

## Permanently excluded

These are out of scope because they carry no call graph for corbel to map:

| Language | Reason                                             |
| -------- | --------------------------------------------------- |
| HTML     | Markup, not executable call structure.               |
| CSS      | Styling rules, not executable call structure.        |
| JSON     | Data, not executable call structure.                 |
| Bash     | Shell scripting outside corbel's resolution model.   |
