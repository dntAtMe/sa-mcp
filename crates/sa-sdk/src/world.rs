//! Player, entity and pool helpers. Reads are fault-tolerant (return defaults on bad memory).

use crate::addr::*;
use crate::mem;

pub fn rd<T: Copy + Default>(address: u32) -> T {
    mem::read(address).unwrap_or_default()
}

/// Local player's CPed, or 0 while no game is running.
pub fn player_ped() -> u32 {
    rd::<u32>(PLAYERS)
}

pub fn entity_pos(e: u32) -> [f32; 3] {
    let m = rd::<u32>(e + ENT_MATRIX);
    let base = if m != 0 { m + MAT_POS } else { e + ENT_PLACEMENT };
    [rd(base), rd(base + 4), rd(base + 8)]
}

/// Heading in degrees, 0 = north (+Y), counter-clockwise, matching the game's convention.
pub fn entity_heading(e: u32) -> f32 {
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

/// CPhysical move speed converted to metres per second (the game stores units per 1/50 s).
pub fn move_speed(e: u32) -> [f32; 3] {
    let v: [f32; 3] = [rd(e + PHYS_MOVE_SPEED), rd(e + PHYS_MOVE_SPEED + 4), rd(e + PHYS_MOVE_SPEED + 8)];
    [v[0] * 50.0, v[1] * 50.0, v[2] * 50.0]
}

pub fn ped_health(ped: u32) -> f32 {
    rd(ped + PED_HEALTH)
}

pub fn ped_armor(ped: u32) -> f32 {
    rd(ped + PED_ARMOR)
}

pub fn ped_in_vehicle(ped: u32) -> bool {
    rd::<u32>(ped + PED_FLAGS) & (1 << 8) != 0 && rd::<u32>(ped + PED_VEHICLE) != 0
}

pub fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Live slots of a CPool, as object addresses.
pub fn pool_iter(pool_ptr_addr: u32, obj_size: u32) -> Vec<u32> {
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

/// CPed for a script char handle (as returned by create_char), or 0 if the slot is free or
/// was reused. Handle = (slot << 8) | slot flags byte.
pub fn ped_from_handle(handle: u32) -> u32 {
    let pool = rd::<u32>(PED_POOL);
    if pool == 0 || handle == 0 {
        return 0;
    }
    let slot = handle >> 8;
    let size = rd::<i32>(pool + 8).max(0) as u32;
    if slot >= size {
        return 0;
    }
    let flag: u8 = rd(rd::<u32>(pool + 4) + slot);
    if flag != (handle & 0xFF) as u8 {
        return 0;
    }
    rd::<u32>(pool) + slot * PED_SIZE
}
