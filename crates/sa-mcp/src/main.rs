//! Minimal MCP server over stdio (JSON-RPC 2.0, newline-delimited).
//! Implements: initialize, ping, tools/list, tools/call. Logs go to stderr.

mod bridge;
mod multi;
mod screenshot;
mod tools;
mod window;

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

const SUPPORTED_PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Child processes (game clients) must not inherit our stdio pipes: CreateProcess with
/// handle inheritance passes every inheritable handle, not just the child's own stdio, and a
/// game holding the MCP pipe keeps it open after we exit.
fn make_stdio_non_inheritable() {
    use windows::Win32::Foundation::{SetHandleInformation, HANDLE_FLAGS, HANDLE_FLAG_INHERIT};
    use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        unsafe {
            if let Ok(h) = GetStdHandle(id) {
                let _ = SetHandleInformation(h, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
            }
        }
    }
}

fn main() {
    make_stdio_non_inheritable();
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    eprintln!("sa-mcp {} ready", env!("CARGO_PKG_VERSION"));

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_msg(&mut stdout, &error_response(Value::Null, -32700, &format!("parse error: {e}")));
                continue;
            }
        };
        if let Some(resp) = handle(&msg) {
            write_msg(&mut stdout, &resp);
        }
    }
}

fn write_msg(out: &mut impl Write, v: &Value) {
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Returns None for notifications (no `id`).
fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = SUPPORTED_PROTOCOLS.iter().find(|v| **v == requested).unwrap_or(&SUPPORTED_PROTOCOLS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "sa-mcp", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Tools for GTA San Andreas 1.0 US clients running the sa_bridge.asi plugin. \
                    Call list_instances first; launch_instances starts clients that boot straight into an empty world. \
                    Per-instance tools take an optional `instance` (default: lowest running). \
                    Coordinates are world units (metres); heading is degrees, 0 = north.",
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools::definitions() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match tools::call(name, &args) {
                Some(r) => r,
                None => return Some(error_response(id, -32602, &format!("unknown tool: {name}"))),
            }
        }
        _ => return Some(error_response(id, -32601, &format!("method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}
