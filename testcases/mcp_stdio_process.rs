use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn metadata() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {
            "name": "lific-process-test",
            "version": "1"
        },
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

#[test]
fn modern_stdio_process_discovers_lists_and_calls_without_initialize() {
    let scratch = tempfile::tempdir().expect("create scratch directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_lific"))
        .args([
            "--db",
            &scratch.path().join("lific.db").display().to_string(),
            "mcp",
        ])
        .env_remove("LIFIC_TOKEN")
        .env("RUST_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start lific MCP process");

    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {"_meta": metadata()}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {"_meta": metadata()}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "search",
                "arguments": {"query": "lific-process-test-no-match"},
                "_meta": metadata()
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/list",
            "params": {}
        }),
    ];
    let mut stdin = child.stdin.take().expect("piped stdin");
    for request in requests {
        serde_json::to_writer(&mut stdin, &request).expect("write request");
        stdin.write_all(b"\n").expect("terminate request");
    }
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("wait for lific MCP process");
    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("stdout line is JSON-RPC"))
        .collect();

    assert_eq!(responses.len(), 4, "one response per modern request");
    let response = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap_or_else(|| panic!("missing response {id}: {responses:?}"))
    };
    let discovery = response(1);
    let tools = response(2);
    let call = response(3);
    let missing_metadata = response(4);
    assert!(
        discovery["result"]["supportedVersions"]
            .as_array()
            .is_some_and(|versions| versions.iter().any(|version| version == "2026-07-28"))
    );
    assert_eq!(discovery["result"]["resultType"], "complete");
    assert!(tools["result"]["tools"].is_array());
    assert_eq!(tools["result"]["resultType"], "complete");
    assert!(call["result"]["content"].is_array());
    assert_eq!(call["result"]["resultType"], "complete");
    assert_eq!(missing_metadata["error"]["code"], -32602);
}
