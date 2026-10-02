# sa-mcp

An [MCP](https://modelcontextprotocol.io) server that lets AI agents inspect and control a running
**GTA San Andreas** (PC, 1.0 US) instance — read player/world state, list vehicles and peds,
teleport, change time/weather, poke memory, and take screenshots.

Built as a dev tool for multiplayer/mod work: the agent can observe the game instead of guessing.

```
Agent ──stdio MCP──► sa-mcp.exe ──JSON lines over 127.0.0.1:47311──► sa_bridge.asi (inside gta_sa.exe)
                         │                                              ├─ network thread: accept + parse
                         └─ screenshots via PrintWindow                 └─ Idle/FrontendIdle hooks: run requests on the game thread
```

## Crates

| Crate | What |
|---|---|
| `crates/proto` | Request/response types shared by both sides |
| `crates/sa-bridge` | `cdylib` → `sa_bridge.asi`. Hooks the `call Idle` / `call FrontendIdle` sites (chaining to whatever was there, so it composes with modloader/SilentPatch); all game access happens on the game thread |
| `crates/sa-mcp` | stdio MCP server (hand-rolled JSON-RPC, no async runtime) |

## Requirements

- `gta_sa.exe` **1.0 US** (the bridge checks `*(u32*)0x82457C == 0x94BF` and refuses to hook anything else)
- An ASI loader (e.g. Silent's ASI Loader / modloader)
- Rust with the `i686-pc-windows-msvc` target: `rustup target add i686-pc-windows-msvc`
- Windowed mode recommended (screenshots of exclusive-fullscreen D3D9 are unreliable)

## Setup

```powershell
$env:GTA_SA_DIR = "C:\path\to\GTA San Andreas"   # or setx GTA_SA_DIR ... to persist
./scripts/deploy.ps1                              # builds and copies sa_bridge.asi into the game folder
```

`.mcp.json` registers the server for Claude Code as `gta-sa` (it reads `GTA_SA_DIR` for `launch_game`).
Other MCP clients: run `target/i686-pc-windows-msvc/release/sa-mcp.exe` over stdio.

The bridge writes `sa_bridge.log` next to `gta_sa.exe`, including register/stack dumps of the first
access violations if the game crashes.

Tested with: 1.0 US exe + Silent's ASI Loader, SilentPatch, modloader, III.VC.SA.WindowedMode.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `cannot reach sa-bridge` | Game not running, `.asi` not loaded (check for `sa_bridge.log`), or port 47311 taken |
| `timed out waiting for game thread` | Game is in intro movies, loading, frozen or minimized. `read_memory`, `game_status` and `bridge_logs` still work |
| `game window not found` (screenshot) | No visible window owned by `gta_sa.exe` |
| Crash right after starting/loading a game | Check `sa_bridge.log` for an `AV at eip=...` dump; see `docs/addresses.md` for known conflicts |

## Tools

| Tool | Description |
|---|---|
| `game_status` | Bridge reachable, game version, hooks installed, frames pumped, player present |
| `get_player_state` | Position, heading, health, armor, money, wanted level, interior, vehicle |
| `get_world_state` | Clock, weather, area, timer, game state |
| `list_vehicles` / `list_peds` | Pool contents, optional `radius` around the player |
| `teleport` | Move player (or their vehicle) |
| `set_player` | Health / armor / money |
| `set_time` / `set_weather` | World control |
| `read_memory` / `write_memory` | Raw, fault-safe (ReadProcessMemory) access |
| `bridge_logs` | Ring buffer of bridge log lines |
| `screenshot` | PNG of the game window (downscaled to 960px wide by default) |
| `launch_game` | Starts `gta_sa.exe` from `GTA_SA_DIR` |

## Adding a tool

1. Add a variant to `proto::Request`.
2. Handle it in `crates/sa-bridge/src/game.rs` (runs on the game thread).
3. Add a definition + mapping in `crates/sa-mcp/src/tools.rs`.
4. Put any new addresses in `crates/sa-bridge/src/addr.rs` and note how they were verified in `docs/addresses.md`.

## Safety

The bridge listens on loopback only and has no authentication — anything on the machine can drive the
game while it runs. Do not ship it in builds for players.

## License

MIT
