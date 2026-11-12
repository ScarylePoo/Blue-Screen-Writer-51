//! Rasterise physical pages (page.rs) with real TrueType fonts — for View
//! Document.

use crate::doc::Attr;
use crate::page::Page;
use std::cell::RefCell;
use std::collections::HashMap;

static FALLBACK: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

struct Face {
    font: fontdue::Font,
    synth_bold: bool,
    synth_italic: bool,
}

thread_local! {
    static FACES: RefCell<HashMap<(String, bool, bool), std::rc::Rc<Face>>> = RefCell::new(HashMap::new());
}

fn face(family: &str, bold: bool, italic: bool) -> std::rc::Rc<Face> {
    let key = (family.to_lowercase(), bold, italic);
    FACES.with(|f| {
        if let Some(x) = f.borrow().get(&key) {
            return x.clone();
        }
        let loaded = crate::fonts::find_face(family, bold, italic).and_then(|ff| {
            let data = std::fs::read(&ff.path).ok()?;
            let font = fontdue::Font::from_bytes(data, fontdue::FontSettings { collection_index: ff.index, ..Default::default() }).ok()?;
            let st = ff.style.to_lowercase();
            Some(Face {
                font,
                synth_bold: bold && !st.contains("bold"),
                synth_italic: italic && !(st.contains("italic") || st.contains("oblique")),
            })
        });
        let face = loaded.unwrap_or_else(|| Face {
            font: fontdue::Font::from_bytes(FALLBACK, fontdue::FontSettings::default()).unwrap(),
            synth_bold: bold,
            synth_italic: italic,
        });
        let rc = std::rc::Rc::new(face);
        f.borrow_mut().insert(key, rc.clone());
        rc
    })
}

/// Draw `page` into `buf` (stride `bw`) at pixel offset (ox, oy) with
/// `scale` pixels per WPU.  The paper is white with a shadow.
pub fn render_page(page: &Page, buf: &mut [u32], bw: usize, bh: usize, ox: i32, oy: i32, scale: f64) {
    let pw = (page.width as f64 * scale) as i32;
    let ph = (page.height as f64 * scale) as i32;
    let fill = |buf: &mut [u32], x0: i32, y0: i32, x1: i32, y1: i32, c: u32| {
        for y in y0.max(0)..y1.min(bh as i32) {
            for x in x0.max(0)..x1.min(bw as i32) {
                buf[y as usize * bw + x as usize] = c;
            }
        }
    };
    fill(buf, ox + 4, oy + 4, ox + pw + 4, oy + ph + 4, 0x202020);
    fill(buf, ox, oy, ox + pw, oy + ph, 0xFFFFFF);
    for im in &page.images {
        let (x, y, w, h) = im.clip;
        let x0 = ox + (x as f64 * scale).round() as i32;
        let y0 = oy + (y as f64 * scale).round() as i32;
        let x1 = ox + ((x + w) as f64 * scale).round() as i32;
        let y1 = oy + ((y + h) as f64 * scale).round() as i32;
        let (pw_, ph_) = ((x1 - x0).max(1) as usize, (y1 - y0).max(1) as usize);
        // only the visible part matters for huge zooms
        if pw_ * ph_ > 16_000_000 {
            continue;
        }
        let px = im.compose(pw_, ph_);
        for yy in 0..ph_ {
            let by = y0 + yy as i32;
            if by < 0 || by >= bh as i32 {
                continue;
            }
            for xx in 0..pw_ {
                let bx = x0 + xx as i32;
                if bx < 0 || bx >= bw as i32 {
                    continue;
                }
                let i = by as usize * bw + bx as usize;
                buf[i] = crate::raster::over(buf[i], px[yy * pw_ + xx]);
            }
        }
    }
    for &(x, y, w, h) in &page.rules {
        let x0 = ox + (x as f64 * scale) as i32;
        let y0 = oy + (y as f64 * scale) as i32;
        let x1 = (ox + ((x + w) as f64 * scale) as i32).max(x0 + 1);
        let y1 = (oy + ((y + h) as f64 * scale) as i32).max(y0 + 1);
        fill(buf, x0, y0, x1, y1, 0x000000);
    }
    for run in &page.runs {
        let bold = run.attrs.has(Attr::Bold);
        let italic = run.attrs.has(Attr::Italic);
        let f = face(&run.font, bold, italic);
        let px = (run.size_cpt as f64 / 100.0) * (1200.0 / 72.0) * scale;
        if px < 1.0 {
            continue;
        }
        let color: u32 = if run.attrs.has(Attr::Redline) { 0xC00000 } else { 0x000000 };
        let base_y = oy as f64 + run.y as f64 * scale;
        let mut x = ox as f64 + run.x as f64 * scale;
        let x_start = x;
        for (i, ch) in run.text.iter().enumerate() {
            let shown = if run.attrs.has(Attr::SmallCaps) { ch.to_uppercase().next().unwrap_or(*ch) } else { *ch };
            let gpx = if run.attrs.has(Attr::SmallCaps) && ch.is_lowercase() { px * 0.8 } else { px };
            if *ch != ' ' {
                let (m, bmp) = f.font.rasterize(shown, gpx as f32);
                let gx0 = (x + m.xmin as f64).round() as i32;
                let gy0 = (base_y - m.height as f64 - m.ymin as f64).round() as i32;
                let passes = if f.synth_bold { 2 } else { 1 };
                for pass in 0..passes {
                    for yy in 0..m.height {
                        let shear = if f.synth_italic { ((m.height - yy) as f64 * 0.2) as i32 } else { 0 };
                        for xx in 0..m.width {
                            let a = bmp[yy * m.width + xx] as u32;
                            if a == 0 {
                                continue;
                            }
                            let px_ = gx0 + xx as i32 + shear + pass;
                            let py_ = gy0 + yy as i32;
                            if px_ < 0 || py_ < 0 || px_ >= bw as i32 || py_ >= bh as i32 {
                                continue;
                            }
                            let idx = py_ as usize * bw + px_ as usize;
                            buf[idx] = blend(buf[idx], color, a);
                        }
                    }
                }
            }
            x += run.adv[i] as f64 * scale;
        }
        let t = (px / 14.0).max(1.0) as i32;
        if run.attrs.has(Attr::Underline) || run.attrs.has(Attr::DblUnderline) {
            let y = (base_y + px * 0.12) as i32;
            fill(buf, x_start as i32, y, x as i32, y + t, color);
            if run.attrs.has(Attr::DblUnderline) {
                fill(buf, x_start as i32, y + 2 * t, x as i32, y + 3 * t, color);
            }
        }
        if run.attrs.has(Attr::Strikeout) {
            let y = (base_y - px * 0.28) as i32;
            fill(buf, x_start as i32, y, x as i32, y + t, color);
        }
    }
}

fn blend(dst: u32, src: u32, a: u32) -> u32 {
    let inv = 255 - a;
    let r = (((src >> 16) & 0xff) * a + ((dst >> 16) & 0xff) * inv) / 255;
    let g = (((src >> 8) & 0xff) * a + ((dst >> 8) & 0xff) * inv) / 255;
    let b = ((src & 0xff) * a + (dst & 0xff) * inv) / 255;
    (r << 16) | (g << 8) | b
}
