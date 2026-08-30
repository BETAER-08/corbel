use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;
use tempfile::tempdir;

fn corbel_cmd() -> Command {
    Command::cargo_bin("corbel").unwrap()
}

fn indexed_repo() -> TempDir {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\n").unwrap();
    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();
    repo_dir
}

fn indexed_repo_with_call_chain() -> TempDir {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {\n    b();\n}\n").unwrap();
    fs::write(repo_dir.path().join("b.rs"), b"pub fn b() {\n    c();\n}\n").unwrap();
    fs::write(repo_dir.path().join("c.rs"), b"pub fn c() {}\n").unwrap();
    corbel_cmd()
        .arg("index")
        .arg(repo_dir.path())
        .assert()
        .success();
    repo_dir
}

fn tools_call_request(id: u64, tool_name: &str, arguments: Value) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{tool_name}","arguments":{arguments}}}}}"#
    )
}

fn run_requests(repo_dir: &std::path::Path, lines: &[String]) -> Vec<Value> {
    let mut input = String::new();
    for line in lines {
        input.push_str(line);
        input.push('\n');
    }

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir)
        .write_stdin(input)
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn initialize_request(protocol_version: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{protocol_version}","capabilities":{{}},"clientInfo":{{"name":"test-client","version":"0.0.0"}}}}}}"#
    )
}

