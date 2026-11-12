//! WordPerfect 6/7/8/9/10/11/12/X3–X9 document reader (`FF WPC`, file
//! type 0x0A, major version 2).
//!
//! Layout: a 16-byte prefix (document pointer at 4, encryption key at 12,
//! index header pointer at 14), the index of *prefix packets* (14-byte
//! entries: flags, type, use count, hide count, data size, data offset),
//! then the document text.  Text bytes: 0x01–0x20 frequent international
//! characters, 0x21–0x7F ASCII, 0x80–0xCF single-byte functions (spaces,
//! hyphens, the end-of-line / page / table codes), 0xD0–0xEF variable
//! length groups (`[grp][sub][size u16][flags]([n][pid u16]*n)[nd u16]…
//! [size u16][grp]`), 0xF0–0xFE fixed-length groups (extended character,
//! undo, attribute on/off, …).  Footnotes, endnotes, headers and footers
//! refer (by 1-based prefix id) to "General WordPerfect Text" packets that
//! hold their text in the same encoding.
//!
//! Written from the WordPerfect 6 file format description; libwpd was used
//! as a reference for format facts.  Every read is bounds-checked: damaged
//! files give an error or partial text, never a panic.

use crate::doc::*;
use crate::fmt_build::Builder;

const PT_GENERAL_TEXT: u8 = 0x08;
const PT_INITIAL_FONT: u8 = 0x25;
const PT_FONT_DESCRIPTOR: u8 = 0x55;

/// Characters 0x01–0x20 of the text stream.
const EXT_INTL: [char; 32] = [
    'å', 'Å', 'æ', 'Æ', 'ä', 'Ä', 'á', 'à', 'â', 'ã', 'Ã', 'ç', 'Ç', 'ë', 'é', 'É', 'è', 'ê', 'í', 'ñ', 'Ñ', 'ø', 'Ø', 'õ', 'Õ', 'ö', 'Ö', 'ü', 'Ü', 'ú', 'ù', 'ß',
];

/// Total size of the fixed-length groups 0xF0..=0xFE.
const FIXED_SIZE: [usize; 15] = [4, 5, 3, 3, 3, 3, 4, 4, 4, 5, 5, 6, 6, 8, 8];

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o.checked_add(1)?)?]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    let s = b.get(o..o.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

struct Packet<'a> {
    ptype: u8,
    data: &'a [u8],
}

struct Ctx<'a> {
    /// prefix packets, index 0 = prefix id 1
    packets: Vec<Packet<'a>>,
}

impl<'a> Ctx<'a> {
    fn packet(&self, pid: u16) -> Option<&Packet<'a>> {
        self.packets.get((pid as usize).checked_sub(1)?)
    }

    /// Text of a General WordPerfect Text packet.
    fn general_text(&self, pid: u16) -> Option<Vec<u8>> {
        let p = self.packet(pid)?;
        if p.ptype != PT_GENERAL_TEXT {
            return None;
        }
        let d = p.data;
        let n = u16_at(d, 0)? as usize;
        let mut off = 6;
        let mut sizes = vec![];
        for _ in 0..n.min(d.len() / 4) {
            sizes.push(u32_at(d, off)? as usize);
            off += 4;
        }
        let mut out = vec![];
        for s in sizes {
            let chunk = d.get(off..off.saturating_add(s).min(d.len()))?;
            out.extend_from_slice(chunk);
            off = off.saturating_add(s);
        }
        Some(out)
    }

    /// Font name from a font descriptor packet.
    fn font_name(&self, pid: u16) -> Option<String> {
        let p = self.packet(pid)?;
        if p.ptype != PT_FONT_DESCRIPTOR {
            return None;
        }
        font_descriptor_name(p.data)
    }
}

/// Name in a font descriptor (22 bytes of metrics, name length, then
/// WP characters as (char, set) words).
fn font_descriptor_name(d: &[u8]) -> Option<String> {
    let len = u16_at(d, 22)? as usize;
    let mut s = String::new();
    for k in 0..len / 2 {
        let Some(w) = u16_at(d, 24 + 2 * k) else { break };
        if w == 0 {
            break;
        }
        let (ch, set) = ((w & 0xFF) as u8, (w >> 8) as u8);
        if set == 0 {
            if (0x20..0x7F).contains(&ch) {
                s.push(ch as char);
            }
        } else {
            s.push_str(&crate::wpchars::wp_to_string(set, ch as u16));
        }
    }
    Some(clean_font_name(&s))
}

