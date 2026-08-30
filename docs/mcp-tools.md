# MCP tools

corbel exposes three tools over MCP. Every `get_symbol` and `impact` response
includes a `resolution` field describing how references were resolved and a
`truncated` field indicating whether the response was cut to fit the output
budget. `find` is a name search, not a call-graph query, and its response
shape is documented separately below.

## Transport rules

`stdout` carries the MCP protocol exclusively. All logs and diagnostics are
written to `stderr`.

## get_symbol

Return the definition and metadata for a single symbol, plus everything that
calls it (callers) and everything it calls (callees).

| Input          | Type   | Required | Description                        |
| -------------- | ------ | -------- | ---------------------------------- |
| `name`         | string | yes      | Symbol name to look up.            |
| `file`         | string | no       | File to disambiguate the symbol.   |
| `line`         | number | no       | Definition line to disambiguate further, for when `name` and `file` alone still match more than one symbol (e.g. overloaded declarations in the same file). Requires `file` to also be set. |
| `token_budget` | number | no       | Cap on the size of the response, in estimated tokens, split evenly between callers and callees so a large caller list can't crowd out callees or vice versa. Defaults to corbel's built-in budget. |

| Response field | Description                                                    |
| -------------- | ---------------------------------------------------------------- |
| `query`        | The symbol name that was looked up.                               |
| `found`        | Whether `name` (as narrowed by `file`/`line`) matched any symbol. |
| `count`        | Number of matched symbols in `results` (`name` alone can match more than one, e.g. overloaded declarations in different files). |
| `results`      | Matched symbols, each described below.                            |
| `message`      | Present only when `found` is `false`: "no symbol named ... found in the index". |

Each entry in `results` is:

| Field             | Description                                                  |
| ----------------- | -------------------------------------------------------------- |
| `name`, `file`, `line`, `kind`, `signature`, `is_public` | The symbol's own definition metadata. |
| `callers`         | Symbols that call this one. Each entry has `name`, `file`, `line` (the caller's own definition line, not the call site), and `resolution` (how corbel resolved that reference, e.g. same-file, scoped, global-unique). |
| `callees`         | Symbols this one calls. Each entry has `name`, `file` (`null` if unresolved or external), and `resolution`. |
| `truncated`       | Whether `callers` and/or `callees` were cut to fit the token budget for this result. |
| `truncated_count` | How many caller and callee entries together were left out.   |

## impact

Return the symbols affected by a change to a given symbol: the reverse call
graph, walked across multiple hops.

| Input          | Type   | Required | Description                       |
| -------------- | ------ | -------- | --------------------------------- |
| `name`         | string | yes      | Symbol to analyze.                |
| `file`         | string | no       | File to disambiguate the symbol.  |
| `token_budget` | number | no       | Cap on the size of the response, in estimated tokens. Defaults to corbel's built-in budget. |

| Response field | Description                                                    |
| -------------- | ---------------------------------------------------------------- |
| `query`        | The symbol name the impact analysis started from.                 |
| `found`        | Whether `name` (as narrowed by `file`) matched any symbol.        |
| `count`        | Number of matched symbols in `results` (usually `1` unless `name` is ambiguous without `file`). |
| `results`      | One impact analysis per matched symbol, each described below.     |
| `message`      | Present only when `found` is `false`: "no symbol named ... found in the index". |

Each entry in `results` is:

| Field               | Description                                                  |
| ------------------- | -------------------------------------------------------------- |
| `target_name`, `target_file`, `target_line` | The symbol the analysis started from.       |
| `affected`           | Every symbol reachable by walking callers outward from the target. Each entry has `name`, `file`, `line`, `resolution`, and `depth` (how many hops away it is). |
| `affected_count`     | Number of entries in `affected`.                              |
| `max_depth_reached`  | The largest `depth` value present in `affected`.               |
| `truncated`          | Whether `affected` was cut to fit the token budget.            |
| `truncated_count`    | How many further affected symbols were left out.               |

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

**Known limitation:** the query's `%query%` pattern (leading wildcard, for
substring matching) cannot use the `idx_symbols_name` index — both the
`COUNT(*)` and the main `SELECT` fall back to a full scan of the `symbols`
table on every call. This is fine at the scale corbel is built for today, but
on a very large repository's index it will be the slowest of the three
tools. Not fixed as part of this change.

| Input          | Type   | Required | Description                                             |
| -------------- | ------ | -------- | -------------------------------------------------------- |
| `query`        | string | yes      | Substring to search for in symbol names.                 |
| `limit`        | number | no       | Maximum number of matches to return, from `0` up to corbel's hard maximum of 200. Requests above 200 are rejected with an error rather than silently reduced. Defaults to corbel's built-in limit. |
| `token_budget` | number | no       | Cap on the size of the response, in estimated tokens. Defaults to corbel's built-in budget. |

| Response field   | Description                                                    |
| ---------------- | ---------------------------------------------------------------- |
| `query`          | The substring that was searched for.                             |
| `found`          | Whether `total_matches` is greater than zero. Note this can be `true` even when `results` is empty: if every match was cut by the token budget, `found` still reflects that matches exist, and `message` explains why none are shown (see below). |
| `count`          | Number of matches actually included in `results` (`0` when the token budget was too small to include any). |
| `results`        | Matching symbols (`name`, `file`, `line`, `kind`, `signature`, `is_public`). |
| `total_matches`  | How many symbols in the index matched the query, before `limit` or the token budget were applied. |
| `truncated`      | Whether `results` is fewer than `total_matches`.                 |
| `truncated_count`| How many matches were left out of `results`.                     |
| `message`        | Present only when `results` is empty. Reads "no symbol matching ... found in the index" when `total_matches` is `0`, or "N symbol(s) matched ... but none fit within the token budget" when matches exist but the budget excluded all of them. |
