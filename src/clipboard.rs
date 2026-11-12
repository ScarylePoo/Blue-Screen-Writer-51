//! Windows clipboard: Shift-Ins paste, Ctrl-Ins copy, Shift-Del cut (CUA
//! keys that WP 5.1 leaves free); Block Move/Copy also put the text there.
//! Copy places plain text (CF_UNICODETEXT), RTF and HTML; paste prefers
//! our own codes (if the clipboard still holds what we copied), then RTF,
//! then plain text.

use crate::doc::*;
use crate::ui::Wp;

pub const CLIP_PASTE: u16 = 0xC101;
pub const CLIP_COPY: u16 = 0xC102;
pub const CLIP_CUT: u16 = 0xC103;

/// What was last copied from here: (plain text, exact elements).
static LAST: std::sync::Mutex<Option<(String, Vec<El>)>> = std::sync::Mutex::new(None);

pub struct Clip {
    pub text: Option<String>,
    pub rtf: Option<Vec<u8>>,
}

#[cfg(all(windows, not(test)))]
mod sys {
    use super::Clip;
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 2;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(hwnd: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(fmt: u32, h: isize) -> isize;
        fn GetClipboardData(fmt: u32) -> isize;
        fn IsClipboardFormatAvailable(fmt: u32) -> i32;
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        fn GlobalLock(h: isize) -> *mut u8;
        fn GlobalUnlock(h: isize) -> i32;
        fn GlobalSize(h: isize) -> usize;
        fn GlobalFree(h: isize) -> isize;
        fn Sleep(ms: u32);
    }

    fn format(name: &str) -> u32 {
        let w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: NUL-terminated UTF-16 string.
        unsafe { RegisterClipboardFormatW(w.as_ptr()) }
    }

    fn open() -> Result<(), String> {
        // another program may hold the clipboard for a moment
        for _ in 0..10 {
            // SAFETY: plain Win32 call.
            if unsafe { OpenClipboard(0) } != 0 {
                return Ok(());
            }
            unsafe { Sleep(20) };
        }
        Err("the clipboard is in use by another program".into())
    }

    unsafe fn put(fmt: u32, data: &[u8]) {
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE, data.len());
            if h == 0 {
                return;
            }
            let p = GlobalLock(h);
            if p.is_null() {
                GlobalFree(h);
                return;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            GlobalUnlock(h);
            if SetClipboardData(fmt, h) == 0 {
                GlobalFree(h);
            }
        }
    }

    unsafe fn get(fmt: u32) -> Option<Vec<u8>> {
        unsafe {
            if IsClipboardFormatAvailable(fmt) == 0 {
                return None;
            }
            let h = GetClipboardData(fmt);
            if h == 0 {
                return None;
            }
            let p = GlobalLock(h);
            if p.is_null() {
                return None;
            }
            let n = GlobalSize(h);
            let v = std::slice::from_raw_parts(p, n).to_vec();
            GlobalUnlock(h);
            Some(v)
        }
    }

    pub fn set(text: &str, rtf: Option<&str>, html: Option<&str>) -> Result<(), String> {
        open()?;
        // SAFETY: the clipboard is open; buffers are copied into global memory.
        unsafe {
            EmptyClipboard();
            let w: Vec<u8> = text.encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect();
            put(CF_UNICODETEXT, &w);
            if let Some(r) = rtf {
                let mut b = r.as_bytes().to_vec();
                b.push(0);
                put(format("Rich Text Format"), &b);
            }
            if let Some(h) = html {
                let mut b = h.as_bytes().to_vec();
                b.push(0);
                put(format("HTML Format"), &b);
            }
            CloseClipboard();
        }
        Ok(())
    }

    pub fn get_all() -> Result<Clip, String> {
        open()?;
        // SAFETY: the clipboard is open; data is copied out before closing.
        let (t, r) = unsafe {
            let t = get(CF_UNICODETEXT);
            let r = get(format("Rich Text Format"));
            CloseClipboard();
            (t, r)
        };
        let text = t.map(|b| {
            let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
            String::from_utf16_lossy(&u)
        });
        let rtf = r.map(|mut b| {
            if let Some(z) = b.iter().position(|&x| x == 0) {
                b.truncate(z);
            }
            b
        });
        Ok(Clip { text, rtf })
    }
}

