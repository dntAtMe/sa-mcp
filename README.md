# sa-mcp

An [MCP](https://modelcontextprotocol.io) server that lets AI agents run, inspect and drive
**GTA San Andreas** (PC, 1.0 US) clients: launch several instances that boot straight into an empty
world, inject controller input, run SCM opcodes, record movement, compare clients for desync, read
state, poke memory and take screenshots.

Built as a dev tool for multiplayer framework work (SA-MP / MTA style): the agent can observe and
exercise the game instead of guessing.

```
Agent ──stdio MCP──► sa-mcp.exe ──JSON lines, 127.0.0.1:47311+N──► sa_bridge.asi in gta_sa.exe #N
                         │                                            ├─ network thread: accept + parse
                         ├─ instance discovery (named mutexes)        ├─ Idle/FrontendIdle call-site hooks: requests run on the game thread
                         └─ screenshots via PrintWindow               └─ CPad::UpdatePads call-site hook: input injection
```

## Crates

| Crate | What |
|---|---|
| `crates/proto` | Request/response and boot-config types shared by both sides |
| `crates/sa-bridge` | `cdylib` → `sa_bridge.asi`. Call-site hooks (chaining to whatever was there, so it composes with other plugins); all game access happens on the game thread |
| `crates/sa-mcp` | stdio MCP server (hand-rolled JSON-RPC, no async runtime) |

## Requirements

- `gta_sa.exe` **1.0 US** (the bridge checks `*(u32*)0x82457C == 0x94BF` and refuses to hook anything else)
- An ASI loader (e.g. Silent's / Ultimate ASI Loader)
- Rust with the `i686-pc-windows-msvc` target: `rustup target add i686-pc-windows-msvc`
- Windowed mode (e.g. III.VC.SA.WindowedMode) for screenshots and side-by-side clients
- **No modloader if you want more than one instance**: modloader crashes every second
  `gta_sa.exe` during its startup (it dereferences another process's pointers from shared memory).
  Rename `modloader.asi` to `modloader.asi.off` if you don't need it.

## Setup

```powershell
$env:GTA_SA_DIR = "C:\path\to\GTA San Andreas"   # or setx GTA_SA_DIR ... to persist
./scripts/deploy.ps1                              # builds and copies sa_bridge.asi into the game folder
```

`.mcp.json` registers the server for Claude Code as `gta-sa` (it reads `GTA_SA_DIR`).
Other MCP clients: run `target/i686-pc-windows-msvc/release/sa-mcp.exe` over stdio.

Each instance writes `sa_bridge.N.log` next to `gta_sa.exe`, including register/stack dumps of the
first access violations if the game crashes.

Tested with: 1.0 US exe + Ultimate ASI Loader, SilentPatch, III.VC.SA.WindowedMode (modloader
single-instance only).

## Instances and fast boot

Every game with the bridge claims the first free slot N (0-7) through the named mutex
`Local\sa-mcp-bridge-N`, listens on port `47311 + N` and logs to `sa_bridge.N.log`. Per-instance
tools take an optional `instance` (default: lowest running).

The bridge always patches out the single-instance check. `launch_instances` passes a boot config
in the `SA_BRIDGE_BOOT` env var; defaults:

| Option | Default (launch_instances) | Effect |
|---|---|---|
| `pause_when_unfocused` | `false` | Off: the main loop keeps running and no pause menu opens when the window is in the background |
| `skip_intro` | `true` | Skip logo/title/intro movies |
| `auto_start` | `true` | Leave the main menu and start a game immediately |
| `mp_mode` | `true` | main.scm disabled after its loading tick; player dressed and moved to `spawn`, no traffic/peds/wanted level |
| `spawn` | Grove Street | `{x, y, z, heading}`; x is offset 2 m per instance |
| `time`, `weather` | unset | `[h, m]`, weather id (mp mode) |

Two clients are in the world about 10 s after `launch_instances {count: 2}`. Without the env var
(normal launch) only the multi-instance and background patches apply.

## Tools

| Tool | Description |
|---|---|
| `list_instances` | Running clients: slot, pid, port, game state, player position |
| `launch_instances` | Start N clients with a boot config (+ `env` / `env_per_instance`), wait until they are in game, tile windows |
| `stop_instances` | Terminate clients |
| `input` | Timed controller-input sequences (forward, sprint, jump, enter_exit, accelerate, steer_left, ... or raw `CControllerState` fields). No window focus needed |
| `input_clear` | Stop injected input |
| `run_script` | Execute SCM opcodes in a persistent script context (32 local vars kept between calls; created handles come back in vars) |
| `record` | Sample player position/heading/speed/health on several clients in parallel |
| `compare_instances` | Desync check: where does client A's player appear in client B. Exact via a plugin's net ids (splits network lag vs render error), else nearest-entity heuristic |
| `sync_trace` | `compare_instances` sampled over time: mean/p95/max error per pair, snap count, series |
| `plugin_query` | Read a plugin's debug state through its `sa_debug_json` export (see below) |
| `game_status` | Bridge, game version, hooks, frames pumped, pid, player present |
| `get_player_state` | Position, heading, health, armor, money, wanted level, interior, vehicle |
| `get_world_state` | Clock, weather, area, timer, game state |
| `list_vehicles` / `list_peds` | Pool contents, optional `radius` around the player |
| `teleport` | Move player (or their vehicle) |
| `set_player` | Health / armor / money |
| `set_time` / `set_weather` | World control |
| `read_memory` / `write_memory` | Raw, fault-safe (ReadProcessMemory) access; reads work during loads |
| `bridge_logs` | Ring buffer of bridge log lines |
| `screenshot` | PNG of the instance's back buffer, copied in-process right before Present (falls back to PrintWindow) |
| `server_start` / `server_stop` | Run the multiplayer server under development (`SA_MCP_SERVER_CMD`) |
| `server_status` / `server_packets` / `server_logs` | Players, RTT, counters, packet log, stdout |
| `server_netsim` | Simulated latency / jitter / loss on the server |
| `server_kick` | Kick a player |

## Developing a plugin against sa-mcp

- **`sa-sdk` crate**: addresses, fault-tolerant memory access, call-site hooks, SCM executor and
  world helpers. Use it from your own ASI: `sa-sdk = { git = "https://github.com/dntAtMe/sa-mcp" }`.
- **Debug export**: export `extern "C" fn sa_debug_json(buf: *mut u8, cap: u32) -> u32` (write UTF-8
  JSON into `buf`, return the full length needed). `plugin_query` calls it on the game thread.
  Include top-level `net_id` and `remotes: [{net_id, position, net_position, age_ms, ped_mode,
  snaps}]` and `compare_instances` / `sync_trace` match players exactly.
- **Server admin contract** (for `server_*`): loopback TCP (`SA_MCP_SERVER_ADMIN`, default
  127.0.0.1:7778), one JSON per line: `{"cmd":"status"|"netsim"|"packets"|"kick"|"shutdown", ...}`
  answered with `{"ok":true,"data":...}` or `{"ok":false,"error":"..."}`.

[mini-samp](https://github.com/dntAtMe/minisamp) (branch `rust-multiplayer`) implements all three.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `no game instance ... is running` / `cannot reach bridge` | Game not running, `.asi` not loaded (check for `sa_bridge.N.log`), or the port is taken |
| `timed out waiting for game thread` | Game is loading or frozen. `read_memory`, `game_status` and `bridge_logs` still work |
| Second instance exits immediately | modloader (see Requirements); check the log for an `AV at eip=` inside modloader.asi |
| `game window not found` (screenshot) | No visible window owned by that `gta_sa.exe` |
| Crash right after starting/loading a game | Check `sa_bridge.N.log` for an `AV at eip=...` dump; see `docs/addresses.md` for known conflicts |

## Adding a tool

1. Add a variant to `proto::Request`.
2. Handle it in `crates/sa-bridge/src/game.rs` (runs on the game thread).
3. Add a definition + mapping in `crates/sa-mcp/src/tools.rs`.
4. Put any new addresses in `crates/sa-bridge/src/addr.rs` and note how they were verified in `docs/addresses.md`.

Often a new capability is just a `run_script` call with the right opcode; check that first.

## Safety

The bridge listens on loopback only and has no authentication — anything on the machine can drive the
game while it runs. Do not ship it in builds for players.

## License

MIT
