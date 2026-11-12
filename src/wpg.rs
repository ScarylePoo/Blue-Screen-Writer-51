//! WordPerfect Graphics 1.0 (.WPG) reader and rasteriser.
//!
//! A WPG 1.0 file is `FF "WPC"`, a data offset, product 1 / type 0x16, and
//! then records `type, length, data`.  Coordinates are 1/1200 inch with
//! the origin at the bottom left.  We draw vector records (lines,
//! polylines, rectangles, polygons, ellipses/arcs, curves, text) and
//! bitmaps (types 1 and 2, RLE) into an ARGB raster with 2× supersampling.

use crate::raster::Raster;

const EGA: [u32; 16] = [
    0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA, 0x555555, 0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555,
    0xFF55FF, 0xFFFF55, 0xFFFFFF,
];

fn u16_at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).unwrap_or(0)
}
fn i16_at(b: &[u8], o: usize) -> i32 {
    u16_at(b, o) as i16 as i32
}

/// Iterate the records: (type, data).
fn records(b: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = vec![];
    if b.len() < 16 || &b[..4] != b"\xFFWPC" {
        return out;
    }
    let mut p = (u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize).max(16);
    while p + 2 <= b.len() {
        let t = b[p];
        let mut len = b[p + 1] as usize;
        p += 2;
        if len == 0xFF {
            let w = u16_at(b, p) as usize;
            p += 2;
            if w & 0x8000 != 0 {
                len = ((w & 0x7FFF) << 16) | u16_at(b, p) as usize;
                p += 2;
            } else {
                len = w;
            }
        }
        let end = (p + len).min(b.len());
        out.push((t, &b[p..end]));
        p = end;
        if t == 0x10 {
            break;
        }
    }
    out
}

/// Picture size in WPU (1/1200") from the Start record.
pub fn size(b: &[u8]) -> Option<(i32, i32)> {
    records(b).into_iter().find(|r| r.0 == 0x0F).map(|(_, d)| (u16_at(d, 2) as i32, u16_at(d, 4) as i32)).filter(|s| s.0 > 0 && s.1 > 0)
}

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u32>,
    /// pixels per WPG unit
    k: f64,
    /// picture height in units (for flipping y)
    ph: f64,
}

impl Canvas {
    fn pt(&self, x: i32, y: i32) -> (f64, f64) {
        (x as f64 * self.k, (self.ph - y as f64) * self.k)
    }

