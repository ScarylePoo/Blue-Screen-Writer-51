//! Loading and saving documents in every format we support.

use crate::doc::*;
use crate::docx;
use std::path::{Path, PathBuf};

pub const CP850_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜø£Ø×ƒáíóúñÑªº¿®¬½¼¡«»░▒▓│┤ÁÂÀ©╣║╗╝¢¥┐└┴┬├─┼ãÃ╚╔╩╦╠═╬¤ðÐÊËÈıÍÎÏ┘┌█▄¦Ì▀ÓßÔÒõÕµþÞÚÛÙýÝ¯´\u{ad}±‗¾¶§÷¸°¨·¹³²■\u{a0}";
pub const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
const CP1252_80: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8d}Ž\u{8f}\u{90}‘’“”•–—˜™š›œ\u{9d}žŸ";

pub fn decode_cp(bytes: &[u8], high: &str) -> String {
    let table: Vec<char> = high.chars().collect();
    bytes.iter().map(|&b| if b < 0x80 { b as char } else { table[(b - 0x80) as usize] }).collect()
}

pub fn decode_1252(bytes: &[u8]) -> String {
    let t: Vec<char> = CP1252_80.chars().collect();
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => t[(b - 0x80) as usize],
            _ => b as char, // Latin-1 = Unicode for A0..FF
        })
        .collect()
}

/// Decode a plain-text file: UTF-8/UTF-16 (with or without BOM) or the
/// most plausible 8-bit code page (Windows-1252/1251/1250, DOS 850/866,
/// KOI8-R, Mac Roman) — see `fmt_text`.
pub fn decode_text(bytes: &[u8]) -> String {
    crate::fmt_text::decode_auto(bytes)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Kind {
    /// Any ZIP package (.docx/.docm/.dotx/.dotm …)
    Docx,
    Wp5,
    /// WordPerfect 6–12 / X3–X9 (`FF WPC`, major version 2)
    Wp6,
    /// WordPerfect for Macintosh 2.x/3.x (`FF WPC`, file type 0x2C)
    WpMac,
    /// WordPerfect 4.2 and earlier (no header)
    Wp42,
    /// OLE compound file: Word 97–2003 / 6 / 95, or Works (.wps)
    OleDoc,
    Rtf,
    Text,
    /// Our own code-stream text format (`#WPSTREAM 1`)
    Stream,
    /// Microsoft Write (.wri) and Word for DOS
    Write,
    /// Word for Windows 1.x / 2.0
    Word2,
    /// Word 2003 XML (`w:wordDocument`) or Word flat OPC XML
    WordXml,
    Abiword,
    /// gzip-compressed file (e.g. .zabw)
    Gzip,
    AmiPro,
    WordStar,
    Html,
    Markdown,
    Pdf,
}

/// Content signature only.
pub fn sniff(bytes: &[u8]) -> Kind {
    if bytes.starts_with(b"PK\x03\x04") {
        Kind::Docx
    } else if bytes.starts_with(b"\xFFWPC") {
        let (ftype, major) = (bytes.get(9).copied().unwrap_or(0), bytes.get(10).copied().unwrap_or(0));
        if ftype == 0x2C {
            Kind::WpMac
        } else if major >= 2 {
            Kind::Wp6
        } else {
            Kind::Wp5
        }
    } else if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        Kind::OleDoc
    } else if bytes.starts_with(b"{\\rtf") {
        Kind::Rtf
    } else if bytes.starts_with(&[0x1F, 0x8B]) {
        Kind::Gzip
    } else if bytes.len() >= 6 && matches!(bytes[0], 0x31 | 0x32) && bytes[1..6] == [0xBE, 0, 0, 0, 0xAB] {
        Kind::Write
    } else if bytes.len() >= 4 && matches!(u16::from_le_bytes([bytes[0], bytes[1]]), 0xA59B | 0xA59C | 0xA5DB) {
        Kind::Word2
    } else if bytes.starts_with(b"#WPSTREAM 1") {
        Kind::Stream
    } else if bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
        Kind::Pdf
    } else {
        sniff_markup(bytes)
    }
}

