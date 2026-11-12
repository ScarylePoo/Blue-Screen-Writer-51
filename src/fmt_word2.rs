//! Word for Windows 1.x / 2.0 binary document reader (no OLE container;
//! wIdent 0xA59B / 0xA59C = Word 1.x, 0xA5DB = Word 2.0).
//!
//! The layout is the ancestor of the Word 6 format: a FIB with character
//! counts and (fc, cb16) pairs, the text (from `fcMin`, or through a piece
//! table when the file was fast-saved), 512-byte FKP pages with character
//! runs (a CHP structure prefix, not sprms) and paragraph runs (style code,
//! PHE, then one-byte sprms), a section table, footnotes and the
//! header/footer story.  Styles are not interpreted: formatting stored in
//! the runs is kept; the base font is the first font of the font table.
//!
//! Written from the publicly known format facts (LibreOffice's ww8 filter
//! was used as a reference for them).  Everything is bounds-checked.

use crate::doc::*;
use crate::fmt_build::Builder;
use crate::rtf::decode_byte;

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

#[derive(Clone, Default, PartialEq)]
struct Chp {
    bold: bool,
    italic: bool,
    strike: bool,
    outline: bool,
    smallcaps: bool,
    caps: bool,
    vanish: bool,
    spec: bool,
    ftc: Option<u16>,
    hps: Option<u16>,
    kul: u8,
    pos: u8,
}

#[derive(Clone, Default)]
struct Pap {
    jc: u8,
    in_table: bool,
    ttp: bool,
    cols: Vec<i32>,
}

struct Run<T> {
    s: u32,
    e: u32,
    v: T,
}

struct Font {
    name: String,
    cp: u32,
}

struct W2<'a> {
    data: &'a [u8],
    /// (byte, fc) per cp
    text: Vec<(u8, u32)>,
    chps: Vec<Run<Chp>>,
    paps: Vec<Run<Pap>>,
    fonts: Vec<Font>,
    /// footnote reference cp → (start, end) cp of the note text
    notes: std::collections::HashMap<usize, (usize, usize)>,
    base: FontSpec,
    text_width: i32,
}

/// Operand length of a Word 2 sprm (code, rest) — None = unknown code.
fn sprm_len(op: u8, rest: &[u8]) -> Option<usize> {
    let var = || rest.first().map(|&n| 1 + n as usize);
    let var2 = || u16_at(rest, 0).map(|n| 2 + n as usize);
    Some(match op {
        0 | 52 | 58 => 0,
        3 | 15 | 23 | 57 | 78 | 95 | 155 => return var(),
        152 | 154 => return var2(),
        2 | 4..=14 | 24 | 25 | 29 | 37 | 44 | 50 | 51 | 53..=55 | 60..=67 | 69 | 73..=77 | 80 | 81 | 84..=87 | 94 => 1,
        112 | 114 | 117 | 118 | 121 | 122 | 125..=128 | 133 | 134 | 137 | 138 | 149 => 1,
        16..=22 | 26..=28 | 30..=36 | 38..=43 | 45..=49 | 68 | 71 | 72 | 82 | 83 | 96..=99 => 2,
        115 | 116 | 119 | 120 | 123 | 124 | 129..=132 | 135 | 136 | 139..=148 | 153 | 159 | 161 | 162 => 2,
        70 => 3,
        158 | 160 | 164 => 4,
        157 | 163 => 5,
        _ => return None,
    })
}

fn sprms(grp: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = vec![];
    let mut p = 0;
    while p < grp.len() && out.len() < 1000 {
        let op = grp[p];
        let Some(len) = sprm_len(op, &grp[p + 1..]) else { break };
        let e = p + 1 + len;
        if e > grp.len() {
            break;
        }
        out.push((op, &grp[p + 1..e]));
        p = e;
    }
    out
}

/// Column widths from a sprmTDefTable(10) operand (2-byte length first).
fn tdef_cols(op: &[u8]) -> Vec<i32> {
    let Some(&n) = op.get(2) else { return vec![] };
    let n = (n as usize).min(64);
    let xs: Vec<i32> = (0..=n).map_while(|k| u16_at(op, 3 + 2 * k).map(|v| v as i16 as i32)).collect();
    xs.windows(2).map(|w| ((w[1] - w[0]).max(144) as i64 * 1200 / 1440) as i32).collect()
}

