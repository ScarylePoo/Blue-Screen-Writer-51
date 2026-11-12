//! .docx (Office Open XML) writer and reader.
//!
//! Writing: the WP code stream is converted to paragraphs and runs that
//! Word understands.  A private part `wp51/stream.txt` keeps the exact
//! codes; it is only trusted on reading if document.xml is unchanged
//! (hash check), i.e. nobody edited the file in another program since.

use crate::doc::*;
use crate::stream;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::io::{Read, Write};

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STREAM_PART: &str = "wp51/stream.txt";
const STREAM_REL: &str = "http://wp51-native.local/relationships/codestream";

fn fnv(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub(crate) fn xml_esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

fn twips(wpu: i32) -> i32 {
    // 1 twip = 1/1440 inch; WPU = 1/1200 inch
    (wpu as i64 * 1440 / 1200) as i32
}

fn wpu_from_twips(t: i32) -> i32 {
    (t as i64 * 1200 / 1440) as i32
}

#[derive(Clone, PartialEq)]
struct RunProps {
    attrs: AttrSet,
    font: FontSpec,
    /// tracked change: (deleted?, author, date)
    rev: Option<(bool, String, String)>,
    /// inside a hyperlink (gets Word's "Hyperlink" character style)
    link: bool,
    /// character style (a paired style used inside a paragraph)
    rstyle: Option<String>,
}

static REV_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn rpr_xml(rp: &RunProps, base: &FontSpec) -> String {
    let a = rp.attrs;
    let mut s = String::new();
    if rp.link {
        s.push_str("<w:rStyle w:val=\"Hyperlink\"/>");
    }
    if let Some(st) = &rp.rstyle {
        s.push_str(&format!("<w:rStyle w:val=\"{}Char\"/>", crate::docx_styles::style_id(st)));
    }
    if rp.font.name != base.name {
        let n = xml_esc(&rp.font.name);
        s.push_str(&format!("<w:rFonts w:ascii=\"{n}\" w:hAnsi=\"{n}\" w:cs=\"{n}\" w:eastAsia=\"{n}\"/>"));
    }
    if a.has(Attr::Bold) {
        s.push_str("<w:b/><w:bCs/>");
    }
    if a.has(Attr::Italic) {
        s.push_str("<w:i/><w:iCs/>");
    }
    if a.has(Attr::SmallCaps) {
        s.push_str("<w:smallCaps/>");
    }
    if a.has(Attr::Strikeout) {
        s.push_str("<w:strike/>");
    }
    if a.has(Attr::Outline) {
        s.push_str("<w:outline/>");
    }
    if a.has(Attr::Shadow) {
        s.push_str("<w:shadow/>");
    }
    if a.has(Attr::Redline) {
        s.push_str("<w:color w:val=\"C00000\"/>");
    }
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
    let half_pts = (rp.font.size_cpt as i64 * pct / 100 / 50) as i64;
    if rp.font.size_cpt != base.size_cpt || pct != 100 {
        s.push_str(&format!("<w:sz w:val=\"{half_pts}\"/><w:szCs w:val=\"{half_pts}\"/>"));
    }
    if a.has(Attr::DblUnderline) {
        s.push_str("<w:u w:val=\"double\"/>");
    } else if a.has(Attr::Underline) {
        s.push_str("<w:u w:val=\"single\"/>");
    }
    if a.has(Attr::Superscript) {
        s.push_str("<w:vertAlign w:val=\"superscript\"/>");
    } else if a.has(Attr::Subscript) {
        s.push_str("<w:vertAlign w:val=\"subscript\"/>");
    }
    if s.is_empty() { s } else { format!("<w:rPr>{s}</w:rPr>") }
}

struct ParaFmt {
    just: Justify,
    left: i32,
    right: i32,
    spacing: u32,
    tabs: TabSet,
}

/// Build word/document.xml from the code stream.
/// Footnotes, endnotes, headers and footers collected while writing.
#[derive(Default)]
pub struct Parts {
    pub foot: Vec<String>,
    pub end: Vec<String>,
    /// (which 0..3, occurrence, paragraphs xml)
    pub hf: Vec<(u8, Occurrence, String)>,
    /// Comment texts (word/comments.xml), id = index.
    pub comments: Vec<String>,
    /// Bookmarks written so far (ids for cross-reference targets).
    pub bookmarks: u32,
    /// Section breaks written for text columns so far.
    pub sections: usize,
    /// Newspaper columns still on at the end of the document.
    pub final_cols: Option<Vec<(i32, i32)>>,
    /// External hyperlink targets (relationship rIdL{index}).
    pub links: Vec<String>,
    /// > 0 while writing notes/headers (parts without their own hyperlink
    /// relationships: links there are kept as plain text).
    pub nested: u32,
    /// Paragraph number definitions used (numbering.xml; numId = index + 1).
    pub nums: Vec<ParNumDef>,
    /// Pictures for word/media (gfxdocx.rs).
    pub media: crate::gfxdocx::Media,
}

/// A Word bookmark name for a WP target name: letters, digits and `_`,
/// starting with a letter, at most 40 characters.
fn bookmark_name(name: &str) -> String {
    let mut s: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    if !s.chars().next().is_some_and(|c| c.is_alphabetic()) {
        s.insert_str(0, "T_");
    }
    format!("WPT_{}", s.chars().take(36).collect::<String>())
}

/// Placeholder for the header/footer references in section breaks.
const SECT_REFS: &str = "\u{1}SECTREFS\u{1}";

/// `<w:sectPr>` with page size and margins (and newspaper columns).
fn sect_pr(doc: &Document, refs: &str, continuous: bool, cols: Option<&[(i32, i32)]>) -> String {
    let init = &doc.init;
    let (pw, ph, orient) = crate::fmtdocx::page_size(doc);
    let lnnum = crate::fmtdocx::ln_num_xml(doc);
    let ty = if continuous { "<w:type w:val=\"continuous\"/>" } else { "" };
    let mut c = String::new();
    if let Some(cols) = cols.filter(|c| c.len() > 1) {
        let w: Vec<i32> = cols.iter().map(|(a, b)| b - a).collect();
        let gaps: Vec<i32> = cols.windows(2).map(|p| p[1].0 - p[0].1).collect();
        if w.iter().all(|x| *x == w[0]) && gaps.iter().all(|g| *g == gaps[0]) {
            c = format!("<w:cols w:num=\"{}\" w:space=\"{}\"/>", cols.len(), twips(gaps[0]));
        } else {
            c = format!("<w:cols w:num=\"{}\" w:space=\"{}\" w:equalWidth=\"0\">", cols.len(), twips(gaps[0]));
            for (k, wd) in w.iter().enumerate() {
                c.push_str(&format!("<w:col w:w=\"{}\" w:space=\"{}\"/>", twips(*wd), twips(gaps.get(k).copied().unwrap_or(0))));
            }
            c.push_str("</w:cols>");
        }
    }
    format!(
        "<w:sectPr>{refs}{ty}<w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>{c}{lnnum}</w:sectPr>",
        pw,
        ph,
        twips(init.top),
        twips(init.right),
        twips(init.bottom),
        twips(init.left),
        twips(init.top),
        twips(init.bottom)
    )
}

/// A continuous section break ending the section before it (text columns
/// on/off).  Word keeps it in the last paragraph of the section.
fn section_break(body: &mut String, doc: &Document, parts: &mut Parts, cols: Option<&[(i32, i32)]>) {
    let sect = sect_pr(doc, SECT_REFS, parts.sections > 0, cols);
    parts.sections += 1;
    if body.ends_with("</w:p>") {
        if let Some(p) = body.rfind("<w:p><w:pPr>") {
            if !body[p..].contains("<w:tbl") && !body[p..].contains("<w:sectPr>") {
                if let Some(q) = body[p..].find("</w:pPr>") {
                    body.insert_str(p + q, &sect);
                    return;
                }
            }
        }
    }
    body.push_str(&format!("<w:p><w:pPr>{sect}</w:pPr></w:p>"));
}

/// Parallel text columns ([Col On] … [Col Off], columns split by [HPg]):
/// one borderless table row, columns at their positions.
fn parallel_xml(doc: &Document, els: &[El], cols: &[(i32, i32)], protect: bool, parts: &mut Parts) -> String {
    let n = cols.len();
    let mut pieces: Vec<&[El]> = els.split(|e| *e == El::Code(Code::HPg)).collect();
    pieces.resize(n.max(pieces.len()), &[]);
    let ind = cols[0].0 - doc.init.left;
    let mut x = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/><w:tblBorders><w:top w:val=\"none\" w:sz=\"0\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"none\" w:sz=\"0\" w:space=\"0\" w:color=\"auto\"/><w:bottom w:val=\"none\" w:sz=\"0\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"none\" w:sz=\"0\" w:space=\"0\" w:color=\"auto\"/><w:insideH w:val=\"none\" w:sz=\"0\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"none\" w:sz=\"0\" w:space=\"0\" w:color=\"auto\"/></w:tblBorders><w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:left w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>",
        twips(ind)
    );
    let width = |k: usize| if k + 1 < n { cols[k + 1].0 - cols[k].0 } else { cols[k].1 - cols[k].0 };
    for k in 0..n {
        x.push_str(&format!("<w:gridCol w:w=\"{}\"/>", twips(width(k))));
    }
    x.push_str("</w:tblGrid><w:tr>");
    if protect {
        x.push_str("<w:trPr><w:cantSplit/></w:trPr>");
    }
    for k in 0..n {
        let gap = if k + 1 < n { cols[k + 1].0 - cols[k].1 } else { 0 };
        x.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/><w:tcMar><w:right w:w=\"{}\" w:type=\"dxa\"/></w:tcMar></w:tcPr>", twips(width(k)), twips(gap)));
        // extra [HPg] pieces stay in the last column
        let content: Vec<El> = if k + 1 == n {
            pieces[k..].join(&El::Code(Code::HRt))
        } else {
            pieces[k].to_vec()
        };
        let inner = body_xml(doc, &content, parts);
        x.push_str(if inner.is_empty() { "<w:p/>" } else { &inner });
        x.push_str("</w:tc>");
    }
    x.push_str("</w:tr></w:tbl>");
    x
}

static BM_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// `<w:hyperlink …>` start tag for a link target (external URL or `#bookmark`).
fn hyperlink_open(url: &str, parts: &mut Parts) -> String {
    if let Some(anchor) = url.strip_prefix('#') {
        format!("<w:hyperlink w:anchor=\"{}\" w:history=\"1\">", xml_esc(anchor))
    } else {
        let k = match parts.links.iter().position(|u| u == url) {
            Some(k) => k,
            None => {
                parts.links.push(url.to_string());
                parts.links.len() - 1
            }
        };
        format!("<w:hyperlink r:id=\"rIdL{k}\" w:history=\"1\">")
    }
}

const NOTE_REF_RPR: &str = "<w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr>";

pub fn document_xml(doc: &Document, parts: &mut Parts) -> String {
    let els = crate::styles::docx_stream(doc);
    let body = body_xml(doc, &els, parts);
    let mut refs = String::new();
    for (k, (which, occ, _)) in parts.hf.iter().enumerate() {
        let tag = if *which < 2 { "headerReference" } else { "footerReference" };
        let ty = if *occ == Occurrence::Even { "even" } else { "default" };
        refs.push_str(&format!("<w:{tag} w:type=\"{ty}\" r:id=\"rIdHF{k}\"/>"));
    }
    let sect = sect_pr(doc, &refs, parts.sections > 0, parts.final_cols.as_deref());
    let body = body.replace(SECT_REFS, &refs);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W_NS}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" {}><w:body>{body}{sect}</w:body></w:document>",
        crate::gfxdocx::NS
    )
}

