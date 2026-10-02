//! MCP tool definitions and dispatch.

use std::process::Stdio;

use base64::Engine;
use proto::{Request, Response};
use serde_json::{json, Value};

use crate::{bridge, screenshot};

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

pub fn definitions() -> Vec<Value> {
    let radius = json!({ "radius": { "type": "number", "description": "Only include entities within this distance of the player" } });
    let address = json!({ "type": ["string", "integer"], "description": "Address as hex string (\"0xB7CD98\") or integer" });
    vec![
        tool("game_status", "Bridge connectivity, game version, whether the game loop is running, player presence. Call first.", json!({}), &[]),
        tool("get_player_state", "Player position, heading, health, armor, money, wanted level, interior and current vehicle.", json!({}), &[]),
        tool("get_world_state", "Game clock, weather, current interior area, game timer and game state.", json!({}), &[]),
        tool("list_vehicles", "Vehicles in the vehicle pool with model id, position, health and distance to the player.", radius.clone(), &[]),
        tool("list_peds", "Peds in the ped pool with model id, position, health and distance to the player.", radius, &[]),
        tool(
            "teleport",
            "Move the player (or the player's vehicle) to world coordinates. Pick z slightly above ground; distant areas may need a moment to stream collision.",
            json!({ "x": { "type": "number" }, "y": { "type": "number" }, "z": { "type": "number" } }),
            &["x", "y", "z"],
        ),
        tool(
            "set_player",
            "Set any of player health (0-100 normally), armor (0-100) and money.",
            json!({ "health": { "type": "number" }, "armor": { "type": "number" }, "money": { "type": "integer" } }),
            &[],
        ),
        tool(
            "set_time",
            "Set the in-game clock.",
            json!({ "hour": { "type": "integer", "minimum": 0, "maximum": 23 }, "minute": { "type": "integer", "minimum": 0, "maximum": 59 } }),
            &["hour", "minute"],
        ),
        tool(
            "set_weather",
            "Force weather id (0-45, e.g. 0 sunny LS, 8 rainy, 9 foggy, 19 sandstorm).",
            json!({ "id": { "type": "integer", "minimum": 0, "maximum": 45 } }),
            &["id"],
        ),
        tool(
            "read_memory",
            "Read raw bytes from game memory (fault-safe). Max 4096 bytes.",
            json!({ "address": address, "length": { "type": "integer", "minimum": 1, "maximum": 4096 } }),
            &["address", "length"],
        ),
        tool(
            "write_memory",
            "Write raw bytes into game memory. Dangerous: wrong writes crash the game.",
            json!({ "address": address, "bytes_hex": { "type": "string", "description": "e.g. \"90 90 90\"" } }),
            &["address", "bytes_hex"],
        ),
        tool("bridge_logs", "Recent log lines from the in-game bridge plugin.", json!({}), &[]),
        tool(
            "screenshot",
            "Capture the game window as a PNG image.",
            json!({ "max_width": { "type": "integer", "description": "Downscale to this width (default 960, 0 = full size)" } }),
            &[],
        ),
        tool("launch_game", "Start gta_sa.exe from GTA_SA_DIR if it is not already running.", json!({}), &[]),
    ]
}

fn text_result(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn bridge_result(req: Request) -> Value {
    match bridge::send(&req) {
        Ok(Response { ok: true, data, .. }) => {
            text_result(serde_json::to_string_pretty(&data.unwrap_or(Value::Null)).unwrap(), false)
        }
        Ok(Response { error, .. }) => text_result(error.unwrap_or_else(|| "unknown bridge error".into()), true),
        Err(e) => text_result(e, true),
    }
}

fn f32_arg(args: &Value, k: &str) -> Option<f32> {
    args.get(k).and_then(Value::as_f64).map(|v| v as f32)
}

fn parse_address(v: Option<&Value>) -> Result<u32, String> {
    match v {
        Some(Value::Number(n)) => n.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or("address out of range".into()),
        Some(Value::String(s)) => {
            let s = s.trim();
            let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => s.parse(),
            };
            parsed.map_err(|e| format!("bad address {s:?}: {e}"))
        }
        _ => Err("address is required".into()),
    }
}

