//! Styles and paragraph numbering in .docx: our styles ↔ `word/styles.xml`
//! (pStyle / rStyle), `[Par Num]` / `[Par Num Def]` ↔ `word/numbering.xml`
//! (abstractNum / num, numPr ilvl / numId).

use crate::doc::*;
use crate::docx::{get_attr, local, toggle_on, xml_esc};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;

/// Word style id for one of our style names ("Heading 1" → "Heading1").
pub fn style_id(name: &str) -> String {
    let id: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
    if id.is_empty() { "BSWStyle".into() } else { id }
}

/// Built-in Word names are lower case ("heading 1"); keep Word's own name
/// for headings so Word treats them as real headings.
fn word_name(name: &str) -> String {
    if heading_level(name).is_some() { name.to_lowercase() } else { name.to_string() }
}

fn heading_level(name: &str) -> Option<u8> {
    let l = name.to_lowercase();
    let n = l.strip_prefix("heading ").or_else(|| l.strip_prefix("überschrift "))?;
    n.trim().parse::<u8>().ok().filter(|n| (1..=9).contains(n))
}

/// Our name for a Word style name ("heading 1" → "Heading 1").
fn our_name(word: &str) -> String {
    let mut c = word.chars();
    match c.next() {
        Some(f) if f.is_ascii_lowercase() => f.to_ascii_uppercase().to_string() + c.as_str(),
        _ => word.to_string(),
    }
}

fn props_xml(s: &StyleDef, base: &FontSpec) -> (String, String) {
    let mut a = AttrSet::default();
    let mut font: Option<FontSpec> = None;
    let mut jc: Option<Justify> = None;
    for e in &s.on {
        if let El::Code(c) = e {
            match c {
                Code::AttrOn(x) => a.set(*x, true),
                Code::AttrOff(x) => a.set(*x, false),
                Code::Font(f) => font = Some(f.clone()),
                Code::Just(j) => jc = Some(*j),
                _ => {}
            }
        }
    }
    let mut r = String::new();
    if let Some(f) = &font {
        if f.name != base.name {
            let n = xml_esc(&f.name);
            r.push_str(&format!("<w:rFonts w:ascii=\"{n}\" w:hAnsi=\"{n}\" w:cs=\"{n}\"/>"));
        }
    }
    for (at, x) in [(Attr::Bold, "<w:b/>"), (Attr::Italic, "<w:i/>"), (Attr::SmallCaps, "<w:smallCaps/>"), (Attr::Strikeout, "<w:strike/>"), (Attr::Outline, "<w:outline/>"), (Attr::Shadow, "<w:shadow/>")] {
        if a.has(at) {
            r.push_str(x);
        }
    }
    let size = font.as_ref().map(|f| f.size_cpt).unwrap_or(base.size_cpt);
    let pct = if a.has(Attr::ExtraLarge) {
        200
    } else if a.has(Attr::VeryLarge) {
        150
    } else if a.has(Attr::Large) {
        120
    } else if a.has(Attr::Small) {
        80
    } else if a.has(Attr::Fine) {
        60
    } else {
        100
    };
    let hp = size * pct / 100 / 50;
    if hp != base.size_cpt / 50 {
        r.push_str(&format!("<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/>"));
    }
    if a.has(Attr::DblUnderline) {
        r.push_str("<w:u w:val=\"double\"/>");
    } else if a.has(Attr::Underline) {
        r.push_str("<w:u w:val=\"single\"/>");
    }
    if a.has(Attr::Superscript) {
        r.push_str("<w:vertAlign w:val=\"superscript\"/>");
    } else if a.has(Attr::Subscript) {
        r.push_str("<w:vertAlign w:val=\"subscript\"/>");
    }
    let mut p = String::new();
    if let Some(j) = jc {
        let v = match j {
            Justify::Left => "left",
            Justify::Center => "center",
            Justify::Right => "right",
            Justify::Full => "both",
        };
        p.push_str(&format!("<w:jc w:val=\"{v}\"/>"));
    }
    (p, r)
}

