//! Multi-instance tools: discovery, launching, stopping, recording and desync comparison.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use proto::{BootConfig, Request, BOOT_ENV};
use serde_json::{json, Map, Value};

use crate::{bridge, window};

const LAUNCH_TIMEOUT: Duration = Duration::from_secs(90);

fn status_of(n: u8) -> Option<Value> {
    bridge::query(Some(n), &Request::Status).ok()
}

pub fn list() -> Value {
    let items: Vec<Value> = bridge::live_instances()
        .into_iter()
        .map(|n| {
            let mut entry = json!({ "instance": n, "port": proto::BASE_PORT + n as u16 });
            match status_of(n) {
                Some(s) => {
                    for k in ["pid", "game_state", "frames_pumped", "player_ped", "input_pending_ms"] {
                        entry[k] = s.get(k).cloned().unwrap_or(Value::Null);
                    }
                    if let Ok(p) = bridge::query(Some(n), &Request::PlayerState) {
                        entry["position"] = p["position"].clone();
                        entry["in_vehicle"] = p["in_vehicle"].clone();
                    }
                }
                None => entry["error"] = json!("bridge not answering"),
            }
            entry
        })
        .collect();
    json!({ "count": items.len(), "instances": items })
}

fn default_boot() -> BootConfig {
    BootConfig { skip_intro: true, auto_start: true, mp_mode: true, ..Default::default() }
}

pub fn launch(args: &Value) -> Result<Value, String> {
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(1).clamp(1, proto::MAX_INSTANCES as u64) as usize;
    let dir = std::env::var("GTA_SA_DIR").map_err(|_| "GTA_SA_DIR env var is not set (configure it in .mcp.json)")?;
    let exe = std::path::Path::new(&dir).join("gta_sa.exe");

    // Merge overrides onto the default boot config via JSON so unknown keys are rejected loudly.
    let mut boot = serde_json::to_value(default_boot()).unwrap();
    if let Some(over) = args.get("boot").and_then(Value::as_object) {
        for (k, v) in over {
            if boot.get(k).is_none() {
                return Err(format!("unknown boot option {k:?}"));
            }
            boot[k] = v.clone();
        }
    }
    let boot: BootConfig = serde_json::from_value(boot).map_err(|e| format!("bad boot config: {e}"))?;
    let boot_json = serde_json::to_string(&boot).unwrap();

    let free = proto::MAX_INSTANCES as usize - bridge::live_instances().len();
    if count > free {
        return Err(format!("only {free} instance slots free"));
    }

    let env_all = args.get("env").and_then(Value::as_object).cloned().unwrap_or_default();
    let env_each = args.get("env_per_instance").and_then(Value::as_array).cloned().unwrap_or_default();
    let env_str = |v: &Value| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());

    let mut pids = Vec::new();
    for i in 0..count {
        let mut cmd = Command::new(&exe);
        for (k, v) in env_all.iter().chain(env_each.get(i).and_then(Value::as_object).into_iter().flatten()) {
            cmd.env(k, env_str(v));
        }
        // Detach stdio: the game must not inherit our stdout, which is the MCP channel.
        let child = cmd
            .current_dir(&dir)
            .env(BOOT_ENV, &boot_json)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to start {}: {e}", exe.display()))?;
        pids.push(child.id());
        // Start clients one at a time: plugins such as modloader share on-disk caches and
        // crash when two processes initialise concurrently. The game thread pumping frames
        // means plugin init is over.
        wait_until_pumping(child.id(), Duration::from_secs(30));
    }

    // Wait for each process's bridge to come up, then (with auto_start) for its player.
    let deadline = Instant::now() + LAUNCH_TIMEOUT;
    let mut found: Map<String, Value> = Map::new();
    let want_player = boot.auto_start;
    while Instant::now() < deadline {
        for n in bridge::live_instances() {
            if let Some(s) = status_of(n) {
                let pid = s.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
                let ready = !want_player || s.get("player_ped").and_then(Value::as_str).is_some_and(|p| p != "0x0");
                if pids.contains(&pid) && ready {
                    found.insert(pid.to_string(), json!(n));
                }
            }
        }
        let dead = pids.iter().filter(|p| !found.contains_key(&p.to_string()) && !window::is_game_running(**p)).count();
        if found.len() + dead == pids.len() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let tiled = if args.get("tile").and_then(Value::as_bool).unwrap_or(true) { window::tile(&pids) } else { 0 };
    let started: Vec<Value> = pids
        .iter()
        .map(|pid| {
            json!({
                "pid": pid,
                "instance": found.get(&pid.to_string()).cloned().unwrap_or(Value::Null),
                "running": window::is_game_running(*pid),
            })
        })
        .collect();
    let mut out = json!({ "started": started, "boot": boot, "windows_tiled": tiled });
    if found.len() != pids.len() {
        out["warning"] = json!(format!(
            "only {}/{} instances became ready (exited ones crashed; see sa_bridge.N.log in GTA_SA_DIR)",
            found.len(),
            pids.len()
        ));
    }
    Ok(out)
}