/// "Times New Roman Regular", "Arial-WP", "Times New Roman Standaard" →
/// the family name.
fn clean_font_name(s: &str) -> String {
    let mut n = s.replace("-WP", "").trim().to_string();
    loop {
        let before = n.len();
        for suf in [" Regular", " Standaard", " Standard", " Normal", " Bold", " Italic", " Oblique", " Fett", " Kursiv", " Roman"] {
            if n.ends_with(suf) && !(suf == " Roman" && n.ends_with("New Roman")) {
                n.truncate(n.len() - suf.len());
                n = n.trim_end().to_string();
            }
        }
        if n.len() == before {
            break;
        }
    }
    n.trim_end_matches('-').trim().to_string()
}

fn attr_of(n: u8) -> Option<Attr> {
    Some(match n {
        0 => Attr::ExtraLarge,
        1 => Attr::VeryLarge,
        2 => Attr::Large,
        3 => Attr::Small,
        4 => Attr::Fine,
        5 => Attr::Superscript,
        6 => Attr::Subscript,
        7 => Attr::Outline,
        8 => Attr::Italic,
        9 => Attr::Shadow,
        10 => Attr::Redline,
        11 => Attr::DblUnderline,
        12 => Attr::Bold,
        13 => Attr::Strikeout,
        14 => Attr::Underline,
        15 => Attr::SmallCaps,
        _ => return None,
    })
}

/// Formatting state of the main text that may still go into the
/// document's initial format (before any text).
struct Fmt {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    page: Option<(i32, i32)>,
    line_spacing: Option<u32>,
    init_lr: Option<(i32, i32)>,
    init_tb: Option<(i32, i32)>,
}

struct Parser<'c, 'a> {
    ctx: &'c Ctx<'a>,
    depth: u32,
    undo: bool,
    /// column widths of a table definition not yet started
    tbl_pending: Option<Vec<i32>>,
    fmt: Fmt,
    /// is this the main text (initial format may be set)?
    main: bool,
    span_extra: usize,
}

/// Has any text (or table) been written yet?
fn started(b: &Builder) -> bool {
    b.els.iter().any(|e| matches!(e, El::Ch(_) | El::Code(Code::TblDef { .. } | Code::Tab | Code::HRt | Code::HPg)))
}

impl Parser<'_, '_> {
    fn sub_text(&self, pid: u16, font: &FontSpec) -> Vec<El> {
        let Some(t) = self.ctx.general_text(pid) else { return vec![] };
        if self.depth >= 4 {
            return vec![];
        }
        let mut b = Builder::new(font.clone());
        b.font = font.clone();
        let mut p = Parser { ctx: self.ctx, depth: self.depth + 1, undo: false, tbl_pending: None, fmt: Fmt { left: 0, right: 0, top: 0, bottom: 0, page: None, line_spacing: None, init_lr: None, init_tb: None }, main: false, span_extra: 0 };
        p.run(&t, &mut b);
        b.finish()
    }

    fn start_row(&mut self, b: &mut Builder) {
        self.flush_span(b);
        if !b.in_table() {
            let cols = self.tbl_pending.take().unwrap_or_default();
            b.table_start(cols);
        }
        b.row();
        b.cell();
    }

