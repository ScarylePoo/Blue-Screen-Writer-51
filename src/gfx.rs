//! Graphics boxes and graphic lines (Alt-F9), WP 5.1 style.
//!
//! * Model: `GBox` (Figure, Table Box, Text Box, User Box, Equation) with
//!   its contents (embedded image bytes, text, equation source), caption,
//!   anchor, position, size and wrap; `BoxOpts` (`[Fig Opt]`…), new box
//!   numbers, `[Box Num]` and `GLine` (`[HLine:…]`/`[VLine:…]`).
//! * Reveal Codes text and the lossless code-stream form (image bytes as
//!   base64, so documents stay self-contained).
//! * Layout support (numbering, reserved space / text wrap zones, box
//!   geometry), page drawing for View Document and printing, and the box
//!   outline on the text-mode editing screen.

use crate::doc::*;
use crate::layout::{COL_WPU, Layout, Line, Metrics};
use crate::page::{Page, Run};
use crate::raster::Placed;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ================================================================ model

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoxKind {
    Figure,
    Table,
    Text,
    User,
    Equation,
}

impl BoxKind {
    pub const ALL: [BoxKind; 5] = [BoxKind::Figure, BoxKind::Table, BoxKind::Text, BoxKind::User, BoxKind::Equation];
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn from_idx(i: usize) -> BoxKind {
        Self::ALL[i.min(4)]
    }
    /// Reveal Codes name: `[Fig Box:…]`.
    pub fn reveal(self) -> &'static str {
        ["Fig Box", "Tbl Box", "Text Box", "Usr Box", "Equ Box"][self.idx()]
    }
    /// `[Fig Opt]`, `[New Fig Num:n]`.
    pub fn short(self) -> &'static str {
        ["Fig", "Tbl", "Txt", "Usr", "Equ"][self.idx()]
    }
    /// Edit-screen outline label.
    pub fn label(self) -> &'static str {
        ["FIG", "TBL", "TXT", "USR", "EQU"][self.idx()]
    }
    /// Menu / screen name.
    pub fn name(self) -> &'static str {
        ["Figure", "Table Box", "Text Box", "User Box", "Equation"][self.idx()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Para,
    Page,
    Char,
}

/// Vertical position: paragraph boxes use `Set(offset from top of
/// paragraph)`; page boxes Full/Top/Center/Bottom/Set(from top of page);
/// character boxes Top/Center/Bottom/Baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VPos {
    Full,
    Top,
    Center,
    Bottom,
    Baseline,
    Set(i32),
}

/// Horizontal position (Set = from the left edge of the page).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HPos {
    Left,
    Right,
    Center,
    Full,
    Set(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contents {
    Empty,
    Image,
    Text,
    Equation,
}

impl Contents {
    pub fn name(self) -> &'static str {
        match self {
            Contents::Empty => "None",
            Contents::Image => "Graphic",
            Contents::Text => "Text",
            Contents::Equation => "Equation",
        }
    }
}

/// Graphics → Edit (image editing): move, scale, rotate, mirror, invert, B&W.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImgEdit {
    pub dx: i32,
    pub dy: i32,
    /// percent
    pub sx: i32,
    pub sy: i32,
    /// degrees counter-clockwise
    pub rot: i32,
    pub mirror: bool,
    pub invert: bool,
    pub bw: bool,
}