/// Without Windows (and in tests): a clipboard inside the program.
#[cfg(any(not(windows), test))]
mod sys {
    use super::Clip;
    static MEM: std::sync::Mutex<Option<(String, Option<String>)>> = std::sync::Mutex::new(None);
    pub fn set(text: &str, rtf: Option<&str>, _html: Option<&str>) -> Result<(), String> {
        *MEM.lock().unwrap() = Some((text.to_string(), rtf.map(|s| s.to_string())));
        Ok(())
    }
    pub fn get_all() -> Result<Clip, String> {
        let m = MEM.lock().unwrap();
        Ok(match &*m {
            Some((t, r)) => Clip { text: Some(t.clone()), rtf: r.as_ref().map(|r| r.as_bytes().to_vec()) },
            None => Clip { text: None, rtf: None },
        })
    }
}

/// Plain text only (e.g. figures from Document Information).
pub fn set_text(s: &str) -> Result<(), String> {
    sys::set(&s.replace("\r\n", "\n").replace('\n', "\r\n"), None, None)
}

/// CF_HTML wrapper around an HTML fragment.
pub fn cf_html(fragment: &str) -> String {
    let pre = "<html><body>\r\n<!--StartFragment-->";
    let post = "<!--EndFragment-->\r\n</body></html>";
    let header_len = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n".len();
    let start_html = header_len;
    let start_frag = start_html + pre.len();
    let end_frag = start_frag + fragment.len();
    let end_html = end_frag + post.len();
    format!("Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_frag:010}\r\nEndFragment:{end_frag:010}\r\n{pre}{fragment}{post}")
}

/// Put elements on the clipboard as text + RTF + HTML.
pub fn put_els(els: &[El], like: &Document) -> Result<(), String> {
    let mut d = Document { els: els.to_vec(), ..Default::default() };
    d.init = like.init.clone();
    d.initial_codes = like.initial_codes.clone();
    // attributes in effect at the block start are part of the copy
    d.normalize_attrs();
    let text = d.plain_text(0, d.len()).replace('\n', "\r\n");
    let rtf = crate::export::to_rtf(&d);
    let html = crate::export::to_html(&d);
    let frag = html.split_once("<body>\n").map(|(_, b)| b).and_then(|b| b.rsplit_once("</body>")).map(|(b, _)| b).unwrap_or(&html);
    *LAST.lock().unwrap() = Some((text.clone(), els.to_vec()));
    sys::set(&text, Some(&rtf), Some(&cf_html(frag)))
}

/// Elements to paste from the clipboard.
pub fn get_els() -> Result<Option<Vec<El>>, String> {
    let c = sys::get_all()?;
    if let (Some(t), Some((lt, els))) = (&c.text, &*LAST.lock().unwrap()) {
        if t.replace("\r\n", "\n") == lt.replace("\r\n", "\n") {
            return Ok(Some(els.clone()));
        }
    }
    if let Some(r) = &c.rtf {
        if let Ok(d) = crate::rtf::read(r) {
            let mut els = d.els;
            // keep only text-level codes: page setup from another program
            // should not change this document
            els.retain(|e| !matches!(e, El::Code(Code::HeadFoot { .. })));
            if !els.is_empty() {
                return Ok(Some(els));
            }
        }
    }
    Ok(c.text.map(|t| Document::from_text(&t.replace("\r\n", "\n")).els))
}

