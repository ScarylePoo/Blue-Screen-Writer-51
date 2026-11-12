//! Microsoft Write (.wri, Write 3.0 = 0xBE31, Write 3.1 = 0xBE32) and
//! Word for DOS 4/5/5.5/6 document reader.
//!
//! Both share one layout (128-byte pages): a header page, the text from
//! 0x80 to `fcMac`, then pages of character runs (CHP FODs, starting at
//! the first page after the text) and paragraph runs (PAP FODs at
//! `pnPara`), a section table and — Write — a font table, Word for DOS —
//! a footnote table.  Write text is Windows ANSI (fonts named "… Cyr" /
//! "… CE" switch to 1251 / 1250); Word for DOS uses the DOS code page
//! stored in the header (437 when unset).
//!
//! Written from the publicly documented format (and libwps as a reference
//! for the format facts).  Everything is bounds-checked.

use crate::doc::*;
use crate::fmt_build::Builder;
use crate::fmt_text::{Enc, byte_char, enc_for_codepage};

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o.checked_add(1)?)?]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    let s = b.get(o..o.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Loc {
    Main,
    /// header (false) / footer (true), occurrence
    HeadFoot(bool, Occurrence),
    /// first-page-only / never shown header or footer
    Hidden,
    Note,
    Graphics,
}

struct Pap {
    s: u32,
    e: u32,
    just: Justify,
    loc: Loc,
}

#[derive(Clone)]
struct Chp {
    s: u32,
    e: u32,
    attrs: AttrSet,
    font: FontSpec,
    caps: bool,
    special: bool,
    hidden: bool,
    /// Word for DOS: annotation reference style
    annot: bool,
    enc: Enc,
}

struct WriteDoc<'a> {
    data: &'a [u8],
    fc_mac: u32,
    paps: Vec<Pap>,
    chps: Vec<Chp>,
    /// (fcRef, fcFtn) — Word for DOS footnotes, sorted by fcFtn; the last
    /// entry may be a sentinel
    notes: Vec<(u32, u32)>,
    /// fc of section marks (0x0C) that are not page breaks
    continuous_marks: Vec<u32>,
}

/// Run through the FOD pages starting at page `pn`; calls `f(fcFirst,
/// fcLim, props)`.
fn read_fods(data: &[u8], mut pn: usize, fc_mac: u32, mut f: impl FnMut(u32, u32, &[u8])) {
    let mut fc = 0x80u32;
    let max_pages = data.len() / 128 + 1;
    let mut pages = 0;
    while pages < max_pages {
        pages += 1;
        let base = pn * 128;
        let Some(page) = data.get(base..base + 128) else { break };
        let cfod = (page[0x7F] as usize).min(20);
        if cfod == 0 {
            break;
        }
        for i in 0..cfod {
            let o = 4 + 6 * i;
            let (Some(lim), Some(bf)) = (u32_at(page, o), u16_at(page, o + 4)) else { return };
            let props: &[u8] = if (bf as usize) < 0x7F - 4 {
                let p = 4 + bf as usize;
                let cch = page[p] as usize;
                if p + 1 + cch <= 0x7F + 1 && p + 1 + cch <= 128 { &page[p + 1..p + 1 + cch] } else { &[] }
            } else {
                &[]
            };
            if lim <= fc {
                // damaged run list: stop rather than loop
                if lim < fc {
                    return;
                }
                continue;
            }
            f(fc, lim, props);
            if lim >= fc_mac {
                return;
            }
            fc = lim;
        }
        pn += 1;
    }
}

fn font_enc(name: &str) -> Enc {
    let l = name.to_ascii_lowercase();
    if l.ends_with(" cyr") || l.contains("cyrillic") {
        Enc::Cp1251
    } else if l.ends_with(" ce") {
        Enc::Cp1250
    } else {
        Enc::Cp1252
    }
}

