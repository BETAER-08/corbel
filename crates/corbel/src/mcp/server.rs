use std::io::{BufRead, Write};

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::mcp::tools::{ToolCallError, call_find, call_get_symbol, call_impact, list_tools};

const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";

pub struct McpServer {
    conn: Connection,
}

impl McpServer {
    pub fn new(conn: Connection) -> Self {
        McpServer { conn }
    }

    pub fn run(&self, input: &mut dyn BufRead, output: &mut dyn Write) -> anyhow::Result<()> {
        tracing::info!("mcp server ready, reading requests from stdin");
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = input.read_line(&mut line)?;
            if bytes_read == 0 {
                tracing::debug!("stdin closed, shutting down");
                return Ok(());
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            tracing::debug!(request = trimmed, "received mcp request");

            if let Some(response) = handle_line(trimmed, &self.conn) {
                tracing::debug!(response = %response, "sending mcp response");
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
    }
}

fn handle_line(line: &str, conn: &Connection) -> Option<String> {
    let parsed: Result<Value, _> = serde_json::from_str(line);
    let value = match parsed {
        Ok(value) => value,
        Err(_) => return Some(error_response(Value::Null, -32700, "Parse error")),
    };

    let Some(object) = value.as_object() else {
        return Some(error_response(Value::Null, -32600, "Invalid Request"));
    };

    let has_id = object.contains_key("id");
    let id = object.get("id").cloned().unwrap_or(Value::Null);

    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return if has_id {
            Some(error_response(id, -32600, "Invalid Request"))
        } else {
            None
        };
    };

    let params = object.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => Some(success_response(id, handle_initialize(&params))),
        "notifications/initialized" => None,
        "tools/list" => Some(success_response(id, json!({ "tools": list_tools() }))),
        "tools/call" => Some(handle_tools_call(id, &params, conn)),
        _ => {
            if has_id {
                Some(error_response(
                    id,
                    -32601,
                    &format!("Method not found: {method}"),
                ))
            } else {
                None
            }
        }
    }
}

fn handle_tools_call(id: Value, params: &Value, conn: &Connection) -> String {
    let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match tool_name {
        "get_symbol" => call_get_symbol(conn, &arguments),
        "impact" => call_impact(conn, &arguments),
        "find" => call_find(conn, &arguments),
        _ => {
            return error_response(id, -32602, &format!("Unknown tool: {tool_name}"));
        }
    };

    match result {
        Ok(payload) => success_response(id, payload),
        Err(ToolCallError::InvalidParams(message)) => error_response(id, -32602, &message),
        Err(ToolCallError::Internal(message)) => error_response(id, -32603, &message),
    }
}

