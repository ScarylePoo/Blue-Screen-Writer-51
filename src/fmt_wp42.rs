//! WordPerfect 4.2 (and earlier DOS versions) document reader.
//!
//! These files have no header.  Bytes 0x20–0x7F are text, 0x00–0x1F
//! control characters (0x0A hard return, 0x0D soft return, 0x0C hard page,
//! 0x09 tab), 0x80–0xBF single-byte function codes (attributes on/off) and
//! 0xC0–0xFE multi-byte functions that end with the same byte: fixed size
//! ones (size table below) and variable size ones (e.g. headers/footers,
//! footnotes) whose data ends with 0xFF before the closing byte.
//! Accented letters are stored as `[E1 char E1]` with the IBM PC (code page
//! 437) character number.
//!
//! libwpd's WP 4.2 parser was used as a reference for format facts.  All
//! reads are bounds-checked: damaged files never panic.

use crate::doc::*;
use crate::fmt_build::Builder;

/// Total size of the multi-byte functions 0xC0..=0xFE; -1 = variable
/// (ends at the next occurrence of the function byte).
const GROUP_SIZE: [i16; 63] = [
    6, 4, 3, 5, 5, 6, 4, 6, 8, 42, 3, 6, 4, 3, 4, 3, // C0
    6, -1, -1, 4, 4, 4, 6, -1, 4, 4, 4, 4, -1, 24, 4, -1, // D0
    4, 3, -1, 150, 6, 23, 11, 3, 3, -1, -1, -1, 4, -1, 44, 18, // E0
    6, 106, -1, 100, 4, -1, 5, -1, -1, -1, -1, -1, -1, -1, -1, // F0
];

/// End (exclusive) of the multi-byte function starting at `i`, if it is
/// properly closed.
fn group_end(d: &[u8], i: usize) -> Option<usize> {
    let c = d[i];
    if !(0xC0..=0xFE).contains(&c) {
        return None;
    }
    let sz = GROUP_SIZE[(c - 0xC0) as usize];
    if sz > 0 {
        let e = i + sz as usize;
        (e <= d.len() && d[e - 1] == c).then_some(e)
    } else {
        // variable: up to the next occurrence of the function byte
        let k = d[i + 1..].iter().position(|&b| b == c)?;
        Some(i + 1 + k + 1)
    }
}

/// Does this look like a WP 4.2 document?  `ext_hint` = the file name has
/// a WordPerfect-like extension (.wp, .doc, .wpd, none), so weaker evidence
/// suffices.
pub fn probe(bytes: &[u8], ext_hint: bool) -> bool {
    if bytes.starts_with(&[0xFE, 0xFF, 0x61, 0x61]) {
        return true; // encrypted WP 4.2
    }
    if bytes.is_empty() || std::str::from_utf8(bytes).is_ok() {
        return false;
    }
    let d = &bytes[..bytes.len().min(1 << 20)];
    let mut i = 0;
    let mut groups = 0;
    let mut text = 0;
    let (mut on, mut off) = (0, 0);
    while i < d.len() {
        let c = d[i];
        match c {
            0xFF => return false,
            0x00..=0x08 | 0x0E..=0x1F => {
                // other control characters are rare in WP 4.2 text
                i += 1;
            }
            0x09..=0x0D | 0x20..=0x7F => {
                text += 1;
                i += 1;
            }
            0x80..=0xBF => {
                match c {
                    0x9D | 0x94 | 0x92 | 0x90 | 0xB2 => on += 1,
                    0x9C | 0x95 | 0x93 | 0x91 | 0xB3 => off += 1,
                    _ => {}
                }
                i += 1;
            }
            _ => match group_end(d, i) {
                Some(e) => {
                    groups += 1;
                    i = e;
                }
                None => {
                    // an unclosed group at the very end of a cut sample is
                    // tolerated only when the whole file was not scanned
                    if bytes.len() > d.len() && d.len() - i < 200 {
                        break;
                    }
                    return false;
                }
            },
        }
    }
    if text == 0 {
        return false;
    }
    groups >= 1 || (ext_hint && on > 0 && on == off)
}