#[test]
fn initialize_returns_valid_response_with_server_info() {
    let repo_dir = indexed_repo();

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(format!("{}\n", initialize_request("2024-11-05")))
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let line = stdout.lines().next().expect("one response line");
    let response: Value = serde_json::from_str(line).unwrap();

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["serverInfo"]["name"], "corbel");
    assert!(response["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn protocol_version_negotiation_echoes_supported_client_version() {
    let repo_dir = indexed_repo();

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(format!("{}\n", initialize_request("2024-11-05")))
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let response: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();

    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
}

#[test]
fn protocol_version_negotiation_falls_back_for_unsupported_client_version() {
    let repo_dir = indexed_repo();

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(format!("{}\n", initialize_request("1970-01-01")))
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let response: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();

    let negotiated = response["result"]["protocolVersion"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(negotiated, "1970-01-01");
    assert!(!negotiated.is_empty());
}

#[test]
fn tools_list_returns_valid_json_rpc_response() {
    let repo_dir = indexed_repo();

    let input = format!(
        "{}\n{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}}\n",
        initialize_request("2024-11-05")
    );

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(input)
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);

    let response: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(response["id"], 2);
    assert!(response["result"]["tools"].is_array());
}

#[test]
fn tools_list_declares_get_symbol_impact_and_find_with_required_fields() {
    let repo_dir = indexed_repo();

    let responses = run_requests(
        repo_dir.path(),
        &[
            initialize_request("2024-11-05"),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
        ],
    );

    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 3);

    let get_symbol = tools.iter().find(|t| t["name"] == "get_symbol").unwrap();
    assert!(
        get_symbol["description"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("caller")
    );
    assert_eq!(
        get_symbol["inputSchema"]["required"],
        serde_json::json!(["name"])
    );
    assert!(get_symbol["inputSchema"]["properties"]["file"].is_object());
    assert!(get_symbol["inputSchema"]["properties"]["line"].is_object());
    assert!(get_symbol["inputSchema"]["properties"]["token_budget"].is_object());

    let impact = tools.iter().find(|t| t["name"] == "impact").unwrap();
    assert!(
        impact["description"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("depth")
    );
    assert_eq!(
        impact["inputSchema"]["required"],
        serde_json::json!(["name"])
    );
    assert!(impact["inputSchema"]["properties"]["token_budget"].is_object());

    let find = tools.iter().find(|t| t["name"] == "find").unwrap();
    assert!(
        find["description"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("search")
    );
    assert_eq!(
        find["inputSchema"]["required"],
        serde_json::json!(["query"])
    );
    assert!(find["inputSchema"]["properties"]["limit"].is_object());
    assert!(find["inputSchema"]["properties"]["token_budget"].is_object());
}

#[test]
fn get_symbol_call_returns_definition_callers_and_callees() {
    let repo_dir = indexed_repo_with_call_chain();

    let responses = run_requests(
        repo_dir.path(),
        &[
            initialize_request("2024-11-05"),
            tools_call_request(2, "get_symbol", serde_json::json!({ "name": "b" })),
        ],
    );

    let text = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();

    assert_eq!(payload["found"], true);
    let result = &payload["results"][0];
    assert_eq!(result["name"], "b");
    assert_eq!(result["file"], "b.rs");
    assert_eq!(result["callers"][0]["name"], "a");
    assert!(result["callers"][0]["resolution"].is_string());
    assert_eq!(result["callees"][0]["name"], "c");
    assert!(result["callees"][0]["resolution"].is_string());
    assert_eq!(result["truncated"], false);
    assert_eq!(result["truncated_count"], 0);
}

#[test]
fn impact_call_returns_reverse_call_graph_with_truncated_field() {
    let repo_dir = indexed_repo_with_call_chain();

    let responses = run_requests(
        repo_dir.path(),
        &[
            initialize_request("2024-11-05"),
            tools_call_request(2, "impact", serde_json::json!({ "name": "c" })),
        ],
    );

    let text = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();

    assert_eq!(payload["found"], true);
    let result = &payload["results"][0];
    assert_eq!(result["truncated"], false);
    let affected = result["affected"].as_array().unwrap();
    assert!(
        affected
            .iter()
            .any(|node| node["name"] == "b" && node["depth"] == 1)
    );
    assert!(
        affected
            .iter()
            .any(|node| node["name"] == "a" && node["depth"] == 2)
    );
}

#[test]
fn impact_call_with_small_token_budget_is_truncated() {
    let repo_dir = indexed_repo_with_call_chain();

    let responses = run_requests(
        repo_dir.path(),
        &[
            initialize_request("2024-11-05"),
            tools_call_request(
                2,
                "impact",
                serde_json::json!({ "name": "c", "token_budget": 1 }),
            ),
        ],
    );

    let text = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();

    assert_eq!(payload["results"][0]["truncated"], true);
    assert!(payload["results"][0]["truncated_count"].as_u64().unwrap() > 0);
}

#[test]
fn get_symbol_call_for_unknown_name_returns_not_found_without_error() {
    let repo_dir = indexed_repo_with_call_chain();

    let responses = run_requests(
        repo_dir.path(),
        &[
            initialize_request("2024-11-05"),
            tools_call_request(
                2,
                "get_symbol",
                serde_json::json!({ "name": "no_such_symbol" }),
            ),
        ],
    );

    assert!(responses[1].get("error").is_none());
    let text = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert_eq!(payload["found"], false);
    assert!(
        payload["message"]
            .as_str()
            .unwrap()
            .contains("no_such_symbol")
    );
}

#[test]
fn get_symbol_call_missing_required_name_returns_error() {
    let repo_dir = indexed_repo_with_call_chain();

    let responses = run_requests(
        repo_dir.path(),
        &[
            initialize_request("2024-11-05"),
            tools_call_request(2, "get_symbol", serde_json::json!({})),
        ],
    );

    assert!(responses[1]["error"]["code"].is_number());
}

#[test]
fn unknown_method_returns_standard_json_rpc_error() {
    let repo_dir = indexed_repo();

    let input = format!(
        "{}\n{{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"does/not/exist\"}}\n",
        initialize_request("2024-11-05")
    );

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(input)
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    let response: Value = serde_json::from_str(lines[1]).unwrap();

    assert_eq!(response["id"], 3);
    assert_eq!(response["error"]["code"], -32601);
}

#[test]
fn unknown_tool_call_returns_standard_json_rpc_error() {
    let repo_dir = indexed_repo();

    let input = format!(
        "{}\n{{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{{\"name\":\"ghost\"}}}}\n",
        initialize_request("2024-11-05")
    );

    let assert = corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(input)
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    let response: Value = serde_json::from_str(lines[1]).unwrap();

    assert_eq!(response["id"], 4);
    assert!(response["error"]["code"].is_number());
}

#[test]
fn missing_index_fails_with_nonzero_exit_and_stderr_message() {
    let repo_dir = tempdir().unwrap();

    corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("index"));
}

#[test]
fn verbose_serve_keeps_stdout_pure_json_rpc_only() {
    let repo_dir = indexed_repo_with_call_chain();

    let input = format!(
        "{}\n{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}\n{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}}\n{}\n{}\n",
        initialize_request("2024-11-05"),
        tools_call_request(3, "get_symbol", serde_json::json!({ "name": "b" })),
        tools_call_request(4, "impact", serde_json::json!({ "name": "c" })),
    );

    let assert = corbel_cmd()
        .arg("-v")
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(input)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout_bytes = &output.stdout;
    let stdout = String::from_utf8(stdout_bytes.clone()).expect("stdout is valid utf-8");

    let mut response_count = 0;
    for line in stdout.split('\n') {
        if line.is_empty() {
            continue;
        }
        let parsed: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("stdout line is not valid JSON-RPC: {line:?}: {e}"));
        assert_eq!(parsed["jsonrpc"], "2.0");
        response_count += 1;
    }
    assert_eq!(response_count, 4);

    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(!stderr.is_empty());
}

fn write_v1_schema_index(repo_dir: &std::path::Path) {
    let corbel_dir = repo_dir.join(".corbel");
    fs::create_dir_all(&corbel_dir).unwrap();
    let db_path = corbel_dir.join("index.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "
        CREATE TABLE files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            lang TEXT NOT NULL,
            hash TEXT NOT NULL,
            indexed_at INTEGER NOT NULL
        );

        CREATE TABLE symbols (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            line INTEGER NOT NULL,
            signature TEXT,
            is_public INTEGER NOT NULL
        );

        CREATE TABLE relationships (
            id INTEGER PRIMARY KEY,
            caller_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
            callee_name TEXT NOT NULL,
            callee_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
            resolution TEXT NOT NULL CHECK (resolution IN ('same-file', 'scoped', 'global-unique', 'unresolved'))
        );

        CREATE TABLE imports (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            local_name TEXT NOT NULL,
            source_path TEXT NOT NULL,
            kind TEXT NOT NULL
        );

        CREATE TABLE embeddings (
            symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
            vector BLOB NOT NULL
        );

        CREATE INDEX idx_symbols_name ON symbols(name);
        CREATE INDEX idx_relationships_callee_file ON relationships(callee_file_id);
        CREATE INDEX idx_symbols_file ON symbols(file_id);
        CREATE INDEX idx_imports_file ON imports(file_id);
        ",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
}

#[test]
fn serve_rejects_old_schema_version_without_migrating_and_advises_reindex() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\n").unwrap();
    write_v1_schema_index(repo_dir.path());

    let db_path = repo_dir.path().join(".corbel").join("index.db");
    let bytes_before = fs::read(&db_path).unwrap();

    corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("schema version"))
        .stderr(predicate::str::contains("corbel index"));

    let bytes_after = fs::read(&db_path).unwrap();
    assert_eq!(bytes_before, bytes_after);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let user_version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(user_version, 1);
}

#[test]
fn serve_rejects_corrupted_index_with_distinct_message() {
    let repo_dir = tempdir().unwrap();
    fs::write(repo_dir.path().join("a.rs"), b"pub fn a() {}\n").unwrap();
    let corbel_dir = repo_dir.path().join(".corbel");
    fs::create_dir_all(&corbel_dir).unwrap();
    fs::write(corbel_dir.join("index.db"), b"not a sqlite database at all").unwrap();

    corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "could not be read as a corbel index",
        ))
        .stderr(predicate::str::contains("corbel index"))
        .stderr(predicate::str::contains("schema version").not());
}

#[test]
fn serve_starts_normally_against_current_schema_version() {
    let repo_dir = indexed_repo();

    corbel_cmd()
        .arg("serve")
        .arg(repo_dir.path())
        .write_stdin(format!("{}\n", initialize_request("2024-11-05")))
        .assert()
        .success();
}
