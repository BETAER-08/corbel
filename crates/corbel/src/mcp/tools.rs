use rusqlite::Connection;
use serde_json::{Value, json};

use corbel_core::budget::{
    DEFAULT_FIND_TOKEN_BUDGET, DEFAULT_GET_SYMBOL_TOKEN_BUDGET, DEFAULT_IMPACT_TOKEN_BUDGET,
    TokenBudget,
};
use corbel_core::query::{
    self, CalleeInfo, CallerInfo, DEFAULT_FIND_LIMIT, FindMatch, FindResult, ImpactResult,
    MAX_FIND_LIMIT, SymbolResult,
};

pub enum ToolCallError {
    InvalidParams(String),
    Internal(String),
}

pub fn list_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "get_symbol",
            "description": "Look up a single symbol by name in the local corbel index and return where it is defined (file, line, signature), everything that calls it (callers), and everything it calls (callees). Every caller and callee comes with a `resolution` field showing exactly how corbel resolved that reference to a specific definition (e.g. same-file, scoped, global-unique) rather than a guess from text matching — this is what makes the result trustworthy for navigation and refactoring, unlike a grep/text search which can't tell you if a match is actually the same symbol. Use this tool when you need to jump to a function's or type's definition, inspect its signature, or see who calls it and what it calls, before editing it. If `name` (optionally narrowed by `file`) still matches more than one symbol — e.g. overloaded declarations in the same file — pass `line` as well; the `find` tool's results already carry the exact `name`/`file`/`line` triple needed to pin down one match. The response can be truncated to fit within a token budget; when it is, `truncated` is set to true and `truncated_count` reports how many additional callers and callees together were left out. The budget is divided evenly across every matched symbol first (so if `name` is ambiguous and returns several results, no single match can consume the whole budget and starve the others), then each match's own share is split evenly between its callers and its callees (so a hot function's huge caller list can't crowd out its callees, or vice versa). The index is built ahead of time by `corbel index` and only reflects the state of the repository as of the last index run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The symbol name to look up (function, method, type, etc.)."
                    },
                    "file": {
                        "type": "string",
                        "description": "Optional file path to disambiguate when multiple symbols share this name."
                    },
                    "line": {
                        "type": "number",
                        "description": "Optional definition line to disambiguate further, for when `name` and `file` alone still match more than one symbol (e.g. overloaded declarations in the same file). Requires `file` to also be set."
                    },
                    "token_budget": {
                        "type": "number",
                        "description": "Optional cap on the size of the response, in estimated tokens. Divided evenly across every matched symbol first, then each match's share is split evenly between its callers and callees. Defaults to corbel's built-in budget if omitted."
                    }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "impact",
            "description": "Trace the blast radius of changing a symbol: starting from the given symbol, walk the reverse call graph — direct callers, their callers, and so on across multiple hops — and return every symbol that could be affected by a change to it. Each affected symbol comes with a `depth` field (how many hops away it is) and a `resolution` field showing how corbel resolved that call edge, so results are grounded in real, resolved call relationships rather than a text search for the symbol's name (which cannot follow more than one hop and cannot tell a real call from a coincidental name match). Use this tool before refactoring — e.g. changing a function's signature or behavior — to find every place in the codebase that may need to change as a result, including indirect callers that a single-hop \"find references\" would miss. The response can be truncated to fit within a token budget; when it is, `truncated` is set to true and `truncated_count` reports how many additional affected symbols were left out. Results come from the local corbel index built by `corbel index` and only reflect the repository as of the last index run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The symbol name to start the impact analysis from."
                    },
                    "file": {
                        "type": "string",
                        "description": "Optional file path to disambiguate when multiple symbols share this name."
                    },
                    "token_budget": {
                        "type": "number",
                        "description": "Optional cap on the size of the response, in estimated tokens. Defaults to corbel's built-in budget if omitted."
                    }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "find",
            "description": "Search the local corbel index for symbols whose name matches a query, for when you don't know the exact symbol name to pass to `get_symbol`. Matching is substring-based and case-insensitive, ranked so exact name matches come first, then names starting with the query, then names merely containing it; within each of those tiers results are ordered by name, then file, then line for a stable, repeatable order. Case-insensitivity only folds ASCII letters (SQLite's default `LIKE` behavior) — it will not match differently-cased Unicode identifiers (e.g. a Python or JavaScript identifier using non-ASCII letters), so queries against such names must match the identifier's actual case. Each match reports `name`, `file`, and `line`, which together are the exact triple `get_symbol` needs to pin down that one symbol even when several symbols elsewhere share its name. This tool does not resolve or return call relationships — use `get_symbol` or `impact` on a specific match for that. The response can be truncated to fit within `limit` and a token budget; when it is, `truncated` is set to true and `truncated_count` reports how many further matches were left out. Results come from the local corbel index built by `corbel index` and only reflect the repository as of the last index run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Substring to search for in symbol names (case-insensitive for ASCII)."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Optional cap on the number of matches returned, from 0 up to corbel's hard maximum of 200 (requests above 200 are rejected, not silently reduced). Defaults to corbel's built-in limit if omitted."
                    },
                    "token_budget": {
                        "type": "number",
                        "description": "Optional cap on the size of the response, in estimated tokens. Defaults to corbel's built-in budget if omitted."
                    }
                },
                "required": ["query"]
            }
        }),
    ]
}

