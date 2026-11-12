//! Microsoft Works word processor (.wps) reader.
//!
//! * Works 3/4 for Windows (and DOS Works 2/3 converted): OLE compound
//!   file with an "MN0" stream — a Write-like layout: 8-bit text from
//!   0x100 (header, footer, main zone), 128-byte formatting pages (FDPs)
//!   for character and paragraph runs, a font table, footnotes.
//! * Works 5 (2000) – 9: OLE compound file with a "CONTENTS" stream that
//!   starts with "CHNKINK"/"CHNKWKS": an index of named chunks; the text
//!   is UTF-16 ("TEXT"), split into zones (main, notes, header/footer) by
//!   "STRS"; character/paragraph runs are "FDPC"/"FDPP" pages whose
//!   properties are small tagged structures; notes are in "FTN "/"EDN ".
//!
//! Written from the format facts documented by the libwps project (no code
//! taken).  Everything is bounds-checked; damaged parts are skipped.

use crate::doc::*;
use crate::fmt_build::Builder;
use crate::msdoc::Cfb;
use crate::rtf::decode_byte;
use std::collections::HashMap;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o.checked_add(1)?)?]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    let s = b.get(o..o.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn slice(b: &[u8], o: usize, len: usize) -> &[u8] {
    let s = o.min(b.len());
    let e = o.saturating_add(len).min(b.len());
    &b[s..e]
}

/// Is this OLE compound file a Works document?
pub fn probe(bytes: &[u8]) -> bool {
    let Ok(cfb) = Cfb::parse(bytes) else { return false };
    if cfb.stream("MN0").is_some() {
        return true;
    }
    cfb.stream("CONTENTS").is_some_and(|c| c.starts_with(b"CHNKWKS") || c.starts_with(b"CHNKINK"))
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let cfb = Cfb::parse(bytes)?;
    if let Some(mn0) = cfb.stream("MN0") {
        if cfb.stream("MM").is_some_and(|m| m.starts_with(&[0x44, 0x4E])) {
            return Err("Works for Macintosh documents are not supported".into());
        }
        if u16_at(&mn0, 0) == Some(0x54FF) {
            return Err("this is a Works database, not a word processor document".into());
        }
        return read_v4(&mn0);
    }
    if let Some(c) = cfb.stream("CONTENTS") {
        if c.starts_with(b"CHNKWKS") || c.starts_with(b"CHNKINK") {
            return read_v8(&c);
        }
    }
    Err("not a Microsoft Works word processor document".into())
}

#[derive(Clone, Default, PartialEq)]
struct Chp {
    attrs: AttrSet,
    font: Option<String>,
    size: Option<u32>,
    caps: bool,
    /// footnote mark / field (Works 4: "special"; Works 8: special type)
    special: i32,
}

#[derive(Clone, Copy, Default)]
struct Pap {
    jc: Option<Justify>,
}

struct Run<T> {
    s: usize,
    e: usize,
    v: T,
}

fn find<T: Clone + Default>(runs: &[Run<T>], pos: usize) -> T {
    let i = runs.partition_point(|r| r.e <= pos);
    runs.get(i).filter(|r| r.s <= pos).map(|r| r.v.clone()).unwrap_or_default()
}

fn apply_chp(b: &mut Builder, c: &Chp, base: &FontSpec) {
    b.attrs = c.attrs;
    b.font = FontSpec { name: c.font.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| base.name.clone()), size_cpt: c.size.unwrap_or(base.size_cpt) };
}

// ===========================================================================
// Works 3/4 (MN0)

/// A Works 4 PLC: text positions (absolute or relative to the text start)
/// up to the text end, then fixed-size data.  Returns (positions, data).
fn plc4(d: &[u8], begin: usize, len: usize, tb: usize, te: usize) -> (Vec<usize>, Vec<&[u8]>) {
    let z = slice(d, begin, len);
    let mut pos = vec![];
    let mut rel: Option<bool> = None;
    let mut k = 0;
    while 4 * k + 4 <= z.len() && pos.len() < 10_000 {
        let mut p = u32_at(z, 4 * k).unwrap_or(0) as usize;
        k += 1;
        let r = *rel.get_or_insert(p < tb || p + tb <= te);
        if r {
            p += tb;
        }
        if p > te {
            break;
        }
        if pos.last().is_some_and(|&l| p < l) {
            break;
        }
        pos.push(p);
        if p == te {
            break;
        }
    }
    let n = pos.len().saturating_sub(1);
    let mut data = vec![];
    if n > 0 {
        let rest = z.len().saturating_sub(4 * (n + 1));
        let sz = rest / n;
        if sz > 0 && sz <= 100 {
            for i in 0..n {
                data.push(slice(z, 4 * (n + 1) + sz * i, sz));
            }
        }
    }
    (pos, data)
}

