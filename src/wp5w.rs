//! WordPerfect 5.1 document export: the inverse of wp5.rs, written from
//! docs/formats/wp5_document.md and the bytes of real 5.1 files.
//!
//! The file is a 16-byte header, a prefix (index blocks and packets: the
//! fonts, the printer, the summary, the document initial codes, embedded
//! WPG images) and the body: text bytes, single-byte functions,
//! fixed-length functions (0xC0–0xCF) and variable-length functions
//! (0xD0–0xFF).  No formatter bookkeeping is written (soft returns, the D4
//! group, end of indent …): 5.1 makes it again when it formats the text.
//! Where a function stores a position 5.1 has calculated (tab codes, table
//! rows), a plausible one is written.
//!
//! What a 5.1 file cannot hold is left out and counted (see `report`):
//!   written as something else — styles and paragraph numbers (as the codes
//!     and text they stand for), tracked changes (redline / strikeout),
//!     characters outside the WP character sets (base letter or `?`);
//!   left out, text kept — hyperlinks, bookmarks, spreadsheet links, table
//!     formulas (the result stays);
//!   left out — border options, math and paragraph number definitions, the
//!     numbering method and caption position of box options, images that
//!     are not WPG (the box stays, with the file name), codes of other
//!     formats kept as `Code::Unknown`.

use crate::doc::*;
use crate::fmt::{Adv, Fmt, Spacing};
use crate::wp5::{ATTRS, FIXED, MERGE};
use crate::wpchars::{str_to_wp, unicode_to_wp};
use std::collections::BTreeMap;

/// Maximum nesting of text fields, as in the reader.
const MAX_DEPTH: u32 = 8;

/// Width of US Letter, the paper of a 5.1 document without a paper code.
const LETTER_W: i32 = 10200;

// ------------------------------------------------------------------ bytes

fn w16(o: &mut Vec<u8>, v: i32) {
    o.extend_from_slice(&(v.clamp(-32768, 65535) as u16).to_le_bytes());
}

/// Payload of a variable-length function.  Like `wp5::Grp`, offsets are
/// those of the format description: measured from the code byte, so the
/// payload starts at 4.
struct Pay(Vec<u8>);

impl Pay {
    /// A zeroed payload that ends before offset `end`.
    fn new(end: usize) -> Pay {
        Pay(vec![0; end - 4])
    }
    fn b(mut self, k: usize, v: u8) -> Pay {
        self.0[k - 4] = v;
        self
    }
    fn w(mut self, k: usize, v: i32) -> Pay {
        self.0[k - 4..k - 2].copy_from_slice(&(v.clamp(-32768, 65535) as u16).to_le_bytes());
        self
    }
    /// Bytes at `k`, cut to `max`.
    fn s(mut self, k: usize, v: &[u8], max: usize) -> Pay {
        let n = v.len().min(max);
        self.0[k - 4..k - 4 + n].copy_from_slice(&v[..n]);
        self
    }
    /// Bytes behind the fixed part.
    fn tail(mut self, v: &[u8]) -> Pay {
        self.0.extend_from_slice(v);
        self
    }
    /// The whole function: `code sub len payload len sub code`.
    fn put(self, o: &mut Vec<u8>, c: u8, sub: u8) {
        let Ok(ln) = u16::try_from(self.0.len() + 4) else { return };
        o.extend_from_slice(&[c, sub]);
        o.extend_from_slice(&ln.to_le_bytes());
        o.extend_from_slice(&self.0);
        o.extend_from_slice(&ln.to_le_bytes());
        o.extend_from_slice(&[sub, c]);
    }
}

/// Is `d` one whole, well framed 5.x function (or a single-byte code)?
/// Only then can a `Code::Unknown` be put back as it is.
fn framed(d: &[u8]) -> bool {
    match d {
        [] => false,
        [c] => matches!(c, 0x01..=0x1F | 0x80..=0xBF),
        [c @ 0xC0..=0xCF, ..] => d.len() == FIXED[(*c - 0xC0) as usize] && d.last() == Some(c),
        [c @ 0xD0..=0xFF, sub, l0, l1, ..] => {
            let ln = u16::from_le_bytes([*l0, *l1]) as usize;
            let n = d.len();
            ln >= 4 && n == ln + 4 && d[n - 1] == *c && d[n - 2] == *sub && d[n - 4..n - 2] == [*l0, *l1]
        }
        _ => false,
    }
}

// ------------------------------------------------------------------ characters

/// One character as body bytes; false when the WP character sets lack it.
fn put_char(o: &mut Vec<u8>, c: char) -> bool {
    match c {
        // the typed hyphen is the function 0xA9; ASCII '-' is the hyphen
        // character (Home,-), our `Code::HardHyphen`
        '-' => o.push(0xA9),
        ' '..='~' => o.push(c as u8),
        _ => match unicode_to_wp(c) {
            Some((set, idx)) if idx < 256 => o.extend_from_slice(&[0xC0, idx as u8, set, 0xC0]),
            _ => return false,
        },
    }
    true
}

/// What to write for a character the WP character sets lack.
fn fallback(c: char) -> String {
    match c {
        '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => " ".into(),
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}' => String::new(),
        '\u{2010}' | '\u{2012}' | '\u{2015}' | '\u{2212}' => "-".into(),
        '€' => "EUR".into(),
        _ => {
            // an accented letter: its base letter
            let f = crate::sort::fold(&c.to_string());
            if !f.is_empty() && f.chars().all(|x| x.is_ascii_alphanumeric()) {
                if c.is_uppercase() { f.to_uppercase() } else { f }
            } else {
                "?".into()
            }
        }
    }
}

/// A WP string (ASCII and `C0` codes, no NUL) for names and other strings
/// inside functions.
fn wps(s: &str) -> Vec<u8> {
    let mut o = Vec::new();
    for c in s.chars() {
        match c {
            '\n' => o.push(0x0A),
            '\u{2}' => o.push(0x02),
            '\u{a0}' => o.push(0xA0),
            '-' => o.push(b'-'),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {}
            c => {
                if !put_char(&mut o, c) {
                    o.extend(fallback(c).bytes());
                }
            }
        }
    }
    o
}

/// Spacing accent of a combining mark: what 5.1 overstrikes a letter with
/// (the reverse of the reader's `combining`).
fn spacing(c: char) -> Option<char> {
    Some(match c {
        '\u{301}' => '´',
        '\u{300}' => '`',
        '\u{302}' => '^',
        '\u{303}' => '~',
        '\u{304}' => '¯',
        '\u{306}' => '˘',
        '\u{307}' => '˙',
        '\u{308}' => '¨',
        '\u{30A}' => '˚',
        '\u{30B}' => '˝',
        '\u{30C}' => 'ˇ',
        '\u{327}' => '¸',
        '\u{328}' => '˛',
        _ => return None,
    })
}

fn is_mark(e: Option<&El>) -> Option<char> {
    match e {
        Some(El::Ch(c)) if ('\u{300}'..='\u{36F}').contains(c) => Some(*c),
        _ => None,
    }
}

fn attr_ix(a: Attr) -> u8 {
    ATTRS.iter().position(|x| *x == a).unwrap_or(12) as u8
}

fn just_code(j: Justify) -> u8 {
    match j {
        Justify::Left => 0,
        Justify::Full => 1,
        Justify::Center => 2,
        Justify::Right => 3,
    }
}

// ------------------------------------------------------------------ format state

/// The format in effect while writing: for the "old" values and the
/// calculated positions the functions hold.
#[derive(Clone)]
struct St {
    page_w: i32,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    spacing: u32,
    tabs: TabSet,
    /// Justification in our stream, and the one last written to the body:
    /// in a table it is part of the cell codes.
    just: Justify,
    wp_just: Justify,
    /// Tabs so far on the line.
    line_tabs: usize,
    in_table: bool,
}

impl St {
    /// What 5.1 starts a document with.
    fn wp_default() -> St {
        St {
            page_w: LETTER_W,
            left: 1200,
            right: 1200,
            top: 1200,
            bottom: 1200,
            spacing: 100,
            tabs: TabSet::default_tabs(),
            just: Justify::Full,
            wp_just: Justify::Full,
            line_tabs: 0,
            in_table: false,
        }
    }

    /// The tab stop the next tab on the line goes to (from the paper
    /// edge): the n-th stop right of the left margin.  5.1 works out the
    /// real one again when it formats the line.
    fn stop(&mut self) -> (i32, TabKind, bool) {
        let off = if self.tabs.relative { self.left } else { 0 };
        let n = self.line_tabs;
        self.line_tabs += 1;
        let mut stops: Vec<(i32, TabKind, bool)> = self.tabs.stops.iter().map(|s| (s.pos + off, s.kind, s.dot_leader)).filter(|s| s.0 > self.left).collect();
        stops.sort_by_key(|s| s.0);
        stops.get(n).copied().unwrap_or((self.left + 600 * (n as i32 + 1), TabKind::Left, false))
    }
}

/// `C1`: tab, centre, flush right, margin release.
fn c1(o: &mut Vec<u8>, flags: u8, pos: i32) {
    o.extend_from_slice(&[0xC1, flags]);
    w16(o, pos);
    w16(o, pos);
    w16(o, pos / 120);
    o.push(0xC1);
}

