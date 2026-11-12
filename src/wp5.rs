//! WordPerfect 5.0/5.1 document import, per docs/formats/wp5_document.md.
//!
//! The body is walked exactly like WP walks it: text bytes, single-byte
//! functions, fixed-length functions (0xC0–0xCF) and variable-length
//! functions (0xD0–0xFF).  Everything our model understands becomes a real
//! code; everything else becomes `Code::Unknown { name, data }` where
//! `name` is the Reveal Codes text and `data` the raw bytes of the whole
//! function, so a later WP exporter can write it back unchanged.
//!
//! Formatter bookkeeping that WP regenerates on retrieve (D4 group, end of
//! indent, soft page/return variants, C/A/Flrt ends …) is dropped.

use crate::doc::*;
use crate::wpchars::wp_to_string;
use std::collections::HashMap;

/// Total size of fixed-length functions 0xC0..=0xCF (both code bytes included).
const FIXED: [usize; 16] = [4, 9, 11, 3, 3, 5, 6, 7, 4, 5, 6, 6, 8, 10, 10, 12];

/// WP attribute number (C3/C4) → our attribute.
const ATTRS: [Attr; 16] = [
    Attr::ExtraLarge,
    Attr::VeryLarge,
    Attr::Large,
    Attr::Small,
    Attr::Fine,
    Attr::Superscript,
    Attr::Subscript,
    Attr::Outline,
    Attr::Italic,
    Attr::Shadow,
    Attr::Redline,
    Attr::DblUnderline,
    Attr::Bold,
    Attr::Strikeout,
    Attr::Underline,
    Attr::SmallCaps,
];

/// Merge commands DE/0x20.. (TK).
const MERGE: [&str; 68] = [
    "ASSIGN", "BELL", "BREAK", "CALL", "CANCEL OFF", "CANCEL ON", "CASE", "CASE CALL", "CHAIN MACRO",
    "CHAIN PRIMARY", "CHAIN SECONDARY", "CHAR", "COMMENT", "CTON", "DATE", "DOCUMENT", "ELSE", "END FIELD",
    "END FOR", "END IF", "END RECORD", "END WHILE", "FIELD", "FOR", "FOR EACH", "GO", "IF", "IF BLANK",
    "IF EXISTS", "IF NOT BLANK", "KEYBOARD", "LABEL", "LOCAL", "LOOK", "MID", "MRG CMND", "NEST MACRO",
    "NEST PRIMARY", "NEST SECONDARY", "NEXT", "NEXT RECORD", "NTOC", "PROCESS", "ON CANCEL", "ON ERROR",
    "PAGE OFF", "PAGE ON", "PRINT", "PROMPT", "QUIT", "RETURN", "RETURN CANCEL", "RETURN ERROR", "REWRITE",
    "STEP OFF", "STEP ON", "SUBST PRIMARY", "SUBST SECONDARY", "SYSTEM", "TEXT", "VARIABLE", "WAIT", "WHILE",
    "STATUS PROMPT", "INPUT", "LEN", "FIELD NAMES", "END MRG CMNDS",
];

/// Maximum nesting of text fields (header → box → footnote …).
const MAX_DEPTH: u32 = 8;

/// Length of the abbreviated nested text shown in an Unknown code's name.
const BRIEF_LEN: usize = 40;

// ------------------------------------------------------------------ bytes

fn u16_at(b: &[u8], o: usize) -> u16 {
    match b.get(o..o + 2) {
        Some(s) => u16::from_le_bytes([s[0], s[1]]),
        None => 0,
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    match b.get(o..o + 4) {
        Some(s) => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        None => 0,
    }
}

/// A variable-length function.  Offsets passed to the accessors are TK
/// offsets, i.e. measured from the code byte; they read only the payload
/// (never the trailing `len sub code`) and yield 0 / empty when out of range.
struct Grp<'a> {
    p: &'a [u8],
}

impl<'a> Grp<'a> {
    fn b(&self, k: usize) -> u8 {
        k.checked_sub(4).and_then(|i| self.p.get(i)).copied().unwrap_or(0)
    }
    fn w(&self, k: usize) -> u16 {
        k.checked_sub(4).map(|i| u16_at(self.p, i)).unwrap_or(0)
    }
    fn sw(&self, k: usize) -> i16 {
        self.w(k) as i16
    }
    fn from(&self, k: usize) -> &'a [u8] {
        let i = k.saturating_sub(4);
        self.p.get(i..).unwrap_or(&[])
    }
    fn range(&self, k: usize, n: usize) -> &'a [u8] {
        let i = k.saturating_sub(4).min(self.p.len());
        &self.p[i..(i + n).min(self.p.len())]
    }
    fn has(&self, k: usize) -> bool {
        k >= 4 && k - 4 < self.p.len()
    }
}

/// Length of a WP string up to its NUL, skipping over `C0 c s C0` codes
/// (which may contain NUL bytes).
fn cstr_len(b: &[u8]) -> usize {
    let mut j = 0;
    while j < b.len() {
        if b[j] == 0xC0 && j + 3 < b.len() && b[j + 3] == 0xC0 {
            j += 4;
        } else if b[j] == 0 {
            break;
        } else {
            j += 1;
        }
    }
    j
}

/// Decode a WP string (ASCII + extended characters) up to its NUL.
fn wpstr(b: &[u8]) -> String {
    let b = &b[..cstr_len(b)];
    let mut s = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == 0xC0 && i + 3 < b.len() && b[i + 3] == 0xC0 {
            s.push_str(&wp_to_string(b[i + 2], b[i + 1] as u16));
            i += 4;
            continue;
        }
        match c {
            0x20..=0x7E => s.push(c as char),
            0xA9..=0xAB => s.push('-'),
            0xA0 => s.push('\u{a0}'),
            0x0A => s.push('\n'),
            0x0D => s.push(' '),
            0xC1..=0xCF => {
                // Skip a whole fixed function if it is well framed.
                let sz = FIXED[(c - 0xC0) as usize];
                if i + sz <= b.len() && b[i + sz - 1] == c {
                    i += sz;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    s
}

/// Read a NUL-terminated WP string at `*pos`, advancing past the NUL.
fn next_cstr(b: &[u8], pos: &mut usize) -> String {
    let start = (*pos).min(b.len());
    let n = cstr_len(&b[start..]);
    *pos = start + n + 1;
    wpstr(&b[start..start + n])
}

/// WPU → WP 5.1 Reveal Codes inches (up to 3 decimals).
fn inch(w: i32) -> String {
    let s = format!("{:.3}", w as f64 / 1200.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    let s = if s == "-0" { "0" } else { s };
    format!("{s}\"")
}

/// Abbreviated Reveal-Codes-style text of a nested stream.
fn brief(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(c) => s.push_str(&crate::reveal::code_text(c)),
        }
        if s.chars().count() > BRIEF_LEN {
            let t: String = s.chars().take(BRIEF_LEN).collect();
            return format!("{t}...");
        }
    }
    s
}

/// Plain text of a nested stream (for comments).
fn plain(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(Code::HRt) => s.push('\n'),
            El::Code(Code::Tab | Code::Indent | Code::DblIndent) => s.push('\t'),
            El::Code(Code::HardSpace) => s.push(' '),
            El::Code(Code::HardHyphen) => s.push('-'),
            _ => {}
        }
    }
    s
}

fn unknown(name: impl Into<String>, data: &[u8]) -> El {
    let name = name.into();
    // Format codes we model (fmt.rs) become real codes.
    if let Some(c) = crate::fmt::from_wp5(&name, data) {
        return El::Code(c);
    }
    El::Code(Code::Unknown { name, data: data.to_vec() })
}

/// Paper size of a `[Paper Sz/Typ]` code (imported or still raw).
fn paper_of(c: &Code) -> Option<(i32, i32)> {
    match c {
        Code::Fmt(crate::fmt::Fmt::Paper(p)) if p.w > 0 && p.h > 0 => Some((p.w, p.h)),
        Code::Unknown { data, .. } => paper_dims(data),
        _ => None,
    }
}

fn code(c: Code) -> El {
    El::Code(c)
}

// ------------------------------------------------------------------ fonts / prefix

