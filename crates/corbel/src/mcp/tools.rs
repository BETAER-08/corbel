use rusqlite::Connection;
use serde_json::{Value, json};

use corbel_core::budget::{DEFAULT_IMPACT_TOKEN_BUDGET, TokenBudget};
use corbel_core::query::{self, CalleeInfo, CallerInfo, ImpactResult, SymbolResult};

pub enum ToolCallError {
    InvalidParams(String),
    Internal(String),
}

pub fn list_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "get_symbol",
            "description": "Look up a single symbol by name in the local corbel index and return where it is defined (file, line, signature), everything that calls it (callers), and everything it calls (callees). Every caller and callee comes with a `resolution` field showing exactly how corbel resolved that reference to a specific definition (e.g. same-file, scoped, global-unique) rather than a guess from text matching — this is what makes the result trustworthy for navigation and refactoring, unlike a grep/text search which can't tell you if a match is actually the same symbol. Use this tool when you need to jump to a function's or type's definition, inspect its signature, or see who calls it and what it calls, before editing it. The index is built ahead of time by `corbel index` and only reflects the state of the repository as of the last index run.",
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

fn optional_token_budget(arguments: &Value) -> Result<usize, ToolCallError> {
    match arguments.get("token_budget") {
        None | Some(Value::Null) => Ok(DEFAULT_IMPACT_TOKEN_BUDGET),
        Some(value) => match value.as_u64() {
            Some(budget) => Ok(budget as usize),
            None => Err(ToolCallError::InvalidParams(
                "argument \"token_budget\" must be a non-negative number".to_string(),
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

pub fn call_get_symbol(conn: &Connection, arguments: &Value) -> Result<Value, ToolCallError> {
    let name = required_string(arguments, "name")?;
    let file = optional_string(arguments, "file")?;

    let results = query::get_symbol(conn, &name, file.as_deref())
        .map_err(|err| ToolCallError::Internal(err.to_string()))?;

    Ok(tool_text_response(get_symbol_payload(&name, &results)))
}

pub fn call_impact(conn: &Connection, arguments: &Value) -> Result<Value, ToolCallError> {
    let name = required_string(arguments, "name")?;
    let file = optional_string(arguments, "file")?;
    let token_budget = optional_token_budget(arguments)?;

    let results = query::impact(conn, &name, file.as_deref(), TokenBudget::new(token_budget))
        .map_err(|err| ToolCallError::Internal(err.to_string()))?;

    Ok(tool_text_response(impact_payload(&name, &results)))
}