/// Text-based formats recognised by their first few KB.
fn sniff_markup(bytes: &[u8]) -> Kind {
    let head = &bytes[..bytes.len().min(4096)];
    // UTF-16 markup: drop the zero bytes for the purpose of sniffing
    let head: Vec<u8> = if head.iter().filter(|&&b| b == 0).count() * 4 > head.len() { head.iter().copied().filter(|&b| b != 0).collect() } else { head.to_vec() };
    let lower: String = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let t = lower.trim_start_matches(['\u{feff}', '\u{fffd}', ' ', '\t', '\r', '\n']);
    if t.starts_with("[ver]") {
        return Kind::AmiPro;
    }
    if t.starts_with('<') {
        if lower.contains("<w:worddocument") || lower.contains("<pkg:package") {
            return Kind::WordXml;
        }
        if lower.contains("<abiword") || lower.contains("<!doctype abiword") {
            return Kind::Abiword;
        }
        if lower.contains("<html") || lower.contains("<!doctype html") || lower.contains("<body") {
            return Kind::Html;
        }
    }
    if crate::fmt_wordstar::probe(bytes) {
        return Kind::WordStar;
    }
    Kind::Text
}

/// Content first, file name extension second.
pub fn detect(p: &Path, bytes: &[u8]) -> Kind {
    let k = sniff(bytes);
    if k != Kind::Text {
        return k;
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" | "mkd" | "mdown" => return Kind::Markdown,
        "htm" | "html" | "xhtml" | "shtml" | "xht" => return Kind::Html,
        "sam" => return Kind::AmiPro,
        "ws" | "wsd" | "ws1" | "ws2" | "ws3" | "ws4" | "ws5" | "ws6" | "ws7" if crate::fmt_wordstar::probe_weak(bytes) => return Kind::WordStar,
        "abw" => return Kind::Abiword,
        _ => {}
    }
    let wp_ext = matches!(ext.as_str(), "wp" | "wp4" | "wp42" | "wpd" | "doc" | "wpt" | "");
    if crate::fmt_wp42::probe(bytes, wp_ext) {
        return Kind::Wp42;
    }
    Kind::Text
}

/// Is the file a password-protected (encrypted) Office document?
pub fn is_encrypted(p: &Path) -> bool {
    std::fs::read(p).is_ok_and(|b| crate::agile::is_encrypted_package(&b))
}

/// Open an encrypted .docx; the password is kept so it is saved encrypted again.
pub fn load_with_password(p: &Path, password: &str) -> std::io::Result<Document> {
    let bytes = std::fs::read(p)?;
    let plain = crate::agile::decrypt(&bytes, password).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, if e.contains("wrong") { "Incorrect password".to_string() } else { e }))?;
    let mut d = load_bytes(p, &plain, 0)?;
    d.password = Some(password.to_string());
    Ok(d)
}

/// Read a document in any supported format.
pub fn load(p: &Path) -> std::io::Result<Document> {
    let bytes = std::fs::read(p)?;
    if crate::agile::is_encrypted_package(&bytes) {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "password protected"));
    }
    load_bytes(p, &bytes, 0)
}

