# corbel

*A corbel (/ˈkɔːrbəl/) is the bracket built into a wall that carries the load above it. corbel maps what carries what in your code.*

corbel is a local MCP server that gives coding agents an accurate call graph of your codebase. Its core question is "if I change this function, what breaks?" — corbel answers it by walking the reverse call graph across multiple hops, something a text search can't do even in principle, since it has no notion of what actually calls what. It ships as a single static binary with no runtime dependencies, and your code never leaves the machine.

## See it work

Point Claude Code (or any MCP client) at a corbel-indexed repo and ask a refactoring question in plain language:

> "If I change `resolve_all`, what else needs to change?"

The agent calls corbel's `impact` tool, which walks the reverse call graph from `resolve_all` and returns every affected symbol with its hop count (`depth`) and how the call edge was resolved:

```json
{
  "affected": [
    {
      "depth": 1,
      "file": "crates/corbel-core/src/index.rs",
      "line": 25,
      "name": "index_repo",
      "resolution": "scoped"
    },
    {
      "depth": 2,
      "file": "crates/corbel/src/mcp/server.rs",
      "line": 164,
      "name": "indexed_conn",
      "resolution": "scoped"
    },
    {
      "depth": 3,
      "file": "crates/corbel/src/mcp/server.rs",
      "line": 285,
      "name": "impact_call_returns_affected_symbols_with_truncated_field",
      "resolution": "same-file"
    }
  ],
  "affected_count": 51,
  "max_depth_reached": 3,
  "target_name": "resolve_all",
  "truncated": false,
  "truncated_count": 0
}
```

This is the actual response from running corbel against its own repository. `index_repo` calls `resolve_all` directly (depth 1); `indexed_conn` calls `index_repo` (depth 2); and so on out to depth 3, where the trail runs into tests exercising the whole chain. 51 symbols are affected in total, and `truncated: false` means nothing was cut to fit the token budget. No hop in that chain is a coincidental name match — every edge is grounded in a resolved reference.

## Install

```
cargo install corbel
```

Pre-built binaries will be added here once releases start shipping.

## Usage

```
corbel index .
corbel serve
```

`corbel index` walks the repository, parses every supported file, and resolves each call to a specific definition. Running it against corbel's own source produces:

```
Indexed 61 files (0 unchanged)
486 symbols, 3110 references

Internal calls: 664 resolved / 728 total (91.2%)
  same-file: 429, scoped: 115, global-unique: 120
  unresolved (ambiguous): 64
External calls: 2382 (std, crates, dynamic dispatch)
Note: 64 internal call(s) could not be resolved because multiple definitions share the same name.
```

corbel reports its own resolution quality rather than hiding it: the internal-resolution percentage and the unresolved count tell you how much of the call graph it could actually pin down for this codebase. `corbel serve` starts the MCP server over stdio, reading from whatever index is already on disk.

## MCP setup

With the Claude Code CLI:

```
claude mcp add corbel -- /path/to/corbel serve
```

For other MCP clients, add corbel to the server configuration directly:

```json
{
  "servers": {
    "corbel": {
      "command": "corbel",
      "args": ["serve"]
    }
  }
}
```

Once connected, the agent calls `get_symbol` and `impact` on its own whenever a question implies them — no special syntax needed from you.

## Tools

| Tool | What it does |
| --- | --- |
| `get_symbol` | Looks up a symbol by name and returns its definition (file, line, signature), its callers, and its callees. Every caller/callee edge carries a `resolution` field explaining how it was resolved. |
| `impact` | Traces a symbol's blast radius: walks the reverse call graph from the symbol across multiple hops and returns every affected symbol, tagged with `depth` and `resolution`. Truncates to a token budget and reports `truncated`/`truncated_count` when it does. |

## Honest limits

A single lookup — "where is this function defined" — doesn't need corbel; ripgrep answers that as well as anything. corbel earns its keep on multi-hop tracing (`impact`) and on telling apart same-named symbols in different files, which text search cannot do.

Some calls are outside what static analysis can resolve at all:

- **Dynamic dispatch** (trait objects, virtual calls) and **macro-generated code** can't be resolved statically. corbel doesn't guess at these — it marks them `unresolved` or `external` rather than fabricating a call edge.
- **Standard library and external crate calls** are outside the index entirely and are reported as `external`; corbel doesn't attempt to resolve into dependencies.

This isn't a rough edge to be polished away — it's the design principle: corbel would rather tell you it doesn't know than guess and be wrong.

## Supported languages

Rust, Python, TypeScript, TSX, JavaScript.

See [docs/language-support.md](docs/language-support.md) for the criteria a language has to clear, and which languages are queued for it.

## Privacy

corbel never sends your code anywhere. Indexing and querying run entirely offline, with no network code in the binary. What an agent sends to its model is between the agent and its MCP client — corbel itself never touches the network.

## Non-goals

corbel does not edit code, generate documentation, ship a web UI, read git history, scan for secrets, integrate with the Language Server Protocol, or collect telemetry.

## License

MIT
