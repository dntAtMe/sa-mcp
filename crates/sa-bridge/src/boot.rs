//! Startup automation driven by the SA_BRIDGE_BOOT env var (JSON BootConfig):
//! multi-instance and run-in-background patches, movie skip, auto-start from the menu, and "mp mode" world setup.
//!
//! mp mode: main.scm runs exactly its loading-time tick (which creates the player), then
//! CTheScripts::Process is disabled on the first in-game frame and the bridge sets up the
//! world itself through SCM commands.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use proto::{BootConfig, Spawn, BOOT_ENV};
use sa_sdk::script::{self, cmd, Arg::*};

use crate::addr::*;
use crate::{instance, log, mem};

static CONFIG: OnceLock<BootConfig> = OnceLock::new();
static MENU_FRAMES: AtomicU32 = AtomicU32::new(0);
static GAME_FRAMES: AtomicU32 = AtomicU32::new(0);

/// Frames to wait in the menu / in game before acting, so the game finishes its own setup.
const MENU_SETTLE_FRAMES: u32 = 3;
const GAME_SETTLE_FRAMES: u32 = 10;

const DEFAULT_SPAWN: Spawn = Spawn { x: 2495.0, y: -1680.0, z: 13.4, heading: 0.0 };

pub fn config() -> &'static BootConfig {
    CONFIG.get_or_init(|| {
        std::env::var(BOOT_ENV)
            .ok()
            .and_then(|s| match serde_json::from_str(&s) {
                Ok(c) => Some(c),
                Err(e) => {
                    log::write(&format!("ignoring invalid {BOOT_ENV}: {e}"));
                    None
                }
            })
            .unwrap_or_default()
    })
}

/// Code patches; must run before WinMain (i.e. from DllMain).
pub fn apply_patches() {
    let cfg = config();
    // IsAlreadyRunning -> `xor eax, eax; ret`, so several clients can run side by side.
    patch(IS_ALREADY_RUNNING, &[0x31, 0xC0, 0xC3], "multi-instance");
    if !cfg.pause_when_unfocused {
        patch(MAINLOOP_BACKGROUND_JE, &[0x90; 6], "run in background");
        patch(PAUSE_ON_FOCUS_LOSS, &[0xC3], "no pause menu on focus loss");
        // A background client still runs its mouse code, which recentres the cursor every
        // frame and takes the mouse away from the user. Only let SetCursorPos through while
        // this game window is in the foreground.
        unsafe { redirect_set_cursor_pos() };
    }
    if cfg.skip_intro {
        patch(LOGO_NEXT_STATE_IMM, &5u32.to_le_bytes(), "skip intro movies");
    }
    log::write(&format!("boot config: {}", serde_json::to_string(cfg).unwrap_or_default()));
}

/// Slot holding our SetCursorPos filter; the `call [slot]` operands are pointed here.
static mut CURSOR_SLOT: usize = 0;

unsafe extern "system" fn set_cursor_pos_when_focused(x: i32, y: i32) -> i32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0u32;
    GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
    if pid != std::process::id() {
        return 1;
    }
    // Whatever is in the IAT now (another plugin may have hooked it).
    let real: unsafe extern "system" fn(i32, i32) -> i32 =
        std::mem::transmute(mem::read::<u32>(IAT_SET_CURSOR_POS).unwrap_or(0) as usize);
    real(x, y)
}

unsafe fn redirect_set_cursor_pos() {
    CURSOR_SLOT = set_cursor_pos_when_focused as *const () as usize;
    let slot = std::ptr::addr_of!(CURSOR_SLOT) as u32;
    for site in CALL_SET_CURSOR_POS {
        if mem::read::<[u8; 6]>(site) == Some([0xFF, 0x15, 0x00, 0x83, 0x85, 0x00]) {
            patch(site + 2, &slot.to_le_bytes(), "SetCursorPos only when focused");
        } else {
            log::write(&format!("unexpected bytes at SetCursorPos call {site:#x}; left alone"));
        }
    }
}

fn patch(address: u32, bytes: &[u8], what: &str) {
    match mem::write_bytes(address, bytes) {
        Ok(()) => log::write(&format!("patched {what} at {address:#x}")),
        Err(e) => log::write(&format!("patch {what} failed: {e}")),
    }
}