/// Paragraphs for a list of elements (the main text or nested text).
fn body_xml(doc: &Document, els_in: &[El], parts: &mut Parts) -> String {
    let init = &doc.init;
    let mut body = String::new();
    let mut st_font = init.font.clone();
    let mut rev: Option<(bool, String, String)> = None;
    let mut link: Option<String> = None;
    let mut attrs = AttrSet::default();
    let mut pf = ParaFmt { just: init.justify, left: init.left, right: init.right, spacing: init.line_spacing, tabs: init.tabs.clone() };
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            apply_para(&mut pf, &mut st_font, c);
        }
    }
    let base_font = st_font.clone();
    let els = els_in;
    // styles / paragraph numbering state
    let mut para_style: Option<String> = None;
    let mut cstyle: Option<String> = None;
    let mut cur_num: Option<usize> = None;
    let style_kind = |n: &str| crate::styles::find_style(doc, n).map(|s| s.kind);
    let mut fps = crate::fmtdocx::ParaState::new(doc);
    let any_hyph = crate::fmtdocx::any_hyph(doc);
    let mut i = 0;
    let n = els.len();
    // text columns (only in the main text)
    let top = parts.nested == 0;
    let mut coldef: Option<(ColKind, Vec<(i32, i32)>)> = None;
    let mut news_cols: Option<Vec<(i32, i32)>> = None;
    // Paragraph-at-a-time
    loop {
        // [Col Def] / [Col On] / [Col Off]: newspaper columns become a
        // section with <w:cols>, parallel columns a borderless table
        if i < n {
            let mut handled = true;
            match &els[i] {
                El::Code(Code::ColDef { kind, cols }) => coldef = Some((*kind, cols.clone())),
                El::Code(Code::ColOn) if top => match coldef.clone() {
                    Some((ColKind::Newspaper, cols)) if cols.len() > 1 => {
                        if !body.is_empty() {
                            section_break(&mut body, doc, parts, news_cols.as_deref());
                        }
                        news_cols = Some(cols);
                    }
                    Some((kind, cols)) if kind != ColKind::Newspaper && !cols.is_empty() => {
                        if let Some(c) = news_cols.take() {
                            section_break(&mut body, doc, parts, Some(&c));
                        }
                        let end = els[i + 1..].iter().position(|e| matches!(e, El::Code(Code::ColOff))).map(|k| i + 1 + k).unwrap_or(n);
                        body.push_str(&parallel_xml(doc, &els[i + 1..end], &cols, kind == ColKind::ParallelProtect, parts));
                        i = end;
                    }
                    _ => {}
                },
                El::Code(Code::ColOff) if top => {
                    if let Some(c) = news_cols.take() {
                        section_break(&mut body, doc, parts, Some(&c));
                    }
                }
                El::Code(Code::ColOn | Code::ColOff) => {}
                _ => handled = false,
            }
            if handled {
                i += 1;
                if i >= n {
                    // Word wants a paragraph after a table / section break
                    if top {
                        body.push_str("<w:p/>");
                    }
                    break;
                }
                continue;
            }
        }
        // a table: [Tbl Def] … [Tbl Off] becomes a <w:tbl>
        if i < n {
            if let El::Code(Code::TblDef { .. }) = els[i] {
                if let Some(t) = crate::tables::parse(els, i) {
                    body.push_str(&table_xml(doc, els, &t, parts));
                    i = t.off + 1;
                    if i < n && matches!(els[i], El::Code(Code::HRt)) {
                        i += 1;
                    }
                    if i >= n {
                        break;
                    }
                    continue;
                }
            }
        }
        // collect paragraph elements [i, j) ending at HRt/HPg (exclusive)
        let mut j = i;
        while j < n && !matches!(els[j], El::Code(Code::HRt | Code::HPg | Code::TblDef { .. } | Code::ColDef { .. } | Code::ColOn | Code::ColOff)) {
            j += 1;
        }
        let ends_at_table = j < n && matches!(els[j], El::Code(Code::TblDef { .. } | Code::ColDef { .. } | Code::ColOn | Code::ColOff));
        // leading codes: paragraph style, numbering
        let mut lead = i;
        let mut lead_tabs = 0u8;
        let mut num_at: Option<u8> = None;
        while lead < j {
            let El::Code(c) = &els[lead] else { break };
            match c {
                Code::StyleOn(s) if style_kind(s).is_some_and(|k| k != StyleKind::Open) => para_style = Some(s.clone()),
                Code::StyleOff(s) if para_style.as_ref() == Some(s) => para_style = None,
                Code::ParNumDef(d) => {
                    parts.nums.push(d.clone());
                    cur_num = Some(parts.nums.len() - 1);
                }
                Code::Tab | Code::Indent | Code::DblIndent if num_at.is_none() => lead_tabs += 1,
                Code::ParNum(l) if num_at.is_none() => num_at = Some(l.unwrap_or(lead_tabs + 1).clamp(1, 8)),
                Code::HardSpace | Code::HardHyphen => break,
                _ => {}
            }
            lead += 1;
        }
        let numbered = num_at.is_some();
        // first pass: format codes inside the paragraph that affect it
        let mut para_just: Option<Justify> = None;
        let mut align_at: Option<usize> = None;
        {
            // Center/FlushRight at paragraph start → alignment
            let mut k = i;
            while k < j && els[k].code().is_some() && !matches!(els[k], El::Code(Code::Center | Code::FlushRight)) {
                if let El::Code(c) = &els[k] {
                    apply_para(&mut pf, &mut st_font, c);
                    if let Some((a, on)) = c.attr() {
                        attrs.set(a, on);
                    }
                }
                k += 1;
            }
            if k < j {
                match els[k] {
                    El::Code(Code::Center) => para_just = Some(Justify::Center),
                    El::Code(Code::FlushRight) => para_just = Some(Justify::Right),
                    _ => {}
                }
                align_at = Some(k);
            }
        }
        let jc = match para_just.unwrap_or(pf.just) {
            Justify::Left => "left",
            Justify::Center => "center",
            Justify::Right => "right",
            Justify::Full => "both",
        };
        let mut ppr = String::new();
        if let Some(s) = &para_style {
            ppr.push_str(&format!("<w:pStyle w:val=\"{}\"/>", crate::docx_styles::style_id(s)));
        }
        if let Some(l) = num_at {
            let id = match cur_num {
                Some(k) => k,
                None => {
                    parts.nums.push(crate::styles::default_def());
                    parts.nums.len() - 1
                }
            };
            cur_num = Some(id);
            ppr.push_str(&format!("<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>", l - 1, id + 1));
        }
        fps.scan(&els[i..j]);
        let (ppr_pre, ppr_mid) = fps.ppr(any_hyph);
        ppr.push_str(&ppr_pre);
        if !pf.tabs.stops.is_empty() {
            let base = if pf.tabs.relative { 0 } else { -pf.left };
            let mut tabs = String::new();
            let mut plain_half_inch = true;
            let mut n = 0;
            for t in &pf.tabs.stops {
                let pos = t.pos + base;
                if pos <= 0 {
                    continue;
                }
                n += 1;
                if t.kind != TabKind::Left || t.dot_leader || twips(pos) != 720 * n {
                    plain_half_inch = false;
                }
                let val = match t.kind {
                    TabKind::Left => "left",
                    TabKind::Center => "center",
                    TabKind::Right => "right",
                    TabKind::Decimal => "decimal",
                };
                let leader = if t.dot_leader { " w:leader=\"dot\"" } else { "" };
                tabs.push_str(&format!("<w:tab w:val=\"{val}\" w:pos=\"{}\"{leader}/>", twips(pos)));
            }
            // tabs every 0.5" are Word's default (settings: defaultTabStop
            // 720), so they need not be repeated in every paragraph
            if !plain_half_inch {
                ppr.push_str("<w:tabs>");
                ppr.push_str(&tabs);
                ppr.push_str("</w:tabs>");
            }
        }
        let line = 240 * pf.spacing / 100;
        ppr.push_str(&ppr_mid);
        ppr.push_str(&format!("<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{line}\" w:lineRule=\"auto\"/>"));
        // Indent: margins differing from the section margins, plus a
        // leading [->Indent] / [->Indent<-]
        let mut ind_left = pf.left - init.left;
        let mut ind_right = pf.right - init.right;
        let mut hanging = 0;
        {
            let mut k = i;
            while k < j && is_formatting(&els[k]) {
                k += 1;
            }
            let mut indents = 0;
            let mut dbl = false;
            while k < j {
                match els[k] {
                    El::Code(Code::Indent) => indents += 1,
                    El::Code(Code::DblIndent) => {
                        indents += 1;
                        dbl = true
                    }
                    El::Code(Code::MarRel) if indents > 0 => {
                        hanging += 600;
                    }
                    _ => break,
                }
                k += 1;
            }
            if indents > 0 {
                ind_left += indents * 600;
                if dbl {
                    ind_right += indents * 600;
                }
            } else if k < j {
                // "[Indent]…" after margin release handled above; a
                // paragraph starting with [<-Mar Rel] then text: hanging
            }
            if k > i && matches!(els[k.saturating_sub(1)], El::Code(Code::MarRel)) && indents == 0 {
                hanging = 600;
            }
        }
        if numbered {
            // the list level supplies the indent
            ind_left = pf.left - init.left;
            ind_right = pf.right - init.right;
            hanging = 0;
        }
        if ind_left != 0 || ind_right != 0 || hanging != 0 {
            let h = if hanging > 0 { format!(" w:hanging=\"{}\"", twips(hanging)) } else { String::new() };
            ppr.push_str(&format!("<w:ind w:left=\"{}\" w:right=\"{}\"{h}/>", twips(ind_left), twips(ind_right)));
        }
        ppr.push_str(&format!("<w:jc w:val=\"{jc}\"/>"));
        body.push_str("<w:p><w:pPr>");
        body.push_str(&ppr);
        body.push_str("</w:pPr>");
        let mut link_open = false;
        if let Some(u) = &link {
            if parts.nested == 0 {
                body.push_str(&hyperlink_open(u, parts));
                link_open = true;
            }
        }

        // runs
        let mut text = String::new();
        let mut cur = RunProps { attrs, font: st_font.clone(), rev: rev.clone(), link: link.is_some(), rstyle: cstyle.clone() };
        let mut indent_skipping = true;
        let flush = |body: &mut String, text: &mut String, cur: &RunProps| {
            if !text.is_empty() {
                let run = format!("<w:r>{}{}</w:r>", rpr_xml(cur, &base_font), text);
                match &cur.rev {
                    Some((del, author, date)) => {
                        let id = REV_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let tag = if *del { "del" } else { "ins" };
                        let run = if *del { run.replace("<w:t ", "<w:delText ").replace("</w:t>", "</w:delText>") } else { run };
                        body.push_str(&format!("<w:{tag} w:id=\"{id}\" w:author=\"{}\" w:date=\"{}\">{run}</w:{tag}>", xml_esc(author), xml_esc(date)));
                    }
                    None => body.push_str(&run),
                }
                text.clear();
            }
        };
        let mut k = i;
        while k < j {
            match &els[k] {
                El::Ch(c) => {
                    indent_skipping = false;
                    let rp = RunProps { attrs, font: st_font.clone(), rev: rev.clone(), link: link.is_some(), rstyle: cstyle.clone() };
                    if rp != cur {
                        flush(&mut body, &mut text, &cur);
                        cur = rp;
                    }
                    let s = xml_esc(&c.to_string());
                    // consecutive letters share one <w:t> element
                    if text.ends_with("</w:t>") {
                        text.truncate(text.len() - "</w:t>".len());
                        text.push_str(&s);
                        text.push_str("</w:t>");
                    } else {
                        text.push_str(&format!("<w:t xml:space=\"preserve\">{s}</w:t>"));
                    }
                }
                El::Code(c) => {
                    let rp_now = RunProps { attrs, font: st_font.clone(), rev: rev.clone(), link: link.is_some(), rstyle: cstyle.clone() };
                    match c {
                        Code::Tab | Code::Indent | Code::DblIndent | Code::MarRel if numbered && k < lead => {}
                        Code::StyleOn(_) | Code::StyleOff(_) | Code::ParNumDef(_) | Code::ParNum(_) if k < lead => {}
                        Code::StyleOn(s) => {
                            if style_kind(s).is_some_and(|k| k != StyleKind::Open) {
                                cstyle = Some(s.clone());
                            }
                        }
                        Code::StyleOff(s) => {
                            if cstyle.as_ref() == Some(s) {
                                cstyle = None;
                            } else if para_style.as_ref() == Some(s) {
                                para_style = None;
                            }
                        }
                        Code::ParNumDef(d) => {
                            parts.nums.push(d.clone());
                            cur_num = Some(parts.nums.len() - 1);
                        }
                        Code::Tab => {
                            if rp_now != cur {
                                flush(&mut body, &mut text, &cur);
                                cur = rp_now;
                            }
                            text.push_str("<w:tab/>");
                            indent_skipping = false;
                        }
                        Code::Indent | Code::DblIndent | Code::MarRel if indent_skipping => {}
                        Code::Indent | Code::DblIndent => text.push_str("<w:tab/>"),
                        Code::HardSpace => text.push_str("<w:t xml:space=\"preserve\">\u{a0}</w:t>"),
                        Code::HardHyphen => text.push_str("<w:noBreakHyphen/>"),
                        Code::SoftHyphen => text.push_str("<w:softHyphen/>"),
                        Code::FlushRight | Code::FlushRightDot if align_at != Some(k) => {
                            // flush right inside a line (e.g. generated ToC page numbers)
                            if rp_now != cur {
                                flush(&mut body, &mut text, &cur);
                                cur = rp_now;
                            }
                            let leader = if matches!(c, Code::FlushRightDot) { "dot" } else { "none" };
                            text.push_str(&format!("<w:ptab w:relativeTo=\"margin\" w:alignment=\"right\" w:leader=\"{leader}\"/>"));
                            indent_skipping = false;
                        }
                        Code::Center | Code::FlushRight | Code::FlushRightDot => {}
                        Code::Comment(t) => {
                            flush(&mut body, &mut text, &cur);
                            let id = parts.comments.len();
                            let paras: String = t
                                .split('\n')
                                .map(|l| format!("<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml_esc(l)))
                                .collect();
                            parts.comments.push(paras);
                            body.push_str(&format!(
                                "<w:commentRangeStart w:id=\"{id}\"/><w:commentRangeEnd w:id=\"{id}\"/><w:r><w:commentReference w:id=\"{id}\"/></w:r>"
                            ));
                        }
                        Code::Mark(Mark::Target { name }) => {
                            flush(&mut body, &mut text, &cur);
                            let id = parts.bookmarks;
                            parts.bookmarks += 1;
                            body.push_str(&format!("<w:bookmarkStart w:id=\"{id}\" w:name=\"{}\"/><w:bookmarkEnd w:id=\"{id}\"/>", xml_esc(&bookmark_name(name))));
                        }
                        Code::Mark(Mark::Ref { text: v, .. }) => {
                            // cross-references go out as plain text (the generated value)
                            indent_skipping = false;
                            if rp_now != cur {
                                flush(&mut body, &mut text, &cur);
                                cur = rp_now;
                            }
                            text.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t>", xml_esc(v)));
                        }
                        // Merge codes have no Word equivalent: they are written as
                        // plain-text placeholders "{FIELD}name~" (their parameters
                        // are ordinary text already).  Ctrl-F9, 3 turns such text
                        // back into codes.  Math operators / formulas / link codes
                        // are not printed; computed values are ordinary text.
                        Code::Merge(name) => {
                            if rp_now != cur {
                                flush(&mut body, &mut text, &cur);
                                cur = rp_now;
                            }
                            indent_skipping = false;
                            text.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t>", xml_esc(&format!("{{{name}}}"))));
                        }
                        Code::Link(u) => {
                            flush(&mut body, &mut text, &cur);
                            if link_open {
                                body.push_str("</w:hyperlink>");
                                link_open = false;
                            }
                            if parts.nested == 0 {
                                body.push_str(&hyperlink_open(u, parts));
                                link_open = true;
                            }
                            link = Some(u.clone());
                        }
                        Code::LinkEnd => {
                            flush(&mut body, &mut text, &cur);
                            if link_open {
                                body.push_str("</w:hyperlink>");
                                link_open = false;
                            }
                            link = None;
                        }
                        Code::Bookmark(name) => {
                            flush(&mut body, &mut text, &cur);
                            let id = BM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            body.push_str(&format!("<w:bookmarkStart w:id=\"{id}\" w:name=\"{}\"/><w:bookmarkEnd w:id=\"{id}\"/>", xml_esc(name)));
                        }
                        Code::Note { kind, els: nels } => {
                            flush(&mut body, &mut text, &cur);
                            parts.nested += 1;
                            let mut inner = body_xml(doc, nels, parts);
                            parts.nested -= 1;
                            if inner.is_empty() {
                                inner = "<w:p></w:p>".into();
                            }
                            let (tag, reft) = if *kind == NoteKind::Foot { ("footnoteReference", "footnoteRef") } else { ("endnoteReference", "endnoteRef") };
                            let marker = format!("<w:r>{NOTE_REF_RPR}<w:{reft}/></w:r><w:r><w:t xml:space=\"preserve\"> </w:t></w:r>");
                            let inner = match inner.find("</w:pPr>") {
                                Some(p) => format!("{}{marker}{}", &inner[..p + 8], &inner[p + 8..]),
                                None => inner,
                            };
                            let list = if *kind == NoteKind::Foot { &mut parts.foot } else { &mut parts.end };
                            list.push(inner);
                            let id = list.len();
                            body.push_str(&format!("<w:r>{NOTE_REF_RPR}<w:{tag} w:id=\"{id}\"/></w:r>"));
                        }
                        Code::HeadFoot { which, occ, els: hels } => {
                            if *occ != Occurrence::Discontinue && !parts.hf.iter().any(|(w, o, _)| *w == *which || (*w < 2) == (*which < 2) && *o == *occ) {
                                parts.nested += 1;
                                let x = body_xml(doc, hels, parts);
                                parts.nested -= 1;
                                parts.hf.push((*which, *occ, x));
                            }
                        }
                        Code::GBox(b) => {
                            flush(&mut body, &mut text, &cur);
                            let o = crate::gfx::opts_before(els, k, b.kind);
                            let inner = if b.contents == crate::gfx::Contents::Text { body_xml(doc, &b.text, parts) } else { String::new() };
                            let num = crate::gfx::number_before(els, k);
                            let cap = if b.caption.is_empty() { String::new() } else { body_xml(doc, &crate::gfx::expand_caption(&b.caption, &o, &num), parts) };
                            body.push_str(&crate::gfxdocx::box_xml(doc, b, &o, &inner, &cap, &mut parts.media));
                        }
                        Code::GLine(l) => {
                            flush(&mut body, &mut text, &cur);
                            body.push_str(&crate::gfxdocx::line_xml(doc, l));
                        }
                        Code::RevOn { del, author, date } => rev = Some((*del, author.clone(), date.clone())),
                        Code::RevOff { .. } => rev = None,
                        Code::AttrOn(a) => attrs.set(*a, true),
                        Code::AttrOff(a) => attrs.set(*a, false),
                        other => apply_para(&mut pf, &mut st_font, other),
                    }
                }
            }
            k += 1;
        }
        flush(&mut body, &mut text, &cur);
        if link_open {
            body.push_str("</w:hyperlink>");
        }
        // Hard page: page break at end of paragraph
        if j < n && matches!(els[j], El::Code(Code::HPg)) {
            // in newspaper columns [HPg] is a column break
            let ty = if news_cols.is_some() { "column" } else { "page" };
            body.push_str(&format!("<w:r><w:br w:type=\"{ty}\"/></w:r>"));
        }
        body.push_str("</w:p>");
        if j >= n {
            break;
        }
        i = if ends_at_table { j } else { j + 1 };
        if i >= n {
            // trailing HRt: Word shows the empty last paragraph
            body.push_str("<w:p/>");
            break;
        }
    }
    if top {
        parts.final_cols = news_cols;
    }
    body
}

fn table_xml(doc: &Document, els: &[El], t: &crate::tables::Table, parts: &mut Parts) -> String {
    let mut x = String::from(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"double\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/><w:left w:val=\"double\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/><w:bottom w:val=\"double\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/><w:right w:val=\"double\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/></w:tblBorders><w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:left w:w=\"115\" w:type=\"dxa\"/><w:right w:w=\"115\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>",
    );
    for w in &t.cols {
        x.push_str(&format!("<w:gridCol w:w=\"{}\"/>", twips(*w)));
    }
    x.push_str("</w:tblGrid>");
    for (r, row) in t.rows.iter().enumerate() {
        x.push_str("<w:tr>");
        for (c, _) in row.iter().enumerate() {
            let (a, b) = t.cell_range(els, r, c);
            let w = t.cols.get(c).copied().unwrap_or(1200);
            x.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/></w:tcPr>", twips(w)));
            let inner = body_xml(doc, &els[a..b], parts);
            x.push_str(if inner.is_empty() { "<w:p/>" } else { &inner });
            x.push_str("</w:tc>");
        }
        x.push_str("</w:tr>");
    }
    x.push_str("</w:tbl>");
    x
}

fn is_formatting(e: &El) -> bool {
    match e {
        El::Ch(_) => false,
        El::Code(c) => !matches!(
            c,
            Code::Tab | Code::Indent | Code::DblIndent | Code::MarRel | Code::HardSpace | Code::HardHyphen | Code::HRt | Code::HPg
        ),
    }
}

fn apply_para(pf: &mut ParaFmt, font: &mut FontSpec, c: &Code) {
    match c {
        Code::Just(j) => pf.just = *j,
        Code::LRMargin { left, right } => {
            pf.left = *left;
            pf.right = *right
        }
        Code::LineSpacing(s) => pf.spacing = *s,
        Code::TabSet(t) => pf.tabs = t.clone(),
        Code::Font(f) => *font = f.clone(),
        _ => {}
    }
}

fn styles_xml(doc: &Document) -> String {
    let f = xml_esc(&doc.init.font.name);
    let sz = doc.init.font.size_cpt / 50;
    let lang = crate::fmtdocx::doc_lang(doc);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W_NS}\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:cs=\"{f}\" w:eastAsia=\"{f}\"/><w:sz w:val=\"{sz}\"/><w:szCs w:val=\"{sz}\"/><w:lang w:val=\"{lang}\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style><w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:uiPriority w:val=\"99\"/><w:unhideWhenUsed/><w:rPr><w:color w:val=\"0563C1\" w:themeColor=\"hyperlink\"/><w:u w:val=\"single\"/></w:rPr></w:style>{}</w:styles>",
        crate::docx_styles::styles_xml(doc)
    )
}

