//! WordStar for DOS (3.x – 7.0) document reader (.ws, .wsd, .ws7, .doc).
//!
//! Written from "File Format for WordStar Release 7.0" (WordStar
//! International, 1992):
//! * text is 7-bit ASCII; bit 7 marks soft spaces (0xA0), soft returns
//!   (0x8D 0x0A) and, before release 5, the last letter of each word — it
//!   is masked off;
//! * hard return = 0x0D 0x0A, 0x1A = end of file;
//! * print controls toggle attributes: ^B bold, ^D double strike, ^S
//!   underline, ^Y italic, ^T superscript, ^V subscript, ^X strikeout;
//!   ^I tab, ^L page break, ^O binding (hard) space, 0x1E/0x1F soft hyphens;
//! * extended characters (3.4+): `1B xx 1C` — xx in the DOS code page
//!   (437/850, or 866 for Russian; detected);
//! * "symmetrical sequences" (5.0+): `1D count(2) type … count(2) 1D`;
//!   the file header is one (type 0, 128 bytes), footnotes (3) and
//!   endnotes (4) carry their text inside, everything else is skipped;
//! * dot commands (lines starting with `.`): `.pa` page break, `.he`/`.h1`
//!   header, `.fo`/`.f1` footer; the rest is dropped.

use crate::doc::*;
use crate::fmt_build::Builder;
use crate::fmt_text::Enc;

const TOGGLES: [u8; 7] = [0x02, 0x04, 0x13, 0x19, 0x14, 0x16, 0x18];

fn u16_at(b: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]) as usize)
}

/// Length of a symmetrical sequence starting at `i` (0x1D), if valid.
fn sym_len(b: &[u8], i: usize) -> Option<usize> {
    let count = u16_at(b, i + 1)?;
    if count < 4 {
        return None;
    }
    let end = i.checked_add(count)?;
    if b.get(end + 2) == Some(&0x1D) && u16_at(b, end) == Some(count) {
        Some(count + 3)
    } else {
        None
    }
}

fn has_header(b: &[u8]) -> bool {
    b.len() >= 8 && b[0] == 0x1D && b[3] == 0 && sym_len(b, 0).is_some()
}

/// Strong evidence of a WordStar file.
pub fn probe(bytes: &[u8]) -> bool {
    if has_header(bytes) {
        return true;
    }
    let b = &bytes[..bytes.len().min(65536)];
    if b.contains(&0) {
        return false;
    }
    let soft_crlf = b.windows(2).filter(|w| w == &[0x8D, 0x0A]).count();
    let toggles = b.iter().filter(|c| TOGGLES.contains(c)).count();
    let mut high_end = 0usize;
    let mut plain_end = 0usize;
    for w in b.windows(3) {
        let end_ok = matches!(w[2], b' ' | 0xA0 | 0x0D | 0x8D | b',' | b'.');
        if !end_ok || !w[0].is_ascii_lowercase() {
            continue;
        }
        if w[1].is_ascii_lowercase() {
            plain_end += 1;
        } else if w[1] >= 0x80 && ((w[1] & 0x7F).is_ascii_alphanumeric() || b".,;:!?)\"'".contains(&(w[1] & 0x7F))) {
            high_end += 1;
        }
    }
    let ratio_hi = high_end * 2 > (high_end + plain_end) && high_end >= 10;
    (soft_crlf >= 3 && (toggles >= 2 || ratio_hi)) || (ratio_hi && high_end * 4 > (high_end + plain_end) * 3) || (toggles >= 4 && soft_crlf >= 1)
}

/// Weak evidence (the file name says WordStar).
pub fn probe_weak(bytes: &[u8]) -> bool {
    if has_header(bytes) {
        return true;
    }
    let b = &bytes[..bytes.len().min(65536)];
    b.iter().any(|&c| TOGGLES.contains(&c) || matches!(c, 0x0F | 0x1B | 0x1D | 0x1E | 0x1F) || c >= 0x80)
        || b.starts_with(b".")
        || b.windows(3).any(|w| w[0] == b'\n' && w[1] == b'.' && w[2].is_ascii_alphabetic())
}

