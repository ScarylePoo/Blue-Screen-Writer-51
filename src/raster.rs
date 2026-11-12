//! Bitmaps for graphics boxes: decoding embedded image bytes (PNG, JPEG,
//! BMP, GIF through the pure-Rust `image` crate, WPG through wpg.rs) into
//! ARGB rasters, a small decode cache, and placing/transforming a raster
//! inside a box (scale, move, rotate, mirror, invert, black & white).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Straight-alpha ARGB pixels (0xAARRGGBB), row-major, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Raster {
    pub fn new(w: usize, h: usize, fill: u32) -> Raster {
        Raster { w, h, px: vec![fill; w * h] }
    }
}

/// What kind of image the bytes are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Wpg,
    Png,
    Jpeg,
    Bmp,
    Gif,
}

pub fn sniff(b: &[u8]) -> Option<Format> {
    if b.len() >= 4 && &b[..4] == b"\xFFWPC" {
        Some(Format::Wpg)
    } else if b.starts_with(b"\x89PNG") {
        Some(Format::Png)
    } else if b.starts_with(&[0xFF, 0xD8]) {
        Some(Format::Jpeg)
    } else if b.starts_with(b"BM") {
        Some(Format::Bmp)
    } else if b.starts_with(b"GIF8") {
        Some(Format::Gif)
    } else {
        None
    }
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Wpg => "wpg",
            Format::Png => "png",
            Format::Jpeg => "jpeg",
            Format::Bmp => "bmp",
            Format::Gif => "gif",
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            Format::Wpg => "image/x-wpg",
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Bmp => "image/bmp",
            Format::Gif => "image/gif",
        }
    }
}