fn plc_cps(plc: &[u8], cb: usize) -> Vec<u32> {
    if plc.len() < 4 {
        return vec![];
    }
    let n = (plc.len() - 4) / (4 + cb);
    (0..=n).filter_map(|i| u32_at(plc, 4 * i)).collect()
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let ident = u16_at(bytes, 0).ok_or("not a Word for Windows 2 document")?;
    if !matches!(ident, 0xA59B | 0xA59C | 0xA5DB) || bytes.len() < 0x150 {
        return Err("not a Word for Windows 1/2 document".into());
    }
    let w1 = ident != 0xA5DB;
    let flags = u16_at(bytes, 10).unwrap_or(0);
    if flags & 0x0100 != 0 {
        return Err("Document is password protected".into());
    }
    let complex = flags & 0x0004 != 0;
    let fc_min = u32_at(bytes, 0x18).unwrap_or(0) as usize;
    let ccp = |o: usize| (u32_at(bytes, o).unwrap_or(0) as usize).min(bytes.len());
    let (ccp_text, ccp_ftn, ccp_hdd) = (ccp(0x34), ccp(0x38), ccp(0x3C));
    let pair = |i: usize| -> &[u8] {
        let o = 0x58 + 6 * i;
        let fc = u32_at(bytes, o).unwrap_or(0) as usize;
        let cb = u16_at(bytes, o + 4).unwrap_or(0) as usize;
        if cb == 0 { &[] } else { slice(bytes, fc, cb) }
    };
    let total = ccp_text + ccp_ftn + ccp_hdd + 1;

    // ---- text
    let mut text: Vec<(u8, u32)> = vec![];
    // Word 1 lacks two printer-environment pairs before the piece table
    let clx = if complex { pair(if w1 { 31 } else { 33 }) } else { &[] };
    let mut have_pieces = false;
    let mut p = 0;
    let mut guard = 0;
    while p < clx.len() && guard < 10_000 {
        guard += 1;
        match clx[p] {
            1 => p += 3 + u16_at(clx, p + 1).unwrap_or(0) as usize,
            2 => {
                let len = u16_at(clx, p + 1).unwrap_or(0) as usize;
                let plc = slice(clx, p + 3, len);
                if plc.len() >= 4 {
                    have_pieces = true;
                    let n = (plc.len() - 4) / 12;
                    for k in 0..n {
                        let (Some(a), Some(b)) = (u32_at(plc, 4 * k), u32_at(plc, 4 * k + 4)) else { break };
                        let Some(fc) = u32_at(plc, 4 * (n + 1) + 8 * k + 2) else { break };
                        let len = (b.saturating_sub(a) as usize).min(total.saturating_sub(text.len()));
                        for q in 0..len {
                            let Some(&c) = bytes.get(fc as usize + q) else { break };
                            text.push((c, fc + q as u32));
                        }
                    }
                }
                break;
            }
            _ => break,
        }
    }
    if !have_pieces {
        for q in 0..total {
            let Some(&c) = bytes.get(fc_min + q) else { break };
            text.push((c, (fc_min + q) as u32));
        }
    }

    // ---- formatting runs
    let (cpn_chp, cpn_pap) = if w1 {
        (u16_at(bytes, 0x136).unwrap_or(0) as usize, u16_at(bytes, 0x138).unwrap_or(0) as usize)
    } else {
        (u16_at(bytes, 0x142).unwrap_or(0) as usize, u16_at(bytes, 0x144).unwrap_or(0) as usize)
    };
    let pages = |idx: usize, cpn: usize| -> Vec<usize> {
        let plc = pair(idx);
        if plc.len() < 4 {
            return vec![];
        }
        let n = (plc.len() - 4) / 6;
        let mut pns: Vec<usize> = (0..n).filter_map(|i| u16_at(plc, 4 * (n + 1) + 2 * i).map(|v| v as usize)).collect();
        // an incomplete BTE table: the remaining pages follow the last one
        if cpn > pns.len() && cpn < 10_000 {
            if let Some(&last) = pns.last() {
                for k in 1..=(cpn - pns.len()) {
                    pns.push(last + k);
                }
            }
        }
        pns.dedup();
        pns
    };
    let mut chps: Vec<Run<Chp>> = vec![];
    for pn in pages(12, cpn_chp) {
        let page = slice(bytes, pn * 512, 512);
        if page.len() < 512 {
            continue;
        }
        let crun = page[511] as usize;
        if 5 * crun + 4 > 511 {
            continue;
        }
        for j in 0..crun {
            let (Some(s), Some(e)) = (u32_at(page, 4 * j), u32_at(page, 4 * j + 4)) else { break };
            let off = page[4 * (crun + 1) + j] as usize * 2;
            let mut c = Chp::default();
            if off != 0 {
                let len = page.get(off).copied().unwrap_or(0) as usize;
                let x = slice(page, off + 1, len.min(511 - off.min(511)));
                let g = |i: usize| x.get(i).copied().unwrap_or(0);
                let (b0, b1, b2) = (g(0), g(1), g(2));
                c.bold = b0 & 0x01 != 0;
                c.italic = b0 & 0x02 != 0;
                c.outline = b0 & 0x08 != 0;
                c.smallcaps = b0 & 0x20 != 0;
                c.caps = b0 & 0x40 != 0;
                c.vanish = b0 & 0x80 != 0;
                c.spec = b1 & 0x02 != 0;
                c.strike = b1 & 0x04 != 0;
                if b2 & 0x02 != 0 {
                    c.ftc = u16_at(x, 4);
                }
                if b2 & 0x04 != 0 {
                    c.hps = u16_at(x, 6).filter(|&h| (4..=400).contains(&h));
                }
                if b2 & 0x08 != 0 {
                    c.kul = g(9) >> 5;
                }
                if b2 & 0x10 != 0 {
                    c.pos = g(10);
                }
            }
            chps.push(Run { s, e, v: c });
        }
    }
    chps.sort_by_key(|r| r.s);
    let mut paps: Vec<Run<Pap>> = vec![];
    for pn in pages(13, cpn_pap) {
        let page = slice(bytes, pn * 512, 512);
        if page.len() < 512 {
            continue;
        }
        let n = page[511] as usize;
        if 5 * n + 4 > 511 {
            continue;
        }
        for j in 0..n {
            let (Some(s), Some(e)) = (u32_at(page, 4 * j), u32_at(page, 4 * j + 4)) else { break };
            let off = page[4 * (n + 1) + j] as usize * 2;
            let mut pap = Pap::default();
            if off != 0 {
                let cw = page.get(off).copied().unwrap_or(0) as usize;
                let len = (cw * 2).saturating_sub(7);
                let grp = slice(page, off + 8, len.min(512usize.saturating_sub(off + 8)));
                for (op, v) in sprms(grp) {
                    match op {
                        5 => pap.jc = v.first().copied().unwrap_or(0),
                        24 => pap.in_table = v.first().is_some_and(|&x| x != 0),
                        25 => pap.ttp = v.first().is_some_and(|&x| x != 0),
                        152 | 154 => pap.cols = tdef_cols(v),
                        _ => {}
                    }
                }
            }
            paps.push(Run { s, e, v: pap });
        }
    }
    paps.sort_by_key(|r| r.s);

    // ---- fonts
    let mut fonts = vec![];
    {
        let ft = pair(15);
        let total = (u16_at(ft, 0).unwrap_or(0) as usize).min(ft.len());
        let mut off = 2;
        while off < total && fonts.len() < 1000 {
            let cb = ft[off] as usize + 1;
            let ffn = slice(ft, off, cb);
            off += cb;
            let chs = ffn.get(2).copied().unwrap_or(0);
            let cp = crate::rtf::charset_cp(chs as i32).unwrap_or(1252);
            let name: String = ffn.get(3..).unwrap_or(&[]).iter().take_while(|&&b| b != 0).map(|&b| decode_byte(b, 1252)).collect();
            fonts.push(Font { name, cp });
        }
    }

    // ---- sections: page setup, header/footer flags
    let mut d = Document::new();
    let sed = pair(6);
    let sect_cps = plc_cps(sed, 6);
    let nsect = sect_cps.len().saturating_sub(1).min(1000);
    let mut sect_grpf: Vec<u8> = vec![];
    for k in 0..nsect {
        let fcs = u32_at(sed, 4 * (nsect + 1) + 6 * k + 2).unwrap_or(u32::MAX) as usize;
        let grp: &[u8] = if fcs == u32::MAX as usize {
            &[]
        } else {
            let n = bytes.get(fcs).copied().unwrap_or(0) as usize;
            slice(bytes, fcs + 1, n)
        };
        let (mut pw, mut ph, mut ml, mut mr, mut mt, mut mb) = (12240, 15840, 1800, 1800, 1440, 1440);
        let mut grpf = 0u8;
        for (op, v) in sprms(grp) {
            let w = u16_at(v, 0).unwrap_or(0) as i16 as i32;
            match op {
                128 => grpf = v.first().copied().unwrap_or(0),
                139 => pw = w,
                140 => ph = w,
                141 => ml = w,
                142 => mr = w,
                143 => mt = w.abs(),
                144 => mb = w.abs(),
                _ => {}
            }
        }
        sect_grpf.push(grpf);
        if k == 0 {
            let tw = |t: i32| (t as i64 * 1200 / 1440) as i32;
            if pw > 1440 && ph > 1440 && pw < 1440 * 40 && ph < 1440 * 40 {
                d.init.page_width = tw(pw);
                d.init.page_height = tw(ph);
                if ml >= 0 && mr >= 0 && ml + mr < pw {
                    d.init.left = tw(ml);
                    d.init.right = tw(mr);
                }
                if mt + mb < ph {
                    d.init.top = tw(mt);
                    d.init.bottom = tw(mb);
                }
            }
        }
    }

    let base = FontSpec { name: fonts.first().map(|f| f.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Times New Roman".into()), size_cpt: 1000 };
    let mut w = W2 { data: bytes, text, chps, paps, fonts, notes: Default::default(), base: base.clone(), text_width: d.init.page_width - d.init.left - d.init.right };

    // ---- footnotes
    let refs = plc_cps(pair(2), 2);
    let txt = plc_cps(pair(3), 0);
    for (k, &r) in refs.iter().enumerate().take(refs.len().saturating_sub(1)) {
        let (Some(&a), Some(&b)) = (txt.get(k), txt.get(k + 1)) else { break };
        let (a, b) = (a as usize, (b as usize).min(ccp_ftn));
        if a < b && (r as usize) < ccp_text {
            w.notes.insert(r as usize, (ccp_text + a, ccp_text + b));
        }
    }

    // ---- headers / footers (first section that has them)
    let mut hf: Vec<El> = vec![];
    if ccp_hdd > 0 {
        let hdd = plc_cps(pair(11), 0);
        if sect_grpf.iter().all(|&g| g & 0x3F == 0) {
            let g = match hdd.len().saturating_sub(2) {
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
        let hdd_base = ccp_text + ccp_ftn;
        for (which, kind) in [(0u8, 1usize), (2, 3)] {
            let mut idx = 0usize;
            for g in &sect_grpf {
                let g = g & 0x3F;
                if g & (1 << kind) != 0 {
                    let s = idx + (g & ((1u8 << kind) - 1)).count_ones() as usize;
                    if s + 1 < hdd.len() {
                        let (a, b) = (hdd[s] as usize, (hdd[s + 1] as usize).min(ccp_hdd));
                        if a < b {
                            let els = w.emit(hdd_base + a, hdd_base + b, false);
                            if els.iter().any(|e| e.is_char()) {
                                hf.push(El::Code(Code::HeadFoot { which, occ: Occurrence::Every, els }));
                                break;
                            }
                        }
                    }
                }
                idx += g.count_ones() as usize;
            }
        }
    }

    let mut els = w.emit(0, ccp_text, true);
    d.init.font = base;
    d.init.justify = Justify::Left;
    if let Some(El::Code(Code::Just(j))) = els.first().cloned() {
        d.init.justify = j;
        els.remove(0);
    }
    d.els = hf;
    d.els.extend(els);
    Ok(d)
}

impl W2<'_> {
    fn chp_at(&self, fc: u32) -> Chp {
        let i = self.chps.partition_point(|r| r.e <= fc);
        self.chps.get(i).filter(|r| r.s <= fc).map(|r| r.v.clone()).unwrap_or_default()
    }
    fn pap_at(&self, fc: u32) -> Pap {
        let i = self.paps.partition_point(|r| r.e <= fc);
        self.paps.get(i).filter(|r| r.s <= fc).map(|r| r.v.clone()).unwrap_or_default()
    }

    fn emit(&self, cp0: usize, cp1: usize, main: bool) -> Vec<El> {
        let mut b = Builder::new(self.base.clone());
        b.text_width = self.text_width;
        let cp1 = cp1.min(self.text.len());
        let mut i = cp0;
        let mut para_start = true;
        let mut pap = Pap::default();
        let mut fields: Vec<bool> = vec![];
        // table state
        let mut need_row = true;
        let mut need_cell = false;
        let mut def_at: Option<usize> = None;
        while i < cp1 {
            let (byte, fc) = self.text[i];
            if para_start {
                para_start = false;
                let mut j = i;
                while j < cp1 && !matches!(self.text[j].0, 0x0D | 0x07) {
                    j += 1;
                }
                let efc = self.text.get(j.min(cp1.saturating_sub(1))).map(|t| t.1).unwrap_or(fc);
                pap = self.pap_at(efc);
                if pap.in_table {
                    if !b.in_table() {
                        b.table_start(pap.cols.clone());
                        def_at = b.els.iter().rposition(|e| matches!(e, El::Code(Code::TblDef { .. })));
                        need_row = true;
                    }
                } else if b.in_table() {
                    b.table_end();
                }
                if !pap.in_table {
                    b.just = match pap.jc {
                        1 => Justify::Center,
                        2 => Justify::Right,
                        3 => Justify::Full,
                        _ => Justify::Left,
                    };
                }
            }
            let cp = i;
            i += 1;
            if matches!(byte, 0x0D | 0x07) {
                para_start = true;
            }
            match byte {
                0x13 => {
                    fields.push(false);
                    continue;
                }
                0x14 => {
                    if let Some(f) = fields.last_mut() {
                        *f = true;
                    }
                    continue;
                }
                0x15 => {
                    fields.pop();
                    continue;
                }
                _ => {}
            }
            if fields.iter().any(|f| !*f) {
                continue;
            }
            let chp = self.chp_at(fc);
            if chp.vanish && !matches!(byte, 0x0D | 0x07) {
                continue;
            }
            // table cell / row structure
            if pap.in_table && b.in_table() && !(byte == 0x07 && pap.ttp) {
                if need_row {
                    b.row();
                    b.cell();
                    need_row = false;
                    need_cell = false;
                } else if need_cell && byte != 0x07 {
                    b.cell();
                    need_cell = false;
                }
            }
            let font = chp.ftc.and_then(|f| self.fonts.get(f as usize));
            let mut a = AttrSet::default();
            a.set(Attr::Bold, chp.bold);
            a.set(Attr::Italic, chp.italic);
            a.set(Attr::Strikeout, chp.strike);
            a.set(Attr::Outline, chp.outline);
            a.set(Attr::SmallCaps, chp.smallcaps);
            a.set(Attr::Underline, chp.kul != 0 && chp.kul != 3);
            a.set(Attr::DblUnderline, chp.kul == 3);
            if chp.pos != 0 {
                a.set(if chp.pos >= 128 { Attr::Subscript } else { Attr::Superscript }, true);
            }
            b.attrs = a;
            b.font = FontSpec {
                name: font.map(|f| f.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| self.base.name.clone()),
                size_cpt: chp.hps.map(|h| h as u32 * 50).unwrap_or(self.base.size_cpt),
            };
            if main {
                if let Some(&(s, e)) = self.notes.get(&cp) {
                    let mut els = self.emit(s, e, false);
                    while matches!(els.first(), Some(El::Ch(' '))) {
                        els.remove(0);
                    }
                    b.note(NoteKind::Foot, els);
                    continue;
                }
            }
            match byte {
                0x0D => {
                    if pap.in_table {
                        b.ch(' ');
                    } else {
                        b.para();
                    }
                }
                0x07 => {
                    if pap.ttp {
                        need_row = true;
                        need_cell = false;
                        if let Some(at) = def_at {
                            if let Some(El::Code(Code::TblDef { cols })) = b.els.get_mut(at) {
                                if cols.len() < pap.cols.len() {
                                    *cols = pap.cols.clone();
                                }
                            }
                        }
                    } else if b.in_table() {
                        need_cell = true;
                    } else {
                        b.code_fmt(Code::Tab);
                    }
                }
                0x0C => b.page(),
                0x0B | 0x0E => b.para(),
                0x09 => b.code_fmt(Code::Tab),
                0x1E => b.code_fmt(Code::HardHyphen),
                0x1F => b.code(Code::SoftHyphen),
                0xA0 => b.code_fmt(Code::HardSpace),
                0x00..=0x1F => {}
                _ if chp.spec => {}
                _ => {
                    let cp = font.map(|f| f.cp).unwrap_or(1252);
                    let fname = font.map(|f| f.name.as_str()).unwrap_or("");
                    let c = crate::rtf::symbol_char(fname, byte as u32).unwrap_or_else(|| decode_byte(byte, cp));
                    if chp.caps {
                        for u in c.to_uppercase() {
                            b.ch(u);
                        }
                    } else {
                        b.ch(c);
                    }
                }
            }
            let _ = self.data;
        }
        b.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    #[test]
    fn word2_sample() {
        let d = read(&std::fs::read("tests/data/formats/word2.doc").unwrap()).unwrap();
        let t = debug_text(&d.els);
        assert!(t.starts_with("[Header:\n\tPARTICIPANTS LIST"), "{t}");
        assert!(t.contains("[Footer:\t03/10/2003\tPage:"), "{t}");
        assert!(t.contains("Mr. Benni Alexander\tNokia Japan Co, Ltd"), "{t}");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
    }

    #[test]
    fn word1_fast_saved() {
        let d = read(&std::fs::read("tests/data/formats/word1.doc").unwrap()).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Wegen günstiger Einkaufsmöglichkeiten"), "{t}");
        assert!(t.contains("Aparthotel Schneefreude, Neustift, Österreich"), "{t}");
    }

    #[test]
    fn word2_never_panics() {
        for e in std::fs::read_dir("tests/data/formats/damaged").unwrap().flatten() {
            if e.file_name().to_string_lossy().starts_with("ww2") {
                let _ = read(&std::fs::read(e.path()).unwrap());
            }
        }
        let b = std::fs::read("tests/data/formats/word2.doc").unwrap();
        for cut in (0..b.len()).step_by(37) {
            let _ = read(&b[..cut]);
        }
        let mut seed: u32 = 777;
        for _ in 0..300 {
            let mut v = b.clone();
            for _ in 0..16 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let i = 2 + (seed as usize >> 3) % (v.len() - 2);
                v[i] = (seed >> 24) as u8;
            }
            let _ = read(&v);
        }
    }
}

#[cfg(test)]
mod dump {
    /// Developer aid: `cargo test dump_files -- --ignored --nocapture`
    /// prints every file listed in test-out/dump.txt.
    #[test]
    #[ignore]
    fn dump_files() {
        let list = std::fs::read_to_string("test-out/dump.txt").unwrap_or_default();
        for p in list.lines().filter(|l| !l.trim().is_empty()) {
            println!("===== {p}");
            let p = std::path::Path::new(p.trim());
            let b = std::fs::read(p).unwrap_or_default();
            let r = if b.starts_with(&[0xDB, 0xA5]) { super::read(&b).map_err(std::io::Error::other) } else { crate::fileio::load(p) };
            match r {
                Ok(d) => println!("{}\n--- codes: {:?}", crate::fmt_build::debug_text(&d.els), d.els.iter().filter(|e| !e.is_char()).take(60).collect::<Vec<_>>()),
                Err(e) => println!("ERROR {e}"),
            }
        }
    }
}
