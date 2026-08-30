use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[test]
fn legacy_stdio_process_negotiates_lists_and_calls() {
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
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");

    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 41,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "lific-process-test", "version": "1"}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 17,
            "method": "tools/list",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 88,
            "method": "tools/call",
            "params": {
                "name": "search",
                "arguments": {"query": "lific-process-test-no-match"}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 89,
            "method": "tools/call",
            "params": {
                "name": "search",
                "arguments": {"query": 3}
            }
        }),
    ];
    let mut stdin = child.stdin.take().expect("piped stdin");
    for request in requests {
        serde_json::to_writer(&mut stdin, &request).expect("write request");
        stdin.write_all(b"\n").expect("terminate request");
    }
    drop(stdin);

    let deadline = Instant::now() + Duration::from_secs(10);
    let output = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout_bytes = Vec::new();
                let mut stderr_bytes = Vec::new();
                stdout
                    .read_to_end(&mut stdout_bytes)
                    .expect("collect MCP stdout");
                stderr
                    .read_to_end(&mut stderr_bytes)
                    .expect("collect MCP stderr");
                break std::process::Output {
                    status,
                    stdout: stdout_bytes,
                    stderr: stderr_bytes,
                };
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("lific MCP process did not exit within 10 seconds");
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("could not poll lific MCP process: {error}");
            }
        }
    };
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

    assert_eq!(responses.len(), 4, "notifications produce no response");
    let response = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap_or_else(|| panic!("missing response {id}: {responses:?}"))
    };
    assert_eq!(response(41)["result"]["protocolVersion"], "2025-03-26");
    assert!(response(17)["result"]["tools"].is_array());
    assert!(response(88)["result"]["content"].is_array());
    assert_eq!(response(89)["error"]["code"], -32602);
    assert!(response(89).get("result").is_none());
}