    fn run(&mut self, d: &[u8], b: &mut Builder) {
        let mut i = 0;
        while i < d.len() {
            let c = d[i];
            match c {
                0x00 => i += 1,
                0x01..=0x20 => {
                    if !self.undo {
                        b.ch(EXT_INTL[(c - 1) as usize]);
                    }
                    i += 1;
                }
                0x21..=0x7F => {
                    if !self.undo {
                        b.ch(c as char);
                    }
                    i += 1;
                }
                0x80..=0xCF => {
                    if !self.undo {
                        self.single(c, b);
                    }
                    i += 1;
                }
                0xD0..=0xEF => {
                    let Some(size) = u16_at(d, i + 2).map(|s| s as usize) else { break };
                    let ok = size >= 8 && i + size <= d.len() && u16_at(d, i + size - 3) == Some(size as u16) && d[i + size - 1] == c;
                    if !ok {
                        i += 1;
                        continue;
                    }
                    let g = &d[i..i + size];
                    if !self.undo {
                        self.variable(g, b);
                    }
                    i += size;
                }
                0xF0..=0xFE => {
                    let size = FIXED_SIZE[(c - 0xF0) as usize];
                    if i + size > d.len() || d[i + size - 1] != c {
                        i += 1;
                        continue;
                    }
                    let g = &d[i..i + size];
                    match c {
                        0xF0 if !self.undo => {
                            let (ch, set) = (g[1], g[2]);
                            if set == 0 {
                                if (0x20..0x7F).contains(&ch) {
                                    b.ch(ch as char);
                                }
                            } else {
                                b.text(&crate::wpchars::wp_to_string(set, ch as u16));
                            }
                        }
                        0xF1 => match g[1] {
                            0 => self.undo = true,
                            1 => self.undo = false,
                            _ => {}
                        },
                        0xF2 | 0xF3 if !self.undo => {
                            if let Some(a) = attr_of(g[1]) {
                                b.set(a, c == 0xF2);
                            }
                        }
                        _ => {}
                    }
                    i += size;
                }
                0xFF => i += 1,
            }
        }
    }

    fn single(&mut self, c: u8, b: &mut Builder) {
        match c {
            0x80 | 0xCD | 0xCE | 0xCF => b.ch(' '),
            0x81 => b.ch('\u{a0}'),
            0x82 | 0x83 => b.ch('\u{ad}'),
            0x84 => b.ch('-'),
            0x87 | 0xB7 | 0xB8 | 0xB9 | 0xCA | 0xCB | 0xCC => b.para(),
            0xB5 | 0xB6 | 0xC8 | 0xC9 => b.para(),
            0xB4 | 0xC7 => b.page(),
            0xBD | 0xBE | 0xBF => {
                self.flush_span(b);
                b.table_end();
                self.tbl_pending = None;
            }
            0xC0..=0xC5 => self.start_row(b),
            0xC6 => {
                self.flush_span(b);
                if b.in_table() {
                    b.cell();
                } else {
                    self.start_row(b);
                }
            }
            _ => {}
        }
    }