/// A tab: `hard` forces its kind (Home,Tab …), a soft one takes the kind
/// of its tab stop.
fn tab(o: &mut Vec<u8>, st: &mut St, hard: Option<TabKind>) {
    let (pos, kind, dot) = st.stop();
    let (kind, soft) = match hard {
        Some(k) => (k, 0),
        None => (kind, 2),
    };
    let f = match kind {
        TabKind::Left => 0x00,
        TabKind::Right => 0x08,
        TabKind::Center => 0xC8,
        TabKind::Decimal => 0x40,
    };
    c1(o, f | soft | if dot { 0x10 } else { 0 }, pos);
}

// ------------------------------------------------------------------ fonts

/// The 27-byte font descriptor of a prefix font record (its first 24
/// bytes are the descriptor in a font change code): the numbers of the
/// Courier in WPCorp's printer test, with our point size and weight.  A
/// real 5.1 matches it to a font of its own printer.
fn font_desc(f: &FontSpec) -> [u8; 27] {
    let mut d: [u8; 27] = [0xDB, 0x01, 0x78, 0x00, 0x14, 0x1E, 0x0C, 0x17, 0x8C, 0x0A, 0, 0, 0x00, 0x04, 0x11, 0x40, 0xC9, 0x00, 0x87, 0xCF, 0x01, 0x00, 0x01, 0x00, 0, 0, 0];
    let size = (f.size_cpt / 2).clamp(1, 0xFFFF);
    d[0..2].copy_from_slice(&((size * 475 / 600).clamp(1, 0xFFFF) as u16).to_le_bytes());
    d[2..4].copy_from_slice(&((size / 5).clamp(1, 0xFFFF) as u16).to_le_bytes());
    d[24..26].copy_from_slice(&(size as u16).to_le_bytes());
    let n = f.name.to_lowercase();
    let fixed = ["courier", "mono", "consolas", "typewriter", "pitch", "cpi", "terminal"].iter().any(|x| n.contains(x));
    d[26] = if n.contains("bold") { 0x80 } else { 0x40 } | if n.contains("ital") || n.contains("oblique") { 0x08 } else { 0 } | if fixed { 0 } else { 0x10 };
    d
}

/// 5.1's own descriptor hash is not documented; the record and the font
/// change code only have to agree.
fn font_hash(d: &[u8]) -> i32 {
    d.iter().fold(0u16, |h, &b| h.rotate_left(3) ^ b as u16) as i32
}

// ------------------------------------------------------------------ writer

struct Writer {
    /// Prefix font records; 0 is the document's initial font.
    fonts: Vec<FontSpec>,
    /// Embedded WPG images (prefix packet 8).
    images: Vec<std::sync::Arc<Vec<u8>>>,
    /// What could not be written: name → how often.
    lost: BTreeMap<&'static str, usize>,
    /// Footnotes, endnotes, tables and boxes so far (they are numbered).
    notes: [i32; 2],
    tables: i32,
    boxes: [i32; 5],
    /// The format a text field (header, note, box) starts with.
    base: St,
}

impl Writer {
    fn lose(&mut self, what: &'static str) {
        *self.lost.entry(what).or_insert(0) += 1;
    }

    fn font(&mut self, f: &FontSpec) -> usize {
        if let Some(k) = self.fonts.iter().position(|x| x == f) {
            return k;
        }
        if self.fonts.len() >= 255 {
            return 0;
        }
        self.fonts.push(f.clone());
        self.fonts.len() - 1
    }

    /// A text field inside a function: it starts with the document's
    /// initial format and inherits nothing from the text around it.
    fn nested(&mut self, els: &[El], depth: u32) -> Vec<u8> {
        if depth >= MAX_DEPTH {
            return Vec::new();
        }
        let mut st = self.base.clone();
        self.stream(els, &mut st, depth + 1)
    }

    /// Body bytes of a stream (the document, or a text field).
    fn stream(&mut self, els: &[El], st: &mut St, depth: u32) -> Vec<u8> {
        let mut o = Vec::new();
        let mut i = 0;
        while i < els.len() {
            match &els[i] {
                El::Ch(c) => i += self.chars(*c, els, i, st, &mut o),
                El::Code(Code::TblDef { .. }) if !st.in_table => i = self.table(els, i, st, depth, &mut o),
                El::Code(c) => {
                    self.code(c, st, depth, &mut o);
                    i += 1;
                }
            }
        }
        o
    }

    /// The character at `i`; returns how many elements it took (a letter
    /// takes its combining marks along).
    fn chars(&mut self, c: char, els: &[El], i: usize, st: &mut St, o: &mut Vec<u8>) -> usize {
        let mut marks = String::new();
        while let Some(m) = is_mark(els.get(i + 1 + marks.chars().count())) {
            marks.push(m);
        }
        if !marks.is_empty() && c > ' ' {
            let n = 1 + marks.chars().count();
            // one WP character, or the letter overstruck with its accents
            if let Some((set, idx)) = str_to_wp(&format!("{c}{marks}")).filter(|x| x.1 < 256) {
                o.extend_from_slice(&[0xC0, idx as u8, set, 0xC0]);
                return n;
            }
            let mut t = Vec::new();
            if put_char(&mut t, c) {
                let before = t.len();
                for m in marks.chars().filter_map(spacing) {
                    match m {
                        '^' | '~' | '`' => t.push(m as u8),
                        m => {
                            put_char(&mut t, m);
                        }
                    }
                }
                if t.len() > before {
                    Pay::new(6).w(4, 1).tail(&t).put(o, 0xD8, 2);
                } else {
                    o.extend_from_slice(&t);
                }
                return n;
            }
        }
        match c {
            '\n' => {
                o.push(0x0A);
                st.line_tabs = 0;
            }
            '\u{c}' => o.push(0x0C),
            '\t' => tab(o, st, None),
            '\u{2}' => o.push(0x02),
            '\u{a0}' => o.push(0xA0),
            '\u{ad}' => o.push(0xAC),
            '\u{2011}' => o.push(b'-'),
            '\u{200b}' => o.push(0x93),
            c if (c as u32) < 0x20 || ('\u{7f}'..='\u{9f}').contains(&c) || ('\u{300}'..='\u{36F}').contains(&c) => {}
            c => {
                if !put_char(o, c) {
                    let f = fallback(c);
                    if f == "?" {
                        self.lose("characters (written as ?)");
                    }
                    o.extend(f.bytes().map(|b| if b == b'-' { 0xA9 } else { b }));
                }
            }
        }
        1
    }