fn core_xml(doc: &Document) -> String {
    let s = &doc.summary;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>{}</dc:title><dc:subject>{}</dc:subject><dc:creator>{}</dc:creator><dc:description>{}</dc:description></cp:coreProperties>",
        xml_esc(&s.descriptive_name),
        xml_esc(&s.subject),
        xml_esc(&s.author),
        xml_esc(&s.comments)
    )
}

fn custom_xml(doc: &Document) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><property fmtid=\"{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}\" pid=\"2\" name=\"WP51NativeId\"><vt:lpwstr>{}</vt:lpwstr></property></Properties>",
        xml_esc(&doc.id)
    )
}

pub fn write_docx(doc: &Document) -> std::io::Result<Vec<u8>> {
    let mut parts = Parts::default();
    let docxml = document_xml(doc, &mut parts);
    let hash = fnv(docxml.as_bytes());
    let stream_txt = format!("#HASH {hash:016x}\n{}", stream::write(doc));
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        const WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";
        const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let ns = format!("xmlns:w=\"{W_NS}\" xmlns:r=\"{REL}\" {}", crate::gfxdocx::NS);
        let mut overrides = format!(
            "<Override PartName=\"/word/document.xml\" ContentType=\"{WML}.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"{WML}.styles+xml\"/><Override PartName=\"/word/settings.xml\" ContentType=\"{WML}.settings+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/custom.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.custom-properties+xml\"/>"
        );
        let mut rels = format!(
            "<Relationship Id=\"rId1\" Type=\"{REL}/styles\" Target=\"styles.xml\"/><Relationship Id=\"rId2\" Type=\"{REL}/settings\" Target=\"settings.xml\"/>"
        );
        let mut files: Vec<(String, String)> = vec![];
        let sep = |kind: &str| {
            format!(
                "<w:{kind} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}><w:{kind} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}>"
            )
        };
        for (kind, list, root) in [("footnote", &parts.foot, "footnotes"), ("endnote", &parts.end, "endnotes")] {
            if list.is_empty() {
                continue;
            }
            let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{root} {ns}>{}", sep(kind));
            for (k, body) in list.iter().enumerate() {
                x.push_str(&format!("<w:{kind} w:id=\"{}\">{body}</w:{kind}>", k + 1));
            }
            x.push_str(&format!("</w:{root}>"));
            overrides.push_str(&format!("<Override PartName=\"/word/{root}.xml\" ContentType=\"{WML}.{root}+xml\"/>"));
            rels.push_str(&format!("<Relationship Id=\"rId{root}\" Type=\"{REL}/{root}\" Target=\"{root}.xml\"/>"));
            files.push((format!("word/{root}.xml"), x));
        }
        if !parts.comments.is_empty() {
            let author = xml_esc(&crate::track::author());
            let date = crate::track::now_iso();
            let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:comments {ns}>");
            for (k, body) in parts.comments.iter().enumerate() {
                x.push_str(&format!("<w:comment w:id=\"{k}\" w:author=\"{author}\" w:date=\"{date}\">{body}</w:comment>"));
            }
            x.push_str("</w:comments>");
            overrides.push_str(&format!("<Override PartName=\"/word/comments.xml\" ContentType=\"{WML}.comments+xml\"/>"));
            rels.push_str(&format!("<Relationship Id=\"rIdComments\" Type=\"{REL}/comments\" Target=\"comments.xml\"/>"));
            files.push(("word/comments.xml".into(), x));
        }
        if !parts.nums.is_empty() {
            overrides.push_str(&format!("<Override PartName=\"/word/numbering.xml\" ContentType=\"{WML}.numbering+xml\"/>"));
            rels.push_str(&format!("<Relationship Id=\"rIdNumbering\" Type=\"{REL}/numbering\" Target=\"numbering.xml\"/>"));
            files.push(("word/numbering.xml".into(), crate::docx_styles::numbering_xml(&parts.nums)));
        }
        let mut even_odd = false;
        for (k, (which, occ, body)) in parts.hf.iter().enumerate() {
            let tag = if *which < 2 { "hdr" } else { "ftr" };
            let part = if *which < 2 { "header" } else { "footer" };
            if *occ == Occurrence::Even {
                even_odd = true;
            }
            let body = if body.is_empty() { "<w:p/>".to_string() } else { body.clone() };
            let x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{tag} {ns}>{body}</w:{tag}>");
            overrides.push_str(&format!("<Override PartName=\"/word/{part}{k}.xml\" ContentType=\"{WML}.{part}+xml\"/>"));
            rels.push_str(&format!("<Relationship Id=\"rIdHF{k}\" Type=\"{REL}/{part}\" Target=\"{part}{k}.xml\"/>"));
            files.push((format!("word/{part}{k}.xml"), x));
        }
        for (k, url) in parts.links.iter().enumerate() {
            rels.push_str(&format!("<Relationship Id=\"rIdL{k}\" Type=\"{REL}/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>", xml_esc(url)));
        }
        for (k, (name, _, ext)) in parts.media.iter().enumerate() {
            rels.push_str(&format!("<Relationship Id=\"rIdImg{}\" Type=\"{REL}/image\" Target=\"media/{name}\"/>", k + 1));
            let def = format!("<Default Extension=\"{ext}\" ContentType=\"{}\"/>", if *ext == "jpeg" { "image/jpeg" } else { crate::raster::sniff(&parts.media[k].1).map(|f| f.mime()).unwrap_or("application/octet-stream") });
            if !overrides.contains(&def) {
                overrides.push_str(&def);
            }
        }
        let settings = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:settings {ns}>{}<w:defaultTabStop w:val=\"720\"/>{}<w:footnotePr><w:numFmt w:val=\"decimal\"/></w:footnotePr><w:endnotePr><w:numFmt w:val=\"decimal\"/></w:endnotePr></w:settings>",
            format!("{}{}", if even_odd { "<w:evenAndOddHeaders/>" } else { "" }, if doc.track { "<w:trackRevisions/>" } else { "" }),
            crate::fmtdocx::settings_xml(doc)
        );
        let mut all: Vec<(String, String)> = vec![
            (
                "[Content_Types].xml".into(),
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"txt\" ContentType=\"text/plain\"/>{overrides}</Types>"),
            ),
            (
                "_rels/.rels".into(),
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"{STREAM_REL}\" Target=\"{STREAM_PART}\"/><Relationship Id=\"rId4\" Type=\"{REL}/custom-properties\" Target=\"docProps/custom.xml\"/></Relationships>"),
            ),
            (
                "word/_rels/document.xml.rels".into(),
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"),
            ),
            ("word/document.xml".into(), docxml),
            ("word/styles.xml".into(), styles_xml(doc)),
            ("word/settings.xml".into(), settings),
            ("docProps/core.xml".into(), core_xml(doc)),
            ("docProps/custom.xml".into(), custom_xml(doc)),
            (STREAM_PART.into(), stream_txt),
        ];
        all.extend(files);
        let files = all;
        for (name, data) in files {
            z.start_file(name.as_str(), opt).map_err(std::io::Error::other)?;
            z.write_all(data.as_bytes())?;
        }
        for (name, data, _) in &parts.media {
            z.start_file(format!("word/media/{name}"), zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).map_err(std::io::Error::other)?;
            z.write_all(data)?;
        }
        z.finish().map_err(std::io::Error::other)?;
    }
    Ok(buf.into_inner())
}