    /// One variable-length group (`g` = the whole group, consistency
    /// already checked).
    fn variable(&mut self, g: &[u8], b: &mut Builder) {
        let (grp, sub, flags) = (g[0], g[1], g[4]);
        let end = g.len() - 3; // start of the closing size word
        let mut p = 5;
        let mut pids: Vec<u16> = vec![];
        if flags & 0x80 != 0 {
            let n = g.get(p).copied().unwrap_or(0) as usize;
            p += 1;
            for _ in 0..n {
                let Some(v) = u16_at(g, p) else { return };
                pids.push(v);
                p += 2;
            }
        }
        let Some(nd) = u16_at(g, p).map(|v| v as usize) else { return };
        p += 2;
        if p > end {
            return;
        }
        let body = &g[p..end];
        let w = |o: usize| u16_at(body, o);
        match grp {
            0xD0 => self.eol(sub, &body[..nd.min(body.len())], b),
            0xD1 => match sub {
                0 | 1 => {
                    if let Some(m) = w(0) {
                        let m = m as i32;
                        if sub == 0 { self.fmt.top = m } else { self.fmt.bottom = m }
                        if self.main && started(b) {
                            b.code_before_text(Code::TBMargin { top: self.fmt.top, bottom: self.fmt.bottom });
                        } else if self.main {
                            self.fmt.init_tb = Some((self.fmt.top, self.fmt.bottom));
                        }
                    }
                }
                0x11 => {
                    if let (Some(h), Some(wd)) = (w(3), w(5)) {
                        if h > 1200 && wd > 1200 && self.main && !started(b) {
                            self.fmt.page = Some((wd as i32, h as i32));
                        }
                    }
                }
                _ => {}
            },
            0xD2 => {
                if sub <= 1 {
                    if let Some(m) = w(0) {
                        let m = m as i32;
                        if sub == 0 { self.fmt.left = m } else { self.fmt.right = m }
                        if self.main && started(b) {
                            b.code_before_text(Code::LRMargin { left: self.fmt.left, right: self.fmt.right });
                        } else if self.main {
                            self.fmt.init_lr = Some((self.fmt.left, self.fmt.right));
                        }
                    }
                }
            }
            0xD3 => match sub {
                0x01 => {
                    if let Some(v) = u32_at(body, 0) {
                        let ls = ((v as f64) / 65536.0 * 100.0).round() as u32;
                        if (50..=1000).contains(&ls) {
                            if self.main && !started(b) {
                                self.fmt.line_spacing = Some(ls);
                            } else {
                                b.code_before_text(Code::LineSpacing(ls));
                            }
                        }
                    }
                }
                0x05 => {
                    if let Some(&j) = body.first() {
                        b.just = match j {
                            1 | 4 => Justify::Full,
                            2 => Justify::Center,
                            3 => Justify::Right,
                            _ => Justify::Left,
                        };
                    }
                }
                _ => {}
            },
            0xD4 => match sub {
                0x1A => {
                    let size = w(6).filter(|s| *s > 0 && *s < 30000);
                    let name = pids.first().and_then(|&pid| self.ctx.font_name(pid)).filter(|n| !n.is_empty());
                    if let Some(n) = name {
                        b.font.name = n;
                    }
                    if let Some(s) = size {
                        b.font.size_cpt = s as u32 * 2;
                    }
                }
                0x1B => {
                    if let Some(s) = w(0).filter(|s| *s > 0 && *s < 30000) {
                        b.font.size_cpt = s as u32 * 2;
                    }
                }
                0x2A => {
                    if b.in_table() {
                        b.table_end();
                    }
                    self.tbl_pending = Some(vec![]);
                }
                0x2C => {
                    if let (Some(cols), Some(wd)) = (self.tbl_pending.as_mut(), w(1)) {
                        if cols.len() < 64 {
                            cols.push(wd as i32);
                        }
                    }
                }
                _ => {}
            },
            0xD6 if sub <= 3 => {
                let occ = match body.first().copied().unwrap_or(0) & 3 {
                    0 => Occurrence::Discontinue,
                    1 => Occurrence::Odd,
                    2 => Occurrence::Even,
                    _ => Occurrence::Every,
                };
                if occ != Occurrence::Discontinue {
                    if let Some(&pid) = pids.first() {
                        let els = self.sub_text(pid, &b.font.clone());
                        b.head_foot(sub, occ, els);
                    }
                }
            }
            0xD7 if sub == 0 || sub == 2 => {
                if let Some(&pid) = pids.first() {
                    let els = self.sub_text(pid, &b.font.clone());
                    b.note(if sub == 0 { NoteKind::Foot } else { NoteKind::End }, els);
                }
            }
            0xE0 => {
                // tab group: the type is in the upper five bits
                if flags & 0x40 != 0 {
                    return; // "ignore function"
                }
                match sub >> 3 {
                    0x00 => b.code(Code::MarRel),
                    0x06 => b.code(Code::Indent),
                    0x07 => b.code(Code::DblIndent),
                    0x08 | 0x09 => b.code(Code::Center),
                    0x10 => b.code(Code::FlushRight),
                    _ => b.code_fmt(Code::Tab),
                }
            }
            _ => {}
        }
    }

    /// End-of-line group (returns, page breaks, table structure).
    fn eol(&mut self, sub: u8, nd: &[u8], b: &mut Builder) {
        match sub {
            0x01..=0x03 => b.ch(' '),
            0x04..=0x08 | 0x17..=0x1C => b.para(),
            0x09 => b.page(),
            0x0A => {
                self.flush_span(b);
                if b.in_table() {
                    b.cell();
                } else {
                    self.start_row(b);
                }
                self.span_cells(nd, b);
            }
            0x0B..=0x10 => {
                self.start_row(b);
                self.span_cells(nd, b);
            }
            0x11..=0x13 => {
                self.flush_span(b);
                b.table_end();
                self.tbl_pending = None;
            }
            _ => {}
        }
    }

