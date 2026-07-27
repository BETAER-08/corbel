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
2. `scoped` — a definition reachable through the reference's lexical scope.
3. `global-unique` — a single matching definition across the whole index.
4. `unresolved` — no unique definition was found.

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