/// Runs of a small (128-byte) FDP page.
fn fdp4(d: &[u8], page: usize, te: usize, tb: usize, mut f: impl FnMut(usize, usize, &[u8])) -> bool {
    let p = slice(d, page, 128);
    if p.len() < 128 {
        return false;
    }
    let n = p[127] as usize;
    if n == 0 || 5 * n + 4 > 128 {
        return false;
    }
    let fc = |i: usize| -> usize {
        let v = u32_at(p, 4 * i).unwrap_or(0) as usize;
        if v == 0 { tb } else { v }
    };
    for i in 0..n {
        let (s, e) = (fc(i), fc(i + 1));
        if e < s || e > te {
            return false;
        }
        let depl = p[4 * (n + 1) + i] as usize;
        let props: &[u8] = if depl == 0 {
            &[]
        } else {
            let sz = p.get(depl).copied().unwrap_or(0) as usize;
            slice(p, depl + 1, sz.min(127usize.saturating_sub(depl)))
        };
        f(s, e, props);
    }
    true
}

fn read_v4(d: &[u8]) -> Result<Document, String> {
    if d.len() < 0x100 {
        return Err("damaged Works document".into());
    }
    let vers = d[0];
    let dos = vers == 1;
    // signed: -1 marks an absent zone
    let lim: Vec<i64> = (0..4).map(|i| u32_at(d, 0x1A + 4 * i).unwrap_or(0) as i32 as i64).collect();
    let eof = (u32_at(d, 0x2A).unwrap_or(0) as usize).min(d.len());
    // zones: header, footer, main
    let mut zones: [Option<(usize, usize)>; 3] = [None; 3];
    let mut last = lim[0].max(0x100) as usize;
    let (mut tb, mut te) = (usize::MAX, 0usize);
    for i in 0..3 {
        if lim[i + 1] < 0 {
            continue;
        }
        let np = lim[i + 1] as usize;
        let z = (last, np);
        if np >= last {
            last = np;
        }
        if z.1 > z.0 && z.0 >= 0x100 && z.1 <= eof {
            zones[i] = Some(z);
            tb = tb.min(z.0);
            te = te.max(z.1);
        }
    }
    let Some(mut main) = zones[2] else {
        return Err("damaged Works document (no text)".into());
    };
    let entry = |i: usize| -> (usize, usize) {
        let o = 0x2E + 6 * i;
        (u32_at(d, o).unwrap_or(0) as usize, u16_at(d, o + 4).unwrap_or(0) as usize)
    };
    // code page: Windows Works uses the ANSI code page
    let oem = (u16_at(d, 0x10).unwrap_or(0) >> 4) & 0x7FF;
    let doc_cp: u32 = match oem as u32 {
        cp @ (1250 | 1251 | 1252) => cp,
        cp @ (437 | 850) if dos => cp,
        866 if dos => 866,
        _ => if dos { 437 } else { 1252 },
    };

    // fonts
    let mut fonts: HashMap<u8, String> = HashMap::new();
    let (fb, fl) = entry(8);
    let ft = slice(d, fb, fl);
    let mut p = 0;
    while p + 3 <= ft.len() && fonts.len() < 256 {
        let (num, n) = (ft[p], ft[p + 2] as usize);
        let name: String = slice(ft, p + 3, n).iter().filter(|&&c| (b' '..=b'z').contains(&c)).map(|&c| c as char).collect();
        fonts.insert(num, name);
        p += 3 + n;
    }

    // character / paragraph runs
    let mut chps: Vec<Run<Chp>> = vec![];
    let mut paps: Vec<Run<Pap>> = vec![];
    let mut fallback_next: Option<usize> = None;
    for which in 0..2 {
        let (b, l) = entry(which);
        let mut pages: Vec<usize> = vec![];
        if b > 0 && l > 0 {
            let (_, data) = plc4(d, b, l, tb, te);
            pages = data.iter().filter_map(|x| u16_at(x, 0)).map(|v| v as usize * 0x80).collect();
        }
        if pages.is_empty() {
            // the FDPs follow the text: character pages, then paragraph pages
            let mut pg = fallback_next.unwrap_or(te.div_ceil(128) * 128);
            for _ in 0..64 {
                let p = slice(d, pg, 128);
                let n = p.get(127).copied().unwrap_or(0) as usize;
                if p.len() < 128 || n == 0 {
                    break;
                }
                pages.push(pg);
                pg += 128;
                if u32_at(p, 4 * n).unwrap_or(0) as usize >= te {
                    break;
                }
            }
            fallback_next = Some(pg);
        }
        for pg in pages {
            if which == 0 {
                fdp4(d, pg, te, tb, |s, e, x| {
                    let g = |i: usize| x.get(i).copied().unwrap_or(0);
                    let mut c = Chp::default();
                    let fl0 = g(0);
                    c.attrs.set(Attr::Bold, fl0 & 1 != 0);
                    c.attrs.set(Attr::Italic, fl0 & 2 != 0);
                    c.attrs.set(Attr::Strikeout, fl0 & 4 != 0);
                    if g(1) & 2 != 0 {
                        c.special = 1;
                    }
                    if x.len() > 2 {
                        c.font = fonts.get(&g(2)).cloned();
                    }
                    if g(3) != 0 {
                        c.attrs.set(Attr::Underline, true);
                    }
                    if g(4) != 0 {
                        c.size = Some(g(4) as u32 * 50);
                    }
                    let dec = g(5) as i8;
                    if dec > 0 {
                        c.attrs.set(Attr::Superscript, true);
                    } else if dec < 0 {
                        c.attrs.set(Attr::Subscript, true);
                    }
                    chps.push(Run { s, e, v: c });
                });
            } else {
                fdp4(d, pg, te, tb, |s, e, x| {
                    let mut pap = Pap::default();
                    let mut q = 3;
                    while q < x.len() {
                        let tag = x[q];
                        q += 1;
                        let n = match tag {
                            0x02 | 0x05 | 0x07..=0x0E | 0x10 | 0x18 | 0x1A | 0x1B => 1,
                            0x11..=0x17 => 2,
                            0x0F => 1 + x.get(q).copied().unwrap_or(0) as usize,
                            _ => break,
                        };
                        if tag == 0x05 {
                            pap.jc = Some(match x.get(q).copied().unwrap_or(0) {
                                1 => Justify::Center,
                                2 => Justify::Right,
                                3 => Justify::Full,
                                _ => Justify::Left,
                            });
                        }
                        q += n;
                    }
                    paps.push(Run { s, e, v: pap });
                });
            }
        }
    }
    chps.sort_by_key(|r| r.s);
    paps.sort_by_key(|r| r.s);

    // footnotes: references (FTNp) and texts (FTNd, at the end of the main zone)
    let mut notes: HashMap<usize, (usize, usize)> = HashMap::new();
    let ((pb, pl), (db, dl)) = (entry(5), entry(6));
    if pl > 0 && dl > 0 {
        let (refs, _) = plc4(d, pb, pl, tb, te);
        let (defs, _) = plc4(d, db, dl, tb, te);
        let n = refs.len().saturating_sub(1).min(defs.len().saturating_sub(1));
        let mut first = main.1;
        for i in 0..n {
            if defs[i] < defs[i + 1] {
                notes.insert(refs[i], (defs[i], defs[i + 1]));
                first = first.min(defs[i]);
            }
        }
        if first > main.0 {
            main.1 = first;
        }
    }

    let base = FontSpec {
        name: chps.iter().find(|r| r.e > main.0).and_then(|r| r.v.font.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Times New Roman".into()),
        size_cpt: chps.iter().find(|r| r.e > main.0).and_then(|r| r.v.size).unwrap_or(1200),
    };
    let w = W4 { d, chps, paps, notes, cp: doc_cp, base: base.clone() };
    let mut out = vec![];
    for (i, z) in zones.iter().enumerate().take(2) {
        if let Some((s, e)) = z {
            let els = w.emit(*s, *e, false);
            if els.iter().any(|e| e.is_char()) {
                out.push(El::Code(Code::HeadFoot { which: if i == 0 { 0 } else { 2 }, occ: Occurrence::Every, els }));
            }
        }
    }
    let mut b = Builder::new(base.clone());
    w.emit_into(&mut b, main.0, main.1, true);
    let body = b.into_document(base);
    let mut doc = body.clone();
    doc.els = out;
    doc.els.extend(body.els);
    Ok(doc)
}

struct W4<'a> {
    d: &'a [u8],
    chps: Vec<Run<Chp>>,
    paps: Vec<Run<Pap>>,
    notes: HashMap<usize, (usize, usize)>,
    cp: u32,
    base: FontSpec,
}