    /// Even-odd scanline fill of one or more closed paths.  `pattern` > 1
    /// draws a hatch instead of a solid fill.
    fn fill(&mut self, paths: &[Vec<(f64, f64)>], color: u32, pattern: u8) {
        let (mut y0, mut y1) = (f64::MAX, f64::MIN);
        for p in paths {
            for &(_, y) in p {
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        if y0 > y1 {
            return;
        }
        let ys = (y0.floor().max(0.0)) as usize;
        let ye = (y1.ceil().min(self.h as f64)) as usize;
        let mut xs: Vec<f64> = Vec::new();
        for row in ys..ye {
            let sy = row as f64 + 0.5;
            xs.clear();
            for p in paths {
                let n = p.len();
                for i in 0..n {
                    let (ax, ay) = p[i];
                    let (bx, by) = p[(i + 1) % n];
                    if (ay <= sy && by > sy) || (by <= sy && ay > sy) {
                        xs.push(ax + (sy - ay) / (by - ay) * (bx - ax));
                    }
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for pair in xs.chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let a = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let b = ((pair[1] - 0.5).floor() + 1.0).clamp(0.0, self.w as f64) as usize;
                for x in a..b.min(self.w) {
                    if pattern > 1 && (x + row) % 10 > 1 {
                        continue;
                    }
                    self.px[row * self.w + x] = 0xFF00_0000 | color;
                }
            }
        }
    }

    fn stroke(&mut self, pts: &[(f64, f64)], closed: bool, width: f64, color: u32) {
        let hw = (width / 2.0).max(0.75);
        let n = pts.len();
        if n == 0 {
            return;
        }
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % n];
            let (dx, dy) = (bx - ax, by - ay);
            let l = (dx * dx + dy * dy).sqrt();
            if l < 1e-9 {
                continue;
            }
            let (nx, ny) = (-dy / l * hw, dx / l * hw);
            self.fill(&[vec![(ax + nx, ay + ny), (bx + nx, by + ny), (bx - nx, by - ny), (ax - nx, ay - ny)]], color, 1);
        }
        if hw > 1.5 {
            for &(x, y) in pts {
                let c: Vec<(f64, f64)> = (0..8).map(|k| {
                    let a = k as f64 * std::f64::consts::PI / 4.0;
                    (x + hw * a.cos(), y + hw * a.sin())
                }).collect();
                self.fill(&[c], color, 1);
            }
        }
    }
}

fn points(d: &[u8], off: usize, n: usize) -> Vec<(i32, i32)> {
    let n = n.min(d.len().saturating_sub(off) / 4);
    (0..n).map(|k| (i16_at(d, off + 4 * k), i16_at(d, off + 4 * k + 2))).collect()
}

fn ellipse_pts(cx: f64, cy: f64, rx: f64, ry: f64, rot: f64, a0: f64, a1: f64, steps: usize) -> Vec<(f64, f64)> {
    let (cr, sr) = (rot.to_radians().cos(), rot.to_radians().sin());
    let span = if a1 > a0 { a1 - a0 } else { a1 + 360.0 - a0 };
    (0..=steps)
        .map(|k| {
            let a = (a0 + span * k as f64 / steps as f64).to_radians();
            let (x, y) = (rx * a.cos(), ry * a.sin());
            (cx + x * cr - y * sr, cy + x * sr + y * cr)
        })
        .collect()
}

/// Decode a WPG RLE bitmap into palette indices / packed bits.
fn decode_rle(d: &[u8], scanline: usize, rows: usize) -> Vec<u8> {
    let want = scanline * rows;
    let mut out: Vec<u8> = Vec::with_capacity(want);
    let mut p = 0;
    while out.len() < want && p < d.len() {
        let op = d[p];
        p += 1;
        let count = (op & 0x7F) as usize;
        if op & 0x80 != 0 {
            let (pixel, count) = if count > 0 {
                let v = d.get(p).copied().unwrap_or(0);
                p += 1;
                (v, count)
            } else {
                let c = d.get(p).copied().unwrap_or(0) as usize;
                p += 1;
                (0xFF, c)
            };
            out.extend(std::iter::repeat_n(pixel, count));
        } else if count > 0 {
            let end = (p + count).min(d.len());
            out.extend_from_slice(&d[p..end]);
            p = end;
        } else {
            let c = d.get(p).copied().unwrap_or(0) as usize;
            p += 1;
            if out.len() < scanline || scanline == 0 {
                break;
            }
            for _ in 0..c {
                let s = out.len() - scanline;
                for j in 0..scanline {
                    out.push(out[s + j]);
                }
            }
        }
    }
    out.resize(want, 0);
    out
}

/// Render to a raster whose longer side is about `target` pixels.
pub fn render(b: &[u8], target: usize) -> Option<Raster> {
    let recs = records(b);
    let (pw, ph) = size(b).unwrap_or((1200, 1200));
    const SS: usize = 2;
    let k = (target * SS) as f64 / pw.max(ph) as f64;
    let w = ((pw as f64 * k).round() as usize).max(1);
    let h = ((ph as f64 * k).round() as usize).max(1);
    let mut cv = Canvas { w, h, px: vec![0; w * h], k, ph: ph as f64 };
    let mut pal = [0u32; 256];
    for (i, c) in pal.iter_mut().enumerate() {
        *c = if i < 16 { EGA[i] } else { let g = (i as u32 - 16) * 255 / 239; g << 16 | g << 8 | g };
    }
    let (mut fill_style, mut fill_color) = (1u8, 0u32);
    let (mut line_style, mut line_color, mut line_w) = (1u8, 0u32, 1.0f64);
    let mut text_h = 0.0f64;
    let mut drew = false;
    for (t, d) in recs {
        match t {
            0x01 => {
                fill_style = d.first().copied().unwrap_or(1);
                fill_color = pal[d.get(1).copied().unwrap_or(0) as usize];
            }
            0x02 => {
                line_style = d.first().copied().unwrap_or(1);
                line_color = pal[d.get(1).copied().unwrap_or(0) as usize];
                line_w = (u16_at(d, 2) as f64 * k).max(1.0);
            }
            0x0E => {
                let start = u16_at(d, 0) as usize;
                let n = u16_at(d, 2) as usize;
                for i in 0..n {
                    let o = 4 + 3 * i;
                    if start + i < 256 && o + 3 <= d.len() {
                        pal[start + i] = (d[o] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o + 2] as u32;
                    }
                }
            }
            0x05 | 0x06 | 0x08 | 0x07 => {
                let pts: Vec<(f64, f64)> = match t {
                    0x05 => points(d, 0, 2).into_iter().map(|(x, y)| cv.pt(x, y)).collect(),
                    0x07 => {
                        let (x, y, rw, rh) = (i16_at(d, 0), i16_at(d, 2), i16_at(d, 4), i16_at(d, 6));
                        vec![cv.pt(x, y), cv.pt(x + rw, y), cv.pt(x + rw, y + rh), cv.pt(x, y + rh)]
                    }
                    _ => points(d, 2, u16_at(d, 0) as usize).into_iter().map(|(x, y)| cv.pt(x, y)).collect(),
                };
                let closed = matches!(t, 0x07 | 0x08);
                if closed && fill_style != 0 {
                    cv.fill(&[pts.clone()], fill_color, fill_style);
                }
                if line_style != 0 {
                    cv.stroke(&pts, closed, line_w, line_color);
                }
                drew = true;
            }
            0x13 => {
                // curved polyline: 4 reserved bytes, count, then p0 (c1 c2 p)*
                let n = u16_at(d, 4) as usize;
                let ps = points(d, 6, n);
                if let Some(&first) = ps.first() {
                    let mut out = vec![cv.pt(first.0, first.1)];
                    for seg in ps[1..].chunks(3) {
                        if seg.len() < 3 {
                            break;
                        }
                        let p0 = *out.last().unwrap();
                        let (c1, c2, p3) = (cv.pt(seg[0].0, seg[0].1), cv.pt(seg[1].0, seg[1].1), cv.pt(seg[2].0, seg[2].1));
                        for s in 1..=16 {
                            let t = s as f64 / 16.0;
                            let u = 1.0 - t;
                            let x = u * u * u * p0.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * p3.0;
                            let y = u * u * u * p0.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * p3.1;
                            out.push((x, y));
                        }
                    }
                    if line_style != 0 {
                        cv.stroke(&out, false, line_w, line_color);
                    }
                    drew = true;
                }
            }
            0x09 => {
                let (cx, cy) = cv.pt(i16_at(d, 0), i16_at(d, 2));
                let (rx, ry) = (i16_at(d, 4) as f64 * k, i16_at(d, 6) as f64 * k);
                let rot = u16_at(d, 8) as f64;
                let (a0, a1) = (u16_at(d, 10) as f64, u16_at(d, 12) as f64);
                let full = a0 == a1 || (a0 == 0.0 && a1 >= 360.0);
                let steps = ((rx.max(ry) * 0.5) as usize).clamp(24, 360);
                // y is flipped: negate the angles/rotation
                let mut pts = if full {
                    ellipse_pts(cx, cy, rx, ry, -rot, 0.0, 360.0, steps)
                } else {
                    ellipse_pts(cx, cy, rx, ry, -rot, 360.0 - a1, 360.0 - a0, steps)
                };
                if full {
                    pts.pop();
                }
                if fill_style != 0 {
                    let mut fp = pts.clone();
                    if !full {
                        fp.push((cx, cy));
                    }
                    cv.fill(&[fp], fill_color, fill_style);
                }
                if line_style != 0 {
                    cv.stroke(&pts, full, line_w, line_color);
                }
                drew = true;
            }
            0x0D => {
                // text attributes: font, width, height (units) …
                text_h = u16_at(d, 4) as f64 * k;
            }
            0x0C => {
                let n = u16_at(d, 0) as usize;
                let (x, y) = cv.pt(i16_at(d, 2), i16_at(d, 4));
                let s: String = d.get(6..6 + n).unwrap_or(&[]).iter().map(|&c| c as char).collect();
                let px = if text_h > 2.0 { text_h } else { h as f64 / 20.0 };
                draw_text(&mut cv, x, y, &s, px, line_color);
                drew = true;
            }
            0x0B | 0x14 => {
                let (o, bw, bh, depth, x1, y1, x2, y2) = if t == 0x0B {
                    let (bw, bh, dp) = (u16_at(d, 0) as usize, u16_at(d, 2) as usize, u16_at(d, 4) as usize);
                    (10, bw, bh, dp, 0, 0, pw, ph)
                } else {
                    let (x1, y1, x2, y2) = (i16_at(d, 2), i16_at(d, 4), i16_at(d, 6), i16_at(d, 8));
                    (20, u16_at(d, 10) as usize, u16_at(d, 12) as usize, u16_at(d, 14) as usize, x1, y1, x2, y2)
                };
                if bw == 0 || bh == 0 || bw > 20000 || bh > 20000 || !matches!(depth, 1 | 2 | 4 | 8) {
                    continue;
                }
                let scan = (bw * depth).div_ceil(8);
                let data = decode_rle(d.get(o..).unwrap_or(&[]), scan, bh);
                let (ax, ay) = cv.pt(x1.min(x2), y1.max(y2));
                let (bx, by) = cv.pt(x1.max(x2), y1.min(y2));
                let (dw, dh) = ((bx - ax).max(1.0), (by - ay).max(1.0));
                for py in (ay.max(0.0) as usize)..(by.min(h as f64) as usize) {
                    let sy = (((py as f64 + 0.5 - ay) / dh) * bh as f64) as usize;
                    if sy >= bh {
                        continue;
                    }
                    for px in (ax.max(0.0) as usize)..(bx.min(w as f64) as usize) {
                        let sx = (((px as f64 + 0.5 - ax) / dw) * bw as f64) as usize;
                        if sx >= bw {
                            continue;
                        }
                        let bit = sx * depth;
                        let byte = data[sy * scan + bit / 8];
                        let v = ((byte as usize) >> (8 - depth - bit % 8)) & ((1 << depth) - 1);
                        let c = if depth == 1 { if v == 1 { 0xFFFFFF } else { 0x000000 } } else { pal[v] };
                        cv.px[py * w + px] = 0xFF00_0000 | c;
                    }
                }
                drew = true;
            }
            0x10 => break,
            _ => {}
        }
    }
    if !drew {
        return None;
    }
    // 2× → 1× box downsample
    let (ow, oh) = (w / SS, h / SS);
    let mut out = Raster::new(ow.max(1), oh.max(1), 0);
    for y in 0..oh {
        for x in 0..ow {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for j in 0..SS {
                for i in 0..SS {
                    let p = cv.px[(y * SS + j) * w + x * SS + i];
                    let pa = p >> 24;
                    r += ((p >> 16) & 255) * pa;
                    g += ((p >> 8) & 255) * pa;
                    b += (p & 255) * pa;
                    a += pa;
                }
            }
            if a > 0 {
                out.px[y * ow + x] = (a / (SS * SS) as u32) << 24 | (r / a) << 16 | (g / a) << 8 | (b / a);
            }
        }
    }
    Some(out)
}

fn draw_text(cv: &mut Canvas, x: f64, y: f64, s: &str, px: f64, color: u32) {
    static FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
    thread_local! {
        static F: fontdue::Font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()).unwrap();
    }
    F.with(|f| {
        let mut cx = x;
        for ch in s.chars() {
            let (m, bmp) = f.rasterize(ch, px as f32);
            let gx = (cx + m.xmin as f64).round() as i64;
            let gy = (y - m.height as f64 - m.ymin as f64).round() as i64;
            for yy in 0..m.height {
                for xx in 0..m.width {
                    let a = bmp[yy * m.width + xx];
                    let (qx, qy) = (gx + xx as i64, gy + yy as i64);
                    if a > 100 && qx >= 0 && qy >= 0 && (qx as usize) < cv.w && (qy as usize) < cv.h {
                        cv.px[qy as usize * cv.w + qx as usize] = 0xFF00_0000 | color;
                    }
                }
            }
            cx += m.advance_width as f64;
        }
    });
}

