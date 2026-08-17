# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Repository foundation: error types, repo-relative paths, content hashing,
  gitignore-aware file walking, and the SQLite-backed store (schema +
  migrations).
- Indexing pipeline: parses each file, stores symbols and imports, then
  resolves every call through same-file → scoped-import → global-unique →
  external → unresolved, in that order.
- `corbel index` CLI command, reporting files indexed and internal
  resolution rate.
- Language support for Rust, Python, TypeScript, and TSX, sharing one
  `LanguageSupport` contract (JavaScript is next).
- `ImportKind` enum (`Direct`, `Reexport`, `Wildcard`, `Namespace`,
  `SideEffect`) replacing string-typed import kinds, closing a bug where a
  wildcard re-export could be mistaken for a scoped import.

### Changed

- Internal refactors across the store, resolver, and language layer to
  support the above without changing external behavior.
