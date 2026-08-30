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
| `line`  | number | no       | Definition line to disambiguate further, for when `name` and `file` alone still match more than one symbol (e.g. overloaded declarations in the same file). Requires `file` to also be set. |

| Response field | Description                                  |
| -------------- | -------------------------------------------- |
| `symbol`       | The matched symbol record.                   |
| `resolution`   | How the symbol was resolved.                 |
| `truncated`    | Whether the response was truncated.          |

## find

Search for symbols by a substring of their name, for when the exact name to
pass to `get_symbol` isn't known. Matching is case-insensitive and ranked so
exact name matches sort before names starting with the query, which sort
before names that merely contain it; ties within a rank are ordered by name,
then file, then line. Each match reports the `name`/`file`/`line` triple that
`get_symbol` needs to pin down that exact symbol, even when other symbols
elsewhere share its name.

Case-insensitivity only folds ASCII letters (SQLite's default `LIKE`
behavior). It does not fold case for non-ASCII Unicode identifiers, so a
query against a Python or JavaScript identifier that uses non-ASCII letters
must match that identifier's actual case.

`find` does not resolve or return call relationships — it is a name search
over the `symbols` table, not a call-graph query. Use `get_symbol` or
`impact` on a specific match for that.

| Input          | Type   | Required | Description                                             |
| -------------- | ------ | -------- | -------------------------------------------------------- |
| `query`        | string | yes      | Substring to search for in symbol names.                 |
| `limit`        | number | no       | Maximum number of matches to return. Defaults to corbel's built-in limit. |
| `token_budget` | number | no       | Cap on the size of the response, in estimated tokens. Defaults to corbel's built-in budget. |

| Response field   | Description                                                    |
| ---------------- | ---------------------------------------------------------------- |
| `results`        | Matching symbols (`name`, `file`, `line`, `kind`, `signature`, `is_public`). |
| `total_matches`  | How many symbols in the index matched the query, before `limit` or the token budget were applied. |
| `truncated`      | Whether `results` is fewer than `total_matches`.                 |
| `truncated_count`| How many matches were left out of `results`.                     |

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
