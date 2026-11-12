//! Make Blue Screen Writer 51 available as the program for word-processing
//! files in Windows ("Open with", Default apps), and the program icon.
//!
//! Windows 10/11 do not let programs silently make themselves the default
//! for a file type; we register everything per user (no admin rights) and
//! then open the Default apps page where the user picks us once.

use std::path::{Path, PathBuf};

pub const PROG_ID: &str = "BlueScreenWriter51.Document";
pub const APP_NAME: &str = "Blue Screen Writer 51";

/// Extensions we open.
pub const EXTENSIONS: &[&str] = &[
    "docx", "doc", "dotx", "dot", "docm", "rtf", "odt", "ott", "fodt", "wpd", "wp", "wp5", "wp6", "wp4", "wpt", "wri",
    "wps", "abw", "sam", "txt", "md", "htm", "html", "pdf",
];

// ------------------------------------------------------------------ icon

/// Draw the program icon (blue screen, white "51", a cursor) as RGBA.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as usize;
    let mut px = vec![0u8; s * s * 4];
    let r = (s as f32 * 0.14) as i32;
    let put = |px: &mut Vec<u8>, x: usize, y: usize, c: [u8; 4]| {
        let i = (y * s + x) * 4;
        px[i..i + 4].copy_from_slice(&c);
    };
    // rounded blue square
    for y in 0..s {
        for x in 0..s {
            let (xi, yi) = (x as i32, y as i32);
            let si = s as i32;
            let cx = if xi < r { r } else if xi >= si - r { si - r - 1 } else { xi };
            let cy = if yi < r { r } else if yi >= si - r { si - r - 1 } else { yi };
            // smooth (anti-aliased) rounded corners
            let dist = (((xi - cx).pow(2) + (yi - cy).pow(2)) as f32).sqrt() - r as f32;
            let a = (0.5 - dist).clamp(0.0, 1.0);
            if a > 0.0 {
                put(&mut px, x, y, [0x00, 0x00, 0xAA, (a * 255.0).round() as u8]);
            }
        }
    }
    // "51" in the screen font
    static FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf");
    if let Ok(font) = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()) {
        let pxs = s as f32 * 0.55;
        let glyphs: Vec<_> = "51".chars().map(|c| font.rasterize(c, pxs)).collect();
        let total: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
        let mut x0 = (s as f32 - total) / 2.0;
        let base = s as f32 * 0.66;
        for (m, bmp) in &glyphs {
            for gy in 0..m.height {
                for gx in 0..m.width {
                    let a = bmp[gy * m.width + gx];
                    if a == 0 {
                        continue;
                    }
                    let x = (x0 + m.xmin as f32) as i32 + gx as i32;
                    let y = (base - m.height as f32 - m.ymin as f32) as i32 + gy as i32;
                    if x >= 0 && y >= 0 && (x as usize) < s && (y as usize) < s {
                        let i = (y as usize * s + x as usize) * 4;
                        let blend = |bg: u8, fg: u8| ((fg as u32 * a as u32 + bg as u32 * (255 - a as u32)) / 255) as u8;
                        px[i] = blend(px[i], 0xFF);
                        px[i + 1] = blend(px[i + 1], 0xFF);
                        px[i + 2] = blend(px[i + 2], 0xFF);
                    }
                }
            }
            x0 += m.advance_width;
        }
    }
    // blinking-cursor underline
    let y0 = (s as f32 * 0.78) as usize;
    let h = (s / 16).max(1);
    let x0 = (s as f32 * 0.22) as usize;
    let x1 = (s as f32 * 0.42) as usize;
    for y in y0..(y0 + h).min(s) {
        for x in x0..x1 {
            put(&mut px, x, y, [0x55, 0xFF, 0xFF, 0xFF]);
        }
    }
    px
}