impl W4<'_> {
    fn emit(&self, s: usize, e: usize, main: bool) -> Vec<El> {
        let mut b = Builder::new(self.base.clone());
        self.emit_into(&mut b, s, e, main);
        b.finish()
    }

    fn emit_into(&self, b: &mut Builder, s: usize, e: usize, main: bool) {
        let e = e.min(self.d.len());
        let mut i = s;
        while i < e {
            let byte = self.d[i];
            let pap = find(&self.paps, i);
            if let Some(j) = pap.jc {
                b.just = j;
            }
            let chp = find(&self.chps, i);
            apply_chp(b, &chp, &self.base);
            if main {
                if let Some(&(ns, ne)) = self.notes.get(&i) {
                    let mut els = self.emit(ns, ne, false);
                    while matches!(els.first(), Some(El::Ch(' '))) {
                        els.remove(0);
                    }
                    b.note(NoteKind::Foot, els);
                    i += 1;
                    continue;
                }
            }
            i += 1;
            match byte {
                0x0D | 0x00 => {}
                0x0A | 0x0B => b.para(),
                0x09 => b.code_fmt(Code::Tab),
                0x0C => b.page(),
                0x11 => b.code_fmt(Code::HardHyphen),
                0x12 => b.code_fmt(Code::HardSpace),
                0x1F => b.code(Code::SoftHyphen),
                0x01..=0x1F => {}
                _ if chp.special != 0 => {}
                _ => b.ch(decode_byte(byte, self.cp)),
            }
        }
    }
}

