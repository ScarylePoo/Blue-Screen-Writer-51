//! Rasterises a [`Grid`] into a 0x00RRGGBB pixel buffer using TrueType
//! fonts (fontdue).  Bold / italic use real font faces.

use crate::grid::*;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

static FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf");
static FONT_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Oblique.ttf");
static FONT_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-BoldOblique.ttf");

struct Glyph {
    w: usize,
    h: usize,
    xmin: i32,
    ymin: i32,
    bitmap: Vec<u8>,
}

pub struct Renderer {
    fonts: [Font; 4],
    /// Windows fonts for characters the screen font lacks (Hebrew, …),
    /// loaded on first need.
    fallback: Option<Vec<Font>>,
    cache: HashMap<(char, u8, u32), Glyph>,
    pub cell_w: usize,
    pub cell_h: usize,
    pub off_x: usize,
    pub off_y: usize,
    px: f32,
    ascent: f32,
    /// Draw bold/italic/underline as real typography (true) or only as
    /// colours like DOS text mode (false).
    pub typographic: bool,
}

impl Renderer {
    pub fn new() -> Renderer {
        let load = |b: &[u8]| Font::from_bytes(b, FontSettings::default()).expect("font");
        Renderer {
            fonts: [load(FONT_REGULAR), load(FONT_BOLD), load(FONT_ITALIC), load(FONT_BOLD_ITALIC)],
            fallback: None,
            cache: HashMap::new(),
            cell_w: 9,
            cell_h: 18,
            off_x: 0,
            off_y: 0,
            px: 14.0,
            ascent: 12.0,
            typographic: true,
        }
    }

    /// Width/height ratio of a character cell for the screen font.
    pub fn cell_aspect(&self) -> f32 {
        let lm = self.fonts[0].horizontal_line_metrics(1.0).unwrap();
        let line = (lm.ascent - lm.descent) * 1.05;
        let adv = self.fonts[0].metrics('M', 1.0).advance_width;
        adv / line
    }

    /// Recompute cell metrics to fit `cols×rows` into `w×h` pixels.
    pub fn fit(&mut self, w: usize, h: usize, cols: usize, rows: usize) {
        let cw = (w / cols).max(4);
        let ch = (h / rows).max(8);
        let lm = self.fonts[0].horizontal_line_metrics(1.0).unwrap();
        let line = lm.ascent - lm.descent;
        let adv = self.fonts[0].metrics('M', 1.0).advance_width;
        let px_h = ch as f32 / (line * 1.05);
        let px_w = cw as f32 / adv;
        let px = px_h.min(px_w).max(6.0);
        if (px - self.px).abs() > 0.01 || cw != self.cell_w || ch != self.cell_h {
            self.cache.clear();
        }
        self.px = px;
        self.cell_w = cw;
        self.cell_h = ch;
        let lm = self.fonts[0].horizontal_line_metrics(px).unwrap();
        let used = lm.ascent - lm.descent;
        self.ascent = lm.ascent + ((ch as f32 - used) / 2.0).max(0.0);
        self.off_x = (w - cw * cols) / 2;
        self.off_y = (h - ch * rows) / 2;
    }