/// `<w:style>` elements for the document's styles: a paragraph style and
/// a character style ("…Char") for every paired style.
pub fn styles_xml(doc: &Document) -> String {
    let base = &doc.init.font;
    let mut x = String::new();
    for s in &doc.styles {
        if s.kind == StyleKind::Open {
            continue;
        }
        let id = style_id(&s.name);
        let name = xml_esc(&word_name(&s.name));
        let (ppr, rpr) = props_xml(s, base);
        let outline = heading_level(&s.name).map(|l| format!("<w:outlineLvl w:val=\"{}\"/>", l - 1)).unwrap_or_default();
        let keep = if heading_level(&s.name).is_some() { "<w:keepNext/>" } else { "" };
        let ppr = format!("{keep}{ppr}{outline}");
        let ppr = if ppr.is_empty() { String::new() } else { format!("<w:pPr>{ppr}</w:pPr>") };
        let rpr = if rpr.is_empty() { String::new() } else { format!("<w:rPr>{rpr}</w:rPr>") };
        x.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:customStyle=\"{}\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:link w:val=\"{id}Char\"/><w:qFormat/>{ppr}{rpr}</w:style>",
            if heading_level(&s.name).is_some() { 0 } else { 1 }
        ));
        x.push_str(&format!(
            "<w:style w:type=\"character\" w:customStyle=\"1\" w:styleId=\"{id}Char\"><w:name w:val=\"{} Char\"/><w:link w:val=\"{id}\"/>{rpr}</w:style>",
            xml_esc(&s.name)
        ));
    }
    x
}

fn fmt_val(f: NumFmt) -> &'static str {
    match f {
        NumFmt::Decimal => "decimal",
        NumFmt::LowerLetter => "lowerLetter",
        NumFmt::UpperLetter => "upperLetter",
        NumFmt::LowerRoman => "lowerRoman",
        NumFmt::UpperRoman => "upperRoman",
        NumFmt::Bullet => "bullet",
    }
}

/// word/numbering.xml: one abstractNum + num per `[Par Num Def]` used
/// (numId = index + 1).
pub fn numbering_xml(defs: &[ParNumDef]) -> String {
    let w_ns = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"{w_ns}\">");
    for (k, d) in defs.iter().enumerate() {
        x.push_str(&format!("<w:abstractNum w:abstractNumId=\"{k}\"><w:multiLevelType w:val=\"multilevel\"/>"));
        for (l, lv) in d.levels.iter().enumerate() {
            x.push_str(&format!(
                "<w:lvl w:ilvl=\"{l}\"><w:start w:val=\"{}\"/><w:numFmt w:val=\"{}\"/><w:lvlText w:val=\"{}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"720\"/></w:pPr></w:lvl>",
                lv.start,
                fmt_val(lv.fmt),
                xml_esc(&lv.text),
                (l + 1) * 720
            ));
        }
        x.push_str("</w:abstractNum>");
    }
    for k in 0..defs.len() {
        x.push_str(&format!("<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"{k}\"/></w:num>", k + 1));
    }
    x.push_str("</w:numbering>");
    x
}

// ----------------------------------------------------------------- read

#[derive(Clone, Debug, Default)]
pub struct WStyle {
    pub name: String,
    pub character: bool,
    pub default: bool,
    attrs: AttrSet,
    font: Option<String>,
    size: Option<u32>,
    pub jc: Option<Justify>,
    based_on: Option<String>,
}