impl Default for ImgEdit {
    fn default() -> Self {
        ImgEdit { dx: 0, dy: 0, sx: 100, sy: 100, rot: 0, mirror: false, invert: false, bw: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GBox {
    pub kind: BoxKind,
    pub contents: Contents,
    /// File name shown in Reveal Codes (`MOUSE.WPG`).
    pub file: String,
    /// Embedded image bytes in their original format (WPG/PNG/JPEG/BMP/GIF).
    pub data: Option<Arc<Vec<u8>>>,
    /// Text box / table box / user box text.
    pub text: Vec<El>,
    /// Equation source (WP equation language).
    pub equation: String,
    pub caption: Vec<El>,
    pub anchor: Anchor,
    pub vpos: VPos,
    pub hpos: HPos,
    pub width: i32,
    pub height: i32,
    /// bit 0 = automatic height, bit 1 = automatic width
    pub auto: u8,
    pub wrap: bool,
    pub img: ImgEdit,
}

impl GBox {
    pub fn new(kind: BoxKind) -> GBox {
        let (contents, hpos, auto) = match kind {
            BoxKind::Text => (Contents::Text, HPos::Right, 1),
            BoxKind::Equation => (Contents::Equation, HPos::Full, 1),
            BoxKind::Table => (Contents::Text, HPos::Center, 1),
            _ => (Contents::Empty, HPos::Right, 1),
        };
        GBox {
            kind,
            contents,
            file: String::new(),
            data: None,
            text: vec![],
            equation: String::new(),
            caption: vec![],
            anchor: Anchor::Para,
            vpos: VPos::Set(0),
            hpos,
            width: 3900,
            height: 3900,
            auto,
            wrap: true,
            img: ImgEdit::default(),
        }
    }

    /// Put image bytes in the box (contents = Graphic, auto height).
    pub fn set_image(&mut self, name: &str, bytes: Vec<u8>) {
        self.file = name.to_uppercase();
        self.data = Some(Arc::new(bytes));
        self.contents = Contents::Image;
        self.img = ImgEdit::default();
        self.auto_height();
    }

    /// Height from the image aspect for automatic sizing.
    pub fn auto_height(&mut self) {
        if self.contents == Contents::Image {
            if let Some((iw, ih)) = self.data.as_ref().and_then(|d| crate::raster::dims(d)) {
                if iw > 0 && ih > 0 {
                    if self.auto & 1 != 0 || self.auto == 0 {
                        self.height = (self.width as i64 * ih as i64 / iw as i64) as i32;
                    } else if self.auto & 2 != 0 {
                        self.width = (self.height as i64 * iw as i64 / ih as i64) as i32;
                    }
                }
            }
        }
    }
}

/// Box options code (`[Fig Opt]`…).
#[derive(Clone, Debug, PartialEq)]
pub struct BoxOpts {
    pub kind: BoxKind,
    /// Left, Right, Top, Bottom: 0 None, 1 Single, 2 Double, 3 Dashed,
    /// 4 Dotted, 5 Thick, 6 Extra Thick.
    pub border: [u8; 4],
    pub outside: [i32; 4],
    pub inside: [i32; 4],
    /// 0 Off, 1 Numbers, 2 Letters, 3 Roman Numerals
    pub num1: u8,
    pub num2: u8,
    /// e.g. `[BOLD]Figure 1[bold]` (1 = first level number)
    pub cap_style: String,
    /// 0 below box, 1 above box
    pub cap_pos: u8,
    pub cap_inside: bool,
    pub min_offset: i32,
    /// gray shading, % of black
    pub gray: u8,
}

pub const BORDER_NAMES: [&str; 7] = ["None", "Single", "Double", "Dashed", "Dotted", "Thick", "Extra Thick"];
pub const NUM_NAMES: [&str; 4] = ["Off", "Numbers", "Letters", "Roman Numerals"];

impl BoxOpts {
    pub fn default_for(kind: BoxKind) -> BoxOpts {
        let o = 200; // 0.167"
        match kind {
            BoxKind::Figure => BoxOpts {
                kind,
                border: [1, 1, 1, 1],
                outside: [o; 4],
                inside: [0; 4],
                num1: 1,
                num2: 0,
                cap_style: "[BOLD]Figure 1[bold]".into(),
                cap_pos: 0,
                cap_inside: false,
                min_offset: 0,
                gray: 0,
            },
            BoxKind::Table => BoxOpts {
                kind,
                border: [0, 0, 5, 5],
                outside: [o; 4],
                inside: [o; 4],
                num1: 3,
                num2: 0,
                cap_style: "[BOLD]Table 1[bold]".into(),
                cap_pos: 1,
                cap_inside: false,
                min_offset: 0,
                gray: 0,
            },
            BoxKind::Text => BoxOpts {
                kind,
                border: [0, 0, 5, 5],
                outside: [o; 4],
                inside: [o; 4],
                num1: 1,
                num2: 0,
                cap_style: "[BOLD]1[bold]".into(),
                cap_pos: 0,
                cap_inside: false,
                min_offset: 0,
                gray: 10,
            },
            BoxKind::User => BoxOpts {
                kind,
                border: [0; 4],
                outside: [o; 4],
                inside: [0; 4],
                num1: 1,
                num2: 0,
                cap_style: "[BOLD]1[bold]".into(),
                cap_pos: 0,
                cap_inside: false,
                min_offset: 0,
                gray: 0,
            },
            BoxKind::Equation => BoxOpts {
                kind,
                border: [0; 4],
                outside: [100, 100, 100, 100],
                inside: [100; 4],
                num1: 1,
                num2: 0,
                cap_style: "[BOLD](1)[bold]".into(),
                cap_pos: 0,
                cap_inside: false,
                min_offset: 0,
                gray: 0,
            },
        }
    }
}

/// `[HLine:…]` / `[VLine:…]`.
#[derive(Clone, Debug, PartialEq)]
pub struct GLine {
    pub vertical: bool,
    /// horizontal lines: 0 Left, 1 Right, 2 Center, 3 Full, 4 Set Position;
    /// vertical lines: 0 Left Margin, 1 Right Margin, 2 Between Columns, 3 Set Position
    pub halign: u8,
    pub x: i32,
    /// horizontal lines: 0 Baseline, 4 Set Position; vertical lines:
    /// 0 Full Page, 1 Top, 2 Center, 3 Bottom, 4 Set Position
    pub valign: u8,
    pub y: i32,
    /// 0 = automatic (margin to margin / full page)
    pub length: i32,
    pub width: i32,
    /// % of black
    pub gray: u8,
}

impl GLine {
    pub fn new(vertical: bool, length: i32) -> GLine {
        GLine { vertical, halign: if vertical { 0 } else { 3 }, x: 0, valign: 0, y: 0, length, width: 16, gray: 100 }
    }
    pub fn halign_name(&self) -> String {
        if self.vertical {
            match self.halign {
                0 => "Left Margin".into(),
                1 => "Right Margin".into(),
                2 => "Between Columns".into(),
                _ => fmt_inches(self.x),
            }
        } else {
            match self.halign {
                0 => "Left".into(),
                1 => "Right".into(),
                2 => "Center".into(),
                3 => "Full".into(),
                _ => fmt_inches(self.x),
            }
        }
    }
    pub fn valign_name(&self) -> String {
        if self.vertical {
            match self.valign {
                0 => "Full Page".into(),
                1 => "Top".into(),
                2 => "Center".into(),
                3 => "Bottom".into(),
                _ => fmt_inches(self.y),
            }
        } else if self.valign == 0 {
            "Baseline".into()
        } else {
            fmt_inches(self.y)
        }
    }
}

// ================================================================ reveal codes

/// Reveal Codes text; `#` stands for the box number (filled in by the
/// caller from the layout).
pub fn reveal(c: &Code) -> Option<String> {
    Some(match c {
        Code::GBox(b) => {
            let cap = crate::reveal::preview(&b.caption);
            format!("[{}:#;{};{}]", b.kind.reveal(), b.file, cap)
        }
        Code::BoxOpt(o) => format!("[{} Opt]", o.kind.short()),
        Code::BoxNewNum(k, n) => format!("[New {} Num:{}]", k.short(), n),
        Code::BoxNum => "[Box Num]".into(),
        Code::GLine(l) => {
            let len = if l.length > 0 { fmt_inches(l.length) } else { "Auto".into() };
            format!(
                "[{}:{},{},{},{},{}%]",
                if l.vertical { "VLine" } else { "HLine" },
                l.halign_name(),
                l.valign_name(),
                len,
                fmt_fine(l.width),
                l.gray
            )
        }
        _ => return None,
    })
}

/// Thin measurements (line widths) with three decimals: `0.013"`.
pub fn fmt_fine(wpu: i32) -> String {
    if UNIT_ENTRY.load(std::sync::atomic::Ordering::Relaxed) != b'"' {
        return fmt_inches(wpu);
    }
    let s = format!("{:.3}", wpu as f64 / 1200.0);
    format!("{}\"", s.trim_end_matches('0').trim_end_matches('.'))
}

/// Number formatting for box numbers: 1 Numbers, 2 Letters, 3 Roman.
pub fn fmt_num(n: u32, method: u8) -> String {
    match method {
        2 => {
            let mut s = String::new();
            let mut k = n.max(1);
            while k > 0 {
                k -= 1;
                s.insert(0, (b'A' + (k % 26) as u8) as char);
                k /= 26;
            }
            s
        }
        3 => {
            let mut k = n.max(1);
            let mut s = String::new();
            for (v, r) in [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")] {
                while k >= v {
                    s.push_str(r);
                    k -= v;
                }
            }
            s
        }
        _ => n.to_string(),
    }
}

/// The caption with `[Box Num]` replaced by the caption number style.
pub fn expand_caption(caption: &[El], o: &BoxOpts, num: &str) -> Vec<El> {
    let mut out = vec![];
    for e in caption {
        if matches!(e, El::Code(Code::BoxNum)) {
            out.extend(style_els(&o.cap_style, num));
        } else {
            out.push(e.clone());
        }
    }
    out
}

/// `[BOLD]Figure 1[bold]` → elements.
pub fn style_els(style: &str, num: &str) -> Vec<El> {
    let mut out = vec![];
    let mut rest = style;
    while !rest.is_empty() {
        if rest.starts_with('[') {
            if let Some(e) = rest.find(']') {
                let name = &rest[1..e];
                if let Some(a) = Attr::from_mnemonic(name) {
                    let on = name.chars().any(|c| c.is_uppercase());
                    out.push(El::Code(if on { Code::AttrOn(a) } else { Code::AttrOff(a) }));
                    rest = &rest[e + 1..];
                    continue;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        if c == '1' {
            out.extend(num.chars().map(El::Ch));
        } else {
            out.push(El::Ch(c));
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

// ================================================================ stream

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(B64[(n >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn b64_decode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

fn vpos_str(v: VPos) -> String {
    match v {
        VPos::Full => "F".into(),
        VPos::Top => "T".into(),
        VPos::Center => "C".into(),
        VPos::Bottom => "B".into(),
        VPos::Baseline => "L".into(),
        VPos::Set(n) => format!("S{n}"),
    }
}
fn parse_vpos(s: &str) -> VPos {
    match s {
        "F" => VPos::Full,
        "T" => VPos::Top,
        "C" => VPos::Center,
        "B" => VPos::Bottom,
        "L" => VPos::Baseline,
        _ => VPos::Set(s.get(1..).and_then(|x| x.parse().ok()).unwrap_or(0)),
    }
}
fn hpos_str(h: HPos) -> String {
    match h {
        HPos::Left => "L".into(),
        HPos::Right => "R".into(),
        HPos::Center => "C".into(),
        HPos::Full => "F".into(),
        HPos::Set(n) => format!("S{n}"),
    }
}
fn parse_hpos(s: &str) -> HPos {
    match s {
        "L" => HPos::Left,
        "R" => HPos::Right,
        "C" => HPos::Center,
        "F" => HPos::Full,
        _ => HPos::Set(s.get(1..).and_then(|x| x.parse().ok()).unwrap_or(0)),
    }
}

fn list<T: ToString>(v: &[T]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}
fn nums(s: &str) -> Vec<i32> {
    s.split(',').filter_map(|x| x.parse().ok()).collect()
}

/// Code → stream record (without the leading `k`).  Fields are `|`
/// separated and escaped by stream.rs rules.
pub fn to_stream(c: &Code) -> Option<String> {
    use crate::stream::{els_to_string, esc};
    Some(match c {
        Code::GBox(b) => {
            let i = &b.img;
            format!(
                "GBox|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                b.kind.idx(),
                b.contents as u8,
                esc(&b.file),
                b.data.as_ref().map(|d| b64_encode(d)).unwrap_or_default(),
                esc(&els_to_string(&b.text)),
                esc(&b.equation),
                esc(&els_to_string(&b.caption)),
                b.anchor as u8,
                vpos_str(b.vpos),
                hpos_str(b.hpos),
                b.width,
                b.height,
                b.auto,
                b.wrap as u8,
                list(&[i.dx, i.dy, i.sx, i.sy, i.rot, i.mirror as i32, i.invert as i32, i.bw as i32])
            )
        }
        Code::BoxOpt(o) => format!(
            "BOpt|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            o.kind.idx(),
            list(&o.border),
            list(&o.outside),
            list(&o.inside),
            o.num1,
            o.num2,
            esc(&o.cap_style),
            o.cap_pos,
            o.cap_inside as u8,
            o.min_offset,
            o.gray
        ),
        Code::BoxNewNum(k, n) => format!("BNum|{}|{n}", k.idx()),
        Code::BoxNum => "BoxNum".into(),
        Code::GLine(l) => format!("GLine|{}|{}|{}|{}|{}|{}|{}|{}", l.vertical as u8, l.halign, l.x, l.valign, l.y, l.length, l.width, l.gray),
        _ => return None,
    })
}

pub fn from_stream(p: &[&str]) -> Option<Code> {
    use crate::stream::{string_to_els, unesc};
    let g = |i: usize| p.get(i).copied().unwrap_or("");
    let n = |i: usize| g(i).parse::<i32>().unwrap_or(0);
    Some(match g(0) {
        "GBox" => {
            let im = nums(g(15));
            let iv = |k: usize, d: i32| im.get(k).copied().unwrap_or(d);
            let data = if g(4).is_empty() { None } else { Some(Arc::new(b64_decode(g(4)))) };
            Code::GBox(Box::new(GBox {
                kind: BoxKind::from_idx(n(1) as usize),
                contents: match n(2) {
                    1 => Contents::Image,
                    2 => Contents::Text,
                    3 => Contents::Equation,
                    _ => Contents::Empty,
                },
                file: unesc(g(3)),
                data,
                text: string_to_els(&unesc(g(5))),
                equation: unesc(g(6)),
                caption: string_to_els(&unesc(g(7))),
                anchor: match n(8) {
                    1 => Anchor::Page,
                    2 => Anchor::Char,
                    _ => Anchor::Para,
                },
                vpos: parse_vpos(g(9)),
                hpos: parse_hpos(g(10)),
                width: n(11),
                height: n(12),
                auto: n(13) as u8,
                wrap: g(14) != "0",
                img: ImgEdit { dx: iv(0, 0), dy: iv(1, 0), sx: iv(2, 100), sy: iv(3, 100), rot: iv(4, 0), mirror: iv(5, 0) != 0, invert: iv(6, 0) != 0, bw: iv(7, 0) != 0 },
            }))
        }
        "BOpt" => {
            let kind = BoxKind::from_idx(n(1) as usize);
            let mut o = BoxOpts::default_for(kind);
            let b = nums(g(2));
            for k in 0..4 {
                o.border[k] = b.get(k).copied().unwrap_or(0).clamp(0, 6) as u8;
                o.outside[k] = nums(g(3)).get(k).copied().unwrap_or(0);
                o.inside[k] = nums(g(4)).get(k).copied().unwrap_or(0);
            }
            o.num1 = n(5) as u8;
            o.num2 = n(6) as u8;
            o.cap_style = unesc(g(7));
            o.cap_pos = n(8) as u8;
            o.cap_inside = g(9) == "1";
            o.min_offset = n(10);
            o.gray = n(11).clamp(0, 100) as u8;
            Code::BoxOpt(Box::new(o))
        }
        "BNum" => Code::BoxNewNum(BoxKind::from_idx(n(1) as usize), n(2).max(1) as u32),
        "BoxNum" => Code::BoxNum,
        "GLine" => Code::GLine(GLine {
            vertical: g(1) == "1",
            halign: n(2) as u8,
            x: n(3),
            valign: n(4) as u8,
            y: n(5),
            length: n(6),
            width: n(7),
            gray: n(8).clamp(0, 100) as u8,
        }),
        _ => return None,
    })
}

// ================================================================ sizes

/// Height of nested text laid out `w` wide.
pub fn text_height(doc: &Document, els: &[El], w: i32, m: &dyn Metrics) -> i32 {
    if els.is_empty() {
        return 0;
    }
    let mut d = Document::new();
    d.init = doc.init.clone();
    d.init.left = 0;
    d.init.right = (d.init.page_width - w.max(COL_WPU * 4)).max(0);
    crate::layout::block_height(&d, els, m)
}

/// Runs of nested text laid out in the column (x, w) with its top at y.
pub fn runs_in(doc: &Document, els: &[El], m: &dyn Metrics, x: i32, y: i32, w: i32) -> Vec<Run> {
    let mut d = Document::new();
    d.init = doc.init.clone();
    d.init.left = x;
    d.init.right = (d.init.page_width - x - w.max(COL_WPU * 4)).max(0);
    if d.init.right == 0 {
        d.init.page_width = x + w.max(COL_WPU * 4);
    }
    crate::page::nested_runs(&d, els, m, y).0
}

fn eq_size(doc: &Document) -> u32 {
    doc.init.font.size_cpt.max(1000)
}

/// Frame (border box) size for a box: (width, height).  `avail` is the
/// width between the margins, `body_h` the height between top and bottom
/// margins (for Full Page boxes).
pub fn frame_size(doc: &Document, b: &GBox, o: &BoxOpts, avail: i32, body_h: i32, m: &dyn Metrics) -> (i32, i32) {
    let mut w = if b.anchor != Anchor::Char && b.hpos == HPos::Full { avail } else { b.width };
    w = w.max(COL_WPU * 2);
    let inner_w = (w - o.inside[0] - o.inside[1]).max(COL_WPU);
    let mut h = b.height;
    if b.auto & 1 != 0 {
        h = match b.contents {
            Contents::Text => text_height(doc, &b.text, inner_w, m) + o.inside[2] + o.inside[3],
            Contents::Equation => {
                let e = crate::equation::layout_fit(&b.equation, eq_size(doc), inner_w, m);
                e.asc + e.desc + o.inside[2] + o.inside[3]
            }
            Contents::Image => match b.data.as_ref().and_then(|d| crate::raster::dims(d)) {
                Some((iw, ih)) if iw > 0 => (w as i64 * ih as i64 / iw as i64) as i32,
                _ => b.height,
            },
            Contents::Empty => b.height,
        };
        h = h.max(240);
    }
    if b.anchor == Anchor::Page && b.vpos == VPos::Full {
        h = body_h - o.outside[2] - o.outside[3];
    }
    (w, h.max(COL_WPU))
}

/// Caption height incl. the gap to the frame (0 if no caption).
pub fn caption_height(doc: &Document, b: &GBox, o: &BoxOpts, num: &str, w: i32, m: &dyn Metrics) -> i32 {
    if b.caption.is_empty() {
        return 0;
    }
    text_height(doc, &expand_caption(&b.caption, o, num), w, m) + 60
}

// ================================================================ layout support

/// Where a box ended up (WPU, absolute on its page).
#[derive(Clone, Debug)]
pub struct BoxGeom {
    pub el: usize,
    pub kind: BoxKind,
    pub anchor: Anchor,
    pub num: String,
    pub opts: BoxOpts,
    /// frame
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// caption height (drawn below or above the frame)
    pub cap_h: i32,
    pub page: u32,
    /// paragraph boxes: offset of the frame top from the anchor row;
    /// page boxes: absolute frame top; character boxes: descent below
    /// the frame bottom.
    pub dy: i32,
    pub vrow: Option<usize>,
    pub line: usize,
    /// A paragraph box that did not fit: moved to the top of the next page.
    pub bumped: bool,
}

impl BoxGeom {
    /// Outer extent incl. outside spacing and caption: (x0, y0, x1, y1).
    pub fn outer(&self) -> (i32, i32, i32, i32) {
        let o = &self.opts;
        let above = if o.cap_pos == 1 { self.cap_h } else { 0 };
        let below = if o.cap_pos == 1 { 0 } else { self.cap_h };
        (self.x - o.outside[0], self.y - o.outside[2] - above, self.x + self.w + o.outside[1], self.y + self.h + below + o.outside[3])
    }
}

struct Zone {
    x0: i32,
    x1: i32,
    /// paragraph zones: height still to wrap around
    remaining: i32,
    /// page zones: (page, y0, y1)
    page: Option<(u32, i32, i32)>,
}

/// Page/margin context for laying out a box.
pub struct Ctx {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    pub page_w: i32,
    pub page_h: i32,
    pub page: u32,
    pub y: i32,
    pub vrow: Option<usize>,
    pub line: usize,
}

/// Per-layout state: numbering, options, wrap zones, geometry.
#[derive(Default)]
pub struct LayoutState {
    seen: usize,
    counters: [u32; 5],
    opts: Vec<BoxOpts>,
    nums: HashMap<usize, String>,
    active: HashSet<usize>,
    zones: Vec<Zone>,
    pub boxes: Vec<BoxGeom>,
}

/// Codes that may precede a box at the start of a line.
fn leading_code(c: &Code) -> bool {
    !matches!(
        c,
        Code::HRt
            | Code::HPg
            | Code::Tab
            | Code::Indent
            | Code::DblIndent
            | Code::MarRel
            | Code::Center
            | Code::FlushRight
            | Code::HardSpace
            | Code::HardHyphen
            | Code::Note { .. }
            | Code::TblDef { .. }
            | Code::Row
            | Code::Cell
            | Code::TblOff
    ) && !matches!(c, Code::GBox(b) if b.anchor == Anchor::Char)
}

impl LayoutState {
    pub fn new() -> LayoutState {
        LayoutState { opts: BoxKind::ALL.iter().map(|k| BoxOpts::default_for(*k)).collect(), ..Default::default() }
    }

    /// Numbering / options codes, each processed once in document order.
    pub fn visit(&mut self, i: usize, c: &Code) {
        if i < self.seen {
            return;
        }
        self.seen = i + 1;
        match c {
            Code::BoxOpt(o) => self.opts[o.kind.idx()] = (**o).clone(),
            Code::BoxNewNum(k, n) => self.counters[k.idx()] = n.saturating_sub(1),
            Code::GBox(b) => {
                let k = b.kind.idx();
                self.counters[k] += 1;
                let s = fmt_num(self.counters[k], self.opts[k].num1);
                self.nums.insert(i, s);
            }
            _ => {}
        }
    }

    pub fn num(&self, i: usize) -> String {
        self.nums.get(&i).cloned().unwrap_or_else(|| "1".into())
    }

    /// Paragraph/page boxes among the codes starting a line take effect
    /// for this line already.
    pub fn activate_leading(&mut self, doc: &Document, i: usize, cx: &Ctx, m: &dyn Metrics) {
        let mut k = i;
        while let Some(El::Code(c)) = doc.els.get(k) {
            if !leading_code(c) {
                break;
            }
            self.visit(k, c);
            if let Code::GBox(b) = c {
                self.activate(doc, k, b, cx, m);
            }
            k += 1;
        }
    }

    /// Boxes inside the line just finished take effect from the next line.
    pub fn activate_range(&mut self, doc: &Document, r: std::ops::Range<usize>, cx: &Ctx, m: &dyn Metrics) {
        for k in r {
            if let Some(El::Code(c @ Code::GBox(b))) = doc.els.get(k) {
                self.visit(k, c);
                if b.anchor != Anchor::Char {
                    self.activate(doc, k, b, cx, m);
                }
            }
        }
    }

    fn activate(&mut self, doc: &Document, k: usize, b: &GBox, cx: &Ctx, m: &dyn Metrics) {
        if b.anchor == Anchor::Char || !self.active.insert(k) {
            return;
        }
        let o = self.opts[b.kind.idx()].clone();
        let num = self.num(k);
        let avail = (cx.page_w - cx.left - cx.right).max(COL_WPU * 4);
        let body_h = (cx.page_h - cx.top - cx.bottom).max(1200);
        let (w, mut h) = frame_size(doc, b, &o, avail, body_h, m);
        let cap_h = caption_height(doc, b, &o, &num, w, m);
        if b.anchor == Anchor::Page && b.vpos == VPos::Full {
            h = (h - cap_h).max(COL_WPU);
        }
        let x = match b.hpos {
            HPos::Left => cx.left,
            HPos::Right => cx.page_w - cx.right - w,
            HPos::Center => cx.left + (avail - w) / 2,
            HPos::Full => cx.left,
            HPos::Set(v) => v,
        };
        let above = if o.cap_pos == 1 { cap_h } else { 0 };
        let outer_h = o.outside[2] + h + cap_h + o.outside[3];
        let mut bumped = false;
        let (mut dy, mut zone_page) = if b.anchor == Anchor::Para {
            let off = match b.vpos {
                VPos::Set(v) => v.max(0),
                _ => 0,
            };
            // below boxes of earlier paragraphs that are still in the way
            let (ox0, ox1) = (x - o.outside[0], x + w + o.outside[1]);
            let push = self.zones.iter().filter(|z| z.page.is_none() && z.x0 < ox1 && z.x1 > ox0).map(|z| z.remaining).max().unwrap_or(0);
            (off + push + o.outside[2] + above, None)
        } else {
            let top = match b.vpos {
                VPos::Full | VPos::Top => cx.top,
                VPos::Center => cx.top + (body_h - outer_h) / 2,
                VPos::Bottom => cx.page_h - cx.bottom - outer_h,
                VPos::Set(v) => v,
                VPos::Baseline => cx.top,
            };
            (top + o.outside[2] + above, Some((cx.page, top, top + outer_h)))
        };
        // WP bumps a paragraph box that does not fit to the next page
        if b.anchor == Anchor::Para && cx.y > cx.top + 10 && outer_h <= body_h && cx.y + dy - o.outside[2] - above + outer_h > cx.page_h - cx.bottom {
            bumped = true;
            dy = cx.top + o.outside[2] + above;
            zone_page = Some((cx.page + 1, cx.top, cx.top + outer_h));
        }
        if b.wrap {
            let remaining = if zone_page.is_some() { 0 } else { dy - o.outside[2] - above + outer_h };
            self.zones.push(Zone { x0: x - o.outside[0], x1: x + w + o.outside[1], remaining, page: zone_page });
        }
        self.boxes.push(BoxGeom {
            el: k,
            kind: b.kind,
            anchor: b.anchor,
            num,
            opts: o,
            x,
            y: 0,
            w,
            h,
            cap_h,
            page: cx.page,
            dy,
            vrow: cx.vrow,
            line: cx.line,
            bumped,
        });
    }

    /// Text margins for a line at (page, y) of height `lh`: Some((left,
    /// right)) or None when the line must be left empty (a box spans it).
    pub fn narrow(&self, lm: i32, rm: i32, page: u32, y: i32, lh: i32) -> Option<(i32, i32)> {
        let (mut l, mut r) = (lm, rm);
        for z in &self.zones {
            let hit = match z.page {
                Some((p, y0, y1)) => p == page && y < y1 && y + lh > y0,
                None => z.remaining > 0,
            };
            if !hit || z.x1 <= l || z.x0 >= r {
                continue;
            }
            if z.x0 <= l + COL_WPU && z.x1 >= r - COL_WPU {
                return None;
            }
            let left_room = z.x0 - l;
            let right_room = r - z.x1;
            if right_room >= left_room {
                l = l.max(z.x1);
            } else {
                r = r.min(z.x0);
            }
        }
        if r - l < 720 { None } else { Some((l, r)) }
    }

    pub fn consume(&mut self, h: i32, page: u32) {
        for z in &mut self.zones {
            if z.page.is_none() {
                z.remaining -= h;
            }
        }
        self.zones.retain(|z| match z.page {
            Some((p, ..)) => p >= page,
            None => z.remaining > 0,
        });
    }

    /// A character box in the line: (width, height, screen columns, label).
    pub fn char_box(&mut self, doc: &Document, i: usize, b: &GBox, m: &dyn Metrics) -> (i32, i32, i32, String) {
        let o = self.opts[b.kind.idx()].clone();
        let (w, h) = frame_size(doc, b, &o, b.width, 0, m);
        let cols = ((w + COL_WPU / 2) / COL_WPU).max(1);
        let label = format!("{} {}", b.kind.label(), self.num(i));
        (w, h, cols, label)
    }

    /// Record the geometry of character boxes on a finished line.
    pub fn record_char_boxes(&mut self, doc: &Document, line: &Line, li: usize, desc: i32, m: &dyn Metrics) {
        for k in line.start..line.end {
            if let Some(El::Code(Code::GBox(b))) = doc.els.get(k) {
                if b.anchor != Anchor::Char {
                    continue;
                }
                let o = self.opts[b.kind.idx()].clone();
                let (w, h) = frame_size(doc, b, &o, b.width, 0, m);
                let (_, x) = line.caret_of(k);
                self.boxes.push(BoxGeom { el: k, kind: b.kind, anchor: Anchor::Char, num: self.num(k), opts: o, x, y: 0, w, h, cap_h: 0, page: line.page, dy: desc, vrow: None, line: li, bumped: false });
            }
        }
    }

    pub fn has_zones(&self) -> bool {
        !self.zones.is_empty()
    }
}

/// After pagination: final page and y of every box.
pub fn finish(l: &mut Layout) {
    let mut boxes = std::mem::take(&mut l.boxes);
    for b in &mut boxes {
        let anchor_row = || -> (u32, i32) {
            if let Some(v) = b.vrow {
                if v < l.vrow_y.len() {
                    return (l.vrow_page[v], l.vrow_y[v]);
                }
            }
            let li = b.line.min(l.lines.len().saturating_sub(1));
            l.lines.get(li).map(|x| (x.page, x.y)).unwrap_or((1, 0))
        };
        match b.anchor {
            Anchor::Para if b.bumped => {
                b.page = anchor_row().0 + 1;
                b.y = b.dy;
            }
            Anchor::Para => {
                let (p, y) = anchor_row();
                b.page = p;
                b.y = y + b.dy;
            }
            Anchor::Page => {
                b.page = anchor_row().0;
                b.y = b.dy;
            }
            Anchor::Char => {
                if let Some(ln) = l.lines.get(b.line) {
                    b.page = ln.page;
                    b.y = ln.y + ln.height - b.dy - b.h;
                }
            }
        }
    }
    l.boxes = boxes;
}

/// Does this line hold a character box (text then sits on the box bottom)?
pub fn line_has_char_box(doc: &Document, line: &Line) -> bool {
    doc.els[line.start.min(doc.els.len())..line.end.min(doc.els.len())].iter().any(|e| matches!(e, El::Code(Code::GBox(b)) if b.anchor == Anchor::Char))
}

// ================================================================ pages

fn gray_color(gray: u8) -> u32 {
    let a = (gray.min(100) as u32 * 255 / 100) & 255;
    a << 24
}

fn fill(p: &mut Page, x: i32, y: i32, w: i32, h: i32, gray: u8) {
    if w <= 0 || h <= 0 {
        return;
    }
    if gray >= 100 {
        p.rules.push((x, y, w, h));
    } else if gray > 0 {
        p.images.push(Placed::solid(x, y, w, h, gray_color(gray)));
    }
}

fn border_side(p: &mut Page, x: i32, y: i32, len: i32, style: u8, horizontal: bool, inward: i32) {
    // `inward`: +1 the line grows right/down from (x,y), -1 left/up
    let seg = |p: &mut Page, a: i32, t: i32, l: i32, th: i32| {
        let off = if inward < 0 { -th } else { 0 };
        if horizontal {
            p.rules.push((x + a, y + t + off, l, th));
        } else {
            p.rules.push((x + t + off, y + a, th, l));
        }
    };
    match style {
        1 => seg(p, 0, 0, len, 16),
        2 => {
            seg(p, 0, 0, len, 12);
            seg(p, 0, 36 * inward.signum(), len, 12);
        }
        3 => {
            let mut a = 0;
            while a < len {
                seg(p, a, 0, 96.min(len - a), 16);
                a += 144;
            }
        }
        4 => {
            let mut a = 0;
            while a < len {
                seg(p, a, 0, 16.min(len - a), 16);
                a += 48;
            }
        }
        5 => seg(p, 0, 0, len, 75),
        6 => seg(p, 0, 0, len, 150),
        _ => {}
    }
}

fn margins_at(doc: &Document, idx: usize) -> (i32, i32, i32, i32) {
    let i = &doc.init;
    let (mut l, mut r, mut t, mut b) = (i.left, i.right, i.top, i.bottom);
    for e in doc.initial_codes.iter().chain(doc.els[..idx.min(doc.els.len())].iter()) {
        match e {
            El::Code(Code::LRMargin { left, right }) => {
                l = *left;
                r = *right;
            }
            El::Code(Code::TBMargin { top, bottom }) => {
                t = *top;
                b = *bottom;
            }
            _ => {}
        }
    }
    (l, r, t, b)
}

/// Draw boxes and graphic lines onto the physical pages.
pub fn draw(doc: &Document, layout: &Layout, pages: &mut Vec<Page>, m: &dyn Metrics) {
    // a box bumped past the last page of text gets its own page
    while let Some(last) = layout.boxes.iter().map(|b| b.page as usize).max().filter(|p| *p > pages.len() && *p <= pages.len() + 1) {
        let _ = last;
        let n = pages.len() as u32 + 1;
        pages.push(Page { number: n, width: doc.init.page_width, height: doc.init.page_height, ..Default::default() });
    }
    for g in &layout.boxes {
        let Some(El::Code(Code::GBox(b))) = doc.els.get(g.el) else { continue };
        let Some(p) = pages.get_mut(g.page.max(1) as usize - 1) else { continue };
        draw_box(doc, b, g, p, m);
    }
    for (idx, e) in doc.els.iter().enumerate() {
        let El::Code(Code::GLine(gl)) = e else { continue };
        let li = layout.line_of(idx);
        let Some(line) = layout.lines.get(li) else { continue };
        let Some(p) = pages.get_mut(line.page.max(1) as usize - 1) else { continue };
        let (l, r, t, b) = margins_at(doc, idx);
        let pw = doc.init.page_width;
        let ph = doc.init.page_height;
        let avail = pw - l - r;
        if gl.vertical {
            let body = ph - t - b;
            let len = if gl.valign == 0 || gl.length <= 0 { body } else { gl.length };
            let x = match gl.halign {
                0 => l - 100 - gl.width,
                1 => pw - r + 100,
                2 => l + avail / 2 - gl.width / 2,
                _ => gl.x,
            };
            let y = match gl.valign {
                0 | 1 => t,
                2 => t + (body - len) / 2,
                3 => ph - b - len,
                _ => gl.y,
            };
            fill(p, x, y, gl.width.max(1), len, gl.gray);
        } else {
            let len = if gl.halign == 3 || gl.length <= 0 { avail } else { gl.length };
            let x = match gl.halign {
                0 | 3 => l,
                1 => pw - r - len,
                2 => l + (avail - len) / 2,
                _ => gl.x,
            };
            let y = if gl.valign == 0 { line.y + line.height * 4 / 5 - gl.width } else { gl.y };
            fill(p, x, y, len, gl.width.max(1), gl.gray);
        }
    }
}

fn draw_box(doc: &Document, b: &GBox, g: &BoxGeom, p: &mut Page, m: &dyn Metrics) {
    let o = &g.opts;
    let (x, y, w, h) = (g.x, g.y, g.w, g.h);
    if o.gray > 0 {
        p.images.push(Placed::solid(x, y, w, h, gray_color(o.gray)));
    }
    let (ix, iy) = (x + o.inside[0], y + o.inside[2]);
    let iw = (w - o.inside[0] - o.inside[1]).max(1);
    let ih = (h - o.inside[2] - o.inside[3]).max(1);
    match b.contents {
        Contents::Image => {
            if let Some(r) = b.data.as_ref().and_then(|d| crate::raster::decode(d)) {
                let (nw, nh) = b.data.as_ref().and_then(|d| crate::raster::dims(d)).unwrap_or((r.w as u32, r.h as u32));
                // fit inside, keep aspect
                let (nw, nh) = (nw.max(1) as f64, nh.max(1) as f64);
                let k = (iw as f64 / nw).min(ih as f64 / nh);
                let dw = (nw * k * b.img.sx as f64 / 100.0) as i32;
                let dh = (nh * k * b.img.sy as f64 / 100.0) as i32;
                let dx = ix + (iw - dw) / 2 + b.img.dx;
                let dy = iy + (ih - dh) / 2 - b.img.dy;
                p.images.push(Placed {
                    clip: (ix, iy, iw, ih),
                    img: Some(r),
                    color: 0,
                    dst: (dx, dy, dw.max(1), dh.max(1)),
                    rot: b.img.rot,
                    mirror: b.img.mirror,
                    invert: b.img.invert,
                    bw: b.img.bw,
                });
            }
        }
        Contents::Text => p.runs.extend(runs_in(doc, &b.text, m, ix, iy, iw)),
        Contents::Equation => {
            let e = crate::equation::layout_fit(&b.equation, eq_size(doc), iw, m);
            let ox = ix + (iw - e.w) / 2;
            let base = iy + (ih - e.asc - e.desc) / 2 + e.asc;
            for mut r in e.runs {
                r.x += ox;
                r.y += base;
                p.runs.push(r);
            }
            for (rx, ry, rw, rh) in e.rules {
                p.rules.push((rx + ox, ry + base, rw, rh));
            }
        }
        Contents::Empty => {}
    }
    let bs = o.border;
    border_side(p, x, y, h, bs[0], false, 1);
    border_side(p, x + w, y, h, bs[1], false, -1);
    border_side(p, x, y, w, bs[2], true, 1);
    border_side(p, x, y + h, w, bs[3], true, -1);
    if !b.caption.is_empty() && g.cap_h > 0 {
        let mut els = vec![El::Code(Code::Just(Justify::Left))];
        els.extend(expand_caption(&b.caption, o, &g.num));
        let cy = if o.cap_pos == 1 { y - g.cap_h } else { y + h + 60 };
        p.runs.extend(runs_in(doc, &els, m, x, cy, w));
    }
}

// ================================================================ edit screen

/// Box outlines on the text-mode editing screen (`┌FIG 1───┐`).
pub fn draw_outlines(g: &mut crate::grid::Grid, e: &crate::editor::Editor, doc_rows: usize, st: crate::grid::Style) {
    let lay = &e.layout;
    let left = e.left_col;
    for b in &lay.boxes {
        if b.anchor == Anchor::Char {
            continue;
        }
        let (_, oy0, _, oy1) = b.outer();
        let mut rows = vec![];
        for row in 0..doc_rows {
            let Some(crate::layout::DRow::V(v)) = lay.drows.get(e.top_row + row).copied() else { continue };
            let (Some(&pg), Some(&vy)) = (lay.vrow_page.get(v), lay.vrow_y.get(v)) else { continue };
            if pg == b.page && vy + 10 >= oy0 && vy < oy1 - 10 {
                rows.push(row);
            }
        }
        if rows.is_empty() {
            continue;
        }
        let c0 = b.x / COL_WPU - left;
        let c1 = ((b.x + b.w) / COL_WPU - left - 1).max(c0 + 1);
        let label = format!("{} {}", b.kind.label(), b.num);
        let put = |g: &mut crate::grid::Grid, c: i32, r: usize, ch: char| {
            if c < 0 || c as usize >= g.cols {
                return;
            }
            if let Some(cell) = g.get_mut(c as usize, r) {
                if cell.ch == ' ' || "┌┐└┘─│".contains(cell.ch) {
                    g.put(c as usize, r, ch, st);
                }
            }
        };
        let (first, last) = (rows[0], *rows.last().unwrap());
        if first == last {
            put(g, c0, first, '[');
            for (k, ch) in label.chars().enumerate() {
                if c0 + 1 + (k as i32) < c1 {
                    put(g, c0 + 1 + k as i32, first, ch);
                }
            }
            put(g, c1, first, ']');
            continue;
        }
        for &r in &rows {
            let (lc, mc, rc) = if r == first {
                ('┌', '─', '┐')
            } else if r == last {
                ('└', '─', '┘')
            } else {
                ('│', ' ', '│')
            };
            put(g, c0, r, lc);
            if mc != ' ' {
                for c in c0 + 1..c1 {
                    put(g, c, r, mc);
                }
            }
            put(g, c1, r, rc);
        }
        let lr = rows[1.min(rows.len() - 1)];
        let lr = if lr == last { first } else { lr };
        for (k, ch) in label.chars().enumerate() {
            let c = c0 + 1 + k as i32;
            if c < c1 {
                if let Some(cell) = (c >= 0).then(|| g.get_mut(c as usize, lr)).flatten() {
                    if cell.ch == ' ' || "─".contains(cell.ch) {
                        g.put(c as usize, lr, ch, st);
                    }
                }
            }
        }
    }
}

/// The `[Fig Opt]`… in effect before element `idx`.
pub fn opts_before(els: &[El], idx: usize, kind: BoxKind) -> BoxOpts {
    els[..idx.min(els.len())]
        .iter()
        .rev()
        .find_map(|e| match e {
            El::Code(Code::BoxOpt(o)) if o.kind == kind => Some((**o).clone()),
            _ => None,
        })
        .unwrap_or_else(|| BoxOpts::default_for(kind))
}

/// The number of the box at `idx` counted within `els`.
pub fn number_before(els: &[El], idx: usize) -> String {
    let mut st = LayoutState::new();
    for (i, e) in els.iter().enumerate().take(idx + 1) {
        if let El::Code(c) = e {
            st.visit(i, c);
        }
    }
    st.num(idx)
}

/// Box number of the box at element `idx` (for Reveal Codes).
pub fn box_number(lay: &Layout, idx: usize) -> String {
    lay.boxes.iter().find(|b| b.el == idx).map(|b| b.num.clone()).unwrap_or_else(|| "?".into())
}

/// Element indices of boxes of a kind, with their numbers, in order.
pub fn boxes_of(doc: &Document, kind: BoxKind) -> Vec<(usize, String)> {
    let mut st = LayoutState::new();
    let mut out = vec![];
    for (i, e) in doc.els.iter().enumerate() {
        if let El::Code(c) = e {
            st.visit(i, c);
            if let Code::GBox(b) = c {
                if b.kind == kind {
                    out.push((i, st.num(i)));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{MonoMetrics, VRow};

    fn figure() -> GBox {
        let mut b = GBox::new(BoxKind::Figure);
        b.set_image("square.wpg", crate::wpg::tests_sample());
        b.caption = vec![El::Code(Code::BoxNum), El::Ch(' '), El::Ch('X')];
        b
    }

    #[test]
    fn base64() {
        for n in 0..10 {
            let d: Vec<u8> = (0..n).map(|k| (k * 37 + 5) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&d)), d);
        }
    }

    #[test]
    fn stream_roundtrip() {
        let mut d = Document::from_text("Hallo\n");
        let mut b = figure();
        b.text = vec![El::Ch('t'), El::Code(Code::HRt), El::Ch('|')];
        b.equation = "x over y | z".into();
        b.img.rot = 45;
        d.els.push(El::Code(Code::GBox(Box::new(b))));
        let mut o = BoxOpts::default_for(BoxKind::Table);
        o.gray = 30;
        d.els.push(El::Code(Code::BoxOpt(Box::new(o))));
        d.els.push(El::Code(Code::BoxNewNum(BoxKind::Text, 5)));
        d.els.push(El::Code(Code::GLine(GLine::new(true, 3600))));
        let s = crate::stream::write(&d);
        let d2 = crate::stream::read(&s).unwrap();
        assert_eq!(d.els, d2.els);
    }

    #[test]
    fn reveal_names() {
        let b = figure();
        assert_eq!(reveal(&Code::GBox(Box::new(b))).unwrap(), "[Fig Box:#;SQUARE.WPG; X]");
        assert_eq!(reveal(&Code::BoxOpt(Box::new(BoxOpts::default_for(BoxKind::Text)))).unwrap(), "[Txt Opt]");
        assert_eq!(reveal(&Code::BoxNewNum(BoxKind::Figure, 3)).unwrap(), "[New Fig Num:3]");
        assert_eq!(reveal(&Code::GLine(GLine::new(false, 7800))).unwrap(), "[HLine:Full,Baseline,6.5\",0.013\",100%]");
        assert_eq!(fmt_num(4, 3), "IV");
        assert_eq!(fmt_num(28, 2), "AB");
    }

    #[test]
    fn caption_style() {
        let o = BoxOpts::default_for(BoxKind::Figure);
        let els = expand_caption(&[El::Code(Code::BoxNum), El::Ch('!')], &o, "2");
        let d = Document { els: els.clone(), ..Default::default() };
        assert_eq!(d.plain_text(0, els.len()), "Figure 2!");
        assert_eq!(els[0], El::Code(Code::AttrOn(Attr::Bold)));
    }

    #[test]
    fn wrap_and_numbering() {
        let mut d = Document::from_text(&"word ".repeat(200));
        let mut b = figure();
        b.width = 2400;
        d.els.insert(0, El::Code(Code::GBox(Box::new(b.clone()))));
        let mut full = b.clone();
        full.hpos = HPos::Full;
        full.auto = 0;
        full.height = 1800;
        d.els.push(El::Code(Code::HRt));
        d.els.push(El::Code(Code::GBox(Box::new(full))));
        d.els.extend("after".chars().map(El::Ch));
        let l = crate::layout::layout(&d, &MonoMetrics);
        assert_eq!(l.boxes.len(), 2);
        assert_eq!(l.boxes[0].num, "1");
        assert_eq!(l.boxes[1].num, "2");
        // first lines are shortened by the right-aligned 2" box
        let first = &l.lines[0];
        assert!(first.right <= d.init.page_width - d.init.right - 2400, "{}", first.right);
        // the full-width box leaves empty rows
        assert!(l.vrows.iter().any(|v| matches!(v, VRow::Gap(_))));
        let after = l.lines.iter().rfind(|x| x.end == d.els.len() && x.end > x.start).unwrap();
        assert!(after.y >= l.boxes[1].y + l.boxes[1].h, "text below the box: {} {:?}", after.y, l.boxes[1]);
        // pages carry an image for each box
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        assert!(pages[0].images.len() >= 2);
    }

    /// One of her WPG clip-art files in a figure box, rendered like View
    /// Document: the yellow bulb shows up inside the box frame.
    #[test]
    fn her_wpg_on_page() {
        let Some(bytes) = ["/mnt/i/Claude Space/wp51/WPG/BULB.WPG", "I:\\Claude Space\\wp51\\WPG\\BULB.WPG"].iter().find_map(|p| std::fs::read(p).ok()) else { return };
        let mut d = Document::from_text("Text neben dem Bild.\n");
        let mut b = GBox::new(BoxKind::Figure);
        b.set_image("BULB.WPG", bytes);
        d.els.insert(0, El::Code(Code::GBox(Box::new(b))));
        let l = crate::layout::layout(&d, &MonoMetrics);
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        let scale = 0.1;
        let (w, h) = ((pages[0].width as f64 * scale) as usize, (pages[0].height as f64 * scale) as usize);
        let mut buf = vec![0u32; w * h];
        crate::pagerender::render_page(&pages[0], &mut buf, w, h, 0, 0, scale);
        let g = &l.boxes[0];
        let (x0, y0) = ((g.x as f64 * scale) as usize, (g.y as f64 * scale) as usize);
        let (x1, y1) = (((g.x + g.w) as f64 * scale) as usize, ((g.y + g.h) as f64 * scale) as usize);
        let mut yellow = 0;
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                let p = buf[y * w + x];
                if (p >> 16) & 255 > 200 && (p >> 8) & 255 > 200 && p & 255 < 140 {
                    yellow += 1;
                }
            }
        }
        assert!(yellow > 200, "yellow pixels: {yellow}");
        // the frame (single border) is drawn
        assert!((y0..y0 + 2).any(|y| buf[y * w + (x0 + x1) / 2] & 0xFFFFFF == 0));
    }

    #[test]
    fn box_too_tall_moves_to_next_page() {
        let mut d = Document::from_text(&"Zeile\n".repeat(40));
        let mut b = GBox::new(BoxKind::User);
        b.auto = 0;
        b.width = 2400;
        b.height = 6000;
        d.els.push(El::Code(Code::GBox(Box::new(b))));
        d.els.extend("Rest".chars().map(El::Ch));
        let l = crate::layout::layout(&d, &MonoMetrics);
        assert_eq!(l.boxes[0].page, 2);
        assert_eq!(l.boxes[0].y, d.init.top + 200);
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        assert!(pages.len() >= 2);
    }

    #[test]
    fn char_box_in_line() {
        let mut d = Document::from_text("ab");
        let mut b = figure();
        b.anchor = Anchor::Char;
        b.width = 600;
        b.caption.clear();
        d.els.insert(1, El::Code(Code::GBox(Box::new(b))));
        let l = crate::layout::layout(&d, &MonoMetrics);
        assert_eq!(l.boxes.len(), 1);
        assert!(l.lines[0].height >= 600);
        let (_, xb) = l.lines[0].caret_of(2);
        let (_, xa) = l.lines[0].caret_of(1);
        assert_eq!(xb - xa, 600);
    }
}
