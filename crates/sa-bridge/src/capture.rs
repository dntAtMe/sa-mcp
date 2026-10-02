//! In-process screenshots: hooks IDirect3DDevice9::Present (vtable slot 17 of the device
//! RenderWare uses) and copies the back buffer right before it is presented.
//!
//! GDI capture (PrintWindow / BitBlt) of a windowed D3D9 game can return plain white when DWM
//! is not redirecting the window's surface; reading the back buffer works regardless of focus,
//! occlusion or compositor mode.

use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use proto::Response;
use serde_json::json;
use windows::core::{Interface, HRESULT};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct3D9::*;
use windows::Win32::Graphics::Gdi::RGNDATA;

use crate::{log, mem};

/// RwD3D9Device: `IDirect3DDevice9*` used by RenderWare.
const RW_D3D9_DEVICE: u32 = 0xC97C28;
const PRESENT_SLOT: u32 = 17;

type PresentFn = unsafe extern "system" fn(*mut c_void, *const RECT, *const RECT, HWND, *const RGNDATA) -> HRESULT;

static ORIG_PRESENT: OnceLock<usize> = OnceLock::new();
static PENDING: Mutex<Vec<(u32, Sender<Response>)>> = Mutex::new(Vec::new());

/// Game thread: installs the Present hook once the device exists.
pub fn ensure_hook() {
    if ORIG_PRESENT.get().is_some() {
        return;
    }
    let Some(device) = mem::read::<u32>(RW_D3D9_DEVICE).filter(|d| *d != 0) else { return };
    let Some(vtable) = mem::read::<u32>(device) else { return };
    let slot = vtable + PRESENT_SLOT * 4;
    let Some(previous) = mem::read::<u32>(slot) else { return };
    if ORIG_PRESENT.set(previous as usize).is_err() {
        return;
    }
    let detour = hk_present as *const () as usize as u32;
    match mem::write(slot, detour) {
        Ok(()) => log::write(&format!("hooked Present: vtable {vtable:#x} slot 17 (previous {previous:#x})")),
        Err(e) => log::write(&format!("Present hook failed: {e}")),
    }
}

pub fn request(max_width: u32, reply: Sender<Response>) {
    PENDING.lock().unwrap().push((max_width, reply));
}

pub fn hooked() -> bool {
    ORIG_PRESENT.get().is_some()
}

unsafe extern "system" fn hk_present(
    this: *mut c_void,
    src: *const RECT,
    dst: *const RECT,
    hwnd: HWND,
    dirty: *const RGNDATA,
) -> HRESULT {
    let pending: Vec<_> = match PENDING.try_lock() {
        Ok(mut p) if !p.is_empty() => p.drain(..).collect(),
        _ => Vec::new(),
    };
    for (max_width, reply) in pending {
        let resp = match grab(this, max_width) {
            Ok(v) => Response::ok(v),
            Err(e) => Response::err(format!("back buffer capture failed: {e}")),
        };
        let _ = reply.send(resp);
    }
    let orig: PresentFn = std::mem::transmute(*ORIG_PRESENT.get().unwrap());
    orig(this, src, dst, hwnd, dirty)
}

unsafe fn grab(raw_device: *mut c_void, max_width: u32) -> windows::core::Result<serde_json::Value> {
    let device = IDirect3DDevice9::from_raw_borrowed(&raw_device).ok_or_else(|| windows::core::Error::from(HRESULT(-1)))?;
    let rt = device.GetRenderTarget(0)?;
    let mut desc = D3DSURFACE_DESC::default();
    rt.GetDesc(&mut desc)?;
    if desc.Format != D3DFMT_X8R8G8B8 && desc.Format != D3DFMT_A8R8G8B8 {
        return Err(windows::core::Error::new(HRESULT(-1), format!("unsupported back buffer format {:?}", desc.Format)));
    }

    // GetRenderTargetData needs a non-multisampled source.
    let source = if desc.MultiSampleType != D3DMULTISAMPLE_NONE {
        let mut resolved: Option<IDirect3DSurface9> = None;
        device.CreateRenderTarget(desc.Width, desc.Height, desc.Format, D3DMULTISAMPLE_NONE, 0, false, &mut resolved, std::ptr::null_mut())?;
        let resolved = resolved.unwrap();
        device.StretchRect(&rt, std::ptr::null(), &resolved, std::ptr::null(), D3DTEXF_NONE)?;
        resolved
    } else {
        rt.clone()
    };

    let mut sys: Option<IDirect3DSurface9> = None;
    device.CreateOffscreenPlainSurface(desc.Width, desc.Height, desc.Format, D3DPOOL_SYSTEMMEM, &mut sys, std::ptr::null_mut())?;
    let sys = sys.unwrap();
    device.GetRenderTargetData(&source, &sys)?;

    let mut locked = D3DLOCKED_RECT::default();
    sys.LockRect(&mut locked, std::ptr::null(), D3DLOCK_READONLY as u32)?;
    let (w, h) = (desc.Width, desc.Height);
    let (ow, oh) = if max_width > 0 && w > max_width { (max_width, (h as u64 * max_width as u64 / w as u64).max(1) as u32) } else { (w, h) };
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
    for y in 0..oh {
        let sy = (y as u64 * h as u64 / oh as u64) as usize;
        let row = (locked.pBits as *const u8).add(sy * locked.Pitch as usize);
        for x in 0..ow {
            let sx = (x as u64 * w as u64 / ow as u64) as usize;
            out.extend_from_slice(std::slice::from_raw_parts(row.add(sx * 4), 4));
        }
    }
    let _ = sys.UnlockRect();
    Ok(json!({
        "width": ow,
        "height": oh,
        "source_width": w,
        "source_height": h,
        "bgra_b64": base64::engine::general_purpose::STANDARD.encode(&out),
    }))
}