fn required_string(arguments: &Value, field: &str) -> Result<String, ToolCallError> {
    match arguments.get(field).and_then(Value::as_str) {
        Some(value) if !value.is_empty() => Ok(value.to_string()),
        Some(_) => Err(ToolCallError::InvalidParams(format!(
            "argument \"{field}\" must not be empty"
        ))),
        None => Err(ToolCallError::InvalidParams(format!(
            "missing required argument \"{field}\""
        ))),
    }
}

fn optional_string(arguments: &Value, field: &str) -> Result<Option<String>, ToolCallError> {
    match arguments.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ToolCallError::InvalidParams(format!(
            "argument \"{field}\" must be a string"
        ))),
    }
}

fn optional_token_budget(arguments: &Value, default: usize) -> Result<usize, ToolCallError> {
    match arguments.get("token_budget") {
        None | Some(Value::Null) => Ok(default),
        Some(value) => match value.as_u64() {
            Some(budget) => Ok(budget as usize),
            None => Err(ToolCallError::InvalidParams(
                "argument \"token_budget\" must be a non-negative number".to_string(),
            )),
        },
    }
}

fn optional_line(arguments: &Value) -> Result<Option<u32>, ToolCallError> {
    match arguments.get("line") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_u64().and_then(|line| u32::try_from(line).ok()) {
            Some(line) => Ok(Some(line)),
            None => Err(ToolCallError::InvalidParams(
                "argument \"line\" must be a non-negative number".to_string(),
            )),
        },
    }
}

fn optional_limit(arguments: &Value) -> Result<usize, ToolCallError> {
    match arguments.get("limit") {
        None | Some(Value::Null) => Ok(DEFAULT_FIND_LIMIT),
        Some(value) => match value.as_u64() {
            Some(limit) if limit > MAX_FIND_LIMIT as u64 => Err(ToolCallError::InvalidParams(
                format!("argument \"limit\" must not exceed {MAX_FIND_LIMIT}"),
            )),
            Some(limit) => Ok(limit as usize),
            None => Err(ToolCallError::InvalidParams(
                "argument \"limit\" must be a non-negative number".to_string(),
            )),
        },
    }
}

fn caller_json(caller: &CallerInfo) -> Value {
    json!({
        "name": caller.name,
        "file": caller.file,
        "line": caller.line,
        "resolution": caller.resolution,
    })
}

fn callee_json(callee: &CalleeInfo) -> Value {
    json!({
        "name": callee.name,
        "file": callee.file,
        "resolution": callee.resolution,
    })
}

fn find_match_json(found: &FindMatch) -> Value {
    json!({
        "name": found.name,
        "file": found.file,
        "line": found.line,
        "kind": found.kind,
        "signature": found.signature,
        "is_public": found.is_public,
    })
}

fn tool_text_response(payload: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": payload.to_string()
            }
        ]
    })
}

fn get_symbol_payload(name: &str, results: &[SymbolResult]) -> Value {
    if results.is_empty() {
        return json!({
            "query": name,
            "found": false,
            "count": 0,
            "results": [],
            "message": format!("no symbol named \"{name}\" found in the index"),
        });
    }

    let results_json: Vec<Value> = results
        .iter()
        .map(|result| {
            json!({
                "name": result.symbol.name,
                "file": result.symbol.file,
                "line": result.symbol.line,
                "kind": result.symbol.kind,
                "signature": result.symbol.signature,
                "is_public": result.symbol.is_public,
                "callers": result.callers.iter().map(caller_json).collect::<Vec<_>>(),
                "callees": result.callees.iter().map(callee_json).collect::<Vec<_>>(),
                "truncated": result.truncated,
                "truncated_count": result.truncated_count,
            })
        })
        .collect();

    json!({
        "query": name,
        "found": true,
        "count": results_json.len(),
        "results": results_json,
    })
}

