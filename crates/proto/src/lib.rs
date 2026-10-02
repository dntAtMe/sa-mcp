//! Wire protocol between the MCP server (`sa-mcp`) and the in-game bridge (`sa-bridge`).
//!
//! Transport: TCP on `127.0.0.1:BRIDGE_PORT`, one JSON object per line in each direction.
//! Every request gets exactly one response line, in order.

use serde::{Deserialize, Serialize};

pub const BRIDGE_PORT: u16 = 47311;

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
