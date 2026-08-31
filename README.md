# corbel

corbel is a local **MCP server** that performs **static analysis** to build a resolved **call graph** of your codebase, so a coding agent can ask "what calls this?" and "what breaks if I change this?" without guessing. It is not AI-based — no model runs inside it, and it makes no probabilistic claims about your code. corbel is also the name of an architectural bracket, and unrelated to the Microsoft font of the same name.

*A corbel (/ˈkɔːrbəl/) is the bracket built into a wall that carries the load above it. corbel maps what carries what in your code.*

corbel ships as a single static binary with no runtime dependencies, and your code never leaves the machine.

## The problem, in one real query

"What calls `open_connection`?" — ripgrep and corbel, run against this repository:

```
$ rg -n '\bopen_connection\s*\(' --type rust -g '!target' .
./crates/corbel/src/mcp/server.rs:162:        open_connection(":memory:").unwrap()
./crates/corbel/src/mcp/server.rs:171:        let conn = open_connection(":memory:").unwrap();
./crates/corbel/src/mcp/server.rs:187:        let conn = open_connection(":memory:").unwrap();
./crates/corbel-core/tests/store_tests.rs:122:    let conn1 = open_connection(&db_path).unwrap();
./crates/corbel-core/tests/store_tests.rs:125:    let conn2 = open_connection(&db_path).unwrap();
...19 more lines, each a bare file:line with no indication of which function the call is inside
```

ripgrep finds every text occurrence of `open_connection(` — 24 lines, unlabeled. Telling which caller is which means opening each file. corbel's `get_symbol`, called on the same function, resolves each hit to the function it's actually inside:

```json
{
  "callers": [
    { "file": "crates/corbel-core/tests/store_tests.rs", "line": 118, "name": "reopening_migrated_db_does_not_duplicate_schema", "resolution": "scoped" },
    { "file": "crates/corbel-core/tests/store_tests.rs", "line": 118, "name": "reopening_migrated_db_does_not_duplicate_schema", "resolution": "scoped" },
    { "file": "crates/corbel/src/commands/index.rs", "line": 11, "name": "run", "resolution": "scoped" },
    { "file": "crates/corbel/src/mcp/server.rs", "line": 165, "name": "indexed_conn", "resolution": "scoped" }
  ]
}
```

Two of those 24 `rg` lines are both inside `reopening_migrated_db_does_not_duplicate_schema` calling `open_connection` twice — `get_symbol` tells you that directly; grep leaves you to work it out by reading the file. That's the gap corbel closes: not finding text, but naming the caller.

## Install

```
cargo install corbel
```