fn impact_payload(name: &str, results: &[ImpactResult]) -> Value {
    if results.is_empty() {
        return json!({
            "query": name,
            "found": false,
            "count": 0,
            "results": [],
            "message": format!("no symbol named \"{name}\" found in the index"),
        });
    }

    let results_json: Vec<Value> = results
        .iter()
        .map(|result| {
            json!({
                "target_name": result.target.name,
                "target_file": result.target.file,
                "target_line": result.target.line,
                "truncated": result.truncated,
                "truncated_count": result.truncated_count,
                "max_depth_reached": result.max_depth_reached,
                "affected_count": result.affected.len(),
                "affected": result.affected.iter().map(|node| json!({
                    "name": node.name,
                    "file": node.file,
                    "line": node.line,
                    "resolution": node.resolution,
                    "depth": node.depth,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "query": name,
        "found": true,
        "count": results_json.len(),
        "results": results_json,
    })
}

fn find_payload(query: &str, result: &FindResult) -> Value {
    if result.total_matches == 0 {
        return json!({
            "query": query,
            "found": false,
            "count": 0,
            "total_matches": 0,
            "truncated": result.truncated,
            "truncated_count": result.truncated_count,
            "results": [],
            "message": format!("no symbol matching \"{query}\" found in the index"),
        });
    }

    if result.matches.is_empty() {
        return json!({
            "query": query,
            "found": true,
            "count": 0,
            "total_matches": result.total_matches,
            "truncated": result.truncated,
            "truncated_count": result.truncated_count,
            "results": [],
            "message": format!(
                "{} symbol(s) matched \"{query}\" but none fit within the token budget; increase token_budget to see them",
                result.total_matches
            ),
        });
    }

    json!({
        "query": query,
        "found": true,
        "count": result.matches.len(),
        "total_matches": result.total_matches,
        "truncated": result.truncated,
        "truncated_count": result.truncated_count,
        "results": result.matches.iter().map(find_match_json).collect::<Vec<_>>(),
    })
}

pub fn call_get_symbol(conn: &Connection, arguments: &Value) -> Result<Value, ToolCallError> {
    let name = required_string(arguments, "name")?;
    let file = optional_string(arguments, "file")?;
    let line = optional_line(arguments)?;
    let token_budget = optional_token_budget(arguments, DEFAULT_GET_SYMBOL_TOKEN_BUDGET)?;

    if line.is_some() && file.is_none() {
        return Err(ToolCallError::InvalidParams(
            "argument \"line\" requires \"file\" to also be set".to_string(),
        ));
    }

    let results = query::get_symbol(
        conn,
        &name,
        file.as_deref(),
        line,
        TokenBudget::new(token_budget),
    )
    .map_err(|err| ToolCallError::Internal(err.to_string()))?;

    Ok(tool_text_response(get_symbol_payload(&name, &results)))
}

pub fn call_impact(conn: &Connection, arguments: &Value) -> Result<Value, ToolCallError> {
    let name = required_string(arguments, "name")?;
    let file = optional_string(arguments, "file")?;
    let token_budget = optional_token_budget(arguments, DEFAULT_IMPACT_TOKEN_BUDGET)?;

    let results = query::impact(conn, &name, file.as_deref(), TokenBudget::new(token_budget))
        .map_err(|err| ToolCallError::Internal(err.to_string()))?;

    Ok(tool_text_response(impact_payload(&name, &results)))
}

pub fn call_find(conn: &Connection, arguments: &Value) -> Result<Value, ToolCallError> {
    let query = required_string(arguments, "query")?;
    let limit = optional_limit(arguments)?;
    let token_budget = optional_token_budget(arguments, DEFAULT_FIND_TOKEN_BUDGET)?;

    let result = query::find(conn, &query, limit, TokenBudget::new(token_budget))
        .map_err(|err| ToolCallError::Internal(err.to_string()))?;

    Ok(tool_text_response(find_payload(&query, &result)))
}
