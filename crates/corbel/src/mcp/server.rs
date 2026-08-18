use std::io::{BufRead, Write};

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::mcp::tools::list_tools;

const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";

pub struct McpServer {
    #[allow(dead_code)]
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

            if let Some(response) = handle_line(trimmed) {
                tracing::debug!(response = %response, "sending mcp response");
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
    }
}

fn handle_line(line: &str) -> Option<String> {
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
        "tools/call" => {
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
            Some(error_response(
                id,
                -32602,
                &format!("Unknown tool: {tool_name}"),
            ))
        }
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
    fn tools_list_returns_empty_array() {
        let response = handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["result"]["tools"], json!([]));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let response =
            handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"does/not/exist"}"#).unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32601);
    }

    #[test]
    fn unknown_tool_call_returns_error() {
        let response = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ghost"}}"#,
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32602);
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let response = handle_line("not json").unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }

    #[test]
    fn notification_produces_no_response() {
        assert!(handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn unknown_notification_produces_no_response() {
        assert!(handle_line(r#"{"jsonrpc":"2.0","method":"notifications/ghost"}"#).is_none());
    }
}