#[cfg(test)]
pub fn tests_sample() -> Vec<u8> {
    tests::sample()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(t: u8, d: &[u8]) -> Vec<u8> {
        let mut v = vec![t, d.len() as u8];
        v.extend_from_slice(d);
        v
    }

    /// A tiny synthetic WPG: a black filled square on the left half.
    pub fn sample() -> Vec<u8> {
        let mut b = vec![0xFF, b'W', b'P', b'C', 16, 0, 0, 0, 1, 0x16, 1, 0, 0, 0, 0, 0];
        b.extend(rec(0x0F, &[1, 0, 0xB0, 0x04, 0xB0, 0x04])); // 1200 x 1200
        b.extend(rec(0x01, &[1, 0])); // solid black
        b.extend(rec(0x02, &[0, 0, 1, 0])); // no outline
        let pts: [i16; 9] = [4, 0, 0, 600, 0, 600, 1200, 0, 1200];
        let mut d = vec![];
        for p in pts {
            d.extend_from_slice(&p.to_le_bytes());
        }
        b.extend(rec(0x08, &d));
        b.extend(rec(0x10, &[]));
        b
    }

    #[test]
    fn synthetic_polygon() {
        let b = sample();
        assert_eq!(size(&b), Some((1200, 1200)));
        let r = render(&b, 100).unwrap();
        assert_eq!((r.w, r.h), (100, 100));
        assert_eq!(r.px[50 * 100 + 10] >> 24, 255, "left half filled");
        assert_eq!(r.px[50 * 100 + 90] >> 24, 0, "right half empty");
    }

    #[test]
    fn rle() {
        // run of 3 x 0xAA, 2 literal bytes, then repeat the 5-byte line once
        let d = [0x83, 0xAA, 0x02, 1, 2, 0x00, 1];
        assert_eq!(decode_rle(&d, 5, 2), vec![0xAA, 0xAA, 0xAA, 1, 2, 0xAA, 0xAA, 0xAA, 1, 2]);
    }

    #[test]
    fn her_clipart() {
        let dir = std::path::Path::new("/mnt/i/Claude Space/wp51/WPG");
        let dir = if dir.is_dir() { dir.to_path_buf() } else { std::path::PathBuf::from("I:\\Claude Space\\wp51\\WPG") };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut n = 0;
        for f in rd.flatten() {
            let b = std::fs::read(f.path()).unwrap();
            let r = render(&b, 200).unwrap_or_else(|| panic!("{:?}", f.path()));
            let inked = r.px.iter().filter(|p| **p >> 24 > 0).count();
            assert!(inked > 100, "{:?} drew nothing", f.path());
            n += 1;
        }
        assert!(n == 0 || n >= 10);
    }
}