struct St {
    bold: bool,
    dbl: bool,
    und: bool,
    ital: bool,
    sup: bool,
    sub: bool,
    strike: bool,
}

impl St {
    fn new() -> St {
        St { bold: false, dbl: false, und: false, ital: false, sup: false, sub: false, strike: false }
    }
    fn apply(&self, b: &mut Builder) {
        b.set(Attr::Bold, self.bold || self.dbl);
        b.set(Attr::Underline, self.und);
        b.set(Attr::Italic, self.ital);
        b.set(Attr::Superscript, self.sup);
        b.set(Attr::Subscript, self.sub);
        b.set(Attr::Strikeout, self.strike);
    }
}

struct Ctx {
    enc: Enc,
    headers: Vec<El>,
}

/// Bytes of the document's extended characters, for code page detection.
fn ext_sample(b: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1B && b.get(i + 2) == Some(&0x1C) {
            out.push(b[i + 1]);
            i += 3;
        } else {
            let c = b[i] & 0x7F;
            out.push(if c >= 0x20 { c } else { b' ' });
            i += 1;
        }
    }
    out
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if bytes.is_empty() {
        return Err("empty file".into());
    }
    let enc = match crate::fmt_text::detect_8bit(&ext_sample(bytes)) {
        Enc::Cp866 | Enc::Koi8r | Enc::Cp1251 => Enc::Cp866,
        _ => Enc::Cp437,
    };
    let font = FontSpec { name: "Courier New".into(), size_cpt: 1200 };
    let mut ctx = Ctx { enc, headers: vec![] };
    let mut b = Builder::new(font.clone());
    parse(bytes, &mut b, &mut ctx, 0);
    let mut d = b.into_document(font);
    if !ctx.headers.is_empty() {
        let mut els = std::mem::take(&mut ctx.headers);
        els.extend(d.els);
        d.els = els;
    }
    Ok(d)
}

fn parse(bytes: &[u8], b: &mut Builder, ctx: &mut Ctx, depth: u32) {
    let mut st = St::new();
    let mut i = 0;
    let mut line_start = true;
    let mut soft_space = false;
    let mut last_space = true;
    while i < bytes.len() {
        let c = bytes[i];
        // dot command
        if line_start && (c & 0x7F) == b'.' && depth == 0 {
            let mut j = i;
            while j < bytes.len() && bytes[j] != 0x0D && bytes[j] != 0x8D && bytes[j] != 0x0A && bytes[j] != 0x1A {
                j += 1;
            }
            let line: String = bytes[i + 1..j].iter().map(|&x| x & 0x7F).filter(|x| *x >= 0x20).map(|x| x as char).collect();
            dot_command(&line, b, ctx);
            i = j;
            while i < bytes.len() && matches!(bytes[i], 0x0D | 0x8D | 0x0A) {
                i += 1;
                if bytes.get(i - 1) == Some(&0x0A) {
                    break;
                }
            }
            continue;
        }
        let was_line_start = std::mem::replace(&mut line_start, false);
        match c {
            0x1A if depth == 0 => break,
            0x0D | 0x8D => {
                let soft = c == 0x8D;
                i += 1;
                if bytes.get(i) == Some(&0x0A) {
                    i += 1;
                }
                if !soft {
                    b.para();
                    last_space = true;
                    soft_space = false;
                }
                line_start = true;
                continue;
            }
            0x0A => {}
            0x1D => {
                if let Some(n) = sym_len(bytes, i) {
                    let typ = bytes[i + 3];
                    if matches!(typ, 3 | 4) && depth < 2 && n > 12 {
                        let inner = &bytes[i + 9..i + n - 3];
                        let mut nb = Builder::new(b.font.clone());
                        parse(inner, &mut nb, ctx, depth + 1);
                        let els = nb.finish();
                        b.note(if typ == 3 { NoteKind::Foot } else { NoteKind::End }, els);
                        last_space = false;
                    } else {
                        line_start = was_line_start;
                    }
                    i += n;
                    continue;
                }
            }
            0x1B => {
                if bytes.get(i + 2) == Some(&0x1C) {
                    let x = bytes[i + 1];
                    let ch = if x < 0x20 {
                        ' '
                    } else {
                        crate::fmt_text::byte_char(x, ctx.enc)
                    };
                    emit(b, &st, ch, &mut soft_space, &mut last_space);
                    i += 3;
                    continue;
                }
            }
            0x02 => st.bold = !st.bold,
            0x04 => st.dbl = !st.dbl,
            0x13 => st.und = !st.und,
            0x19 => st.ital = !st.ital,
            0x14 => {
                st.sup = !st.sup;
                st.sub = false;
            }
            0x16 => {
                st.sub = !st.sub;
                st.sup = false;
            }
            0x18 => st.strike = !st.strike,
            0x09 => {
                st.apply(b);
                b.ch('\t');
                last_space = true;
                soft_space = false;
            }
            0x0C => {
                b.page();
                line_start = true;
                last_space = true;
            }
            0x0F => {
                st.apply(b);
                b.ch('\u{a0}');
                last_space = true;
            }
            0xA0 => soft_space = true,
            _ => {
                let m = c & 0x7F;
                if (0x20..0x7F).contains(&m) {
                    emit(b, &st, m as char, &mut soft_space, &mut last_space);
                }
            }
        }
        i += 1;
    }
}

