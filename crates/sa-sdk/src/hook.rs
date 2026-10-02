//! Call-site hooks.
//!
//! Rewrites the rel32 of a `call` instruction and returns its previous target, which the
//! detour must call to chain. Unlike prologue detours this composes with other plugins in any
//! load order: whoever hooks later simply chains to whoever hooked earlier.

use crate::mem;

/// Redirects the `call rel32` (E8) at `site` to `detour`. Returns the previous call target.
///
/// # Safety
/// `detour` must have the same calling convention and signature as the original callee and
/// must call the returned target to keep the game working.
pub unsafe fn hook_call(site: u32, detour: usize) -> Result<usize, String> {
    let opcode = mem::read::<u8>(site).ok_or_else(|| format!("cannot read {site:#x}"))?;
    if opcode != 0xE8 {
        return Err(format!("expected call (E8) at {site:#x}, found {opcode:#04x}"));
    }
    let rel = mem::read::<i32>(site + 1).ok_or_else(|| format!("cannot read {:#x}", site + 1))?;
    let previous = (site + 5).wrapping_add(rel as u32);
    let new_rel = (detour as u32).wrapping_sub(site + 5);
    mem::write_bytes(site + 1, &new_rel.to_le_bytes())?;
    Ok(previous as usize)
}