fn hash(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325 ^ b.len() as u64;
    // long images: hash a sample (head, tail and every 61st byte)
    let step = if b.len() > 1 << 20 { 61 } else { 1 };
    for x in b.iter().step_by(step) {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    for x in b.iter().rev().take(64) {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

type Cache = Mutex<HashMap<u64, Option<Arc<Raster>>>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Decode (cached).  `None` if the bytes are not a supported image.
pub fn decode(bytes: &[u8]) -> Option<Arc<Raster>> {
    let key = hash(bytes);
    if let Some(r) = cache().lock().ok()?.get(&key) {
        return r.clone();
    }
    let r = decode_uncached(bytes).map(Arc::new);
    if let Ok(mut c) = cache().lock() {
        if c.len() > 48 {
            c.clear();
        }
        c.insert(key, r.clone());
    }
    r
}

fn decode_uncached(bytes: &[u8]) -> Option<Raster> {
    match sniff(bytes)? {
        Format::Wpg => crate::wpg::render(bytes, 1600),
        _ => {
            let img = image::load_from_memory(bytes).ok()?.to_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            let px = img.pixels().map(|p| (p[3] as u32) << 24 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
            Some(Raster { w, h, px })
        }
    }
}

/// Natural aspect: (width, height) in arbitrary units (pixels, or WPU for WPG).
pub fn dims(bytes: &[u8]) -> Option<(u32, u32)> {
    if sniff(bytes)? == Format::Wpg {
        return crate::wpg::size(bytes).map(|(w, h)| (w as u32, h as u32));
    }
    let r = decode(bytes)?;
    Some((r.w as u32, r.h as u32))
}

/// PNG encoding (for .docx export of WPG pictures).
pub fn to_png(r: &Raster) -> Option<Vec<u8>> {
    let mut rgba = Vec::with_capacity(r.px.len() * 4);
    for p in &r.px {
        rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
    }
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    image::ImageEncoder::write_image(enc, &rgba, r.w as u32, r.h as u32, image::ExtendedColorType::Rgba8).ok()?;
    Some(out)
}

/// An image (or a flat colour) placed on a page.  Rectangles are WPU.
#[derive(Clone, Debug)]
pub struct Placed {
    /// Only this rectangle is painted.
    pub clip: (i32, i32, i32, i32),
    /// None = fill the clip with `color`.
    pub img: Option<Arc<Raster>>,
    /// ARGB for flat fills (alpha used for gray shading).
    pub color: u32,
    /// Where the whole (unrotated) image lands.
    pub dst: (i32, i32, i32, i32),
    pub rot: i32,
    pub mirror: bool,
    pub invert: bool,
    pub bw: bool,
}

impl Placed {
    pub fn solid(x: i32, y: i32, w: i32, h: i32, color: u32) -> Placed {
        Placed { clip: (x, y, w, h), img: None, color, dst: (x, y, w, h), rot: 0, mirror: false, invert: false, bw: false }
    }

    /// Paint the clip rectangle at `pw` × `ph` pixels: ARGB, straight alpha.
    pub fn compose(&self, pw: usize, ph: usize) -> Vec<u32> {
        let mut out = vec![0u32; pw * ph];
        if pw == 0 || ph == 0 {
            return out;
        }
        let Some(img) = &self.img else {
            out.fill(self.color);
            return out;
        };
        let (cx, cy, cw, ch) = self.clip;
        let (dx, dy, dw, dh) = self.dst;
        if dw <= 0 || dh <= 0 || img.w == 0 || img.h == 0 {
            return out;
        }
        let sx = cw as f64 / pw as f64; // WPU per output pixel
        let sy = ch as f64 / ph as f64;
        let (mx, my) = (dx as f64 + dw as f64 / 2.0, dy as f64 + dh as f64 / 2.0);
        let a = (self.rot as f64).to_radians();
        let (ca, sa) = (a.cos(), a.sin());
        // image pixels per WPU
        let kx = img.w as f64 / dw as f64;
        let ky = img.h as f64 / dh as f64;
        // supersampling when shrinking a lot
        let foot = (sx * kx).max(sy * ky);
        let n = if foot > 1.5 { (foot.ceil() as usize).min(4) } else { 1 };
        for py in 0..ph {
            for px in 0..pw {
                let (mut r, mut g, mut b, mut al) = (0u32, 0u32, 0u32, 0u32);
                for j in 0..n {
                    for i in 0..n {
                        let wx = cx as f64 + (px as f64 + (i as f64 + 0.5) / n as f64) * sx - mx;
                        let wy = cy as f64 + (py as f64 + (j as f64 + 0.5) / n as f64) * sy - my;
                        let (rx, ry) = (wx * ca - wy * sa, wx * sa + wy * ca);
                        let mut ix = (rx + dw as f64 / 2.0) * kx;
                        let iy = (ry + dh as f64 / 2.0) * ky;
                        if self.mirror {
                            ix = img.w as f64 - ix;
                        }
                        if ix < 0.0 || iy < 0.0 || ix >= img.w as f64 || iy >= img.h as f64 {
                            continue;
                        }
                        let p = img.px[iy as usize * img.w + ix as usize];
                        let pa = p >> 24;
                        r += ((p >> 16) & 255) * pa;
                        g += ((p >> 8) & 255) * pa;
                        b += (p & 255) * pa;
                        al += pa;
                    }
                }
                if al == 0 {
                    continue;
                }
                let (mut r, mut g, mut b) = (r / al, g / al, b / al);
                let a = al / (n * n) as u32;
                if self.bw {
                    let l = (r * 30 + g * 59 + b * 11) / 100;
                    let v = if l < 160 { 0 } else { 255 };
                    (r, g, b) = (v, v, v);
                }
                if self.invert {
                    (r, g, b) = (255 - r, 255 - g, 255 - b);
                }
                out[py * pw + px] = a.min(255) << 24 | r << 16 | g << 8 | b;
            }
        }
        out
    }
}

/// Blend ARGB `src` onto opaque RGB `dst`.
pub fn over(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    if a == 0 {
        return dst;
    }
    if a >= 255 {
        return src & 0xFFFFFF;
    }
    let inv = 255 - a;
    let r = (((src >> 16) & 255) * a + ((dst >> 16) & 255) * inv) / 255;
    let g = (((src >> 8) & 255) * a + ((dst >> 8) & 255) * inv) / 255;
    let b = ((src & 255) * a + (dst & 255) * inv) / 255;
    r << 16 | g << 8 | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_roundtrip_and_compose() {
        let mut r = Raster::new(4, 2, 0xFFFF0000);
        r.px[0] = 0xFF0000FF;
        let png = to_png(&r).unwrap();
        assert_eq!(sniff(&png), Some(Format::Png));
        let d = decode(&png).unwrap();
        assert_eq!((d.w, d.h), (4, 2));
        assert_eq!(d.px[0], 0xFF0000FF);
        assert_eq!(dims(&png), Some((4, 2)));
        let p = Placed { img: Some(d), ..Placed::solid(0, 0, 400, 200, 0) };
        let out = p.compose(4, 2);
        assert_eq!(out[1], 0xFFFF0000);
        let mirrored = Placed { mirror: true, ..p.clone() }.compose(4, 2);
        assert_eq!(mirrored[3], 0xFF0000FF);
    }
}
