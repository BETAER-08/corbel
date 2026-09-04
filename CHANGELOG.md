# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-09-04

### Added

- `find` tool: substring search over symbol names, case-insensitive
  (ASCII-only folding — differently-cased Unicode identifiers won't match).
- `token_budget` parameter for `get_symbol`. The budget is split evenly
  across matched symbols, and each symbol's share is split evenly again
  between its callers and callees.
- Index schema versioning.
- `owner` column on the `symbols` table.
- Benchmark harness and a 120-item golden set.
- Windows x86_64 binary (CI's `windows-latest` job passes on the current
  `main`).

### Changed

- **BREAKING:** `callers`/`callees` `name` values in MCP responses are now
  owner-qualified for methods — `Owner.method` (`Owner::method` for Rust).
  Free functions keep their bare name; this asymmetry is intentional. JSON
  keys are unchanged, only the meaning of the `name` value changes.
- **BREAKING:** index schema v3 → v4. Existing indexes must be rebuilt with
  `corbel index`.
- `corbel serve` now opens the index read-only. On a schema version
  mismatch it no longer migrates silently — it exits with an error telling
  you to reindex.
- Requests to `find` above `MAX_FIND_LIMIT` are now rejected with
  `InvalidParams` instead of being silently clamped.

### Fixed

- A single file failing to index no longer aborts the whole indexing run.
- Binary/non-UTF-8 files were silently mis-indexed instead of being
  skipped (`from_utf8_lossy` → `from_utf8`).
- A UTF-8 BOM at the start of a file could break parsing.
- Skipped files weren't counted in the indexing summary.
- The repository URL pointed at an org that doesn't exist.

### Known issues

- Methods defined as default bodies inside a trait declaration don't get
  an owner attributed, because `owner_of_definition` only walks
  `impl_item` ancestors, not `trait_item`. Confirmed on 3 cases in the
  benchmark set.
- `find` can't use an index for its leading-wildcard `LIKE` query, so each
  call does two full table scans. Observed p99 of 122ms at ~110-120k
  symbols.
- `impact` has no `depth` parameter — it always traverses to the internal
  max depth (10) or until its budget is exhausted.

### Notes

- The 0.1.0 install script points at an internal download URL under an
  org that doesn't exist, so it doesn't work. The metadata published to
  crates.io for 0.1.0 can't be fixed retroactively. 0.1.0 users should use
  `cargo install corbel` or the 0.2.0 install script instead.

## [0.1.0] - 2026-08-31

### Added

- Repository foundation: error types, repo-relative paths, content hashing,
  gitignore-aware file walking, and the SQLite-backed store (schema +
  migrations).
- Indexing pipeline: parses each file, stores symbols and imports, then
  resolves every call through same-file → scoped-import → global-unique →
  external → unresolved, in that order.
- `corbel index` CLI command, reporting files indexed and internal
  resolution rate.
- Language support for Rust, Python, TypeScript, TSX, and JavaScript,
  sharing one `LanguageSupport` contract.
- `ImportKind` enum (`Direct`, `Reexport`, `Wildcard`, `Namespace`,
  `SideEffect`) replacing string-typed import kinds, closing a bug where a
  wildcard re-export could be mistaken for a scoped import.

### Changed

- Internal refactors across the store, resolver, and language layer to
  support the above without changing external behavior.
