//! `sa_bridge.asi`: loaded into gta_sa.exe (1.0 US) by an ASI loader.
//!
//! Threading model:
//! - A network thread accepts connections on 127.0.0.1 and parses requests.
//! - Requests that touch game state are queued and executed on the game thread
//!   from hooks on the `call Idle` (in-game frames) and `call FrontendIdle` (menu frames)
//!   sites in RsEventHandler.
//! - The network thread waits for the result and writes it back.
//!
//! Why call-site hooks instead of detouring function prologues: other plugins
//! (SilentPatch, modloader) patch instructions inside Idle's first bytes and hook the same
//! call sites. Rewriting a call's rel32 and chaining to whatever it pointed at before is the
//! GTA modding convention and composes with them in any load order.

mod addr;
mod boot;
mod crash;
mod game;
mod instance;
mod log;
mod mem;
mod pad;
mod record;
mod script;
mod server;

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use proto::{Request, Response};
use windows::Win32::Foundation::{BOOL, HMODULE, TRUE};
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

pub(crate) struct Job {
    pub req: Request,
    pub reply: Sender<Response>,
}

pub(crate) static JOBS: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
/// Incremented once per game-thread frame; lets `status` tell whether the loop is alive.
pub(crate) static FRAMES_PUMPED: AtomicU32 = AtomicU32::new(0);
pub(crate) static SUPPORTED: AtomicBool = AtomicBool::new(false);
pub(crate) static HOOKED: AtomicBool = AtomicBool::new(false);

type IdleFn = unsafe extern "C" fn(*mut c_void);
type UpdatePadsFn = unsafe extern "C" fn();
/// Previous call targets, stored as addresses.
static ORIG_IDLE: OnceLock<usize> = OnceLock::new();
static ORIG_FRONTEND_IDLE: OnceLock<usize> = OnceLock::new();
static ORIG_UPDATE_PADS: OnceLock<usize> = OnceLock::new();

fn pump_jobs() {
    if FRAMES_PUMPED.fetch_add(1, Ordering::Relaxed) == 0 {
        log::write(&format!("game thread id {}", unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }));
    }
    let jobs: Vec<Job> = match JOBS.lock() {
        Ok(mut q) if !q.is_empty() => q.drain(..).collect(),
        _ => return,
    };
    for job in jobs {
        let resp = unsafe { game::handle(&job.req) };
        let _ = job.reply.send(resp);
    }
}

unsafe extern "C" fn hk_idle(arg: *mut c_void) {
    boot::on_game_frame();
    pump_jobs();
    record::tick();
    let orig: IdleFn = std::mem::transmute(*ORIG_IDLE.get().unwrap());
    orig(arg)
}

unsafe extern "C" fn hk_frontend_idle(arg: *mut c_void) {
    boot::on_menu_frame();
    pump_jobs();
    let orig: IdleFn = std::mem::transmute(*ORIG_FRONTEND_IDLE.get().unwrap());
    orig(arg)
}

unsafe extern "C" fn hk_update_pads() {
    let orig: UpdatePadsFn = std::mem::transmute(*ORIG_UPDATE_PADS.get().unwrap());
    orig();
    pad::apply();
}

/// Redirects the `call rel32` at `site` to `detour`, storing the previous target in `slot`.
unsafe fn hook_call_site(site: u32, detour: usize, slot: &OnceLock<usize>) -> Result<(), String> {
    let opcode = mem::read::<u8>(site).ok_or_else(|| format!("cannot read {site:#x}"))?;
    if opcode != 0xE8 {
        return Err(format!("expected call (E8) at {site:#x}, found {opcode:#04x}"));
    }
    let rel = mem::read::<i32>(site + 1).ok_or_else(|| format!("cannot read {:#x}", site + 1))?;
    let previous = (site + 5).wrapping_add(rel as u32);
    let _ = slot.set(previous as usize);
    let new_rel = (detour as u32).wrapping_sub(site + 5);
    mem::write_bytes(site + 1, &new_rel.to_le_bytes())?;
    log::write(&format!("hooked call at {site:#x} (previous target {previous:#x})"));
    Ok(())
}

unsafe fn install_hooks() -> Result<(), String> {
    hook_call_site(addr::CALL_IDLE, hk_idle as *const () as usize, &ORIG_IDLE)?;
    hook_call_site(addr::CALL_FRONTEND_IDLE, hk_frontend_idle as *const () as usize, &ORIG_FRONTEND_IDLE)?;
    hook_call_site(addr::CALL_UPDATE_PADS, hk_update_pads as *const () as usize, &ORIG_UPDATE_PADS)?;
    Ok(())
}

#[no_mangle]
unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        let slot = instance::claim();
        log::init(slot);
        log::write(&format!("module base {:#x}, instance {slot:?}", module.0 as usize));
        crash::install();
        let supported = mem::read::<u32>(addr::VERSION_CHECK) == Some(addr::VERSION_CHECK_US10);
        SUPPORTED.store(supported, Ordering::SeqCst);
        if supported {
            boot::apply_patches();
            match install_hooks() {
                Ok(()) => {
                    HOOKED.store(true, Ordering::SeqCst);
                    log::write("hooks installed");
                }
                Err(e) => log::write(&format!("hook install failed: {e}")),
            }
        } else {
            log::write("unsupported gta_sa.exe (need 1.0 US); serving status only");
        }
        match slot {
            Some(n) => {
                std::thread::spawn(move || server::run(proto::BASE_PORT + n as u16));
            }
            None => log::write("all instance slots taken; bridge server not started"),
        }
    }
    TRUE
}
