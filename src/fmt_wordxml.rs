//! Word 2003 XML (`<w:wordDocument>`, "WordprocessingML 2003") and Word
//! 2007+ flat OPC XML (`<pkg:package>`) reader.
//!
//! Both are close relatives of .docx, so the .docx reader does the real
//! work:
//! * flat OPC: the package parts are pulled out of `pkg:part/pkg:xmlData`
//!   and handed to `docx::read_parts`;
//! * Word 2003 XML: the body is rewritten into .docx-style markup —
//!   paragraph/character styles are resolved and inlined (`w:pStyle` →
//!   its `w:jc` and run properties, `w:rStyle` → its run properties),
//!   inline `w:footnote`/`w:endnote` become references plus a notes map,
//!   `w:hdr`/`w:ftr` (inside `w:sectPr`) become headers/footers, images
//!   (`w:binData`, `w:pict`) are skipped — and then parsed with
//!   `docx::parse_document_xml_ext`.

use crate::doc::*;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use std::collections::HashMap;

fn local(n: &[u8]) -> &[u8] {
    match n.iter().position(|&c| c == b':') {
        Some(p) => &n[p + 1..],
        None => n,
    }
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| local(a.key.as_ref()) == key).and_then(|a| a.unescape_value().ok().map(|v| v.to_string()))
}

fn decode(bytes: &[u8]) -> String {
    use crate::fmt_text::{Enc, decode as dec, detect};
    let (enc, bom) = detect(&bytes[..bytes.len().min(4096)]);
    if matches!(enc, Enc::Utf16Le | Enc::Utf16Be) {
        return dec(&bytes[bom..], enc);
    }
    let body = &bytes[bom..];
    // XML declaration encoding (Word writes UTF-8, but be tolerant)
    let head = String::from_utf8_lossy(&body[..body.len().min(200)]).to_string();
    if let Some(k) = head.find("encoding=") {
        let rest = &head[k + 9..];
        let q = rest.chars().next().unwrap_or('"');
        if let Some(label) = rest[1..].split(q).next() {
            if let Some(e) = crate::fmt_text::enc_for_label(label) {
                if !matches!(e, Enc::Utf8 | Enc::Utf16Le | Enc::Utf16Be) {
                    return dec(body, e);
                }
            }
        }
    }
    String::from_utf8_lossy(body).into_owned()
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let xml = decode(bytes);
    if xml.contains("<pkg:package") {
        return read_flat_opc(&xml);
    }
    if !xml.contains("wordDocument") {
        return Err("not a Word XML document".into());
    }
    read_2003(&xml)
}

// ------------------------------------------------------------- flat OPC

/// Parts of a flat OPC package: name (without leading '/') → XML text.
fn flat_parts(xml: &str) -> HashMap<String, String> {
    let mut parts = HashMap::new();
    let mut i = 0;
    while let Some(k) = xml[i..].find("<pkg:part ") {
        let s = i + k;
        let Some(end) = xml[s..].find("</pkg:part>").map(|x| s + x) else { break };
        let part = &xml[s..end];
        i = end + 11;
        let head_end = part.find('>').unwrap_or(part.len());
        let head = &part[..head_end];
        let Some(name) = head.find("pkg:name=\"").map(|p| p + 10).and_then(|p| head[p..].find('"').map(|q| &head[p..p + q])) else { continue };
        let (Some(a), Some(b)) = (part.find("<pkg:xmlData>"), part.rfind("</pkg:xmlData>")) else { continue };
        if a + 13 <= b {
            parts.insert(name.trim_start_matches('/').to_string(), part[a + 13..b].to_string());
        }
    }
    parts
}

fn read_flat_opc(xml: &str) -> Result<Document, String> {
    let parts = flat_parts(xml);
    if !parts.contains_key("word/document.xml") {
        return Err("not a Word XML document (no word/document.xml part)".into());
    }
    let mut look = |n: &str| parts.get(n).cloned();
    crate::docx::read_parts(&mut look).map_err(|e| e.to_string())
}