fn dos_font(ftc: u8) -> &'static str {
    match ftc {
        0..=15 => "Courier New",
        16..=31 => "Times New Roman",
        _ => "Arial",
    }
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if bytes.len() < 0x100 || !matches!(bytes[0], 0x31 | 0x32) || bytes[1..6] != [0xBE, 0, 0, 0, 0xAB] {
        return Err("not a Write / Word for DOS document".into());
    }
    let dos = u16_at(bytes, 96) == Some(0);
    let fc_mac = u32_at(bytes, 14).unwrap_or(0);
    if fc_mac < 0x80 || fc_mac as usize > bytes.len() {
        return Err("damaged Write / Word for DOS document".into());
    }
    if dos && bytes.get(117).is_some_and(|b| b & 2 != 0) {
        return Err("this Word for DOS file is an autosave file".into());
    }
    let doc_enc = if dos {
        let cp = u16_at(bytes, 126).unwrap_or(0) as u32;
        enc_for_codepage(cp).unwrap_or(Enc::Cp437)
    } else {
        Enc::Cp1252
    };

    // ---- fonts (Write)
    let mut fonts: Vec<String> = vec![];
    if !dos {
        let mut pn = u16_at(bytes, 28).unwrap_or(0) as usize;
        let pn_mac = u16_at(bytes, 96).unwrap_or(0) as usize;
        if pn != 0 && pn != pn_mac {
            let mut off = pn * 128 + 2;
            let mut guard = 0;
            while guard < 1000 {
                guard += 1;
                let Some(cb) = u16_at(bytes, off) else { break };
                if cb == 0 {
                    break;
                }
                if cb == 0xFFFF {
                    pn += 1;
                    off = pn * 128;
                    continue;
                }
                let cb = cb as usize;
                let Some(ffn) = bytes.get(off + 2..off + 2 + cb) else { break };
                let name: Vec<u8> = ffn[1..].iter().copied().take_while(|&b| b != 0).collect();
                fonts.push(name.iter().map(|&b| byte_char(b, Enc::Cp1252)).collect());
                off += 2 + cb;
            }
        }
        if fonts.is_empty() {
            fonts.push("Arial".into());
        }
    }

    // ---- character runs
    let mut chps: Vec<Chp> = vec![];
    let chp_pn = (fc_mac as usize).div_ceil(128);
    read_fods(bytes, chp_pn, fc_mac, |s, e, p| {
        let g = |i: usize| p.get(i).copied().unwrap_or(if i == 2 { 24 } else { 0 });
        let (b1, hps, ul, b4, pos) = (g(1), g(2), g(3), g(4), g(5));
        let mut a = AttrSet::default();
        a.set(Attr::Bold, b1 & 1 != 0);
        a.set(Attr::Italic, b1 & 2 != 0);
        let mut caps = false;
        let mut hidden = false;
        let font_name;
        let mut enc = doc_enc;
        if dos {
            a.set(Attr::Underline, ul & 1 != 0 && ul & 4 == 0);
            a.set(Attr::Strikeout, ul & 2 != 0);
            a.set(Attr::DblUnderline, ul & 4 != 0);
            caps = ul & 0x30 == 0x10;
            a.set(Attr::SmallCaps, ul & 0x30 == 0x30);
            hidden = ul & 0x80 != 0;
            font_name = dos_font(b1 >> 2).to_string();
        } else {
            a.set(Attr::Underline, ul & 1 != 0);
            let ftc = ((b1 >> 2) as usize) | (((b4 & 7) as usize) << 6);
            font_name = fonts.get(ftc).or(fonts.first()).cloned().unwrap_or_else(|| "Arial".into());
            enc = font_enc(&font_name);
        }
        if pos != 0 {
            a.set(if pos & 0x80 != 0 { Attr::Subscript } else { Attr::Superscript }, true);
        }
        let hps = if hps == 0 { 24 } else { hps };
        let annot = dos && g(0) & 1 != 0 && g(0) / 2 == 26;
        chps.push(Chp { s, e, attrs: a, font: FontSpec { name: font_name, size_cpt: hps as u32 * 50 }, caps, special: ul & 0x40 != 0, hidden, annot, enc });
    });

    // ---- paragraph runs
    let mut paps: Vec<Pap> = vec![];
    let pn_para = u16_at(bytes, 18).unwrap_or(0) as usize;
    if pn_para > 0 {
        read_fods(bytes, pn_para, fc_mac, |s, e, p| {
            let g = |i: usize| p.get(i).copied().unwrap_or(0);
            let just = match g(1) & 3 {
                1 => Justify::Center,
                2 => Justify::Right,
                3 => Justify::Full,
                _ => Justify::Left,
            };
            let rhc = g(16);
            let mut loc = Loc::Main;
            if dos {
                if rhc & 0x0E != 0 {
                    let occ = match (rhc >> 1) & 3 {
                        3 => Some(Occurrence::Every),
                        2 => Some(Occurrence::Even),
                        1 => Some(Occurrence::Odd),
                        _ => None,
                    };
                    loc = occ.map(|o| Loc::HeadFoot(rhc & 1 != 0, o)).unwrap_or(Loc::Hidden);
                }
                let st = g(0);
                if st & 1 != 0 && matches!(st / 2, 39 | 87) {
                    loc = Loc::Note;
                }
            } else if rhc & 0x10 != 0 {
                loc = Loc::Graphics;
            } else if rhc & 6 != 0 {
                loc = Loc::HeadFoot(rhc & 1 != 0, Occurrence::Every);
            }
            paps.push(Pap { s, e, just, loc });
        });
    }
    if paps.is_empty() {
        paps.push(Pap { s: 0x80, e: fc_mac, just: Justify::Left, loc: Loc::Main });
    }
    if chps.is_empty() {
        chps.push(Chp { s: 0x80, e: fc_mac, attrs: AttrSet::default(), font: FontSpec { name: fonts.first().cloned().unwrap_or_else(|| "Courier New".into()), size_cpt: 1200 }, caps: false, special: false, hidden: false, annot: false, enc: doc_enc });
    }

    // ---- sections: page setup; section marks without page break
    let mut d = Document::new();
    let mut continuous_marks = vec![];
    let pn_setb = u16_at(bytes, 24).unwrap_or(0) as usize;
    let pn_bftb = u16_at(bytes, 26).unwrap_or(0) as usize;
    if pn_setb != 0 && pn_setb != pn_bftb {
        let base = pn_setb * 128;
        let cset = (u16_at(bytes, base).unwrap_or(0) as usize).min(12);
        for k in 0..cset {
            let o = base + 4 + 10 * k;
            let (Some(cp), Some(fc_sep)) = (u32_at(bytes, o), u32_at(bytes, o + 6)) else { break };
            if fc_sep == u32::MAX {
                break;
            }
            let Some(&cch) = bytes.get(fc_sep as usize) else { continue };
            let sep = bytes.get(fc_sep as usize + 1..fc_sep as usize + 1 + cch as usize).unwrap_or(&[]);
            let w = |i: usize, dflt: u16| if sep.len() >= i + 2 { u16_at(sep, i).unwrap_or(dflt) } else { dflt };
            if k == 0 {
                let (ya_mac, xa_mac, ya_top, dya_text, xa_left, dxa_text) = (w(2, 15840), w(4, 12240), w(8, 1440), w(10, 12960), w(12, 1800), w(14, 8640));
                let tw = |t: u16| (t as i64 * 1200 / 1440) as i32;
                if ya_mac > 1440 && xa_mac > 1440 && ya_mac < 60000 && xa_mac < 60000 {
                    d.init.page_width = tw(xa_mac);
                    d.init.page_height = tw(ya_mac);
                    if (xa_left as u32 + dxa_text as u32) < xa_mac as u32 {
                        d.init.left = tw(xa_left);
                        d.init.right = tw(xa_mac - xa_left - dxa_text);
                    }
                    if (ya_top as u32 + dya_text as u32) < ya_mac as u32 {
                        d.init.top = tw(ya_top);
                        d.init.bottom = tw(ya_mac - ya_top - dya_text);
                    }
                }
            }
            // Word for DOS: each section ends with 0x0C; bkc 0 = continuous
            if dos && sep.len() >= 2 && sep[1] & 7 == 0 {
                continuous_marks.push((cp + 0x80).saturating_sub(1));
            }
        }
    }

    // ---- footnotes (Word for DOS)
    let mut notes = vec![];
    if dos {
        let pn_fntb = u16_at(bytes, 20).unwrap_or(0) as usize;
        let pn_bkmk = u16_at(bytes, 22).unwrap_or(0) as usize;
        if pn_fntb != 0 && pn_fntb != pn_bkmk {
            let base = pn_fntb * 128;
            let n = (u16_at(bytes, base).unwrap_or(0) as usize).min(5000);
            for k in 0..n {
                let o = base + 4 + 8 * k;
                let (Some(r), Some(f)) = (u32_at(bytes, o), u32_at(bytes, o + 4)) else { break };
                notes.push((r.saturating_add(0x80), f.saturating_add(0x80)));
            }
        }
    }

    let w = WriteDoc { data: bytes, fc_mac, paps, chps, notes, continuous_marks };
    // base font: the one of the first main text run
    let first_main = w.paps.iter().find(|p| p.loc == Loc::Main).map(|p| p.s).unwrap_or(0x80);
    let base_font = w.chp_at(first_main).map(|c| c.font.clone()).unwrap_or_else(|| FontSpec { name: "Arial".into(), size_cpt: 1200 });
    let text_width = d.init.page_width - d.init.left - d.init.right;

    // headers / footers
    let mut hf: Vec<El> = vec![];
    for which in 0u8..4 {
        let want = |l: Loc| match l {
            Loc::HeadFoot(foot, occ) => {
                let base = if foot { 2 } else { 0 };
                base + if occ == Occurrence::Even { 1 } else { 0 } == which
            }
            _ => false,
        };
        let Some(occ) = w.paps.iter().find_map(|p| match p.loc {
            l @ Loc::HeadFoot(_, o) if want(l) => Some(o),
            _ => None,
        }) else {
            continue;
        };
        let mut b = Builder::new(base_font.clone());
        b.text_width = text_width;
        w.emit(&mut b, 0x80, fc_mac, &want, 0);
        let els = b.finish();
        if els.iter().any(|e| e.is_char()) {
            hf.push(El::Code(Code::HeadFoot { which, occ, els }));
        }
    }

    let mut b = Builder::new(base_font.clone());
    b.text_width = text_width;
    w.emit(&mut b, 0x80, fc_mac, &|l| l == Loc::Main, 0);
    let body = b.into_document(base_font);
    d.init.font = body.init.font;
    d.init.justify = body.init.justify;
    d.els = hf;
    d.els.extend(body.els);
    Ok(d)
}

