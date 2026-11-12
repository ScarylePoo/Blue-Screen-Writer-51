//! Word (.docx) mapping of the Format codes (fmt.rs): landscape paper
//! (`w:pgSz w:orient`), line numbering (`w:lnNumType`), hyphenation
//! (`w:autoHyphenation`, `w:suppressAutoHyphens`), widow/orphan
//! protection (`w:widowControl`), block protect / conditional end of page
//! (`w:keepLines`, `w:keepNext`) and the language (`w:lang`).

use crate::doc::*;
use crate::fmt::{Fmt, LineNum, Paper};

/// The first `<w:name …>` tag (its text up to `>`).
fn tag(xml: &str, name: &str) -> Option<String> {
    let pat = format!("<w:{name}");
    let mut from = 0;
    while let Some(k) = xml[from..].find(&pat) {
        let a = from + k;
        let next = xml[a + pat.len()..].chars().next();
        if matches!(next, Some(' ' | '/' | '>')) {
            let b = xml[a..].find('>')? + a;
            return Some(xml[a..=b].to_string());
        }
        from = a + pat.len();
    }
    None
}

fn attr(t: &str, k: &str) -> Option<String> {
    let pat = format!("{k}=\"");
    let a = t.find(&pat)? + pat.len();
    let b = t[a..].find('"')? + a;
    Some(t[a..b].to_string())
}

fn twips(wpu: i32) -> i32 {
    wpu * 1440 / 1200
}

fn wpu(tw: i32) -> i32 {
    tw * 1200 / 1440
}

/// The paper of the document's first page: a `[Paper Sz/Typ]` before any
/// text (or in the initial codes), else the document page size.
pub fn first_paper(doc: &Document) -> Option<Paper> {
    let mut p = None;
    for e in doc.initial_codes.iter() {
        if let El::Code(Code::Fmt(Fmt::Paper(x))) = e {
            p = Some(x.clone());
        }
    }
    for e in &doc.els {
        match e {
            El::Ch(_) => break,
            El::Code(Code::Fmt(Fmt::Paper(x))) => p = Some(x.clone()),
            _ => {}
        }
    }
    p.filter(|p| p.labels.is_none())
}

/// Page width/height for `w:pgSz` and the orientation attribute.
pub fn page_size(doc: &Document) -> (i32, i32, &'static str) {
    let (w, h) = match first_paper(doc) {
        Some(p) => (p.w, p.h),
        None => (doc.init.page_width, doc.init.page_height),
    };
    (twips(w), twips(h), if w > h { " w:orient=\"landscape\"" } else { "" })
}

fn first_ln_num(doc: &Document) -> Option<LineNum> {
    doc.initial_codes.iter().chain(doc.els.iter()).find_map(|e| match e {
        El::Code(Code::Fmt(Fmt::LnNum(Some(n)))) => Some(n.clone()),
        _ => None,
    })
}

/// `<w:lnNumType …/>` for the section (after pgMar).
pub fn ln_num_xml(doc: &Document) -> String {
    match first_ln_num(doc) {
        Some(n) => {
            // distance is measured from the text; WP measures from the paper edge
            let dist = (doc.init.left - n.pos).max(120);
            format!(
                "<w:lnNumType w:countBy=\"{}\" w:start=\"{}\" w:distance=\"{}\" w:restart=\"{}\"/>",
                n.every.max(1),
                n.start.saturating_sub(1),
                twips(dist),
                if n.restart { "newPage" } else { "continuous" }
            )
        }
        None => String::new(),
    }
}

/// settings.xml: automatic hyphenation when the document uses it.
pub fn settings_xml(doc: &Document) -> String {
    let on = doc.initial_codes.iter().chain(doc.els.iter()).any(|e| matches!(e, El::Code(Code::Hyphenation(true))));
    if on { "<w:autoHyphenation/><w:hyphenationZone w:val=\"360\"/>".into() } else { String::new() }
}