/// Hands focus back to the launcher's window once our window takes it (first 15 s only).
unsafe fn return_focus() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    let Some(target) = config().return_focus_to else { return };
    if DONE.load(Ordering::Relaxed) || START.get_or_init(std::time::Instant::now).elapsed().as_secs() > 15 {
        return;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
    if pid == std::process::id() {
        DONE.store(true, Ordering::Relaxed);
        let ok = SetForegroundWindow(HWND(target as isize as *mut _)).as_bool();
        log::write(&format!("returned focus to {target:#x}: {ok}"));
    }
}

/// Game thread, menu frames.
pub unsafe fn on_menu_frame() {
    return_focus();
    if !config().auto_start {
        return;
    }
    let state = mem::read::<i32>(GAME_STATE).unwrap_or(0);
    if state != GS_FRONTEND_IDLE {
        return;
    }
    if MENU_FRAMES.fetch_add(1, Ordering::Relaxed) == MENU_SETTLE_FRAMES {
        // WinMain's FRONTEND_IDLE state starts loading as soon as the menu is inactive.
        let _ = mem::write_bytes(MENU_ACTIVATE_NEXT_FRAME, &[0]);
        let _ = mem::write_bytes(MENU_ACTIVE, &[0]);
        let _ = mem::write_bytes(TIMER_USER_PAUSE, &[0]);
        log::write("auto-start: leaving menu, starting game");
    }
}

/// Game thread, in-game frames.
pub unsafe fn on_game_frame() {
    return_focus();
    let cfg = config();
    let frame = GAME_FRAMES.fetch_add(1, Ordering::Relaxed);
    if frame == 0 && cfg.mp_mode {
        // CTheScripts::Process -> ret before the first in-game script tick. main.scm's tick
        // during loading already created the player (loading dereferences it, so scripts
        // cannot be off earlier); the intro mission it queued never runs.
        patch(THE_SCRIPTS_PROCESS, &[0xC3], "disable main.scm");
    }
    if frame != GAME_SETTLE_FRAMES {
        return;
    }
    if cfg.mp_mode {
        match mp_setup(cfg) {
            Ok(()) => log::write("mp mode: world ready"),
            Err(e) => log::write(&format!("mp mode setup failed: {e}")),
        }
    } else if cfg.spawn.is_some() || cfg.time.is_some() || cfg.weather.is_some() {
        log::write("boot spawn/time/weather are only applied in mp_mode");
    }
}

unsafe fn mp_setup(cfg: &BootConfig) -> Result<(), String> {
    let mut s = cfg.spawn.unwrap_or(DEFAULT_SPAWN);
    // Keep clients started with the same config from spawning inside each other.
    s.x += instance::id().unwrap_or(0) as f32 * 2.0;

    // Player handle == player index (0); the ped already exists (created by main.scm while loading).
    let mut cmds = vec![
        cmd(0x01F5, vec![Int(0), Var(1)]),                                      // get_player_char -> var1
        cmd(0x04BB, vec![Int(0)]),                                              // select_interior 0
        cmd(0x03CB, vec![Float(s.x), Float(s.y), Float(s.z)]),                  // load_scene
        cmd(0x00A1, vec![Var(1), Float(s.x), Float(s.y), Float(s.z)]),          // set_char_coordinates
        cmd(0x0173, vec![Var(1), Float(s.heading)]),                            // set_char_heading
        // CJ's body is built by the intro mission, which never runs here: dress + rebuild.
        cmd(0x087B, vec![Int(0), Str("vest".into()), Str("vest".into()), Int(0)]),
        cmd(0x087B, vec![Int(0), Str("JEANSDENIM".into()), Str("JEANS".into()), Int(2)]),
        cmd(0x087B, vec![Int(0), Str("SNEAKERBINCBLK".into()), Str("SNEAKER".into()), Int(3)]),
        cmd(0x070D, vec![Int(0)]),                                              // rebuild_player
        cmd(0x0373, vec![]),                                                    // camera behind player
        cmd(0x02EB, vec![]),                                                    // restore_camera_jumpcut
        cmd(0x01B4, vec![Int(0), Int(1)]),                                      // player can move
        cmd(0x03DE, vec![Float(0.0)]),                                          // ped density 0
        cmd(0x01EB, vec![Float(0.0)]),                                          // car density 0
        cmd(0x01F0, vec![Int(0)]),                                              // max wanted level 0
        cmd(0x016A, vec![Int(500), Int(1)]),                                    // fade in
    ];
    if let Some([h, m]) = cfg.time {
        cmds.push(cmd(0x00C0, vec![Int(h as i32), Int(m as i32)]));
    }
    if let Some(w) = cfg.weather {
        cmds.push(cmd(0x01B6, vec![Int(w as i32)]));
    }
    script::run(&cmds).map(|_| ())
}
