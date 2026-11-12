//! Microsoft Word 97–2003 binary (.doc) reader, plus best-effort support
//! for Word 6 / Word 95 files.
//!
//! Written from the [MS-CFB] and [MS-DOC] open specifications.  Everything
//! is bounds-checked: malformed input yields an error or partial text,
//! never a panic.
//!
//! Conventions follow the .docx reader: character properties become
//! paired attribute codes, paragraph alignment `[Just:…]`, page breaks
//! `[HPg]`, tables are flattened (Tab between cells, HRt after a row),
//! footnotes/endnotes become `Code::Note`, the primary header/footer
//! `Code::HeadFoot`.

use crate::doc::*;
use crate::rtf::{TableState, decode_byte, finish_els, sync_attrs, widths_from_bounds};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// little helpers

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

// ---------------------------------------------------------------------------
// Compound File Binary (OLE2 structured storage)

const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const MAXREGSECT: u32 = 0xFFFF_FFFA;

struct DirEntry {
    name: String,
    typ: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

pub struct Cfb<'a> {
    data: &'a [u8],
    ssz: usize,
    fat: Vec<u32>,
    minifat: Vec<u32>,
    dir: Vec<DirEntry>,
    mini: Vec<u8>,
    cutoff: u64,
}

impl<'a> Cfb<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Cfb<'a>, String> {
        if data.len() < 512 || !data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
            return Err("not an OLE compound file".into());
        }
        let shift = u16_at(data, 0x1E).unwrap_or(9);
        if shift != 9 && shift != 12 {
            return Err("damaged OLE compound file (sector size)".into());
        }
        let mut c = Cfb { data, ssz: 1usize << shift, fat: vec![], minifat: vec![], dir: vec![], mini: vec![], cutoff: 4096 };
        c.cutoff = u32_at(data, 0x38).unwrap_or(4096) as u64;
        // DIFAT
        let mut difat: Vec<u32> = (0..109).filter_map(|i| u32_at(data, 0x4C + 4 * i)).collect();
        let mut next = u32_at(data, 0x44).unwrap_or(ENDOFCHAIN);
        let mut guard = 0;
        while next <= MAXREGSECT && guard < 100_000 {
            guard += 1;
            let Some(sec) = c.sector(next) else { break };
            let n = c.ssz / 4 - 1;
            for i in 0..n {
                if let Some(v) = u32_at(sec, 4 * i) {
                    difat.push(v);
                }
            }
            next = u32_at(sec, 4 * n).unwrap_or(ENDOFCHAIN);
        }
        let max_sectors = data.len() / c.ssz + 1;
        for s in difat {
            if s > MAXREGSECT {
                continue;
            }
            if c.fat.len() > max_sectors + c.ssz {
                break;
            }
            if let Some(sec) = c.sector(s) {
                for i in 0..sec.len() / 4 {
                    c.fat.push(u32_at(sec, 4 * i).unwrap_or(ENDOFCHAIN));
                }
            }
        }
        // directory
        let dirb = c.chain(u32_at(data, 0x30).unwrap_or(ENDOFCHAIN), usize::MAX);
        for e in dirb.chunks_exact(128) {
            let nl = (u16_at(e, 64).unwrap_or(0) as usize).min(64);
            let units: Vec<u16> = (0..nl / 2).filter_map(|i| u16_at(e, 2 * i)).take_while(|&u| u != 0).collect();
            c.dir.push(DirEntry {
                name: String::from_utf16_lossy(&units),
                typ: e[66],
                left: u32_at(e, 68).unwrap_or(u32::MAX),
                right: u32_at(e, 72).unwrap_or(u32::MAX),
                child: u32_at(e, 76).unwrap_or(u32::MAX),
                start: u32_at(e, 116).unwrap_or(ENDOFCHAIN),
                size: if shift == 9 { u32_at(e, 120).unwrap_or(0) as u64 } else { u32_at(e, 120).unwrap_or(0) as u64 | ((u32_at(e, 124).unwrap_or(0) as u64) << 32) },
            });
        }
        if c.dir.is_empty() {
            return Err("damaged OLE compound file (no directory)".into());
        }
        let mf = c.chain(u32_at(data, 0x3C).unwrap_or(ENDOFCHAIN), usize::MAX);
        c.minifat = (0..mf.len() / 4).filter_map(|i| u32_at(&mf, 4 * i)).collect();
        let root_size = c.dir[0].size.min(data.len() as u64) as usize;
        c.mini = c.chain(c.dir[0].start, root_size);
        Ok(c)
    }

    fn sector(&self, n: u32) -> Option<&'a [u8]> {
        let off = (n as usize).checked_add(1)?.checked_mul(self.ssz)?;
        if off >= self.data.len() {
            return None;
        }
        Some(&self.data[off..(off + self.ssz).min(self.data.len())])
    }

    fn chain(&self, start: u32, max: usize) -> Vec<u8> {
        let mut out = vec![];
        let mut s = start;
        let mut steps = 0;
        while s <= MAXREGSECT && out.len() < max && steps <= self.fat.len() {
            steps += 1;
            let Some(sec) = self.sector(s) else { break };
            out.extend_from_slice(sec);
            s = self.fat.get(s as usize).copied().unwrap_or(ENDOFCHAIN);
        }
        out.truncate(max);
        out
    }

    fn mini_chain(&self, start: u32, max: usize) -> Vec<u8> {
        let mut out = vec![];
        let mut s = start;
        let mut steps = 0;
        while s <= MAXREGSECT && out.len() < max && steps <= self.minifat.len() {
            steps += 1;
            let off = (s as usize) * 64;
            if off >= self.mini.len() {
                break;
            }
            out.extend_from_slice(&self.mini[off..(off + 64).min(self.mini.len())]);
            s = self.minifat.get(s as usize).copied().unwrap_or(ENDOFCHAIN);
        }
        out.truncate(max);
        out
    }

    /// Directory ids of all entries directly inside the storage whose
    /// first child is `child` (the red-black tree of siblings).
    fn siblings(&self, child: u32) -> Vec<usize> {
        let mut out = vec![];
        let mut stack = vec![child];
        let mut seen = vec![false; self.dir.len()];
        while let Some(id) = stack.pop() {
            let Some(e) = self.dir.get(id as usize) else { continue };
            if std::mem::replace(&mut seen[id as usize], true) {
                continue;
            }
            out.push(id as usize);
            stack.push(e.left);
            stack.push(e.right);
        }
        out
    }

    fn read_entry(&self, e: &DirEntry) -> Vec<u8> {
        let size = e.size.min(self.data.len() as u64 * 2) as usize;
        if e.size < self.cutoff { self.mini_chain(e.start, size) } else { self.chain(e.start, size) }
    }

    /// Names of the storages (sub-folders) directly below the root.
    pub fn storages(&self) -> Vec<String> {
        let mut v: Vec<String> = self.siblings(self.dir[0].child).into_iter().filter(|&i| self.dir[i].typ == 1).map(|i| self.dir[i].name.clone()).collect();
        v.sort();
        v
    }

    /// A stream inside the storage `storage` (directly below the root).
    pub fn stream_in(&self, storage: &str, name: &str) -> Option<Vec<u8>> {
        let st = self.siblings(self.dir[0].child).into_iter().find(|&i| self.dir[i].typ == 1 && self.dir[i].name.eq_ignore_ascii_case(storage))?;
        let id = self.siblings(self.dir[st].child).into_iter().find(|&i| self.dir[i].typ == 2 && self.dir[i].name.eq_ignore_ascii_case(name))?;
        Some(self.read_entry(&self.dir[id]))
    }

    /// A stream directly below the root storage.
    pub fn stream(&self, name: &str) -> Option<Vec<u8>> {
        let mut stack = vec![self.dir[0].child];
        let mut seen = vec![false; self.dir.len()];
        while let Some(id) = stack.pop() {
            let Some(e) = self.dir.get(id as usize) else { continue };
            if std::mem::replace(&mut seen[id as usize], true) {
                continue;
            }
            if e.typ == 2 && e.name.eq_ignore_ascii_case(name) {
                let size = e.size.min(self.data.len() as u64 * 2) as usize;
                return Some(if (e.size) < self.cutoff { self.mini_chain(e.start, size) } else { self.chain(e.start, size) });
            }
            stack.push(e.left);
            stack.push(e.right);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Word binary format

/// Character properties (the subset we map to WP codes).
#[derive(Clone, Debug, PartialEq, Default)]
struct Chp {
    bold: bool,
    italic: bool,
    strike: bool,
    dstrike: bool,
    outline: bool,
    shadow: bool,
    smallcaps: bool,
    caps: bool,
    vanish: bool,
    kul: u8,
    iss: u8,
    hps: u16,
    ftc: u16,
    del: bool,
    ins: bool,
    ibst: u16,
    dttm: u32,
    ibst_del: u16,
    dttm_del: u32,
    spec: bool,
    symbol: Option<u16>,
}

#[derive(Clone, Debug, Default)]
struct Pap {
    jc: u8,
    in_table: bool,
    ttp: bool,
    inner_cell: bool,
    inner_ttp: bool,
    /// column widths (WPU) from sprmTDefTable of a row-end paragraph
    cols: Vec<i32>,
}

#[derive(Default, Clone)]
struct Style {
    stk: u8,
    base: u16,
    papx: Vec<u8>,
    chpx: Vec<u8>,
}

struct Font {
    name: String,
    cp: Option<u32>,
}

#[derive(Clone, Copy)]
struct TChar {
    u: u16,
    fc: u32,
    /// 8-bit character from a Word 6/95 file, decoded with the font's
    /// code page at output time.
    raw: bool,
}

struct ChpxRun {
    s: u32,
    e: u32,
    grp: Vec<u8>,
}

struct PapxRun {
    s: u32,
    e: u32,
    istd: u16,
    grp: Vec<u8>,
}

struct Word {
    w6: bool,
    text: Vec<TChar>,
    chpx: Vec<ChpxRun>,
    papx: Vec<PapxRun>,
    styles: Vec<Option<Style>>,
    fonts: Vec<Font>,
    default_ftc: u16,
    notes: HashMap<usize, (NoteKind, usize, usize)>,
    annots: HashMap<usize, (usize, usize)>,
    /// section boundaries: (cp of section mark, next section continuous)
    sect_marks: HashMap<usize, bool>,
    chp_cache: std::cell::RefCell<HashMap<(usize, u16), Chp>>,
    rev_authors: Vec<String>,
    text_width: i32,
}

/// Map a Word 6 (one byte) sprm to its Word 97 equivalent (only those we
/// interpret).
fn w6_sprm(op: u8) -> u16 {
    match op {
        2 => 0x4600,
        5 => 0x2403,
        24 => 0x2416,
        25 => 0x2417,
        65 => 0x0800,
        66 => 0x0801,
        69 => 0x4804,
        70 => 0x6805,
        190 => 0xD608,
        71 => 0x0806,
        75 => 0x080A,
        80 => 0x4A30,
        83 => 0x2A33,
        85..=92 => 0x0835 + (op as u16 - 85),
        93 => 0x4A4F,
        94 => 0x2A3E,
        99 => 0x4A43,
        104 => 0x2A48,
        117 => 0x0855,
        142 => 0x3009,
        // sprmSGprfIhdt: which header/footer stories a section has (Word 6)
        153 => 0x3013,
        164 => 0xB01F,
        165 => 0xB020,
        166 => 0xB021,
        167 => 0xB022,
        168 => 0x9023,
        169 => 0x9024,
        _ => 0xFFFF,
    }
}

/// Operand length of a Word 6 sprm, None when unknown.
fn w6_len(op: u8, rest: &[u8]) -> Option<usize> {
    let var = || rest.first().map(|&n| 1 + n as usize);
    Some(match op {
        0 => 0,
        2 => 2,
        3 | 12 | 15 | 52 | 68 | 74 | 81 | 103 | 105 | 106 | 108 | 133 | 188 | 191 => return var(),
        73 => 3,
        4..=11 | 13 | 14 | 24 | 25 | 29 | 44 | 50 | 51 | 65 | 66 | 67 | 71 => 1,
        16..=19 | 21 | 22 | 26..=28 | 30..=43 | 45..=49 => 2,
        20 => 4,
        23 => return chgtabs_len(rest),
        69 | 72 => 2,
        70 => 4,
        75 | 76..=79 => 1,
        80 => 2,
        82 | 83 => 0,
        84..=92 => 1,
        93 => 2,
        94 => 1,
        95 => 3,
        96 | 97 => 2,
        98 => 1,
        99 => 2,
        100 => 1,
        101 => 2,
        102 => 1,
        104 => 1,
        107 | 109 => 2,
        117 | 118 => 1,
        131 | 132 => 1,
        136 | 137 => 3,
        138 | 139 => 1,
        140 | 141 => 2,
        142 | 143 => 1,
        144 | 145 => 2,
        146 | 147 => 1,
        148 | 149 => 2,
        150..=153 => 1,
        154..=157 => 2,
        158 | 159 => 1,
        160 | 161 => 2,
        162 | 163 => 1,
        164..=171 => 2,
        182 | 183 | 184 => 2,
        185 | 186 => 1,
        187 => 12,
        189 => 2,
        190 => return u16_at(rest, 0).map(|n| 2 + n as usize),
        192 => 4,
        193 => 5,
        194 => 4,
        195 => 2,
        196 => 4,
        197 | 198 => 2,
        199 => 5,
        200 => 4,
        _ => return None,
    })
}

fn chgtabs_len(rest: &[u8]) -> Option<usize> {
    let cb = *rest.first()? as usize;
    if cb != 255 {
        return Some(1 + cb);
    }
    let del = *rest.get(1)? as usize;
    let add = *rest.get(2 + 4 * del)? as usize;
    Some(1 + 1 + 4 * del + 1 + 3 * add)
}

/// Operand length of a Word 97 sprm.
fn w97_len(code: u16, rest: &[u8]) -> Option<usize> {
    Some(match code >> 13 {
        0 | 1 => 1,
        2 | 4 | 5 => 2,
        3 => 4,
        7 => 3,
        _ => {
            if code == 0xD608 || code == 0xD606 {
                let cb = u16_at(rest, 0)? as usize;
                2 + cb.saturating_sub(1)
            } else if code == 0xC615 {
                chgtabs_len(rest)?
            } else {
                1 + *rest.first()? as usize
            }
        }
    })
}

/// Iterate (sprm code in Word 97 numbering, operand) over a grpprl.
fn sprms(grp: &[u8], w6: bool) -> Vec<(u16, &[u8])> {
    let mut out = vec![];
    let mut p = 0;
    while p < grp.len() {
        let (code, hdr, len) = if w6 {
            let op = grp[p];
            let Some(len) = w6_len(op, &grp[p + 1..]) else { break };
            (w6_sprm(op), 1, len)
        } else {
            let Some(code) = u16_at(grp, p) else { break };
            let Some(len) = w97_len(code, grp.get(p + 2..).unwrap_or(&[])) else { break };
            (code, 2, len)
        };
        let s = p + hdr;
        let e = s.saturating_add(len);
        if e > grp.len() {
            break;
        }
        out.push((code, &grp[s..e]));
        p = e;
    }
    out
}

fn toggle(cur: bool, style: bool, v: u8) -> bool {
    match v {
        0 => false,
        1 => true,
        0x80 => style,
        0x81 => !style,
        _ => cur,
    }
}

impl Word {
    fn style(&self, istd: u16) -> Option<&Style> {
        self.styles.get(istd as usize).and_then(|s| s.as_ref())
    }

    fn default_chp(&self) -> Chp {
        Chp { hps: 20, ftc: self.default_ftc, ..Default::default() }
    }

    /// Character properties of a paragraph style (with inheritance).
    fn para_style_chp(&self, istd: u16, depth: u32) -> Chp {
        let Some(st) = self.style(istd) else { return self.default_chp() };
        let mut chp = if depth < 12 && st.base != 0x0FFF && st.base != istd { self.para_style_chp(st.base, depth + 1) } else { self.default_chp() };
        let base = chp.clone();
        self.apply_chpx(&mut chp, &base, &st.chpx, depth);
        chp
    }

    fn apply_char_style(&self, chp: &mut Chp, base: &Chp, istd: u16, depth: u32) {
        let Some(st) = self.style(istd) else { return };
        if st.stk != 2 || depth > 12 {
            return;
        }
        if st.base != 0x0FFF && st.base != istd {
            self.apply_char_style(chp, base, st.base, depth + 1);
        }
        let g = st.chpx.clone();
        self.apply_chpx(chp, base, &g, depth + 1);
    }

    fn apply_chpx(&self, chp: &mut Chp, base: &Chp, grp: &[u8], depth: u32) {
        for (code, op) in sprms(grp, self.w6) {
            let b = op.first().copied().unwrap_or(0);
            let w = u16_at(op, 0).unwrap_or(0);
            match code {
                0x0800 => chp.del = b != 0,
                0x0801 => chp.ins = b != 0,
                0x4804 => chp.ibst = w,
                0x6805 => chp.dttm = u32_at(op, 0).unwrap_or(0),
                0x4863 => chp.ibst_del = w,
                0x6864 => chp.dttm_del = u32_at(op, 0).unwrap_or(0),
                0x0835 => chp.bold = toggle(chp.bold, base.bold, b),
                0x0836 => chp.italic = toggle(chp.italic, base.italic, b),
                0x0837 => chp.strike = toggle(chp.strike, base.strike, b),
                0x0838 => chp.outline = toggle(chp.outline, base.outline, b),
                0x0839 => chp.shadow = toggle(chp.shadow, base.shadow, b),
                0x083A => chp.smallcaps = toggle(chp.smallcaps, base.smallcaps, b),
                0x083B => chp.caps = toggle(chp.caps, base.caps, b),
                0x083C => chp.vanish = toggle(chp.vanish, base.vanish, b),
                0x2A53 => chp.dstrike = toggle(chp.dstrike, base.dstrike, b),
                0x2A3E => chp.kul = b,
                0x2A48 => chp.iss = b,
                0x4A43 => {
                    if (2..=3276).contains(&w) {
                        chp.hps = w
                    }
                }
                0x4A4F | 0x4A3D => chp.ftc = w,
                0x0855 => chp.spec = b != 0,
                0x6A09 => {
                    chp.symbol = u16_at(op, 2);
                    chp.ftc = w;
                }
                0x4A30 => {
                    let keep = chp.clone();
                    self.apply_char_style(chp, base, w, depth);
                    (chp.del, chp.ins, chp.spec) = (keep.del, keep.ins, keep.spec);
                }
                0x2A33 => {
                    // sprmCPlain: back to the paragraph style's properties
                    let keep = chp.clone();
                    *chp = base.clone();
                    (chp.del, chp.ins, chp.spec) = (keep.del, keep.ins, keep.spec);
                }
                _ => {}
            }
        }
    }

    fn apply_papx(&self, pap: &mut Pap, grp: &[u8]) {
        for (code, op) in sprms(grp, self.w6) {
            let b = op.first().copied().unwrap_or(0);
            match code {
                0x2403 | 0x2461 => pap.jc = b,
                0x2416 => pap.in_table = b != 0,
                0x2417 => pap.ttp = b != 0,
                0x244B => pap.inner_cell = b != 0,
                0x244C => pap.inner_ttp = b != 0,
                0xD608 => {
                    // cb (2), itcMac (1), rgdxaCenter[itcMac + 1] (i16)
                    let n = op.get(2).copied().unwrap_or(0) as usize;
                    let bounds: Vec<i32> = (0..=n).map_while(|k| u16_at(op, 3 + 2 * k)).map(|v| v as i16 as i32).collect();
                    if bounds.len() >= 2 {
                        pap.cols = widths_from_bounds(&bounds[1..], bounds[0]);
                    }
                }
                _ => {}
            }
        }
    }

    fn para_style_pap(&self, istd: u16, depth: u32) -> Pap {
        let Some(st) = self.style(istd) else { return Pap::default() };
        let mut pap = if depth < 12 && st.base != 0x0FFF && st.base != istd { self.para_style_pap(st.base, depth + 1) } else { Pap::default() };
        let g = st.papx.clone();
        self.apply_papx(&mut pap, &g);
        pap
    }

    fn find_run<T>(runs: &[T], fc: u32, key: impl Fn(&T) -> (u32, u32)) -> Option<usize> {
        let i = runs.partition_point(|r| key(r).0 <= fc);
        if i == 0 {
            return None;
        }
        let (s, e) = key(&runs[i - 1]);
        (fc >= s && fc < e).then_some(i - 1)
    }

    fn pap_at(&self, fc: u32) -> (u16, Pap) {
        match Self::find_run(&self.papx, fc, |r| (r.s, r.e)) {
            Some(i) => {
                let r = &self.papx[i];
                let mut pap = self.para_style_pap(r.istd, 0);
                self.apply_papx(&mut pap, &r.grp);
                (r.istd, pap)
            }
            None => (0, self.para_style_pap(0, 0)),
        }
    }

    fn chp_at(&self, fc: u32, istd: u16) -> Chp {
        let run = Self::find_run(&self.chpx, fc, |r| (r.s, r.e)).unwrap_or(usize::MAX);
        if let Some(c) = self.chp_cache.borrow().get(&(run, istd)) {
            return c.clone();
        }
        let base = self.para_style_chp(istd, 0);
        let mut chp = base.clone();
        if let Some(r) = self.chpx.get(run) {
            self.apply_chpx(&mut chp, &base, &r.grp, 0);
        }
        self.chp_cache.borrow_mut().insert((run, istd), chp.clone());
        chp
    }

    fn font_name(&self, ftc: u16) -> String {
        self.fonts.get(ftc as usize).map(|f| f.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Times New Roman".into())
    }

    fn font_spec(&self, chp: &Chp) -> FontSpec {
        FontSpec { name: self.font_name(chp.ftc), size_cpt: chp.hps as u32 * 50 }
    }
}

struct Emit {
    els: Vec<El>,
    attrs: AttrSet,
    font: FontSpec,
    just: Justify,
    fields: Vec<bool>,
    pending_tab: bool,
    /// justification of the current paragraph; the code is only written
    /// once the paragraph has content
    want_just: Justify,
    /// current paragraph is in a (top level) table
    intbl: bool,
    tbl: TableState,
    text_width: i32,
    rev: Option<(bool, String, String)>,
}

fn want_attrs(chp: &Chp) -> AttrSet {
    let mut w = AttrSet::default();
    w.set(Attr::Bold, chp.bold);
    w.set(Attr::Italic, chp.italic);
    w.set(Attr::Underline, chp.kul != 0 && chp.kul != 3);
    w.set(Attr::DblUnderline, chp.kul == 3);
    w.set(Attr::Strikeout, chp.strike || chp.dstrike);
    w.set(Attr::SmallCaps, chp.smallcaps);
    w.set(Attr::Superscript, chp.iss == 1);
    w.set(Attr::Subscript, chp.iss == 2);
    w.set(Attr::Outline, chp.outline);
    w.set(Attr::Shadow, chp.shadow);
    w
}

fn jc_just(jc: u8) -> Justify {
    match jc {
        1 => Justify::Center,
        2 => Justify::Right,
        3 | 4 | 5 | 7 | 8 => Justify::Full,
        _ => Justify::Left,
    }
}

impl Word {
    /// Convert the CP range [cp0, cp1) into WP elements.
    fn emit(&self, cp0: usize, cp1: usize, main: bool, init_font: &FontSpec) -> Vec<El> {
        let mut em = Emit { els: vec![], attrs: AttrSet::default(), font: init_font.clone(), just: Justify::Left, fields: vec![], pending_tab: false, want_just: Justify::Left, intbl: false, tbl: TableState::default(), text_width: self.text_width, rev: None };
        let cp1 = cp1.min(self.text.len());
        let mut para_start = true;
        let mut istd = 0u16;
        let mut pap = Pap::default();
        let mut i = cp0;
        while i < cp1 {
            let tc = self.text[i];
            if para_start {
                para_start = false;
                let mut j = i;
                while j < cp1 && !matches!(self.text[j].u, 0x0D | 0x07) {
                    j += 1;
                }
                let fc = self.text.get(j.min(cp1.saturating_sub(1))).map(|t| t.fc).unwrap_or(tc.fc);
                (istd, pap) = self.pap_at(fc);
                if !pap.in_table {
                    em.want_just = jc_just(pap.jc);
                }
                em.intbl = pap.in_table;
            }
            let cp = i;
            i += 1;
            let u = tc.u;
            if matches!(u, 0x0D | 0x07) {
                para_start = true;
            }
            let chp = self.chp_at(tc.fc, istd);
            let rev = if chp.del {
                Some((true, self.rev_author(chp.ibst_del), dttm_iso(chp.dttm_del)))
            } else if chp.ins {
                Some((false, self.rev_author(chp.ibst), dttm_iso(chp.dttm)))
            } else {
                None
            };
            // fields: keep only the result
            match u {
                0x13 => {
                    em.fields.push(false);
                    continue;
                }
                0x14 => {
                    if let Some(f) = em.fields.last_mut() {
                        *f = true;
                    }
                    continue;
                }
                0x15 => {
                    em.fields.pop();
                    continue;
                }
                _ => {}
            }
            if em.fields.iter().any(|f| !*f) {
                continue;
            }
            if chp.vanish && u != 0x07 && u != 0x0D {
                continue;
            }
            if main {
                if let Some(&(kind, s, e)) = self.notes.get(&cp) {
                    let mut els = self.emit(s, e, false, init_font);
                    if let Some(p) = els.iter().position(|e| e.is_char()) {
                        if els[p] == El::Ch(' ') {
                            els.remove(p);
                        }
                    }
                    while matches!(els.last(), Some(El::Code(Code::HRt))) {
                        els.pop();
                    }
                    flush(&mut em, true);
                    sync_rev(&mut em, &rev);
                    em.els.push(El::Code(Code::Note { kind, els }));
                    continue;
                }
                if let Some(&(s, e)) = self.annots.get(&cp) {
                    let els = self.emit(s, e, false, init_font);
                    let t = Document { els, ..Default::default() };
                    let txt = t.plain_text(0, t.len());
                    let txt = txt.trim();
                    if !txt.is_empty() {
                        flush(&mut em, true);
                        em.els.push(El::Code(Code::Comment(txt.to_string())));
                    }
                    continue;
                }
            }
            let push_text = |em: &mut Emit, c: char| {
                flush(em, true);
                sync_rev(em, &rev);
                sync_attrs(&mut em.els, &mut em.attrs, &mut em.font, want_attrs(&chp), self.font_spec(&chp));
                if chp.caps {
                    em.els.extend(c.to_uppercase().map(El::Ch));
                } else {
                    em.els.push(El::Ch(c));
                }
            };
            let push_code = |em: &mut Emit, code: Code, sync: bool| {
                flush(em, sync);
                sync_rev(em, &rev);
                if sync {
                    sync_attrs(&mut em.els, &mut em.attrs, &mut em.font, want_attrs(&chp), self.font_spec(&chp));
                }
                em.els.push(El::Code(code));
            };
            match u {
                0x0D => {
                    if pap.inner_cell {
                        flush(&mut em, false);
                        em.pending_tab = true;
                    } else if pap.inner_ttp {
                        em.pending_tab = false;
                        em.els.push(El::Code(Code::HRt));
                    } else {
                        push_code(&mut em, Code::HRt, false);
                    }
                }
                0x07 if em.tbl.in_tbl || pap.in_table => {
                    sync_rev(&mut em, &None);
                    if pap.ttp {
                        em.tbl.row_end(&mut em.els, &pap.cols);
                    } else {
                        flush(&mut em, false);
                        em.tbl.cell_end();
                    }
                }
                0x07 => {
                    // no paragraph properties (damaged / old file): flatten
                    if em.pending_tab {
                        em.pending_tab = false;
                        em.els.push(El::Code(Code::HRt));
                    } else {
                        flush(&mut em, false);
                        em.pending_tab = true;
                    }
                }
                0x0C => {
                    let continuous = main && self.sect_marks.get(&cp).copied().unwrap_or(false);
                    push_code(&mut em, if continuous { Code::HRt } else { Code::HPg }, false);
                }
                0x0B | 0x0E => push_code(&mut em, Code::HRt, false),
                0x09 => push_code(&mut em, Code::Tab, true),
                0x1E => push_code(&mut em, Code::HardHyphen, true),
                0x1F => push_code(&mut em, Code::SoftHyphen, true),
                0xA0 if !tc.raw => push_code(&mut em, Code::HardSpace, true),
                0x28 if chp.spec && chp.symbol.is_some() => {
                    let s = chp.symbol.unwrap_or(0x28) as u32;
                    let s = crate::rtf::symbol_char(&self.font_name(chp.ftc), s).map(|c| c as u32).unwrap_or(s);
                    if let Some(c) = char::from_u32(s) {
                        push_text(&mut em, c);
                    }
                }
                0x00..=0x1F => {} // pictures, objects, note marks, …
                0x28 if chp.spec => {}
                _ => {
                    let c = if tc.raw {
                        let cp = self.fonts.get(chp.ftc as usize).and_then(|f| f.cp).unwrap_or(1252);
                        let fname = self.fonts.get(chp.ftc as usize).map(|f| f.name.as_str()).unwrap_or("");
                        let c = crate::rtf::symbol_char(fname, u as u32).unwrap_or_else(|| decode_byte(u as u8, cp));
                        if c == '\u{a0}' {
                            push_code(&mut em, Code::HardSpace, true);
                            continue;
                        }
                        Some(c)
                    } else if (0xD800..0xDC00).contains(&u) {
                        let lo = self.text.get(i).map(|t| t.u).unwrap_or(0);
                        if (0xDC00..0xE000).contains(&lo) && i < cp1 {
                            i += 1;
                            char::from_u32(0x10000 + (((u as u32) - 0xD800) << 10) + (lo as u32 - 0xDC00))
                        } else {
                            None
                        }
                    } else {
                        let fname = self.fonts.get(chp.ftc as usize).map(|f| f.name.as_str()).unwrap_or("");
                        crate::rtf::symbol_char(fname, u as u32).or_else(|| char::from_u32(u as u32))
                    };
                    if let Some(c) = c.filter(|c| !matches!(c, '\u{200b}' | '\u{feff}')) {
                        push_text(&mut em, c);
                    }
                }
            }
        }
        sync_rev(&mut em, &None);
        let tw = em.text_width;
        em.tbl.close(&mut em.els, tw);
        finish_els(em.els, em.attrs)
    }

    fn rev_author(&self, ibst: u16) -> String {
        self.rev_authors.get(ibst as usize).cloned().unwrap_or_default()
    }
}

/// Word DTTM → ISO 8601 date (empty when unset).
fn dttm_iso(d: u32) -> String {
    if d == 0 {
        return String::new();
    }
    let (mi, hr, dom, mon, yr) = (d & 0x3F, (d >> 6) & 0x1F, (d >> 11) & 0x1F, (d >> 16) & 0xF, 1900 + ((d >> 20) & 0x1FF));
    format!("{yr:04}-{mon:02}-{dom:02}T{hr:02}:{mi:02}:00Z")
}

fn sync_rev(em: &mut Emit, want: &Option<(bool, String, String)>) {
    if em.rev == *want {
        return;
    }
    if let Some((del, _, _)) = em.rev.take() {
        em.els.push(El::Code(Code::RevOff { del }));
    }
    if let Some((del, author, date)) = want.clone() {
        em.els.push(El::Code(Code::RevOn { del, author: author.clone(), date: date.clone() }));
        em.rev = Some((del, author, date));
    }
}

/// An STTB of UTF-16 strings (extended form) or 8-bit strings.
fn sttb_strings(b: &[u8]) -> Vec<String> {
    let mut out = vec![];
    let Some(first) = u16_at(b, 0) else { return out };
    let ext = first == 0xFFFF;
    let (n, mut off) = if ext { (u16_at(b, 2).unwrap_or(0) as usize, 4) } else { (first as usize, 2) };
    let extra = u16_at(b, off).unwrap_or(0) as usize;
    off += 2;
    for _ in 0..n.min(10_000) {
        if ext {
            let Some(cch) = u16_at(b, off) else { break };
            let units: Vec<u16> = (0..cch as usize).map_while(|k| u16_at(b, off + 2 + 2 * k)).collect();
            out.push(String::from_utf16_lossy(&units));
            off += 2 + 2 * cch as usize + extra;
        } else {
            let Some(&cch) = b.get(off) else { break };
            out.push(slice(b, off + 1, cch as usize).iter().map(|&c| decode_byte(c, 1252)).collect());
            off += 1 + cch as usize + extra;
        }
    }
    out
}

fn flush(em: &mut Emit, content: bool) {
    let tw = em.text_width;
    let intbl = em.intbl;
    em.tbl.sync(&mut em.els, intbl, &[], tw);
    if em.pending_tab {
        em.pending_tab = false;
        em.els.push(El::Code(Code::Tab));
    }
    if content && em.want_just != em.just {
        em.els.push(El::Code(Code::Just(em.want_just)));
        em.just = em.want_just;
    }
}

/// A PLC of CPs: n+1 CPs followed by n data items of `cb` bytes.
fn plc_cps(plc: &[u8], cb: usize) -> Vec<u32> {
    if plc.len() < 4 {
        return vec![];
    }
    let n = (plc.len() - 4) / (4 + cb);
    (0..=n).filter_map(|i| u32_at(plc, 4 * i)).collect()
}

fn twips_to_wpu(t: i32) -> i32 {
    (t as i64 * 1200 / 1440) as i32
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let cfb = Cfb::parse(bytes)?;
    if cfb.stream("EncryptedPackage").is_some() || cfb.stream("EncryptionInfo").is_some() {
        return Err("Document is password protected".into());
    }
    let Some(wd) = cfb.stream("WordDocument") else {
        return Err("not a Word document (no WordDocument stream)".into());
    };
    let ident = u16_at(&wd, 0).ok_or("damaged Word document")?;
    let nfib = u16_at(&wd, 2).ok_or("damaged Word document")?;
    // Word 6/95 files from some (e.g. East Asian) versions carry other
    // wIdent values (0xA697, 0xA699): trust nFib for those.
    if ident != 0xA5EC && ident != 0xA5DC && !(101..=105).contains(&nfib) {
        return Err("unsupported Word version".into());
    }
    let flags = u16_at(&wd, 0x0A).unwrap_or(0);
    if flags & 0x0100 != 0 {
        return Err("Document is password protected".into());
    }
    // Word 6.0 = 101, Word 95 = 104, Word 97 and later >= 193
    let w6 = nfib <= 105;
    if nfib < 101 {
        return Err("Word for Windows 2 and older documents are not supported".into());
    }

    // FIB: character counts and fc/lcb pairs
    let (ccp, fclcb_base): ([u32; 8], usize) = if w6 {
        let c = |o| u32_at(&wd, o).unwrap_or(0);
        ([c(0x34), c(0x38), c(0x3C), c(0x40), c(0x44), c(0x48), c(0x4C), c(0x50)], 0x58)
    } else {
        let csw = u16_at(&wd, 32).unwrap_or(14) as usize;
        let lw = 34 + csw * 2;
        let cslw = u16_at(&wd, lw).unwrap_or(22) as usize;
        let rglw = lw + 2;
        let c = |i: usize| u32_at(&wd, rglw + 4 * i).unwrap_or(0);
        ([c(3), c(4), c(5), c(6), c(7), c(8), c(9), c(10)], rglw + cslw * 4 + 2)
    };
    let [ccp_text, ccp_ftn, ccp_hdd, ccp_mcr, ccp_atn, ccp_edn, _, _] = ccp.map(|v| v.min(50_000_000) as usize);
    let fclcb = |i: usize| -> (usize, usize) {
        let o = fclcb_base + 8 * i;
        (u32_at(&wd, o).unwrap_or(0) as usize, u32_at(&wd, o + 4).unwrap_or(0) as usize)
    };
    let table_owned;
    let tbl: &[u8] = if w6 {
        &wd
    } else {
        let name = if flags & 0x0200 != 0 { "1Table" } else { "0Table" };
        table_owned = cfb.stream(name).ok_or("damaged Word document (no table stream)")?;
        &table_owned
    };
    let part = |i: usize| -> &[u8] {
        let (fc, lcb) = fclcb(i);
        if lcb == 0 { &[] } else { slice(tbl, fc, lcb) }
    };

    // ---- text via the piece table
    let total_cp: usize = ccp.iter().map(|&v| v.min(50_000_000) as usize).sum::<usize>() + 1;
    let mut text: Vec<TChar> = Vec::new();
    let clx = part(33);
    let mut have_clx = false;
    {
        let mut p = 0;
        let mut guard = 0;
        while p < clx.len() && guard < 100_000 {
            guard += 1;
            match clx[p] {
                1 => {
                    let cb = u16_at(clx, p + 1).unwrap_or(0) as usize;
                    p += 3 + cb;
                }
                2 => {
                    let lcb = u32_at(clx, p + 1).unwrap_or(0) as usize;
                    let plc = slice(clx, p + 5, lcb);
                    if plc.len() >= 4 {
                        have_clx = true;
                        let n = (plc.len() - 4) / 12;
                        for k in 0..n {
                            let (Some(a), Some(b)) = (u32_at(plc, 4 * k), u32_at(plc, 4 * k + 4)) else { break };
                            let pcd = 4 * (n + 1) + 8 * k;
                            let Some(fc) = u32_at(plc, pcd + 2) else { break };
                            let len = (b.saturating_sub(a) as usize).min(total_cp.saturating_sub(text.len()));
                            if w6 {
                                let off = fc as usize;
                                for q in 0..len {
                                    let Some(&byte) = wd.get(off + q) else { break };
                                    text.push(TChar { u: byte as u16, fc: (off + q) as u32, raw: true });
                                }
                            } else if fc & 0x4000_0000 != 0 {
                                let off = ((fc & 0x3FFF_FFFF) / 2) as usize;
                                for q in 0..len {
                                    let Some(&byte) = wd.get(off + q) else { break };
                                    let c = decode_byte(byte, 1252) as u32 as u16;
                                    text.push(TChar { u: c, fc: (off + q) as u32, raw: false });
                                }
                            } else {
                                let off = fc as usize;
                                for q in 0..len {
                                    let Some(u) = u16_at(&wd, off + 2 * q) else { break };
                                    text.push(TChar { u, fc: (off + 2 * q) as u32, raw: false });
                                }
                            }
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
    }
    if !have_clx {
        if !w6 {
            return Err("damaged Word document (no piece table)".into());
        }
        let fc_min = u32_at(&wd, 0x18).unwrap_or(0) as usize;
        for q in 0..total_cp {
            let Some(&byte) = wd.get(fc_min + q) else { break };
            text.push(TChar { u: byte as u16, fc: (fc_min + q) as u32, raw: true });
        }
    }

    // ---- formatting runs from the FKPs
    let read_bte = |idx: usize| -> Vec<usize> {
        let plc = part(idx);
        if plc.len() < 4 {
            return vec![];
        }
        let cb = if w6 { 2 } else { 4 };
        let n = (plc.len() - 4) / (4 + cb);
        let mut pns: Vec<usize> = (0..n)
            .filter_map(|i| {
                let o = 4 * (n + 1) + cb * i;
                if w6 { u16_at(plc, o).map(|v| v as usize) } else { u32_at(plc, o).map(|v| (v & 0x3F_FFFF) as usize) }
            })
            .collect();
        pns.dedup();
        pns
    };
    let mut chpx: Vec<ChpxRun> = vec![];
    for pn in read_bte(12) {
        let page = slice(&wd, pn * 512, 512);
        if page.len() < 512 {
            continue;
        }
        let crun = page[511] as usize;
        if 4 * (crun + 1) + crun > 511 {
            continue;
        }
        for j in 0..crun {
            let (Some(s), Some(e)) = (u32_at(page, 4 * j), u32_at(page, 4 * j + 4)) else { break };
            let b = page[4 * (crun + 1) + j] as usize;
            let grp = if b == 0 {
                vec![]
            } else {
                let cb = page.get(b * 2).copied().unwrap_or(0) as usize;
                slice(page, b * 2 + 1, cb).to_vec()
            };
            chpx.push(ChpxRun { s, e, grp });
        }
    }
    chpx.sort_by_key(|r| r.s);
    let mut papx: Vec<PapxRun> = vec![];
    for pn in read_bte(13) {
        let page = slice(&wd, pn * 512, 512);
        if page.len() < 512 {
            continue;
        }
        let cpara = page[511] as usize;
        let bx = if w6 { 7 } else { 13 };
        if 4 * (cpara + 1) + bx * cpara > 511 {
            continue;
        }
        for j in 0..cpara {
            let (Some(s), Some(e)) = (u32_at(page, 4 * j), u32_at(page, 4 * j + 4)) else { break };
            let b = page[4 * (cpara + 1) + bx * j] as usize * 2;
            let data: &[u8] = if b == 0 {
                &[]
            } else if w6 {
                let cw = page.get(b).copied().unwrap_or(0) as usize;
                slice(page, b + 1, 2 * cw)
            } else {
                let cb = page.get(b).copied().unwrap_or(0) as usize;
                if cb == 0 {
                    let cb2 = page.get(b + 1).copied().unwrap_or(0) as usize;
                    slice(page, b + 2, 2 * cb2)
                } else {
                    slice(page, b + 1, 2 * cb - 1)
                }
            };
            let istd = u16_at(data, 0).unwrap_or(0);
            papx.push(PapxRun { s, e, istd, grp: data.get(2..).unwrap_or(&[]).to_vec() });
        }
    }
    papx.sort_by_key(|r| r.s);

    // ---- stylesheet
    let mut styles: Vec<Option<Style>> = vec![];
    let default_ftc;
    {
        let st = part(1);
        let cb_stshi = u16_at(st, 0).unwrap_or(0) as usize;
        let cstd = u16_at(st, 2).unwrap_or(0) as usize;
        let cb_base = u16_at(st, 4).unwrap_or(10) as usize;
        default_ftc = u16_at(st, 2 + 12).unwrap_or(0);
        let mut off = 2 + cb_stshi;
        for _ in 0..cstd.min(4000) {
            let Some(cb) = u16_at(st, off) else { break };
            let cb = cb as usize;
            let std = slice(st, off + 2, cb);
            off += 2 + cb;
            if cb == 0 || std.len() < 6 {
                styles.push(None);
                continue;
            }
            let w2 = u16_at(std, 2).unwrap_or(0);
            let stk = (w2 & 0xF) as u8;
            let base = w2 >> 4;
            let cupx = (u16_at(std, 4).unwrap_or(0) & 0xF) as usize;
            let mut p = if w6 {
                let n = std.get(cb_base).copied().unwrap_or(0) as usize;
                cb_base + 1 + n + 1
            } else {
                let n = u16_at(std, cb_base).unwrap_or(0) as usize;
                cb_base + 2 + 2 * n + 2
            };
            let mut upx: Vec<Vec<u8>> = vec![];
            for _ in 0..cupx.min(3) {
                if p % 2 == 1 {
                    p += 1;
                }
                let n = u16_at(std, p).unwrap_or(0) as usize;
                upx.push(slice(std, p + 2, n).to_vec());
                p += 2 + n;
            }
            let mut s = Style { stk, base, ..Default::default() };
            match (stk, upx.len()) {
                (1, 2..) => {
                    s.papx = upx[0].get(2..).unwrap_or(&[]).to_vec();
                    s.chpx = upx[1].clone();
                }
                (2, 1..) => s.chpx = upx[0].clone(),
                (3, 3..) if !w6 => {
                    s.papx = upx[1].get(2..).unwrap_or(&[]).to_vec();
                    s.chpx = upx[2].clone();
                }
                _ => {}
            }
            styles.push(Some(s));
        }
    }

    // ---- font table
    let mut fonts: Vec<Font> = vec![];
    {
        let ft = part(15);
        let ffn_cp = |chs: u8| crate::rtf::charset_cp(chs as i32);
        let w6_lid_cp = match u16_at(&wd, 6).unwrap_or(0) {
            0x0419 | 0x0422 | 0x0402 | 0x0423 | 0x0C1A | 0x042F => Some(1251),
            0x0415 | 0x0405 | 0x040E | 0x041B | 0x0424 | 0x041A | 0x0418 | 0x041C => Some(1250),
            _ => None,
        };
        if w6 {
            let total = (u16_at(ft, 0).unwrap_or(0) as usize).min(ft.len());
            let mut off = 2;
            while off < total && fonts.len() < 2000 {
                let cb = ft[off] as usize + 1;
                let ffn = slice(ft, off, cb);
                off += cb;
                let name: String = ffn.get(6..).unwrap_or(&[]).iter().take_while(|&&b| b != 0).map(|&b| decode_byte(b, 1252)).collect();
                // Word 6/95 from Russian / Central European Windows: ANSI
                // fonts hold text in the system code page — use the
                // document language to pick it
                let chs = ffn.get(4).copied().unwrap_or(0);
                let cp = match (chs, w6_lid_cp) {
                    (0, Some(cp)) if !crate::rtf::is_symbol_font(&name) => Some(cp),
                    _ => ffn_cp(chs),
                };
                fonts.push(Font { name, cp });
            }
        } else {
            let mut off = 0;
            let mut n = u16_at(ft, 0).unwrap_or(0) as usize;
            off += 2;
            if n == 0xFFFF {
                n = u16_at(ft, 2).unwrap_or(0) as usize;
                off += 2;
            }
            off += 2; // cbExtra
            for _ in 0..n.min(2000) {
                let Some(&m1) = ft.get(off) else { break };
                let cb = m1 as usize + 1;
                let ffn = slice(ft, off, cb);
                off += cb;
                let units: Vec<u16> = (0..).map_while(|k| u16_at(ffn, 40 + 2 * k)).take_while(|&u| u != 0).collect();
                fonts.push(Font { name: String::from_utf16_lossy(&units), cp: ffn.get(4).and_then(|&c| ffn_cp(c)) });
            }
        }
    }

    let mut w = Word {
        w6,
        text,
        chpx,
        papx,
        styles,
        fonts,
        default_ftc,
        notes: HashMap::new(),
        annots: HashMap::new(),
        sect_marks: HashMap::new(),
        chp_cache: Default::default(),
        rev_authors: if w6 { vec![] } else { sttb_strings(part(51)) },
        text_width: 0,
    };

    // ---- footnotes / endnotes / annotations
    let ftn_base = ccp_text;
    let hdd_base = ccp_text + ccp_ftn;
    let atn_base = hdd_base + ccp_hdd + ccp_mcr;
    let edn_base = atn_base + ccp_atn;
    let mut notes_of = |ref_idx: usize, txt_idx: usize, ref_cb: usize, base: usize, len: usize, kind: Option<NoteKind>| {
        let refs = plc_cps(part(ref_idx), ref_cb);
        let txt = plc_cps(part(txt_idx), 0);
        for (k, &r) in refs.iter().enumerate().take(refs.len().saturating_sub(1)) {
            let (Some(&a), Some(&b)) = (txt.get(k), txt.get(k + 1)) else { break };
            let (a, b) = (a as usize, (b as usize).min(len));
            if a >= b || (r as usize) >= ccp_text {
                continue;
            }
            match kind {
                Some(kind) => {
                    w.notes.insert(r as usize, (kind, base + a, base + b));
                }
                None => {
                    w.annots.insert(r as usize, (base + a, base + b));
                }
            }
        }
    };
    notes_of(2, 3, 2, ftn_base, ccp_ftn, Some(NoteKind::Foot));
    if !w6 {
        notes_of(46, 47, 2, edn_base, ccp_edn, Some(NoteKind::End));
        notes_of(4, 5, 30, atn_base, ccp_atn, None);
    } else {
        // Word 6/95 annotations: ATRD is 20 bytes (8-bit initials)
        notes_of(4, 5, 20, atn_base, ccp_atn, None);
    }

    // ---- sections: page setup and section break kinds
    let mut d = Document::new();
    d.init.justify = Justify::Left;
    let sed = part(6);
    let sect_cps = plc_cps(sed, 12);
    let nsect = sect_cps.len().saturating_sub(1).min(10_000);
    let mut sect_grpf: Vec<u8> = vec![];
    for k in 0..nsect {
        let fc_sepx = u32_at(sed, 4 * (nsect + 1) + 12 * k + 2).unwrap_or(u32::MAX);
        let grp: &[u8] = if fc_sepx == u32::MAX {
            &[]
        } else {
            let cb = u16_at(&wd, fc_sepx as usize).unwrap_or(0) as usize;
            slice(&wd, fc_sepx as usize + 2, cb)
        };
        let mut continuous = false;
        let mut grpf = 0u8;
        let (mut pw, mut ph, mut ml, mut mr, mut mt, mut mb) = (12240, 15840, 1800, 1800, 1440, 1440);
        for (code, op) in sprms(grp, w6) {
            let v = u16_at(op, 0).unwrap_or(0);
            match code {
                0x3009 => continuous = op.first() == Some(&0),
                0x3013 => grpf = op.first().copied().unwrap_or(0),
                0xB01F => pw = v as i32,
                0xB020 => ph = v as i32,
                0xB021 => ml = v as i32,
                0xB022 => mr = v as i32,
                0x9023 => mt = (v as i16 as i32).abs(),
                0x9024 => mb = (v as i16 as i32).abs(),
                _ => {}
            }
        }
        sect_grpf.push(grpf);
        if k == 0 {
            let ok = |v: i32| v > 0 && v < 1440 * 60;
            if ok(pw) && ok(ph) {
                d.init.page_width = twips_to_wpu(pw);
                d.init.page_height = twips_to_wpu(ph);
            }
            if ml >= 0 && mr >= 0 && ml + mr < pw {
                d.init.left = twips_to_wpu(ml);
                d.init.right = twips_to_wpu(mr);
            }
            if mt + mb < ph {
                d.init.top = twips_to_wpu(mt);
                d.init.bottom = twips_to_wpu(mb);
            }
        } else if let Some(&end) = sect_cps.get(k) {
            // the section mark that ends section k-1 is at cp end-1
            if end > 0 {
                w.sect_marks.insert(end as usize - 1, continuous);
            }
        }
    }

    w.text_width = d.init.page_width - d.init.left - d.init.right;

    // ---- base font: the Normal style
    let normal = w.para_style_chp(0, 0);
    d.init.font = w.font_spec(&normal);
    let init_font = d.init.font.clone();

    // ---- headers / footers (primary = odd/default page of the first
    // section that has one)
    let mut hf: Vec<El> = vec![];
    if ccp_hdd > 0 {
        let hdd = plc_cps(part(11), 0);
        if w6 && sect_grpf.iter().all(|&g| g & 0x3F == 0) && part(31).get(1).copied().unwrap_or(0) & 0x3F == 0 {
            // stories without any flags (seen in real Word 6 files): assume
            // the usual header/footer stories (the last one is a guard)
            let n = hdd.len().saturating_sub(2);
            let g = match n {
                0 => 0,
                1 => 0x02,
                2 => 0x0A,
                3 => 0x0B,
                _ => 0x3F,
            };
            if sect_grpf.is_empty() {
                sect_grpf.push(g);
            } else {
                sect_grpf[0] = g;
            }
        }
        // story index in plcfhdd of (section, kind); kinds: 0 even header,
        // 1 odd header, 2 even footer, 3 odd footer, 4 first header,
        // 5 first footer
        let story = |sect: usize, kind: usize| -> Option<usize> {
            if !w6 {
                return Some(6 + 6 * sect + kind);
            }
            // Word 6/95: only the stories that exist are stored — first the
            // note separators flagged in the DOP, then per section the
            // stories flagged in its grpfIhdt.
            let dop = part(31);
            let mut idx = (dop.get(1).copied().unwrap_or(0) & 0x3F).count_ones() as usize;
            for g in sect_grpf.iter().take(sect) {
                idx += (g & 0x3F).count_ones() as usize;
            }
            let g = *sect_grpf.get(sect)?;
            if g & (1 << kind) == 0 {
                return None;
            }
            Some(idx + (g & ((1u8 << kind) - 1)).count_ones() as usize)
        };
        let nsec = if w6 { sect_grpf.len() } else { hdd.len().saturating_sub(7) / 6 };
        for (which, kind) in [(0u8, 1usize), (2, 3)] {
            for sect in 0..nsec.max(1) {
                let Some(s) = story(sect, kind) else { continue };
                if s + 1 >= hdd.len() {
                    break;
                }
                let (a, b) = (hdd[s] as usize, (hdd[s + 1] as usize).min(ccp_hdd));
                if a < b {
                    let mut els = w.emit(hdd_base + a, hdd_base + b, false, &init_font);
                    while matches!(els.last(), Some(El::Code(Code::HRt))) {
                        els.pop();
                    }
                    if els.iter().any(|e| e.is_char()) {
                        hf.push(El::Code(Code::HeadFoot { which, occ: Occurrence::Every, els }));
                    }
                    break;
                }
            }
        }
    }

    // ---- main text
    let mut els = w.emit(0, ccp_text.min(w.text.len()), true, &init_font);
    while matches!(els.last(), Some(El::Code(Code::HRt))) {
        els.pop();
    }
    if let Some(El::Code(Code::Just(j))) = els.first().cloned() {
        d.init.justify = j;
        els.remove(0);
    }
    d.els = hf;
    d.els.extend(els);
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(els: &[El]) -> String {
        let d = Document { els: els.to_vec(), ..Default::default() };
        d.plain_text(0, d.len())
    }

    #[test]
    fn sample_doc() {
        let Ok(b) = std::fs::read("tests/data/sample.doc") else { return };
        let d = read(&b).unwrap();
        let t = text(&d.els);
        eprintln!("{t}");
        eprintln!("{:?}", d.els);
        assert!(t.contains("Zentrierte Überschrift"), "{t}");
        assert!(t.contains("Grüße aus München: ÄÖÜäöüß."), "{t}");
        assert!(t.contains("Russisch: Привет мир"), "{t}");
        assert_eq!(crate::rtf::tests::table_text(&d.els), "Zelle A1|Zelle B1/Zelle A2|Zelle B2");
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::TblDef { cols }) if cols.len() == 2 && cols.iter().all(|w| (2000..3000).contains(w)))), "table widths");
        assert!(t.contains("Ende."), "{t}");
        assert_eq!(d.init.justify, Justify::Center);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Underline))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::DblUnderline))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Strikeout))));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Full))));
        let note = d.els.iter().find_map(|e| match e {
            El::Code(Code::Note { kind: NoteKind::Foot, els }) => Some(text(els)),
            _ => None,
        });
        assert_eq!(note.as_deref(), Some("Text der Fußnote."));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: 0, els, .. }) if text(els) == "Kopfzeile")));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: 2, els, .. }) if text(els) == "Fußzeile")));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::Font(f)) if f.name == "Arial" && f.size_cpt == 1400)));
        assert_eq!(d.init.page_width, 11906 * 1200 / 1440);
        assert_eq!(d.init.left, 1417 * 1200 / 1440);
    }

    fn check_revisions(d: &Document) {
        let t = text(&d.els);
        assert_eq!(t, "Alter Text eingefügt und gelöscht Ende.");
        let ons: Vec<&El> = d.els.iter().filter(|e| matches!(e, El::Code(Code::RevOn { .. }))).collect();
        assert_eq!(ons.len(), 2, "{:?}", d.els);
        assert!(matches!(ons[0], El::Code(Code::RevOn { del: false, author, date }) if author == "Maria" && date.starts_with("2010-11-11")), "{:?}", ons);
        assert!(matches!(ons[1], El::Code(Code::RevOn { del: true, author, .. }) if author == "Maria"), "{:?}", ons);
        assert_eq!(d.els.iter().filter(|e| matches!(e, El::Code(Code::RevOff { .. }))).count(), 2);
    }

    #[test]
    fn tracked_changes() {
        if let Ok(b) = std::fs::read("tests/data/revisions.doc") {
            check_revisions(&read(&b).unwrap());
        }
        if let Ok(b) = std::fs::read("tests/data/revisions.rtf") {
            check_revisions(&crate::rtf::read(&b).unwrap());
        }
    }

    #[test]
    fn rejects_non_word() {
        assert!(read(b"hello").is_err());
        let mut v = vec![0u8; 1024];
        v[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
        assert!(read(&v).is_err());
    }

    #[test]
    fn encrypted_flag() {
        let Ok(mut b) = std::fs::read("tests/data/sample.doc") else { return };
        // WordDocument usually starts at sector 0 (offset 512)
        if u16_at(&b, 512) == Some(0xA5EC) {
            b[512 + 0x0B] |= 0x01;
            assert_eq!(read(&b).err().as_deref(), Some("Document is password protected"));
        }
    }

    #[test]
    fn never_panics_on_damaged_files() {
        let Ok(b) = std::fs::read("tests/data/sample.doc") else { return };
        for cut in (0..b.len()).step_by(97) {
            let _ = read(&b[..cut]);
        }
        let mut seed: u32 = 987654321;
        for _ in 0..300 {
            let mut v = b.clone();
            for _ in 0..30 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let i = (seed as usize >> 3) % v.len();
                v[i] = (seed >> 24) as u8;
            }
            let _ = read(&v);
        }
    }

    #[test]
    fn word6_russian_tables() {
        let d = read(&std::fs::read("tests/data/formats/word6_russian_tables.doc").unwrap()).unwrap();
        let t = crate::fmt_build::debug_text(&d.els);
        assert!(t.starts_with("Компания Аскат М\nпредлагает со склада в Москве"), "{t}");
        assert!(t.contains("|Алиготе, белое п/с|"), "{t}");
        assert!(d.els.iter().filter(|e| matches!(e, El::Code(Code::TblDef { .. }))).count() >= 3);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
    }

    #[test]
    fn word6_headers_footers() {
        let d = read(&std::fs::read("tests/data/formats/word6_header_footer.doc").unwrap()).unwrap();
        let t = crate::fmt_build::debug_text(&d.els);
        assert!(t.starts_with("[Header:30  APRIL][Footer:2]"), "{t}");
        assert!(t.contains("The Holy Apostle James, Son of Zebedee"), "{t}");
    }

    #[test]
    fn word6_other_ident_and_word95() {
        let d = read(&std::fs::read("tests/data/formats/word6_ident_a697.doc").unwrap()).unwrap();
        assert!(!d.els.is_empty());
        let d = read(&std::fs::read("tests/data/formats/word95.doc").unwrap()).unwrap();
        let t = text(&d.els);
        assert!(t.contains("Paragraph 3. Has some RED text and some BLUE BOLD text in it."), "{t}");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
    }

    /// Fuzzed Word 6 files from the LibreOffice test suite and damaged
    /// copies of real Word 6 documents.
    #[test]
    fn word6_never_panics() {
        for e in std::fs::read_dir("tests/data/formats/damaged").unwrap().flatten() {
            if e.file_name().to_string_lossy().starts_with("ww6") {
                let _ = read(&std::fs::read(e.path()).unwrap());
            }
        }
        for f in ["word6_russian_tables.doc", "word6_header_footer.doc"] {
            let b = std::fs::read(format!("tests/data/formats/{f}")).unwrap();
            for cut in (0..b.len()).step_by(101) {
                let _ = read(&b[..cut]);
            }
            let mut seed: u32 = 4242;
            for _ in 0..200 {
                let mut v = b.clone();
                for _ in 0..20 {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    let i = 512 + (seed as usize >> 3) % (v.len() - 512);
                    v[i] = (seed >> 24) as u8;
                }
                let _ = read(&v);
            }
        }
    }

    #[test]
    fn client_file_section_vi() {
        let p = "/mnt/i/Claude Space/wp51/Section VI.doc";
        let p2 = "I:\\Claude Space\\wp51\\Section VI.doc";
        let Ok(b) = std::fs::read(p).or_else(|_| std::fs::read(p2)) else { return };
        let d = read(&b).unwrap();
        let t = text(&d.els);
        eprintln!("chars: {}  els: {}  init: {:?}", t.chars().count(), d.els.len(), d.init.font);
        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        for e in &d.els {
            if let El::Code(c) = e {
                let k = format!("{c:?}");
                let k = k.split(['(', ' ', '{']).next().unwrap_or("").to_string();
                *kinds.entry(k).or_default() += 1;
            }
        }
        eprintln!("codes: {kinds:?}  page {}x{} margins {} {} {} {}", d.init.page_width, d.init.page_height, d.init.left, d.init.right, d.init.top, d.init.bottom);
        eprintln!("{}", t.chars().take(1500).collect::<String>());
        assert!(t.contains("Section VI"));
        assert!(t.contains("Bordereau des quantit"));
    }
}
