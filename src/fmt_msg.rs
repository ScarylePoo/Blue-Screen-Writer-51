//! Outlook e-mail messages (.msg): an OLE compound file with one stream per
//! MAPI property (`__substg1.0_<tag>`).  Shown as a header (From, To, Cc,
//! Date, Subject, attachment names) followed by the message text.

use crate::doc::*;
use crate::msdoc::Cfb;

/// Is this compound file an Outlook message?
pub fn probe(bytes: &[u8]) -> bool {
    let Ok(c) = Cfb::parse(bytes) else { return false };
    c.stream("__properties_version1.0").is_some() && (c.stream("__substg1.0_001A001F").is_some() || c.stream("__substg1.0_001A001E").is_some() || c.stream("__substg1.0_0037001F").is_some() || c.stream("__substg1.0_0037001E").is_some())
}

/// A string property: Unicode (001F) or 8-bit (001E) variant.
fn prop(c: &Cfb, id: &str, storage: Option<&str>) -> Option<String> {
    let get = |name: &str| match storage {
        Some(s) => c.stream_in(s, name),
        None => c.stream(name),
    };
    if let Some(b) = get(&format!("__substg1.0_{id}001F")) {
        let u: Vec<u16> = b.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
        let s = String::from_utf16_lossy(&u);
        return Some(s.trim_end_matches('\0').to_string());
    }
    let b = get(&format!("__substg1.0_{id}001E"))?;
    Some(crate::fileio::decode_text(&b).trim_end_matches('\0').to_string())
}

/// A time property (FILETIME) from the fixed-size property stream.
fn time_prop(c: &Cfb, ids: &[u16]) -> Option<i64> {
    let p = c.stream("__properties_version1.0")?;
    // top-level message: 32-byte header, then 16-byte entries
    for e in p.get(32..)?.chunks_exact(16) {
        let tag = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        if tag & 0xFFFF == 0x0040 && ids.contains(&((tag >> 16) as u16)) {
            let ft = u64::from_le_bytes(e[8..16].try_into().ok()?);
            if ft > 0 {
                return Some(ft as i64 / 10_000_000 - 11_644_473_600);
            }
        }
    }
    None
}

fn fmt_time(unix: i64) -> String {
    let local = unix + crate::sys::utc_offset_at(unix);
    let (y, m, d) = crate::filelist::civil_from_days(local.div_euclid(86400));
    let r = local.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", r / 3600, (r % 3600) / 60)
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let c = Cfb::parse(bytes)?;
    let from = {
        let name = prop(&c, "0C1A", None).unwrap_or_default();
        let addr = prop(&c, "5D01", None).or_else(|| prop(&c, "0C1F", None)).unwrap_or_default();
        match (name.is_empty(), addr.is_empty() || addr.starts_with("/O=") || name.contains(&addr)) {
            (true, _) => addr,
            (false, true) => name,
            (false, false) => format!("{name} <{addr}>"),
        }
    };
    let attachments: Vec<String> = c
        .storages()
        .iter()
        .filter(|s| s.starts_with("__attach_version1.0_"))
        .filter_map(|s| prop(&c, "3707", Some(s)).or_else(|| prop(&c, "3704", Some(s))).or_else(|| prop(&c, "3001", Some(s))))
        .filter(|n| !n.is_empty())
        .collect();
    let fields = [
        ("From", from),
        ("To", prop(&c, "0E04", None).unwrap_or_default()),
        ("Cc", prop(&c, "0E03", None).unwrap_or_default()),
        // sent (client submit) time, else delivery time
        ("Date", time_prop(&c, &[0x0039, 0x0E06]).map(fmt_time).unwrap_or_default()),
        ("Subject", prop(&c, "0037", None).unwrap_or_default()),
        ("Attachments", attachments.join(", ")),
    ];
    let mut els = vec![];
    for (label, value) in fields {
        if value.trim().is_empty() {
            continue;
        }
        els.push(El::Code(Code::AttrOn(Attr::Bold)));
        els.extend(format!("{label}:").chars().map(El::Ch));
        els.push(El::Code(Code::AttrOff(Attr::Bold)));
        els.push(El::Ch(' '));
        els.extend(value.chars().filter(|c| !c.is_control()).map(El::Ch));
        els.push(El::Code(Code::HRt));
    }
    if !els.is_empty() {
        els.push(El::Code(Code::HRt));
    }
    // the message text: plain body, else the HTML body
    let body = prop(&c, "1000", None).unwrap_or_default();
    let mut d = if !body.trim().is_empty() {
        Document::from_text(&clean(&body))
    } else if let Some(h) = c.stream("__substg1.0_10130102") {
        crate::fmt_html::read(&h).unwrap_or_else(|_| Document::from_text(""))
    } else {
        Document::from_text("")
    };
    els.append(&mut d.els);
    d.els = els;
    d.summary.subject = prop(&c, "0037", None).unwrap_or_default();
    Ok(d)
}

/// Line ends to \n, no other control characters.
fn clean(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n").chars().filter(|&c| c == '\n' || c == '\t' || !c.is_control()).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn text(f: &str) -> String {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/formats").join(f);
        let d = crate::fileio::load(&p).unwrap();
        d.plain_text(0, d.len())
    }

    #[test]
    fn outlook_message_header_and_body() {
        let t = text("outlook_plain.msg");
        assert!(t.contains("To: Someone Else"), "{t}");
        assert!(t.contains("Subject: Test for MSGConvert -- plain text"), "{t}");
        assert!(t.contains("The body is in plain text"), "{t}");
        let t = text("outlook_embedded.msg");
        assert!(t.contains("From: Joseph Q Bloggs"), "{t}");
        assert!(t.contains("Date: 2019-10-0"), "{t}");
    }

    #[test]
    fn anything_opens_as_text() {
        // binary junk with a readable string inside
        let mut b = vec![0u8, 1, 2, 3, 0xFF, 0x00, 0x07];
        b.extend_from_slice(b"Hello readable part");
        b.extend_from_slice(&[0, 0, 5, 6, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15]);
        let d = crate::fileio::load_bytes(Path::new("x.bin"), &b, 0).unwrap();
        assert!(d.plain_text(0, d.len()).contains("Hello readable part"));
        // a damaged "Word" file (compound file signature, nothing else)
        let mut w = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        w.extend_from_slice(&[0; 600]);
        w.extend_from_slice(b"some text left in it");
        let d = crate::fileio::load_bytes(Path::new("broken.doc"), &w, 0).unwrap();
        assert!(d.plain_text(0, d.len()).contains("some text left in it"));
        // empty file
        assert!(crate::fileio::load_bytes(Path::new("e.xyz"), &[], 0).is_ok());
    }
}