// ===========================================================================
// Works 5–9 (CONTENTS)

/// A node of the tagged property structures.
#[derive(Clone, Debug, Default)]
struct Data {
    id: u8,
    ty: u8,
    value: i64,
    children: Vec<Data>,
}

impl Data {
    fn is_true(&self) -> bool {
        self.ty == 0x0A
    }
}

/// Read a block: u16 value, then children up to `end`.
fn read_block(d: &[u8], start: usize, end: usize, depth: u32) -> Data {
    let mut out = Data { value: u16_at(d, start).unwrap_or(0) as i64, ..Default::default() };
    let end = end.min(d.len());
    let mut p = start + 2;
    while p + 2 <= end && out.children.len() < 500 {
        let v = u16_at(d, p).unwrap_or(0);
        let (ty, id) = ((v >> 8) as u8, (v & 0xFF) as u8);
        if ty & 5 != 0 {
            break;
        }
        let mut c = Data { id, ty, ..Default::default() };
        match ty >> 4 {
            0 => p += 2,
            1 => {
                if p + 4 > end {
                    break;
                }
                c.value = if ty == 0x12 { d[p + 2] as i64 } else { u16_at(d, p + 2).unwrap_or(0) as i64 };
                p += 4;
            }
            2 => {
                if ty == 0x2A {
                    if p + 10 > end {
                        break;
                    }
                    c.value = u32_at(d, p + 6).unwrap_or(0) as i32 as i64;
                    p += 10;
                } else {
                    if p + 6 > end {
                        break;
                    }
                    c.value = u32_at(d, p + 2).unwrap_or(0) as i32 as i64;
                    p += 6;
                }
            }
            8 => {
                if p + 4 > end {
                    break;
                }
                let extra = u16_at(d, p + 2).unwrap_or(0) as usize;
                let ne = p + 2 + extra;
                if extra % 2 == 1 || ne > end {
                    break;
                }
                if depth < 8 {
                    let sub = read_block(d, p + 4, ne, depth + 1);
                    c.children = sub.children;
                    c.value = sub.value;
                }
                p = ne;
            }
            _ => break,
        }
        out.children.push(c);
    }
    out
}

struct Entry {
    name: [u8; 4],
    id: u16,
    ty: [u8; 4],
    begin: usize,
    len: usize,
}