pub fn bcp47(lang: &str) -> &'static str {
    match lang {
        "US" => "en-US",
        "UK" | "OZ" => "en-GB",
        "FR" | "CF" => "fr-FR",
        "IT" => "it-IT",
        "ES" => "es-ES",
        "RU" => "ru-RU",
        "NL" => "nl-NL",
        "DK" => "da-DK",
        "SD" => "de-CH",
        _ => "de-DE",
    }
}

fn wp_lang(bcp: &str) -> Option<&'static str> {
    let l = bcp.to_ascii_lowercase();
    Some(match l.get(..2)? {
        "de" => "DE",
        "en" if l.contains("gb") => "UK",
        "en" => "US",
        "fr" => "FR",
        "it" => "IT",
        "es" => "ES",
        "ru" => "RU",
        _ => return None,
    })
}

/// The document language for styles.xml.
pub fn doc_lang(doc: &Document) -> &'static str {
    let l = doc.initial_codes.iter().chain(doc.els.iter()).find_map(|e| match e {
        El::Code(Code::Fmt(Fmt::Lang(l))) => Some(l.clone()),
        _ => None,
    });
    bcp47(l.as_deref().unwrap_or("DE"))
}

/// Paragraph properties driven by codes, tracked across paragraphs.
#[derive(Default)]
pub struct ParaState {
    pub wo: bool,
    pub hyph: bool,
    pub block: bool,
    /// the block was open at some point in this paragraph
    block_here: bool,
    cndl: bool,
}

impl ParaState {
    pub fn new(doc: &Document) -> ParaState {
        let mut s = ParaState::default();
        for e in &doc.initial_codes {
            if let El::Code(c) = e {
                s.apply(c);
            }
        }
        s.block_here = s.block;
        s
    }
    pub fn apply(&mut self, c: &Code) {
        match c {
            Code::WidowOrphan(w) => self.wo = *w,
            Code::Hyphenation(h) => self.hyph = *h,
            Code::Fmt(Fmt::BlockPro(on)) => {
                self.block = *on;
                if *on {
                    self.block_here = true;
                }
            }
            Code::Fmt(Fmt::CndlEop(_)) => self.cndl = true,
            _ => {}
        }
    }
    /// Scan a paragraph's elements (before writing its pPr).
    pub fn scan(&mut self, els: &[El]) {
        for e in els {
            if let El::Code(c) = e {
                self.apply(c);
            }
        }
    }
    /// (properties before `w:tabs`, properties before `w:spacing`) — then
    /// reset per-paragraph flags.
    pub fn ppr(&mut self, any_hyph: bool) -> (String, String) {
        let mut pre = String::new();
        if self.cndl || self.block_here && self.block {
            pre.push_str("<w:keepNext/>");
        }
        if self.block_here || self.cndl {
            pre.push_str("<w:keepLines/>");
        }
        if self.wo {
            pre.push_str("<w:widowControl/>");
        } else {
            pre.push_str("<w:widowControl w:val=\"0\"/>");
        }
        let mid = if any_hyph && !self.hyph { "<w:suppressAutoHyphens/>".to_string() } else { String::new() };
        self.block_here = self.block;
        self.cndl = false;
        (pre, mid)
    }
}

pub fn any_hyph(doc: &Document) -> bool {
    doc.initial_codes.iter().chain(doc.els.iter()).any(|e| matches!(e, El::Code(Code::Hyphenation(true))))
}