/// RGBA → PNG bytes.
pub fn rgba_png(rgba: &[u8], size: u32) -> Vec<u8> {
    // our encoder writes RGB; composite on nothing → encode RGBA ourselves
    fn crc32(data: &[u8]) -> u32 {
        let mut c: u32 = 0xFFFF_FFFF;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    }
    let s = size as usize;
    let mut raw = Vec::with_capacity((s * 4 + 1) * s);
    for y in 0..s {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * s * 4..(y + 1) * s * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let mut it = raw.chunks(65535).peekable();
    while let Some(c) = it.next() {
        z.push(it.peek().is_none() as u8);
        let n = c.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &d in &raw {
        a = (a + d as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = vec![];
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A .ico file with 16, 32, 48 and 256 pixel PNG images.
pub fn icon_ico() -> Vec<u8> {
    let sizes = [16u32, 32, 48, 256];
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&s| rgba_png(&icon_rgba(s), s)).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (k, &s) in sizes.iter().enumerate() {
        let d = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[d, d, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(pngs[k].len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += pngs[k].len() as u32;
    }
    for p in pngs {
        out.extend_from_slice(&p);
    }
    out
}

pub fn icon_path() -> PathBuf {
    crate::settings::config_dir().join("bsw51.ico")
}

// -------------------------------------------------------------- registry

#[cfg(windows)]
mod reg {
    pub const HKCU: isize = 0x8000_0001u32 as i32 as isize;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn RegCreateKeyExW(
            key: isize,
            sub: *const u16,
            reserved: u32,
            class: *const u16,
            options: u32,
            sam: u32,
            sec: *const core::ffi::c_void,
            result: *mut isize,
            disp: *mut u32,
        ) -> i32;
        pub fn RegSetValueExW(key: isize, name: *const u16, reserved: u32, ty: u32, data: *const u8, n: u32) -> i32;
        pub fn RegCloseKey(key: isize) -> i32;
        pub fn RegDeleteTreeW(key: isize, sub: *const u16) -> i32;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        pub fn SHChangeNotify(event: i32, flags: u32, a: *const core::ffi::c_void, b: *const core::ffi::c_void);
        pub fn ShellExecuteW(hwnd: isize, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Set HKCU\<path>\[name] = value (REG_SZ).  name "" = default value.
    pub fn set(path: &str, name: &str, value: &str) -> Result<(), String> {
        let p = wide(path);
        let mut key: isize = 0;
        let mut disp = 0u32;
        // KEY_WRITE = 0x20006
        let r = unsafe { RegCreateKeyExW(HKCU, p.as_ptr(), 0, std::ptr::null(), 0, 0x20006, std::ptr::null(), &mut key, &mut disp) };
        if r != 0 {
            return Err(format!("cannot write registry key {path} (error {r})"));
        }
        let n = wide(name);
        let v = wide(value);
        let r = unsafe { RegSetValueExW(key, if name.is_empty() { std::ptr::null() } else { n.as_ptr() }, 0, 1, v.as_ptr() as *const u8, (v.len() * 2) as u32) };
        unsafe { RegCloseKey(key) };
        if r != 0 { Err(format!("cannot set {path}\\{name} (error {r})")) } else { Ok(()) }
    }

    pub fn delete_tree(path: &str) {
        let p = wide(path);
        unsafe {
            RegDeleteTreeW(HKCU, p.as_ptr());
        }
    }

    pub fn delete_value(path: &str, name: &str) {
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn RegDeleteKeyValueW(key: isize, sub: *const u16, name: *const u16) -> i32;
        }
        let p = wide(path);
        let n = wide(name);
        unsafe {
            RegDeleteKeyValueW(HKCU, p.as_ptr(), n.as_ptr());
        }
    }

    pub fn notify() {
        // SHCNE_ASSOCCHANGED, SHCNF_IDLIST
        unsafe { SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null()) };
    }

    pub fn open(target: &str) {
        let op = wide("open");
        let t = wide(target);
        unsafe {
            ShellExecuteW(0, op.as_ptr(), t.as_ptr(), std::ptr::null(), std::ptr::null(), 1);
        }
    }
}

/// Register the program for all supported file types (current user).
pub fn register(exe: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let ico = icon_path();
        if let Some(d) = ico.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&ico, icon_ico()).map_err(|e| e.to_string())?;
        let exe_s = exe.display().to_string();
        let cmd = format!("\"{exe_s}\" \"%1\"");
        let ico_s = ico.display().to_string();
        let base = format!("Software\\Classes\\{PROG_ID}");
        reg::set(&base, "", "Blue Screen Writer 51 Document")?;
        reg::set(&format!("{base}\\DefaultIcon"), "", &ico_s)?;
        reg::set(&format!("{base}\\shell\\open\\command"), "", &cmd)?;
        let app = "Software\\Classes\\Applications\\bsw51.exe";
        reg::set(app, "FriendlyAppName", APP_NAME)?;
        reg::set(&format!("{app}\\DefaultIcon"), "", &ico_s)?;
        reg::set(&format!("{app}\\shell\\open\\command"), "", &cmd)?;
        let caps = "Software\\BlueScreenWriter51\\Capabilities";
        reg::set(caps, "ApplicationName", APP_NAME)?;
        reg::set(caps, "ApplicationDescription", "Word processor with the classic 5.1 keyboard and blue screen")?;
        reg::set(caps, "ApplicationIcon", &ico_s)?;
        for ext in EXTENSIONS {
            reg::set(&format!("Software\\Classes\\.{ext}\\OpenWithProgids"), PROG_ID, "")?;
            reg::set(&format!("{app}\\SupportedTypes"), &format!(".{ext}"), "")?;
            reg::set(&format!("{caps}\\FileAssociations"), &format!(".{ext}"), PROG_ID)?;
        }
        reg::set("Software\\RegisteredApplications", APP_NAME, caps)?;
        reg::notify();
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err("file type registration is only available on Windows".into())
    }
}

pub fn unregister() {
    #[cfg(windows)]
    {
        reg::delete_tree(&format!("Software\\Classes\\{PROG_ID}"));
        reg::delete_tree("Software\\Classes\\Applications\\bsw51.exe");
        reg::delete_tree("Software\\BlueScreenWriter51");
        reg::delete_value("Software\\RegisteredApplications", APP_NAME);
        for ext in EXTENSIONS {
            reg::delete_value(&format!("Software\\Classes\\.{ext}\\OpenWithProgids"), PROG_ID);
        }
        reg::notify();
    }
}

/// Open Windows' Default apps page for this program.
pub fn open_default_apps() {
    #[cfg(windows)]
    reg::open("ms-settings:defaultapps?registeredAppUser=Blue%20Screen%20Writer%2051");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_builds() {
        let ico = icon_ico();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert!(ico.len() > 1000);
        let px = icon_rgba(32);
        assert_eq!(px.len(), 32 * 32 * 4);
    }
}