// ------------------------------------------------------------------ read

pub fn read_docx(bytes: &[u8]) -> std::io::Result<Document> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(std::io::Error::other)?;
    let media = crate::gfxdocx::read_media(&mut z);
    let mut read_part = |name: &str| -> Option<String> {
        let mut f = z.by_name(name).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        Some(s)
    };
    read_parts_with_media(&mut read_part, &media)
}

/// Read a WordprocessingML package given a part lookup (`"word/document.xml"`
/// → its text).  Used for .docx/.docm/.dotx/.dotm (ZIP) and for Word's
/// flat OPC XML (`pkg:package`).
pub fn read_parts(read_part: &mut dyn FnMut(&str) -> Option<String>) -> std::io::Result<Document> {
    read_parts_with_media(read_part, &Default::default())
}

/// Read the document from its package parts; `media` holds word/media/* pictures.
pub fn read_parts_with_media(read_part: &mut dyn FnMut(&str) -> Option<String>, media: &std::collections::HashMap<String, Vec<u8>>) -> std::io::Result<Document> {
    let docxml = read_part("word/document.xml").ok_or_else(|| std::io::Error::other("not a Word document"))?;
    let doc_id = read_part("docProps/custom.xml")
        .and_then(|x| {
            let k = x.find("name=\"WP51NativeId\"")?;
            let rest = &x[k..];
            let a = rest.find("<vt:lpwstr>")? + "<vt:lpwstr>".len();
            let b = rest[a..].find("</vt:lpwstr>")? + a;
            Some(rest[a..b].to_string())
        })
        .unwrap_or_default();
    if let Some(st) = read_part(STREAM_PART) {
        if let Some(rest) = st.strip_prefix("#HASH ") {
            if let Some((h, body)) = rest.split_once('\n') {
                if u64::from_str_radix(h.trim(), 16).ok() == Some(fnv(docxml.as_bytes())) {
                    if let Some(mut d) = stream::read(body) {
                        if d.id.is_empty() {
                            d.id = doc_id.clone();
                        }
                        return Ok(d);
                    }
                }
            }
        }
    }
    let styles = read_part("word/styles.xml");
    let mut notes: std::collections::HashMap<(bool, String), Vec<El>> = Default::default();
    for (part, tag, foot) in [("word/footnotes.xml", "footnote", true), ("word/endnotes.xml", "endnote", false)] {
        if let Some(x) = read_part(part) {
            for (id, inner) in elements_with_id(&x, tag) {
                let mut d = parse_document_xml(&wrap_body(&inner), styles.as_deref());
                // drop the space after the reference mark
                if matches!(d.els.first(), Some(El::Ch(' '))) {
                    d.els.remove(0);
                }
                notes.insert((foot, id), d.els);
            }
        }
    }
    let mut comments: std::collections::HashMap<String, String> = Default::default();
    if let Some(x) = read_part("word/comments.xml") {
        for (id, inner) in elements_with_id(&x, "comment") {
            let cd = parse_document_xml(&wrap_body(&inner), styles.as_deref());
            comments.insert(id, cd.plain_text(0, cd.len()).trim_end_matches('\n').to_string());
        }
    }
    let numbering = read_part("word/numbering.xml");
    let rels0 = read_part("word/_rels/document.xml.rels").unwrap_or_default();
    let body_xml_conv = crate::gfxdocx::convert_drawings(&docxml, &rels0, media, styles.as_deref());
    let mut d = parse_document_xml_full(&body_xml_conv, styles.as_deref(), numbering.as_deref(), &notes, &comments, true);
    crate::fmtdocx::after_read(&mut d, &docxml, read_part("word/settings.xml").as_deref(), styles.as_deref());
    // headers / footers referenced from the (last) section
    let rels = read_part("word/_rels/document.xml.rels").unwrap_or_default();
    let mut hf: Vec<El> = vec![];
    let has_even = docxml.contains("w:type=\"even\"");
    for (tag, base) in [("headerReference", 0u8), ("footerReference", 2u8)] {
        for t in tags(&docxml, tag) {
            let ty = attr_in(&t, "w:type").unwrap_or_else(|| "default".into());
            let Some(rid) = attr_in(&t, "r:id") else { continue };
            let occ = match ty.as_str() {
                "even" => Occurrence::Even,
                "first" => continue,
                _ if has_even => Occurrence::Odd,
                _ => Occurrence::Every,
            };
            let Some(target) = tags(&rels, "Relationship").into_iter().find(|r| attr_in(r, "Id").as_deref() == Some(rid.as_str())).and_then(|r| attr_in(&r, "Target")) else {
                continue;
            };
            let path = format!("word/{}", target.trim_start_matches('/').trim_start_matches("word/"));
            if let Some(x) = read_part(&path) {
                let hd = parse_document_xml(&wrap_body(&x), styles.as_deref());
                let mut els = hd.els;
                resolve_links(&mut els, "");
                while matches!(els.last(), Some(El::Code(Code::HRt))) {
                    els.pop();
                }
                if els.is_empty() {
                    continue;
                }
                let which = base + if occ == Occurrence::Even { 1 } else { 0 };
                if !hf.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) {
                    hf.push(El::Code(Code::HeadFoot { which, occ, els }));
                }
            }
        }
    }
    resolve_links(&mut d.els, &rels);
    if !hf.is_empty() {
        let tail = std::mem::take(&mut d.els);
        d.els = hf;
        d.els.extend(tail);
    }
    d.id = doc_id;
    if read_part("word/settings.xml").is_some_and(|x| x.contains("trackRevisions")) {
        d.track = true;
    }
    Ok(d)
}