#[derive(Default)]
struct Ctx {
    /// Packet 7: font name pool.
    pool: Vec<u8>,
    /// Packet 15/2 records: (offset of name in pool, point size in 1/100 pt).
    fonts: Vec<(usize, u32)>,
    /// Packet 8: embedded WPG images.
    images: Vec<Vec<u8>>,
}

impl Ctx {
    fn font(&self, n: usize) -> Option<(String, u32)> {
        let &(off, size) = self.fonts.get(n)?;
        let name = self.pool.get(off..).map(wpstr).unwrap_or_default();
        let name = name.trim().to_string();
        Some((if name.is_empty() { format!("Font #{n}") } else { name }, size))
    }
}

/// Packet list: type → (offset, length); the last non-deleted one wins.
fn read_prefix(d: &[u8], docoff: usize) -> HashMap<u16, (usize, usize)> {
    let mut out = HashMap::new();
    let mut pos = 16usize;
    let mut seen = Vec::new();
    while pos != 0 && !seen.contains(&pos) && pos + 50 <= d.len() && seen.len() < 10_000 {
        seen.push(pos);
        if u16_at(d, pos) != 0xFFFB {
            break;
        }
        let n = (u16_at(d, pos + 2) as usize).clamp(1, 5);
        for k in 1..n {
            let o = pos + 10 * k;
            let t = u16_at(d, o);
            let ln = u32_at(d, o + 2) as usize;
            let off = u32_at(d, o + 6) as usize;
            if t == 0 || t >= 0xFFFC || off >= d.len() {
                continue;
            }
            let ln = if t == 8 { d.len() - off } else { ln.min(d.len() - off) };
            out.insert(t, (off, ln));
        }
        pos = u32_at(d, pos + 6) as usize;
        if pos >= docoff.max(16) && pos >= d.len() {
            break;
        }
    }
    out
}

fn read_fonts(d: &[u8], pk: &HashMap<u16, (usize, usize)>) -> Ctx {
    let mut ctx = Ctx::default();
    if let Some(&(o, l)) = pk.get(&7) {
        ctx.pool = d[o..o + l].to_vec();
    }
    let (rec, size_off, blob) = if let Some(&(o, l)) = pk.get(&15) {
        (86usize, 47usize, &d[o..o + l])
    } else if let Some(&(o, l)) = pk.get(&2) {
        // WP 5.0: TK says 86-byte records, real files use 82.
        let rs = if l % 86 != 0 && l % 82 == 0 { 82 } else { 86 };
        (rs, 22, &d[o..o + l])
    } else {
        return ctx;
    };
    for r in blob.chunks_exact(rec) {
        let name = u16_at(r, 18) as usize;
        let size = u16_at(r, size_off) as u32 * 2; // 3600ths of an inch → 1/100 pt
        ctx.fonts.push((name, size));
    }
    ctx
}

fn read_summary(b: &[u8]) -> Summary {
    let mut s = Summary::default();
    let wp51 = b.last() == Some(&0xFF);
    let mut p = 0;
    let f = |p: &mut usize| next_cstr(b, p).trim().to_string();
    let _created = f(&mut p);
    s.descriptive_name = f(&mut p);
    if wp51 {
        let _ty = f(&mut p);
    }
    s.subject = f(&mut p);
    s.author = f(&mut p);
    s.typist = f(&mut p);
    s.comments = f(&mut p);
    s
}

// ------------------------------------------------------------------ body

struct Parser<'c> {
    ctx: &'c Ctx,
    /// Names of the styles defined in the file: their cached expansions
    /// in the body are dropped (the definition is applied instead).
    styles: Vec<String>,
}

/// Paper size of a D0/0B group: (width, length) in WPU.
fn paper_dims(data: &[u8]) -> Option<(i32, i32)> {
    if data.len() < 110 || data[0] != 0xD0 || data[1] != 0x0B {
        return None;
    }
    let len = u16_at(data, 99) as i32;
    let wid = u16_at(data, 101) as i32;
    (len > 0 && wid > 0).then_some((wid, len))
}