fn emit(b: &mut Builder, st: &St, ch: char, soft_space: &mut bool, last_space: &mut bool) {
    if std::mem::take(soft_space) && !*last_space && ch != ' ' {
        b.ch(' ');
    }
    st.apply(b);
    b.ch(ch);
    *last_space = ch == ' ';
}

fn dot_command(line: &str, b: &mut Builder, ctx: &mut Ctx) {
    let l = line.to_ascii_lowercase();
    let cmd: String = l.chars().take(2).collect();
    let arg = line.get(2..).unwrap_or("").trim_start_matches(|c: char| c.is_ascii_digit()).trim();
    match cmd.as_str() {
        "pa" => b.page(),
        "he" | "h1" | "fo" | "f1" => {
            let footer = cmd.starts_with('f');
            let which = if footer { 2 } else { 0 };
            if ctx.headers.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) || arg.is_empty() {
                return;
            }
            let els: Vec<El> = arg.chars().map(El::Ch).collect();
            ctx.headers.push(El::Code(Code::HeadFoot { which, occ: Occurrence::Every, els }));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    /// A WordStar 6-style document with header, dot commands, extended
    /// characters (CP437), print controls and a footnote.
    pub fn sample_bytes() -> Vec<u8> {
        let mut v = vec![0x1D, 0x7D, 0x00, 0x00, 0x60];
        v.resize(125, 0);
        v.extend_from_slice(&[0x7D, 0x00, 0x1D]);
        assert_eq!(v.len(), 128);
        let ext = |c: u8| [0x1B, c, 0x1C];
        v.extend_from_slice(b".he Kopfzeile\r\n");
        v.extend_from_slice(b"Gr");
        v.extend_from_slice(&ext(0x81)); // ü
        v.extend_from_slice(&ext(0xE1)); // ß
        v.extend_from_slice(b"e aus K");
        v.extend_from_slice(&ext(0x94)); // ö
        v.extend_from_slice(b"ln, \x02fett\x02 und\xA0\x8D\n\x19kursiv\x19.");
        // footnote: 1D count type=3, line count, tag offset, flag, text, count, 1D
        let note = b"Fu\x1B\xE1\x1Cnote hier.";
        let count = (3 + 1 + 5 + note.len()) as u16;
        v.push(0x1D);
        v.extend_from_slice(&count.to_le_bytes());
        v.extend_from_slice(&[3, 1, 0, 1, 0, 0]);
        v.extend_from_slice(note);
        v.extend_from_slice(&count.to_le_bytes());
        v.push(0x1D);
        v.extend_from_slice(b"\r\n.pa\r\nSeite \x13zwei\x13\r\n\x1a\x1a\x1a");
        v
    }

    #[test]
    fn sample() {
        let b = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/sample.ws")).unwrap();
        assert_eq!(b, sample_bytes(), "tests/data/formats/sample.ws is out of date");
        assert!(probe(&b));
        assert_eq!(crate::fileio::sniff(&b), crate::fileio::Kind::WordStar);
        let d = read(&b).unwrap();
        assert_eq!(debug_text(&d.els), "[Header:Kopfzeile]Grüße aus Köln, fett und kursiv.[Footnote:Fußnote hier.]\n[HPg]\nSeite zwei");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Underline))));
    }

    #[test]
    fn wordstar3_high_bits_and_russian_extended() {
        // WS 3/4: last letter of each word has bit 7 set, soft returns
        let mut v = vec![];
        for w in ["The", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog", "again", "and", "again"] {
            v.extend_from_slice(&w.as_bytes()[..w.len() - 1]);
            v.push(w.as_bytes()[w.len() - 1] | 0x80);
            v.push(b' ');
        }
        v.extend_from_slice(b"\x8D\nend\r\n");
        assert!(probe(&v));
        let d = read(&v).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "The quick brown fox jumps over the lazy dog again and again end");
        // CP866 extended characters
        let mut r = vec![];
        for &c in &[0x8F, 0xE0, 0xA8, 0xA2, 0xA5, 0xE2, 0x20, 0xAC, 0xA8, 0xE0] {
            if c == 0x20 {
                r.push(b' ');
            } else {
                r.extend_from_slice(&[0x1B, c, 0x1C]);
            }
        }
        let d = read(&r).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "Привет мир");
    }

    #[test]
    fn plain_text_is_not_wordstar() {
        let de = "Grüße aus Köln: Äpfel, Öl und Übermaß für die Bäckerei. Café au lait.\r\n".repeat(20);
        let ru = "Привет, мир! Это проверка русского текста. ОН ОН\n".repeat(20);
        let mut samples: Vec<Vec<u8>> = vec![de.as_bytes().to_vec(), ru.as_bytes().to_vec()];
        samples.push(de.chars().map(|c| c as u32 as u8).collect()); // 1252
        samples.push(de.chars().map(|c| if (c as u32) < 0x80 { c as u8 } else { 0x80 + crate::fileio::CP850_HIGH.chars().position(|x| x == c).unwrap_or(0) as u8 }).collect());
        for enc in [Enc::Cp866, Enc::Koi8r, Enc::Cp1251] {
            samples.push(ru.chars().map(|c| if (c as u32) < 0x80 { c as u8 } else { (0x80..=0xFF).find(|&b| crate::fmt_text::byte_char(b, enc) == c).unwrap_or(b'?') }).collect());
        }
        for s in samples {
            assert!(!probe(&s), "{:?}", String::from_utf8_lossy(&s[..40]));
            assert_eq!(crate::fileio::detect(std::path::Path::new("brief.txt"), &s), crate::fileio::Kind::Text);
        }
    }

    #[test]
    fn damaged_input_never_panics() {
        let s = sample_bytes();
        for n in 0..s.len() {
            let _ = read(&s[..n]);
        }
        let mut g = s.clone();
        for i in (0..g.len()).step_by(7) {
            g[i] ^= 0x1D;
        }
        let _ = read(&g);
        let _ = read(&[0x1D, 0xFF, 0xFF, 3, 0x1B]);
    }
}
