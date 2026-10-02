//! Captures the game window's client area via PrintWindow(PW_RENDERFULLCONTENT), which
//! works for D3D9 windows even when partially covered. Returns PNG bytes.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsIconic};

use crate::window;

const PW_CLIENTONLY_RENDERFULLCONTENT: u32 = 0x1 | 0x2;

pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Captures the window of game process `pid` (or any gta_sa.exe window when None).
pub fn capture(pid: Option<u32>, max_width: u32) -> Result<Shot, String> {
    let hwnd = window::find(pid).ok_or("game window not found")?;
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