    fn glyph(&mut self, ch: char, face: u8) -> &Glyph {
        // face bit 4: Japanese paragraph (only matters for CJK fonts)
        let jp = face & 4 != 0;
        let face = face & 3;
        let key = (ch, face | if jp { 4 } else { 0 }, self.px.to_bits());
        if !self.cache.contains_key(&key) {
            let mut font = &self.fonts[face as usize];
            if font.lookup_glyph_index(ch) == 0 && face != 0 {
                font = &self.fonts[0];
            }
            let mut px = self.px;
            if font.lookup_glyph_index(ch) == 0 && crate::cjk::is_cjk(ch) {
                if let Some(f) = cjk_font(ch, jp) {
                    font = f;
                    // fill two cells (one for halfwidth forms), not taller than a cell
                    let cols = if crate::cjk::is_wide(ch) { 2.0 } else { 1.0 };
                    let lm = f.horizontal_line_metrics(1.0).map(|l| l.ascent - l.descent).unwrap_or(1.2);
                    px = (self.cell_w as f32 * cols * 0.92).min(self.cell_h as f32 / lm);
                }
            } else if font.lookup_glyph_index(ch) == 0 && !ch.is_ascii() {
                if self.fallback.is_none() {
                    self.fallback = Some(load_fallback_fonts());
                }
                if let Some(f) = self.fallback.as_ref().unwrap().iter().find(|f| f.lookup_glyph_index(ch) != 0) {
                    font = f;
                }
            }
            let (m, bitmap) = font.rasterize(ch, px);
            self.cache.insert(key, Glyph { w: m.width, h: m.height, xmin: m.xmin, ymin: m.ymin, bitmap });
        }
        &self.cache[&key]
    }

    /// Box-drawing and block characters are drawn geometrically so they
    /// join seamlessly across cells, like on a real text-mode screen.
    fn draw_box_char(buf: &mut [u32], stride: usize, x0: usize, y0: usize, cw: usize, ch_: usize, c: char, fg: u32) -> bool {
        let (l, r, u, d, dbl) = match c {
            '─' => (1, 1, 0, 0, false),
            '│' => (0, 0, 1, 1, false),
            '┌' => (0, 1, 0, 1, false),
            '┐' => (1, 0, 0, 1, false),
            '└' => (0, 1, 1, 0, false),
            '┘' => (1, 0, 1, 0, false),
            '├' => (0, 1, 1, 1, false),
            '┤' => (1, 0, 1, 1, false),
            '┬' => (1, 1, 0, 1, false),
            '┴' => (1, 1, 1, 0, false),
            '┼' => (1, 1, 1, 1, false),
            '═' => (1, 1, 0, 0, true),
            '║' => (0, 0, 1, 1, true),
            '╔' => (0, 1, 0, 1, true),
            '╗' => (1, 0, 0, 1, true),
            '╚' => (0, 1, 1, 0, true),
            '╝' => (1, 0, 1, 0, true),
            '╠' => (0, 1, 1, 1, true),
            '╣' => (1, 0, 1, 1, true),
            '╦' => (1, 1, 0, 1, true),
            '╩' => (1, 1, 1, 0, true),
            '╬' => (1, 1, 1, 1, true),
            '█' | '▀' | '▄' | '▌' | '▐' => {
                let (fx0, fy0, fx1, fy1) = match c {
                    '█' => (0, 0, cw, ch_),
                    '▀' => (0, 0, cw, ch_ / 2),
                    '▄' => (0, ch_ / 2, cw, ch_),
                    '▌' => (0, 0, cw / 2, ch_),
                    _ => (cw / 2, 0, cw, ch_),
                };
                for y in fy0..fy1 {
                    for x in fx0..fx1 {
                        buf[(y0 + y) * stride + x0 + x] = fg;
                    }
                }
                return true;
            }
            _ => return false,
        };
        let t = (ch_ / 16).max(1);
        let cx = x0 + cw / 2;
        let cy = y0 + ch_ / 2;
        let mut hline = |y: usize, xa: usize, xb: usize| {
            for x in xa..xb {
                for k in 0..t {
                    buf[(y + k) * stride + x] = fg;
                }
            }
        };
        let gap = if dbl { t * 2 } else { 0 };
        let offs: &[isize] = if dbl { &[-(gap as isize), gap as isize] } else { &[0] };
        for &o in offs {
            let y = (cy as isize + o) as usize;
            // for doubles, inner corner lines shorten — approximate
            if l == 1 {
                hline(y, x0, cx + t);
            }
            if r == 1 {
                hline(y, cx, x0 + cw);
            }
        }
        for &o in offs {
            let x = (cx as isize + o) as usize;
            let ya = if u == 1 { y0 } else { cy };
            let yb = if d == 1 { y0 + ch_ } else { cy + t };
            if u == 1 || d == 1 {
                for y in ya..yb {
                    for k in 0..t {
                        buf[y * stride + x + k] = fg;
                    }
                }
            }
        }
        true
    }

