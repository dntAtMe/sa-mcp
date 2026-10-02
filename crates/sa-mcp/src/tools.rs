//! MCP tool definitions and dispatch.

use std::time::Duration;

use base64::Engine;
use proto::{InputStep, Request, Response, ScriptArg, ScriptCommand};
use serde_json::{json, Value};

use crate::{bridge, mpserver, multi, screenshot};

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

/// A tool that targets one game instance: adds the optional `instance` property.
fn itool(name: &str, description: &str, mut properties: Value, required: &[&str]) -> Value {
    properties.as_object_mut().unwrap().insert(
        "instance".into(),
        json!({ "type": "integer", "minimum": 0, "maximum": 7, "description": "Game instance (see list_instances). Default: lowest running." }),
    );
    tool(name, description, properties, required)
}

/// CControllerState field names in memory order (index = field id on the wire).
pub const PAD_FIELD_NAMES: [&str; proto::PAD_FIELDS] = [
    "LeftStickX", "LeftStickY", "RightStickX", "RightStickY",
    "LeftShoulder1", "LeftShoulder2", "RightShoulder1", "RightShoulder2",
    "DPadUp", "DPadDown", "DPadLeft", "DPadRight",
    "Start", "Select", "ButtonSquare", "ButtonTriangle", "ButtonCross", "ButtonCircle",
    "ShockButtonL", "ShockButtonR", "ChatIndicated", "PedWalk", "VehicleMouseLook", "RadioTrackSkip",
];

/// High-level action -> (field, value). On foot and in vehicles the same buttons mean
/// different things, hence the aliases.
const ACTIONS: &[(&str, &str, i16)] = &[
    ("forward", "LeftStickY", -128),
    ("back", "LeftStickY", 128),
    ("left", "LeftStickX", -128),
    ("right", "LeftStickX", 128),
    ("steer_left", "LeftStickX", -128),
    ("steer_right", "LeftStickX", 128),
    ("sprint", "ButtonCross", 255),
    ("accelerate", "ButtonCross", 255),
    ("jump", "ButtonSquare", 255),
    ("brake", "ButtonSquare", 255),
    ("enter_exit", "ButtonTriangle", 255),
    ("fire", "ButtonCircle", 255),
    ("aim", "RightShoulder1", 255),
    ("handbrake", "RightShoulder1", 255),
    ("crouch", "ShockButtonL", 255),
    ("horn", "ShockButtonL", 255),
    ("walk", "PedWalk", 255),
    ("look_left", "RightStickX", -128),
    ("look_right", "RightStickX", 128),
];