/// Load from memory; `p` only supplies the file name extension.
pub fn load_bytes(p: &Path, bytes: &[u8], depth: u32) -> std::io::Result<Document> {
    // OpenDocument (.odt/.ott zip by mimetype, .fodt flat XML)
    if bytes.starts_with(b"PK\x03\x04") && (crate::odt::is_odt(bytes) || crate::odt::is_ott(bytes)) {
        return crate::odt::read_odt(bytes);
    }
    if crate::odt::is_fodt(bytes) {
        return crate::odt::read_fodt(bytes);
    }
    let err = |e: String| std::io::Error::other(e);
    match detect(p, bytes) {
        Kind::Docx => docx::read_docx(bytes),
        Kind::Wp5 => crate::wp5::read(bytes).map_err(err),
        Kind::Wp6 => crate::fmt_wp6::read(bytes).map_err(err),
        Kind::WpMac => crate::fmt_wpmac::read(bytes).map_err(err),
        Kind::Wp42 => crate::fmt_wp42::read(bytes).map_err(err),
        Kind::OleDoc => {
            if crate::fmt_works::probe(bytes) {
                crate::fmt_works::read(bytes).map_err(err)
            } else {
                crate::msdoc::read(bytes).map_err(err)
            }
        }
        Kind::Rtf => crate::rtf::read(bytes).map_err(err),
        Kind::Stream => {
            let s = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
            crate::stream::read(&s).ok_or_else(|| err("damaged code stream file".into()))
        }
        Kind::Write => crate::fmt_write::read(bytes).map_err(err),
        Kind::Word2 => crate::fmt_word2::read(bytes).map_err(err),
        Kind::WordXml => crate::fmt_wordxml::read(bytes).map_err(err),
        Kind::Abiword => crate::fmt_abw::read(bytes).map_err(err),
        Kind::Gzip => {
            if depth > 2 {
                return Err(err("nested compressed file".into()));
            }
            let mut out = Vec::new();
            let r = flate2::read::GzDecoder::new(bytes);
            std::io::Read::read_to_end(&mut std::io::Read::take(r, 512 << 20), &mut out).map_err(|e| err(format!("damaged compressed file: {e}")))?;
            // name.zabw → treat as .abw, name.xyz.gz → .xyz
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let inner = if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zabw")) { PathBuf::from(format!("{stem}.abw")) } else { PathBuf::from(stem) };
            load_bytes(&inner, &out, depth + 1)
        }
        Kind::AmiPro => crate::fmt_amipro::read(bytes).map_err(err),
        Kind::WordStar => crate::fmt_wordstar::read(bytes).map_err(err),
        Kind::Html => crate::fmt_html::read(bytes).map_err(err),
        Kind::Markdown => crate::fmt_md::read(bytes).map_err(err),
        Kind::Pdf => crate::fmt_pdf::read(bytes).map_err(err),
        Kind::Text => {
            let s = decode_text(bytes);
            let s = s.replace("\r\n", "\n");
            Ok(Document::from_text(&s))
        }
    }
}

/// Write atomically: to a temporary file next to the target, then rename.
/// Works the same on local disks, mapped drives and UNC paths.
/// Setup: Original Document Backup (.BK!).
pub static ORIGINAL_BACKUP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn write_atomic(p: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    let tmp = dir.join(format!("~{name}.wptmp"));
    std::fs::write(&tmp, data)?;
    // Keep one backup of the previous version (WP 5.1 "Original Document
    // Backup" — always on here, it's cheap insurance).
    if p.exists() && ORIGINAL_BACKUP.load(std::sync::atomic::Ordering::Relaxed) {
        let bk = dir.join(format!("{name}.bk!"));
        let _ = std::fs::remove_file(&bk);
        let _ = std::fs::copy(p, &bk);
    }
    match std::fs::rename(&tmp, p) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Some network shares refuse rename-over; fall back.
            let _ = std::fs::remove_file(p);
            let r = std::fs::rename(&tmp, p);
            if r.is_err() {
                let r2 = std::fs::write(p, data);
                let _ = std::fs::remove_file(&tmp);
                return r2;
            }
            r
        }
    }
}

pub fn save(p: &Path, doc: &Document) -> std::io::Result<()> {
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "txt" | "asc" => {
            let s = doc.plain_text(0, doc.len()).replace('\n', "\r\n");
            write_atomic(p, s.as_bytes())
        }
        "wps" => write_atomic(p, crate::stream::write(doc).as_bytes()),
        "rtf" => write_atomic(p, crate::export::to_rtf(doc).as_bytes()),
        "odt" => write_atomic(p, &crate::odt::write_odt(doc)?),
        "ott" => write_atomic(p, &crate::odt::write_ott(doc)?),
        "fodt" => write_atomic(p, crate::odt::write_fodt(doc).as_bytes()),
        "htm" | "html" => write_atomic(p, crate::export::to_html(doc).as_bytes()),
        "md" | "markdown" => write_atomic(p, crate::fmt_md::write(doc).as_bytes()),
        _ => match &doc.password {
            Some(pw) if !pw.is_empty() => write_atomic(p, &crate::agile::encrypt(&docx::write_docx(doc)?, pw)),
            _ => write_atomic(p, &docx::write_docx(doc)?),
        },
    }
}