    pub fn render(&mut self, g: &Grid, buf: &mut [u32], w: usize, h: usize, blink_on: bool, bg: u32) {
        for p in buf.iter_mut() {
            *p = bg;
        }
        let cw = self.cell_w;
        let chh = self.cell_h;
        for row in 0..g.rows {
            // backgrounds first: a double-width (CJK) glyph covers two cells
            for col in 0..g.cols {
                let cell = g.cells[row * g.cols + col];
                let x0 = self.off_x + col * cw;
                let y0 = self.off_y + row * chh;
                if x0 + cw > w || y0 + chh > h {
                    continue;
                }
                for y in y0..y0 + chh {
                    buf[y * w + x0..y * w + x0 + cw].fill(cell.bg.0);
                }
            }
            for col in 0..g.cols {
                let cell = g.cells[row * g.cols + col];
                let x0 = self.off_x + col * cw;
                let y0 = self.off_y + row * chh;
                if x0 + cw > w || y0 + chh > h {
                    continue;
                }
                let visible = !(cell.flags & F_BLINK != 0 && !blink_on);
                // '\0' = right half of a double-width character
                if cell.ch != ' ' && cell.ch != '\0' && visible {
                    let fg = cell.fg.0;
                    if !Self::draw_box_char(buf, w, x0, y0, cw, chh, cell.ch, fg) {
                        let face = if self.typographic {
                            (cell.flags & F_BOLD != 0) as u8 | (((cell.flags & F_ITALIC != 0) as u8) << 1)
                        } else {
                            0
                        };
                        let ascent = self.ascent;
                        let wide = crate::cjk::is_wide(cell.ch);
                        let g = self.glyph(cell.ch, face | if cell.flags & F_JA != 0 { 4 } else { 0 });
                        let gx = if wide {
                            x0 as i32 + (2 * cw as i32 - g.w as i32) / 2
                        } else {
                            x0 as i32 + g.xmin + ((cw as i32 - (g.w as i32 + g.xmin.max(0))) / 2).max(0).min(1)
                        };
                        let gy = y0 as i32 + ascent.round() as i32 - g.h as i32 - g.ymin;
                        for yy in 0..g.h {
                            let py = gy + yy as i32;
                            if py < y0 as i32 || py >= (y0 + chh) as i32 {
                                continue;
                            }
                            for xx in 0..g.w {
                                let px = gx + xx as i32;
                                if px < 0 || px >= w as i32 {
                                    continue;
                                }
                                let a = g.bitmap[yy * g.w + xx] as u32;
                                if a == 0 {
                                    continue;
                                }
                                let idx = py as usize * w + px as usize;
                                buf[idx] = blend(buf[idx], fg, a);
                            }
                        }
                    }
                }
                if visible && self.typographic {
                    let fg = cell.fg.0;
                    let t = (chh / 16).max(1);
                    if cell.flags & (F_UNDER | F_DUNDER) != 0 {
                        let y = y0 + chh - 2 * t;
                        for k in 0..t {
                            buf[(y + k) * w + x0..(y + k) * w + x0 + cw].fill(fg);
                        }
                        if cell.flags & F_DUNDER != 0 {
                            let y = y0 + chh - 4 * t;
                            for k in 0..t {
                                buf[(y + k) * w + x0..(y + k) * w + x0 + cw].fill(fg);
                            }
                        }
                    }
                    if cell.flags & F_STRIKE != 0 {
                        let y = y0 + chh / 2;
                        for k in 0..t {
                            buf[(y + k) * w + x0..(y + k) * w + x0 + cw].fill(fg);
                        }
                    }
                }
            }
        }
        // spell check as you type: red wavy underline
        for row in 0..g.rows {
            for col in 0..g.cols {
                let cell = g.cells[row * g.cols + col];
                if cell.flags & F_SPELL == 0 {
                    continue;
                }
                let x0 = self.off_x + col * cw;
                let y0 = self.off_y + row * chh;
                if x0 + cw > w || y0 + chh > h {
                    continue;
                }
                let amp = (chh / 12).max(1);
                let base = y0 + chh - 1 - amp;
                for x in x0..x0 + cw {
                    let ph = (x / (2 * amp)) % 2;
                    let y = if ph == 0 { base } else { base + amp };
                    for yy in [y, y + 1] {
                        if yy < h {
                            buf[yy * w + x] = 0xFF4040;
                        }
                    }
                }
            }
        }
        // DOS-style blinking underline cursor
        if let Some((cc, cr)) = g.cursor {
            if blink_on && cc < g.cols && cr < g.rows {
                let cell = g.cells[cr * g.cols + cc];
                let x0 = self.off_x + cc * cw;
                let y0 = self.off_y + cr * chh;
                let t = (chh / 8).max(2);
                let color = cell.fg.0;
                for y in y0 + chh - t - 1..y0 + chh - 1 {
                    if y < h && x0 + cw <= w {
                        buf[y * w + x0..y * w + x0 + cw].fill(color);
                    }
                }
            }
        }
    }
}

