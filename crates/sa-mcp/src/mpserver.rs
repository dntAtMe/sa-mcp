//! Tools for the multiplayer server under development.
//!
//! The server is started from `SA_MCP_SERVER_CMD` (a command line; stdout/stderr go to
//! `%TEMP%\sa-mcp-server.log`) and controlled through a loopback JSON-lines admin port
//! (`SA_MCP_SERVER_ADMIN`, default 127.0.0.1:7778). Admin contract, implemented by the server:
//! `{"cmd":"status"}`, `{"cmd":"netsim",...}`, `{"cmd":"packets","limit":N,"kind":K}`,
//! `{"cmd":"kick","id":N}`, `{"cmd":"shutdown"}` -> `{"ok":bool,"data"|"error"}`.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

static CHILD: Mutex<Option<Child>> = Mutex::new(None);

fn admin_addr() -> Result<SocketAddr, String> {
    let s = std::env::var("SA_MCP_SERVER_ADMIN").unwrap_or_else(|_| "127.0.0.1:7778".into());
    s.parse().map_err(|e| format!("bad SA_MCP_SERVER_ADMIN {s:?}: {e}"))
}

fn log_path() -> PathBuf {
    std::env::temp_dir().join("sa-mcp-server.log")
}

pub fn admin(req: Value) -> Result<Value, String> {
    let addr = admin_addr()?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1000))
        .map_err(|e| format!("server admin port {addr} not reachable ({e}); is the server running? (server_start)"))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut w = stream.try_clone().map_err(|e| e.to_string())?;
    writeln!(w, "{req}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
    let resp: Value = serde_json::from_str(&line).map_err(|e| format!("bad admin response: {e}: {line}"))?;
    if resp["ok"].as_bool() == Some(true) {
        Ok(resp["data"].clone())
    } else {
        Err(resp["error"].as_str().unwrap_or("unknown admin error").to_string())
    }
}

/// Split a command line into program + args, honouring double quotes.
fn split_cmd(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn start(args: &Value) -> Result<Value, String> {
    if admin(json!({ "cmd": "status" })).is_ok() {
        return Ok(json!({ "already_running": true }));
    }
    let cmdline = args
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| std::env::var("SA_MCP_SERVER_CMD").ok())
        .ok_or("no server command: pass `command` or set SA_MCP_SERVER_CMD")?;
    let parts = split_cmd(&cmdline);
    let (prog, rest) = parts.split_first().ok_or("empty server command")?;
    let log = std::fs::File::create(log_path()).map_err(|e| e.to_string())?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(prog);
    cmd.args(rest).stdin(Stdio::null()).stdout(log).stderr(log2);
    if let Some(dir) = PathBuf::from(prog).parent().filter(|d| !d.as_os_str().is_empty()) {
        cmd.current_dir(dir);
    }
    let child = cmd.spawn().map_err(|e| format!("failed to start {prog}: {e}"))?;
    let pid = child.id();
    *CHILD.lock().unwrap() = Some(child);

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(status) = admin(json!({ "cmd": "status" })) {
            return Ok(json!({ "pid": pid, "log": log_path(), "status": status }));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!("server (pid {pid}) did not open its admin port within 10s; see {}", log_path().display()))
}

pub fn stop() -> Result<Value, String> {
    match CHILD.lock().unwrap().take() {
        Some(mut c) => {
            let pid = c.id();
            let _ = c.kill();
            let _ = c.wait();
            Ok(json!({ "stopped": pid }))
        }
        // Not ours (or sa-mcp restarted): ask the server to exit.
        None => admin(json!({ "cmd": "shutdown" })).map(|_| json!({ "stopped": "via admin shutdown" })),
    }
}

pub fn logs(args: &Value) -> Result<Value, String> {
    let lines = args.get("lines").and_then(Value::as_u64).unwrap_or(50) as usize;
    let text = std::fs::read_to_string(log_path()).map_err(|e| format!("{}: {e}", log_path().display()))?;
    let all: Vec<&str> = text.lines().collect();
    Ok(json!({ "lines": all[all.len().saturating_sub(lines)..] }))
}
