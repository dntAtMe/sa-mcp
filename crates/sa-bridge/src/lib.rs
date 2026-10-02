//! `sa_bridge.asi` — loaded into gta_sa.exe (1.0 US) by an ASI loader.
//!
//! Threading model:
//! - A network thread accepts connections on 127.0.0.1 and parses requests.
//! - Requests that touch game state are queued and executed on the game thread
//!   from hooks on `Idle` (in-game frames) and `FrontendIdle` (menu frames).
//! - The network thread waits for the result and writes it back.

mod addr;
mod game;
mod log;
mod mem;
mod server;

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use minhook::MinHook;
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
static ORIG_IDLE: OnceLock<IdleFn> = OnceLock::new();
static ORIG_FRONTEND_IDLE: OnceLock<IdleFn> = OnceLock::new();

fn pump_jobs() {
    FRAMES_PUMPED.fetch_add(1, Ordering::Relaxed);
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
    pump_jobs();
    (ORIG_IDLE.get().unwrap())(arg)
}

unsafe extern "C" fn hk_frontend_idle(arg: *mut c_void) {
    pump_jobs();
    (ORIG_FRONTEND_IDLE.get().unwrap())(arg)
}

unsafe fn install_hooks() -> Result<(), String> {
    let hook = |target: u32, detour: IdleFn, slot: &OnceLock<IdleFn>| -> Result<(), String> {
        let tramp = MinHook::create_hook(target as *mut c_void, detour as *mut c_void)
            .map_err(|e| format!("create_hook {target:#x}: {e:?}"))?;
        let _ = slot.set(std::mem::transmute::<*mut c_void, IdleFn>(tramp));
        Ok(())
    };
    hook(addr::IDLE, hk_idle, &ORIG_IDLE)?;
    hook(addr::FRONTEND_IDLE, hk_frontend_idle, &ORIG_FRONTEND_IDLE)?;
    MinHook::enable_all_hooks().map_err(|e| format!("enable_all_hooks: {e:?}"))?;
    Ok(())
}

#[no_mangle]
unsafe extern "system" fn DllMain(_module: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        log::init();
        let supported = mem::read::<u32>(addr::VERSION_CHECK) == Some(addr::VERSION_CHECK_US10);
        SUPPORTED.store(supported, Ordering::SeqCst);
        if supported {
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
        std::thread::spawn(server::run);
    }
    TRUE
}