// ------------------------------------------------------------- Word 2003

#[derive(Default, Clone)]
struct Style {
    based_on: Option<String>,
    rpr: String,
    jc: Option<String>,
    is_default_para: bool,
}

/// Inner XML of the first `<w:TAG …>…</w:TAG>` in `s` ("" if self-closing / absent).
fn inner_of(s: &str, tag: &str) -> Option<String> {
    let open = format!("<w:{tag}");
    let mut from = 0;
    loop {
        let k = s[from..].find(&open)? + from;
        let after = &s[k + open.len()..];
        // exact tag name (not e.g. w:rPrChange)
        match after.chars().next() {
            Some('>') | Some(' ') | Some('/') => {}
            _ => {
                from = k + open.len();
                continue;
            }
        }
        let gt = s[k..].find('>')? + k;
        if s[..gt].ends_with('/') {
            return Some(String::new());
        }
        let close = format!("</w:{tag}>");
        let e = s[gt..].find(&close)? + gt;
        return Some(s[gt + 1..e].to_string());
    }
}

fn val_of(s: &str, tag: &str) -> Option<String> {
    let open = format!("<w:{tag} ");
    let k = s.find(&open)?;
    let gt = s[k..].find('>')? + k;
    let t = &s[k..gt];
    let p = t.find("w:val=\"")? + 7;
    let q = t[p..].find('"')? + p;
    Some(t[p..q].to_string())
}

fn parse_styles(xml: &str) -> HashMap<String, Style> {
    let mut out = HashMap::new();
    let Some(st) = inner_of(xml, "styles") else { return out };
    let mut i = 0;
    while let Some(k) = st[i..].find("<w:style ") {
        let s = i + k;
        let Some(gt) = st[s..].find('>').map(|x| s + x) else { break };
        let head = &st[s..=gt];
        let (body, next) = if head.ends_with("/>") {
            ("", gt + 1)
        } else {
            match st[gt..].find("</w:style>") {
                Some(e) => (&st[gt + 1..gt + e], gt + e + 10),
                None => break,
            }
        };
        i = next;
        let get = |n: &str| {
            let p = head.find(&format!("{n}=\""))? + n.len() + 2;
            let q = head[p..].find('"')? + p;
            Some(head[p..q].to_string())
        };
        let Some(id) = get("w:styleId") else { continue };
        let typ = get("w:type").unwrap_or_default();
        // paragraph properties may contain their own rPr (paragraph mark):
        // only take the style's direct rPr
        let ppr = inner_of(body, "pPr").unwrap_or_default();
        let body_wo_ppr = body.replacen(&ppr, "", 1);
        let style = Style {
            based_on: val_of(body, "basedOn"),
            rpr: inner_of(&body_wo_ppr, "rPr").unwrap_or_default(),
            jc: if typ == "paragraph" { val_of(&ppr, "jc") } else { None },
            is_default_para: typ == "paragraph" && matches!(get("w:default").as_deref(), Some("on") | Some("1") | Some("true")),
        };
        out.insert(id, style);
    }
    out
}

/// Style with its `basedOn` chain folded in (rPr concatenated, later wins).
fn resolve(styles: &HashMap<String, Style>, id: &str) -> Style {
    let mut chain = vec![];
    let mut cur = Some(id.to_string());
    while let Some(c) = cur {
        if chain.len() > 12 || chain.contains(&c) {
            break;
        }
        let Some(s) = styles.get(&c) else { break };
        cur = s.based_on.clone();
        chain.push(c);
    }
    let mut out = Style::default();
    for c in chain.iter().rev() {
        let s = &styles[c];
        out.rpr.push_str(&s.rpr);
        if s.jc.is_some() {
            out.jc = s.jc.clone();
        }
    }
    out
}

struct Ctx {
    w: Writer<Vec<u8>>,
    para_rpr: String,
    kind: CtxKind,
}

