//! Ami Pro (.sam) document reader — best effort (there is no public
//! specification; written from observed files and descriptions).
//!
//! A .sam file is Windows-1252 text in sections: `[ver]`, `[sty]`,
//! `[fnt]`, `[tag]` (paragraph styles), … then `[edoc]` followed by the
//! document text up to the next `[section]` line (`[ftnote]` holds
//! footnote texts).  In the text:
//! * paragraphs are separated by an empty line; a paragraph may start
//!   with its style tag `@Style Name@`; single line breaks are wrapping;
//! * `<+X>` / `<-X>` switch an attribute on/off: `!` bold, `"` italic,
//!   `#` underline, `$` word underline, `'` double underline, `&` small
//!   caps, `(` superscript, `)` subscript, `*` strikeout;
//! * `<\X>` is the character X + 0x80 (accented letters: `<\i>` = é);
//! * `<<` is a literal `<`; `<:#…>` is a footnote reference; other
//!   `<…>` codes (fonts, frames, fields) are skipped.

use crate::doc::*;
use crate::fmt_build::Builder;

fn attr_of(c: char) -> Option<Attr> {
    Some(match c {
        '!' => Attr::Bold,
        '"' => Attr::Italic,
        '#' | '$' => Attr::Underline,
        '\'' => Attr::DblUnderline,
        '&' => Attr::SmallCaps,
        '(' => Attr::Superscript,
        ')' => Attr::Subscript,
        '*' => Attr::Strikeout,
        _ => return None,
    })
}

fn is_section(line: &str) -> Option<String> {
    let t = line.trim_end();
    if t.len() >= 3 && t.starts_with('[') && t.ends_with(']') && t[1..t.len() - 1].chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(t[1..t.len() - 1].to_ascii_lowercase())
    } else {
        None
    }
}

/// Split the file into (section name, lines).
fn sections(text: &str) -> Vec<(String, Vec<&str>)> {
    let mut out: Vec<(String, Vec<&str>)> = vec![(String::new(), vec![])];
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(s) = is_section(line) {
            out.push((s, vec![]));
        } else if let Some(last) = out.last_mut() {
            last.1.push(line);
        }
    }
    out
}

/// Paragraphs: groups of non-empty lines.
fn paragraphs<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut out = vec![];
    let mut cur: Vec<&str> = vec![];
    for &l in lines {
        if l.trim().is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            if l.starts_with('@') && !cur.is_empty() && l[1..].contains('@') {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(l);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Write one paragraph's text (with codes) into the builder.
fn emit_para(b: &mut Builder, lines: &[&str], notes: &mut std::collections::VecDeque<Vec<El>>, heading_bold: bool) {
    let mut text = String::new();
    for (k, l) in lines.iter().enumerate() {
        if k > 0 && !text.ends_with(' ') && !text.ends_with('-') {
            text.push(' ');
        }
        text.push_str(l.trim_end_matches('\r'));
    }
    let mut s: &str = &text;
    b.just = Justify::Left;
    let mut style_bold = false;
    if let Some(rest) = s.strip_prefix('@') {
        if let Some(e) = rest.find('@') {
            let style = rest[..e].to_ascii_lowercase();
            s = &rest[e + 1..];
            if heading_bold && (style.contains("title") || style.contains("heading") || style.starts_with("head") || style.contains("berschrift")) {
                style_bold = true;
            }
            if style.contains("center") {
                b.just = Justify::Center;
            }
        }
    }
    let mut attrs = AttrSet::default();
    let apply = |b: &mut Builder, attrs: AttrSet| {
        b.attrs = attrs;
        if style_bold {
            b.set(Attr::Bold, true);
        }
    };
    apply(b, attrs);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            if chars.get(i + 1) == Some(&'<') {
                b.ch('<');
                i += 2;
                continue;
            }
            // find the closing '>'
            let Some(len) = chars[i + 1..].iter().position(|&x| x == '>') else {
                b.ch('<');
                i += 1;
                continue;
            };
            let code: String = chars[i + 1..i + 1 + len].iter().collect();
            i += len + 2;
            let mut cc = code.chars();
            match (cc.next(), cc.next()) {
                (Some(on @ ('+' | '-')), Some(x)) if code.chars().count() == 2 => {
                    if let Some(a) = attr_of(x) {
                        attrs.set(a, on == '+');
                        apply(b, attrs);
                    }
                }
                (Some('\\'), Some(x)) if code.chars().count() == 2 => {
                    let v = x as u32;
                    if (0x20..0x80).contains(&v) {
                        b.ch(crate::rtf::decode_byte(v as u8 | 0x80, 1252));
                    }
                }
                (Some(':'), Some('#')) => {
                    if let Some(n) = notes.pop_front() {
                        b.note(NoteKind::Foot, n);
                    }
                }
                _ => {}
            }
            continue;
        }
        b.ch(c);
        i += 1;
    }
    b.attrs = AttrSet::default();
    b.para();
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let text: String = if std::str::from_utf8(bytes).is_ok() && bytes.iter().any(|&b| b >= 0x80) {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        bytes.iter().map(|&b| crate::rtf::decode_byte(b, 1252)).collect()
    };
    let secs = sections(&text);
    let Some((_, body)) = secs.iter().find(|(n, _)| n == "edoc") else {
        if secs.iter().any(|(n, _)| n == "ver") {
            return Ok(Document::new());
        }
        return Err("not an Ami Pro document".into());
    };
    let font = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    // footnote texts, in order
    let mut notes = std::collections::VecDeque::new();
    if let Some((_, fl)) = secs.iter().find(|(n, _)| n == "ftnote") {
        for p in paragraphs(fl) {
            let mut nb = Builder::new(font.clone());
            let mut none = std::collections::VecDeque::new();
            let mut lines = p.clone();
            // a leading footnote number / code line
            if let Some(first) = lines.first_mut() {
                let t = first.trim_start();
                let t = t.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start();
                *first = t;
            }
            emit_para(&mut nb, &lines, &mut none, false);
            notes.push_back(nb.finish());
        }
    }
    let mut b = Builder::new(font.clone());
    for p in paragraphs(body) {
        emit_para(&mut b, &p, &mut notes, true);
    }
    Ok(b.into_document(font))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    fn sample() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/sample.sam")).unwrap()
    }

    #[test]
    fn sample_sam() {
        let b = sample();
        assert_eq!(crate::fileio::sniff(&b), crate::fileio::Kind::AmiPro);
        let d = read(&b).unwrap();
        assert_eq!(debug_text(&d.els), "Überschrift\nGrüße aus Köln, fett und kursiv <Café>.[Footnote:Fußnote hier.]\nZweiter Absatz über zwei Zeilen.");
        let bold = d.els.iter().filter(|e| **e == El::Code(Code::AttrOn(Attr::Bold))).count();
        assert_eq!(bold, 2);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
    }

    #[test]
    fn damaged_input_never_panics() {
        let s = sample();
        for n in 0..s.len() {
            let _ = read(&s[..n]);
        }
        let _ = read(b"[ver]\n[edoc]\n<+!<\\<:#");
        let _ = read(&[0xFF; 50]);
    }
}