    fn code(&mut self, c: &Code, st: &mut St, depth: u32, o: &mut Vec<u8>) {
        let pair = |o: &mut Vec<u8>, c: u8, sub: u8, old: u8, new: u8| Pay::new(6).b(4, old).b(5, new).put(o, c, sub);
        match c {
            Code::HRt => {
                o.push(0x0A);
                st.line_tabs = 0;
            }
            Code::HPg => {
                o.push(0x0C);
                st.line_tabs = 0;
            }
            Code::Tab => tab(o, st, None),
            Code::Indent | Code::DblIndent => {
                let pos = st.stop().0;
                o.extend_from_slice(&[0xC2, matches!(c, Code::DblIndent) as u8]);
                w16(o, pos - st.left);
                w16(o, pos);
                w16(o, pos);
                w16(o, pos / 120);
                o.push(0xC2);
            }
            Code::MarRel => c1(o, 0x80, (st.left - 600).max(0)),
            Code::Center => c1(o, 0xE0, (st.left + st.page_w - st.right) / 2),
            Code::FlushRight => c1(o, 0x60, st.page_w - st.right),
            Code::FlushRightDot => c1(o, 0x70, st.page_w - st.right),
            Code::HardSpace => o.push(0xA0),
            Code::HardHyphen => o.push(b'-'),
            Code::SoftHyphen => o.push(0xAC),
            Code::AttrOn(a) => o.extend_from_slice(&[0xC3, attr_ix(*a), 0xC3]),
            Code::AttrOff(a) => o.extend_from_slice(&[0xC4, attr_ix(*a), 0xC4]),
            // tracked changes: 5.1 marks them with redline and strikeout
            Code::RevOn { del, .. } => {
                self.lose("tracked changes (written as redline and strikeout)");
                o.extend_from_slice(&[0xC3, if *del { 13 } else { 10 }, 0xC3]);
            }
            Code::RevOff { del } => o.extend_from_slice(&[0xC4, if *del { 13 } else { 10 }, 0xC4]),
            Code::Font(f) => {
                let k = self.font(f);
                let d = font_desc(f);
                Pay::new(35).s(5, &d, 24).b(29, k as u8).w(30, font_hash(&d[..24])).w(32, (f.size_cpt / 2) as i32).b(34, d[26]).put(o, 0xD1, 1);
            }
            Code::LRMargin { left, right } => {
                Pay::new(12).w(4, st.left).w(6, st.right).w(8, *left).w(10, *right).put(o, 0xD0, 1);
                (st.left, st.right) = (*left, *right);
            }
            Code::TBMargin { top, bottom } => {
                Pay::new(12).w(4, st.top).w(6, st.bottom).w(8, *top).w(10, *bottom).put(o, 0xD0, 5);
                (st.top, st.bottom) = (*top, *bottom);
            }
            Code::LineSpacing(h) => {
                // whole lines in the high byte, 256ths in the low byte
                let v = |h: u32| ((h / 100).min(255) << 8 | ((h % 100) * 256 + 50) / 100) as i32;
                Pay::new(8).w(4, v(st.spacing)).w(6, v(*h)).put(o, 0xD0, 2);
                st.spacing = *h;
            }
            Code::Just(j) => {
                st.just = *j;
                if !st.in_table && *j != st.wp_just {
                    pair(o, 0xD0, 6, just_code(st.wp_just), just_code(*j));
                    st.wp_just = *j;
                }
            }
            Code::TabSet(t) => {
                // 40 positions (unused 0xFFFF), a nibble of type for each,
                // and for relative tabs the left margin they are measured
                // from, which is added to every position (more than the
                // margin where a stop would otherwise be negative)
                let rel = if t.relative { t.stops.iter().map(|s| -s.pos).fold(st.left, i32::max) } else { 0 };
                let mut p = Pay::new(208);
                let stops: Vec<&TabStop> = t.stops.iter().filter(|s| (0..0xFFFF).contains(&(s.pos + rel))).take(40).collect();
                for k in 0..40 {
                    p = p.w(104 + 2 * k, stops.get(k).map_or(0xFFFF, |s| s.pos + rel));
                }
                for (k, s) in stops.iter().enumerate() {
                    let ty = match s.kind {
                        TabKind::Left => 0,
                        TabKind::Center => 1,
                        TabKind::Right => 2,
                        TabKind::Decimal => 3,
                    } | if s.dot_leader { 4 } else { 0 };
                    p.0[180 + k / 2] |= if k % 2 == 0 { ty << 4 } else { ty };
                }
                p.w(204, if st.tabs.relative { st.left } else { 0xFFFF }).w(206, if t.relative { rel } else { 0xFFFF }).put(o, 0xD0, 4);
                st.tabs = t.clone();
            }
            Code::CenterPage => o.push(0x86),
            Code::Comment(s) => {
                // an empty comment is only a placeholder of ours
                if !s.is_empty() {
                    Pay::new(7).w(4, 10).b(6, s.lines().count().clamp(1, 255) as u8).tail(&wps(s)).put(o, 0xD9, 2);
                }
            }
            Code::DateCode(f) => Pay::new(68).s(4, &wps(f), 63).put(o, 0xD8, 0),
            Code::Hyphenation(on) => o.push(if *on { 0x9F } else { 0x9E }),
            Code::WidowOrphan(on) => o.push(if *on { 0x8A } else { 0x8B }),
            Code::PageNumber(n) => Pay::new(10).w(6, (*n).min(0x7FFF) as i32).put(o, 0xD3, 4),
            Code::PageNumberPos(p) => Pay::new(10).b(7, *p).put(o, 0xD0, 8),
            Code::Suppress(b) => pair(o, 0xD0, 7, 0, *b),
            Code::Note { kind, els } => {
                // the number in the note is superscript, 5.1's standard
                let k = (*kind == NoteKind::End) as usize;
                self.notes[k] += 1;
                let mut t = vec![0xC3, 5, 0xC3, 0x8D, 0xC4, 5, 0xC4];
                t.extend(self.nested(els, depth));
                match kind {
                    // flags, number, no further pages, then the formatter's work area
                    NoteKind::Foot => Pay::new(19).w(5, self.notes[0]).w(8, 200 * lines(els)).tail(&t).put(o, 0xD6, 0),
                    NoteKind::End => Pay::new(11).w(5, self.notes[1]).tail(&t).put(o, 0xD6, 1),
                }
            }
            Code::HeadFoot { which, occ, els } => {
                let (n, t) = match occ {
                    Occurrence::Discontinue => (0, Vec::new()),
                    Occurrence::Every => (1, self.nested(els, depth)),
                    Occurrence::Odd => (2, self.nested(els, depth)),
                    Occurrence::Even => (4, self.nested(els, depth)),
                };
                Pay::new(22).b(11, n).w(12, if n == 0 { 0 } else { 200 * lines(els) }).w(14, 1).tail(&t).put(o, 0xD5, (*which).min(3));
            }
            Code::ColDef { kind, cols } => {
                let n = cols.len().min(24);
                let k = match kind {
                    ColKind::Newspaper => 0,
                    ColKind::Parallel => 0x40,
                    ColKind::ParallelProtect => 0xC0,
                };
                let mut p = Pay::new(198).b(101, n as u8 | k);
                for (j, (l, r)) in cols.iter().take(n).enumerate() {
                    p = p.w(102 + 4 * j, *l).w(104 + 4 * j, *r);
                }
                p.put(o, 0xD2, 1);
            }
            Code::ColOn => o.push(0x87),
            Code::ColOff => o.push(0x88),
            Code::Merge(name) => match MERGE.iter().position(|m| m == name) {
                // {END RECORD} carries a field count
                Some(k) => Pay::new(if name == "END RECORD" { 6 } else { 4 }).put(o, 0xDE, 0x20 + k as u8),
                None => self.lose("merge commands 5.1 does not have"),
            },
            Code::Math(on) => o.push(if *on { 0xA7 } else { 0xA8 }),
            Code::MathOp(op) => match op {
                '+' => o.push(0xA1),
                't' => o.push(0xA2),
                '=' => o.push(0xA3),
                'T' => o.push(0xA4),
                '*' => o.push(0xA5),
                '!' => o.push(0xA6),
                'N' => o.push(0xB1),
                _ => {}
            },
            Code::Outline(true) => Pay::new(20).put(o, 0xD9, 4),
            Code::Outline(false) => o.push(0xB2),
            Code::BoxNum => o.push(0x8E),
            Code::BoxNewNum(kind, n) => Pay::new(8).w(6, ((*n).min(2047) << 5 | 1) as i32).put(o, 0xD3, 0x0C + kind.idx() as u8),
            Code::GLine(l) => {
                let (w, h) = if l.vertical { (l.width, l.length) } else { (l.length, l.width) };
                Pay::new(121).b(6, l.valign << 2).b(7, l.halign).w(8, w).w(10, h).w(12, l.x).w(14, l.y).b(36, l.gray).w(44, 100).w(46, 100).w(103, 1).put(o, 0xDA, if l.vertical { 6 } else { 5 });
            }
            Code::GBox(b) => self.gbox(b, st, depth, o),
            Code::Mark(m) => mark(m, o),
            Code::Fmt(f) => self.fmt(f, st, o),
            Code::Unknown { name, data } => {
                if framed(data) {
                    o.extend_from_slice(data);
                } else if let (Some(l), true) = (name.strip_prefix("Lang:"), data.is_empty()) {
                    self.fmt(&Fmt::Lang(l.into()), st, o);
                } else if !name.starts_with('\u{0}') {
                    self.lose("codes of other file formats");
                }
            }
            // styles and paragraph numbers were replaced by what they stand
            // for (`styles::flat_document`); these are leftovers in text fields
            Code::StyleOn(_) | Code::StyleOff(_) | Code::OpenStyle(_) => self.lose("styles (written as their codes)"),
            Code::ParNum(_) => self.lose("paragraph numbers (written as text)"),
            Code::ParNumDef(_) => self.lose("paragraph number definitions"),
            Code::MathDef(_) => self.lose("math definitions"),
            Code::BoxOpt(b) => {
                // the new values at +66: flags (numbers, as 5.1 has them
                // by default), shading, the four borders in one word,
                // spacing, and the caption number style with its
                // attribute codes.  Numbering method and caption
                // position are not written.
                let mut cap = Vec::new();
                let mut rest = b.cap_style.as_str();
                while let Some(ch) = rest.chars().next() {
                    let tok = rest.strip_prefix('[').and_then(|t| t.split_once(']')).and_then(|(m, r)| Some((Attr::from_mnemonic(m)?, m, r)));
                    match tok {
                        Some((a, m, r)) => {
                            let on = if m.chars().any(|x| x.is_ascii_uppercase()) { 0xC3 } else { 0xC4 };
                            cap.extend_from_slice(&[on, attr_ix(a), on]);
                            rest = r;
                        }
                        None => {
                            cap.extend(wps(&ch.to_string()));
                            rest = &rest[ch.len_utf8()..];
                        }
                    }
                }
                let bw = b.border.iter().fold(0i32, |w, x| w << 4 | (*x as i32 & 15));
                let mut p = Pay::new(128).b(66, 1).b(67, b.gray).w(68, bw).w(70, b.min_offset).s(88, &cap, 19);
                for k in 0..4 {
                    p = p.w(72 + 2 * k, b.outside[k]).w(80 + 2 * k, b.inside[k]);
                }
                p.put(o, 0xD2, 5 + b.kind.idx() as u8);
            }
            Code::Formula(_) => self.lose("table formulas (the result is kept)"),
            Code::SsLink { .. } => self.lose("spreadsheet links (the text is kept)"),
            Code::Link(_) => self.lose("hyperlinks (the text is kept)"),
            Code::Bookmark(_) => self.lose("bookmarks"),
            Code::SsLinkEnd | Code::LinkEnd => {}
            // table codes outside a table
            Code::TblDef { .. } | Code::Row | Code::Cell | Code::Join { .. } | Code::TblOpt(_) | Code::CellFmt(_) | Code::TblOff => {}
        }
    }