fn handle_initialize(params: &Value) -> Value {
    let requested_version = params.get("protocolVersion").and_then(Value::as_str);
    let negotiated_version = match requested_version {
        Some(version) if SUPPORTED_PROTOCOL_VERSIONS.contains(&version) => version,
        _ => LATEST_PROTOCOL_VERSION,
    };

    json!({
        "protocolVersion": negotiated_version,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "corbel",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn success_response(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
    .to_string()
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use corbel_core::index::index_repo;
    use corbel_core::path::RepoRoot;
    use corbel_core::store::migrate::open_connection;
    use corbel_lang::langs::rust::RustSupport;
    use corbel_lang::registry::LanguageRegistry;
    use std::fs;
    use tempfile::tempdir;

    fn empty_conn() -> Connection {
        open_connection(":memory:").unwrap()
    }

    fn indexed_conn() -> Connection {
        let repo_dir = tempdir().unwrap();
        fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
        fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {}\n").unwrap();

        let root = RepoRoot::new(repo_dir.path()).unwrap();
        let conn = open_connection(":memory:").unwrap();
        let mut registry = LanguageRegistry::new();
        registry.register(Box::new(RustSupport)).unwrap();
        index_repo(&root, &conn, &registry).unwrap();
        conn
    }

    fn indexed_conn_with_method_caller() -> Connection {
        let repo_dir = tempdir().unwrap();
        fs::write(
            repo_dir.path().join("lib.rs"),
            b"pub fn helper() {}\n\nstruct Signer;\n\nimpl Signer {\n    pub fn unsign(&self) {\n        helper();\n    }\n}\n",
        )
        .unwrap();

        let root = RepoRoot::new(repo_dir.path()).unwrap();
        let conn = open_connection(":memory:").unwrap();
        let mut registry = LanguageRegistry::new();
        registry.register(Box::new(RustSupport)).unwrap();
        index_repo(&root, &conn, &registry).unwrap();
        conn
    }

    fn indexed_conn_with_duplicate_name_in_same_file() -> Connection {
        let repo_dir = tempdir().unwrap();
        fs::write(
            repo_dir.path().join("overload.rs"),
            b"pub fn widget(x: i32) -> i32 {\n    x\n}\n\npub fn widget(x: i32, y: i32) -> i32 {\n    x + y\n}\n",
        )
        .unwrap();

        let root = RepoRoot::new(repo_dir.path()).unwrap();
        let conn = open_connection(":memory:").unwrap();
        let mut registry = LanguageRegistry::new();
        registry.register(Box::new(RustSupport)).unwrap();
        index_repo(&root, &conn, &registry).unwrap();
        conn
    }

    #[test]
    fn initialize_negotiates_supported_client_version() {
        let params = json!({ "protocolVersion": "2024-11-05" });
        let result = handle_initialize(&params);
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "corbel");
    }

    #[test]
    fn initialize_falls_back_to_latest_for_unsupported_client_version() {
        let params = json!({ "protocolVersion": "1999-01-01" });
        let result = handle_initialize(&params);
        assert_eq!(result["protocolVersion"], LATEST_PROTOCOL_VERSION);
    }

    #[test]
    fn initialize_falls_back_to_latest_when_version_missing() {
        let params = json!({});
        let result = handle_initialize(&params);
        assert_eq!(result["protocolVersion"], LATEST_PROTOCOL_VERSION);
    }

    #[test]
    fn tools_list_returns_get_symbol_impact_and_find() {
        let conn = empty_conn();
        let response =
            handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, &conn).unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let tools = parsed["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["get_symbol", "impact", "find"]);
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let conn = empty_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"does/not/exist"}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32601);
    }

    #[test]
    fn unknown_tool_call_returns_error() {
        let conn = empty_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ghost"}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32602);
    }

    #[test]
    fn get_symbol_call_reports_owner_qualified_name_for_a_method_caller() {
        let conn = indexed_conn_with_method_caller();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"helper"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["results"][0]["callers"][0]["name"], "Signer::unsign");
    }

    #[test]
    fn get_symbol_call_returns_callers_and_callees() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"b"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["found"], true);
        assert_eq!(payload["results"][0]["name"], "b");
        assert_eq!(payload["results"][0]["callers"][0]["name"], "a");
        assert_eq!(
            payload["results"][0]["callers"][0]["resolution"],
            "global-unique"
        );
        assert_eq!(payload["results"][0]["truncated"], false);
        assert_eq!(payload["results"][0]["truncated_count"], 0);
    }

    #[test]
    fn get_symbol_call_with_tiny_budget_is_truncated() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"b","token_budget":1}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["results"][0]["truncated"], true);
        assert!(payload["results"][0]["truncated_count"].as_u64().unwrap() > 0);
        assert!(
            payload["results"][0]["callers"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn get_symbol_call_for_missing_symbol_reports_not_found() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"ghost"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["found"], false);
        assert!(payload["message"].as_str().unwrap().contains("ghost"));
    }

    #[test]
    fn get_symbol_call_missing_name_argument_is_invalid_params() {
        let conn = empty_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32602);
    }

    #[test]
    fn impact_call_returns_affected_symbols_with_truncated_field() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"impact","arguments":{"name":"b"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["found"], true);
        assert_eq!(payload["results"][0]["truncated"], false);
        assert_eq!(payload["results"][0]["affected"][0]["name"], "a");
        assert_eq!(payload["results"][0]["affected"][0]["depth"], 1);
    }

    #[test]
    fn impact_call_with_tiny_budget_is_truncated() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"impact","arguments":{"name":"b","token_budget":1}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["results"][0]["truncated"], true);
    }

    #[test]
    fn get_symbol_call_with_duplicate_name_in_same_file_returns_both_without_line() {
        let conn = indexed_conn_with_duplicate_name_in_same_file();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"widget","file":"overload.rs"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["count"], 2);
    }

    #[test]
    fn get_symbol_call_with_line_narrows_duplicate_name_in_same_file_to_one() {
        let conn = indexed_conn_with_duplicate_name_in_same_file();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"widget","file":"overload.rs","line":5}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["results"][0]["line"], 5);
        assert_eq!(
            payload["results"][0]["signature"],
            "pub fn widget(x: i32, y: i32) -> i32"
        );
    }

    #[test]
    fn get_symbol_call_with_line_but_no_file_is_invalid_params() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"b","line":1}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32602);
    }

    #[test]
    fn find_call_returns_matching_symbols() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"a"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["found"], true);
        let names: Vec<&str> = payload["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a"]);
        assert_eq!(payload["results"][0]["file"], "a.rs");
    }

    #[test]
    fn find_call_ranks_exact_match_before_prefix_and_substring_matches() {
        let repo_dir = tempdir().unwrap();
        fs::write(
            repo_dir.path().join("widgets.rs"),
            b"pub fn widget_factory() {}\n\npub fn old_widget() {}\n\npub fn widget() {}\n",
        )
        .unwrap();

        let root = RepoRoot::new(repo_dir.path()).unwrap();
        let conn = open_connection(":memory:").unwrap();
        let mut registry = LanguageRegistry::new();
        registry.register(Box::new(RustSupport)).unwrap();
        index_repo(&root, &conn, &registry).unwrap();

        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"widget"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        let names: Vec<&str> = payload["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["widget", "widget_factory", "old_widget"]);
    }

    #[test]
    fn find_call_respects_limit() {
        let repo_dir = tempdir().unwrap();
        fs::write(
            repo_dir.path().join("widgets.rs"),
            b"pub fn widget_a() {}\n\npub fn widget_b() {}\n\npub fn widget_c() {}\n",
        )
        .unwrap();

        let root = RepoRoot::new(repo_dir.path()).unwrap();
        let conn = open_connection(":memory:").unwrap();
        let mut registry = LanguageRegistry::new();
        registry.register(Box::new(RustSupport)).unwrap();
        index_repo(&root, &conn, &registry).unwrap();

        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"widget","limit":2}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["count"], 2);
        assert_eq!(payload["total_matches"], 3);
        assert_eq!(payload["truncated"], true);
        assert_eq!(payload["truncated_count"], 1);
    }

    #[test]
    fn find_call_with_tiny_budget_is_truncated() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"a","token_budget":1}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["truncated"], true);
        assert_eq!(payload["found"], true);
        assert_eq!(payload["count"], 0);
        assert_eq!(payload["total_matches"], 1);
        assert!(payload["results"].as_array().unwrap().is_empty());
        let message = payload["message"].as_str().unwrap();
        assert!(message.contains('1'));
        assert!(message.contains("token_budget"));
    }

    #[test]
    fn find_call_for_no_matches_reports_not_found() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"ghost"}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["found"], false);
        assert!(payload["message"].as_str().unwrap().contains("ghost"));
    }

    #[test]
    fn find_call_missing_query_argument_is_invalid_params() {
        let conn = empty_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32602);
    }

    #[test]
    fn find_call_limit_above_maximum_is_invalid_params() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"a","limit":500}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32602);
    }

    #[test]
    fn find_call_with_zero_limit_returns_no_results_without_error() {
        let conn = indexed_conn();
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"a","limit":0}}}"#,
            &conn,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert_eq!(payload["found"], true);
        assert_eq!(payload["count"], 0);
        assert_eq!(payload["total_matches"], 1);
        assert!(payload["results"].as_array().unwrap().is_empty());
    }

    #[test]
    fn find_then_get_symbol_workflow_disambiguates_duplicate_name() {
        let conn = indexed_conn_with_duplicate_name_in_same_file();
        let find_response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find","arguments":{"query":"widget"}}}"#,
            &conn,
        )
        .unwrap();
        let find_parsed: Value = serde_json::from_str(&find_response).unwrap();
        let find_text = find_parsed["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        let find_payload: Value = serde_json::from_str(find_text).unwrap();
        assert_eq!(find_payload["count"], 2);

        let second_match = &find_payload["results"][1];
        let file = second_match["file"].as_str().unwrap();
        let line = second_match["line"].as_u64().unwrap();

        let get_symbol_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "get_symbol",
                "arguments": { "name": "widget", "file": file, "line": line }
            }
        });
        let get_symbol_response = handle_line(&get_symbol_request.to_string(), &conn).unwrap();
        let get_symbol_parsed: Value = serde_json::from_str(&get_symbol_response).unwrap();
        let get_symbol_text = get_symbol_parsed["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        let get_symbol_payload: Value = serde_json::from_str(get_symbol_text).unwrap();
        assert_eq!(get_symbol_payload["count"], 1);
        assert_eq!(get_symbol_payload["results"][0]["line"], line);
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let conn = empty_conn();
        let response = handle_line("not json", &conn).unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }

    #[test]
    fn notification_produces_no_response() {
        let conn = empty_conn();
        assert!(
            handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                &conn
            )
            .is_none()
        );
    }

    #[test]
    fn unknown_notification_produces_no_response() {
        let conn = empty_conn();
        assert!(
            handle_line(r#"{"jsonrpc":"2.0","method":"notifications/ghost"}"#, &conn).is_none()
        );
    }
}
