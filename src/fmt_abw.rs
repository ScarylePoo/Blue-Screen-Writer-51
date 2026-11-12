//! AbiWord (.abw; .zabw is gunzipped by `fileio` first) reader.
//!
//! AbiWord's XML: `<section>` → `<p style=… props=…>` → `<c props=…>`
//! spans (may nest), `<br/>` line break, `<pbr/>` page break, `<cbr/>`
//! column break, `<table>`/`<cell props="left-attach:…;top-attach:…">`,
//! `<foot>` / `<endnote>` notes (their paragraphs inline), header/footer
//! sections (`<section type="header" id=…>`), `<styles><s …/>` style
//! definitions, `<pagesize>` and `<metadata><m key=…>`.

use crate::doc::*;
use crate::fmt_build::Builder;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
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

type Props = Vec<(String, String)>;

fn parse_props(s: &str) -> Props {
    s.split(';')
        .filter_map(|kv| {
            let (k, v) = kv.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect()
}

fn get<'a>(p: &'a Props, k: &str) -> Option<&'a str> {
    p.iter().rev().find(|(kk, _)| kk == k).map(|(_, v)| v.as_str())
}

/// A length like "1.25in", "2cm", "12pt", "30mm" → WPU.
fn length(v: &str) -> Option<i32> {
    let v = v.trim();
    let num_end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(v.len());
    let n: f64 = v[..num_end].parse().ok()?;
    let inches = match v[num_end..].trim() {
        "in" | "" => n,
        "cm" => n / 2.54,
        "mm" => n / 25.4,
        "pt" => n / 72.0,
        "pi" => n / 6.0,
        "px" => n / 96.0,
        _ => return None,
    };
    Some((inches * 1200.0).round() as i32)
}

#[derive(Clone, Default)]
struct StyleDef {
    based_on: Option<String>,
    props: Props,
}

struct Reader2 {
    styles: HashMap<String, StyleDef>,
    base_font: FontSpec,
}

impl Reader2 {
    /// Properties of a style including the ones it is based on.
    fn style_props(&self, name: &str) -> Props {
        let mut chain = vec![];
        let mut cur = Some(name.to_string());
        while let Some(c) = cur {
            if chain.len() > 12 || chain.contains(&c) {
                break;
            }
            let Some(s) = self.styles.get(&c) else { break };
            cur = s.based_on.clone();
            chain.push(c);
        }
        let mut out = Props::new();
        for c in chain.iter().rev() {
            out.extend(self.styles[c].props.iter().cloned());
        }
        if chain.is_empty() && name.to_ascii_lowercase().starts_with("heading") {
            out.push(("font-weight".into(), "bold".into()));
        }
        out
    }

    /// Apply character properties to the builder's wanted state.
    fn apply_char(&self, b: &mut Builder, props: &Props) {
        let mut attrs = AttrSet::default();
        let mut font = self.base_font.clone();
        for (k, v) in props {
            let v = v.to_ascii_lowercase();
            match k.as_str() {
                "font-weight" => attrs.set(Attr::Bold, v == "bold" || v.parse::<u32>().is_ok_and(|n| n >= 600)),
                "font-style" => attrs.set(Attr::Italic, v == "italic" || v == "oblique"),
                "text-decoration" => {
                    attrs.set(Attr::Underline, v.contains("underline"));
                    attrs.set(Attr::Strikeout, v.contains("line-through"));
                }
                "text-position" => {
                    attrs.set(Attr::Superscript, v == "superscript");
                    attrs.set(Attr::Subscript, v == "subscript");
                }
                "font-variant" => attrs.set(Attr::SmallCaps, v == "small-caps"),
                "font-family" => {
                    let orig = props.iter().rev().find(|(kk, _)| kk == "font-family").map(|(_, v)| v.clone()).unwrap_or_default();
                    if !orig.is_empty() {
                        font.name = orig;
                    }
                }
                "font-size" => {
                    if let Some(w) = length(&v) {
                        font.size_cpt = (w as u32 * 72 * 100 / 1200).clamp(100, 20000);
                    }
                }
                _ => {}
            }
        }
        b.attrs = attrs;
        b.font = font;
    }
}

/// Builder stack entry.
struct Target {
    b: Builder,
    kind: TKind,
}