    /// A cell spanning several columns: add empty cells so the table stays
    /// rectangular.
    fn span_cells(&mut self, nd: &[u8], b: &mut Builder) {
        let Some(sd) = u16_at(nd, 0) else { return };
        let mut q = 2 + sd as usize;
        let mut guard = 0;
        while q < nd.len() && guard < 64 {
            guard += 1;
            let f = nd[q];
            let size = match f {
                0x80 => 5,
                0x81 | 0x8E | 0x8F => u16_at(nd, q + 1).map(|v| v as usize).unwrap_or(0),
                0x82 | 0x83 | 0x85 => 4,
                0x84 => 9,
                0x86 => 10,
                0x87 | 0x88 => 6,
                0x89 => 11,
                0x8B | 0x8C => 3,
                0x8D => 1,
                _ => 0,
            };
            if size == 0 {
                return;
            }
            if f == 0x85 {
                let cs = nd.get(q + 1).copied().unwrap_or(1);
                if (2..64).contains(&cs) {
                    // ensure the spanned cell's text is not split: the
                    // extra cells come after it, so remember them
                    self.pending_span(cs as usize - 1, b);
                }
            }
            q += size;
        }
    }

    fn pending_span(&mut self, extra: usize, _b: &mut Builder) {
        // WP stores no cells for the covered columns: add them after this
        // cell's content (before the next cell / row / table end).
        self.span_extra += extra;
    }

    fn flush_span(&mut self, b: &mut Builder) {
        for _ in 0..std::mem::take(&mut self.span_extra) {
            if b.in_table() {
                b.cell();
            }
        }
    }
}