    /// The Shift-F8 Format codes of fmt.rs.
    fn fmt(&mut self, f: &Fmt, st: &mut St, o: &mut Vec<u8>) {
        let pair = |o: &mut Vec<u8>, c: u8, sub: u8, new: u8| Pay::new(6).b(5, new).put(o, c, sub);
        // a WP character word: set in the high byte
        let chw = |c: char| unicode_to_wp(c).map_or(c as i32 & 0x7F, |(set, idx)| (set as i32) << 8 | idx as i32 & 0xFF);
        let pct = |s: Spacing| match s {
            Spacing::Percent(p) => p as i32,
            _ => 0,
        };
        match f {
            Fmt::HZone { left, right } => {
                let z = |p: u8| ((p as u32 * 256 + 50) / 100).min(255) as u8;
                Pay::new(8).b(6, z(*left)).b(7, z(*right)).put(o, 0xD0, 3);
            }
            Fmt::CancelHyph => o.push(0x9A),
            Fmt::LnHeight(h) => Pay::new(8).w(6, h.unwrap_or(0)).put(o, 0xD0, 0),
            Fmt::LnNum(None) => Pay::new(14).put(o, 0xD3, 5),
            Fmt::LnNum(Some(l)) => {
                let fl = 0x80 | l.every.clamp(1, 31) as u8 | if l.restart { 0x20 } else { 0 } | if l.count_blank { 0x40 } else { 0 };
                Pay::new(14).b(9, fl).w(10, l.pos).w(12, l.start as i32).put(o, 0xD3, 5);
            }
            Fmt::JustLim { compress, expand } => Pay::new(12).w(8, *compress as i32).w(10, *expand as i32).put(o, 0xD3, 0x0B),
            Fmt::Force { odd } => Pay::new(7).b(6, *odd as u8).put(o, 0xD3, 7),
            Fmt::PgNumRoman(n) => Pay::new(10).w(6, (*n).min(0x7FFF) as i32).b(9, 1).put(o, 0xD3, 4),
            Fmt::PgNumStyle(s) => Pay::new(64).s(34, &wps(s), 29).put(o, 0xD3, 0x12),
            Fmt::InsertPgNum(s) => Pay::new(4).tail(&wps(s)).tail(&[0]).put(o, 0xD8, 3),
            Fmt::Paper(p) => {
                paper(p, o);
                st.page_w = p.w;
            }
            Fmt::BlockPro(on) => o.extend_from_slice(&[0xC5, !*on as u8, 0, 0, 0xC5]),
            Fmt::CndlEop(n) => Pay::new(5).b(4, (*n).min(255) as u8).put(o, 0xD9, 1),
            Fmt::Adv(a, v) => {
                // flags: 1 = to a position, 2 = across; relative ones are signed
                let (fl, v) = match a {
                    Adv::Up => (0, -*v),
                    Adv::Down => (0, *v),
                    Adv::ToLine => (1, *v),
                    Adv::Left => (2, -*v),
                    Adv::Right => (2, *v),
                    Adv::ToPos => (3, *v),
                };
                Pay::new(9).b(4, fl).w(7, v).put(o, 0xD3, 6);
            }
            Fmt::DecAlign { dec, thou } => Pay::new(12).w(4, chw('.')).w(6, chw(',')).w(8, chw(*dec)).w(10, chw(*thou)).put(o, 0xD3, 0),
            Fmt::Lang(l) => {
                let b: Vec<u8> = l.bytes().filter(u8::is_ascii_alphabetic).map(|b| b.to_ascii_uppercase()).take(2).collect();
                if b.len() == 2 {
                    Pay::new(8).s(4, b"US", 2).s(6, &b, 2).put(o, 0xD3, 0x11);
                }
            }
            Fmt::Ovrstk(s) => Pay::new(6).w(4, 1).tail(&wps(s)).put(o, 0xD8, 2),
            Fmt::Kern(on) => pair(o, 0xD9, 3, *on as u8),
            Fmt::PtrCmnd(s) => {
                let (file, s) = match s.strip_prefix("File=") {
                    Some(f) => (1, f),
                    None => (0, s.as_str()),
                };
                Pay::new(5).b(4, file).tail(&wps(s)).tail(&[0]).put(o, 0xD9, 0);
            }
            Fmt::WrdLtr { word, letter } => Pay::new(12).w(8, pct(*word)).w(10, pct(*letter)).put(o, 0xD3, 0x0A),
            Fmt::Bline(on) => pair(o, 0xD3, 8, *on as u8),
            Fmt::LeadingAdj { primary, secondary } => Pay::new(12).w(8, *primary).w(10, *secondary).put(o, 0xD9, 5),
            Fmt::Undrln { spaces, tabs } => pair(o, 0xD3, 1, *spaces as u8 | (*tabs as u8) << 1),
            Fmt::BrdrOpt(_) => self.lose("border options"),
            Fmt::HardTab(k) => tab(o, st, Some(*k)),
            Fmt::DecTab => tab(o, st, Some(TabKind::Decimal)),
            Fmt::ISRt => o.push(0x93),
        }
    }

    /// A graphics box `DA 0–4`: 117 bytes of settings, the caption, then
    /// the contents (text, or an equation in a `DF` function).
    fn gbox(&mut self, b: &crate::gfx::GBox, st: &St, depth: u32, o: &mut Vec<u8>) {
        use crate::gfx::*;
        let k = b.kind.idx();
        self.boxes[k] += 1;
        let pos = match (b.anchor, b.vpos) {
            (Anchor::Para, _) => 0,
            (Anchor::Char, _) => 2,
            (Anchor::Page, VPos::Full) => 1,
            (Anchor::Page, VPos::Top) => 1 | 1 << 2,
            (Anchor::Page, VPos::Center) => 1 | 2 << 2,
            (Anchor::Page, VPos::Bottom) => 1 | 3 << 2,
            (Anchor::Page, _) => 1 | 4 << 2,
        };
        let al = match b.hpos {
            HPos::Left => 0,
            HPos::Right => 1,
            HPos::Center => 2,
            HPos::Full => 3,
            HPos::Set(_) => 2 << 2,
        } | if b.wrap { 0 } else { 0x80 }
            | if b.auto & 1 == 0 { 0x10 } else { 0 }
            | if b.auto & 2 == 0 { 0x20 } else { 0 };
        let cap = if b.caption.is_empty() { Vec::new() } else { self.nested(&b.caption, depth) };
        let wpg = b.data.as_ref().filter(|d| d.starts_with(b"\xFFWPC") && d.get(9) == Some(&0x16));
        let (ty, image, body) = match b.contents {
            Contents::Text => (0x10, 0, self.nested(&b.text, depth)),
            Contents::Equation => {
                // the source text only: 5.1 compiles it when the equation is edited
                let t = wps(&b.equation);
                let mut df = Vec::new();
                Pay::new(12).w(10, t.len() as i32).tail(&t).put(&mut df, 0xDF, 0);
                (0x08, 0, df)
            }
            Contents::Image => match wpg {
                Some(d) if self.images.len() < 1000 => {
                    self.images.push(d.clone());
                    (0x80, self.images.len() - 1, Vec::new())
                }
                // not a WPG graphic: the box stays, with the file name
                _ => {
                    self.lose("images that are not WPG graphics (the box is kept)");
                    (if b.file.is_empty() { 0 } else { 2 }, 0, Vec::new())
                }
            },
            Contents::Empty => (0, 0, Vec::new()),
        };
        let (x, y) = (if let HPos::Set(x) = b.hpos { x } else { 0 }, if let VPos::Set(y) = b.vpos { y } else { 0 });
        // where the box stands, from the left paper edge (5.1 works it out again)
        let room = st.page_w - st.left - st.right;
        let edge = match b.hpos {
            HPos::Left | HPos::Full => st.left,
            HPos::Right => st.left + room - b.width,
            HPos::Center => st.left + (room - b.width) / 2,
            HPos::Set(x) => x,
        };
        let rot = b.img.rot.rem_euclid(360) | if b.img.invert { 0x4000 } else { 0 } | if b.img.mirror { 0x8000 } else { 0 };
        Pay::new(121)
            .w(4, self.boxes[k].min(2047) << 5 | 1)
            .b(6, pos)
            .b(7, al)
            .w(8, b.width)
            .w(10, b.height)
            .w(12, edge.max(0))
            .w(32, x)
            .w(34, y)
            .w(42, rot)
            .w(44, b.img.sx)
            .w(46, b.img.sy)
            .b(52, ty)
            .s(53, &wps(&b.file), 20)
            .w(103, 1)
            .w(107, b.width)
            .w(109, b.height)
            .w(113, image as i32)
            .w(119, cap.len() as i32)
            .tail(&cap)
            .tail(&body)
            .put(o, 0xDA, k as u8);
    }