pub fn definitions() -> Vec<Value> {
    let radius = json!({ "radius": { "type": "number", "description": "Only include entities within this distance of the player" } });
    let address = json!({ "type": ["string", "integer"], "description": "Address as hex string (\"0xB7CD98\") or integer" });
    let instances = json!({ "type": "array", "items": { "type": "integer" }, "description": "Instances to include. Default: all running." });
    let action_names: Vec<&str> = ACTIONS.iter().map(|a| a.0).collect();
    vec![
        // --- instances ---
        tool("list_instances", "Running game instances (slot, pid, port, game state, player position). Call first.", json!({}), &[]),
        tool(
            "launch_instances",
            "Start N game clients from GTA_SA_DIR. By default they boot straight into an empty multiplayer-style world \
             (movies skipped, menu skipped, main.scm off, player spawned at Grove Street offset by 2m per instance, no traffic/peds/wanted). \
             Waits until each bridge is up and tiles the windows.",
            json!({
                "count": { "type": "integer", "minimum": 1, "maximum": 8 },
                "boot": {
                    "type": "object",
                    "description": "Overrides for the boot config: skip_intro, auto_start, mp_mode (bools), spawn {x,y,z,heading}, time [h,m], weather id. Set mp_mode=false for the normal story game.",
                },
                "tile": { "type": "boolean", "description": "Tile windows on screen (default true)" },
                "env": { "type": "object", "description": "Extra environment variables for every client (e.g. a multiplayer server address)" },
                "env_per_instance": { "type": "array", "items": { "type": "object" }, "description": "Extra env vars per launched client, by launch order (e.g. player names)" },
            }),
            &["count"],
        ),
        tool("stop_instances", "Terminate game instances.", json!({ "instances": instances.clone() }), &[]),
        tool(
            "compare_instances",
            "Desync check: for each pair of instances, where does instance A's local player appear in instance B?              Exact when a plugin exports sa_debug_json with net ids (default plugin minisamp.asi): splits the error into network lag              (A's position vs last state B received) and render error (that state vs B's ped). Otherwise falls back to nearest-ped matching.",
            json!({ "instances": instances.clone(), "plugin": { "type": "string", "description": "Module exporting sa_debug_json (default minisamp.asi)" } }),
            &[],
        ),
        tool(
            "sync_trace",
            "Sample compare_instances (net-id matching) over time and return, per instance pair, mean/p95/max of total, network-lag and              render error plus snap count and a series. Use while driving a client with `input` (wait=false) and/or with server_netsim to tune sync.",
            json!({
                "instances": instances.clone(),
                "seconds": { "type": "number", "description": "default 5, max 60" },
                "hz": { "type": "number", "description": "default 10, max 30" },
                "plugin": { "type": "string", "description": "default minisamp.asi" },
            }),
            &[],
        ),
        tool(
            "record",
            "Sample player state (t_ms, x, y, z, heading, speed m/s, health, in_vehicle, frame) on the game thread at `hz` for `seconds`, on several instances at once. Combine with `input` to measure movement and sync.",
            json!({
                "instances": instances,
                "seconds": { "type": "number", "minimum": 0, "maximum": 60 },
                "hz": { "type": "number", "minimum": 0.5, "maximum": 60, "description": "default 10" },
            }),
            &["seconds"],
        ),
        // --- multiplayer server (SA_MCP_SERVER_CMD / SA_MCP_SERVER_ADMIN) ---
        tool(
            "server_start",
            "Start the multiplayer server under development (command from SA_MCP_SERVER_CMD or `command`), wait for its admin port. Output goes to a log file (server_logs).",
            json!({ "command": { "type": "string", "description": "Override the server command line" } }),
            &[],
        ),
        tool("server_stop", "Stop the server started by server_start.", json!({}), &[]),
        tool("server_status", "Server state from its admin port: players (id, name, address, last state, packet/byte counters, stale syncs), tick, netsim, packet kind totals.", json!({}), &[]),
        tool(
            "server_netsim",
            "Get or set simulated network conditions on the server, applied to every packet in both directions: latency_ms (one-way, RTT grows by 2x), jitter_ms (uniform 0..j extra), loss_pct.",
            json!({ "latency_ms": { "type": "integer" }, "jitter_ms": { "type": "integer" }, "loss_pct": { "type": "number" } }),
            &[],
        ),
        tool(
            "server_packets",
            "Recent packet log from the server (time, direction, client, kind, size, dropped by netsim).",
            json!({ "limit": { "type": "integer", "description": "default 50" }, "kind": { "type": "string", "description": "Only this packet kind, e.g. Sync / Snapshot / Hello" } }),
            &[],
        ),
        tool("server_kick", "Kick a player by id.", json!({ "id": { "type": "integer" } }), &["id"]),
        tool("server_logs", "Tail of the server's stdout/stderr.", json!({ "lines": { "type": "integer", "description": "default 50" } }), &[]),
        // --- per-instance state ---
        itool("game_status", "Bridge connectivity, game version, whether the game loop is running, player presence.", json!({}), &[]),
        itool("get_player_state", "Player position, heading, health, armor, money, wanted level, interior and current vehicle.", json!({}), &[]),
        itool("get_world_state", "Game clock, weather, current interior area, game timer and game state.", json!({}), &[]),
        itool("list_vehicles", "Vehicles in the vehicle pool with model id, position, health and distance to the player.", radius.clone(), &[]),
        itool("list_peds", "Peds in the ped pool with model id, position, health and distance to the player.", radius, &[]),
        itool(
            "teleport",
            "Move the player (or the player's vehicle) to world coordinates. Pick z slightly above ground; distant areas may need a moment to stream collision.",
            json!({ "x": { "type": "number" }, "y": { "type": "number" }, "z": { "type": "number" } }),
            &["x", "y", "z"],
        ),
        itool(
            "set_player",
            "Set any of player health (0-100 normally), armor (0-100) and money.",
            json!({ "health": { "type": "number" }, "armor": { "type": "number" }, "money": { "type": "integer" } }),
            &[],
        ),
        itool(
            "set_time",
            "Set the in-game clock.",
            json!({ "hour": { "type": "integer", "minimum": 0, "maximum": 23 }, "minute": { "type": "integer", "minimum": 0, "maximum": 59 } }),
            &["hour", "minute"],
        ),
        itool(
            "set_weather",
            "Force weather id (0-45, e.g. 0 sunny LS, 8 rainy, 9 foggy, 19 sandstorm).",
            json!({ "id": { "type": "integer", "minimum": 0, "maximum": 45 } }),
            &["id"],
        ),
        // --- control ---
        itool(
            "run_script",
            "Execute GTA SA SCM opcodes (Sanny Builder opcode numbers) in a persistent script context with 32 local variables that survive between calls. \
             Args: JSON integer -> int, number with a decimal point -> float (write 2495.0, not 2495, for coordinates), string -> text, \
             {\"var\": n} -> local var n (for outputs such as created handles, and to pass them back in), {\"float\": x} / {\"int\": n} to force a type. \
             Example: request + load + create car: [{op:\"0247\",args:[411]},{op:\"038B\"},{op:\"00A5\",args:[411,2500.0,-1670.0,13.5,{\"var\":2}]}]. \
             Returns each command's condition result and the values of all vars referenced. Wrong argument counts are detected and stop execution, \
             but bad arguments can still crash the game.",
            json!({
                "commands": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "op": { "type": ["string", "integer"], "description": "Opcode as hex string (\"00A5\"); add 0x8000 for NOT" },
                            "args": { "type": "array" },
                        },
                        "required": ["op"],
                    },
                },
            }),
            &["commands"],
        ),
        itool(
            "input",
            "Inject controller input into the game (no window focus needed; overrides real input for the listed fields). \
             Steps run back to back; each holds its actions for `ms`. Actions: forward, back, left, right, sprint, jump, enter_exit, fire, aim, crouch, walk, look_left, look_right; \
             in vehicles: accelerate, brake, steer_left, steer_right, handbrake, horn. `raw` sets CControllerState fields directly (sticks -128..128, buttons 0/255). \
             A step with no actions is a pause. Movement is relative to the camera.",
            json!({
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ms": { "type": "integer" },
                            "actions": { "type": "array", "items": { "type": "string", "enum": action_names } },
                            "raw": { "type": "object", "description": format!("Field name -> value. Fields: {}", PAD_FIELD_NAMES.join(", ")) },
                        },
                        "required": ["ms"],
                    },
                },
                "append": { "type": "boolean", "description": "Append to the running queue instead of replacing it" },
                "wait": { "type": "boolean", "description": "Return after the sequence finishes (default true, max 30 s)" },
            }),
            &["steps"],
        ),
        itool("input_clear", "Stop injected input immediately.", json!({}), &[]),
        // --- debugging ---
        itool(
            "read_memory",
            "Read raw bytes from game memory (fault-safe, works during loads). Max 4096 bytes.",
            json!({ "address": address, "length": { "type": "integer", "minimum": 1, "maximum": 4096 } }),
            &["address", "length"],
        ),
        itool(
            "write_memory",
            "Write raw bytes into game memory. Dangerous: wrong writes crash the game.",
            json!({ "address": address, "bytes_hex": { "type": "string", "description": "e.g. \"90 90 90\"" } }),
            &["address", "bytes_hex"],
        ),
        itool(
            "plugin_query",
            "Ask a plugin loaded in the game for its debug state. The module must export              `extern \"C\" fn sa_debug_json(buf: *mut u8, cap: u32) -> u32` (writes JSON, returns the length it needs);              it is called on the game thread. e.g. module \"minisamp.asi\".",
            json!({ "module": { "type": "string" }, "export": { "type": "string", "description": "default sa_debug_json" } }),
            &["module"],
        ),
        itool("bridge_logs", "Recent log lines from the in-game bridge plugin.", json!({}), &[]),
        itool(
            "screenshot",
            "Capture the game window as a PNG image.",
            json!({ "max_width": { "type": "integer", "description": "Downscale to this width (default 960, 0 = full size)" } }),
            &[],
        ),
    ]
}