fn wait_until_pumping(pid: u32, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && window::is_game_running(pid) {
        let pumping = bridge::live_instances().into_iter().filter_map(status_of).any(|s| {
            s.get("pid").and_then(Value::as_u64) == Some(pid as u64)
                && s.get("frames_pumped").and_then(Value::as_u64).unwrap_or(0) > 0
        });
        if pumping {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn targets(instances: Option<Vec<u8>>) -> Vec<u8> {
    instances.unwrap_or_else(bridge::live_instances)
}

pub fn stop(instances: Option<Vec<u8>>) -> Result<Value, String> {
    let mut results = Map::new();
    for n in targets(instances) {
        let r = match status_of(n).and_then(|s| s.get("pid").and_then(Value::as_u64)) {
            Some(pid) => window::kill_game(pid as u32).map(|_| json!({ "stopped": pid })),
            None => Err("bridge not answering; cannot determine pid".into()),
        };
        results.insert(n.to_string(), r.unwrap_or_else(|e| json!({ "error": e })));
    }
    Ok(Value::Object(results))
}

pub fn record(args: &Value) -> Result<Value, String> {
    let seconds = args.get("seconds").and_then(Value::as_f64).ok_or("seconds is required")? as f32;
    let hz = args.get("hz").and_then(Value::as_f64).unwrap_or(10.0) as f32;
    let list = targets(crate::tools::instances_arg(args));
    if list.is_empty() {
        return Err("no running instances".into());
    }
    let timeout = Duration::from_secs_f32(seconds + 10.0);
    let handles: Vec<_> = list
        .into_iter()
        .map(|n| {
            std::thread::spawn(move || {
                let r = bridge::send_timeout(Some(n), &Request::Record { seconds, hz }, timeout);
                let v = match r {
                    Ok(r) if r.ok => r.data.unwrap_or(Value::Null),
                    Ok(r) => json!({ "error": r.error }),
                    Err(e) => json!({ "error": e }),
                };
                (n, v)
            })
        })
        .collect();
    let mut out = Map::new();
    for h in handles {
        if let Ok((n, v)) = h.join() {
            out.insert(n.to_string(), v);
        }
    }
    Ok(Value::Object(out))
}

fn pos(v: &Value) -> Option<[f64; 3]> {
    Some([v.get("x")?.as_f64()?, v.get("y")?.as_f64()?, v.get("z")?.as_f64()?])
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Heuristic matching until the multiplayer layer exposes network ids: a remote player is
/// the nearest non-local ped, a remote vehicle the nearest one with the same model.
pub fn compare(instances: Option<Vec<u8>>) -> Result<Value, String> {
    struct Snap {
        n: u8,
        player: Value,
        peds: Vec<Value>,
        vehicles: Vec<Value>,
    }
    let mut snaps = Vec::new();
    for n in targets(instances) {
        let player = bridge::query(Some(n), &Request::PlayerState).map_err(|e| format!("instance {n}: {e}"))?;
        let peds = bridge::query(Some(n), &Request::ListPeds { radius: None }).map_err(|e| format!("instance {n}: {e}"))?;
        let vehicles = bridge::query(Some(n), &Request::ListVehicles { radius: None }).map_err(|e| format!("instance {n}: {e}"))?;
        snaps.push(Snap {
            n,
            player,
            peds: peds["peds"].as_array().cloned().unwrap_or_default(),
            vehicles: vehicles["vehicles"].as_array().cloned().unwrap_or_default(),
        });
    }
    if snaps.len() < 2 {
        return Err("need at least two running instances".into());
    }

    let mut pairs = Vec::new();
    for a in &snaps {
        let Some(pa) = pos(&a.player["position"]) else { continue };
        for b in snaps.iter().filter(|b| b.n != a.n) {
            let nearest_ped = b
                .peds
                .iter()
                .filter(|p| !p["is_player"].as_bool().unwrap_or(false))
                .filter_map(|p| pos(&p["position"]).map(|pp| (dist(pa, pp), p)))
                .min_by(|x, y| x.0.total_cmp(&y.0));
            let mut entry = json!({
                "a": a.n,
                "b": b.n,
                "a_player_position": a.player["position"],
                "remote_ped_in_b": nearest_ped.map(|(d, p)| json!({
                    "ptr": p["ptr"], "model": p["model"], "position": p["position"],
                    "distance": r2(d),
                    "health_diff": r2(p["health"].as_f64().unwrap_or(0.0) - a.player["health"].as_f64().unwrap_or(0.0)),
                })),
            });
            if a.player["in_vehicle"].as_bool().unwrap_or(false) {
                let model = &a.player["vehicle"]["model"];
                let mine = a.vehicles.iter().find(|v| v["ptr"] == a.player["vehicle"]["ptr"]);
                if let Some(av) = mine {
                    let apos = pos(&av["position"]).unwrap_or(pa);
                    let nearest_veh = b
                        .vehicles
                        .iter()
                        .filter(|v| &v["model"] == model)
                        .filter_map(|v| pos(&v["position"]).map(|vp| (dist(apos, vp), v)))
                        .min_by(|x, y| x.0.total_cmp(&y.0));
                    entry["remote_vehicle_in_b"] = json!(nearest_veh.map(|(d, v)| {
                        let dh = (v["heading"].as_f64().unwrap_or(0.0) - av["heading"].as_f64().unwrap_or(0.0) + 540.0) % 360.0 - 180.0;
                        json!({ "ptr": v["ptr"], "position": v["position"], "distance": r2(d), "heading_diff": r2(dh) })
                    }));
                }
            }
            pairs.push(entry);
        }
    }
    Ok(json!({
        "note": "Matching is nearest-entity heuristic (no network ids yet). Missing remote_ped_in_b means B shows no other ped at all.",
        "pairs": pairs,
    }))
}