thread_local! {
    /// CJK screen fonts, loaded once per family when first needed (they are
    /// large).  Leaked so glyph lookups can borrow them for the program's life.
    static CJK_FONTS: std::cell::RefCell<HashMap<&'static str, Option<&'static Font>>> = std::cell::RefCell::new(HashMap::new());
}

/// The Windows CJK font for `ch` (Microsoft YaHei, Yu Gothic, Malgun Gothic …).
fn cjk_font(ch: char, jp: bool) -> Option<&'static Font> {
    for fam in crate::cjk::fallback_families(ch, jp) {
        let f = CJK_FONTS.with(|m| {
            *m.borrow_mut().entry(fam).or_insert_with(|| {
                // on screen the heavier Medium weight reads better (Yu Gothic
                // Regular is very thin)
                let face = crate::fonts::installed()
                    .iter()
                    .find(|f| f.family.eq_ignore_ascii_case(fam) && f.style.eq_ignore_ascii_case("Medium"))
                    .or_else(|| crate::fonts::find_face(fam, false, false))?;
                let data = std::fs::read(&face.path).ok()?;
                let font = Font::from_bytes(data, FontSettings { collection_index: face.index, ..FontSettings::default() }).ok()?;
                Some(&*Box::leak(Box::new(font)))
            })
        });
        if let Some(f) = f.filter(|f| f.lookup_glyph_index(ch) != 0) {
            return Some(f);
        }
    }
    None
}

/// Courier New (closest to the screen font), then Arial: together they cover
/// Hebrew, Arabic, Greek, Cyrillic and most symbols.
fn load_fallback_fonts() -> Vec<Font> {
    let mut v = vec![];
    // Windows, macOS and Linux names
    for fam in ["Courier New", "Arial", "Segoe UI Symbol", "Arial Unicode MS", "DejaVu Sans", "FreeSerif", "Noto Sans Hebrew", "Noto Sans Arabic", "Noto Sans Symbols"] {
        let Some(face) = crate::fonts::find_face(fam, false, false) else { continue };
        if let Ok(b) = std::fs::read(&face.path) {
            if let Ok(f) = Font::from_bytes(b, FontSettings { collection_index: face.index, ..FontSettings::default() }) {
                v.push(f);
            }
        }
    }
    v
}

fn blend(dst: u32, src: u32, a: u32) -> u32 {
    let inv = 255 - a;
    let r = (((src >> 16) & 0xff) * a + ((dst >> 16) & 0xff) * inv) / 255;
    let g = (((src >> 8) & 0xff) * a + ((dst >> 8) & 0xff) * inv) / 255;
    let b = ((src & 0xff) * a + (dst & 0xff) * inv) / 255;
    (r << 16) | (g << 8) | b
}
