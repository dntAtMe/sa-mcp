//! Request handlers. Everything here runs on the game thread (see `pump_jobs`),
//! except `ReadMemory`, which the network thread serves directly.

use proto::{Request, Response};
use serde_json::{json, Value};

use sa_sdk::addr::*;
use sa_sdk::mem;
use sa_sdk::world::*;

fn wr<T: Copy>(address: u32, value: T) -> Result<(), String> {
    mem::write(address, value)
}

fn round2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}

fn pos_json(p: [f32; 3]) -> Value {
    json!({ "x": round2(p[0]), "y": round2(p[1]), "z": round2(p[2]) })
}

fn require_player() -> Result<u32, String> {
    match player_ped() {
        0 => Err("no player ped (game not started / still loading?)".into()),
        p => Ok(p),
    }
}

pub unsafe fn handle(req: &Request) -> Response {
    match dispatch(req) {
        Ok(v) => Response::ok(v),
        Err(e) => Response::err(e),
    }
}

fn dispatch(req: &Request) -> Result<Value, String> {
    match req {
        Request::Status | Request::Logs | Request::Record { .. } | Request::Screenshot { .. } => Err("handled on network thread".into()),

        Request::PlayerState => {
            let ped = require_player()?;
            let veh = rd::<u32>(ped + PED_VEHICLE);
            let in_vehicle = ped_in_vehicle(ped);
            let wanted = rd::<u32>(PLAYER_WANTED_PTR);
            Ok(json!({
                "ped": format!("{ped:#x}"),
                "position": pos_json(entity_pos(ped)),
                "heading": round2(entity_heading(ped)),
                "health": round2(rd(ped + PED_HEALTH)),
                "max_health": round2(rd(ped + PED_MAX_HEALTH)),
                "armor": round2(rd(ped + PED_ARMOR)),
                "money": rd::<i32>(PLAYER_MONEY),
                "wanted_level": if wanted != 0 { rd::<u32>(wanted + WANTED_LEVEL) } else { 0 },
                "model": rd::<u16>(ped + ENT_MODEL),
                "area": rd::<u8>(ped + ENT_AREA),
                "in_vehicle": in_vehicle,
                "vehicle": (in_vehicle).then(|| json!({
                    "ptr": format!("{veh:#x}"),
                    "model": rd::<u16>(veh + ENT_MODEL),
                    "health": round2(rd(veh + VEH_HEALTH)),
                    "is_driver": rd::<u32>(veh + VEH_DRIVER) == ped,
                })),
            }))
        }

        Request::WorldState => Ok(json!({
            "clock": format!("{:02}:{:02}", rd::<u8>(CLOCK_HOURS), rd::<u8>(CLOCK_MINUTES)),
            "weather": {
                "old": rd::<i16>(WEATHER_OLD),
                "new": rd::<i16>(WEATHER_NEW),
                "forced": rd::<i16>(WEATHER_FORCED),
            },
            "current_area": rd::<i32>(CURR_AREA),
            "game_time_ms": rd::<u32>(TIMER_MS),
            "frame_counter": rd::<u32>(FRAME_COUNTER),
            "game_state": rd::<i32>(GAME_STATE),
        })),

        Request::ListVehicles { radius } => {
            let origin = player_ped();
            let origin = (origin != 0).then(|| entity_pos(origin));
            let items: Vec<Value> = pool_iter(VEHICLE_POOL, VEHICLE_SIZE)
                .into_iter()
                .filter_map(|v| {
                    let p = entity_pos(v);
                    let d = origin.map(|o| dist(o, p));
                    if let (Some(r), Some(d)) = (radius, d) {
                        if d > *r {
                            return None;
                        }
                    }
                    Some(json!({
                        "ptr": format!("{v:#x}"),
                        "model": rd::<u16>(v + ENT_MODEL),
                        "position": pos_json(p),
                        "heading": round2(entity_heading(v)),
                        "health": round2(rd(v + VEH_HEALTH)),
                        "has_driver": rd::<u32>(v + VEH_DRIVER) != 0,
                        "distance": d.map(round2),
                    }))
                })
                .collect();
            Ok(json!({ "count": items.len(), "vehicles": items }))
        }

        Request::ListPeds { radius } => {
            let player = player_ped();
            let origin = (player != 0).then(|| entity_pos(player));
            let items: Vec<Value> = pool_iter(PED_POOL, PED_SIZE)
                .into_iter()
                .filter_map(|ped| {
                    let p = entity_pos(ped);
                    let d = origin.map(|o| dist(o, p));
                    if let (Some(r), Some(d)) = (radius, d) {
                        if d > *r {
                            return None;
                        }
                    }
                    Some(json!({
                        "ptr": format!("{ped:#x}"),
                        "model": rd::<u16>(ped + ENT_MODEL),
                        "position": pos_json(p),
                        "health": round2(rd(ped + PED_HEALTH)),
                        "is_player": ped == player,
                        "in_vehicle": ped_in_vehicle(ped),
                        "distance": d.map(round2),
                    }))
                })
                .collect();
            Ok(json!({ "count": items.len(), "peds": items }))
        }

        Request::Teleport { x, y, z } => {
            let ped = require_player()?;
            let target = if ped_in_vehicle(ped) { rd::<u32>(ped + PED_VEHICLE) } else { ped };
            unsafe { teleport_entity(target, [*x, *y, *z])? };
            Ok(json!({ "moved": if target == ped { "ped" } else { "vehicle" }, "position": pos_json(entity_pos(target)) }))
        }

        Request::SetPlayer { health, armor, money } => {
            let ped = require_player()?;
            if let Some(h) = health {
                wr(ped + PED_HEALTH, *h)?;
            }
            if let Some(a) = armor {
                wr(ped + PED_ARMOR, *a)?;
            }
            if let Some(m) = money {
                wr(PLAYER_MONEY, *m)?;
                wr(PLAYER_DISPLAY_MONEY, *m)?;
            }
            Ok(json!({
                "health": round2(rd(ped + PED_HEALTH)),
                "armor": round2(rd(ped + PED_ARMOR)),
                "money": rd::<i32>(PLAYER_MONEY),
            }))
        }

        Request::SetTime { hour, minute } => {
            if *hour > 23 || *minute > 59 {
                return Err("hour must be 0-23, minute 0-59".into());
            }
            wr(CLOCK_HOURS, *hour)?;
            wr(CLOCK_MINUTES, *minute)?;
            Ok(json!({ "clock": format!("{hour:02}:{minute:02}") }))
        }

        Request::SetWeather { id } => {
            if !(0..=45).contains(id) {
                return Err("weather id must be 0-45".into());
            }
            for a in [WEATHER_FORCED, WEATHER_OLD, WEATHER_NEW] {
                wr(a, *id)?;
            }
            Ok(json!({ "weather": id }))
        }

        Request::ReadMemory { address, length } => {
            if *length == 0 || *length > 4096 {
                return Err("length must be 1-4096".into());
            }
            let bytes = mem::read_bytes(*address, *length as usize)
                .ok_or_else(|| format!("cannot read {length} bytes at {address:#x}"))?;
            Ok(json!({ "address": format!("{address:#x}"), "hex": to_hex(&bytes) }))
        }

        Request::WriteMemory { address, bytes_hex } => {
            let bytes = from_hex(bytes_hex)?;
            mem::write_bytes(*address, &bytes)?;
            Ok(json!({ "address": format!("{address:#x}"), "written": bytes.len() }))
        }

        Request::RunScript { commands } => unsafe { crate::script::run(commands) },

        Request::Input { steps, append } => {
            let total = crate::pad::set(steps.clone(), *append);
            Ok(json!({ "queued_ms": total, "pending_ms": crate::pad::pending_ms() }))
        }

        Request::PluginQuery { module, export } => unsafe { plugin_query(module, export) },

        Request::InputClear => {
            crate::pad::clear();
            Ok(json!({ "cleared": true }))
        }
    }
}

