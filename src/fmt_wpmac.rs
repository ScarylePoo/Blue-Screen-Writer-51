//! WordPerfect for Macintosh 2.x/3.x document reader (`FF WPC`, file type
//! 0x2C).
//!
//! Like WP 5/6 the text follows a 16-byte prefix (document pointer at 4).
//! Numbers are big-endian.  Bytes 0x20–0x7E are text, 0x80–0xBF single-byte
//! functions (condensed hard return, tab, indent, hard space, hyphens),
//! 0xC0–0xCF fixed-length groups (extended character with its Mac Roman
//! code, tab, indent, undo, attribute on/off) and 0xD0–0xEF variable groups
//! `[grp][sub][size u16][…][size u16][sub][grp]` (end of line/page, page
//! format, font, header/footer, footnote, table structure).
//!
//! libwpd's WP3 parser was used as a reference for format facts.  All reads
//! are bounds-checked: damaged files never panic.

use crate::doc::*;
use crate::fmt_build::Builder;
use crate::fmt_text::{Enc, byte_char};

/// Total size of the fixed-length groups 0xC0..=0xCF.
const FIXED_SIZE: [usize; 16] = [5, 8, 7, 4, 4, 7, 10, 7, 4, 5, 6, 6, 7, 9, 7, 4];

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(o)?, *b.get(o.checked_add(1)?)?]))
}
fn be32(b: &[u8], o: usize) -> Option<u32> {
    let s = b.get(o..o.checked_add(4)?)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn attr_of(n: u8) -> Option<Attr> {
    Some(match n {
        0 => Attr::Bold,
        1 => Attr::Italic,
        2 => Attr::Underline,
        3 => Attr::Outline,
        4 => Attr::Shadow,
        8 => Attr::Redline,
        9 => Attr::Strikeout,
        10 => Attr::Subscript,
        11 => Attr::Superscript,
        12 => Attr::DblUnderline,
        13 => Attr::ExtraLarge,
        14 => Attr::VeryLarge,
        15 => Attr::Large,
        16 => Attr::Small,
        17 => Attr::Fine,
        18 => Attr::SmallCaps,
        _ => return None,
    })
}

/// 16.16 fixed point points → WPU.
fn fixed_wpu(v: u32) -> i32 {
    ((v as i32 as f64) / 65536.0 * 1200.0 / 72.0).round() as i32
}

struct Parser {
    depth: u32,
    undo: bool,
    need_row: bool,
    need_cell: bool,
    init_lr: Option<(i32, i32)>,
}

fn started(b: &Builder) -> bool {
    b.els.iter().any(|e| matches!(e, El::Ch(_) | El::Code(Code::HRt | Code::HPg | Code::Tab | Code::TblDef { .. })))
}

impl Parser {
    /// Open pending table row / cell before content.
    fn pre(&mut self, b: &mut Builder) {
        if b.in_table() {
            if self.need_row {
                self.need_row = false;
                self.need_cell = false;
                b.row();
                b.cell();
            } else if self.need_cell {
                self.need_cell = false;
                b.cell();
            }
        }
    }

    fn ch(&mut self, b: &mut Builder, c: char) {
        if !self.undo {
            self.pre(b);
            b.ch(c);
        }
    }

    fn run(&mut self, d: &[u8], b: &mut Builder) {
        let mut i = 0;
        while i < d.len() {
            let c = d[i];
            match c {
                0x20..=0x7E => {
                    self.ch(b, c as char);
                    i += 1;
                }
                0x80..=0xBF => {
                    if !self.undo {
                        match c {
                            0x80 => b.para(),
                            0x81 => b.page(),
                            0x82 => {
                                self.pre(b);
                                b.code_fmt(Code::Tab)
                            }
                            0x83 => b.code(Code::MarRel),
                            0x84 => b.code(Code::Indent),
                            0x85 => b.code(Code::DblIndent),
                            0x96 => self.ch(b, '-'),
                            0x97 => self.ch(b, '\u{ad}'),
                            0xA0 => self.ch(b, '\u{a0}'),
                            _ => {}
                        }
                    }
                    i += 1;
                }
                0xC0..=0xCF => {
                    let size = FIXED_SIZE[(c - 0xC0) as usize];
                    if i + size > d.len() || d[i + size - 1] != c {
                        i += 1;
                        continue;
                    }
                    self.fixed(&d[i..i + size], b);
                    i += size;
                }
                0xD0..=0xEF => {
                    let Some(size) = be16(d, i + 2).map(|s| s as usize) else { break };
                    let end = i + size + 4;
                    let ok = size >= 4 && end <= d.len() && be16(d, i + size) == Some(size as u16) && d[end - 2] == d[i + 1] && d[end - 1] == c;
                    if !ok {
                        i += 1;
                        continue;
                    }
                    if !self.undo {
                        self.variable(c, d[i + 1], &d[i + 4..i + size], b);
                    }
                    i = end;
                }
                _ => i += 1,
            }
        }
    }

    fn fixed(&mut self, g: &[u8], b: &mut Builder) {
        match g[0] {
            0xC0 => {
                let (mac, set, ch) = (g[1], g[2], g[3]);
                if mac >= 0x20 {
                    self.ch(b, byte_char(mac, Enc::MacRoman));
                } else if set != 0xFF || (ch != 0xFE && ch != 0xFF) {
                    let s = crate::wpchars::wp_to_string(set, ch as u16);
                    for c in s.chars() {
                        self.ch(b, c);
                    }
                }
            }
            0xC1 if !self.undo => {
                self.pre(b);
                match g[1] {
                    0x01 => b.code(Code::Center),
                    0x02 => b.code(Code::FlushRight),
                    0x03 => b.code(Code::MarRel),
                    0x04 => b.ch(' '),
                    _ => b.code_fmt(Code::Tab),
                }
            }
            0xC2 if !self.undo => {
                self.pre(b);
                b.code(if g[1] == 1 { Code::DblIndent } else { Code::Indent });
            }
            0xC3 if !self.undo => {
                if let Some(a) = attr_of(g[1]) {
                    b.set(a, g[2] & 1 == 1);
                }
            }
            0xCD => match g[1] {
                0 => self.undo = true,
                1 => self.undo = false,
                _ => {}
            },
            _ => {}
        }
    }

    fn sub(&mut self, d: &[u8], b: &Builder) -> Vec<El> {
        if self.depth >= 3 {
            return vec![];
        }
        let mut nb = Builder::new(b.font.clone());
        let mut p = Parser { depth: self.depth + 1, undo: false, need_row: false, need_cell: false, init_lr: None };
        p.run(d, &mut nb);
        nb.finish()
    }

    fn variable(&mut self, grp: u8, sub: u8, c: &[u8], b: &mut Builder) {
        match grp {
            0xDC => match sub {
                0x00 | 0x01 => self.ch(b, ' '),
                0x02..=0x04 | 0x06 | 0x0A | 0x0B => b.para(),
                0x05 | 0x07 => b.page(),
                0x08 | 0x09 | 0x14 | 0x15 => b.para(),
                0x0C | 0x0D => self.ch(b, '-'),
                0x0E | 0x0F => self.ch(b, '\u{ad}'),
                0x16 => {
                    self.pre(b);
                    self.need_cell = true;
                }
                0x18 | 0x19 | 0x1C | 0x1D => {
                    self.pre(b);
                    self.need_row = true;
                }
                0x1A | 0x1B => {
                    self.pre(b);
                    b.table_end();
                    self.need_row = false;
                    self.need_cell = false;
                }
                _ => {}
            },
            0xD0 => match sub {
                0x01 => {
                    // horizontal margins: 8 bytes old values, new left, new right
                    if let (Some(l), Some(r)) = (be32(c, 8), be32(c, 12)) {
                        let (l, r) = (fixed_wpu(l), fixed_wpu(r));
                        if (0..12000).contains(&l) && (0..12000).contains(&r) {
                            if self.depth == 0 && !started(b) {
                                self.init_lr = Some((l, r));
                            } else if self.depth == 0 {
                                b.code_before_text(Code::LRMargin { left: l, right: r });
                            }
                        }
                    }
                }
                0x06 => {
                    if let Some(&j) = c.get(1) {
                        b.just = match j {
                            1 => Justify::Center,
                            2 => Justify::Right,
                            3 | 4 => Justify::Full,
                            _ => Justify::Left,
                        };
                    }
                }
                _ => {}
            },
            0xD1 => match sub {
                0x01 => {
                    if let Some(&n) = c.get(12) {
                        let name: String = c.get(13..13 + n as usize).unwrap_or(&[]).iter().map(|&x| byte_char(x, Enc::MacRoman)).collect();
                        if !name.trim().is_empty() {
                            b.font.name = name.trim().to_string();
                        }
                    }
                }
                0x02 => {
                    if let Some(s) = be16(c, 2).filter(|s| (4..=200).contains(s)) {
                        b.font.size_cpt = s as u32 * 100;
                    }
                }
                _ => {}
            },
            0xD5 if sub <= 3 && self.depth == 0 => {
                // 14 bytes, old text (length + data), definition, 4 bytes,
                // new text length, text
                let Some(old) = be16(c, 14).map(|v| v as usize) else { return };
                let p = 16 + old;
                let Some(&def) = c.get(p) else { return };
                let Some(len) = be16(c, p + 5).map(|v| v as usize) else { return };
                let t = c.get(p + 7..(p + 7 + len).min(c.len())).unwrap_or(&[]);
                let occ = match def & 3 {
                    0 => Occurrence::Discontinue,
                    1 => Occurrence::Even,
                    2 => Occurrence::Odd,
                    _ => Occurrence::Every,
                };
                let els = self.sub(t, b);
                if occ != Occurrence::Discontinue && !els.is_empty() {
                    b.head_foot(sub, occ, els);
                }
            }
            0xD6 if sub <= 1 => {
                let mut p = 25;
                let Some(np) = be16(c, p).map(|v| v as usize) else { return };
                p += 2 + 4 * np;
                let Some(nb) = be16(c, p).map(|v| v as usize) else { return };
                p += 2 + 6 * nb;
                if p < c.len() {
                    let els = self.sub(&c[p..], b);
                    self.pre(b);
                    b.note(if sub == 0 { NoteKind::Foot } else { NoteKind::End }, els);
                }
            }
            0xE2 if sub == 0x01 && self.depth == 0 => {
                if b.in_table() {
                    b.table_end();
                }
                b.table_start(vec![]);
                self.need_row = true;
            }
            _ => {}
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if bytes.len() < 16 || !bytes.starts_with(b"\xFFWPC") || bytes[9] != 0x2C {
        return Err("Not a WordPerfect for Macintosh document".into());
    }
    if bytes[12] != 0 || bytes[13] != 0 {
        return Err("Document is password protected".into());
    }
    let ptr = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if ptr < 16 || ptr > bytes.len() {
        return Err("damaged WordPerfect document (no text)".into());
    }
    let base = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    let mut b = Builder::new(base.clone());
    let mut p = Parser { depth: 0, undo: false, need_row: false, need_cell: false, init_lr: None };
    p.run(&bytes[ptr..], &mut b);
    let mut d = b.into_document(base);
    if let Some((l, r)) = p.init_lr {
        d.init.left = l;
        d.init.right = r;
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    #[test]
    fn sample_file() {
        let bytes = std::fs::read("tests/data/formats/WP3.wpd").unwrap();
        assert_eq!(crate::fileio::sniff(&bytes), crate::fileio::Kind::WpMac);
        let d = read(&bytes).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("This is a file created by WP 2.1.3"), "{t}");
        assert!(t.contains("Bold") && t.contains("Italic"), "{t}");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))), "{:?}", d.els);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
    }

    #[test]
    fn mac_roman_extended_character() {
        let mut f = b"\xFFWPC\x10\0\0\0\x01\x2C\x02\x02\0\0\0\0".to_vec();
        f.extend(b"Gr");
        f.extend([0xC0, 0x9F, 0x01, 0x00, 0xC0]); // ü (Mac Roman 0x9F)
        f.extend([0xC0, 0xA7, 0x01, 0x00, 0xC0]); // ß
        f.extend(b"e");
        f.push(0x80);
        f.extend(b"zwei");
        let d = read(&f).unwrap();
        assert_eq!(debug_text(&d.els), "Grüße\nzwei");
    }

    #[test]
    fn never_panics_on_damaged_files() {
        let bytes = std::fs::read("tests/data/formats/WP3.wpd").unwrap();
        for n in 0..bytes.len() {
            let _ = read(&bytes[..n]);
        }
        let mut seed = 99u32;
        for _ in 0..3000 {
            let mut b = bytes.clone();
            for _ in 0..4 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let k = 16 + (seed >> 8) as usize % (b.len() - 16);
                b[k] = (seed >> 3) as u8;
            }
            let _ = read(&b);
        }
    }
}