#[derive(Clone)]
enum CtxKind {
    Body,
    Note(bool),
    HeadFoot(bool, String),
}

fn heading_rpr(id: &str) -> &'static str {
    // built-in heading styles that were not written out
    let l = id.to_ascii_lowercase();
    if l.starts_with("heading") || l.starts_with("berschrift") || l.starts_with("überschrift") { "<w:b/>" } else { "" }
}

fn read_2003(xml: &str) -> Result<Document, String> {
    let styles = parse_styles(xml);
    // base font from the default paragraph style (or w:defaultFonts)
    let mut default_rpr = styles.iter().find(|(_, s)| s.is_default_para).map(|(id, _)| resolve(&styles, id).rpr).unwrap_or_default();
    if !default_rpr.contains("rFonts") {
        if let Some(k) = xml.find("<w:defaultFonts ") {
            let t = &xml[k..xml[k..].find('>').map(|x| k + x).unwrap_or(xml.len())];
            if let Some(p) = t.find("w:ascii=\"") {
                let f = &t[p + 9..];
                let f = &f[..f.find('"').unwrap_or(0)];
                if !f.is_empty() {
                    default_rpr.push_str(&format!("<w:rFonts w:ascii=\"{f}\"/>"));
                }
            }
        }
    }
    let styles_xml = format!("<w:styles><w:docDefaults><w:rPrDefault><w:rPr>{default_rpr}</w:rPr></w:rPrDefault></w:docDefaults></w:styles>");

    let mut r = Reader::from_str(xml);
    let mut stack: Vec<Ctx> = vec![];
    let mut in_body = false;
    let mut skip: Option<(Vec<u8>, usize)> = None;
    let mut notes: HashMap<(bool, String), Vec<El>> = HashMap::new();
    let mut hf: Vec<(bool, String, String)> = vec![];
    let mut note_id = 0u32;
    let mut body_xml = String::new();
    loop {
        let ev = match r.read_event() {
            Ok(Event::Eof) => break,
            Ok(ev) => ev,
            Err(_) => break,
        };
        if let Some((name, depth)) = &mut skip {
            match &ev {
                Event::Start(e) if e.name().as_ref() == name.as_slice() => *depth += 1,
                Event::End(e) if e.name().as_ref() == name.as_slice() => {
                    *depth -= 1;
                    if *depth == 0 {
                        skip = None;
                    }
                }
                _ => {}
            }
            continue;
        }
        if !in_body {
            if let Event::Start(e) = &ev {
                if local(e.name().as_ref()) == b"body" {
                    in_body = true;
                    stack.push(Ctx { w: Writer::new(Vec::new()), para_rpr: String::new(), kind: CtxKind::Body });
                }
            }
            continue;
        }
        let Some(ctx) = stack.last_mut() else { break };
        match &ev {
            Event::Start(e) => {
                let ln = local(e.name().as_ref()).to_vec();
                match ln.as_slice() {
                    b"binData" | b"pict" | b"object" => {
                        skip = Some((e.name().as_ref().to_vec(), 1));
                        continue;
                    }
                    b"footnote" | b"endnote" => {
                        let foot = ln.as_slice() == b"footnote";
                        stack.push(Ctx { w: Writer::new(Vec::new()), para_rpr: String::new(), kind: CtxKind::Note(foot) });
                        continue;
                    }
                    b"hdr" | b"ftr" => {
                        let typ = attr(e, b"type").unwrap_or_else(|| "odd".into());
                        stack.push(Ctx { w: Writer::new(Vec::new()), para_rpr: String::new(), kind: CtxKind::HeadFoot(ln.as_slice() == b"ftr", typ) });
                        continue;
                    }
                    b"p" => {
                        ctx.para_rpr.clear();
                        let _ = ctx.w.write_event(ev.clone());
                    }
                    b"r" => {
                        let _ = ctx.w.write_event(ev.clone());
                        if !ctx.para_rpr.is_empty() {
                            let inj = format!("<w:rPr>{}</w:rPr>", ctx.para_rpr);
                            ctx.w.get_mut().extend_from_slice(inj.as_bytes());
                        }
                    }
                    b"pStyle" | b"rStyle" => {
                        let _ = ctx.w.write_event(ev.clone());
                        style_ref(ctx, &styles, e, ln.as_slice() == b"pStyle");
                    }
                    _ => {
                        let _ = ctx.w.write_event(ev.clone());
                    }
                }
            }
            Event::Empty(e) => {
                let ln = local(e.name().as_ref()).to_vec();
                match ln.as_slice() {
                    b"pStyle" | b"rStyle" => {
                        let _ = ctx.w.write_event(ev.clone());
                        style_ref(ctx, &styles, e, ln.as_slice() == b"pStyle");
                    }
                    b"footnote" | b"endnote" | b"binData" | b"pict" | b"hdr" | b"ftr" => {}
                    _ => {
                        let _ = ctx.w.write_event(ev.clone());
                    }
                }
            }
            Event::End(e) => {
                let ln = local(e.name().as_ref()).to_vec();
                match (ln.as_slice(), ctx.kind.clone()) {
                    (b"footnote" | b"endnote", CtxKind::Note(foot)) => {
                        let Some(c) = stack.pop() else { break };
                        let inner = String::from_utf8_lossy(&c.w.into_inner()).into_owned();
                        let d = crate::docx::parse_document_xml(&format!("<w:document><w:body>{inner}</w:body></w:document>"), Some(&styles_xml));
                        let mut els = d.els;
                        // drop the leading tab/space after the note mark
                        while matches!(els.first(), Some(El::Ch(' ')) | Some(El::Code(Code::Tab)) | Some(El::Code(Code::HRt))) {
                            els.remove(0);
                        }
                        while matches!(els.last(), Some(El::Code(Code::HRt))) {
                            els.pop();
                        }
                        note_id += 1;
                        notes.insert((foot, note_id.to_string()), els);
                        let tag = if foot { "footnoteReference" } else { "endnoteReference" };
                        if let Some(p) = stack.last_mut() {
                            p.w.get_mut().extend_from_slice(format!("<w:{tag} w:id=\"{note_id}\"/>").as_bytes());
                        }
                    }
                    (b"hdr" | b"ftr", CtxKind::HeadFoot(footer, typ)) => {
                        let Some(c) = stack.pop() else { break };
                        hf.push((footer, typ, String::from_utf8_lossy(&c.w.into_inner()).into_owned()));
                    }
                    (b"body", CtxKind::Body) => {
                        let Some(c) = stack.pop() else { break };
                        body_xml = String::from_utf8_lossy(&c.w.into_inner()).into_owned();
                        break;
                    }
                    _ => {
                        let _ = ctx.w.write_event(ev.clone());
                    }
                }
            }
            _ => {
                let _ = ctx.w.write_event(ev.clone());
            }
        }
    }
    if body_xml.is_empty() {
        // truncated file: use whatever the body writer collected
        if let Some(c) = stack.into_iter().next() {
            body_xml = String::from_utf8_lossy(&c.w.into_inner()).into_owned();
        }
    }
    if body_xml.is_empty() && !in_body {
        return Err("not a Word 2003 XML document (no w:body)".into());
    }
    let mut d = crate::docx::parse_document_xml_ext(&format!("<w:document><w:body>{body_xml}</w:body></w:document>"), Some(&styles_xml), &notes, &Default::default());

    // headers / footers
    let has_even = hf.iter().any(|(_, t, _)| t == "even");
    let mut hfe: Vec<El> = vec![];
    for (footer, typ, x) in hf {
        let occ = match typ.as_str() {
            "even" => Occurrence::Even,
            "first" => continue,
            _ if has_even => Occurrence::Odd,
            _ => Occurrence::Every,
        };
        let hd = crate::docx::parse_document_xml(&format!("<w:document><w:body>{x}</w:body></w:document>"), Some(&styles_xml));
        let mut els = hd.els;
        while matches!(els.last(), Some(El::Code(Code::HRt))) {
            els.pop();
        }
        if els.is_empty() {
            continue;
        }
        let which = if footer { 2 } else { 0 } + if occ == Occurrence::Even { 1 } else { 0 };
        if !hfe.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) {
            hfe.push(El::Code(Code::HeadFoot { which, occ, els }));
        }
    }
    if !hfe.is_empty() {
        let tail = std::mem::take(&mut d.els);
        d.els = hfe;
        d.els.extend(tail);
    }
    // summary
    let prop = |tag: &str| -> String {
        let open = format!("<o:{tag}>");
        let Some(k) = xml.find(&open) else { return String::new() };
        let rest = &xml[k + open.len()..];
        let e = rest.find("</o:").unwrap_or(0);
        rest[..e].replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").trim().to_string()
    };
    d.summary.descriptive_name = prop("Title");
    d.summary.subject = prop("Subject");
    d.summary.author = prop("Author");
    d.summary.comments = prop("Description");
    if xml.contains("<w:trackRevisions") {
        d.track = true;
    }
    Ok(d)
}

