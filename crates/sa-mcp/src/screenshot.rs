//! Captures the game window's client area via PrintWindow(PW_RENDERFULLCONTENT), which
//! works for D3D9 windows even when partially covered. Returns PNG bytes.

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};

const PW_CLIENTONLY_RENDERFULLCONTENT: u32 = 0x1 | 0x2;

unsafe fn process_exe_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let res = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(handle);
    res.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().map(str::to_ascii_lowercase)
}

/// Picks the visible top-level window owned by gta_sa.exe that has a non-empty client area.
unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam.0 as *mut Option<HWND>);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    if process_exe_name(hwnd).as_deref() == Some("gta_sa.exe") {
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() && rc.right > 0 && rc.bottom > 0 {
            *found = Some(hwnd);
            return BOOL(0);
        }
    }
    BOOL(1)
}

pub fn find_game_window() -> Option<HWND> {
    let mut found: Option<HWND> = None;
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn capture(max_width: u32) -> Result<Shot, String> {
    let hwnd = find_game_window().ok_or("game window not found")?;
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return Err("game window is minimized".into());
        }
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc).map_err(|e| e.to_string())?;
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w <= 0 || h <= 0 {
            return Err("game window has empty client area".into());
        }

        let wnd_dc = GetDC(hwnd);
        let mem_dc = CreateCompatibleDC(wnd_dc);
        let bmp = CreateCompatibleBitmap(wnd_dc, w, h);
        let old = SelectObject(mem_dc, bmp);
        let printed = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(PW_CLIENTONLY_RENDERFULLCONTENT)).as_bool();

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(mem_dc, bmp, 0, h as u32, Some(bgra.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(hwnd, wnd_dc);

        if !printed || lines == 0 {
            return Err("PrintWindow/GetDIBits failed".into());
        }
        encode(&bgra, w as u32, h as u32, max_width)
    }
}

/// Nearest-neighbour downscale to `max_width` (keeps token cost of images low), then PNG.
fn encode(bgra: &[u8], w: u32, h: u32, max_width: u32) -> Result<Shot, String> {
    let (ow, oh) = if max_width > 0 && w > max_width {
        (max_width, (h as u64 * max_width as u64 / w as u64).max(1) as u32)
    } else {
        (w, h)
    };
    let mut rgb = Vec::with_capacity((ow * oh * 3) as usize);
    for y in 0..oh {
        let sy = y as u64 * h as u64 / oh as u64;
        for x in 0..ow {
            let sx = x as u64 * w as u64 / ow as u64;
            let i = ((sy * w as u64 + sx) * 4) as usize;
            rgb.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i]]);
        }
    }
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, ow, oh);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    }
    Ok(Shot { png, width: ow, height: oh })
}
