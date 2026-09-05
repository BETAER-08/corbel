# Architecture

## Crate layout

corbel is split into three crates.

| Crate          | Responsibility                                                                 |
| -------------- | ------------------------------------------------------------------------------ |
| `corbel-core`  | File walking, hashing, symbol and scope models, resolution, storage, querying. |
| `corbel-lang`  | Language support plugins built on tree-sitter and the core symbol model.       |
| `corbel`       | Command-line interface and MCP server binary.                                  |

### corbel-core modules

| Module    | Responsibility                                              |
| --------- | ---------------------------------------------------------- |
| `path`    | Repository-relative path normalization.                    |
| `walk`    | Directory traversal honoring ignore rules.                 |
| `hash`    | Content hashing for change detection.                      |
| `symbol`  | Symbol records and their identifiers.                      |
| `scope`   | Lexical scope model used during resolution.                |
| `resolve` | Reference-to-definition resolution.                        |
| `budget`  | Output size accounting for query responses.                |
| `embed`   | Embedding model access.                                    |
| `query`   | Query engine over the indexed store.                       |
| `store`   | Persistent index storage, schema, and migrations.          |
| `error`   | Shared error types.                                        |

### corbel-lang modules

| Module     | Responsibility                                            |
| ---------- | -------------------------------------------------------- |
| `support`  | The `LanguageSupport` contract.                          |
| `registry` | Lookup from file to language support.                    |
| `langs`    | Per-language implementations.                            |

## Indexing pipeline

Indexing walks the repository, filters by ignore rules, hashes each file to
detect changes, parses changed files into symbols and references, builds per-
file scopes, resolves references, and writes the result to the store.

## Resolution chain

A reference is resolved by walking a fixed chain and stopping at the first stage
that yields a unique definition:

1. `same-file` — a definition in the same file.
2. `scoped` / `global-unique` — a single definition with that name exists
   anywhere in the index. Both labels come from the same lookup (exactly one
   candidate outside the caller's file); the label only reports whether the
   caller's file has an import statement whose local name or last path
   segment matches the reference. The import is *not* used to pick the
   definition and does not disambiguate anything: if more than one candidate
   exists, the reference is `unresolved` regardless of whether an import is
   present. Because the import check compares against `last_segment`, which
   splits only on `::`, this matching is effectively Rust-path-shaped —
   in Python and TypeScript, where import paths don't use `::`, `scoped` can
   only be reached via a `local_name` match, not a path match. This is a
   known limitation, not an import-following resolver.
3. `external` — no definition with that name exists anywhere in the index.
4. `unresolved` — more than one same-named definition exists outside the
   caller's file, so no unique target can be chosen.

Every resolution records which stage produced it.

## Schema migrations and content hashes

`files.hash` is only a valid change-detection signal relative to the exact
parsing and symbol-extraction logic that produced the rows keyed by it. If a
future schema migration changes what gets stored for a file (new symbol
fields, a different resolution scheme, a changed embedding format, etc.), the
existing hashes no longer guarantee "nothing to do here" — indexing could skip
files whose stored data is now stale relative to the new schema, even though
the file's on-disk content hasn't changed. Any migration step that alters
what `symbols`, `relationships`, `imports`, or `embeddings` capture for a file
must therefore also clear the `files.hash` column (or truncate `files`
outright) as part of that migration, forcing a full re-index on next run
rather than trusting hashes computed under the old schema's assumptions.
