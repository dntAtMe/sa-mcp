//! Request handlers. Everything here runs on the game thread (see `pump_jobs`),
//! except `ReadMemory`, which the network thread serves directly.

use proto::{Request, Response};
use serde_json::{json, Value};

use crate::addr::*;
use crate::mem;

fn rd<T: Copy + Default>(address: u32) -> T {
    mem::read(address).unwrap_or_default()
}

fn wr<T: Copy>(address: u32, value: T) -> Result<(), String> {
    let bytes = unsafe {
        std::slice::from_raw_parts(&value as *const T as *const u8, std::mem::size_of::<T>())
    };
    mem::write_bytes(address, bytes)
}

fn player_ped() -> u32 {
    rd::<u32>(PLAYERS)
}

fn entity_pos(e: u32) -> [f32; 3] {
    let m = rd::<u32>(e + ENT_MATRIX);
    let base = if m != 0 { m + MAT_POS } else { e + ENT_PLACEMENT };
    [rd(base), rd(base + 4), rd(base + 8)]
}

/// Heading in degrees, 0 = north (+Y), counter-clockwise, matching the game's convention.
fn entity_heading(e: u32) -> f32 {
    let m = rd::<u32>(e + ENT_MATRIX);
    let rad = if m != 0 {
        let fx: f32 = rd(m + MAT_FORWARD);
        let fy: f32 = rd(m + MAT_FORWARD + 4);
        (-fx).atan2(fy)
    } else {
        rd(e + ENT_PLACEMENT + 0x0C)
    };
    rad.to_degrees().rem_euclid(360.0)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn round2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}

fn pos_json(p: [f32; 3]) -> Value {
    json!({ "x": round2(p[0]), "y": round2(p[1]), "z": round2(p[2]) })
}

fn ped_in_vehicle(ped: u32) -> bool {
    rd::<u32>(ped + PED_FLAGS) & (1 << 8) != 0 && rd::<u32>(ped + PED_VEHICLE) != 0
}

/// Iterate live slots of a CPool, yielding object addresses.
fn pool_iter(pool_ptr_addr: u32, obj_size: u32) -> Vec<u32> {
    let pool = rd::<u32>(pool_ptr_addr);
    if pool == 0 {
        return Vec::new();
    }
    let objects = rd::<u32>(pool);
    let byte_map = rd::<u32>(pool + 4);
    let size = rd::<i32>(pool + 8).max(0) as u32;
    let Some(flags) = mem::read_bytes(byte_map, size as usize) else { return Vec::new() };
    flags
        .iter()
        .enumerate()
        .filter(|(_, f)| *f & 0x80 == 0)
        .map(|(i, _)| objects + i as u32 * obj_size)
        .collect()
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
        Request::Status | Request::Logs => Err("handled on network thread".into()),

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
    }
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