impl Parser<'_> {
    /// Walk a body stream (the document, or a nested text field).
    fn decode(&self, b: &[u8], depth: u32) -> Vec<El> {
        let mut out: Vec<El> = Vec::new();
        // table state: rows seen, cells in current row
        let mut rows = 0u32;
        let mut cells = 0u32;
        // start of a cached style expansion (after a style code)
        let mut style_mark: Option<usize> = None;
        let n = b.len();
        let mut i = 0;
        while i < n {
            let c = b[i];
            match c {
                0x20..=0x7E => {
                    // ASCII '-' is WP's "hyphen character" (Home,-); the
                    // ordinary typed hyphen [-] is 0xA9.
                    out.push(if c == b'-' { code(Code::HardHyphen) } else { El::Ch(c as char) });
                    i += 1;
                }
                0x00..=0x1F => {
                    match c {
                        0x0A => out.push(code(Code::HRt)),
                        0x0C => out.push(code(Code::HPg)),
                        // SRt / SPg replaced the space at the wrap point.
                        0x0B | 0x0D => out.push(El::Ch(' ')),
                        // ^B page number: the renderer substitutes it.
                        0x02 => out.push(El::Ch('\u{2}')),
                        0x03..=0x07 | 0x0E..=0x16 => {
                            let name = format!("Mrg:^{}", (b'@' + c) as char);
                            out.push(unknown(name, &b[i..i + 1]));
                        }
                        _ => {}
                    }
                    i += 1;
                }
                0x7F => i += 1,
                0x80..=0xBF => {
                    if let Some(e) = self.single(c, &b[i..i + 1]) {
                        out.push(e);
                    }
                    i += 1;
                }
                0xC0..=0xCF => {
                    let size = FIXED[(c - 0xC0) as usize];
                    if i + size > n || b[i + size - 1] != c {
                        i += 1; // stray byte: resync
                        continue;
                    }
                    let g = &b[i..i + size];
                    self.fixed(c, g, &mut out);
                    i += size;
                }
                0xD0..=0xFF => {
                    if i + 4 > n {
                        i += 1;
                        continue;
                    }
                    let sub = b[i + 1];
                    let ln = u16_at(b, i + 2) as usize;
                    let end = i + 4 + ln;
                    if ln < 4 || end > n || b[end - 1] != c || b[end - 2] != sub || u16_at(b, end - 4) as usize != ln {
                        i += 1;
                        continue;
                    }
                    let raw = &b[i..end];
                    let g = Grp { p: &b[i + 4..end - 4] };
                    // tables: make the cell text readable inline
                    match (c, sub) {
                        (0xD2, 0x0B) => {
                            rows = 0;
                            cells = 0;
                        }
                        (0xDC, 0) => {
                            if cells > 0 {
                                out.push(code(Code::Tab));
                            }
                            cells += 1;
                        }
                        (0xDC, 1) | (0xDD, 1) | (0xDD, 3) => {
                            if rows > 0 {
                                out.push(code(Code::HRt));
                            }
                            rows += 1;
                            cells = 0;
                        }
                        (0xDC, 2) | (0xDD, 2) => {
                            rows = 0;
                            cells = 0;
                        }
                        _ => {}
                    }
                    self.var(c, sub, &g, raw, depth, &mut out);
                    match (c, sub) {
                        (0xDB, 0..=2) => {
                            style_mark = match out.last() {
                                Some(El::Code(Code::StyleOn(n) | Code::StyleOff(n) | Code::OpenStyle(n))) if self.styles.contains(n) => Some(out.len()),
                                _ => None,
                            };
                        }
                        (0xDB, 3) => {
                            if let Some(m) = style_mark.take() {
                                out.truncate(m);
                            }
                        }
                        _ => {}
                    }
                    i = end;
                }
            }
        }
        out
    }

    fn nested(&self, b: &[u8], depth: u32) -> Vec<El> {
        if depth >= MAX_DEPTH { Vec::new() } else { self.decode(b, depth + 1) }
    }

    fn single(&self, c: u8, raw: &[u8]) -> Option<El> {
        let u = |n: &str| Some(unknown(n, raw));
        match c {
            0x81 => Some(code(Code::Just(Justify::Full))),
            0x82 => Some(code(Code::Just(Justify::Left))),
            0x86 => Some(code(Code::CenterPage)),
            0x87 => Some(code(Code::ColOn)),
            0x88 | 0xAF | 0xB0 => Some(code(Code::ColOff)),
            0x8A => Some(code(Code::WidowOrphan(true))),
            0x8B => Some(code(Code::WidowOrphan(false))),
            0x8C | 0x99 => Some(code(Code::HRt)),
            0x8D => u("Note Num"),
            0x8E => Some(code(Code::BoxNum)),
            0x93..=0x95 => u("ISRt"),
            0x9A => u("/"),
            0x9B => Some(code(Code::Mark(Mark::EndDef))),
            0x9E => Some(code(Code::Hyphenation(false))),
            0x9F => Some(code(Code::Hyphenation(true))),
            0xA0 => Some(code(Code::HardSpace)),
            0xA1 => Some(code(Code::MathOp('+'))),
            0xA2 => Some(code(Code::MathOp('t'))),
            0xA3 => Some(code(Code::MathOp('='))),
            0xA4 => Some(code(Code::MathOp('T'))),
            0xA5 => Some(code(Code::MathOp('*'))),
            0xA6 => Some(code(Code::MathOp('!'))),
            0xA7 => Some(code(Code::Math(true))),
            0xA8 => Some(code(Code::Math(false))),
            0xA9..=0xAB => Some(El::Ch('-')),
            0xAC..=0xAE => Some(code(Code::SoftHyphen)),
            0xB1 => Some(code(Code::MathOp('N'))),
            0xB2 => Some(code(Code::Outline(false))),
            0xB3 => u("Rev Dir On"),
            0xB4 => u("Rev Dir Off"),
            0xBF => u("Unknown"),
            // 0x80 0x84 0x85 0x89 0x92 0x96–0x98 0x9C 0x9D 0xB5–0xBE:
            // bookkeeping / reserved.  0x83 0x8F (C/A/Flrt): our center
            // and flush-right codes end at the next tab/HRt by themselves.
            // 0x90 0x91: deletable soft returns.
            _ => None,
        }
    }

    fn fixed(&self, c: u8, g: &[u8], out: &mut Vec<El>) {
        match c {
            0xC0 => out.extend(wp_to_string(g[2], g[1] as u16).chars().map(El::Ch)),
            0xC1 => {
                let f = g[1];
                let kind = f >> 6;
                let e = match kind {
                    2 => Code::MarRel,
                    3 if f & 0x20 != 0 || f & 0x08 == 0 => Code::Center,
                    1 if f & 0x20 != 0 && f & 0x10 != 0 => Code::FlushRightDot,
                    1 if f & 0x20 != 0 => Code::FlushRight,
                    _ => Code::Tab,
                };
                out.push(code(e));
            }
            0xC2 => out.push(code(if g[1] & 1 != 0 { Code::DblIndent } else { Code::Indent })),
            0xC3 | 0xC4 => {
                let a = g[1] as usize;
                if a < 16 {
                    let at = ATTRS[a];
                    out.push(code(if c == 0xC3 { Code::AttrOn(at) } else { Code::AttrOff(at) }));
                } else {
                    out.push(unknown(if c == 0xC3 { "Attr On" } else { "attr off" }, g));
                }
            }
            0xC5 => out.push(unknown(if g[1] & 1 != 0 { "Block Pro:Off" } else { "Block Pro:On" }, g)),
            0xC6 => {} // end of indent: regenerated
            0xC7 => {
                let w = u16_at(g, 2);
                out.extend(wp_to_string((w >> 8) as u8, w & 0xFF).chars().map(El::Ch));
            }
            _ => out.push(unknown("Unknown", g)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn var(&self, c: u8, s: u8, g: &Grp, raw: &[u8], depth: u32, out: &mut Vec<El>) {
        macro_rules! push_u {
            ($e:expr) => {{
                let n: String = $e.into();
                out.push(unknown(n, raw))
            }};
        }
        match c {
            0xD0 => match s {
                0 => {
                    let h = g.w(6);
                    push_u!(format!("Ln Height:{}", if h == 0 { "Auto".into() } else { inch(h as i32) }));
                }
                1 => out.push(code(Code::LRMargin { left: g.w(8) as i32, right: g.w(10) as i32 })),
                2 => {
                    let v = g.w(6) as u32;
                    let hund = (v >> 8) * 100 + ((v & 0xFF) * 100 + 128) / 256;
                    out.push(code(Code::LineSpacing(hund.max(1))));
                }
                3 => push_u!(format!(
                    "HZone:{}%,{}%",
                    (g.b(6) as u32 * 100 + 128) / 256,
                    (g.b(7) as u32 * 100 + 128) / 256
                )),
                4 => out.push(code(Code::TabSet(tab_set(g)))),
                5 => out.push(code(Code::TBMargin { top: g.w(8) as i32, bottom: g.w(10) as i32 })),
                6 => match g.b(5) {
                    0 => out.push(code(Code::Just(Justify::Left))),
                    1 => out.push(code(Code::Just(Justify::Full))),
                    2 => out.push(code(Code::Just(Justify::Center))),
                    3 => out.push(code(Code::Just(Justify::Right))),
                    _ => push_u!("Just:Decimal"),
                },
                7 => out.push(code(Code::Suppress(g.b(5)))),
                8 => out.push(code(Code::PageNumberPos(g.b(7).min(9)))),
                0x0B => {
                    let len = g.w(99) as i32;
                    let wid = g.w(101) as i32;
                    let ty = g.b(103);
                    let nl = (g.b(104) as usize).min(41);
                    let mut name = wpstr(g.range(105, nl));
                    if name.trim().is_empty() {
                        name = paper_type(ty).into();
                    }
                    push_u!(format!("Paper Sz/Typ:{} x {},{}", inch(wid), inch(len), name.trim()));
                }
                _ => push_u!("Unknown"),
            },
            0xD1 => match s {
                0 => push_u!(format!("Color:{},{},{}", g.b(7), g.b(8), g.b(9))),
                1 => {
                    let n = g.b(29) as usize;
                    let (name, rsize) = self.ctx.font(n).unwrap_or_else(|| (format!("Font #{n}"), 1200));
                    let size = if g.has(33) && g.w(32) != 0 { g.w(32) as u32 * 2 } else { rsize };
                    let size = if size == 0 { 1200 } else { size };
                    out.push(code(Code::Font(FontSpec { name, size_cpt: size })));
                }
                _ => push_u!("Unknown"),
            },
            0xD2 if s == 1 => {
                // text column definition: flags, then left/right edge pairs
                let v = g.b(101);
                let ncol = (v & 0x1F) as usize;
                let kind = if v & 0x80 != 0 {
                    ColKind::ParallelProtect
                } else if v & 0x40 != 0 {
                    ColKind::Parallel
                } else {
                    ColKind::Newspaper
                };
                let cols: Vec<(i32, i32)> = (0..ncol).map(|k| (g.w(102 + 4 * k) as i32, g.w(104 + 4 * k) as i32)).collect();
                out.push(code(Code::ColDef { kind, cols }));
            }
            0xD2 if s == 0 => {
                // [Math Def]: one byte per column A–X at +108 (type in the low
                // two bits; digits in bits 4–6 [?] — not verified against WP).
                let cols = (0..24)
                    .map(|k| {
                        let b = if g.has(108 + k) { g.b(108 + k) } else { 2 };
                        let d = (b >> 4) & 7;
                        MathCol { kind: b & 3, paren: false, digits: if d <= 4 { d } else { 2 }, formula: String::new() }
                    })
                    .collect();
                out.push(code(Code::MathDef(cols)));
            }
            0xD2 => {
                let name = match s {
                    0 => "Math Def".into(),
                    1 => {
                        let v = g.b(101);
                        let ncol = (v & 0x1F) as usize;
                        let kind = if v & 0x80 != 0 {
                            "Parallel w/Block Pro"
                        } else if v & 0x40 != 0 {
                            "Parallel"
                        } else {
                            "Newspaper"
                        };
                        let m: Vec<String> = (0..2 * ncol).map(|k| inch(g.w(102 + 2 * k) as i32)).collect();
                        format!("Col Def:{kind};{ncol};{}", m.join(","))
                    }
                    2 => {
                        out.push(code(Code::ParNumDef(par_num_def(g))));
                        return;
                    }
                    3 => "Ftn Opt".into(),
                    4 => "End Opt".into(),
                    5..=9 => {
                        out.push(code(Code::BoxOpt(Box::new(box_opts(s - 5, g)))));
                        return;
                    }
                    0x0B => {
                        let oldcols = (g.w(6) as usize).min(64);
                        let base = 4 + 24 + 5 * oldcols;
                        let flags = g.b(base);
                        let ncols = (g.w(base + 2) as usize).min(64);
                        let pos = ["Left", "Right", "Center", "Full", "Set"].get((flags & 7) as usize).copied().unwrap_or("?");
                        let w: Vec<String> = (0..ncols).map(|k| inch(g.w(base + 24 + 2 * k) as i32)).collect();
                        format!("Tbl Def:{pos};{ncols},{}", w.join(","))
                    }
                    0x0D => {
                        let l = g.b(20) as usize;
                        format!("Link:{}", wpstr(g.range(21, l)))
                    }
                    0x0E => "Link End".into(),
                    0x0F => "DDE Link".into(),
                    0x10 => "DDE Link End".into(),
                    0x11 => "Border Opt".into(),
                    _ => "Unknown".into(),
                };
                push_u!(name);
            }
            0xD3 => match s {
                4 => {
                    let v = g.w(6);
                    let roman = v & 0x8000 != 0 || (g.has(9) && g.b(9) == 1);
                    let num = (v & 0x7FFF) as u32;
                    if roman {
                        push_u!(format!("Pg Num:{}", roman_num(num)));
                    } else {
                        out.push(code(Code::PageNumber(num)));
                    }
                }
                0x0C..=0x10 => out.push(code(Code::BoxNewNum(crate::gfx::BoxKind::from_idx((s - 0x0C) as usize), ((g.w(6) >> 5) as u32).max(1)))),
                _ => push_u!(set_group_name(s, g)),
            },
            0xD4 => {
                // Formatter bookkeeping, regenerated by WP.  Sub 4 holds
                // non-editable overflow text: keep it.
                if s == 4 {
                    let t = self.nested(g.from(6), depth);
                    push_u!(format!("Non-edit Text:{}", brief(&t)));
                }
            }
            0xD5 => {
                let occ = match g.b(11) {
                    0 => Occurrence::Discontinue,
                    2 => Occurrence::Odd,
                    4 => Occurrence::Even,
                    _ => Occurrence::Every,
                };
                let els = if occ == Occurrence::Discontinue { Vec::new() } else { self.nested(g.from(22), depth) };
                out.push(code(Code::HeadFoot { which: s.min(3), occ, els }));
            }
            0xD6 => {
                let els = match s {
                    0 => {
                        let extra = g.b(7) as usize;
                        Some(self.nested(g.from(8 + 2 * (extra + 1) + 9), depth))
                    }
                    1 => Some(self.nested(g.from(11), depth)),
                    _ => None,
                };
                match els {
                    Some(mut els) => {
                        // Numbering is automatic: drop the [Note Num] marker.
                        if let Some(k) = els.iter().position(|e| matches!(e, El::Code(Code::Unknown { name, .. }) if name == "Note Num")) {
                            els.remove(k);
                        }
                        let kind = if s == 0 { NoteKind::Foot } else { NoteKind::End };
                        out.push(code(Code::Note { kind, els }));
                    }
                    None => push_u!("Unknown"),
                }
            }
            0xD7 => {
                // Mark Text codes we model
                let mark = match s {
                    0 | 1 if matches!(g.b(4) >> 4, 0 | 2) => {
                        let f = g.b(4);
                        let (list, n) = (f >> 4 == 2, (f & 15) + 1);
                        Some(if s == 0 { Mark::Begin { list, n } } else { Mark::End { list, n } })
                    }
                    2 => {
                        let f = g.b(4);
                        let mut styles = [4u8; 5];
                        for (k, st) in styles.iter_mut().enumerate() {
                            if g.has(5 + k) {
                                *st = g.b(5 + k).min(4);
                            }
                        }
                        match f >> 4 {
                            0 => Some(Mark::DefToc { levels: ((f & 15) + 1).min(5), wrap_last: false, styles }),
                            1 => Some(Mark::DefIndex { style: styles[0], concordance: if g.has(10) { wpstr(g.from(10)) } else { String::new() } }),
                            2 => Some(Mark::DefList { n: (f & 15) + 1, style: styles[0] }),
                            3 => Some(Mark::DefToA { section: (f & 15) + 1, dots: true, underline: false, blank: true }),
                            _ => None,
                        }
                    }
                    3 => {
                        let mut p = 0;
                        let d = g.from(4);
                        let heading = next_cstr(d, &mut p);
                        let sub = if p < d.len() { wpstr(&d[p..]) } else { String::new() };
                        Some(Mark::Index { heading, sub })
                    }
                    4 => {
                        let d = g.from(5);
                        let mut p = 0;
                        let short = next_cstr(d, &mut p);
                        let full = if p < d.len() { wpstr(&d[p..]) } else { String::new() };
                        let sec = g.b(4);
                        Some(Mark::ToA { section: if sec == 0x20 { 0 } else { sec.max(1) }, short, full })
                    }
                    7 => {
                        let d = g.from(5);
                        let mut p = 0;
                        let name = next_cstr(d, &mut p);
                        let text = if p < d.len() { wpstr(&d[p..]) } else { String::new() };
                        Some(Mark::Ref { name, kind: g.b(4).min(8), text: if text.trim().is_empty() { "?".into() } else { text.trim().to_string() } })
                    }
                    8 => Some(Mark::Target { name: wpstr(g.from(4)) }),
                    9 => Some(Mark::Subdoc(wpstr(g.from(7)))),
                    10 => Some(Mark::SubdocStart(wpstr(g.from(7)))),
                    11 => Some(Mark::SubdocEnd(wpstr(g.from(7)))),
                    _ => None,
                };
                if let Some(m) = mark {
                    out.push(code(Code::Mark(m)));
                    return;
                }
                let name = match s {
                    0 | 1 => {
                        let f = g.b(4);
                        let kind = match f >> 4 {
                            0 => "ToC",
                            2 => "List",
                            _ => "?",
                        };
                        format!("{}:{kind},{}", if s == 0 { "Mark" } else { "End Mark" }, (f & 15) + 1)
                    }
                    2 => {
                        let f = g.b(4);
                        match f >> 4 {
                            0 => format!("Def Mark:ToC,{}", (f & 15) + 1),
                            1 => "Def Mark:Index".into(),
                            2 => format!("Def Mark:List,{}", (f & 15) + 1),
                            3 => format!("Def Mark:ToA,{}", (f & 15) + 1),
                            _ => "Def Mark".into(),
                        }
                    }
                    3 => {
                        let mut p = 0;
                        let d = g.from(4);
                        let h = next_cstr(d, &mut p);
                        let sh = if p < d.len() { wpstr(&d[p..]) } else { String::new() };
                        if sh.is_empty() { format!("Index:{h}") } else { format!("Index:{h};{sh}") }
                    }
                    4 => {
                        let d = g.from(5);
                        let mut p = 0;
                        let short = next_cstr(d, &mut p);
                        let full = if p < d.len() { wpstr(&d[p..]) } else { String::new() };
                        format!("ToA:{};{short};{full}", g.b(4))
                    }
                    5 => "Endnote Placement".into(),
                    7 => {
                        let mut p = 0;
                        let tag = next_cstr(g.from(5), &mut p);
                        let ty = ["Pg", "Par", "Ftn", "End", "Fig", "Tbl", "Txt", "Usr", "Equ"].get(g.b(4) as usize).copied().unwrap_or("?");
                        format!("Ref({tag}):{ty}")
                    }
                    8 => format!("Target({})", wpstr(g.from(4))),
                    9 => format!("Subdoc:{}", wpstr(g.from(7))),
                    10 => format!("Subdoc Start:{}", wpstr(g.from(7))),
                    11 => format!("Subdoc End:{}", wpstr(g.from(7))),
                    _ => "Unknown".into(),
                };
                if s != 6 {
                    push_u!(name);
                }
            }
            0xD8 => match s {
                0 => out.push(code(Code::DateCode(wpstr(g.from(4))))),
                1 => {
                    let l = g.b(4);
                    out.push(code(Code::ParNum(if l & 0x80 != 0 { Some(((l & 0x7F) + 1).min(8)) } else { None })));
                }
                2 => {
                    // Overstrike: show the characters (base + combining marks).
                    let t = self.nested(g.from(6), depth);
                    let mut first = true;
                    for e in &t {
                        if let El::Ch(ch) = e {
                            out.push(El::Ch(if first { *ch } else { combining(*ch) }));
                            first = false;
                        }
                    }
                }
                3 => push_u!(format!("Insert Pg Num:{}", wpstr(g.from(4)))),
                _ => push_u!("Unknown"),
            },
            0xD9 => match s {
                0 => push_u!(format!("Ptr Cmnd:{}{}", if g.b(4) != 0 { "File=" } else { "" }, wpstr(g.from(5)))),
                1 => push_u!(format!("Cndl EOP:{}", g.b(4))),
                2 => {
                    let t = self.nested(g.from(7), depth);
                    out.push(code(Code::Comment(plain(&t))));
                }
                3 => push_u!(format!("Kern:{}", if g.b(5) != 0 { "On" } else { "Off" })),
                4 => out.push(code(Code::Outline(true))),
                5 => push_u!(format!("Leading Adj:{},{}", inch(g.sw(8) as i32), inch(g.sw(10) as i32))),
                _ => push_u!("Unknown"),
            },
            0xDA if s <= 6 => out.push(code(self.box_code(s, g, depth))),
            0xDB => match s {
                0 => out.push(code(Code::StyleOn(style_name(g.range(16, 21))))),
                1 => out.push(code(Code::StyleOff(style_name(g.range(22, 21))))),
                2 => out.push(code(Code::OpenStyle(style_name(g.range(7, 21))))),
                // 3: end of the cached style expansion.  We keep the
                // expanded codes in line (so the formatting shows) and drop
                // the marker, which WP regenerates.
                3 => {}
                _ => push_u!("Unknown"),
            },
            0xDC => match s {
                0 => push_u!("Cell"),
                1 => push_u!("Row"),
                2 => push_u!("Tbl Off"),
                _ => push_u!("Unknown"),
            },
            0xDD => match s {
                1 | 3 => push_u!("Row"),
                2 => push_u!("Tbl Off"),
                _ => push_u!("Unknown"),
            },
            0xDE => {
                let n = (s as usize).checked_sub(0x20).and_then(|k| MERGE.get(k)).map(|s| s.to_string()).unwrap_or_else(|| format!("cmd {s}"));
                if n.starts_with("cmd ") {
                    push_u!(format!("Mrg:{n}"));
                } else {
                    out.push(code(Code::Merge(n)));
                }
            }
            _ => push_u!("Unknown"),
        }
    }
}

impl Parser<'_> {
    /// DA 0–4 graphics box, DA 5/6 horizontal/vertical line (§6.11).
    fn box_code(&self, s: u8, g: &Grp, depth: u32) -> Code {
        use crate::gfx::*;
        if s >= 5 {
            let vertical = s == 6;
            let va = (g.b(6) >> 2) & 7;
            let ha = g.b(7) & 7;
            let (len, width) = if vertical { (g.w(10) as i32, g.w(8) as i32) } else { (g.w(8) as i32, g.w(10) as i32) };
            return Code::GLine(GLine {
                vertical,
                halign: if vertical { ha.min(3) } else { ha.min(4) },
                x: g.w(12) as i32,
                valign: if vertical { va.min(4) } else if va == 4 { 4 } else { 0 },
                y: g.w(14) as i32,
                length: len,
                width: width.max(1),
                gray: g.b(36).min(100),
            });
        }
        let mut b = GBox::new(BoxKind::from_idx(s as usize));
        let (pos, al) = (g.b(6), g.b(7));
        b.anchor = match pos & 3 {
            1 => Anchor::Page,
            2 => Anchor::Char,
            _ => Anchor::Para,
        };
        b.vpos = match b.anchor {
            Anchor::Page => match (pos >> 2) & 7 {
                0 => VPos::Full,
                1 => VPos::Top,
                2 => VPos::Center,
                3 => VPos::Bottom,
                _ => VPos::Set(g.sw(34) as i32),
            },
            Anchor::Para => VPos::Set((g.sw(34) as i32).max(0)),
            Anchor::Char => VPos::Baseline,
        };
        b.hpos = if (al >> 2) & 3 == 2 {
            HPos::Set(g.sw(32) as i32)
        } else {
            [HPos::Left, HPos::Right, HPos::Center, HPos::Full][(al & 3) as usize]
        };
        b.wrap = al & 0x80 == 0;
        let w = if g.w(107) != 0 { g.w(107) } else { g.w(8) } as i32;
        let h = if g.w(109) != 0 { g.w(109) } else { g.w(10) } as i32;
        b.file = wpstr(g.range(53, 21)).trim().to_string();
        let caplen = g.w(119) as usize;
        let mut cap = self.nested(g.range(121, caplen), depth);
        while matches!(cap.last(), Some(El::Code(Code::HRt))) {
            cap.pop();
        }
        b.caption = cap;
        let rest = g.from(121 + caplen);
        match g.b(52) {
            0x80 => {
                if let Some(img) = self.ctx.images.get(g.w(113) as usize) {
                    let name = b.file.clone();
                    b.set_image(&name, img.clone());
                }
            }
            0x02 | 0x81..=0x9F => {
                b.contents = Contents::Image;
            }
            0x08 => {
                b.contents = Contents::Equation;
                if rest.first() == Some(&0xDF) {
                    let n = u16_at(rest, 10) as usize;
                    b.equation = wpstr(rest.get(12..(12 + n).min(rest.len())).unwrap_or(&[])).replace(['\n', '\r'], " ");
                }
            }
            0x10 | 0x11 => {
                b.contents = Contents::Text;
                let mut t = self.nested(rest, depth);
                while matches!(t.last(), Some(El::Code(Code::HRt))) {
                    t.pop();
                }
                b.text = t;
            }
            _ => b.contents = Contents::Empty,
        }
        b.width = w.max(120);
        b.height = h.max(120);
        b.auto = if al & 0x10 != 0 { 0 } else { 1 };
        let rot = g.w(42);
        b.img.rot = (rot & 0xFFF) as i32 % 360;
        b.img.invert = rot & 0x4000 != 0;
        b.img.mirror = rot & 0x8000 != 0;
        if g.w(44) != 0 {
            b.img.sx = g.w(44) as i32;
        }
        if g.w(46) != 0 {
            b.img.sy = g.w(46) as i32;
        }
        Code::GBox(Box::new(b))
    }
}

/// Packet 8: count, sizes, then the WPG images back to back.
fn read_images(p: &[u8]) -> Vec<Vec<u8>> {
    let n = (u16_at(p, 0) as usize).min(1000);
    let mut out = vec![];
    let mut pos = 2 + 4 * n;
    for k in 0..n {
        let sz = u32_at(p, 2 + 4 * k) as usize;
        let Some(img) = p.get(pos..pos + sz) else { break };
        let mut v = img.to_vec();
        if !v.starts_with(b"\xFFWPC") {
            let mut h = vec![0xFF, 0x57, 0x50, 0x43, 0x10, 0, 0, 0, 0x01, 0x16, 0x01, 0, 0, 0, 0, 0];
            h.extend(v);
            v = h;
        }
        out.push(v);
        pos += sz;
    }
    out
}

/// D2 5–9 box options: old values at +4, new values at +66 (§6.3).
fn box_opts(kind: u8, g: &Grp) -> crate::gfx::BoxOpts {
    let k = crate::gfx::BoxKind::from_idx(kind as usize);
    let mut o = crate::gfx::BoxOpts::default_for(k);
    let base = 66;
    let bw = g.w(base + 2);
    for (i, sh) in [12u16, 8, 4, 0].iter().enumerate() {
        o.border[i] = ((bw >> sh) & 15).min(6) as u8;
    }
    for i in 0..4 {
        o.outside[i] = g.w(base + 6 + 2 * i) as i32;
        o.inside[i] = g.w(base + 14 + 2 * i) as i32;
    }
    o.gray = g.b(base + 1).min(100);
    o.min_offset = g.w(base + 4) as i32;
    // caption number style: text with C3/C4 attribute codes
    let s = g.range(base + 22, 20);
    let mut style = String::new();
    let mut i = 0;
    while i < s.len() && s[i] != 0 {
        match s[i] {
            0xC3 | 0xC4 if i + 2 < s.len() => {
                if let Some(a) = ATTRS.get(s[i + 1] as usize) {
                    let m = a.mnemonic();
                    style.push_str(&format!("[{}]", if s[i] == 0xC3 { m.to_string() } else { m.to_lowercase() }));
                }
                i += 3;
            }
            c @ 0x20..=0x7E => {
                style.push(c as char);
                i += 1;
            }
            _ => i += 1,
        }
    }
    if !style.is_empty() {
        o.cap_style = style;
    }
    o
}

fn tab_set(g: &Grp) -> TabSet {
    let rel = if g.has(207) { g.w(206) } else { 0xFFFF };
    let relative = rel != 0xFFFF;
    let mut stops = Vec::new();
    for k in 0..40 {
        let p = g.w(104 + 2 * k);
        if p == 0xFFFF || !g.has(105 + 2 * k) {
            break;
        }
        let t = (g.b(184 + k / 2) >> if k % 2 == 0 { 4 } else { 0 }) & 0x0F;
        let kind = match t & 3 {
            0 => TabKind::Left,
            1 => TabKind::Center,
            2 => TabKind::Right,
            _ => TabKind::Decimal,
        };
        let pos = if relative { p as i32 - rel as i32 } else { p as i32 };
        stops.push(TabStop { pos, kind, dot_leader: t & 4 != 0 });
    }
    TabSet { stops, relative }
}

fn set_group_name(s: u8, g: &Grp) -> String {
    let charw = |w: u16| if w == 0 { String::new() } else { wp_to_string((w >> 8) as u8, w & 0xFF) };
    match s {
        0 => format!("Decml/Algn Char:{},{}", charw(g.w(8)), charw(g.w(10))),
        1 => {
            let v = g.b(5);
            let p: Vec<&str> = [(1, "Spaces"), (2, "Tabs")].iter().filter(|(b, _)| v & b != 0).map(|(_, n)| *n).collect();
            format!("Undrln:{}", p.join(","))
        }
        2 => format!("New Ftn Num:{}", g.w(6)),
        3 => format!("New End Num:{}", g.w(6)),
        5 => format!("Ln Num:{}", if g.b(9) & 0x80 != 0 { "On" } else { "Off" }),
        6 => {
            let f = g.b(4);
            let v = g.sw(7) as i32;
            if f & 1 != 0 {
                format!("AdvTo{}:{}", if f & 2 != 0 { "Pos" } else { "Ln" }, inch(v))
            } else if f & 2 != 0 {
                format!("Adv{}:{}", if v >= 0 { "Rgt" } else { "Lft" }, inch(v.abs()))
            } else {
                format!("Adv{}:{}", if v >= 0 { "Dn" } else { "Up" }, inch(v.abs()))
            }
        }
        7 => format!("Force:{}", if g.b(6) != 0 { "Odd" } else { "Even" }),
        8 => format!("Bline:{}", if g.b(5) != 0 { "On" } else { "Off" }),
        0x0A => format!("Wrd/Ltr Spacing:{}%,{}%", g.w(8), g.w(10)),
        0x0B => format!("Just Lim:{},{}", g.w(8), g.w(10)),
        0x0C..=0x10 => {
            let v = g.w(6);
            let k = ["Fig", "Tbl", "Txt", "Usr", "Equ"][(s - 0x0C) as usize];
            if v & 31 > 1 { format!("New {k} Num:{}.{}", v >> 5, v & 31) } else { format!("New {k} Num:{}", v >> 5) }
        }
        0x11 => {
            let a = g.b(6);
            let b = g.b(7);
            let l: String = [a, b].iter().filter(|c| c.is_ascii_alphabetic()).map(|&c| c as char).collect();
            format!("Lang:{l}")
        }
        0x12 => format!("Pg Num Style:{}", wpstr(g.range(34, 30))),
        _ => "Unknown".into(),
    }
}

/// D2/2 paragraph number definition: 8 formats of 3 bytes at +44
/// (punctuation, style, punctuation; first byte 0 = bullet in bytes 2–3),
/// starting numbers at +68, 5.1 flags at +104/+105, style name at +106.
fn par_num_def(g: &Grp) -> ParNumDef {
    let mut d = crate::styles::default_def();
    if !g.has(44 + 24) {
        return d;
    }
    let punct = |b: u8| if (0x20..0x7F).contains(&b) { (b as char).to_string() } else { String::new() };
    let mut levels: Vec<NumLevel> = vec![];
    for k in 0..8 {
        let (a, s, z) = (g.b(44 + 3 * k), g.b(45 + 3 * k), g.b(46 + 3 * k));
        let start = if g.has(69 + 2 * k) { (g.w(68 + 2 * k) as u32).max(1) } else { 1 };
        let attach = g.has(104) && k > 0 && g.b(104) & (1 << k) != 0;
        let l = if a == 0 && s != 0 {
            let c = wp_to_string(z, s as u16);
            NumLevel { fmt: NumFmt::Bullet, text: if c.trim().is_empty() { "\u{2022}".into() } else { c }, start }
        } else {
            let sc = match s {
                b'1' | b'a' | b'A' | b'i' | b'I' => s as char,
                _ => crate::styles::level_display(&d.levels[k], k + 1).chars().find(|c| "1aAiI".contains(*c)).unwrap_or('1'),
            };
            crate::styles::level_from_display(&format!("{}{sc}{}", punct(a), punct(z)), k + 1, attach, levels.last(), start)
        };
        levels.push(l);
    }
    d.levels = levels;
    if g.has(105) {
        d.enter_inserts = g.b(105) & 1 == 0;
    }
    if g.has(107) {
        d.outline_style = wpstr(g.range(106, 18)).trim().to_string();
    }
    d
}

/// Style definitions from prefix packets 0x100–0x1FF (documents and .STY
/// style libraries).
fn read_style_packets(bytes: &[u8], pk: &HashMap<u16, (usize, usize)>, ctx: &Ctx) -> Vec<StyleDef> {
    let parser = Parser { ctx, styles: vec![] };
    let mut keys: Vec<u16> = pk.keys().copied().filter(|k| (0x100..0x200).contains(k)).collect();
    keys.sort();
    let mut out: Vec<StyleDef> = vec![];
    for k in keys {
        let (o, l) = pk[&k];
        let Some(p) = bytes.get(o..o + l) else { continue };
        if p.len() < 88 {
            continue;
        }
        let name = style_name(&p[..21]).trim().to_string();
        if name.is_empty() || out.iter().any(|s| s.name == name) {
            continue;
        }
        let ty = p[21];
        let beg = u16_at(p, 78) as usize;
        let end = u16_at(p, 82) as usize;
        let xtr = u16_at(p, 86) as usize;
        let part = |a: usize, n: usize| p.get(a.min(p.len())..(a + n).min(p.len())).unwrap_or(&[]);
        let on = parser.decode(part(88, beg), 1);
        let mut off = parser.decode(part(88 + beg, end), 1);
        off.extend(parser.decode(part(88 + beg + end, xtr), 1));
        let kind = if p[0] == 0x7F {
            StyleKind::Outline
        } else if ty & 1 != 0 {
            StyleKind::Paired
        } else {
            StyleKind::Open
        };
        let enter = match ty >> 6 {
            2 => EnterAction::Off,
            3 => EnterAction::OffOn,
            _ => EnterAction::HRt,
        };
        out.push(StyleDef { name, kind, description: wpstr(&p[22..77]).trim().to_string(), on, off, enter });
    }
    out
}

/// Read a WP 5.x style library (`.STY`; any WP 5.x document works).
pub fn read_styles(bytes: &[u8]) -> Result<Vec<StyleDef>, String> {
    if bytes.len() < 16 || &bytes[0..4] != b"\xFFWPC" || bytes[8] != 1 || bytes[10] != 0 {
        return Err("Not a WP 5.x style library".into());
    }
    let docoff = (u32_at(bytes, 4) as usize).clamp(16, bytes.len());
    let pk = read_prefix(bytes, docoff);
    let ctx = read_fonts(bytes, &pk);
    Ok(read_style_packets(bytes, &pk, &ctx))
}

fn style_name(b: &[u8]) -> String {
    // Outline styles: 7F 'a' level, then the family name.
    if b.len() >= 3 && b[0] == 0x7F && b[1] == b'a' && b[2].is_ascii_digit() {
        return format!("{} {}", wpstr(&b[3..]), b[2] as char);
    }
    wpstr(b)
}

fn paper_type(t: u8) -> &'static str {
    match t {
        0 => "Standard",
        1 => "Bond",
        2 => "Letterhead",
        3 => "Labels",
        4 => "Envelope",
        5 => "Transparency",
        6 => "Cardstock",
        _ => "Other",
    }
}