/// Returns None for unknown tool names.
pub fn call(name: &str, args: &Value) -> Option<Value> {
    let req = match name {
        "game_status" => return Some(game_status()),
        "screenshot" => return Some(take_screenshot(args)),
        "launch_game" => return Some(launch_game()),
        "bridge_logs" => Request::Logs,
        "get_player_state" => Request::PlayerState,
        "get_world_state" => Request::WorldState,
        "list_vehicles" => Request::ListVehicles { radius: f32_arg(args, "radius") },
        "list_peds" => Request::ListPeds { radius: f32_arg(args, "radius") },
        "teleport" => match (f32_arg(args, "x"), f32_arg(args, "y"), f32_arg(args, "z")) {
            (Some(x), Some(y), Some(z)) => Request::Teleport { x, y, z },
            _ => return Some(text_result("x, y and z are required numbers".into(), true)),
        },
        "set_player" => Request::SetPlayer {
            health: f32_arg(args, "health"),
            armor: f32_arg(args, "armor"),
            money: args.get("money").and_then(Value::as_i64).map(|v| v as i32),
        },
        "set_time" => match (args.get("hour").and_then(Value::as_u64), args.get("minute").and_then(Value::as_u64)) {
            (Some(h), Some(m)) if h < 24 && m < 60 => Request::SetTime { hour: h as u8, minute: m as u8 },
            _ => return Some(text_result("hour 0-23 and minute 0-59 are required".into(), true)),
        },
        "set_weather" => match args.get("id").and_then(Value::as_i64) {
            Some(id) => Request::SetWeather { id: id as i16 },
            None => return Some(text_result("id is required".into(), true)),
        },
        "read_memory" => match parse_address(args.get("address")) {
            Ok(address) => Request::ReadMemory {
                address,
                length: args.get("length").and_then(Value::as_u64).unwrap_or(16) as u32,
            },
            Err(e) => return Some(text_result(e, true)),
        },
        "write_memory" => match (parse_address(args.get("address")), args.get("bytes_hex").and_then(Value::as_str)) {
            (Ok(address), Some(b)) => Request::WriteMemory { address, bytes_hex: b.to_string() },
            (Err(e), _) => return Some(text_result(e, true)),
            (_, None) => return Some(text_result("bytes_hex is required".into(), true)),
        },
        _ => return None,
    };
    Some(bridge_result(req))
}

fn game_status() -> Value {
    let window = screenshot::find_game_window().is_some();
    match bridge::send(&Request::Status) {
        Ok(Response { data: Some(mut d), .. }) => {
            d["game_window_found"] = json!(window);
            text_result(serde_json::to_string_pretty(&d).unwrap(), false)
        }
        Ok(r) => text_result(format!("bridge error: {:?}", r.error), true),
        Err(e) => text_result(format!("{e}\ngame_window_found: {window}"), true),
    }
}

fn take_screenshot(args: &Value) -> Value {
    let max_width = args.get("max_width").and_then(Value::as_u64).unwrap_or(960) as u32;
    match screenshot::capture(max_width) {
        Ok(shot) => json!({
            "content": [
                { "type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(&shot.png) },
                { "type": "text", "text": format!("{}x{}", shot.width, shot.height) },
            ],
            "isError": false,
        }),
        Err(e) => text_result(e, true),
    }
}

fn launch_game() -> Value {
    if screenshot::find_game_window().is_some() {
        return text_result("game window already open".into(), false);
    }
    let Ok(dir) = std::env::var("GTA_SA_DIR") else {
        return text_result("GTA_SA_DIR env var is not set (configure it in .mcp.json)".into(), true);
    };
    let exe = std::path::Path::new(&dir).join("gta_sa.exe");
    // Detach stdio: the game must not inherit our stdout, which is the MCP channel.
    let spawned = std::process::Command::new(&exe)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(child) => text_result(
            format!("started {} (pid {}). The bridge comes up a few seconds after launch; poll game_status.", exe.display(), child.id()),
            false,
        ),
        Err(e) => text_result(format!("failed to start {}: {e}", exe.display()), true),
    }
}
