# MCP tools

corbel exposes three tools over MCP. Every response includes a `resolution`
field describing how references were resolved and a `truncated` field
indicating whether the response was cut to fit the output budget.

## Transport rules

`stdout` carries the MCP protocol exclusively. All logs and diagnostics are
written to `stderr`.

## get_symbol

Return the definition and metadata for a single symbol.

| Input   | Type   | Required | Description                        |
| ------- | ------ | -------- | ---------------------------------- |
| `name`  | string | yes      | Symbol name to look up.            |
| `path`  | string | no       | File to disambiguate the symbol.   |

| Response field | Description                                  |
| -------------- | -------------------------------------------- |
| `symbol`       | The matched symbol record.                   |
| `resolution`   | How the symbol was resolved.                 |
| `truncated`    | Whether the response was truncated.          |

## find

Search for symbols matching a query.

| Input    | Type   | Required | Description                       |
| -------- | ------ | -------- | --------------------------------- |
| `query`  | string | yes      | Search query.                     |
| `limit`  | number | no       | Maximum number of results.        |

| Response field | Description                                  |
| -------------- | -------------------------------------------- |
| `results`      | Matching symbols.                            |
| `resolution`   | How the results were resolved.               |
| `truncated`    | Whether the response was truncated.          |

## impact

Return the symbols affected by a change to a given symbol.

| Input    | Type   | Required | Description                       |
| -------- | ------ | -------- | --------------------------------- |
| `name`   | string | yes      | Symbol to analyze.                |
| `path`   | string | no       | File to disambiguate the symbol.  |

| Response field | Description                                  |
| -------------- | -------------------------------------------- |
| `callers`      | Symbols that depend on the target.           |
| `resolution`   | How the callers were resolved.               |
| `truncated`    | Whether the response was truncated.          |
