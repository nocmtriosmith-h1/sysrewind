//! Extracts an app's icon from its .exe as a PNG data URL, for the app tables.

use base64::Engine;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

const SIZE: i32 = 48;

#[repr(C)]
struct IconInfo {
    f_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    hbm_mask: isize,
    hbm_color: isize,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_ppm: i32,
    y_ppm: i32,
    clr_used: u32,
    clr_important: u32,
}

/// BITMAPINFO with room for a colour table, which GetDIBits may write for low bit depths.
#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 256],
}

#[link(name = "user32")]
extern "system" {
    fn PrivateExtractIconsW(file: *const u16, index: i32, cx: i32, cy: i32, icons: *mut isize, ids: *mut u32, n: u32, flags: u32) -> u32;
    fn GetIconInfo(icon: isize, info: *mut IconInfo) -> i32;
    fn DestroyIcon(icon: isize) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetDIBits(hdc: isize, hbm: isize, start: u32, lines: u32, bits: *mut c_void, bmi: *mut BitmapInfo, usage: u32) -> i32;
    fn DeleteObject(obj: isize) -> i32;
}

/// Reads a bitmap as top-down 32-bit BGRA.
unsafe fn dib_bgra(hdc: isize, hbm: isize, w: i32, h: i32) -> Option<Vec<u8>> {
    let mut bmi: BitmapInfo = std::mem::zeroed();
    bmi.header = BitmapInfoHeader {
        size: std::mem::size_of::<BitmapInfoHeader>() as u32,
        width: w,
        height: -h, // negative = top-down rows
        planes: 1,
        bit_count: 32,
        compression: 0, // BI_RGB
        size_image: 0,
        x_ppm: 0,
        y_ppm: 0,
        clr_used: 0,
        clr_important: 0,
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let lines = GetDIBits(hdc, hbm, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut bmi, 0);
    (lines == h).then_some(buf)
}

fn extract_rgba(exe: &Path) -> Option<Vec<u8>> {
    let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        let mut icon: isize = 0;
        let mut id = 0u32;
        let n = PrivateExtractIconsW(wide.as_ptr(), 0, SIZE, SIZE, &mut icon, &mut id, 1, 0);
        if n == 0 || n == u32::MAX || icon == 0 {
            return None;
        }
        let mut info: IconInfo = std::mem::zeroed();
        let ok = GetIconInfo(icon, &mut info) != 0;
        let hdc = GetDC(0);
        let result = if ok && info.hbm_color != 0 {
            dib_bgra(hdc, info.hbm_color, SIZE, SIZE).map(|mut px| {
                // Icons without an alpha channel: take transparency from the AND mask.
                if px.chunks(4).all(|p| p[3] == 0) {
                    if let Some(mask) = dib_bgra(hdc, info.hbm_mask, SIZE, SIZE) {
                        for (p, m) in px.chunks_mut(4).zip(mask.chunks(4)) {
                            p[3] = if m[0] == 0 { 255 } else { 0 };
                        }
                    }
                }
                // BGRA -> RGBA
                for p in px.chunks_mut(4) {
                    p.swap(0, 2);
                }
                px
            })
        } else {
            None
        };
        ReleaseDC(0, hdc);
        if info.hbm_color != 0 {
            DeleteObject(info.hbm_color);
        }
        if info.hbm_mask != 0 {
            DeleteObject(info.hbm_mask);
        }
        DestroyIcon(icon);
        result
    }
}

/// `data:image/png;base64,...` for the exe's main icon, or None.
pub fn icon_data_url(exe: &Path) -> Option<String> {
    let rgba = extract_rgba(exe)?;
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, SIZE as u32, SIZE as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png_bytes)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn extracts_notepad_icon() {
        let url = super::icon_data_url(std::path::Path::new(r"C:\Windows\System32\notepad.exe")).expect("icon");
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(url.len() > 500);
    }

    #[test]
    fn missing_exe_gives_none() {
        assert!(super::icon_data_url(std::path::Path::new(r"C:\does\not\exist.exe")).is_none());
    }
}
