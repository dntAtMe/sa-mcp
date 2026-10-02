//! Wire protocol between the MCP server (`sa-mcp`) and the in-game bridge (`sa-bridge`).
//!
//! Transport: TCP on `127.0.0.1:BASE_PORT + instance`, one JSON object per line in each
//! direction. Every request gets exactly one response line, in order.

use serde::{Deserialize, Serialize};

/// Instance N listens on `BASE_PORT + N`.
pub const BASE_PORT: u16 = 47311;
pub const MAX_INSTANCES: u8 = 8;
/// Env var carrying a JSON [`BootConfig`] into a game process started by `launch_instances`.
pub const BOOT_ENV: &str = "SA_BRIDGE_BOOT";

/// Named mutex that marks instance slot `n` as taken. Exists exactly as long as the game runs.
pub fn instance_mutex_name(n: u8) -> String {
    format!("Local\\sa-mcp-bridge-{n}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    /// Answered directly by the bridge's network thread, never touches game state.
    Status,
    /// Recent bridge log lines. Also answered off the game thread.
    Logs,
    PlayerState,
    WorldState,
    ListVehicles { radius: Option<f32> },
    ListPeds { radius: Option<f32> },
    Teleport { x: f32, y: f32, z: f32 },
    SetPlayer { health: Option<f32>, armor: Option<f32>, money: Option<i32> },
    SetTime { hour: u8, minute: u8 },
    SetWeather { id: i16 },
    /// Served off the game thread (fault-safe), so it works during loads and hangs.
    ReadMemory { address: u32, length: u32 },
    WriteMemory { address: u32, bytes_hex: String },
    /// Execute SCM commands in the bridge's persistent script context (32 local vars that
    /// survive between calls).
    RunScript { commands: Vec<ScriptCommand> },
    /// Replace (or append to) the queue of controller-state overrides for pad 0.
    Input { steps: Vec<InputStep>, append: bool },
    InputClear,
    /// Call `export` in loaded module `module` on the game thread and return its JSON.
    /// Contract: `extern "C" fn(buf: *mut u8, cap: u32) -> u32` writes UTF-8 JSON into `buf`
    /// (up to `cap` bytes) and returns the full length it needs.
    PluginQuery { module: String, export: String },
    /// Sample player state on the game thread for `seconds` at `hz`. Reply arrives when done.
    Record { seconds: f32, hz: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptCommand {
    pub op: u16,
    #[serde(default)]
    pub args: Vec<ScriptArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptArg {
    Int(i32),
    Float(f32),
    /// Local variable slot 0-31 (input or output).
    Var(u16),
    Str(String),
}

/// Number of i16 fields in CControllerState.
pub const PAD_FIELDS: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputStep {
    pub ms: u32,
    /// (CControllerState field index, value). Fields not listed keep the game's own input.
    pub fields: Vec<(u8, i16)>,
}

/// Startup behaviour, read by the bridge from the [`BOOT_ENV`] env var.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BootConfig {
    /// Keep the original behaviour of freezing the main loop while the window is not in the
    /// foreground. Off by default: background clients must keep simulating.
    pub pause_when_unfocused: bool,
    /// Skip logo/title/intro movies.
    pub skip_intro: bool,
    /// Leave the main menu immediately and start a game.
    pub auto_start: bool,
    /// Multiplayer-style world: main.scm disabled after its loading tick, player dressed and
    /// moved to `spawn` (x offset 2 m per instance), no traffic or ambient peds, no wanted level.
    pub mp_mode: bool,
    pub spawn: Option<Spawn>,
    /// [hour, minute]
    pub time: Option<[u8; 2]>,
    pub weather: Option<i16>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Spawn {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    #[serde(default)]
    pub heading: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: serde_json::Value) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}