/// Plain text in a chosen encoding (CR/LF).  Returns how many characters
/// the code page could not represent (written as '?').
pub fn save_text(p: &Path, doc: &Document, enc: crate::export::TextEnc) -> std::io::Result<usize> {
    let (bytes, lost) = crate::export::to_text(doc, enc);
    write_atomic(p, &bytes)?;
    Ok(lost)
}

/// Block → Append to a file (adds to the end of a document).
pub fn append(p: &Path, els: &[El]) -> std::io::Result<()> {
    let mut d = if p.exists() { load(p)? } else { Document::new() };
    d.els.extend_from_slice(els);
    save(p, &d)
}

/// Names typed without extension get ".docx" — the native format.
pub fn with_default_ext(p: &Path) -> PathBuf {
    if p.extension().is_none() && !p.exists() {
        let mut s = p.as_os_str().to_owned();
        s.push(".docx");
        PathBuf::from(s)
    } else {
        p.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_complete() {
        assert_eq!(CP850_HIGH.chars().count(), 128);
        assert_eq!(CP437_HIGH.chars().count(), 128);
        assert_eq!(CP1252_80.chars().count(), 32);
    }
    #[test]
    fn detection() {
        let p = Path::new("x.txt");
        assert_eq!(detect(p, b"\xFFWPC\x10\0\0\0\x01\x0a\x02\x01"), Kind::Wp6);
        assert_eq!(detect(p, b"\xFFWPC\x10\0\0\0\x01\x0a\x00\x01"), Kind::Wp5);
        assert_eq!(detect(p, b"\xFFWPC\x10\0\0\0\x01\x2c\x02\x02"), Kind::WpMac);
        assert_eq!(detect(p, b"\x31\xBE\0\0\0\xAB\0\0"), Kind::Write);
        assert_eq!(detect(p, b"%PDF-1.4\n"), Kind::Pdf);
        assert_eq!(detect(p, b"#WPSTREAM 1\nthallo\n"), Kind::Stream);
        assert_eq!(detect(p, b"<?xml version=\"1.0\"?><?mso-application progid=\"Word.Document\"?><w:wordDocument xmlns:w=\"x\">"), Kind::WordXml);
        assert_eq!(detect(p, b"<!DOCTYPE html><html><body>x"), Kind::Html);
        assert_eq!(detect(p, b"[ver]\r\n\t4\r\n"), Kind::AmiPro);
        assert_eq!(detect(Path::new("a.md"), b"# Titel\n\ntext"), Kind::Markdown);
        assert_eq!(detect(Path::new("a.htm"), b"Hallo <b>Welt</b>"), Kind::Html);
        // ordinary text in various encodings stays text
        for t in [&b"Gr\xfc\xdfe aus K\xf6ln\r\n"[..], b"\xcf\xf0\xe8\xe2\xe5\xf2 \xec\xe8\xf0", b"F\x81r \x84rger", "Привет".as_bytes()] {
            assert_eq!(detect(Path::new("a.txt"), t), Kind::Text);
            assert_eq!(detect(Path::new("a.doc"), t), Kind::Text);
            assert_eq!(detect(Path::new("a"), t), Kind::Text);
        }
    }

    #[test]
    fn stream_file_loads_as_document() {
        let mut d = Document::from_text("Hallo");
        d.els.insert(0, El::Code(Code::AttrOn(Attr::Bold)));
        d.els.push(El::Code(Code::AttrOff(Attr::Bold)));
        let s = crate::stream::write(&d);
        let d2 = load_bytes(Path::new("x.wps"), s.as_bytes(), 0).unwrap();
        assert_eq!(d2.els, d.els);
    }

    #[test]
    fn dos_german() {
        let b = [b'F', 0x81, b'r', b' ', 0xE1];
        assert_eq!(decode_text(&b), "Für ß");
    }
}