enum TKind {
    Main,
    Note(NoteKind),
    HeadFoot(String, String), // (section type, id)
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let text = crate::fmt_text::decode_auto(bytes);
    if !text.contains("<abiword") {
        return Err("not an AbiWord document".into());
    }
    let base_font = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    let mut rd = Reader2 { styles: HashMap::new(), base_font: base_font.clone() };
    let mut d = Document::new();

    // pass 1: styles, page size, metadata
    {
        let mut r = Reader::from_str(&text);
        let mut meta_key: Option<String> = None;
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match local(e.name().as_ref()) {
                    b"s" => {
                        if let Some(name) = attr(&e, b"name") {
                            let props = parse_props(&attr(&e, b"props").unwrap_or_default());
                            rd.styles.insert(name, StyleDef { based_on: attr(&e, b"basedon"), props });
                        }
                    }
                    b"pagesize" => {
                        let unit = attr(&e, b"units").unwrap_or_else(|| "in".into());
                        let w = attr(&e, b"width").and_then(|v| length(&format!("{v}{unit}")));
                        let h = attr(&e, b"height").and_then(|v| length(&format!("{v}{unit}")));
                        if let (Some(w), Some(h)) = (w, h) {
                            if (1200..60000).contains(&w) && (1200..60000).contains(&h) {
                                d.init.page_width = w;
                                d.init.page_height = h;
                            }
                        }
                    }
                    b"m" => meta_key = attr(&e, b"key"),
                    _ => {}
                },
                Ok(Event::Text(t)) => {
                    if let Some(k) = meta_key.take() {
                        let v = t.unescape().map(|s| s.trim().to_string()).unwrap_or_default();
                        match k.as_str() {
                            "dc.title" => d.summary.descriptive_name = v,
                            "dc.subject" => d.summary.subject = v,
                            "dc.creator" => d.summary.author = v,
                            "dc.description" => d.summary.comments = v,
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(_)) => meta_key = None,
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    let normal = rd.style_props("Normal");
    {
        let mut tmp = Builder::new(base_font.clone());
        rd.apply_char(&mut tmp, &normal);
        rd.base_font = tmp.font.clone();
    }
    let base_font = rd.base_font.clone();

    // pass 2: content
    let mut stack: Vec<Target> = vec![Target { b: Builder::new(base_font.clone()), kind: TKind::Main }];
    stack[0].b.text_width = d.init.page_width - d.init.left - d.init.right;
    // character property layers: [paragraph (style+props), c, c, …]
    let mut layers: Vec<Props> = vec![];
    let mut in_p = false;
    let mut skip_depth = 0usize;
    let mut first_section = true;
    let mut hf: Vec<(String, String, Vec<El>)> = vec![];
    let mut main_refs: HashMap<String, String> = HashMap::new(); // id → type
    // tables: per open table the cells seen in the current row
    let mut tables: Vec<Option<i32>> = vec![];
    let mut saved: Vec<(Vec<Props>, bool)> = vec![];
    let mut after_pbr = false;
    let mut r = Reader::from_str(&text);
    loop {
        let ev = match r.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(ev) => ev,
        };
        if skip_depth > 0 {
            match ev {
                Event::Start(_) => skip_depth += 1,
                Event::End(_) => skip_depth -= 1,
                _ => {}
            }
            continue;
        }
        let empty = matches!(ev, Event::Empty(_));
        match &ev {
            Event::Start(e) | Event::Empty(e) => {
                let name = local(e.name().as_ref()).to_vec();
                let cur = stack.last_mut().map(|t| &mut t.b);
                let Some(b) = cur else { break };
                match name.as_slice() {
                    b"metadata" | b"styles" | b"history" | b"rdf" | b"data" | b"revisions" | b"lists" | b"pagesize" | b"ignoredwords" | b"image" | b"frame" | b"math" | b"embed" | b"textbox" if !empty => {
                        skip_depth = 1;
                    }
                    b"section" => {
                        let typ = attr(e, b"type").unwrap_or_default();
                        if typ.starts_with("header") || typ.starts_with("footer") {
                            let id = attr(e, b"id").unwrap_or_default();
                            stack.push(Target { b: Builder::new(base_font.clone()), kind: TKind::HeadFoot(typ, id) });
                        } else {
                            for k in ["header", "header-even", "footer", "footer-even", "header-first", "footer-first"] {
                                if let Some(id) = attr(e, k.as_bytes()) {
                                    main_refs.entry(id).or_insert_with(|| k.to_string());
                                }
                            }
                            let props = parse_props(&attr(e, b"props").unwrap_or_default());
                            if first_section {
                                first_section = false;
                                for (k, slot) in [("page-margin-left", &mut d.init.left), ("page-margin-right", &mut d.init.right), ("page-margin-top", &mut d.init.top), ("page-margin-bottom", &mut d.init.bottom)] {
                                    if let Some(w) = get(&props, k).and_then(length) {
                                        if (0..12000).contains(&w) {
                                            *slot = w;
                                        }
                                    }
                                }
                                b.text_width = d.init.page_width - d.init.left - d.init.right;
                            } else {
                                b.para();
                            }
                        }
                    }
                    b"p" => {
                        let mut props = rd.style_props(&attr(e, b"style").unwrap_or_else(|| "Normal".into()));
                        props.extend(parse_props(&attr(e, b"props").unwrap_or_default()));
                        b.just = match get(&props, "text-align") {
                            Some("center") => Justify::Center,
                            Some("right") => Justify::Right,
                            Some("justify") => Justify::Full,
                            _ => Justify::Left,
                        };
                        layers = vec![props];
                        rd.apply_char(b, &layers.concat());
                        in_p = !empty;
                        if empty {
                            b.para();
                        }
                    }
                    b"c" | b"a" | b"span" => {
                        if !empty {
                            let mut props = match attr(e, b"style") {
                                Some(s) => rd.style_props(&s),
                                None => Props::new(),
                            };
                            props.extend(parse_props(&attr(e, b"props").unwrap_or_default()));
                            layers.push(props);
                            rd.apply_char(b, &layers.concat());
                        }
                    }
                    b"br" => b.para(),
                    b"cbr" => b.para(),
                    b"pbr" => {
                        b.para();
                        b.page();
                        after_pbr = true;
                    }
                    b"tab" => b.ch('\t'),
                    b"field" => {
                        if !empty {
                            skip_depth = 1;
                        }
                    }
                    b"foot" | b"endnote" if !empty => {
                        let kind = if name.as_slice() == b"foot" { NoteKind::Foot } else { NoteKind::End };
                        stack.push(Target { b: Builder::new(base_font.clone()), kind: TKind::Note(kind) });
                        saved.push((std::mem::take(&mut layers), in_p));
                        in_p = false;
                    }
                    b"table" if !empty => {
                        let props = parse_props(&attr(e, b"props").unwrap_or_default());
                        let cols: Vec<i32> = get(&props, "table-column-props").map(|v| v.split('/').filter_map(length).collect()).unwrap_or_default();
                        b.table_start(cols);
                        tables.push(None);
                    }
                    b"cell" if !empty => {
                        let props = parse_props(&attr(e, b"props").unwrap_or_default());
                        let n = |k: &str| get(&props, k).and_then(|v| v.parse::<i32>().ok());
                        let top = n("top-attach").unwrap_or(-1);
                        if let Some(row) = tables.last_mut() {
                            let new_row = match *row {
                                None => true,
                                Some(t) => top >= 0 && top != t,
                            };
                            if new_row {
                                *row = Some(top);
                                b.row();
                            }
                        }
                        b.cell();
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let name = local(e.name().as_ref()).to_vec();
                match name.as_slice() {
                    b"p" => {
                        in_p = false;
                        if let Some(t) = stack.last_mut() {
                            if !std::mem::take(&mut after_pbr) {
                                t.b.para();
                            }
                        }
                        layers.clear();
                    }
                    b"c" | b"a" | b"span" => {
                        layers.pop();
                        if let Some(t) = stack.last_mut() {
                            rd.apply_char(&mut t.b, &layers.concat());
                        }
                    }
                    b"foot" | b"endnote" => {
                        if stack.len() > 1 && matches!(stack.last().map(|t| &t.kind), Some(TKind::Note(_))) {
                            let t = stack.pop().unwrap_or_else(|| unreachable!());
                            let TKind::Note(kind) = t.kind else { continue };
                            let els = t.b.finish();
                            if let Some(p) = stack.last_mut() {
                                // restore the paragraph's formatting around the reference
                                if let Some((l, ip)) = saved.pop() {
                                    layers = l;
                                    in_p = ip;
                                }
                                rd.apply_char(&mut p.b, &layers.concat());
                                p.b.note(kind, els);
                            }
                        }
                    }
                    b"section" => {
                        if stack.len() > 1 && matches!(stack.last().map(|t| &t.kind), Some(TKind::HeadFoot(..))) {
                            let t = stack.pop().unwrap_or_else(|| unreachable!());
                            if let TKind::HeadFoot(typ, id) = t.kind {
                                hf.push((typ, id, t.b.finish()));
                            }
                        }
                    }
                    b"table" => {
                        tables.pop();
                        if let Some(t) = stack.last_mut() {
                            t.b.table_end();
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_p => {
                if let (Ok(s), Some(tg)) = (t.unescape(), stack.last_mut()) {
                    if !s.trim().is_empty() {
                        after_pbr = false;
                    }
                    for c in s.chars() {
                        match c {
                            '\n' | '\r' => {}
                            c => tg.b.ch(c),
                        }
                    }
                }
            }
            Event::CData(t) if in_p => {
                if let Some(tg) = stack.last_mut() {
                    tg.b.text(&String::from_utf8_lossy(t));
                }
            }
            _ => {}
        }
    }
    // unwind unclosed note/header builders (truncated file)
    while stack.len() > 1 {
        let t = stack.pop().unwrap_or_else(|| unreachable!());
        let els = t.b.finish();
        if let (TKind::Note(k), Some(p)) = (t.kind, stack.last_mut()) {
            p.b.note(k, els);
        }
    }
    let main = stack.pop().ok_or("damaged AbiWord document")?;
    let body = main.b.into_document(base_font.clone());

    // headers/footers
    let has_even = hf.iter().any(|(t, _, _)| t.ends_with("-even"));
    let mut hfe: Vec<El> = vec![];
    for (typ, id, els) in hf {
        let typ = main_refs.get(&id).cloned().unwrap_or(typ);
        if els.is_empty() || typ.ends_with("-first") || typ.ends_with("-last") {
            continue;
        }
        let even = typ.ends_with("-even");
        let occ = if even {
            Occurrence::Even
        } else if has_even {
            Occurrence::Odd
        } else {
            Occurrence::Every
        };
        let which = if typ.starts_with("footer") { 2 } else { 0 } + u8::from(even);
        if !hfe.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) {
            hfe.push(El::Code(Code::HeadFoot { which, occ, els }));
        }
    }
    d.init.font = body.init.font;
    d.init.justify = body.init.justify;
    d.els = hfe;
    d.els.extend(body.els);
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    fn data(n: &str) -> Vec<u8> {
        std::fs::read(format!("{}/tests/data/formats/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn hello_abw_and_zabw() {
        let d = read(&data("hello.abw")).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "hello world");
        assert_eq!(d.summary.author, "David Tardon");
        assert_eq!(d.init.page_width, 8 * 1200 + 600);
        let p = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/hello.zabw"));
        let d = crate::fileio::load(p).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "hello world");
    }

    #[test]
    fn sample_abw() {
        let b = data("sample.abw");
        assert_eq!(crate::fileio::sniff(&b), crate::fileio::Kind::Abiword);
        let d = read(&b).unwrap();
        let t = debug_text(&d.els);
        assert_eq!(
            t,
            "[Header:Kopfzeile]Überschrift\nGrüße aus Köln, fett und kursiv.\nПривет мир[Footnote:Fußnote hier.]\n[Tbl]\n[Row]|A1|B1\n[Row]|A2|Ä2[TblOff]Ende.\n[HPg]\nSeite 2"
        );
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Center))) || d.init.justify == Justify::Center);
        assert_eq!(d.summary.descriptive_name, "Testbrief");
        assert_eq!(d.init.left, 1500);
    }

    #[test]
    fn damaged_input_never_panics() {
        let s = data("sample.abw");
        for n in (0..s.len()).step_by(37) {
            let _ = read(&s[..n]);
        }
        let mut g = s.clone();
        for i in (0..g.len()).step_by(11) {
            g[i] = g[i].wrapping_add(7);
        }
        let _ = read(&g);
        let _ = read(b"<abiword><section><p><foot><p>x");
    }
}