/// Insert codes for what Word stored in the section, settings and styles
/// (reading a .docx that has no private stream).
pub fn after_read(d: &mut Document, docxml: &str, settings: Option<&str>, styles: Option<&str>) {
    let mut pre: Vec<El> = vec![];
    if let Some(t) = tag(docxml, "lnNumType") {
        let a = |k: &str| attr(&t, k).and_then(|v| v.parse::<i32>().ok());
        let dist = a("w:distance").map(wpu).unwrap_or(360);
        pre.push(El::Code(Code::Fmt(Fmt::LnNum(Some(LineNum {
            count_blank: true,
            every: a("w:countBy").unwrap_or(1).max(1) as u32,
            pos: (d.init.left - dist).max(0),
            start: a("w:start").unwrap_or(0).max(0) as u32 + 1,
            restart: attr(&t, "w:restart").as_deref() != Some("continuous"),
        })))));
    }
    if settings.is_some_and(|s| s.contains("autoHyphenation") && !s.contains("autoHyphenation w:val=\"0\"") && !s.contains("autoHyphenation w:val=\"false\"")) {
        pre.push(El::Code(Code::Hyphenation(true)));
    }
    if let Some(st) = styles {
        let default_wo = st.find("<w:pPrDefault").and_then(|a| st[a..].find("</w:pPrDefault>").map(|b| &st[a..a + b])).is_some_and(|x| x.contains("<w:widowControl/>"));
        if default_wo || docxml.contains("<w:widowControl/>") {
            pre.push(El::Code(Code::WidowOrphan(true)));
        }
        if let Some(l) = st.find("<w:rPrDefault").and_then(|a| tag(&st[a..], "lang")).and_then(|t| attr(&t, "w:val")) {
            if let Some(w) = wp_lang(&l) {
                if w != "DE" {
                    pre.push(El::Code(Code::Fmt(Fmt::Lang(w.into()))));
                }
            }
        }
    }
    if !pre.is_empty() {
        let tail = std::mem::take(&mut d.els);
        d.els = pre;
        d.els.extend(tail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_roundtrip_via_xml() {
        let mut d = Document::from_text("Hallo Welt\nzweiter Absatz\n");
        d.els.insert(0, El::Code(Code::Fmt(Fmt::Paper(Paper { w: 13200, h: 10200, ty: "Standard".into(), labels: None }))));
        d.els.insert(0, El::Code(Code::Hyphenation(true)));
        d.els.insert(0, El::Code(Code::WidowOrphan(true)));
        d.els.insert(0, El::Code(Code::Fmt(Fmt::LnNum(Some(LineNum { every: 5, ..Default::default() })))));
        d.els.insert(0, El::Code(Code::Fmt(Fmt::Lang("FR".into()))));
        let k = d.els.iter().position(|e| *e == El::Ch('z')).unwrap();
        d.els.insert(k, El::Code(Code::Fmt(Fmt::BlockPro(true))));
        d.els.push(El::Code(Code::Fmt(Fmt::BlockPro(false))));
        let bytes = crate::docx::write_docx(&d).unwrap();
        // inspect the XML Word sees
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut part = |n: &str| {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut z.by_name(n).unwrap(), &mut s).unwrap();
            s
        };
        let x = part("word/document.xml");
        assert!(x.contains("w:orient=\"landscape\""), "{x}");
        assert!(x.contains("<w:lnNumType w:countBy=\"5\""), "{x}");
        assert!(x.contains("<w:keepLines/>"));
        assert!(x.contains("<w:widowControl/>"));
        assert!(part("word/settings.xml").contains("<w:autoHyphenation/>"));
        assert!(part("word/styles.xml").contains("fr-FR"));
        // read back without the private stream
        let st = part("word/settings.xml");
        let sy = part("word/styles.xml");
        let mut r = crate::docx::parse_document_xml(&x, Some(&sy));
        after_read(&mut r, &x, Some(&st), Some(&sy));
        assert!(r.init.page_width > r.init.page_height);
        assert!(r.els.iter().any(|e| matches!(e, El::Code(Code::Fmt(Fmt::LnNum(Some(n)))) if n.every == 5)));
        assert!(r.els.contains(&El::Code(Code::Hyphenation(true))));
        assert!(r.els.contains(&El::Code(Code::WidowOrphan(true))));
        assert!(r.els.contains(&El::Code(Code::Fmt(Fmt::BlockPro(true)))));
        assert!(r.els.contains(&El::Code(Code::Fmt(Fmt::Lang("FR".into())))));
    }
}