Pre-built binaries are produced by [cargo-dist](https://github.com/axodotdev/cargo-dist) shell and PowerShell installers on tagged releases:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/trybetaer/corbel/releases/latest/download/corbel-installer.sh | sh
```

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/trybetaer/corbel/releases/latest/download/corbel-installer.ps1 | iex"
```

**Supported platforms** (per `dist-workspace.toml`, each built and tested in CI): `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.

## Claude Code, in three steps

```
corbel index .
claude mcp add corbel -- corbel serve
```

Then ask a refactoring question in plain language — the agent calls `get_symbol`/`impact`/`find` on its own:

> "If I change `resolve_all`, what else needs to change?"

For other MCP clients, add corbel directly to the server config:

```json
{
  "mcpServers": {
    "corbel": { "command": "corbel", "args": ["serve"] }
  }
}
```

## The three tools

**`get_symbol`** looks up a symbol by name and returns its definition (file, line, signature) plus everything that calls it and everything it calls. Every edge carries a `resolution` field explaining how corbel matched it to a specific definition. Real response, `get_symbol("read_schema_version")` against this repo:

```json
{
  "results": [{
    "name": "read_schema_version",
    "file": "crates/corbel-core/src/store/migrate.rs",
    "line": 6,
    "signature": "fn read_schema_version(conn: &Connection) -> Option<i32>",
    "callers": [
      { "file": "crates/corbel-core/src/store/migrate.rs", "line": 32, "name": "open_connection", "resolution": "same-file" },
      { "file": "crates/corbel-core/src/store/migrate.rs", "line": 52, "name": "open_for_serve", "resolution": "same-file" }
    ],
    "callees": [
      { "file": null, "name": "query_row", "resolution": "external" }
    ],
    "truncated": false
  }]
}
```

**`impact`** is the flagship tool: it walks the reverse call graph from a symbol across multiple hops and returns every affected symbol tagged with `depth` and `resolution` — the multi-hop trace a single grep or a one-hop "find references" cannot do. Real response, `impact("resolve_all")` against this repo (86 affected symbols total, truncated here for length):

```json
{
  "results": [{
    "target_name": "resolve_all",
    "affected": [
      { "depth": 1, "file": "crates/corbel-core/src/index.rs", "line": 29, "name": "index_repo", "resolution": "scoped" },
      { "depth": 2, "file": "crates/corbel-core/tests/impact_tests.rs", "line": 29, "name": "direct_caller_is_captured_at_depth_one", "resolution": "scoped" },
      { "depth": 3, "file": "crates/corbel/src/mcp/server.rs", "line": 255, "name": "get_symbol_call_returns_callers_and_callees", "resolution": "same-file" }
    ],
    "affected_count": 86,
    "max_depth_reached": 3,
    "truncated": false
  }]
}
```

**`find`** is a name search over the index, for when the exact name to hand `get_symbol` isn't known yet. It does not resolve call relationships. Real response, `find("resolve", limit=5)` against this repo — 14 symbols match, 5 are returned:

```json
{
  "results": [
    { "name": "resolve_all", "file": "crates/corbel-core/src/resolve.rs", "line": 27, "kind": "function" },
    { "name": "resolve_repo_path", "file": "benchmarks/harness/run_benchmark.py", "line": 30, "kind": "function" }
  ],
  "total_matches": 14,
  "truncated": true,
  "truncated_count": 9
}
```

## Supported languages

| Language | Level | Notes |
| --- | --- | --- |
| Rust | full | Own scope walker; all five resolution stages exercised. |
| Python | full | Own scope walker; all five resolution stages exercised. |
| TypeScript | full | Own scope walker; all five resolution stages exercised. |
| TSX | full | Adds JSX-tag references on top of TypeScript's resolution. |
| JavaScript | full | Shares TypeScript's resolution machinery. CommonJS `require(...)` produces no import entry — only ES-module `import`/`export` is scope-aware. |

Every language above runs through the same five-stage resolution chain, implemented once and shared by all of them: **same-file → scoped → global-unique → external → unresolved**. See [docs/language-support.md](docs/language-support.md) for the promotion criteria new languages must clear.

## Known limitations

corbel resolves what static analysis can prove and refuses to guess at the rest. On its own source (661 symbols, 4713 references at time of writing), 92.5% of internal calls resolve; the remaining 7.5% are calls where more than one definition shares a name and nothing in scope disambiguates them, so corbel marks them `unresolved (ambiguous)` rather than picking one.

Cases that are structurally out of reach for static analysis, by design, in every supported language:

- **Dynamic dispatch** — trait objects (Rust), duck-typed calls (Python), calls through an interface-typed value (TypeScript) — has no statically-determined target. corbel reports these as `external` or `unresolved`, never a fabricated edge.
- **Macro-generated code** (Rust `macro_rules!`/derive output, decorators that rewrite call sites) is invisible to corbel's tree-sitter-based extraction if the macro expansion isn't present in source form.
- **JavaScript/TypeScript CommonJS `require(...)`** doesn't populate an import entry, so a call reached only through a `require`-bound name can resolve less precisely than the same call reached through `import`.
- **`find`'s substring query** (`%query%`) cannot use the symbol-name index — every call does a full scan of the `symbols` table. Fine at the scale corbel targets today; the slowest of the three tools on a very large index.
- **Standard library and external crate/package calls** are outside the index entirely and reported as `external` — corbel does not resolve into dependencies.

## License and boundaries

corbel is licensed under [MIT](LICENSE).

Indexing and querying your own codebase — the entire tool as it exists today — is and will remain free for individual use, with no license server, no telemetry, and no phone-home behavior, ever. Organization-level features (fleet-wide indexing, shared indexes, team administration) are the intended boundary for a future commercial offering; nothing in the current codebase is gated, and this line is drawn now, before any such feature exists, rather than moved after the fact.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow, the language-promotion gates, and the schema-migration rules.

## Privacy

corbel never sends your code anywhere. Indexing and querying run entirely offline; the binary contains no network code. What an agent sends to its model is between the agent and its MCP client — corbel itself never touches the network.

## Non-goals

corbel does not edit code, generate documentation, ship a web UI, read git history, scan for secrets, integrate with the Language Server Protocol, or collect telemetry.