/// After a `w:pStyle`/`w:rStyle` element: inline the style's properties.
fn style_ref(ctx: &mut Ctx, styles: &HashMap<String, Style>, e: &BytesStart, para: bool) {
    let Some(id) = attr(e, b"val") else { return };
    let st = resolve(styles, &id);
    if para {
        if let Some(jc) = &st.jc {
            ctx.w.get_mut().extend_from_slice(format!("<w:jc w:val=\"{jc}\"/>").as_bytes());
        }
        ctx.para_rpr = st.rpr.clone();
        if !styles.contains_key(&id) {
            ctx.para_rpr = heading_rpr(&id).to_string();
        }
    } else {
        ctx.w.get_mut().extend_from_slice(st.rpr.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    fn sample() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/word2003.xml")).unwrap()
    }

    #[test]
    fn libreoffice_word2003() {
        let d = read(&sample()).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Grüße aus Köln, fett und kursiv."), "{t}");
        assert!(t.contains("Привет мир[Footnote:Fußnote hier.]"), "{t}");
        assert!(t.contains("[Header:Kopfzeile]"), "{t}");
        assert!(t.contains("|A1|B1") && t.contains("|A2|Ä2"), "{t}");
        assert!(t.ends_with("Ende."), "{t}");
        // character styles T2 (bold) / T3 (italic) resolved
        let k = d.els.iter().position(|e| *e == El::Ch('f')).unwrap();
        assert_eq!(d.els[k - 1], El::Code(Code::AttrOn(Attr::Bold)), "{:?}", &d.els[k - 3..k + 2]);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
        // paragraph style P1 → centered
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Center))));
        // detection
        assert_eq!(crate::fileio::sniff(&sample()), crate::fileio::Kind::WordXml);
    }

    #[test]
    fn hand_made_2003() {
        let x = r#"<?xml version="1.0"?><?mso-application progid="Word.Document"?>
<w:wordDocument xmlns:w="http://schemas.microsoft.com/office/word/2003/wordml" xmlns:o="urn:schemas-microsoft-com:office:office">
<o:DocumentProperties><o:Title>Brief</o:Title><o:Author>Maria</o:Author></o:DocumentProperties>
<w:styles><w:style w:type="paragraph" w:default="on" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:rFonts w:ascii="Arial" w:h-ansi="Arial"/><w:sz w:val="22"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:b/></w:rPr></w:style></w:styles>
<w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Titel</w:t></w:r></w:p>
<w:p><w:r><w:t>Text</w:t></w:r><w:r><w:endnote><w:p><w:r><w:t>End</w:t></w:r></w:p></w:endnote></w:r><w:r><w:pict><w:binData w:name="x">AAAA</w:binData></w:pict></w:r></w:p>
<w:sectPr><w:ftr w:type="odd"><w:p><w:r><w:t>Fuß</w:t></w:r></w:p></w:ftr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:wordDocument>"#;
        let d = read(x.as_bytes()).unwrap();
        assert_eq!(debug_text(&d.els), "[Footer:Fuß]Titel\nText[Endnote:End]");
        assert_eq!(d.init.font.name, "Arial");
        assert_eq!(d.init.font.size_cpt, 1100);
        assert_eq!(d.init.justify, Justify::Center);
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert_eq!(d.summary.descriptive_name, "Brief");
        assert_eq!(d.summary.author, "Maria");
        assert_eq!(d.init.page_width, 11906 * 1200 / 1440);
    }

    #[test]
    fn flat_opc() {
        let x = r#"<?xml version="1.0" standalone="yes"?><?mso-application progid="Word.Document"?>
<pkg:package xmlns:pkg="http://schemas.microsoft.com/office/2006/xmlPackage">
<pkg:part pkg:name="/_rels/.rels" pkg:contentType="application/vnd.openxmlformats-package.relationships+xml"><pkg:xmlData><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships></pkg:xmlData></pkg:part>
<pkg:part pkg:name="/word/document.xml" pkg:contentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"><pkg:xmlData><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:i/></w:rPr><w:t>Schöne Grüße</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p></w:body></w:document></pkg:xmlData></pkg:part>
<pkg:part pkg:name="/word/footnotes.xml"><pkg:xmlData><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="2"><w:p><w:r><w:t>Anmerkung</w:t></w:r></w:p></w:footnote></w:footnotes></pkg:xmlData></pkg:part>
</pkg:package>"#;
        assert_eq!(crate::fileio::sniff(x.as_bytes()), crate::fileio::Kind::WordXml);
        let d = read(x.as_bytes()).unwrap();
        assert_eq!(debug_text(&d.els), "Schöne Grüße[Footnote:Anmerkung]");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
    }

    #[test]
    fn docm_and_dotx_packages_load() {
        use std::io::Write;
        for (ext, ct) in [("docm", "application/vnd.ms-word.document.macroEnabled.main+xml"), ("dotx", "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml")] {
            let mut out = std::io::Cursor::new(Vec::new());
            {
                let mut z = zip::ZipWriter::new(&mut out);
                let o = zip::write::SimpleFileOptions::default();
                z.start_file("[Content_Types].xml", o).unwrap();
                write!(z, r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="{ct}"/></Types>"#).unwrap();
                z.start_file("word/document.xml", o).unwrap();
                z.write_all(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Vorlage äöü</w:t></w:r></w:p></w:body></w:document>"#.as_bytes()).unwrap();
                if ext == "docm" {
                    z.start_file("word/vbaProject.bin", o).unwrap();
                    z.write_all(b"\xD0\xCF\x11\xE0junk").unwrap();
                }
                z.finish().unwrap();
            }
            let bytes = out.into_inner();
            let p = std::path::PathBuf::from(format!("x.{ext}"));
            let d = crate::fileio::load_bytes(&p, &bytes, 0).unwrap();
            assert_eq!(d.plain_text(0, d.len()), "Vorlage äöü");
        }
    }

    #[test]
    fn damaged_input_never_panics() {
        let s = sample();
        for n in (0..s.len()).step_by(97) {
            let _ = read(&s[..n]);
        }
        let mut g = s.clone();
        for i in (0..g.len()).step_by(13) {
            g[i] ^= 0x5A;
        }
        let _ = read(&g);
        let _ = read(b"<w:wordDocument><w:body><w:p><w:r><w:footnote><w:p>");
        let _ = read(b"<pkg:package><pkg:part pkg:name=\"/word/document.xml\"><pkg:xmlData>");
    }
}