/// The target of a `HYPERLINK "url" \\l "anchor"` field instruction.
fn hyperlink_instr(instr: &str) -> Option<String> {
    let t = instr.trim();
    let rest = t.strip_prefix("HYPERLINK")?.trim();
    let mut url = String::new();
    let mut anchor = None;
    let mut args = vec![];
    let mut it = rest.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut a = String::new();
                for c in it.by_ref() {
                    if c == '"' {
                        break;
                    }
                    a.push(c);
                }
                args.push(a);
            }
            c if c.is_whitespace() => {}
            c => {
                let mut a = String::from(c);
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() {
                        break;
                    }
                    a.push(n);
                    it.next();
                }
                args.push(a);
            }
        }
    }
    let mut k = 0;
    while k < args.len() {
        match args[k].as_str() {
            "\\l" => {
                anchor = args.get(k + 1).cloned();
                k += 2;
            }
            s if s.starts_with('\\') => k += if matches!(s, "\\o" | "\\t") { 2 } else { 1 },
            s => {
                if url.is_empty() {
                    url = s.to_string();
                }
                k += 1;
            }
        }
    }
    match anchor {
        Some(a) if url.is_empty() => Some(format!("#{a}")),
        Some(a) => Some(format!("{url}#{a}")),
        None if url.is_empty() => None,
        None => Some(url),
    }
}

/// Replace `[Link:\0rid:rIdN]` placeholders with the relationship target;
/// links whose target is unknown are dropped (their text stays).
fn resolve_links(els: &mut Vec<El>, rels: &str) {
    let mut out = Vec::with_capacity(els.len());
    let mut drop_ends = 0usize;
    for mut e in els.drain(..) {
        match &mut e {
            El::Code(Code::Link(u)) if u.starts_with('\u{0}') => {
                let rid = u.trim_start_matches("\u{0}rid:").to_string();
                let target = tags(rels, "Relationship").into_iter().find(|r| attr_in(r, "Id").as_deref() == Some(rid.as_str())).and_then(|r| attr_in(&r, "Target"));
                match target {
                    Some(t) => *u = t,
                    None => {
                        drop_ends += 1;
                        continue;
                    }
                }
            }
            El::Code(Code::LinkEnd) if drop_ends > 0 => {
                drop_ends -= 1;
                continue;
            }
            El::Code(Code::Note { els: inner, .. }) | El::Code(Code::HeadFoot { els: inner, .. }) => resolve_links(inner, ""),
            _ => {}
        }
        out.push(e);
    }
    *els = out;
}

#[allow(dead_code)]
pub fn resolve_links_pub(els: &mut Vec<El>, rels: &str) {
    resolve_links(els, rels)
}

fn wrap_body(inner: &str) -> String {
    format!("<w:document><w:body>{inner}</w:body></w:document>")
}

/// Every `<w:TAG …>` start tag (as text) in `xml`.
fn tags(xml: &str, tag: &str) -> Vec<String> {
    let mut out = vec![];
    let pat = format!("<w:{tag} ");
    let pat2 = format!("<{tag} ");
    for p in [pat, pat2] {
        let mut i = 0;
        while let Some(k) = xml[i..].find(&p) {
            let s = i + k;
            let e = xml[s..].find('>').map(|x| s + x + 1).unwrap_or(xml.len());
            out.push(xml[s..e].to_string());
            i = e;
        }
    }
    out
}

fn attr_in(tag: &str, name: &str) -> Option<String> {
    let p = format!("{name}=\"");
    let k = tag.find(&p)? + p.len();
    let e = tag[k..].find('"')? + k;
    Some(tag[k..e].replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\""))
}

/// (id, inner xml) of each `<w:TAG w:id="…">…</w:TAG>` except separators.
fn elements_with_id(xml: &str, tag: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let open = format!("<w:{tag} ");
    let close = format!("</w:{tag}>");
    let mut i = 0;
    while let Some(k) = xml[i..].find(&open) {
        let s = i + k;
        let Some(gt) = xml[s..].find('>').map(|x| s + x) else { break };
        let head = &xml[s..=gt];
        let Some(end) = xml[gt..].find(&close).map(|x| gt + x) else { break };
        let inner = &xml[gt + 1..end];
        i = end + close.len();
        if head.contains("separator") || head.contains("continuationSeparator") || head.contains("continuationNotice") {
            continue;
        }
        if let Some(id) = attr_in(head, "w:id") {
            out.push((id, inner.to_string()));
        }
    }
    out
}

#[derive(Default, Clone)]
struct RState {
    b: bool,
    i: bool,
    u: Option<bool>, // Some(true)=double
    strike: bool,
    smallcaps: bool,
    sup: bool,
    sub: bool,
    font: Option<String>,
    sz: Option<u32>, // half points
    rstyle: Option<String>,
}

fn attr_of(name: &[u8]) -> Option<&'static str> {
    Some(match name {
        b"b" => "b",
        b"i" => "i",
        b"u" => "u",
        b"strike" | b"dstrike" => "strike",
        b"smallCaps" => "smallCaps",
        b"vertAlign" => "vertAlign",
        b"rFonts" => "rFonts",
        b"sz" => "sz",
        _ => return None,
    })
}

pub(crate) fn local(n: &[u8]) -> &[u8] {
    match n.iter().position(|&c| c == b':') {
        Some(p) => &n[p + 1..],
        None => n,
    }
}

pub(crate) fn get_attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        if local(a.key.as_ref()) == key {
            return a.unescape_value().ok().map(|v| v.to_string());
        }
    }
    None
}

pub(crate) fn toggle_on(e: &quick_xml::events::BytesStart) -> bool {
    match get_attr(e, b"val").as_deref() {
        Some("0") | Some("false") | Some("off") | Some("none") => false,
        _ => true,
    }
}

/// Convert Word's paragraphs/runs back into a WP code stream.
/// Reading `<w:sectPr>`: a section with two or more columns becomes
/// `[Col Def:Newspaper…][Col On]` … `[Col Off]`.
#[derive(Default)]
struct SectCols {
    inside: bool,
    /// element index where the current section's text starts
    start: usize,
    num: usize,
    space: i32,
    widths: Vec<(i32, i32)>,
    margins: Option<(Option<i32>, Option<i32>)>,
    /// the section ended with the paragraph being read
    ended: Option<bool>,
}

impl SectCols {
    fn open(&mut self) {
        self.inside = true;
        self.num = 1;
        self.space = 600;
        self.widths.clear();
        self.margins = None;
    }
    fn cols(&mut self, num: Option<String>, space: Option<String>) {
        self.num = num.and_then(|v| v.parse().ok()).unwrap_or(1);
        if let Some(sp) = space.and_then(|v| v.parse::<i32>().ok()) {
            self.space = wpu_from_twips(sp);
        }
    }
    fn col(&mut self, w: Option<String>, space: Option<String>) {
        let w = w.and_then(|v| v.parse::<i32>().ok()).map(wpu_from_twips).unwrap_or(0);
        let sp = space.and_then(|v| v.parse::<i32>().ok()).map(wpu_from_twips).unwrap_or(0);
        self.widths.push((w, sp));
    }
    fn margins(&mut self, l: Option<String>, r: Option<String>) {
        let f = |v: Option<String>| v.and_then(|v| v.parse::<i32>().ok()).map(wpu_from_twips);
        self.margins = Some((f(l), f(r)));
    }
    /// `</w:sectPr>`: the section ends here (in a paragraph: with it).
    fn close(&mut self, d: &mut Document, in_para: bool) {
        if !self.inside {
            return;
        }
        self.inside = false;
        let (ml, mr) = self.margins.unwrap_or((None, None));
        let left = ml.unwrap_or(d.init.left);
        let right = d.init.page_width - mr.unwrap_or(d.init.right);
        let mut cols = vec![];
        if self.widths.len() >= 2 {
            let mut x = left;
            for (w, sp) in &self.widths {
                cols.push((x, x + w));
                x += w + sp;
            }
        } else if self.num >= 2 {
            cols = crate::columns::even_columns(self.num, self.space, left, right);
        }
        let on = cols.len() >= 2;
        if on {
            let mut at = self.start.min(d.els.len());
            // the document's first justification code stays first
            if at == 0 && matches!(d.els.first(), Some(El::Code(Code::Just(_)))) {
                at = 1;
            }
            d.els.splice(at..at, [El::Code(Code::ColDef { kind: ColKind::Newspaper, cols }), El::Code(Code::ColOn)]);
        }
        if in_para {
            self.ended = Some(on);
        } else {
            if on {
                d.els.push(El::Code(Code::ColOff));
            }
            self.start = d.els.len();
        }
    }
    /// A new paragraph/table begins: a section that ended with the
    /// previous paragraph is closed.
    fn next_section(&mut self, d: &mut Document) {
        if let Some(on) = self.ended.take() {
            if on {
                d.els.push(El::Code(Code::ColOff));
            }
            self.start = d.els.len();
        }
    }
}

pub fn parse_document_xml(xml: &str, styles: Option<&str>) -> Document {
    parse_document_xml_ext(xml, styles, &Default::default(), &Default::default())
}

pub fn parse_document_xml_ext(
    xml: &str,
    styles: Option<&str>,
    notes: &std::collections::HashMap<(bool, String), Vec<El>>,
    comments: &std::collections::HashMap<String, String>,
) -> Document {
    parse_document_xml_full(xml, styles, None, notes, comments, false)
}