    /// A whole table, `[Tbl Def]` to `[Tbl Off]`; returns the index behind it.
    fn table(&mut self, els: &[El], def: usize, st: &mut St, depth: u32, o: &mut Vec<u8>) -> usize {
        use crate::tables::Slot;
        let Some(t) = crate::tables::parse(els, def) else { return def + 1 };
        let next = if matches!(els.get(t.off), Some(El::Code(Code::TblOff))) { t.off + 1 } else { t.off };
        let n = t.cols.len().min(32);
        if n == 0 || t.rows.is_empty() {
            return next;
        }
        let opt = crate::tables::opt_at(els, def);
        let slots = t.slots(els);
        let nrows = t.rows.len();
        // the cell a place in the grid belongs to: row, column, columns, rows
        let owner = |r: usize, c: usize| match slots.get(r).and_then(|x| x.get(c)) {
            Some(Slot::Anchor { cols, rows }) => (r, c, *cols, *rows),
            Some(Slot::Covered { r: ar, c: ac }) => match slots[*ar][*ac] {
                Slot::Anchor { cols, rows } => (*ar, *ac, cols, rows),
                _ => (r, c, 1, 1),
            },
            _ => (r, c, 1, 1),
        };
        let cell_fmt = |r: usize, c: usize| if c < t.rows[r].len() { t.fmt_of(els, r, c).map(|x| x.1).unwrap_or_default() } else { CellFmt::default() };
        let side = |f: &CellFmt, k: usize| f.lines[k].map(|s| s as u8);

        // [Tbl Def]: the old values (5.1 writes its 3-column default
        // there), then position, shading, columns, table number, left
        // edge, cell margins, header rows and the columns
        self.tables += 1;
        let total: i32 = t.cols.iter().take(n).sum();
        let room = st.page_w - st.left - st.right;
        let (pos, edge) = match opt.pos {
            TblPos::Left => (0, st.left),
            TblPos::Right => (1, st.left + room - total),
            TblPos::Center => (2, st.left + (room - total) / 2),
            TblPos::Full => (3, st.left),
            TblPos::Set(x) => (4, x),
        };
        let base = 4 + 24 + 15;
        let mut p = Pay::new(base + 24 + 5 * n + 2)
            .b(5, 10)
            .w(6, 3)
            .w(12, 100)
            .w(14, 100)
            .w(16, 120)
            .b(base, pos | if opt.neg_paren { 0x20 } else { 0 })
            .b(base + 1, opt.gray.min(100))
            .w(base + 2, n as i32)
            .w(base + 4, self.tables.min(2047) << 5 | 1)
            .w(base + 6, edge.max(0))
            .w(base + 8, opt.pad[0])
            .w(base + 10, opt.pad[1])
            .w(base + 12, opt.pad[2])
            .w(base + 14, opt.pad[3])
            .w(base + 16, opt.header_rows as i32)
            .w(base + 20, 1)
            .w(base + 22, if pos == 4 { edge } else { 0 });
        for (k, w) in t.cols.iter().take(n).enumerate() {
            // column: width, attributes (none), left justified with 2 decimals
            p = p.w(base + 24 + 2 * k, *w).b(base + 24 + 4 * n + k, 0x20);
        }
        p.put(o, 0xD2, 0x0B);

        // the lines of every row: two bytes for each column (see
        // `wp5::row_lines`).  A cell owns its left and top line; the
        // right and the bottom line only at the edge of the table, or
        // where they were set for this cell.
        let rows: Vec<Vec<u8>> = (0..nrows)
            .map(|r| {
                let pairs: Vec<(u8, u8)> = (0..n)
                    .map(|c| {
                        let (ar, ac, cs, rs) = owner(r, c);
                        let f = cell_fmt(ar, ac);
                        let gone = |f: CellFmt, k: usize| f.lines[k] == Some(LineStyle::None);
                        let left = if c != ac {
                            0
                        } else if c > 0 && f.lines[0].is_none() && gone(cell_fmt(owner(r, c - 1).0, owner(r, c - 1).1), 1) {
                            0
                        } else {
                            side(&f, 0).unwrap_or(if c == 0 { 2 } else { 1 })
                        };
                        let top = if r != ar {
                            0
                        } else if r > 0 && f.lines[2].is_none() && gone(cell_fmt(owner(r - 1, c).0, owner(r - 1, c).1), 3) {
                            0
                        } else {
                            side(&f, 2).unwrap_or(if r == 0 { 2 } else { 1 })
                        };
                        let right = if c + 1 != ac + cs {
                            0
                        } else if c + 1 == n {
                            side(&f, 1).unwrap_or(2)
                        } else {
                            side(&f, 1).unwrap_or(0)
                        };
                        let bottom = if r + 1 != ar + rs {
                            0
                        } else if r + 1 == nrows {
                            side(&f, 3).unwrap_or(2)
                        } else {
                            side(&f, 3).unwrap_or(0)
                        };
                        (left << 3 | top, if f.shade { 0x40 } else { 0 } | right << 3 | bottom)
                    })
                    .collect();
                // equal neighbours as `count 0x80 pair`
                let mut d = Vec::new();
                let mut c = 0;
                while c < n {
                    let run = pairs[c..].iter().take_while(|x| **x == pairs[c]).count();
                    if run > 1 {
                        d.extend_from_slice(&[run as u8, 0x80]);
                    }
                    d.extend_from_slice(&[pairs[c].0, pairs[c].1]);
                    c += run;
                }
                // flags (several lines, automatic height), a height 5.1
                // works out again, 0x80 + length, the lines
                let tall = (0..t.rows[r].len()).map(|c| t.cell_range(els, r, c)).map(|(a, b)| lines(&els[a..b])).max().unwrap_or(1);
                let h = 185 + 200 * tall;
                let mut blk = vec![0x03, h as u8, (h >> 8) as u8, 0x80 | d.len() as u8];
                blk.extend(d);
                blk
            })
            .collect();
        let height = |r: usize| u16::from_le_bytes([rows[r][1], rows[r][2]]) as i32;

        st.in_table = true;
        let mut y = 0;
        for r in 0..nrows {
            // [Row]: the lines of the row before, then of this row, then
            // where the row starts
            Pay::new(4).tail(&rows[r.saturating_sub(1)]).tail(&rows[r]).tail(&(y.min(0xFFFF) as u16).to_le_bytes()).put(o, 0xDC, 1);
            for c in 0..n.min(t.rows[r].len()) {
                // hidden under a joined cell: the one below its first
                // column is written (marked 0x80), the others are not
                let (ar, ac, cs, rs) = owner(r, c);
                let hidden = (ar, ac) != (r, c);
                let (a, b) = t.cell_range(els, r, c);
                if hidden && c != ac {
                    for e in &els[a..b] {
                        if let El::Code(Code::Just(j)) = e {
                            st.just = *j;
                        }
                    }
                    continue;
                }
                // codes at the start of the cell: its justification and the
                // attributes that run to its end go into the [Cell] code
                let lead = els[a..b].iter().take_while(|e| matches!(e, El::Code(Code::Join { .. } | Code::CellFmt(_) | Code::TblOpt(_) | Code::Just(_) | Code::AttrOn(_)))).count();
                let trail = els[a + lead..b].iter().rev().take_while(|e| matches!(e, El::Code(Code::AttrOff(_)))).count();
                let closed = |x: Attr| els[b - trail..b].contains(&El::Code(Code::AttrOff(x)));
                let mut attrs = 0i32;
                let mut body: Vec<El> = Vec::new();
                for e in &els[a..a + lead] {
                    match e {
                        El::Code(Code::Just(j)) => st.just = *j,
                        El::Code(Code::AttrOn(x)) if closed(*x) => attrs |= 1 << attr_ix(*x),
                        El::Code(Code::AttrOn(_)) => body.push(e.clone()),
                        _ => {}
                    }
                }
                body.extend_from_slice(&els[a + lead..b - trail]);
                body.extend(els[b - trail..b].iter().filter(|e| !matches!(e, El::Code(Code::AttrOff(x)) if attrs & 1 << attr_ix(*x) != 0)).cloned());
                // flags: 1 = own justification, 2 = own attributes
                let j = just_code(st.just);
                Pay::new(15).b(4, (j != 0) as u8 | if attrs != 0 { 2 } else { 0 }).b(5, c as u8).b(6, if hidden { 0x80 | cs.min(63) as u8 } else { cs.min(63) as u8 }).b(7, if hidden { 1 } else { rs.min(255) as u8 }).w(12, attrs).b(14, 0x20 | j).put(o, 0xDC, 0);
                st.line_tabs = 0;
                o.extend(self.stream(&body, st, depth));
                // the reader takes one hard return at the end of a cell
                // as part of the next cell code (see `wp5::Parser::decode`)
                if matches!(body.last(), Some(El::Code(Code::HRt))) {
                    o.push(0x0A);
                }
            }
            if r + 1 < nrows {
                y += height(r);
            }
        }
        // [Tbl Off]: the last row's lines, its number, where it starts,
        // header rows, columns
        Pay::new(4).tail(&rows[nrows - 1]).tail(&(nrows as u16 - 1).to_le_bytes()).tail(&(y.min(0xFFFF) as u16).to_le_bytes()).tail(&opt.header_rows.to_le_bytes()).tail(&[n as u8]).put(o, 0xDC, 2);
        st.in_table = false;
        st.line_tabs = 0;
        // a justification set in the cells goes on behind the table
        if st.just != st.wp_just && !matches!(els.get(next), Some(El::Code(Code::Just(_)))) {
            Pay::new(6).b(4, just_code(st.wp_just)).b(5, just_code(st.just)).put(o, 0xD0, 6);
            st.wp_just = st.just;
        }
        next
    }
}