/// Parse the prefix packet index.
fn packets(bytes: &[u8]) -> Vec<Packet<'_>> {
    let mut out = vec![];
    let ih = (u16_at(bytes, 14).unwrap_or(16) as usize).max(16);
    let Some(n) = u16_at(bytes, ih + 2) else { return out };
    let n = (n as usize).min(bytes.len() / 14 + 1);
    for k in 1..n {
        let e = ih + 14 * k;
        let (Some(&flags), Some(&ptype), Some(size), Some(off)) = (bytes.get(e), bytes.get(e + 1), u32_at(bytes, e + 6), u32_at(bytes, e + 10)) else { break };
        let _ = flags;
        let (s, o) = (size as usize, off as usize);
        let data = if o < bytes.len() { &bytes[o..o.saturating_add(s).min(bytes.len())] } else { &[][..] };
        out.push(Packet { ptype, data });
    }
    out
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if bytes.len() < 16 || !bytes.starts_with(b"\xFFWPC") {
        return Err("Not a WordPerfect document".into());
    }
    if bytes[9] != 0x0A || bytes[10] < 2 {
        return Err("Not a WordPerfect 6 or later document".into());
    }
    if u16_at(bytes, 12).unwrap_or(0) != 0 {
        return Err("Document is password protected".into());
    }
    let doc_ptr = u32_at(bytes, 4).unwrap_or(0) as usize;
    if doc_ptr < 16 || doc_ptr > bytes.len() {
        return Err("damaged WordPerfect document (no text)".into());
    }
    let ctx = Ctx { packets: packets(bytes) };

    // initial font
    let mut base = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    if let Some(p) = ctx.packets.iter().find(|p| p.ptype == PT_INITIAL_FONT) {
        if let Some(name) = u16_at(p.data, 2).and_then(|pid| ctx.font_name(pid)).filter(|n| !n.is_empty()) {
            base.name = name;
        }
        if let Some(s) = u16_at(p.data, 4).filter(|s| *s > 100 && *s < 30000) {
            base.size_cpt = s as u32 * 2;
        }
    }

    let mut b = Builder::new(base.clone());
    let mut p = Parser {
        ctx: &ctx,
        depth: 0,
        undo: false,
        tbl_pending: None,
        fmt: Fmt { left: 1200, right: 1200, top: 1200, bottom: 1200, page: None, line_spacing: None, init_lr: None, init_tb: None },
        main: true,
        span_extra: 0,
    };
    p.run(&bytes[doc_ptr..], &mut b);
    p.flush_span(&mut b);
    let mut d = b.into_document(base);
    let f = &p.fmt;
    let ok = |v: i32| (0..12000).contains(&v);
    if let Some((l, r)) = f.init_lr.filter(|(l, r)| ok(*l) && ok(*r)) {
        d.init.left = l;
        d.init.right = r;
    }
    if let Some((t, bm)) = f.init_tb.filter(|(t, b)| ok(*t) && ok(*b)) {
        d.init.top = t;
        d.init.bottom = bm;
    }
    if let Some((w, h)) = f.page {
        d.init.page_width = w;
        d.init.page_height = h;
    }
    if let Some(ls) = f.line_spacing {
        d.init.line_spacing = ls;
    }
    Ok(d)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    /// A variable-length group.
    fn vg(grp: u8, sub: u8, pids: &[u16], nd: &[u8]) -> Vec<u8> {
        let mut body = vec![];
        let flags = if pids.is_empty() { 0 } else { 0x80 };
        body.push(flags);
        if !pids.is_empty() {
            body.push(pids.len() as u8);
            for p in pids {
                body.extend_from_slice(&p.to_le_bytes());
            }
        }
        body.extend_from_slice(&(nd.len() as u16).to_le_bytes());
        body.extend_from_slice(nd);
        let size = (1 + 1 + 2 + body.len() + 2 + 1) as u16;
        let mut g = vec![grp, sub];
        g.extend_from_slice(&size.to_le_bytes());
        g.extend(body);
        g.extend_from_slice(&size.to_le_bytes());
        g.push(grp);
        g
    }
    fn eol(sub: u8) -> Vec<u8> {
        vg(0xD0, sub, &[], &[0, 0])
    }
    fn ext(set: u8, ch: u8) -> Vec<u8> {
        vec![0xF0, ch, set, 0xF0]
    }
    fn general_text(t: &[u8]) -> Vec<u8> {
        let mut d = vec![1, 0, 10, 0, 0, 0];
        d.extend_from_slice(&(t.len() as u32).to_le_bytes());
        d.extend_from_slice(t);
        d
    }
    fn font_desc(name: &str) -> Vec<u8> {
        let mut d = vec![0u8; 22];
        d.extend_from_slice(&((name.len() * 2 + 2) as u16).to_le_bytes());
        for c in name.bytes() {
            d.extend_from_slice(&(c as u16).to_le_bytes());
        }
        d.extend_from_slice(&[0, 0]);
        d
    }
    fn text(s: &str) -> Vec<u8> {
        s.bytes().map(|c| if c == b' ' { 0x80 } else { c }).collect()
    }

    /// Build a WP6 file from (type, data) packets and the text stream.
    pub(crate) fn build(packets: &[(u8, Vec<u8>)], body: &[u8]) -> Vec<u8> {
        let ih = 16usize;
        let idx_len = 14 * (packets.len() + 1);
        let mut data_off = ih + idx_len;
        let mut f = b"\xFFWPC".to_vec();
        let doc_ptr = data_off + packets.iter().map(|p| p.1.len()).sum::<usize>();
        f.extend_from_slice(&(doc_ptr as u32).to_le_bytes());
        f.extend_from_slice(&[1, 0x0A, 2, 1, 0, 0]);
        f.extend_from_slice(&(ih as u16).to_le_bytes());
        // index header
        f.extend_from_slice(&[2, 0]);
        f.extend_from_slice(&((packets.len() + 1) as u16).to_le_bytes());
        f.extend_from_slice(&[0; 10]);
        for (t, d) in packets {
            f.extend_from_slice(&[0, *t, 1, 0, 0, 0]);
            f.extend_from_slice(&(d.len() as u32).to_le_bytes());
            f.extend_from_slice(&(data_off as u32).to_le_bytes());
            data_off += d.len();
        }
        for (_, d) in packets {
            f.extend_from_slice(d);
        }
        assert_eq!(f.len(), doc_ptr);
        f.extend_from_slice(body);
        f
    }

    pub(crate) fn sample() -> Vec<u8> {
        let foot: Vec<u8> = [&b"Fu"[..], &ext(1, 23), &text("note")].concat();
        let packets = vec![
            (PT_FONT_DESCRIPTOR, font_desc("Arial Regular")),
            (PT_GENERAL_TEXT, general_text(&foot)),
            (PT_GENERAL_TEXT, general_text(&text("Kopfzeile"))),
            (PT_INITIAL_FONT, vec![1, 0, 1, 0, 0x58, 0x02]),
        ];
        let mut t = vec![];
        t.extend(vg(0xD2, 0, &[], &1800u16.to_le_bytes()));
        t.extend(vg(0xD6, 0, &[3], &[3]));
        t.extend(b"Gr");
        t.push(0x1C); // ü
        t.extend(ext(1, 23)); // ß
        t.push(b'e');
        t.push(0x80);
        t.extend([0xF2, 12, 0xF2]);
        t.extend(b"fett");
        t.extend([0xF3, 12, 0xF3]);
        t.push(0x80);
        t.extend([0xF2, 8, 0xF2]);
        t.extend(b"kursiv");
        t.extend([0xF3, 8, 0xF3]);
        t.extend(vg(0xD7, 0, &[2], &[]));
        // deleted text (undo) must not appear
        t.extend([0xF1, 0, 1, 0, 0xF1]);
        t.extend(b"WEG");
        t.extend([0xF1, 1, 1, 0, 0xF1]);
        t.extend(eol(4));
        for ch in [32u8, 35, 19, 5, 11, 39] {
            t.extend(ext(10, ch)); // Привет
        }
        t.extend(eol(4));
        // table: definition, two columns, rows
        t.extend(vg(0xD4, 0x2A, &[], &[0, 0, 0, 0]));
        for _ in 0..2 {
            t.extend(vg(0xD4, 0x2C, &[], &[0, 0xB8, 0x0B, 0, 0, 0, 0]));
        }
        t.extend(vg(0xD4, 0x2B, &[], &[]));
        t.extend(eol(0x0B));
        t.extend(b"A1");
        t.extend(eol(0x0A));
        t.extend(b"B1");
        t.extend(eol(0x0B));
        t.extend(b"A2");
        t.push(0xC6); // single-byte cell
        t.extend(b"B2");
        t.extend(eol(0x11));
        t.extend(vg(0xD3, 0x05, &[], &[2]));
        t.extend(text("Ende"));
        t.extend(eol(9));
        t.extend(b"Seite2");
        build(&packets, &t)
    }

    #[test]
    fn synthetic_document() {
        let bytes = sample();
        let d = read(&bytes).unwrap();
        assert_eq!(
            debug_text(&d.els),
            "[Header:Kopfzeile]Grüße fett kursiv[Footnote:Fußnote]\nПривет\n[Tbl]\n[Row]|A1|B1\n[Row]|A2|B2[TblOff]Ende\n[HPg]\nSeite2"
        );
        assert_eq!(d.init.font, FontSpec { name: "Arial".into(), size_cpt: 1200 });
        assert_eq!(d.init.left, 1800);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Center))));
        let at = d.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        assert!(matches!(&d.els[at], El::Code(Code::TblDef { cols }) if cols == &vec![3000, 3000]));
    }

    #[test]
    fn sample_file() {
        let bytes = std::fs::read("tests/data/formats/WP6.wpd").unwrap();
        let d = read(&bytes).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Foo") && t.contains("foo"), "{t}");
        assert!(t.contains("[Header:"), "{t}");
    }

    #[test]
    fn saved_sample_matches() {
        // tests/data/formats/sample_wp6.wpd is the synthetic document
        let path = "tests/data/formats/sample_wp6.wpd";
        if std::env::var("WRITE_SAMPLES").is_ok() {
            std::fs::write(path, sample()).unwrap();
        }
        let saved = std::fs::read(path).unwrap();
        assert_eq!(saved, sample());
        assert_eq!(crate::fileio::sniff(&saved), crate::fileio::Kind::Wp6);
    }

    #[test]
    fn encrypted() {
        let mut b = sample();
        b[12] = 0x34;
        assert!(read(&b).unwrap_err().contains("password"));
    }

    #[test]
    fn never_panics_on_damaged_files() {
        for bytes in [sample(), std::fs::read("tests/data/formats/WP6.wpd").unwrap()] {
            for n in 0..bytes.len() {
                let _ = read(&bytes[..n]);
            }
            let mut seed = 12345u32;
            for _ in 0..3000 {
                let mut b = bytes.clone();
                for _ in 0..4 {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    let k = (seed >> 8) as usize % b.len();
                    b[k] = (seed >> 3) as u8;
                }
                let _ = read(&b);
            }
        }
    }
}