pub fn text_result(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

pub fn json_result(v: Result<Value, String>) -> Value {
    match v {
        Ok(v) => text_result(serde_json::to_string_pretty(&v).unwrap(), false),
        Err(e) => text_result(e, true),
    }
}

fn bridge_result(instance: Option<u8>, req: Request) -> Value {
    match bridge::send(instance, &req) {
        Ok(Response { ok: true, data, .. }) => json_result(Ok(data.unwrap_or(Value::Null))),
        Ok(Response { error, .. }) => text_result(error.unwrap_or_else(|| "unknown bridge error".into()), true),
        Err(e) => text_result(e, true),
    }
}

fn f32_arg(args: &Value, k: &str) -> Option<f32> {
    args.get(k).and_then(Value::as_f64).map(|v| v as f32)
}

pub fn instances_arg(args: &Value) -> Option<Vec<u8>> {
    args.get("instances").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_u64()).map(|v| v as u8).collect())
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

fn parse_script(args: &Value) -> Result<Vec<ScriptCommand>, String> {
    let cmds = args.get("commands").and_then(Value::as_array).ok_or("commands must be an array")?;
    cmds.iter()
        .enumerate()
        .map(|(i, c)| {
            let op = match c.get("op") {
                Some(Value::String(s)) => {
                    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
                    u16::from_str_radix(s, 16).map_err(|e| format!("command {i}: bad op {s:?}: {e}"))?
                }
                Some(Value::Number(n)) => n.as_u64().and_then(|n| u16::try_from(n).ok()).ok_or(format!("command {i}: bad op"))?,
                _ => return Err(format!("command {i}: op is required")),
            };
            let args = match c.get("args") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(a)) => a.iter().map(|v| parse_script_arg(v).map_err(|e| format!("command {i}: {e}"))).collect::<Result<_, _>>()?,
                Some(_) => return Err(format!("command {i}: args must be an array")),
            };
            Ok(ScriptCommand { op, args })
        })
        .collect()
}