/// Lines of a text field (for the heights 5.1 keeps and works out again).
fn lines(els: &[El]) -> i32 {
    1 + els.iter().filter(|e| matches!(e, El::Code(Code::HRt))).count().min(100) as i32
}

/// `[Paper Sz/Typ]`, `D0/0B`: the size asked for and the size used (each
/// length, width, type, name), then the label settings.
fn paper(p: &crate::fmt::Paper, o: &mut Vec<u8>) {
    let ty = match p.ty.as_str() {
        "Standard" => 1,
        "Bond" => 2,
        "Letterhead" => 3,
        "Labels" => 4,
        "Envelope" => 5,
        "Transparency" => 6,
        "Cardstock" => 7,
        _ => 9,
    };
    let name = wps(&p.ty);
    let n = name.len().min(40);
    let mut pay = Pay::new(247).b(195, 3).b(196, 0x3C).b(223, 1);
    for k in [99, 146] {
        pay = pay.w(k, p.h).w(k + 2, p.w).b(k + 4, ty).b(k + 5, n as u8).s(k + 6, &name, 40);
    }
    pay = pay.b(193, p.landscape() as u8);
    for k in 0..4 {
        pay = pay.w(197 + 2 * k, 1200);
    }
    if let Some(l) = &p.labels {
        pay = pay.w(197, l.ml).w(199, l.mr).w(201, l.mt).w(203, l.mb).b(222, 1).b(223, l.cols.clamp(1, 255) as u8).b(224, l.rows.clamp(1, 255) as u8);
        pay = pay.w(225, l.top).w(227, l.left).w(229, l.w).w(231, l.h).w(233, l.gap_col).w(235, l.gap_row);
    }
    pay.put(o, 0xD0, 0x0B);
}

/// Mark Text codes, group `D7` (and `[End Def]`).
fn mark(m: &Mark, o: &mut Vec<u8>) {
    let level = |list: bool, n: u8| (if list { 0x20 } else { 0 }) | (n.clamp(1, 16) - 1);
    let two = |a: &str, b: &str| {
        let mut v = wps(a);
        v.push(0);
        v.extend(wps(b));
        v
    };
    let file = |o: &mut Vec<u8>, sub: u8, f: &str| Pay::new(7).tail(&wps(f)).tail(&[0]).put(o, 0xD7, sub);
    match m {
        Mark::Begin { list, n } => Pay::new(5).b(4, level(*list, *n)).put(o, 0xD7, 0),
        Mark::End { list, n } => Pay::new(5).b(4, level(*list, *n)).put(o, 0xD7, 1),
        Mark::Index { heading, sub } => Pay::new(4).tail(&two(heading, sub)).put(o, 0xD7, 3),
        Mark::ToA { section, short, full } => Pay::new(5).b(4, if *section == 0 { 0x20 } else { *section }).tail(&two(short, full)).put(o, 0xD7, 4),
        // definitions: kind and level, then the page number style of five levels
        Mark::DefToc { levels, styles, .. } => Pay::new(10).b(4, levels.clamp(&1, &5) - 1).s(5, styles, 5).put(o, 0xD7, 2),
        Mark::DefList { n, style } => Pay::new(10).b(4, 0x20 | n.clamp(&1, &16) - 1).s(5, &[*style; 5], 5).put(o, 0xD7, 2),
        Mark::DefIndex { style, concordance } => Pay::new(10).b(4, 0x10).s(5, &[*style; 5], 5).tail(&wps(concordance)).tail(&[0]).put(o, 0xD7, 2),
        Mark::DefToA { section, dots, .. } => Pay::new(10).b(4, 0x30 | section.clamp(&1, &16) - 1).s(5, &[if *dots { 4 } else { 3 }; 5], 5).put(o, 0xD7, 2),
        Mark::EndDef => o.push(0x9B),
        Mark::Ref { name, kind, text } => Pay::new(5).b(4, (*kind).min(8)).tail(&two(name, text)).put(o, 0xD7, 7),
        Mark::Target { name } => Pay::new(4).tail(&wps(name)).tail(&[0]).put(o, 0xD7, 8),
        Mark::Subdoc(f) => file(o, 9, f),
        Mark::SubdocStart(f) => file(o, 10, f),
        Mark::SubdocEnd(f) => file(o, 11, f),
    }
}

// ------------------------------------------------------------------ prefix