fn read_v8(c: &[u8]) -> Result<Document, String> {
    // ---- chunk index
    let mut entries: Vec<Entry> = vec![];
    let mut n_total = u16_at(c, 0x0C).unwrap_or(0) as usize;
    let mut blk = 0x18usize;
    let mut guard = 0;
    while n_total > 0 && guard < 100 {
        guard += 1;
        let (Some(nl), Some(next)) = (u16_at(c, blk + 2), u32_at(c, blk + 4)) else { break };
        if nl > 0x20 {
            break;
        }
        let mut q = blk + 8;
        for _ in 0..nl {
            if n_total == 0 {
                break;
            }
            n_total -= 1;
            let Some(cch) = u16_at(c, q).map(|v| v as usize) else { break };
            if cch < 10 {
                break;
            }
            if cch >= 0x18 {
                let e = slice(c, q, 0x18);
                if e.len() == 0x18 {
                    let mut name = [0u8; 4];
                    name.copy_from_slice(&e[2..6]);
                    let mut ty = [0u8; 4];
                    ty.copy_from_slice(&e[12..16]);
                    let begin = u32_at(e, 16).unwrap_or(0) as usize;
                    let len = u32_at(e, 20).unwrap_or(0) as usize;
                    if begin.saturating_add(len) <= c.len() {
                        entries.push(Entry { name, id: u16_at(e, 6).unwrap_or(0), ty, begin, len });
                    }
                }
            }
            q += cch;
        }
        if next == u32::MAX || (next as usize) <= blk {
            break;
        }
        blk = next as usize;
    }
    let get = |n: [u8; 4]| entries.iter().filter(move |e| e.name == n);
    let text = get(*b"TEXT").next().ok_or("damaged Works document (no text)")?;
    let (tb, te) = (text.begin, text.begin + text.len);

    // ---- text zones (STRS): kind 1 = main, 2 = footnotes, 3 = endnotes,
    // 6 = header, 7 = footer
    let mut zones: Vec<(usize, usize, i64)> = vec![];
    if let Some(strs) = get(*b"STRS").find(|e| &e.ty == b"PLC ") {
        let z = slice(c, strs.begin, strs.len);
        let n = (u32_at(z, 0).unwrap_or(0) as usize).min(1000);
        let mut last = tb;
        let mut ptrs = vec![];
        for i in 0..=n {
            let Some(v) = u32_at(z, 12 + 4 * i) else { break };
            ptrs.push(last);
            last = last.saturating_add(2 * v as usize);
        }
        let mut p = 12 + 4 * (n + 1);
        let mut kind_prev = 1;
        for i in 0..n {
            let sz = u16_at(z, p).unwrap_or(0) as usize;
            let mut kind = kind_prev;
            if sz >= 2 && p + sz <= z.len() {
                let blk = read_block(z, p + 2, p + sz, 0);
                if let Some(k) = blk.children.iter().find(|d| d.id == 0 && d.ty == 0x22) {
                    kind = k.value;
                }
                p += sz;
            } else {
                p = z.len();
            }
            kind_prev = kind;
            if let (Some(&s), Some(&e)) = (ptrs.get(i), ptrs.get(i + 1)) {
                zones.push((s.min(te), e.min(te), kind));
            }
        }
    }
    if zones.is_empty() {
        zones.push((tb, te, 1));
    }

    // ---- fonts
    let mut fonts: Vec<String> = vec![];
    if let Some(f) = get(*b"FONT").next() {
        let z = slice(c, f.begin, f.len);
        let nf = (u32_at(z, 4).unwrap_or(0) as usize).min(1000);
        let mut p = 8;
        while fonts.len() < nf && p + 2 <= z.len() {
            let n = u16_at(z, p).unwrap_or(0) as usize;
            let units: Vec<u16> = (0..n).map_while(|k| u16_at(z, p + 2 + 2 * k)).collect();
            fonts.push(String::from_utf16_lossy(&units));
            p += 2 + 2 * n;
        }
    }

    // ---- character / paragraph runs
    let mut chps: Vec<Run<Chp>> = vec![];
    let mut paps: Vec<Run<Pap>> = vec![];
    for (name, is_chp) in [(b"FDPC", true), (b"FDPP", false)] {
        for e in get(*name) {
            let pg = slice(c, e.begin, e.len);
            let n = u16_at(pg, 0).unwrap_or(0) as usize;
            if n == 0 || 8 + 6 * n > pg.len() {
                continue;
            }
            for i in 0..n {
                let pos = |k: usize| -> usize {
                    let v = u32_at(pg, 4 + 4 * k).unwrap_or(0) as usize;
                    if v == 0 { tb } else { v }
                };
                let (s, en) = (pos(i), pos(i + 1));
                let depl = u16_at(pg, 4 + 4 * (n + 1) + 2 * i).unwrap_or(0) as usize;
                let blk = if depl == 0 || depl + 2 > pg.len() {
                    Data::default()
                } else {
                    let sz = u16_at(pg, depl).unwrap_or(0) as usize;
                    if sz >= 4 && depl + sz <= pg.len() { read_block(pg, depl + 2, depl + sz, 0) } else { Data::default() }
                };
                if is_chp {
                    let mut ch = Chp::default();
                    for dt in &blk.children {
                        match dt.id {
                            0x00 => ch.special = dt.value as i32,
                            0x02 => ch.attrs.set(Attr::Bold, dt.is_true()),
                            0x03 => ch.attrs.set(Attr::Italic, dt.is_true()),
                            0x04 => ch.attrs.set(Attr::Outline, dt.is_true()),
                            0x05 => ch.attrs.set(Attr::Shadow, dt.is_true()),
                            0x0C if dt.value > 0 => ch.size = Some(((dt.value / 12700) as u32 * 100).clamp(400, 20000)),
                            0x0F => {
                                ch.attrs.set(Attr::Superscript, dt.value == 1);
                                ch.attrs.set(Attr::Subscript, dt.value == 2);
                            }
                            0x10 => ch.attrs.set(Attr::Strikeout, dt.is_true()),
                            0x13 => ch.attrs.set(Attr::SmallCaps, dt.is_true()),
                            0x14 => ch.caps = dt.is_true(),
                            0x1E => {
                                ch.attrs.set(Attr::Underline, dt.value != 0 && dt.value != 3);
                                ch.attrs.set(Attr::DblUnderline, dt.value == 3);
                            }
                            0x24 => {
                                if let Some(f) = dt.children.first().and_then(|x| fonts.get(x.value as usize)) {
                                    ch.font = Some(f.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                    chps.push(Run { s, e: en, v: ch });
                } else {
                    let mut pap = Pap::default();
                    if let Some(j) = blk.children.iter().find(|d| d.id == 0x04) {
                        pap.jc = Some(match j.value {
                            1 => Justify::Right,
                            2 => Justify::Center,
                            3 => Justify::Full,
                            _ => Justify::Left,
                        });
                    }
                    paps.push(Run { s, e: en, v: pap });
                }
            }
        }
    }
    chps.sort_by_key(|r| r.s);
    paps.sort_by_key(|r| r.s);

    // ---- notes: text ranges inside the note zones, in reference order
    let mut note_texts: HashMap<i64, Vec<(usize, usize)>> = HashMap::new();
    for (name, kind) in [(b"FTN ", 2i64), (b"EDN ", 3)] {
        for e in get(*name) {
            let z = slice(c, e.begin, e.len);
            let Some(zone) = zones.get(e.id as usize).copied() else { continue };
            let n = (u32_at(z, 4).unwrap_or(0) as usize).min(5000);
            let ptrs: Vec<usize> = (0..=n).filter_map(|i| u32_at(z, 8 + 4 * i)).map(|v| zone.0 + 2 * v as usize).filter(|&p| p <= zone.1).collect();
            let list = note_texts.entry(kind).or_default();
            for w in ptrs.windows(2) {
                if w[0] < w[1] {
                    list.push((w[0], w[1]));
                }
            }
        }
    }

    let main_zone = zones.iter().find(|z| z.2 == 1).copied().unwrap_or((tb, te, 1));
    let first_chp = chps.iter().find(|r| r.e > main_zone.0).map(|r| r.v.clone()).unwrap_or_default();
    let base = FontSpec { name: first_chp.font.clone().unwrap_or_else(|| fonts.first().cloned().unwrap_or_else(|| "Times New Roman".into())), size_cpt: first_chp.size.unwrap_or(1200) };
    let w = W8 { c, chps, paps, base: base.clone(), notes: note_texts, note_idx: Default::default() };

    let mut hf: Vec<El> = vec![];
    for (kind, which) in [(6i64, 0u8), (7, 2)] {
        if let Some(z) = zones.iter().find(|z| z.2 == kind) {
            let mut b = Builder::new(base.clone());
            w.emit_into(&mut b, z.0, z.1, false);
            let els = b.finish();
            if els.iter().any(|e| e.is_char()) {
                hf.push(El::Code(Code::HeadFoot { which, occ: Occurrence::Every, els }));
            }
        }
    }
    let mut b = Builder::new(base.clone());
    w.emit_into(&mut b, main_zone.0, main_zone.1, true);
    let mut doc = b.into_document(base);
    hf.extend(std::mem::take(&mut doc.els));
    doc.els = hf;
    Ok(doc)
}

struct W8<'a> {
    c: &'a [u8],
    chps: Vec<Run<Chp>>,
    paps: Vec<Run<Pap>>,
    base: FontSpec,
    notes: HashMap<i64, Vec<(usize, usize)>>,
    note_idx: std::cell::RefCell<HashMap<i64, usize>>,
}

impl W8<'_> {
    fn emit_into(&self, b: &mut Builder, s: usize, e: usize, main: bool) {
        let e = e.min(self.c.len());
        let mut i = s;
        let mut guard = 0;
        while i + 1 < e + 1 && i + 2 <= e && guard < 5_000_000 {
            guard += 1;
            let u = u16_at(self.c, i).unwrap_or(0);
            let pos = i;
            i += 2;
            let pap = find(&self.paps, pos);
            if let Some(j) = pap.jc {
                b.just = j;
            }
            let chp = find(&self.chps, pos);
            apply_chp(b, &chp, &self.base);
            match u {
                0x0D => b.para(),
                0x0A => b.para(),
                0x09 => b.code_fmt(Code::Tab),
                0x0C => b.page(),
                0x0E => b.para(),
                0x1E => b.code_fmt(Code::HardHyphen),
                0x1F => b.code_fmt(Code::HardSpace),
                0x23 if chp.special != 0 => {
                    // footnote (3) / endnote (4) reference; fields: skip
                    if main && (chp.special == 3 || chp.special == 4) {
                        let kind = if chp.special == 3 { 2 } else { 3 };
                        let k = {
                            let mut m = self.note_idx.borrow_mut();
                            let k = m.entry(kind).or_insert(0);
                            *k += 1;
                            *k - 1
                        };
                        if let Some(&(ns, ne)) = self.notes.get(&kind).and_then(|l| l.get(k)) {
                            let mut nb = Builder::new(self.base.clone());
                            self.emit_into(&mut nb, ns, ne, false);
                            let mut els = nb.finish();
                            while matches!(els.first(), Some(El::Ch(' '))) {
                                els.remove(0);
                            }
                            b.note(if kind == 2 { NoteKind::Foot } else { NoteKind::End }, els);
                        }
                    }
                }
                0xFFFC => {}
                0x00..=0x1F => {}
                0xD800..=0xDBFF => {
                    let lo = u16_at(self.c, i).unwrap_or(0);
                    if (0xDC00..0xE000).contains(&lo) && i + 2 <= e {
                        i += 2;
                        if let Some(ch) = char::from_u32(0x10000 + ((u as u32 - 0xD800) << 10) + (lo as u32 - 0xDC00)) {
                            b.ch(ch);
                        }
                    }
                }
                0xDC00..=0xDFFF => {}
                _ => {
                    if let Some(ch) = char::from_u32(u as u32) {
                        let ch = if (0xF020..=0xF0FF).contains(&u) { crate::rtf::symbol_char(&b.font.name.clone(), u as u32).unwrap_or(ch) } else { ch };
                        if chp.caps {
                            for c in ch.to_uppercase() {
                                b.ch(c);
                            }
                        } else {
                            b.ch(ch);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    fn load(n: &str) -> Vec<u8> {
        std::fs::read(format!("tests/data/formats/{n}")).unwrap()
    }

    #[test]
    fn probes() {
        for n in ["Works_3.0.wps", "Works_4.5.wps", "Works_5.0.wps", "Works_6.0.wps", "works4.wps", "works8.wps", "works2000.wps"] {
            assert!(probe(&load(n)), "{n}");
        }
        assert!(!probe(&load("word95.doc")));
    }

    #[test]
    fn works4_text() {
        let d = read(&load("works4.wps")).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("This is an example document made with Microsoft Works Word Processor"), "{t}");
        // the empty sample documents still open
        for n in ["Works_3.0.wps", "Works_4.5.wps", "Works_5.0.wps", "Works_6.0.wps"] {
            read(&load(n)).unwrap();
        }
    }

    #[test]
    fn works8_text() {
        for n in ["works8.wps", "works2000.wps"] {
            let d = read(&load(n)).unwrap();
            let t = debug_text(&d.els);
            assert!(t.len() > 20, "{n}: {t}");
        }
    }

    fn put32(v: &mut [u8], o: usize, x: u32) {
        v[o..o + 4].copy_from_slice(&x.to_le_bytes());
    }
    fn put16(v: &mut [u8], o: usize, x: u16) {
        v[o..o + 2].copy_from_slice(&x.to_le_bytes());
    }

    /// Hand-made Works 4 MN0 stream: umlauts, a bold run, a centred
    /// paragraph, a font table.
    fn works4_stream() -> Vec<u8> {
        let mut v = vec![0u8; 0x2A0];
        v[0] = 6;
        v[1] = 0xFE;
        let text = b"Gr\xfc\xdfe\r\nMitte\r\n";
        let end = 0x100 + text.len() as u32;
        for (i, x) in [0x100u32, 0x100, 0x100, end].iter().enumerate() {
            put32(&mut v, 0x1A + 4 * i, *x);
        }
        put32(&mut v, 0x2A, 0x2A0);
        // entries: BTEC, BTEP, …, FONT
        put32(&mut v, 0x2E, 0x280);
        put16(&mut v, 0x32, 10);
        put32(&mut v, 0x34, 0x28A);
        put16(&mut v, 0x38, 10);
        put32(&mut v, 0x5E, 0x294);
        put16(&mut v, 0x62, 8);
        v[0x100..0x100 + text.len()].copy_from_slice(text);
        // FDPC page at 0x180: "Grüße" bold
        put32(&mut v, 0x180, 0x100);
        put32(&mut v, 0x184, 0x105);
        put32(&mut v, 0x188, end);
        v[0x18C] = 0x70;
        v[0x18D] = 0;
        v[0x1F0] = 3; // len
        v[0x1F1] = 1; // bold
        v[0x1F2] = 0x08; // font given
        v[0x1F3] = 0; // font number 0
        v[0x1FF] = 2;
        // FDPP page at 0x200: second paragraph centred
        put32(&mut v, 0x200, 0x100);
        put32(&mut v, 0x204, 0x107);
        put32(&mut v, 0x208, end);
        v[0x20C] = 0;
        v[0x20D] = 0x70;
        v[0x270..0x276].copy_from_slice(&[5, 0, 0, 0, 0x05, 1]);
        v[0x27F] = 2;
        // BTEC / BTEP PLCs
        put32(&mut v, 0x280, 0x100);
        put32(&mut v, 0x284, end);
        put16(&mut v, 0x288, 3);
        put32(&mut v, 0x28A, 0x100);
        put32(&mut v, 0x28E, end);
        put16(&mut v, 0x292, 4);
        // FONT: number 0, "Arial"
        v[0x294..0x29C].copy_from_slice(&[0, 0, 5, b'A', b'r', b'i', b'a', b'l']);
        v
    }

    #[test]
    fn works4_formatting() {
        let d = read_v4(&works4_stream()).unwrap();
        assert_eq!(debug_text(&d.els), "Grüße\nMitte");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Center))));
        assert_eq!(d.init.font.name, "Arial");
    }

    /// Hand-made Works 8 CONTENTS stream: Russian text with a bold run.
    #[test]
    fn works8_formatting() {
        let mut v = vec![0u8; 0x600];
        v[..8].copy_from_slice(b"CHNKWKS ");
        put16(&mut v, 0x0C, 2);
        put16(&mut v, 0x1A, 2);
        put32(&mut v, 0x1C, u32::MAX);
        let text: Vec<u16> = "Привет fett\rZwei\r".encode_utf16().collect();
        for (k, (name, b, l)) in [(b"TEXT", 0x200u32, 2 * text.len() as u32), (b"FDPC", 0x400, 0x200)].iter().enumerate() {
            let q = 0x20 + 24 * k;
            put16(&mut v, q, 0x18);
            v[q + 2..q + 6].copy_from_slice(*name);
            v[q + 12..q + 16].copy_from_slice(*name);
            put32(&mut v, q + 16, *b);
            put32(&mut v, q + 20, *l);
        }
        for (i, u) in text.iter().enumerate() {
            put16(&mut v, 0x200 + 2 * i, *u);
        }
        let end = 0x200 + 2 * text.len() as u32;
        put16(&mut v, 0x400, 2);
        put32(&mut v, 0x404, 0x200);
        put32(&mut v, 0x408, 0x200 + 14);
        put32(&mut v, 0x40C, end);
        put16(&mut v, 0x410, 0);
        put16(&mut v, 0x412, 0x100);
        put16(&mut v, 0x500, 6);
        put16(&mut v, 0x504, 0x0A02);
        let d = read_v8(&v).unwrap();
        assert_eq!(debug_text(&d.els), "Привет fett\nZwei");
        let i = d.els.iter().position(|e| *e == El::Code(Code::AttrOn(Attr::Bold))).unwrap();
        assert_eq!(d.els[i - 1], El::Ch(' '));
    }

    #[test]
    fn never_panics() {
        let b = works4_stream();
        for cut in 0..b.len() {
            let _ = read_v4(&b[..cut]);
        }
        for n in ["works4.wps", "works8.wps", "works2000.wps"] {
            let b = load(n);
            for cut in (0..b.len()).step_by(61) {
                let _ = read(&b[..cut]);
            }
            let mut seed: u32 = 99;
            for _ in 0..300 {
                let mut v = b.clone();
                for _ in 0..12 {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    let i = 512 + (seed as usize >> 3) % (v.len() - 512);
                    v[i] = (seed >> 24) as u8;
                }
                let _ = read(&v);
            }
        }
    }
}