/// Block Move/Copy: also place the block on the Windows clipboard.
pub fn export_block(wp: &mut Wp, els: &[El]) {
    let like = wp.ed_ref().doc.clone();
    if let Err(e) = put_els(els, &like) {
        wp.flash(&format!("ERROR: {e}"));
    }
}

/// Clipboard keys on the edit screen.  Returns false for other codes.
pub fn handle(wp: &mut Wp, code: u16) -> bool {
    match code {
        CLIP_PASTE => {
            match get_els() {
                Ok(Some(els)) => {
                    if wp.ed_ref().block_anchor.is_some() {
                        wp.ed().delete_block();
                    }
                    if wp.ed_ref().doc.track {
                        for e in els {
                            crate::track::insert_tracked(wp.ed(), e);
                        }
                    } else {
                        wp.ed().insert_els(&els);
                    }
                }
                Ok(None) => wp.flash("The clipboard is empty"),
                Err(e) => wp.flash(&format!("ERROR: {e}")),
            }
            true
        }
        CLIP_COPY | CLIP_CUT => {
            let Some(els) = wp.ed_ref().block_els() else {
                wp.flash("Block on? (Alt-F4 or F12 first)");
                return true;
            };
            export_block(wp, &els);
            if code == CLIP_CUT {
                if wp.ed_ref().doc.track {
                    crate::track::delete_block_tracked(wp.ed());
                } else {
                    wp.ed().delete_block();
                }
            } else {
                wp.ed().block_anchor = None;
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Func, Input};

    #[test]
    fn cf_html_offsets() {
        let s = cf_html("<b>x</b>");
        let num = |k: &str| -> usize {
            let i = s.find(k).unwrap() + k.len();
            s[i..i + 10].parse().unwrap()
        };
        assert_eq!(&s[num("StartFragment:")..num("EndFragment:")], "<b>x</b>");
        assert!(s[num("StartHTML:")..].starts_with("<html>"));
        assert_eq!(num("EndHTML:"), s.len());
    }

    #[test]
    fn copy_cut_paste() {
        let mut wp = Wp::new(80, 25);
        for c in "Hallo Welt".chars() {
            wp.ed().insert_char(c);
        }
        wp.ed().cursor = 0;
        wp.ed().block_anchor = Some(0);
        wp.ed().cursor = 5;
        wp.input(Input::Func(Func::Other(CLIP_CUT)));
        assert_eq!(wp.ed_ref().doc.plain_text(0, 99), " Welt");
        wp.input(Input::Func(Func::End));
        wp.input(Input::Func(Func::Other(CLIP_PASTE)));
        assert_eq!(wp.ed_ref().doc.plain_text(0, 99), " WeltHallo");
        // from another program: RTF with formatting
        sys::set("fett", Some("{\\rtf1\\ansi {\\b fett}}"), None).unwrap();
        wp.input(Input::Func(Func::Other(CLIP_PASTE)));
        assert!(wp.ed_ref().doc.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert_eq!(wp.ed_ref().doc.plain_text(0, 99), " WeltHallofett");
        // plain text only
        sys::set("Zeile1\r\nZeile2", None, None).unwrap();
        wp.input(Input::Func(Func::Other(CLIP_PASTE)));
        assert!(wp.ed_ref().doc.plain_text(0, 99).ends_with("fettZeile1\nZeile2"));
    }

    #[test]
    fn cua_keys_map() {
        use crate::keys::{K, KeyIn, Mods, default_map};
        let k = |k, shift, ctrl| default_map(KeyIn { k, m: Mods { shift, ctrl, alt: false } });
        assert_eq!(k(K::Ins, true, false), Some(Input::Func(Func::Other(CLIP_PASTE))));
        assert_eq!(k(K::Ins, false, true), Some(Input::Func(Func::Other(CLIP_COPY))));
        assert_eq!(k(K::Del, true, false), Some(Input::Func(Func::Other(CLIP_CUT))));
        assert_eq!(k(K::Ins, false, false), Some(Input::Func(Func::Typeover)));
    }
}