/// Packet 1, the document summary in 5.1 layout: NUL-terminated strings
/// (date, name and type padded with spaces to their full length), then
/// 0xFF, the creation date in 10 bytes and 0xFF.
fn summary(s: &Summary) -> Vec<u8> {
    let pad = |t: &str, n: usize| {
        let mut v = wps(t);
        while v.len() >= n {
            // too long: drop whole characters from the end
            let cut = if v.len() >= 4 && v[v.len() - 1] == 0xC0 { 4 } else { 1 };
            v.truncate(v.len() - cut);
        }
        v.resize(n - 1, b' ');
        v.push(0);
        v
    };
    let mut p = pad("", 26);
    p.extend(pad(&s.descriptive_name, 68));
    p.extend(pad("", 21));
    for t in [s.subject.as_str(), s.author.as_str(), s.typist.as_str(), s.comments.as_str(), "", ""] {
        p.extend(wps(t));
        p.push(0);
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = crate::filelist::civil_from_days(now.div_euclid(86400));
    let secs = now.rem_euclid(86400);
    p.push(0xFF);
    p.extend_from_slice(&(y as u16).to_le_bytes());
    p.extend_from_slice(&[m as u8, d as u8, (secs / 3600) as u8, (secs % 3600 / 60) as u8, 0, 0, 0, 0]);
    p.push(0xFF);
    p
}

/// Header and prefix: index blocks of five 10-byte entries (the first
/// one the block's own header), each followed by the data of its packets.
fn prefix(packets: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut f = vec![0xFF, b'W', b'P', b'C', 0, 0, 0, 0, 1, 0x0A, 0, 1, 0, 0, 0, 0];
    let groups: Vec<&[(u16, Vec<u8>)]> = packets.chunks(4).collect();
    for (g, grp) in groups.iter().enumerate() {
        let start = f.len();
        let size: usize = grp.iter().map(|p| p.1.len()).sum();
        let next = if g + 1 < groups.len() { start + 50 + size } else { 0 };
        f.extend_from_slice(&[0xFB, 0xFF, 5, 0, 50, 0]);
        f.extend_from_slice(&(next as u32).to_le_bytes());
        let mut at = start + 50;
        for k in 0..4 {
            match grp.get(k) {
                Some((ty, data)) => {
                    f.extend_from_slice(&ty.to_le_bytes());
                    // the images of packet 8 do not count in its length
                    f.extend_from_slice(&(if *ty == 8 { 2 } else { data.len() as u32 }).to_le_bytes());
                    f.extend_from_slice(&(at as u32).to_le_bytes());
                    at += data.len();
                }
                None => f.extend_from_slice(&[0; 10]),
            }
        }
        for p in grp.iter() {
            f.extend_from_slice(&p.1);
        }
    }
    let doc = f.len() as u32;
    f[4..8].copy_from_slice(&doc.to_le_bytes());
    f
}

// ------------------------------------------------------------------ entry

/// The document as a WordPerfect 5.1 file.
pub fn write(doc: &Document) -> Vec<u8> {
    report(doc).0
}

/// The file, and what could not be written into it (`hyperlinks (the
/// text is kept) ×3` …).
pub fn report(doc: &Document) -> (Vec<u8>, Vec<String>) {
    // styles and paragraph numbers as the codes and text they stand for
    let flat = crate::styles::flat_document(doc);
    let els = flat.as_ref().map_or(&doc.els, |d| &d.els);
    let mut w = Writer { fonts: vec![doc.init.font.clone()], images: vec![], lost: BTreeMap::new(), notes: [0; 2], tables: 0, boxes: [0; 5], base: St::wp_default() };
    if flat.is_some() {
        w.lose("styles and paragraph numbers (written as their codes and text)");
    }
    if doc.password.is_some() {
        w.lose("the password");
    }

    // Document initial codes (packet 3).  What else the initial format
    // differs in from 5.1's own goes to the top of the text, the paper
    // size first: a 5.1 document without one is on Letter paper.
    let mut st = St::wp_default();
    let initial = w.stream(&doc.initial_codes, &mut st, 0);
    let is_paper = |c: &Code| matches!(c, Code::Fmt(Fmt::Paper(_))) || matches!(c, Code::Unknown { data, .. } if data.starts_with(&[0xD0, 0x0B]));
    let has_paper = doc.initial_codes.iter().chain(els.iter().take_while(|e| !e.is_char())).any(|e| e.code().is_some_and(is_paper));
    let i = &doc.init;
    let mut top: Vec<El> = Vec::new();
    if !has_paper {
        top.push(El::Code(Code::Fmt(Fmt::Paper(crate::fmt::Paper { w: i.page_width, h: i.page_height, ty: "Standard".into(), labels: None }))));
    }
    st.page_w = i.page_width;
    if (i.left, i.right) != (st.left, st.right) {
        top.push(El::Code(Code::LRMargin { left: i.left, right: i.right }));
    }
    if (i.top, i.bottom) != (st.top, st.bottom) {
        top.push(El::Code(Code::TBMargin { top: i.top, bottom: i.bottom }));
    }
    if i.justify != st.just {
        top.push(El::Code(Code::Just(i.justify)));
    }
    if i.line_spacing != st.spacing {
        top.push(El::Code(Code::LineSpacing(i.line_spacing)));
    }
    if i.tabs != st.tabs {
        top.push(El::Code(Code::TabSet(i.tabs.clone())));
    }
    w.base = st.clone();
    let mut body = w.stream(&top, &mut st, 0);
    body.extend(w.stream(els, &mut st, 0));

    // The prefix, as in WPCorp's printer test (a document for no printer
    // in particular): flags, font names, printer, fonts; then summary and
    // initial codes when there are any; the images last.
    let mut names = Vec::new();
    let mut recs = Vec::new();
    for f in &w.fonts {
        let d = font_desc(f);
        let size = (f.size_cpt / 2).clamp(1, 0xFFFF) as i32;
        // font id, no width table, heights and widths, name, flags, hash,
        // descriptor, then "no other font" for attributes, character sets
        // and orientations
        recs.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
        for v in [size * 122 / 600, size * 78 / 600, size / 5, size / 5, size / 5, 10, 1] {
            w16(&mut recs, v);
        }
        w16(&mut recs, names.len() as i32);
        recs.push(0);
        w16(&mut recs, font_hash(&d[..24]));
        recs.extend_from_slice(&d);
        recs.extend_from_slice(&[0xFF; 36]);
        names.extend(f.name.chars().map(|c| if (' '..='~').contains(&c) { c as u8 } else { b'?' }));
        names.push(0);
    }
    let d0 = font_desc(&doc.init.font);
    let mut printer = vec![0u8; 50];
    printer.extend_from_slice(&d0[..24]);
    printer.extend_from_slice(&[0; 13]);
    printer.extend_from_slice(&d0[24..]);
    let mut packets: Vec<(u16, Vec<u8>)> = vec![(6, vec![0x08, 0x23, 0x7C, 0x00, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), (7, names), (12, printer), (15, recs)];
    let s = &doc.summary;
    if [&s.descriptive_name, &s.subject, &s.author, &s.typist, &s.comments].iter().any(|t| !t.trim().is_empty()) {
        packets.push((1, summary(s)));
    }
    if !initial.is_empty() {
        packets.push((3, initial));
    }
    let mut img = (w.images.len() as u16).to_le_bytes().to_vec();
    for d in &w.images {
        img.extend_from_slice(&(d.len() as u32).to_le_bytes());
    }
    for d in &w.images {
        img.extend_from_slice(d);
    }
    packets.push((8, img));
    let mut file = prefix(&packets);
    file.extend(body);
    (file, w.lost.iter().map(|(k, n)| if *n > 1 { format!("{k} \u{d7}{n}") } else { k.to_string() }).collect())
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp5::read;

    fn c(x: Code) -> El {
        El::Code(x)
    }

    fn text(s: &str) -> Vec<El> {
        s.chars().map(El::Ch).collect()
    }

    /// A document on Letter paper (the default paper follows the locale).
    fn doc(els: Vec<El>) -> Document {
        let mut d = Document::new();
        (d.init.page_width, d.init.page_height) = (10200, 13200);
        d.els = els;
        d
    }

    /// Written and read again; the paper size the writer puts first is
    /// checked and taken off.
    fn back(d: &Document) -> Document {
        let mut b = read(&write(d)).expect("our own file reads");
        assert!(matches!(b.els.first(), Some(El::Code(Code::Fmt(Fmt::Paper(p)))) if (p.w, p.h) == (d.init.page_width, d.init.page_height)), "{:?}", b.els.first());
        b.els.remove(0);
        b
    }

    #[test]
    fn header_and_prefix() {
        let f = write(&doc(text("Hi")));
        assert_eq!(f[..4], [0xFF, b'W', b'P', b'C']);
        assert_eq!(f[8..16], [1, 0x0A, 0, 1, 0, 0, 0, 0]);
        let body = u32::from_le_bytes([f[4], f[5], f[6], f[7]]) as usize;
        assert!(f.ends_with(b"Hi") && body < f.len() - 2);
        // index blocks: marker, 5 entries, 50 bytes; the chain ends with 0
        let mut at = 16;
        let mut types = vec![];
        loop {
            assert_eq!(f[at..at + 6], [0xFB, 0xFF, 5, 0, 50, 0]);
            for k in 1..5 {
                let e = &f[at + 10 * k..at + 10 * k + 10];
                let (ty, len, off) = (u16::from_le_bytes([e[0], e[1]]), u32::from_le_bytes([e[2], e[3], e[4], e[5]]) as usize, u32::from_le_bytes([e[6], e[7], e[8], e[9]]) as usize);
                if ty != 0 {
                    assert!(off >= at + 50 && off + len <= body, "packet {ty}");
                    types.push((ty, len));
                }
            }
            at = u32::from_le_bytes([f[at + 6], f[at + 7], f[at + 8], f[at + 9]]) as usize;
            if at == 0 {
                break;
            }
        }
        assert_eq!(types, [(6, 16), (7, 12), (12, 90), (15, 86), (8, 2)]);
        let d = read(&f).unwrap();
        assert_eq!(d.init.font, FontSpec::default_font());
        assert_eq!((d.init.page_width, d.init.page_height), (10200, 13200));
    }

    /// Text with accents, attributes, format codes, a header and notes.
    fn rich() -> Document {
        let tabs = TabSet { stops: vec![TabStop { pos: 600, kind: TabKind::Left, dot_leader: false }, TabStop { pos: 2400, kind: TabKind::Decimal, dot_leader: true }, TabStop { pos: 4800, kind: TabKind::Right, dot_leader: false }], relative: true };
        let mut els = vec![
            c(Code::LRMargin { left: 1800, right: 900 }),
            c(Code::TBMargin { top: 600, bottom: 1500 }),
            c(Code::Just(Justify::Left)),
            c(Code::LineSpacing(150)),
            c(Code::TabSet(tabs)),
            c(Code::HeadFoot { which: 0, occ: Occurrence::Every, els: [vec![c(Code::Center)], text("Kopf – Seite \u{2}")].concat() }),
            c(Code::HeadFoot { which: 3, occ: Occurrence::Even, els: text("Fuß") }),
            c(Code::PageNumberPos(6)),
        ];
        els.extend(text("Grüße à Çé ñ Ω Привет “x” a-b"));
        els.extend([c(Code::Tab), c(Code::AttrOn(Attr::Bold)), c(Code::AttrOn(Attr::Italic))]);
        els.extend(text("fett"));
        els.extend([c(Code::AttrOff(Attr::Bold)), c(Code::AttrOff(Attr::Italic)), c(Code::Note { kind: NoteKind::Foot, els: [text("Fußnote "), vec![c(Code::AttrOn(Attr::Underline))], text("eins"), vec![c(Code::AttrOff(Attr::Underline))]].concat() })]);
        els.extend([c(Code::HRt), c(Code::Center)]);
        els.extend(text("Mitte"));
        els.extend([c(Code::HRt), c(Code::FlushRight)]);
        els.extend(text("rechts"));
        els.extend([c(Code::HRt), c(Code::Indent), c(Code::HardSpace), c(Code::SoftHyphen), c(Code::HardHyphen), c(Code::Note { kind: NoteKind::End, els: text("Ende") }), c(Code::HPg), c(Code::Just(Justify::Center))]);
        els.extend([c(Code::Font(FontSpec { name: "Helvetica".into(), size_cpt: 1400 })), c(Code::Fmt(Fmt::Lang("DE".into()))), c(Code::DateCode("3 1, 4".into())), c(Code::Comment("Zeile 1\nZeile 2".into()))]);
        els.extend([c(Code::Suppress(5)), c(Code::PageNumber(7)), c(Code::WidowOrphan(true)), c(Code::Hyphenation(true)), c(Code::DblIndent), c(Code::MarRel), c(Code::CenterPage)]);
        els.extend([c(Code::Mark(Mark::Begin { list: false, n: 2 })), c(Code::Mark(Mark::End { list: false, n: 2 })), c(Code::Mark(Mark::Index { heading: "Äpfel".into(), sub: "rot".into() }))]);
        els.extend([c(Code::ColDef { kind: ColKind::Newspaper, cols: vec![(1200, 4800), (5400, 9000)] }), c(Code::ColOn), c(Code::ColOff), c(Code::Merge("FIELD".into()))]);
        els.extend(text("1~"));
        els.push(c(Code::Fmt(Fmt::Adv(Adv::Down, 600))));
        doc(els)
    }

    #[test]
    fn text_and_codes_come_back() {
        let d = rich();
        let b = back(&d);
        assert_eq!(b.els.len(), d.els.len());
        for (k, (x, y)) in b.els.iter().zip(&d.els).enumerate() {
            assert_eq!(x, y, "element {k}");
        }
    }

    #[test]
    fn initial_format() {
        // what differs from 5.1's own start goes to the top of the text;
        // document initial codes stay initial codes
        let mut d = doc(text("x"));
        (d.init.left, d.init.right, d.init.justify, d.init.line_spacing) = (600, 900, Justify::Left, 200);
        d.init.font = FontSpec { name: "Times Roman".into(), size_cpt: 1000 };
        let b = back(&d);
        assert_eq!(b.els, [c(Code::LRMargin { left: 600, right: 900 }), c(Code::Just(Justify::Left)), c(Code::LineSpacing(200)), El::Ch('x')]);
        assert_eq!(b.init.font, d.init.font);
        d.initial_codes = vec![c(Code::LRMargin { left: 600, right: 900 }), c(Code::Just(Justify::Left)), c(Code::LineSpacing(200))];
        let b = back(&d);
        assert_eq!(b.els, text("x"));
        assert_eq!(b.initial_codes, d.initial_codes);
        assert_eq!(b.init, d.init);
    }

    #[test]
    fn characters() {
        // no WP character: base letter, or ?; a letter and a combining
        // accent with no WP character of their own: overstrike
        let b = back(&doc(text("ő\u{1ee5}\u{4e2d}q\u{303}x\u{2011}y\u{2009}z")));
        assert_eq!(b.plain_text(0, usize::MAX), "ő??q\u{303}x-y z");
        let (_, lost) = report(&doc(text("\u{4e2d}\u{4e2d}")));
        assert_eq!(lost, ["characters (written as ?) \u{d7}2"]);
        assert!(report(&doc(text("ä"))).1.is_empty());
    }

    /// A table with joined cells, cell justification and attributes,
    /// edited lines and shading.
    fn table() -> Document {
        use LineStyle as L;
        let mut els = text("vor");
        els.extend([c(Code::HRt), c(Code::TblDef { cols: vec![1800, 2400, 3000] }), c(Code::TblOpt(TblOpt { pos: TblPos::Center, header_rows: 1, gray: 20, ..Default::default() }))]);
        // row 1: a cell joined over two columns, then one with its own lines and shading
        els.extend([c(Code::Row), c(Code::Cell), c(Code::Join { cols: 2, rows: 1 }), c(Code::Just(Justify::Center)), c(Code::AttrOn(Attr::Bold))]);
        els.extend(text("Kopf"));
        els.extend([c(Code::AttrOff(Attr::Bold)), c(Code::Cell), c(Code::Cell), c(Code::CellFmt(CellFmt { lines: [None, Some(L::Thick), Some(L::Dashed), None], shade: true })), c(Code::Just(Justify::Left))]);
        els.extend(text("B1"));
        // row 2: a cell joined over two rows; a double line below the middle cell
        els.extend([c(Code::Row), c(Code::Cell), c(Code::Join { cols: 1, rows: 2 })]);
        els.extend(text("A2"));
        els.extend([c(Code::HRt)]);
        els.extend(text("zwei"));
        els.extend([c(Code::Cell), c(Code::CellFmt(CellFmt { lines: [None, None, None, Some(L::Double)], shade: false })), c(Code::Just(Justify::Right))]);
        els.extend(text("3,50"));
        els.extend([c(Code::Cell), c(Code::Just(Justify::Left))]);
        // row 3: no lines at all around the last cell
        els.extend([c(Code::Row), c(Code::Cell), c(Code::Cell)]);
        els.extend(text("B3"));
        els.extend([c(Code::Cell), c(Code::CellFmt(CellFmt { lines: [Some(L::None); 4], shade: false })), c(Code::TblOff), c(Code::Just(Justify::Full)), c(Code::HRt)]);
        els.extend(text("nach"));
        doc(els)
    }

    #[test]
    fn table_with_join_and_lines() {
        let d = table();
        let f = write(&d);
        let b = back(&d);
        for (k, (x, y)) in b.els.iter().zip(&d.els).enumerate() {
            assert_eq!(x, y, "element {k}");
        }
        assert_eq!(b.els.len(), d.els.len());
        // the first row code: both blocks are the row's own lines (flags,
        // height, 0x80 + length, two bytes a column), then its top
        let at = f.windows(4).position(|w| w[..2] == [0xDC, 1] && w[3] == 0).expect("row code");
        let n = f[at + 2] as usize;
        let blk = [0x03, 0x81, 0x01, 0x86, 0x12, 0x00, 0x02, 0x00, 0x0B, 0x68];
        assert_eq!(f[at + 4..at + n], [&blk[..], &blk[..], &[0, 0][..]].concat());
    }

    #[test]
    fn unknown_codes_are_written_back() {
        // real 5.1 functions the reader keeps raw: a footnote option
        // code, a reserved fixed-length function and an old merge code
        let mut opt = vec![0xD2, 0x03, 160, 0];
        opt.extend((0..156).map(|k| (k * 7 % 251) as u8));
        opt.extend_from_slice(&[160, 0, 0x03, 0xD2]);
        let fixed = vec![0xC8, 0x12, 0x34, 0xC8];
        let raw = [b"ab".to_vec(), opt.clone(), b"cd".to_vec(), fixed.clone(), vec![0x12], b"e".to_vec()].concat();
        let mut file = vec![0xFF, b'W', b'P', b'C', 16, 0, 0, 0, 1, 10, 0, 1, 0, 0, 0, 0];
        file.extend_from_slice(&raw);
        let d = read(&file).unwrap();
        assert!(d.els.contains(&c(Code::Unknown { name: "Ftn Opt".into(), data: opt })), "{:?}", d.els);
        let f = write(&d);
        assert!(f.ends_with(&raw), "written back byte for byte");
        assert_eq!(back(&d).els, d.els);
        // codes of other formats are not put into the file
        let (f, lost) = report(&doc(vec![c(Code::Unknown { name: "x".into(), data: vec![0xD2, 0x03, 9, 0, 1] }), El::Ch('y')]));
        assert!(f.ends_with(b"y") && !f.ends_with(&[1, b'y']) && lost == ["codes of other file formats"]);
    }

    /// Real 5.x files written and read again give the same codes.
    #[test]
    fn real_files_round_trip() {
        let dirs = ["/mnt/i/Claude Space/wp51", "I:\\Claude Space\\wp51"];
        let dir = dirs.into_iter().map(std::path::Path::new).find(|p| p.is_dir()).unwrap_or(std::path::Path::new("."));
        for name in ["PRINTER.TST", "KEYBRDS.WP", "KERN.TST", "JOB", "WP.LRS", "STANDARD.CRS", "STYLE/SUB", "STYLE/T", "test-out/issue2b/TABLE.WP5"] {
            let Ok(bytes) = std::fs::read(dir.join(name)).or_else(|_| std::fs::read(name)) else { continue };
            let a = read(&bytes).unwrap();
            let b = read(&write(&a)).unwrap();
            let skip = matches!(b.els.first(), Some(El::Code(Code::Fmt(Fmt::Paper(_))))) as usize;
            for (k, (x, y)) in a.els.iter().zip(&b.els[skip..]).enumerate() {
                assert_eq!(x, y, "{name}: element {k}\n{:?}\n{:?}", &a.els[k.saturating_sub(6)..(k + 4).min(a.els.len())], &b.els[(k + skip).saturating_sub(6)..(k + skip + 4).min(b.els.len())]);
            }
            assert_eq!(a.els.len() + skip, b.els.len(), "{name}");
            assert_eq!(a.init.font, b.init.font, "{name}");
        }
    }

    /// Writes sample files for a look with other programs:
    /// `cargo test --release wp5w_samples -- --ignored`.
    #[test]
    #[ignore]
    fn wp5w_samples() {
        let out = std::path::Path::new("test-out/wp5w");
        std::fs::create_dir_all(out).unwrap();
        std::fs::write(out.join("rich.wp5"), write(&rich())).unwrap();
        std::fs::write(out.join("table.wp5"), write(&table())).unwrap();
        // document initial codes and a summary
        let mut d = doc(text("Initial codes: margins 0.5 and 2 inches, left justified."));
        d.initial_codes = vec![c(Code::LRMargin { left: 600, right: 2400 }), c(Code::Just(Justify::Left))];
        (d.init.left, d.init.right, d.init.justify) = (600, 2400, Justify::Left);
        d.summary = Summary { descriptive_name: "Ein Name".into(), subject: "Betreff".into(), author: "Autorin".into(), typist: "T".into(), comments: "Ü".into() };
        std::fs::write(out.join("initial.wp5"), write(&d)).unwrap();
        for (name, to) in [("I:\\Claude Space\\wp51\\PRINTER.TST", "printer.wp5"), ("I:\\Claude Space\\wp51\\KEYBRDS.WP", "keybrds.wp5"), ("test-out/issue2b/TABLE.WP5", "table5.wp5")] {
            if let Ok(b) = std::fs::read(name) {
                std::fs::write(out.join(to), write(&read(&b).unwrap())).unwrap();
            }
        }
    }
}