/// `with_styles`: turn Word paragraph/character styles into our styles
/// (main text only) and lists from numbering.xml into paragraph numbers.
pub fn parse_document_xml_full(
    xml: &str,
    styles: Option<&str>,
    numbering: Option<&str>,
    notes: &std::collections::HashMap<(bool, String), Vec<El>>,
    comments: &std::collections::HashMap<String, String>,
    with_styles: bool,
) -> Document {
    let wstyles = match styles {
        Some(s) if with_styles => crate::docx_styles::read_word_styles(s),
        _ => Default::default(),
    };
    let wnum = numbering.map(crate::docx_styles::read_numbering).unwrap_or_default();
    let mut lists = crate::docx_styles::ListState::default();
    let mut p_style: Option<String> = None;
    let mut num_id: Option<String> = None;
    let mut num_ilvl: u8 = 0;
    let mut para_open: Option<String> = None;
    // justification set by the paragraph style (its off code resets Left)
    let mut style_jc: Option<Justify> = None;
    let mut cstyle_open: Option<String> = None;
    let mut d = Document::new();
    // default font/size from styles.xml
    let mut base_font = d.init.font.clone();
    if let Some(st) = styles {
        let mut r = Reader::from_str(st);
        let mut in_default = false;
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) if local(e.name().as_ref()) == b"rPrDefault" => in_default = true,
                Ok(Event::End(e)) if local(e.name().as_ref()) == b"rPrDefault" => in_default = false,
                Ok(Event::Empty(e)) | Ok(Event::Start(e)) if in_default => match local(e.name().as_ref()) {
                    b"rFonts" => {
                        if let Some(f) = get_attr(&e, b"ascii").or_else(|| get_attr(&e, b"hAnsi")) {
                            base_font.name = f;
                        }
                    }
                    b"sz" => {
                        if let Some(v) = get_attr(&e, b"val").and_then(|v| v.parse::<u32>().ok()) {
                            base_font.size_cpt = v * 50;
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    d.init.font = base_font.clone();

    let mut r = Reader::from_str(xml);
    let mut in_rpr = false;
    let mut in_ppr = false;
    let mut in_t = false;
    let mut run = RState::default();
    let mut cur_attrs = AttrSet::default();
    let mut cur_font = base_font.clone();
    let mut para_jc: Option<Justify> = None;
    // tab stops of the current paragraph (<w:tabs> in its pPr), and the
    // set in effect in the text so far (empty = the document's default)
    let mut para_tabs: Vec<TabStop> = Vec::new();
    let mut cur_tabs: Vec<TabStop> = Vec::new();
    let mut cur_just = d.init.justify;
    let mut first_para = true;
    let mut tbl_def_at: Option<usize> = None;
    let mut pending_ppr_done = false;
    let mut in_body_para = false;
    // w:keepLines → [Block Pro:On] … [Block Pro:Off] around the paragraph
    let mut para_keep = false;
    let mut depth_tbl = 0;
    let mut cell_count = 0;
    let mut sect_seen = false;
    // sections with <w:cols> → [Col Def][Col On] … [Col Off]
    let mut sect = SectCols::default();
    // field codes (HYPERLINK fields): instruction text being collected and,
    // per open field, whether it produced a [Link]
    let mut in_instr = false;
    let mut instr = String::new();
    let mut fld_links: Vec<bool> = vec![];

    // our style name for a Word style id (registering the definition)
    let use_style = |d: &mut Document, id: &str, character: bool, base: &FontSpec| -> Option<String> {
        let ws = wstyles.get(id)?;
        if ws.character != character || !crate::docx_styles::importable(id, ws) {
            return None;
        }
        let mut def = crate::docx_styles::style_def(ws, base);
        // a linked character style ("Heading 1 Char") is the same style
        if character {
            if let Some(b) = def.name.strip_suffix(" Char") {
                def.name = b.to_string();
            }
        }
        let name = def.name.clone();
        if !d.styles.iter().any(|s| s.name == name) {
            d.styles.push(def);
        }
        Some(name)
    };
    let sync_cstyle = |d: &mut Document, open: &mut Option<String>, want: Option<String>| {
        if *open != want {
            if let Some(o) = open.take() {
                d.els.push(El::Code(Code::StyleOff(o)));
            }
            if let Some(w) = want {
                d.els.push(El::Code(Code::StyleOn(w.clone())));
                *open = Some(w);
            }
        }
    };
    let set_run_attrs = |d: &mut Document, cur: &mut AttrSet, cur_font: &mut FontSpec, rs: &RState, base: &FontSpec| {
        let mut want = AttrSet::default();
        want.set(Attr::Bold, rs.b);
        want.set(Attr::Italic, rs.i);
        want.set(Attr::Underline, rs.u == Some(false));
        want.set(Attr::DblUnderline, rs.u == Some(true));
        want.set(Attr::Strikeout, rs.strike);
        want.set(Attr::SmallCaps, rs.smallcaps);
        want.set(Attr::Superscript, rs.sup);
        want.set(Attr::Subscript, rs.sub);
        let font = FontSpec {
            name: rs.font.clone().unwrap_or_else(|| base.name.clone()),
            size_cpt: rs.sz.map(|s| s * 50).unwrap_or(base.size_cpt),
        };
        // close attrs no longer wanted (in reverse order), then open new
        for a in Attr::ALL.iter().rev() {
            if cur.has(*a) && !want.has(*a) {
                d.els.push(El::Code(Code::AttrOff(*a)));
                cur.set(*a, false);
            }
        }
        if font != *cur_font {
            d.els.push(El::Code(Code::Font(font.clone())));
            *cur_font = font;
        }
        for a in Attr::ALL {
            if !cur.has(a) && want.has(a) {
                d.els.push(El::Code(Code::AttrOn(a)));
                cur.set(a, true);
            }
        }
    };

    loop {
        let ev = r.read_event();
        match ev {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let is_empty = matches!(ev, Ok(Event::Empty(_)));
                let name = local(e.name().as_ref()).to_vec();
                match name.as_slice() {
                    b"p" => {
                        if !first_para {
                            // close attributes at paragraph end like WP would carry? keep running
                            d.els.push(El::Code(Code::HRt));
                        }
                        sect.next_section(&mut d);
                        first_para = false;
                        para_jc = None;
                        para_tabs.clear();
                        p_style = None;
                        style_jc = None;
                        num_id = None;
                        num_ilvl = 0;
                        pending_ppr_done = false;
                        in_body_para = !is_empty;
                        if is_empty {
                            // empty paragraph
                        }
                    }
                    b"pPr" if !is_empty => in_ppr = true,
                    b"pStyle" if in_ppr => p_style = get_attr(e, b"val"),
                    b"numId" if in_ppr => num_id = get_attr(e, b"val"),
                    b"ilvl" if in_ppr => num_ilvl = get_attr(e, b"val").and_then(|v| v.parse().ok()).unwrap_or(0),
                    b"rStyle" if in_rpr => run.rstyle = get_attr(e, b"val"),
                    b"keepLines" if in_ppr && depth_tbl == 0 && toggle_on(e) => {
                        if !para_keep {
                            para_keep = true;
                            d.els.push(El::Code(Code::Fmt(crate::fmt::Fmt::BlockPro(true))));
                        }
                    }
                    b"jc" if in_ppr => {
                        para_jc = match get_attr(e, b"val").as_deref() {
                            Some("center") => Some(Justify::Center),
                            Some("right") | Some("end") => Some(Justify::Right),
                            Some("both") | Some("distribute") => Some(Justify::Full),
                            _ => Some(Justify::Left),
                        };
                    }
                    b"r" if !is_empty => {
                        run = RState::default();
                        if !pending_ppr_done {
                            pending_ppr_done = true;
                            if para_tabs != cur_tabs {
                                let ts = if para_tabs.is_empty() {
                                    d.init.tabs.clone()
                                } else {
                                    // Word continues with default stops every 0.5" after the last one
                                    let mut stops = para_tabs.clone();
                                    stops.sort_by_key(|t| t.pos);
                                    let last = stops.last().map(|t| t.pos).unwrap_or(0);
                                    let mut p = (last / 600 + 1) * 600;
                                    while p <= 9600 {
                                        stops.push(TabStop { pos: p, kind: TabKind::Left, dot_leader: false });
                                        p += 600;
                                    }
                                    TabSet { stops, relative: true }
                                };
                                d.els.push(El::Code(Code::TabSet(ts)));
                                cur_tabs = para_tabs.clone();
                            }
                            let j = para_jc.unwrap_or(Justify::Left);
                            if style_jc.is_some() && (para_jc.is_none() || para_jc == style_jc) {
                                // the style sets it; its [Style Off] restores Left
                                cur_just = Justify::Left;
                            } else if j != cur_just {
                                d.els.push(El::Code(Code::Just(j)));
                                cur_just = j;
                            }
                        }
                    }
                    b"rPr" if !is_empty && !in_ppr => in_rpr = true,
                    n if in_rpr && attr_of(n).is_some() => match attr_of(n).unwrap() {
                        "b" => run.b = toggle_on(e),
                        "i" => run.i = toggle_on(e),
                        "u" => {
                            run.u = match get_attr(e, b"val").as_deref() {
                                Some("none") | Some("0") => None,
                                Some("double") => Some(true),
                                _ => Some(false),
                            }
                        }
                        "strike" => run.strike = toggle_on(e),
                        "smallCaps" => run.smallcaps = toggle_on(e),
                        "vertAlign" => match get_attr(e, b"val").as_deref() {
                            Some("superscript") => run.sup = true,
                            Some("subscript") => run.sub = true,
                            _ => {}
                        },
                        "rFonts" => run.font = get_attr(e, b"ascii").or_else(|| get_attr(e, b"hAnsi")),
                        "sz" => run.sz = get_attr(e, b"val").and_then(|v| v.parse().ok()),
                        _ => {}
                    },
                    b"t" if !is_empty => {
                        in_t = true;
                        let want = run.rstyle.clone().and_then(|id| use_style(&mut d, &id, true, &base_font));
                        sync_cstyle(&mut d, &mut cstyle_open, want);
                        set_run_attrs(&mut d, &mut cur_attrs, &mut cur_font, &run, &base_font);
                    }
                    b"tab" if in_ppr => {
                        // a tab stop (position relative to the paragraph's left edge)
                        let pos = get_attr(e, b"pos").and_then(|v| v.parse::<i64>().ok()).map(|t| (t * 1200 / 1440) as i32);
                        let kind = match get_attr(e, b"val").as_deref() {
                            Some("center") => Some(TabKind::Center),
                            Some("right") | Some("end") => Some(TabKind::Right),
                            Some("decimal") => Some(TabKind::Decimal),
                            Some("left") | Some("start") | None => Some(TabKind::Left),
                            _ => None, // clear, bar, num
                        };
                        if let (Some(pos), Some(kind)) = (pos, kind) {
                            if pos > 0 {
                                let dot_leader = matches!(get_attr(e, b"leader").as_deref(), Some("dot") | Some("middleDot"));
                                para_tabs.push(TabStop { pos, kind, dot_leader });
                            }
                        }
                    }
                    b"tab" if !in_ppr => {
                        set_run_attrs(&mut d, &mut cur_attrs, &mut cur_font, &run, &base_font);
                        d.els.push(El::Code(Code::Tab));
                    }
                    b"sectPr" if !is_empty => sect.open(),
                    b"cols" if sect.inside => sect.cols(get_attr(e, b"num"), get_attr(e, b"space")),
                    b"col" if sect.inside => sect.col(get_attr(e, b"w"), get_attr(e, b"space")),
                    b"pgMar" if sect.inside && sect_seen => sect.margins(get_attr(e, b"left"), get_attr(e, b"right")),
                    b"br" => {
                        match get_attr(e, b"type").as_deref() {
                            Some("page") | Some("column") => {
                                // HRt already pending at para end; turn into HPg
                                d.els.push(El::Code(Code::HPg));
                            }
                            _ => d.els.push(El::Code(Code::HRt)),
                        }
                    }
                    b"noBreakHyphen" => d.els.push(El::Code(Code::HardHyphen)),
                    b"hyperlink" if !is_empty => {
                        if !pending_ppr_done {
                            pending_ppr_done = true;
                            if para_tabs != cur_tabs {
                                let ts = if para_tabs.is_empty() {
                                    d.init.tabs.clone()
                                } else {
                                    // Word continues with default stops every 0.5" after the last one
                                    let mut stops = para_tabs.clone();
                                    stops.sort_by_key(|t| t.pos);
                                    let last = stops.last().map(|t| t.pos).unwrap_or(0);
                                    let mut p = (last / 600 + 1) * 600;
                                    while p <= 9600 {
                                        stops.push(TabStop { pos: p, kind: TabKind::Left, dot_leader: false });
                                        p += 600;
                                    }
                                    TabSet { stops, relative: true }
                                };
                                d.els.push(El::Code(Code::TabSet(ts)));
                                cur_tabs = para_tabs.clone();
                            }
                            let j = para_jc.unwrap_or(Justify::Left);
                            if j != cur_just {
                                d.els.push(El::Code(Code::Just(j)));
                                cur_just = j;
                            }
                        }
                        let target = match get_attr(e, b"anchor") {
                            Some(a) if get_attr(e, b"id").is_none() => format!("#{a}"),
                            _ => format!("\u{0}rid:{}", get_attr(e, b"id").unwrap_or_default()),
                        };
                        d.els.push(El::Code(Code::Link(target)));
                    }
                    b"bookmarkStart" => {
                        if !pending_ppr_done {
                            pending_ppr_done = true;
                            if para_tabs != cur_tabs {
                                let ts = if para_tabs.is_empty() {
                                    d.init.tabs.clone()
                                } else {
                                    // Word continues with default stops every 0.5" after the last one
                                    let mut stops = para_tabs.clone();
                                    stops.sort_by_key(|t| t.pos);
                                    let last = stops.last().map(|t| t.pos).unwrap_or(0);
                                    let mut p = (last / 600 + 1) * 600;
                                    while p <= 9600 {
                                        stops.push(TabStop { pos: p, kind: TabKind::Left, dot_leader: false });
                                        p += 600;
                                    }
                                    TabSet { stops, relative: true }
                                };
                                d.els.push(El::Code(Code::TabSet(ts)));
                                cur_tabs = para_tabs.clone();
                            }
                            let j = para_jc.unwrap_or(Justify::Left);
                            if j != cur_just {
                                d.els.push(El::Code(Code::Just(j)));
                                cur_just = j;
                            }
                        }
                        if let Some(n) = get_attr(e, b"name").filter(|n| n != "_GoBack") {
                            // WP cross-reference targets were written as WPT_name
                            if let Some(t) = n.strip_prefix("WPT_") {
                                d.els.push(El::Code(Code::Mark(Mark::Target { name: t.to_string() })));
                            } else if !n.starts_with('_') {
                                d.els.push(El::Code(Code::Bookmark(n)));
                            }
                        }
                    }
                    b"fldSimple" if !is_empty => {
                        if !pending_ppr_done {
                            pending_ppr_done = true;
                            if para_tabs != cur_tabs {
                                let ts = if para_tabs.is_empty() {
                                    d.init.tabs.clone()
                                } else {
                                    // Word continues with default stops every 0.5" after the last one
                                    let mut stops = para_tabs.clone();
                                    stops.sort_by_key(|t| t.pos);
                                    let last = stops.last().map(|t| t.pos).unwrap_or(0);
                                    let mut p = (last / 600 + 1) * 600;
                                    while p <= 9600 {
                                        stops.push(TabStop { pos: p, kind: TabKind::Left, dot_leader: false });
                                        p += 600;
                                    }
                                    TabSet { stops, relative: true }
                                };
                                d.els.push(El::Code(Code::TabSet(ts)));
                                cur_tabs = para_tabs.clone();
                            }
                            let j = para_jc.unwrap_or(Justify::Left);
                            if j != cur_just {
                                d.els.push(El::Code(Code::Just(j)));
                                cur_just = j;
                            }
                        }
                        let url = get_attr(e, b"instr").as_deref().and_then(hyperlink_instr);
                        fld_links.push(url.is_some());
                        if let Some(u) = url {
                            d.els.push(El::Code(Code::Link(u)));
                        }
                    }
                    b"instrText" if !is_empty => in_instr = true,
                    b"fldChar" => match get_attr(e, b"fldCharType").as_deref() {
                        Some("begin") => {
                            instr.clear();
                            fld_links.push(false);
                        }
                        Some("separate") => {
                            if let Some(u) = hyperlink_instr(&instr) {
                                if let Some(top) = fld_links.last_mut() {
                                    *top = true;
                                    d.els.push(El::Code(Code::Link(u)));
                                }
                            }
                            instr.clear();
                        }
                        Some("end") => {
                            if fld_links.pop() == Some(true) {
                                d.els.push(El::Code(Code::LinkEnd));
                            }
                        }
                        _ => {}
                    },
                    b"ins" | b"del" if !in_rpr && !in_ppr && !is_empty => {
                        let del = name.as_slice() == b"del";
                        let author = get_attr(e, b"author").unwrap_or_default();
                        let date = get_attr(e, b"date").unwrap_or_default();
                        d.els.push(El::Code(Code::RevOn { del, author, date }));
                    }
                    b"delText" if !is_empty => {
                        in_t = true;
                        set_run_attrs(&mut d, &mut cur_attrs, &mut cur_font, &run, &base_font);
                    }
                    b"footnoteReference" | b"endnoteReference" => {
                        let foot = name.as_slice() == b"footnoteReference";
                        if let Some(id) = get_attr(e, b"id") {
                            let els = notes.get(&(foot, id)).cloned().unwrap_or_default();
                            d.els.push(El::Code(Code::Note { kind: if foot { NoteKind::Foot } else { NoteKind::End }, els }));
                        }
                    }
                    b"softHyphen" => d.els.push(El::Code(Code::SoftHyphen)),
                    b"commentReference" => {
                        if let Some(t) = get_attr(e, b"id").and_then(|id| comments.get(&id)) {
                            d.els.push(El::Code(Code::Comment(t.clone())));
                        }
                    }
                    b"ptab" => {
                        set_run_attrs(&mut d, &mut cur_attrs, &mut cur_font, &run, &base_font);
                        match get_attr(e, b"alignment").as_deref() {
                            Some("center") => d.els.push(El::Code(Code::Center)),
                            Some("left") => d.els.push(El::Code(Code::Tab)),
                            _ if get_attr(e, b"leader").as_deref() == Some("dot") => d.els.push(El::Code(Code::FlushRightDot)),
                            _ => d.els.push(El::Code(Code::FlushRight)),
                        }
                    }
                    b"wp51box" => {
                        if let Some(c) = get_attr(e, b"s").and_then(|s| stream::str_to_code(&s)) {
                            d.els.push(El::Code(c));
                        }
                    }
                    b"sym" => {
                        if let Some(c) = get_attr(e, b"char").and_then(|h| u32::from_str_radix(&h, 16).ok()) {
                            let c = if (0xF000..=0xF0FF).contains(&c) { c - 0xF000 } else { c };
                            if let Some(ch) = char::from_u32(c) {
                                d.els.push(El::Ch(ch));
                            }
                        }
                    }
                    b"tbl" => {
                        depth_tbl += 1;
                        if depth_tbl == 1 {
                            // a table starts on its own line in WP
                            if !first_para && !matches!(d.els.last(), Some(El::Code(Code::HRt)) | None) {
                                d.els.push(El::Code(Code::HRt));
                            }
                            sect.next_section(&mut d);
                            tbl_def_at = Some(d.els.len());
                            d.els.push(El::Code(Code::TblDef { cols: vec![] }));
                        }
                    }
                    b"gridCol" if depth_tbl == 1 => {
                        if let (Some(at), Some(w)) = (tbl_def_at, get_attr(e, b"w").and_then(|v| v.parse::<i32>().ok())) {
                            if let El::Code(Code::TblDef { cols }) = &mut d.els[at] {
                                cols.push(wpu_from_twips(w));
                            }
                        }
                    }
                    b"tc" => {
                        if depth_tbl == 1 {
                            d.els.push(El::Code(Code::Cell));
                            first_para = true;
                        } else {
                            if cell_count > 0 {
                                d.els.push(El::Code(Code::Tab));
                            }
                            cell_count += 1;
                        }
                    }
                    b"tr" => {
                        cell_count = 0;
                        if depth_tbl == 1 {
                            d.els.push(El::Code(Code::Row));
                        }
                    }
                    b"pgSz" if !sect_seen => {
                        if let Some(w) = get_attr(e, b"w").and_then(|v| v.parse::<i32>().ok()) {
                            d.init.page_width = wpu_from_twips(w);
                        }
                        if let Some(h) = get_attr(e, b"h").and_then(|v| v.parse::<i32>().ok()) {
                            d.init.page_height = wpu_from_twips(h);
                        }
                    }
                    b"pgMar" if !sect_seen => {
                        let g = |k: &[u8]| get_attr(e, k).and_then(|v| v.parse::<i32>().ok()).map(wpu_from_twips);
                        if let Some(v) = g(b"left") {
                            d.init.left = v;
                        }
                        if let Some(v) = g(b"right") {
                            d.init.right = v;
                        }
                        if let Some(v) = g(b"top") {
                            d.init.top = v;
                        }
                        if let Some(v) = g(b"bottom") {
                            d.init.bottom = v;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => match local(e.name().as_ref()) {
                b"pPr" if in_ppr => {
                    in_ppr = false;
                    if let Some(id) = p_style.take() {
                        if let Some(name) = use_style(&mut d, &id, false, &base_font) {
                            style_jc = wstyles.get(&id).and_then(|w| w.jc);
                            d.els.push(El::Code(Code::StyleOn(name.clone())));
                            para_open = Some(name);
                        }
                    }
                    if let Some(id) = num_id.take() {
                        let codes = lists.para(&wnum, &id, num_ilvl);
                        d.els.extend(codes);
                    }
                }
                b"pPr" => in_ppr = false,
                b"rPr" => in_rpr = false,
                b"t" | b"delText" => in_t = false,
                b"instrText" => in_instr = false,
                b"hyperlink" => d.els.push(El::Code(Code::LinkEnd)),
                b"fldSimple" => {
                    if fld_links.pop() == Some(true) {
                        d.els.push(El::Code(Code::LinkEnd));
                    }
                }
                b"ins" if !in_rpr && !in_ppr => d.els.push(El::Code(Code::RevOff { del: false })),
                b"del" if !in_rpr && !in_ppr => d.els.push(El::Code(Code::RevOff { del: true })),
                b"p" => {
                    in_body_para = false;
                    sync_cstyle(&mut d, &mut cstyle_open, None);
                    if let Some(n) = para_open.take() {
                        d.els.push(El::Code(Code::StyleOff(n)));
                    }
                    if para_keep {
                        para_keep = false;
                        d.els.push(El::Code(Code::Fmt(crate::fmt::Fmt::BlockPro(false))));
                    }
                }
                b"tbl" => {
                    if depth_tbl == 1 {
                        d.els.push(El::Code(Code::TblOff));
                        // fill in missing grid widths
                        if let Some(at) = tbl_def_at.take() {
                            let ncols = crate::tables::parse(&d.els, at).map(|t| t.rows.iter().map(|r| r.len()).max().unwrap_or(1)).unwrap_or(1);
                            if let El::Code(Code::TblDef { cols }) = &mut d.els[at] {
                                if cols.len() < ncols {
                                    let w = (d.init.page_width - d.init.left - d.init.right) / ncols.max(1) as i32;
                                    cols.resize(ncols, w);
                                }
                            }
                        }
                        first_para = true;
                    }
                    depth_tbl -= 1;
                }
                b"sectPr" => {
                    sect_seen = true;
                    sect.close(&mut d, in_body_para);
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_instr => {
                if let Ok(s) = t.unescape() {
                    instr.push_str(&s);
                }
            }
            Ok(Event::Text(t)) if in_t => {
                if let Ok(s) = t.unescape() {
                    for c in s.chars() {
                        d.els.push(if c == '\u{a0}' { El::Code(Code::HardSpace) } else { El::Ch(c) });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    let _ = (in_body_para, depth_tbl);
    sect.next_section(&mut d);
    // close open attributes
    for a in Attr::ALL.iter().rev() {
        if cur_attrs.has(*a) {
            d.els.push(El::Code(Code::AttrOff(*a)));
        }
    }
    // A page break run inside a paragraph followed by the HRt we insert for
    // the next paragraph: collapse "[HPg][HRt]" → "[HPg]".
    let mut out: Vec<El> = Vec::with_capacity(d.els.len());
    for e in d.els.drain(..) {
        if e == El::Code(Code::HRt) && out.last() == Some(&El::Code(Code::HPg)) {
            continue;
        }
        out.push(e);
    }
    d.els = out;
    // the initial justification of the doc follows the first paragraph
    d.init.justify = Justify::Left;
    if let Some(El::Code(Code::Just(j))) = d.els.first().cloned() {
        d.init.justify = j;
        d.els.remove(0);
    }
    d.normalize_attrs();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut d = Document::from_text("Erste Zeile mit Umlauten äöü ß\nПривет мир\n\tTab");
        d.els.insert(0, El::Code(Code::AttrOn(Attr::Bold)));
        d.els.insert(6, El::Code(Code::AttrOff(Attr::Bold)));
        d
    }

    #[test]
    fn roundtrip_private_stream() {
        let d = sample();
        let bytes = write_docx(&d).unwrap();
        let d2 = read_docx(&bytes).unwrap();
        assert_eq!(d.els, d2.els);
    }

    #[test]
    fn notes_and_headers_via_xml() {
        let mut d = sample();
        d.els.insert(0, El::Code(Code::HeadFoot { which: 0, occ: Occurrence::Every, els: "Kopf".chars().map(El::Ch).collect() }));
        d.els.push(El::Code(Code::Note { kind: NoteKind::Foot, els: "Fußnote".chars().map(El::Ch).collect() }));
        let bytes = write_docx(&d).unwrap();
        // force the XML path by corrupting the private stream hash
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for i in 0..z.len() {
                let mut f = z.by_index(i).unwrap();
                let name = f.name().to_string();
                let mut data = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut data).unwrap();
                if name == STREAM_PART {
                    data = b"#HASH 0\n".to_vec();
                }
                w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(&data).unwrap();
            }
            w.finish().unwrap();
        }
        let d2 = read_docx(&out.into_inner()).unwrap();
        assert!(matches!(&d2.els[0], El::Code(Code::HeadFoot { which: 0, els, .. }) if els.len() == 4), "{:?}", &d2.els[..2]);
        assert!(d2.els.iter().any(|e| matches!(e, El::Code(Code::Note { kind: NoteKind::Foot, els }) if els.len() == 7)));
    }

    #[test]
    fn tracked_changes_via_xml() {
        let mut d = Document::from_text("ab");
        d.els.insert(1, El::Code(Code::RevOn { del: false, author: "Maria".into(), date: "2026-11-11T10:00:00Z".into() }));
        d.els.insert(2, El::Ch('X'));
        d.els.insert(3, El::Code(Code::RevOff { del: false }));
        d.els.push(El::Code(Code::RevOn { del: true, author: "Maria".into(), date: "2026-11-11T10:00:00Z".into() }));
        d.els.push(El::Ch('Z'));
        d.els.push(El::Code(Code::RevOff { del: true }));
        let xml = document_xml(&d, &mut Parts::default());
        assert!(xml.contains("<w:ins ") && xml.contains("<w:delText"), "{xml}");
        let d2 = parse_document_xml(&xml, None);
        let revs = d2.els.iter().filter(|e| matches!(e, El::Code(Code::RevOn { .. }))).count();
        assert_eq!(revs, 2, "{:?}", d2.els);
        assert_eq!(d2.plain_text(0, d2.len()), "aXbZ");
    }

    #[test]
    fn table_via_xml() {
        let mut d = Document::from_text("Vor\n");
        let mut t = crate::tables::new_table(2, 2, 7200);
        // put text in cells: after each [Cell]
        let mut k = 0;
        let mut out = vec![];
        for e in t.drain(..) {
            let is_cell = e == El::Code(Code::Cell);
            out.push(e);
            if is_cell {
                k += 1;
                out.extend(format!("Z{k}").chars().map(El::Ch));
            }
        }
        d.els.extend(out);
        d.els.extend("Nach".chars().map(El::Ch));
        let xml = document_xml(&d, &mut Parts::default());
        assert!(xml.contains("<w:tbl>"));
        let d2 = parse_document_xml(&xml, None);
        let t2 = d2.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        let tb = crate::tables::parse(&d2.els, t2).unwrap();
        assert_eq!(tb.rows.len(), 2);
        assert_eq!(tb.cols.len(), 2);
        assert_eq!(d2.plain_text(0, d2.len()).replace('\n', "|"), d.plain_text(0, d.len()).replace('\n', "|"));
    }

    /// Write, then read back through document.xml only (no private stream).
    fn via_xml(d: &Document) -> Document {
        let bytes = write_docx(d).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for i in 0..z.len() {
                let mut f = z.by_index(i).unwrap();
                let name = f.name().to_string();
                let mut data = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut data).unwrap();
                if name == STREAM_PART {
                    data = b"#HASH 0\n".to_vec();
                }
                w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(&data).unwrap();
            }
            w.finish().unwrap();
        }
        read_docx(&out.into_inner()).unwrap()
    }

    #[test]
    fn comments_bookmarks_refs_via_xml() {
        let mut d = Document::from_text("Text mit Kommentar");
        d.els.insert(4, El::Code(Code::Comment("Bitte prüfen\nZeile 2".into())));
        d.els.push(El::Code(Code::Mark(Mark::Target { name: "kap 1".into() })));
        d.els.extend(Document::from_text(" Seite ").els);
        d.els.push(El::Code(Code::Mark(Mark::Ref { name: "kap 1".into(), kind: 0, text: "7".into() })));
        d.els.push(El::Code(Code::HRt));
        d.els.extend(Document::from_text("Kapitel").els);
        d.els.push(El::Code(Code::FlushRightDot));
        d.els.extend(Document::from_text("3").els);
        let bytes = write_docx(&d).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut cx = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/comments.xml").unwrap(), &mut cx).unwrap();
        assert!(cx.contains("Bitte prüfen"), "{cx}");
        let xml = document_xml(&d, &mut Parts::default());
        assert!(xml.contains("<w:commentReference w:id=\"0\"/>") && xml.contains("w:name=\"WPT_kap_1\"") && xml.contains("w:leader=\"dot\""), "{xml}");
        let d2 = via_xml(&d);
        assert!(d2.els.contains(&El::Code(Code::Comment("Bitte prüfen\nZeile 2".into()))), "{:?}", d2.els);
        assert!(d2.els.contains(&El::Code(Code::Mark(Mark::Target { name: "kap_1".into() }))));
        assert!(d2.els.contains(&El::Code(Code::FlushRightDot)));
        assert_eq!(d2.plain_text(0, d2.len()), "Text mit Kommentar Seite 7\nKapitel3");
    }

    fn columns_doc(kind: ColKind) -> Document {
        let mut d = Document::from_text("Intro\n");
        let cols = crate::columns::even_columns(2, 600, d.init.left, d.init.page_width - d.init.right);
        d.els.push(El::Code(Code::ColDef { kind, cols }));
        d.els.push(El::Code(Code::ColOn));
        d.els.extend(Document::from_text("A\nB\u{c}C\n").els);
        d.els.push(El::Code(Code::ColOff));
        d.els.extend("Outro".chars().map(El::Ch));
        d
    }

    #[test]
    fn newspaper_columns_via_xml() {
        let d = columns_doc(ColKind::Newspaper);
        let xml = document_xml(&d, &mut Parts::default());
        assert!(xml.contains("<w:cols w:num=\"2\" w:space=\"720\"/>"), "{xml}");
        assert!(xml.contains("<w:br w:type=\"column\"/>"));
        assert_eq!(xml.matches("<w:sectPr>").count(), 3, "{xml}");
        assert_eq!(xml.matches("<w:type w:val=\"continuous\"/>").count(), 2);
        let d2 = parse_document_xml(&xml, None);
        let strip = |v: &[El]| v.iter().filter(|e| !matches!(e, El::Code(Code::Just(_)))).cloned().collect::<Vec<_>>();
        assert_eq!(strip(&d2.els), strip(&d.els), "{:?}", d2.els);
        // and the native round trip keeps everything
        let d3 = read_docx(&write_docx(&d).unwrap()).unwrap();
        assert_eq!(d3.els, d.els);
    }

    #[test]
    fn parallel_columns_as_table() {
        let d = columns_doc(ColKind::ParallelProtect);
        let xml = document_xml(&d, &mut Parts::default());
        assert!(xml.contains("<w:tbl>") && xml.contains("<w:cantSplit/>") && xml.contains("w:val=\"none\""), "{xml}");
        assert!(!xml.contains("<w:cols"));
        let d2 = parse_document_xml(&xml, None);
        let t = d2.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).expect("table");
        let tb = crate::tables::parse(&d2.els, t).unwrap();
        assert_eq!(tb.rows.len(), 1);
        assert_eq!(tb.rows[0].len(), 2);
        let txt = d2.plain_text(0, d2.len());
        assert!(txt.starts_with("Intro") && txt.contains("A\nB") && txt.contains('C') && txt.ends_with("Outro"), "{txt:?}");
    }

    #[test]
    fn roundtrip_via_xml_only() {
        let d = sample();
        let xml = document_xml(&d, &mut Parts::default());
        let d2 = parse_document_xml(&xml, None);
        assert_eq!(d.plain_text(0, d.len()), d2.plain_text(0, d2.len()));
        assert_eq!(d2.els[0], El::Code(Code::AttrOn(Attr::Bold)));
    }
}

#[cfg(test)]
mod word_edit_tests {
    use super::*;
    use crate::doc::*;

    /// Save, then read back the way a file edited in Word is read (the
    /// private stream part no longer matches): text, attributes and tabs.
    fn without_stream(doc: &Document) -> Document {
        let bytes = write_docx(doc).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            if f.name().starts_with("wp51/") {
                continue;
            }
            let name = f.name().to_string();
            let mut buf = vec![];
            std::io::Read::read_to_end(&mut f, &mut buf).unwrap();
            out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut out, &buf).unwrap();
        }
        read_docx(&out.finish().unwrap().into_inner()).unwrap()
    }

    #[test]
    fn text_runs_and_tabs_survive_word() {
        let mut d = Document::from_text("Erster Absatz\tmit Tab.\n");
        d.els.push(El::Code(Code::AttrOn(Attr::Bold)));
        d.els.extend("fett".chars().map(El::Ch));
        d.els.push(El::Code(Code::AttrOff(Attr::Bold)));
        d.els.push(El::Code(Code::HRt));
        // a custom tab set: right tab at 4", decimal at 5.5"
        d.els.push(El::Code(Code::TabSet(TabSet {
            relative: true,
            stops: vec![
                TabStop { pos: 4800, kind: TabKind::Right, dot_leader: true },
                TabStop { pos: 6600, kind: TabKind::Decimal, dot_leader: false },
            ],
        })));
        d.els.extend("Preis\t12,50\n".chars().map(|c| if c == '\n' { El::Code(Code::HRt) } else if c == '\t' { El::Code(Code::Tab) } else { El::Ch(c) }));
        let r = without_stream(&d);
        assert_eq!(r.plain_text(0, r.len()).trim_end(), d.plain_text(0, d.len()).trim_end());
        assert!(r.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        let ts = r.els.iter().find_map(|e| match e { El::Code(Code::TabSet(t)) if t.stops.iter().any(|s| s.kind == TabKind::Right) => Some(t.clone()), _ => None });
        let ts = ts.expect("custom tab set comes back");
        assert!(ts.stops.iter().any(|s| s.kind == TabKind::Right && s.dot_leader));
        assert!(ts.stops.iter().any(|s| s.kind == TabKind::Decimal));
    }
}
