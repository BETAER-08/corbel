# corbel

*A corbel (/ˈkɔːrbəl/, KOR-bəl) is the bracket built into a wall that carries the load above it. corbel maps what carries what in your code.*

MCP server that gives coding agents an accurate call graph of your codebase. Single static binary, no runtime dependencies, no network code.

## Status

Pre-alpha. Nothing works yet.

## Install

```
cargo install corbel
```

## Usage

```
corbel index .
corbel serve
```

## MCP setup

Add corbel to your MCP client configuration:

```json
{
  "mcpServers": {
    "corbel": {
      "command": "corbel",
      "args": ["serve"]
    }
  }
}
```

With the Claude CLI:

```
claude mcp add corbel -- corbel serve
```

## Supported languages

Rust, Python, TypeScript, JavaScript, TSX.

## Non-goals

corbel does not edit code, generate documentation, ship a web UI, read git history, scan for secrets, integrate with the Language Server Protocol, or collect telemetry.

## Privacy

corbel never sends your code anywhere. A one-time embedding model download is
required on first run; after that, indexing and queries use no network.
What reaches your model is decided by your MCP client.

## License

MIT