fn parse_script_arg(v: &Value) -> Result<ScriptArg, String> {
    match v {
        Value::Number(n) if n.is_i64() || n.is_u64() => {
            n.as_i64().and_then(|n| i32::try_from(n).ok()).map(ScriptArg::Int).ok_or(format!("int out of range: {n}"))
        }
        Value::Number(n) => Ok(ScriptArg::Float(n.as_f64().unwrap() as f32)),
        Value::String(s) => Ok(ScriptArg::Str(s.clone())),
        Value::Object(o) => {
            if let Some(i) = o.get("var").and_then(Value::as_u64) {
                Ok(ScriptArg::Var(i as u16))
            } else if let Some(f) = o.get("float").and_then(Value::as_f64) {
                Ok(ScriptArg::Float(f as f32))
            } else if let Some(i) = o.get("int").and_then(Value::as_i64) {
                Ok(ScriptArg::Int(i as i32))
            } else {
                Err(format!("unknown arg object {v}"))
            }
        }
        _ => Err(format!("unsupported arg {v}")),
    }
}

fn field_index(name: &str) -> Option<u8> {
    PAD_FIELD_NAMES.iter().position(|f| f.eq_ignore_ascii_case(name)).map(|i| i as u8)
}

fn parse_input(args: &Value) -> Result<Vec<InputStep>, String> {
    let steps = args.get("steps").and_then(Value::as_array).ok_or("steps must be an array")?;
    steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ms = s.get("ms").and_then(Value::as_u64).ok_or(format!("step {i}: ms is required"))? as u32;
            let mut fields: Vec<(u8, i16)> = Vec::new();
            let mut set = |idx: u8, val: i16| {
                fields.retain(|f| f.0 != idx);
                fields.push((idx, val));
            };
            for a in s.get("actions").and_then(Value::as_array).into_iter().flatten() {
                let name = a.as_str().unwrap_or("");
                let (_, field, val) = ACTIONS.iter().find(|x| x.0 == name).ok_or(format!("step {i}: unknown action {name:?}"))?;
                set(field_index(field).unwrap(), *val);
            }
            for (k, v) in s.get("raw").and_then(Value::as_object).into_iter().flatten() {
                let idx = field_index(k).or_else(|| k.parse().ok()).ok_or(format!("step {i}: unknown field {k:?}"))?;
                let val = v.as_i64().ok_or(format!("step {i}: {k} must be an integer"))?;
                set(idx, val.clamp(i16::MIN as i64, i16::MAX as i64) as i16);
            }
            Ok(InputStep { ms, fields })
        })
        .collect()
}