impl WriteDoc<'_> {
    fn pap_at(&self, fc: u32) -> Option<&Pap> {
        let i = self.paps.partition_point(|p| p.e <= fc);
        self.paps.get(i).filter(|p| p.s <= fc)
    }
    fn chp_at(&self, fc: u32) -> Option<&Chp> {
        let i = self.chps.partition_point(|c| c.e <= fc);
        self.chps.get(i).filter(|c| c.s <= fc)
    }

    /// Text of footnote `fc_ref` (Word for DOS).
    fn note_text(&self, fc_ref: u32, base: &FontSpec, depth: u32) -> Option<Vec<El>> {
        if depth > 0 {
            return None;
        }
        let k = self.notes.iter().position(|n| n.0 == fc_ref)?;
        let s = self.notes[k].1;
        let e = self.notes.iter().map(|n| n.1).filter(|&f| f > s).min().unwrap_or(self.fc_mac).min(self.fc_mac);
        if s >= e {
            return None;
        }
        let mut b = Builder::new(base.clone());
        self.emit(&mut b, s, e, &|_| true, depth + 1);
        let mut els = b.finish();
        // drop the note's own reference mark at its start
        while matches!(els.first(), Some(El::Ch(c)) if c.is_whitespace() || *c == '\u{5}') {
            els.remove(0);
        }
        Some(els)
    }

    fn emit(&self, b: &mut Builder, fc0: u32, fc1: u32, want: &dyn Fn(Loc) -> bool, depth: u32) {
        let mut fc = fc0.max(0x80);
        let end = fc1.min(self.fc_mac).min(self.data.len() as u32);
        let mut last_just = None;
        while fc < end {
            let Some(pap) = self.pap_at(fc) else {
                fc += 1;
                continue;
            };
            if !want(pap.loc) {
                fc = pap.e.max(fc + 1);
                continue;
            }
            if last_just != Some(pap.just) {
                b.just = pap.just;
                last_just = Some(pap.just);
            }
            let byte = self.data[fc as usize];
            let chp = self.chp_at(fc);
            if let Some(c) = chp {
                if c.hidden && byte >= 0x20 {
                    fc += 1;
                    continue;
                }
                b.attrs = c.attrs;
                b.font = c.font.clone();
            }
            let enc = chp.map(|c| c.enc).unwrap_or(Enc::Cp1252);
            let special = chp.is_some_and(|c| c.special);
            // footnote reference (Word for DOS): auto-numbered (special 5)
            // or with a label
            if !self.notes.is_empty() && depth == 0 {
                if let Some(mut els) = self.note_text(fc, &b.font.clone(), depth) {
                    // the reference run: an auto number (special 4/5) or a label
                    let run_end = chp.map(|c| c.e).unwrap_or(fc + 1).min(end);
                    let mut k = fc;
                    let mut label = String::new();
                    while k < run_end && (k == fc || !self.notes.iter().any(|n| n.0 == k)) {
                        let x = self.data[k as usize];
                        if x < 0x20 && !(special && x <= 9) {
                            break;
                        }
                        if !special {
                            label.push(byte_char(x, enc));
                        }
                        k += 1;
                    }
                    // the note text repeats the label: drop it
                    if !label.is_empty() {
                        let n = label.chars().count();
                        if els.iter().take(n).map(|e| if let El::Ch(c) = e { *c } else { '\0' }).collect::<String>() == label {
                            els.drain(..n);
                            while matches!(els.first(), Some(El::Ch(' '))) {
                                els.remove(0);
                            }
                        }
                    }
                    let annot = chp.is_some_and(|c| c.annot) || (special && byte == 4);
                    if annot {
                        let t = crate::fmt_build::plain(&els);
                        let t = t.trim();
                        if !t.is_empty() {
                            b.code(Code::Comment(t.to_string()));
                        }
                    } else {
                        b.note(NoteKind::Foot, els);
                    }
                    fc = k.max(fc + 1);
                    continue;
                }
            }
            fc += 1;
            if special {
                // page number / date / time / note marks: no text
                continue;
            }
            match byte {
                13 => {}
                10 | 11 | 14 => b.para(),
                9 => b.code_fmt(Code::Tab),
                12 => {
                    if self.continuous_marks.contains(&(fc - 1)) {
                        b.para();
                    } else {
                        b.page();
                    }
                }
                15 => b.ch('—'),
                30 => b.code_fmt(Code::HardHyphen),
                31 => b.code(Code::SoftHyphen),
                0..=0x1F => {}
                0xFF if enc == Enc::Cp437 || enc == Enc::Cp850 => b.code_fmt(Code::HardSpace),
                _ => {
                    let c = byte_char(byte, enc);
                    if chp.is_some_and(|c| c.caps) {
                        for u in c.to_uppercase() {
                            b.ch(u);
                        }
                    } else {
                        b.ch(c);
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

    fn load(name: &str) -> Vec<u8> {
        std::fs::read(format!("tests/data/formats/{name}")).unwrap()
    }

    #[test]
    fn write31_sample() {
        let d = read(&load("Write_3.1.wri")).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Test document for Write conversion"), "{t}");
        assert!(t.contains("[Footer:Footer:"), "{t}");
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::AttrOn(Attr::Bold)))));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::AttrOn(Attr::Italic)))));
    }

    #[test]
    fn word_dos_sample() {
        let d = read(&load("Word_5.0_DOS.doc")).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("This document is written in Microsoft Word for DOS 5.0"), "{t}");
        assert!(t.contains("Copyright (C) 2015 Sean Young"), "{t}");
        // hidden text is not shown
        assert!(!t.contains("hidden superscript"), "{t}");
        assert!(t.contains("footer[Footnote:The label should be 20"), "{t}");
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::Comment(c)) if c.contains("autonumbered annotation"))), "{t}");
        assert!(t.contains("[HPg]"), "{t}");
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::AttrOn(Attr::Strikeout)))));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::AttrOn(Attr::DblUnderline)))));
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::Note { .. }))), "{t}");
    }

    /// Hand-built Write 3.0 file: German umlauts, bold, centred paragraph.
    #[test]
    fn handmade_write() {
        let text: &[u8] = b"Gr\xfc\xdfe \xe4\xf6\xfc\r\nMitte\r\n";
        let mut f = vec![0u8; 128];
        f[0..6].copy_from_slice(&[0x31, 0xBE, 0, 0, 0, 0xAB]);
        let fc_mac = 0x80 + text.len() as u32;
        f[14..18].copy_from_slice(&fc_mac.to_le_bytes());
        f.extend_from_slice(text);
        while f.len() % 128 != 0 {
            f.push(0);
        }
        // CHP page: run 1 bold "Grüße" (6 bytes incl. space? 5 letters), run 2 plain
        let mut chp = vec![0u8; 128];
        chp[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        chp[4..8].copy_from_slice(&(0x80u32 + 5).to_le_bytes());
        chp[8..10].copy_from_slice(&100u16.to_le_bytes());
        chp[10..14].copy_from_slice(&fc_mac.to_le_bytes());
        chp[14..16].copy_from_slice(&0xFFFFu16.to_le_bytes());
        chp[104] = 3; // cch
        chp[105] = 1;
        chp[106] = 1; // bold
        chp[107] = 24;
        chp[0x7F] = 2;
        // PAP page: para 1 left, para 2 centred
        let mut pap = vec![0u8; 128];
        let p1 = 0x80u32 + 11;
        pap[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        pap[4..8].copy_from_slice(&p1.to_le_bytes());
        pap[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
        pap[10..14].copy_from_slice(&fc_mac.to_le_bytes());
        pap[14..16].copy_from_slice(&100u16.to_le_bytes());
        pap[104] = 2;
        pap[105] = 0;
        pap[106] = 1; // centre
        pap[0x7F] = 2;
        let pn_para = (f.len() / 128 + 1) as u16;
        f[18..20].copy_from_slice(&pn_para.to_le_bytes());
        f[96..98].copy_from_slice(&(pn_para + 1).to_le_bytes()); // pnMac ≠ 0 → Write
        f.extend_from_slice(&chp);
        f.extend_from_slice(&pap);
        let d = read(&f).unwrap();
        assert_eq!(debug_text(&d.els), "Grüße äöü\nMitte");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Center))));
    }

    #[test]
    fn never_panics_on_damaged_files() {
        for name in ["Write_3.1.wri", "Word_5.0_DOS.doc"] {
            let b = load(name);
            let step = (b.len() / 300).max(1);
            for n in (0..b.len()).step_by(step) {
                let _ = read(&b[..n]);
            }
            let mut seed = 12345u32;
            for _ in 0..300 {
                let mut c = b.clone();
                for _ in 0..8 {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    let i = (seed as usize >> 4) % c.len();
                    c[i] = (seed >> 20) as u8;
                }
                c[..6].copy_from_slice(&b[..6]);
                let _ = read(&c);
            }
        }
    }
}
