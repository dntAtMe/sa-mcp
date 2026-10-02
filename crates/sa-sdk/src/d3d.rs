//! The game's Direct3D 9 device and chain-friendly vtable hooks on it.
//!
//! Other plugins may have swapped the device's vtable for a heap copy (e.g. windowed-mode
//! plugins replace Reset); hooking a slot of whatever vtable the device currently has, and
//! calling the previous entry, composes with them.

use crate::mem;

/// [live] RwD3D9Device: `IDirect3DDevice9*` used by RenderWare (0 before the device exists).
pub const RW_D3D9_DEVICE: u32 = 0xC97C28;

/// IDirect3DDevice9 vtable slots.
pub const SLOT_RESET: u32 = 16;
pub const SLOT_PRESENT: u32 = 17;
pub const SLOT_END_SCENE: u32 = 42;

pub fn device() -> u32 {
    mem::read::<u32>(RW_D3D9_DEVICE).unwrap_or(0)
}

/// Points vtable slot `slot` of the current device at `detour`. Returns the previous entry,
/// which the detour must call. Fails while the device does not exist yet.
///
/// # Safety
/// `detour` must match the slot's signature (stdcall, `this` first).
pub unsafe fn hook_device_slot(slot: u32, detour: usize) -> Result<usize, String> {
    let dev = device();
    if dev == 0 {
        return Err("D3D device not created yet".into());
    }
    let vtable = mem::read::<u32>(dev).ok_or("cannot read device vtable")?;
    let entry = vtable + slot * 4;
    let previous = mem::read::<u32>(entry).ok_or("cannot read vtable slot")?;
    mem::write(entry, detour as u32)?;
    Ok(previous as usize)
}