/// Returns None for unknown tool names.
pub fn call(name: &str, args: &Value) -> Option<Value> {
    let instance = args.get("instance").and_then(Value::as_u64).map(|v| v as u8);
    let req = match name {
        "list_instances" => return Some(json_result(Ok(multi::list()))),
        "launch_instances" => return Some(json_result(multi::launch(args))),
        "stop_instances" => return Some(json_result(multi::stop(instances_arg(args)))),
        "compare_instances" => return Some(json_result(multi::compare(instances_arg(args), args.get("plugin").and_then(Value::as_str)))),
        "record" => return Some(json_result(multi::record(args))),
        "sync_trace" => return Some(json_result(multi::sync_trace(args))),
        "server_start" => return Some(json_result(mpserver::start(args))),
        "server_stop" => return Some(json_result(mpserver::stop())),
        "server_status" => return Some(json_result(mpserver::admin(json!({ "cmd": "status" })))),
        "server_netsim" => {
            let mut req = json!({ "cmd": "netsim" });
            for k in ["latency_ms", "jitter_ms", "loss_pct"] {
                if let Some(v) = args.get(k) {
                    req[k] = v.clone();
                }
            }
            return Some(json_result(mpserver::admin(req)));
        }
        "server_packets" => {
            let mut req = json!({ "cmd": "packets", "limit": args.get("limit").cloned().unwrap_or(json!(50)) });
            if let Some(k) = args.get("kind") {
                req["kind"] = k.clone();
            }
            return Some(json_result(mpserver::admin(req)));
        }
        "server_kick" => return Some(json_result(mpserver::admin(json!({ "cmd": "kick", "id": args.get("id").cloned().unwrap_or(Value::Null) })))),
        "server_logs" => return Some(json_result(mpserver::logs(args))),
        "game_status" => return Some(game_status(instance)),
        "screenshot" => return Some(take_screenshot(instance, args)),
        "input" => return Some(run_input(instance, args)),
        "bridge_logs" => Request::Logs,
        "get_player_state" => Request::PlayerState,
        "get_world_state" => Request::WorldState,
        "list_vehicles" => Request::ListVehicles { radius: f32_arg(args, "radius") },
        "list_peds" => Request::ListPeds { radius: f32_arg(args, "radius") },
        "input_clear" => Request::InputClear,
        "plugin_query" => Request::PluginQuery {
            module: args.get("module").and_then(Value::as_str).unwrap_or("").to_string(),
            export: args.get("export").and_then(Value::as_str).unwrap_or("sa_debug_json").to_string(),
        },
        "run_script" => match parse_script(args) {
            Ok(commands) => Request::RunScript { commands },
            Err(e) => return Some(text_result(e, true)),
        },
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
    Some(bridge_result(instance, req))
}

fn game_status(instance: Option<u8>) -> Value {
    json_result(bridge::query(instance, &Request::Status).map(|mut d| {
        let pid = d.get("pid").and_then(Value::as_u64).map(|p| p as u32);
        d["game_window_found"] = json!(crate::window::find(pid).is_some());
        d
    }))
}

fn run_input(instance: Option<u8>, args: &Value) -> Value {
    let steps = match parse_input(args) {
        Ok(s) => s,
        Err(e) => return text_result(e, true),
    };
    let total: u32 = steps.iter().map(|s| s.ms).sum();
    let append = args.get("append").and_then(Value::as_bool).unwrap_or(false);
    if let Err(e) = bridge::query(instance, &Request::Input { steps, append }) {
        return text_result(e, true);
    }
    if args.get("wait").and_then(Value::as_bool).unwrap_or(true) {
        std::thread::sleep(Duration::from_millis(total.min(30_000) as u64 + 50));
    }
    json_result(bridge::query(instance, &Request::Status).map(|s| {
        json!({ "queued_ms": total, "pending_ms": s.get("input_pending_ms").cloned().unwrap_or(Value::Null) })
    }))
}

fn take_screenshot(instance: Option<u8>, args: &Value) -> Value {
    let max_width = args.get("max_width").and_then(Value::as_u64).unwrap_or(960) as u32;
    // Prefer the window of the requested instance; fall back to any game window when no bridge runs.
    let pid = match bridge::query(instance, &Request::Status) {
        Ok(s) => s.get("pid").and_then(Value::as_u64).map(|p| p as u32),
        Err(e) if instance.is_some() => return text_result(e, true),
        Err(_) => None,
    };
    // Preferred: back buffer copied in-process by the bridge (immune to DWM/occlusion issues).
    if let Ok(d) = bridge::query(instance, &Request::Screenshot { max_width }) {
        let w = d["width"].as_u64().unwrap_or(0) as u32;
        let h = d["height"].as_u64().unwrap_or(0) as u32;
        let raw = d["bgra_b64"].as_str().and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok());
        if let Some(bgra) = raw.filter(|b| b.len() == (w * h * 4) as usize) {
            return match screenshot::encode(&bgra, w, h, 0) {
                Ok(shot) => json!({
                    "content": [
                        { "type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(&shot.png) },
                        { "type": "text", "text": format!("{}x{} (back buffer)", shot.width, shot.height) },
                    ],
                    "isError": false,
                }),
                Err(e) => text_result(e, true),
            };
        }
    }
    match screenshot::capture(pid, max_width) {
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