fn attr(c: u8) -> Option<(Attr, bool)> {
    Some(match c {
        0x90 => (Attr::Redline, true),
        0x91 => (Attr::Redline, false),
        0x92 => (Attr::Strikeout, true),
        0x93 => (Attr::Strikeout, false),
        0x94 => (Attr::Underline, true),
        0x95 => (Attr::Underline, false),
        0x9C => (Attr::Bold, false),
        0x9D => (Attr::Bold, true),
        0xB2 => (Attr::Italic, true),
        0xB3 => (Attr::Italic, false),
        0xB4 => (Attr::Shadow, true),
        0xB5 => (Attr::Shadow, false),
        _ => return None,
    })
}

/// Character of an `[E1 n E1]` extended character (IBM PC code page 437).
fn ext_char(n: u8) -> char {
    match n {
        0x00..=0x1F => " ☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼".chars().nth(n as usize).unwrap_or(' '),
        0x7F => '⌂',
        _ => crate::rtf::decode_byte(n, 437),
    }
}

struct Parser {
    depth: u32,
    left: i32,
    right: i32,
    init_lr: Option<(i32, i32)>,
}

const PAGE_WIDTH: i32 = 10200; // 8.5" — WP 4.2 margins are in columns at 10 cpi

fn started(b: &Builder) -> bool {
    b.els.iter().any(|e| matches!(e, El::Ch(_) | El::Code(Code::HRt | Code::HPg | Code::Tab)))
}

impl Parser {
    fn run(&mut self, d: &[u8], b: &mut Builder) {
        let mut i = 0;
        while i < d.len() {
            let c = d[i];
            match c {
                0x09 => {
                    b.ch('\t');
                    i += 1;
                }
                0x0A => {
                    b.para();
                    i += 1;
                }
                0x0C => {
                    b.page();
                    i += 1;
                }
                0x0D => {
                    // soft return replaces a space
                    b.ch(' ');
                    i += 1;
                }
                0x20..=0x7F => {
                    b.ch(c as char);
                    i += 1;
                }
                0x80..=0xBF => {
                    if let Some((a, on)) = attr(c) {
                        b.set(a, on);
                    }
                    i += 1;
                }
                0xC0..=0xFE => match group_end(d, i) {
                    Some(e) => {
                        self.group(&d[i..e], b);
                        i = e;
                    }
                    None => i += 1,
                },
                _ => i += 1,
            }
        }
    }

    fn group(&mut self, g: &[u8], b: &mut Builder) {
        let body = &g[1..g.len() - 1];
        match g[0] {
            0xE1 => {
                if let Some(&n) = body.first() {
                    b.ch(ext_char(n));
                }
            }
            0xC0 => {
                // [C0 old-left old-right new-left new-right C0], columns
                if let (Some(&l), Some(&r)) = (body.get(2), body.get(3)) {
                    let (l, r) = (l as i32 * 120, PAGE_WIDTH - r as i32 * 120);
                    if l > 0 && r > 0 && l + r < PAGE_WIDTH - 1200 {
                        self.left = l;
                        self.right = r;
                        if self.depth == 0 && !started(b) {
                            self.init_lr = Some((l, r));
                        } else if self.depth == 0 {
                            b.code_before_text(Code::LRMargin { left: l, right: r });
                        }
                    }
                }
            }
            0xD1 if self.depth == 0 => {
                // [D1 4 bytes text… FF x definition D1]
                if body.len() > 7 {
                    let def = body[body.len() - 1];
                    let text_end = body.len() - 3;
                    if body[text_end] == 0xFF {
                        let els = self.sub(&body[4..text_end], b);
                        let which = def & 3;
                        let occ = match (def >> 2) & 7 {
                            0 => Occurrence::Discontinue,
                            2 => Occurrence::Odd,
                            3 => Occurrence::Even,
                            _ => Occurrence::Every,
                        };
                        if occ != Occurrence::Discontinue && !els.is_empty() {
                            b.head_foot(which, occ, els);
                        }
                    }
                }
            }
            0xE2 if self.depth == 0 => {
                // footnote: a few bytes of numbering data, the note text,
                // then 0xFF and trailing data
                let end = body.iter().rposition(|&x| x == 0xFF).unwrap_or(body.len());
                let t = &body[..end];
                let start = (0..t.len().min(12)).find(|&k| t.len() >= k + 2 && t[k..k + 2].iter().all(|&x| (0x20..0x7F).contains(&x))).unwrap_or(t.len().min(5));
                let els = self.sub(&t[start..], b);
                if !els.is_empty() {
                    b.note(NoteKind::Foot, els);
                }
            }
            _ => {}
        }
    }