/// [x, y, z, heading, speed m/s, health, in_vehicle, frame] for the recorder.
pub fn sample_player() -> Option<[f64; 8]> {
    let ped = player_ped();
    if ped == 0 {
        return None;
    }
    let in_vehicle = ped_in_vehicle(ped);
    let body = if in_vehicle { rd::<u32>(ped + PED_VEHICLE) } else { ped };
    let p = entity_pos(body);
    let v: [f32; 3] = [rd(body + PHYS_MOVE_SPEED), rd(body + PHYS_MOVE_SPEED + 4), rd(body + PHYS_MOVE_SPEED + 8)];
    // Move speed is in units per 1/50 s.
    let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() * 50.0;
    Some([
        p[0] as f64,
        p[1] as f64,
        p[2] as f64,
        entity_heading(body) as f64,
        speed as f64,
        rd::<f32>(ped + PED_HEALTH) as f64,
        in_vehicle as u8 as f64,
        rd::<u32>(FRAME_COUNTER) as f64,
    ])
}

/// See `Request::PluginQuery` for the export contract.
unsafe fn plugin_query(module: &str, export: &str) -> Result<Value, String> {
    use windows::core::{HSTRING, PCSTR};
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    type DebugFn = unsafe extern "C" fn(*mut u8, u32) -> u32;

    let hmod = GetModuleHandleW(&HSTRING::from(module)).map_err(|_| format!("module {module:?} is not loaded"))?;
    let name = std::ffi::CString::new(export).map_err(|e| e.to_string())?;
    let f = GetProcAddress(hmod, PCSTR(name.as_ptr() as *const u8)).ok_or_else(|| format!("{module} has no export {export:?}"))?;
    let f: DebugFn = std::mem::transmute(f);
    let mut buf = vec![0u8; 16 * 1024];
    let mut len = f(buf.as_mut_ptr(), buf.len() as u32) as usize;
    if len > buf.len() {
        buf.resize(len, 0);
        len = f(buf.as_mut_ptr(), buf.len() as u32) as usize;
    }
    let text = String::from_utf8_lossy(&buf[..len.min(buf.len())]);
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.into_owned())))
}

unsafe fn teleport_entity(e: u32, pos: [f32; 3]) -> Result<(), String> {
    type WorldFn = unsafe extern "C" fn(u32);
    let remove: WorldFn = std::mem::transmute(CWORLD_REMOVE as usize);
    let add: WorldFn = std::mem::transmute(CWORLD_ADD as usize);

    remove(e);
    let m = rd::<u32>(e + ENT_MATRIX);
    let base = if m != 0 { m + MAT_POS } else { e + ENT_PLACEMENT };
    for (i, v) in pos.iter().enumerate() {
        wr(base + i as u32 * 4, *v)?;
    }
    for off in [PHYS_MOVE_SPEED, PHYS_TURN_SPEED] {
        for i in 0..3 {
            wr(e + off + i * 4, 0f32)?;
        }
    }
    add(e);
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.is_empty() || clean.len() % 2 != 0 {
        return Err("bytes_hex must be an even number of hex digits".into());
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}