fn roman_num(mut n: u32) -> String {
    let mut s = String::new();
    for (v, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// Spacing accent → combining mark (for overstrike).
fn combining(c: char) -> char {
    match c {
        '´' | '\'' => '\u{301}',
        '`' => '\u{300}',
        '^' | 'ˆ' => '\u{302}',
        '~' | '˜' => '\u{303}',
        '¯' => '\u{304}',
        '˘' => '\u{306}',
        '˙' => '\u{307}',
        '¨' | '"' => '\u{308}',
        '˚' => '\u{30A}',
        '˝' => '\u{30B}',
        'ˇ' => '\u{30C}',
        '¸' => '\u{327}',
        '˛' => '\u{328}',
        other => other,
    }
}

// ------------------------------------------------------------------ entry

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if bytes.len() < 16 || &bytes[0..4] != b"\xFFWPC" {
        return Err("Not a WordPerfect 5.x document".into());
    }
    let (product, ftype, major) = (bytes[8], bytes[9], bytes[10]);
    if major != 0 {
        return Err(if major >= 2 {
            "WordPerfect 6.x and later documents are not supported".into()
        } else {
            "Not a WordPerfect 5.x document".into()
        });
    }
    if product != 1 || ftype != 10 {
        return Err(format!("Not a WordPerfect 5.x document (file type {ftype})"));
    }
    if u16_at(bytes, 12) != 0 {
        return Err("Document is password protected".into());
    }
    let docoff = (u32_at(bytes, 4) as usize).clamp(16, bytes.len());

    let pk = read_prefix(bytes, docoff);
    let mut ctx = read_fonts(bytes, &pk);
    if let Some(&(o, l)) = pk.get(&8) {
        ctx.images = read_images(&bytes[o..o + l]);
    }
    let mut doc = Document::new();
    doc.styles = read_style_packets(bytes, &pk, &ctx);
    let parser = Parser { ctx: &ctx, styles: doc.styles.iter().map(|s| s.name.clone()).collect() };

    if let Some(&(o, l)) = pk.get(&1) {
        doc.summary = read_summary(&bytes[o..o + l]);
    }
    if let Some((name, size)) = ctx.font(0) {
        doc.init.font = FontSpec { name, size_cpt: if size == 0 { 1200 } else { size } };
    }

    // Document initial codes.
    if let Some(&(o, l)) = pk.get(&3) {
        let els = parser.decode(&bytes[o..o + l], 0);
        for e in els {
            let El::Code(c) = e else { continue };
            apply_init(&mut doc.init, &c);
            doc.initial_codes.push(El::Code(c));
        }
    }

    doc.els = parser.decode(&bytes[docoff..], 0);

    // A paper size set before any text is the document's page size.
    for e in &doc.els {
        match e {
            El::Ch(_) => break,
            El::Code(c) => {
                if let Some((w, l)) = paper_of(c) {
                    doc.init.page_width = w;
                    doc.init.page_height = l;
                    break;
                }
            }
        }
    }

    // Close attributes left open (WP lets them run to the end).
    let mut st = AttrSet::default();
    for e in &doc.els {
        if let Some((a, on)) = e.code().and_then(|c| c.attr()) {
            st.set(a, on);
        }
    }
    for a in st.iter() {
        doc.els.push(El::Code(Code::AttrOff(a)));
    }
    doc.normalize_attrs();
    Ok(doc)
}

fn apply_init(init: &mut InitialFormat, c: &Code) {
    match c {
        Code::LRMargin { left, right } => {
            init.left = *left;
            init.right = *right;
        }
        Code::TBMargin { top, bottom } => {
            init.top = *top;
            init.bottom = *bottom;
        }
        Code::Font(f) => init.font = f.clone(),
        Code::TabSet(t) => init.tabs = t.clone(),
        Code::Just(j) => init.justify = *j,
        Code::LineSpacing(s) => init.line_spacing = *s,
        c @ (Code::Unknown { .. } | Code::Fmt(_)) => {
            if let Some((w, l)) = paper_of(c) {
                init.page_width = w;
                init.page_height = l;
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Sample directory (WSL path, or the same drive seen from Windows).
    fn dir() -> &'static Path {
        ["/mnt/i/Claude Space/wp51", "I:\\Claude Space\\wp51"]
            .into_iter()
            .map(Path::new)
            .find(|p| p.is_dir())
            .unwrap_or(Path::new("/nonexistent"))
    }

    fn load(name: &str) -> Option<Document> {
        let p = dir().join(name);
        let b = std::fs::read(&p).ok()?;
        Some(read(&b).unwrap_or_else(|e| panic!("{name}: {e}")))
    }

    fn text(d: &Document) -> String {
        d.plain_text(0, d.els.len())
    }

    fn unknown_names(d: &Document) -> Vec<String> {
        d.els
            .iter()
            .filter_map(|e| match e {
                El::Code(Code::Unknown { name, .. }) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every attribute on has a matching off, never doubled.
    fn assert_balanced(d: &Document) {
        let mut st = AttrSet::default();
        for e in &d.els {
            if let Some((a, on)) = e.code().and_then(|c| c.attr()) {
                assert_ne!(st.has(a), on, "unbalanced {a:?}");
                st.set(a, on);
            }
        }
        assert!(st.is_empty(), "attributes left open: {st:?}");
    }

    /// Names of things we model must never come out as Unknown.
    fn assert_no_modeled_unknowns(d: &Document) {
        const MODELED: [&str; 25] = [
            "Header A", "Header B", "Footer A", "Footer B", "Footnote", "Endnote",
            "HRt", "HPg", "Tab", "Indent", "Center", "Flsh Rgt", "Mar Rel", "Bold", "Und", "Font", "L/R Mar",
            "T/B Mar", "Ln Spacing", "Just", "Tab Set", "Comment", "Col Def", "Col On", "Col Off",
        ];
        for n in unknown_names(d) {
            let head = n.split(':').next().unwrap_or("");
            assert!(!MODELED.contains(&head), "modeled code imported as Unknown: {n}");
        }
    }

    #[test]
    fn wp5_keybrds() {
        let Some(d) = load("KEYBRDS.WP") else { return };
        let t = text(&d);
        assert!(t.contains("Tastatur-Layout: Deutsch"), "{t}");
        assert!(t.contains("betätigen"));
        assert!(t.contains("eingefügt"));
        assert!(t.contains('┌') && t.contains('ß') && t.contains('°') && t.contains('§'));
        assert!(unknown_names(&d).is_empty(), "{:?}", unknown_names(&d));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        let tabs = d.els.iter().find_map(|e| match e {
            El::Code(Code::TabSet(t)) => Some(t.clone()),
            _ => None,
        });
        let tabs = tabs.expect("tab set");
        assert!(tabs.relative);
        assert_eq!(tabs.stops.iter().map(|s| s.pos).collect::<Vec<_>>(), vec![-1200, -600, 288, 648]);
        assert!(d.els.contains(&El::Code(Code::LRMargin { left: 348, right: 300 })));
        assert_balanced(&d);
    }

    #[test]
    fn wp5_kern() {
        let Some(d) = load("KERN.TST") else { return };
        let t = text(&d);
        assert!(t.starts_with("KERN.TST\n"), "{t}");
        // soft returns became the spaces they replaced
        assert!(t.contains("working with by choosing that printer"));
        assert!(t.contains("-00:  MN"));
        let fonts: Vec<&FontSpec> = d.els.iter().filter_map(|e| match e {
            El::Code(Code::Font(f)) => Some(f),
            _ => None,
        }).collect();
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].name, "Courier 10pt 10 Pitch");
        assert_eq!(fonts[0].size_cpt, 1000);
        assert_eq!(d.init.font.name, "Courier 10pt 10 Pitch");
        use crate::fmt::{Adv, Fmt};
        assert!(d.els.contains(&El::Code(Code::Fmt(Fmt::Kern(true)))));
        assert!(d.els.contains(&El::Code(Code::Fmt(Fmt::Adv(Adv::Left, 4)))), "{:?}", unknown_names(&d));
        assert_no_modeled_unknowns(&d);
        assert_balanced(&d);
    }

    #[test]
    fn wp5_printer() {
        let Some(d) = load("PRINTER.TST") else { return };
        let t = text(&d);
        for w in ["WordPerfect 5.1 Printer Test", "Japanese ざ", "Copyright ©", "Greek Δ", "pull-down menus", "Integrating Text and Graphics", "Right Aligned"] {
            assert!(t.contains(w), "missing {w:?} in {t}");
        }
        assert_eq!(d.init.font.name, "Courier 10 Pitch");
        assert_eq!(d.init.font.size_cpt, 1200);
        let u = unknown_names(&d);
        let foot = d.els.iter().find_map(|e| match e {
            El::Code(Code::HeadFoot { which: 2, occ: Occurrence::Every, els }) => Some(els.clone()),
            _ => None,
        });
        let foot = foot.expect("footer A");
        let ft = Document { els: foot.clone(), ..Default::default() };
        assert!(text(&ft).contains("This document printed in WordPerfect 5.1"), "{foot:?}");
        assert!(foot.iter().any(|e| matches!(e, El::Code(Code::DateCode(f)) if f == "3 1, 4  8:9 0")));
        let note = d.els.iter().find_map(|e| match e {
            El::Code(Code::Note { kind: NoteKind::Foot, els }) => Some(els.clone()),
            _ => None,
        });
        let note = note.expect("footnote");
        assert_eq!(note.first(), Some(&El::Code(Code::AttrOn(Attr::Fine))));
        let nt = Document { els: note, ..Default::default() };
        assert!(text(&nt).starts_with("Although only double and single lines"), "{}", text(&nt));
        assert!(u.iter().any(|n| n == "Tbl Def:Center;4,1.693\",1.844\",1.245\",1.417\""), "{u:?}");
        assert_eq!(u.iter().filter(|n| *n == "Cell").count(), 16);
        assert_eq!(u.iter().filter(|n| *n == "Row").count(), 4);
        let fig = d.els.iter().find_map(|e| match e {
            El::Code(Code::GBox(b)) if b.kind == crate::gfx::BoxKind::Figure => Some(b.clone()),
            _ => None,
        });
        let fig = fig.expect("figure box");
        assert_eq!(fig.file, "MOUSE.WPG");
        assert!(fig.data.as_ref().is_some_and(|d| crate::raster::decode(d).is_some()), "embedded WPG renders");
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::GBox(b)) if b.kind == crate::gfx::BoxKind::Equation && !b.equation.is_empty())));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::BoxOpt(o)) if o.kind == crate::gfx::BoxKind::Figure && o.cap_style == "[BOLD]Figure 1[bold]")));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::ColDef { kind: ColKind::Parallel | ColKind::ParallelProtect, cols }) if cols.len() == 2)));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::GLine(l)) if !l.vertical && l.length == 7800 && l.width == 25)));
        assert!(d.els.contains(&El::Code(Code::SoftHyphen)));
        assert!(d.els.contains(&El::Code(Code::Hyphenation(true))));
        assert!(d.els.contains(&El::Code(Code::TBMargin { top: 600, bottom: 600 })));
        assert!(d.els.contains(&El::Code(Code::LineSpacing(200))));
        assert!(d.els.contains(&El::Code(Code::Center)));
        // table cells readable inline
        assert!(t.contains("Japanese ざ\tCopyright ©\tHand ☜\tGreek Δ"), "{t}");
        assert_no_modeled_unknowns(&d);
        assert_balanced(&d);
    }

    #[test]
    fn wp5_other_samples() {
        for f in ["JOB", "STANDARD.CRS", "WP.LRS", "WP{WP}DE.SUP", "WP{WP}FR.SUP", "WP{WP}SP.SUP", "WP{WP}US.SUP",
            "INSTRUCT/CHANGE.SPL", "STYLE/LIBRARY.STY", "STYLE/SUB", "STYLE/T", "WPM/BX", "WPM/WP}WP{PJ.1"]
        {
            let Some(d) = load(f) else { continue };
            assert_no_modeled_unknowns(&d);
            assert_balanced(&d);
        }
        if let Some(d) = load("WP.LRS") {
            assert!(text(&d).contains("Deutsch - BRD"));
        }
        if let Some(d) = load("STYLE/LIBRARY.STY") {
            assert!(d.els.is_empty());
        }
    }

    #[test]
    fn wp5_rejects() {
        assert!(read(b"hello world").is_err());
        assert!(read(b"\xFFWPC").is_err());
        let mut h = vec![0xFF, b'W', b'P', b'C', 16, 0, 0, 0, 1, 10, 0, 1, 0x34, 0x12, 0, 0];
        assert_eq!(read(&h).unwrap_err(), "Document is password protected");
        h[12] = 0;
        h[13] = 0;
        let d = read(&h).unwrap();
        assert!(d.els.is_empty());
        h[10] = 2;
        assert!(read(&h).is_err());
    }

    #[test]
    fn wp5_synthetic() {
        let mut b = vec![0xFF, b'W', b'P', b'C', 16, 0, 0, 0, 1, 10, 0, 1, 0, 0, 0, 0];
        b.extend_from_slice(b"Gr\xC0\x47\x01\xC0n\r\xC3\x0C\xC3x\x0Ay");
        let d = read(&b).unwrap();
        assert_eq!(text(&d), "Grün x\ny");
        assert_balanced(&d);
    }

    #[test]
    fn wp5_style_library() {
        let Ok(b) = std::fs::read(dir().join("STYLE/LIBRARY.STY")) else { return };
        let st = read_styles(&b).unwrap();
        assert!(st.len() >= 20, "{}", st.len());
        let bib = st.iter().find(|s| s.name == "Bibliogrphy").expect("Bibliogrphy");
        assert_eq!(bib.kind, StyleKind::Paired);
        assert_eq!(bib.on, vec![El::Code(Code::Indent), El::Code(Code::MarRel)]);
        assert_eq!(bib.off, vec![El::Code(Code::HRt), El::Code(Code::HRt)]);
        let doc2 = st.iter().find(|s| s.name == "Document 2").expect("outline style");
        assert_eq!(doc2.kind, StyleKind::Outline);
        assert!(doc2.on.iter().any(|e| matches!(e, El::Code(Code::ParNum(Some(2))))), "{:?}", doc2.on);
    }

    #[test]
    fn wp5_style_codes_in_body() {
        // Style On with a cached expansion up to DB/03 → only the style code
        let mut body: Vec<u8> = vec![];
        let grp = |sub: u8, payload: &[u8]| {
            let ln = (payload.len() + 4) as u16;
            let mut g = vec![0xDB, sub, ln as u8, (ln >> 8) as u8];
            g.extend_from_slice(payload);
            g.extend_from_slice(&[ln as u8, (ln >> 8) as u8, sub, 0xDB]);
            g
        };
        let mut on = vec![0u8; 12];
        on.extend_from_slice(b"Head\0");
        on.resize(12 + 21, 0);
        body.extend(grp(0, &on));
        body.extend_from_slice(b"\xC3\x0C\xC3");
        body.extend(grp(3, &[0]));
        body.extend_from_slice(b"Hi");
        let p = Parser { ctx: &Ctx::default(), styles: vec!["Head".into()] };
        let els = p.decode(&body, 0);
        assert_eq!(els[0], El::Code(Code::StyleOn("Head".into())));
        assert_eq!(els[1..], [El::Ch('H'), El::Ch('i')]);
        let p = Parser { ctx: &Ctx::default(), styles: vec![] };
        assert_eq!(p.decode(&body, 0).len(), 4);
    }

    /// Truncated and corrupted files must never panic.
    #[test]
    fn wp5_fuzz() {
        let mut seed = 0x1234_5678u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for f in ["KEYBRDS.WP", "KERN.TST", "PRINTER.TST"] {
            let Ok(orig) = std::fs::read(dir().join(f)) else { continue };
            for cut in (0..orig.len()).step_by(97) {
                let _ = read(&orig[..cut]);
            }
            for _ in 0..300 {
                let mut b = orig.clone();
                for _ in 0..1 + rnd() % 20 {
                    let k = rnd() as usize % b.len();
                    if k >= 12 {
                        b[k] = rnd() as u8;
                    }
                }
                let _ = read(&b);
            }
        }
    }
}
