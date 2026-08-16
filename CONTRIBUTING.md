# Contributing

## Development environment

The toolchain is pinned by `rust-toolchain.toml`. Running any cargo command in
this repository selects the correct compiler, rustfmt, and clippy automatically.

## Verification

Run all of the following before opening a pull request:

```
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Adding a language

A new language is promoted only when it clears every gate below:

- `extract_signature` and `is_public` are implemented for real, not left at a
  default that returns empty or trivial values.
- `build_scope` is implemented.
- The shared fixtures pass.
- The unresolved ratio stays at or below the threshold.

Pull requests that route around these gates with a default implementation are
not accepted.

### Default implementation policy on `LanguageSupport`

A method that returns language-specific data — a symbol kind, a signature, a
visibility flag, a scope entry, or anything else whose correct value differs
per language — must never carry a default implementation. It must be
`required`, with no body, forcing every language to supply its own real
answer. This is a hard rule, not a style preference: amdb 1.0 shipped default
bodies that quietly returned plausible-looking placeholder data for
languages that hadn't implemented the method yet, and that fabricated data
was indexed and served as if it were real, silently corrupting query results
for months before anyone noticed.

A method that only orchestrates — calling other required methods and
assembling their results into a return value, without inventing or guessing
at any language-specific fact itself — may carry a default implementation.
`extract_symbols` and `extract_references` are the current examples: their
default bodies run the tree-sitter query returned by the required
`symbol_query`/`reference_query` methods and delegate every piece of
language-specific judgment (the symbol's kind, its signature, its
visibility) to the required methods that already exist for that purpose.
Sharing this orchestration avoids duplicating identical query-running
boilerplate across every language implementation, with no risk of a
language silently inheriting wrong data, because there is no data left for
the default body to get wrong.

When adding a method to `LanguageSupport`, ask: does this method decide any
fact about the language, or does it only combine facts decided elsewhere? If
it decides a fact, it is required. If it only combines, a default is
allowed.

## Merging

Pull requests are merged with squash merge only.
