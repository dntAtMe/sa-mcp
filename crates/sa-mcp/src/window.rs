//! Game window/process helpers: find a gta_sa.exe window (optionally by pid), tile
//! windows across the screen, terminate a game process.

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetSystemMetrics, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    SetWindowPos, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

unsafe fn process_exe_name(pid: u32) -> Option<String> {
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let res = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(handle);
    res.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().map(str::to_ascii_lowercase)
}

struct Search {
    pid: Option<u32>,
    found: Option<HWND>,
}

/// Picks a visible top-level window owned by gta_sa.exe (and `pid`, if given) that has a
/// non-empty client area.
unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam.0 as *mut Search);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if search.pid.is_some_and(|want| want != pid) {
        return BOOL(1);
    }
    if process_exe_name(pid).as_deref() == Some("gta_sa.exe") {
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() && rc.right > 0 && rc.bottom > 0 {
            search.found = Some(hwnd);
            return BOOL(0);
        }
    }
    BOOL(1)
}

pub fn find(pid: Option<u32>) -> Option<HWND> {
    let mut search = Search { pid, found: None };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut search as *mut _ as isize));
    }
    search.found
}

/// Arranges the windows left-to-right, top-to-bottom on the primary screen without resizing.
/// Returns how many windows were moved.
pub fn tile(pids: &[u32]) -> usize {
    let (screen_w, screen_h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let (mut x, mut y, mut row_h, mut moved) = (0, 0, 0, 0);
    for &pid in pids {
        let Some(hwnd) = find(Some(pid)) else { continue };
        let mut rc = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rc) }.is_err() {
            continue;
        }
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if x > 0 && x + w > screen_w {
            x = 0;
            y += row_h;
            row_h = 0;
        }
        let ty = y.min((screen_h - h).max(0));
        let ok = unsafe { SetWindowPos(hwnd, None, x, ty, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
        if ok.is_ok() {
            moved += 1;
        }
        x += w;
        row_h = row_h.max(h);
    }
    moved
}

pub fn is_game_running(pid: u32) -> bool {
    unsafe { process_exe_name(pid).as_deref() == Some("gta_sa.exe") }
}

/// Terminates `pid`, but only if it is a gta_sa.exe process.
pub fn kill_game(pid: u32) -> Result<(), String> {
    unsafe {
        if process_exe_name(pid).as_deref() != Some("gta_sa.exe") {
            return Err(format!("pid {pid} is not gta_sa.exe"));
        }
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| e.to_string())?;
        let res = TerminateProcess(handle, 0);
        let _ = CloseHandle(handle);
        res.map_err(|e| e.to_string())
    }
}