/// All styles defined in word/styles.xml, by style id.
pub fn read_word_styles(xml: &str) -> HashMap<String, WStyle> {
    let mut out: HashMap<String, WStyle> = HashMap::new();
    let mut r = Reader::from_str(xml);
    let mut cur: Option<(String, WStyle)> = None;
    let mut in_rpr = false;
    let mut in_ppr = false;
    loop {
        let ev = r.read_event();
        match ev {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let empty = matches!(ev, Ok(Event::Empty(_)));
                match local(e.name().as_ref()) {
                    b"style" => {
                        let ty = get_attr(e, b"type").unwrap_or_default();
                        let id = get_attr(e, b"styleId").unwrap_or_default();
                        let ws = WStyle { character: ty == "character", default: get_attr(e, b"default").is_some_and(|v| v == "1" || v == "true"), ..Default::default() };
                        if ty == "paragraph" || ty == "character" {
                            cur = Some((id, ws));
                        }
                        if empty {
                            cur = None;
                        }
                    }
                    b"name" => {
                        if let Some((_, s)) = cur.as_mut() {
                            s.name = get_attr(e, b"val").unwrap_or_default();
                        }
                    }
                    b"basedOn" => {
                        if let Some((_, s)) = cur.as_mut() {
                            s.based_on = get_attr(e, b"val");
                        }
                    }
                    b"rPr" if !empty => in_rpr = true,
                    b"pPr" if !empty => in_ppr = true,
                    n => {
                        if let Some((_, s)) = cur.as_mut() {
                            if in_rpr {
                                match n {
                                    b"b" => s.attrs.set(Attr::Bold, toggle_on(e)),
                                    b"i" => s.attrs.set(Attr::Italic, toggle_on(e)),
                                    b"smallCaps" => s.attrs.set(Attr::SmallCaps, toggle_on(e)),
                                    b"strike" => s.attrs.set(Attr::Strikeout, toggle_on(e)),
                                    b"u" => match get_attr(e, b"val").as_deref() {
                                        Some("none") | Some("0") => {}
                                        Some("double") => s.attrs.set(Attr::DblUnderline, true),
                                        _ => s.attrs.set(Attr::Underline, true),
                                    },
                                    b"vertAlign" => match get_attr(e, b"val").as_deref() {
                                        Some("superscript") => s.attrs.set(Attr::Superscript, true),
                                        Some("subscript") => s.attrs.set(Attr::Subscript, true),
                                        _ => {}
                                    },
                                    b"rFonts" => s.font = get_attr(e, b"ascii").or_else(|| get_attr(e, b"hAnsi")),
                                    b"sz" => s.size = get_attr(e, b"val").and_then(|v| v.parse().ok()),
                                    _ => {}
                                }
                            } else if in_ppr && n == b"jc" {
                                s.jc = match get_attr(e, b"val").as_deref() {
                                    Some("center") => Some(Justify::Center),
                                    Some("right") | Some("end") => Some(Justify::Right),
                                    Some("both") | Some("distribute") => Some(Justify::Full),
                                    _ => Some(Justify::Left),
                                };
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => match local(e.name().as_ref()) {
                b"style" => {
                    if let Some((id, s)) = cur.take() {
                        out.insert(id, s);
                    }
                }
                b"rPr" => in_rpr = false,
                b"pPr" => in_ppr = false,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    // one level of inheritance (Heading 2 based on Heading 1 …), not Normal
    let snapshot = out.clone();
    for s in out.values_mut() {
        if let Some(b) = s.based_on.as_ref().and_then(|b| snapshot.get(b)) {
            if b.default || b.name.eq_ignore_ascii_case("normal") {
                continue;
            }
            s.attrs.0 |= b.attrs.0;
            s.font = s.font.clone().or(b.font.clone());
            s.size = s.size.or(b.size);
            s.jc = s.jc.or(b.jc);
        }
    }
    out
}

/// Should a Word style become one of our styles?
pub fn importable(id: &str, s: &WStyle) -> bool {
    !(s.default
        || s.name.is_empty()
        || matches!(id, "Normal" | "ListParagraph" | "DefaultParagraphFont" | "Hyperlink" | "FootnoteReference" | "EndnoteReference" | "FootnoteText" | "EndnoteText" | "Header" | "Footer"))
}

/// Our style definition for a Word style.
pub fn style_def(s: &WStyle, base: &FontSpec) -> StyleDef {
    let mut on = vec![];
    let mut off = vec![];
    if s.font.is_some() || s.size.is_some() {
        let f = FontSpec { name: s.font.clone().unwrap_or_else(|| base.name.clone()), size_cpt: s.size.map(|h| h * 50).unwrap_or(base.size_cpt) };
        if f != *base {
            on.push(El::Code(Code::Font(f)));
            off.push(El::Code(Code::Font(base.clone())));
        }
    }
    if let Some(j) = s.jc {
        on.push(El::Code(Code::Just(j)));
        off.push(El::Code(Code::Just(Justify::Left)));
    }
    for a in s.attrs.iter() {
        on.push(El::Code(Code::AttrOn(a)));
    }
    for a in Attr::ALL.iter().rev().filter(|a| s.attrs.has(**a)) {
        off.insert(0, El::Code(Code::AttrOff(*a)));
    }
    StyleDef { name: our_name(&s.name), kind: StyleKind::Paired, description: String::new(), on, off, enter: EnterAction::HRt }
}

/// word/numbering.xml: numId → its 8 levels.
#[derive(Default, Debug)]
pub struct WNumbering {
    abs: HashMap<String, Vec<NumLevel>>,
    num: HashMap<String, String>,
}

fn bullet_char(text: &str) -> String {
    let t: String = text
        .chars()
        .map(|c| match c as u32 {
            0xF0B7 | 0xF06C => '\u{2022}',
            0xF0A7 | 0xF06E => '\u{25a0}',
            0xF0D8 => '\u{27a2}',
            0xF0FC => '\u{2713}',
            0xF076 => '\u{2756}',
            0xF000..=0xF0FF => '\u{2022}',
            _ => c,
        })
        .collect();
    if t.trim().is_empty() { "\u{2022}".into() } else { t }
}

pub fn read_numbering(xml: &str) -> WNumbering {
    let mut n = WNumbering::default();
    let mut r = Reader::from_str(xml);
    let mut abs: Option<(String, Vec<NumLevel>)> = None;
    let mut lvl: Option<NumLevel> = None;
    let mut num: Option<String> = None;
    loop {
        let ev = r.read_event();
        match ev {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => match local(e.name().as_ref()) {
                b"abstractNum" => abs = Some((get_attr(e, b"abstractNumId").unwrap_or_default(), vec![])),
                b"lvl" => lvl = Some(NumLevel { fmt: NumFmt::Decimal, text: String::new(), start: 1 }),
                b"start" => {
                    if let Some(l) = lvl.as_mut() {
                        l.start = get_attr(e, b"val").and_then(|v| v.parse().ok()).unwrap_or(1);
                    }
                }
                b"numFmt" => {
                    if let Some(l) = lvl.as_mut() {
                        l.fmt = match get_attr(e, b"val").as_deref() {
                            Some("lowerLetter") => NumFmt::LowerLetter,
                            Some("upperLetter") => NumFmt::UpperLetter,
                            Some("lowerRoman") => NumFmt::LowerRoman,
                            Some("upperRoman") => NumFmt::UpperRoman,
                            Some("bullet") => NumFmt::Bullet,
                            _ => NumFmt::Decimal,
                        };
                    }
                }
                b"lvlText" => {
                    if let Some(l) = lvl.as_mut() {
                        l.text = get_attr(e, b"val").unwrap_or_default();
                    }
                }
                b"num" => num = get_attr(e, b"numId"),
                b"abstractNumId" => {
                    if let (Some(id), Some(a)) = (num.as_ref(), get_attr(e, b"val")) {
                        n.num.insert(id.clone(), a);
                    }
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => match local(e.name().as_ref()) {
                b"lvl" => {
                    if let (Some(mut l), Some((_, v))) = (lvl.take(), abs.as_mut()) {
                        if l.fmt == NumFmt::Bullet {
                            l.text = bullet_char(&l.text);
                        }
                        v.push(l);
                    }
                }
                b"abstractNum" => {
                    if let Some((id, v)) = abs.take() {
                        n.abs.insert(id, v);
                    }
                }
                b"num" => num = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    n
}

impl WNumbering {
    /// The definition of a list (numId), or None for "no list" (numId 0).
    pub fn def(&self, num_id: &str) -> Option<ParNumDef> {
        if num_id == "0" {
            return None;
        }
        let mut d = crate::styles::preset(0);
        if let Some(levels) = self.num.get(num_id).and_then(|a| self.abs.get(a)) {
            for (k, l) in levels.iter().enumerate().take(8) {
                d.levels[k] = l.clone();
            }
        }
        Some(d)
    }
}

/// Import state for lists: continue numbering of a list after another
/// list interrupted it (WP restarts at every [Par Num Def]).
#[derive(Default)]
pub struct ListState {
    current: Option<String>,
    counts: HashMap<String, [u32; 8]>,
}

impl ListState {
    /// Codes to put at the start of a list paragraph.
    pub fn para(&mut self, nb: &WNumbering, num_id: &str, ilvl: u8) -> Vec<El> {
        let Some(mut def) = nb.def(num_id) else { return vec![] };
        let mut out = vec![];
        let cnt = self.counts.entry(num_id.to_string()).or_insert([0; 8]);
        if self.current.as_deref() != Some(num_id) {
            for k in 0..8 {
                def.levels[k].start += cnt[k];
            }
            out.push(El::Code(Code::ParNumDef(def)));
            self.current = Some(num_id.to_string());
        }
        let l = (ilvl as usize).min(7);
        cnt[l] += 1;
        for c in cnt.iter_mut().skip(l + 1) {
            *c = 0;
        }
        for _ in 0..l {
            out.push(El::Code(Code::Tab));
        }
        out.push(El::Code(Code::ParNum(None)));
        out.push(El::Code(Code::Indent));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::{document_xml, parse_document_xml_full, Parts};

    fn list_doc() -> Document {
        let mut d = Document::new();
        d.styles.push(StyleDef {
            name: "Heading 1".into(),
            kind: StyleKind::Paired,
            description: String::new(),
            on: vec![El::Code(Code::Font(FontSpec { name: "Courier New".into(), size_cpt: 1600 })), El::Code(Code::AttrOn(Attr::Bold))],
            off: vec![El::Code(Code::AttrOff(Attr::Bold)), El::Code(Code::Font(FontSpec::default_font()))],
            enter: EnterAction::HRt,
        });
        d.els.push(El::Code(Code::StyleOn("Heading 1".into())));
        d.els.extend("Titel".chars().map(El::Ch));
        d.els.push(El::Code(Code::StyleOff("Heading 1".into())));
        d.els.push(El::Code(Code::HRt));
        d.els.push(El::Code(Code::ParNumDef(crate::styles::preset(3))));
        for (tabs, t) in [(0, "eins"), (1, "zwei"), (0, "drei")] {
            for _ in 0..tabs {
                d.els.push(El::Code(Code::Tab));
            }
            d.els.push(El::Code(Code::ParNum(None)));
            d.els.push(El::Code(Code::Indent));
            d.els.extend(t.chars().map(El::Ch));
            d.els.push(El::Code(Code::HRt));
        }
        d.els.extend("Ende".chars().map(El::Ch));
        d
    }

    #[test]
    fn export_styles_and_lists() {
        let d = list_doc();
        let mut parts = Parts::default();
        let xml = document_xml(&d, &mut parts);
        assert!(xml.contains("<w:pStyle w:val=\"Heading1\"/>"), "{xml}");
        assert!(xml.contains("<w:numPr><w:ilvl w:val=\"1\"/><w:numId w:val=\"1\"/></w:numPr>"), "{xml}");
        assert!(!xml.contains("<w:tab/>"), "{xml}");
        let st = styles_xml(&d);
        assert!(st.contains("w:styleId=\"Heading1\"") && st.contains("heading 1") && st.contains("<w:b/>") && st.contains("w:val=\"32\""), "{st}");
        let nx = numbering_xml(&parts.nums);
        assert!(nx.contains("w:val=\"bullet\"") && nx.contains("\u{2022}"), "{nx}");

        // and back
        let styles = format!("<w:styles>{st}</w:styles>");
        let d2 = parse_document_xml_full(&xml, Some(&styles), Some(&nx), &Default::default(), &Default::default(), true);
        assert_eq!(d2.styles.len(), 1, "{:?}", d2.styles);
        assert_eq!(d2.styles[0].name, "Heading 1");
        assert!(d2.styles[0].on.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert_eq!(d2.els[0], El::Code(Code::StyleOn("Heading 1".into())));
        let pn = d2.els.iter().filter(|e| matches!(e, El::Code(Code::ParNum(None)))).count();
        assert_eq!(pn, 3);
        assert!(d2.els.iter().any(|e| matches!(e, El::Code(Code::ParNumDef(def)) if def.levels[0].fmt == NumFmt::Bullet)));
        let l = crate::layout::layout(&d2, &crate::layout::MonoMetrics);
        let lines: Vec<String> = l.lines.iter().map(|x| x.cells.iter().map(|c| c.ch).collect::<String>()).collect();
        assert_eq!(lines[0], "Titel");
        assert_eq!(lines[1], "\u{2022}eins");
        assert_eq!(lines[2], "ozwei");
    }

    #[test]
    fn import_word_numbering() {
        let numbering = r#"<w:numbering><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2)"/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val=""/></w:lvl></w:abstractNum><w:num w:numId="5"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="6"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;
        let p = |id: &str, l: u8, t: &str| format!("<w:p><w:pPr><w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"{l}\"/><w:numId w:val=\"{id}\"/></w:numPr></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>");
        let body = format!("<w:document><w:body>{}{}{}{}{}</w:body></w:document>", p("5", 0, "A"), p("5", 1, "B"), p("6", 0, "X"), p("5", 1, "C"), p("5", 0, "D"));
        let d = parse_document_xml_full(&body, None, Some(numbering), &Default::default(), &Default::default(), true);
        let l = crate::layout::layout(&d, &crate::layout::MonoMetrics);
        let lines: Vec<String> = l.lines.iter().map(|x| x.cells.iter().map(|c| c.ch).collect::<String>()).collect();
        assert_eq!(lines, vec!["1.A", "a)B", "\u{2022}X", "b)C", "2.D"]);
    }
}