    fn sub(&mut self, d: &[u8], b: &Builder) -> Vec<El> {
        let mut nb = Builder::new(b.font.clone());
        let mut p = Parser { depth: self.depth + 1, left: self.left, right: self.right, init_lr: None };
        p.run(d, &mut nb);
        nb.finish()
    }
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if bytes.starts_with(&[0xFE, 0xFF, 0x61, 0x61]) {
        return Err("Document is password protected".into());
    }
    let base = FontSpec { name: "Courier New".into(), size_cpt: 1200 };
    let mut b = Builder::new(base.clone());
    let mut p = Parser { depth: 0, left: 1200, right: 1200, init_lr: None };
    p.run(bytes, &mut b);
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

    fn sample() -> Vec<u8> {
        let mut t = vec![0xC0, 10, 74, 12, 72, 0xC0];
        t.extend(b"Gr");
        t.extend([0xE1, 0x81, 0xE1]); // ü
        t.extend([0xE1, 0xE1, 0xE1]); // ß (CP437 0xE1)
        t.extend(b"e ");
        t.push(0x9D);
        t.extend(b"fett");
        t.push(0x9C);
        t.push(b' ');
        t.push(0x94);
        t.extend(b"unterstrichen");
        t.push(0x95);
        t.push(0x0A);
        t.extend(b"Zeile\tzwei");
        t.push(0x0C);
        t.extend(b"Seite");
        t
    }

    #[test]
    fn synthetic() {
        let b = sample();
        assert!(probe(&b, false));
        let d = read(&b).unwrap();
        assert_eq!(debug_text(&d.els), "Grüße fett unterstrichen\nZeile\tzwei\n[HPg]\nSeite");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Underline))));
        assert_eq!(d.init.left, 1440);
    }

    #[test]
    fn sample_file() {
        let bytes = std::fs::read("tests/data/formats/WP4.wp").unwrap();
        assert!(probe(&bytes, true));
        assert!(probe(&bytes, false));
        let d = read(&bytes).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Dit is een test\nTest\nTest\nTest"), "{t}");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
    }

    #[test]
    fn plain_text_is_not_wp42() {
        let de_1252: Vec<u8> = "Grüße aus Köln, Äpfel und Öl für die Bäckerei.\r\n".chars().map(|c| c as u32 as u8).collect();
        assert!(!probe(&de_1252, true));
        assert!(!probe("Grüße aus Köln\n".as_bytes(), true));
        let de_850 = [b'F', 0x81, b'r', b' ', 0xE1, b'\r', b'\n', 0x84, 0x94, 0x81];
        assert!(!probe(&de_850, true));
        assert!(!probe(b"just ascii text\n", true));
        let ru_1251: Vec<u8> = "Привет мир".chars().map(|c| if c == ' ' { b' ' } else { (c as u32 - 0x410 + 0xC0) as u8 }).collect();
        assert!(!probe(&ru_1251, true));
    }

    #[test]
    fn never_panics_on_damaged_files() {
        for bytes in [sample(), std::fs::read("tests/data/formats/WP4.wp").unwrap()] {
            for n in 0..bytes.len() {
                let _ = read(&bytes[..n]);
                let _ = probe(&bytes[..n], true);
            }
            let mut seed = 777u32;
            for _ in 0..2000 {
                let mut b = bytes.clone();
                for _ in 0..4 {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    let k = (seed >> 8) as usize % b.len();
                    b[k] = (seed >> 3) as u8;
                }
                let _ = read(&b);
                let _ = probe(&b, false);
            }
        }
    }
}
