//! OpenDocument Text import and export: `.odt` (zip package), `.ott`
//! (template, same package) and `.fodt` (flat single-file XML).
//!
//! Writing mirrors docx.rs: the WP code stream is cut into paragraphs at
//! `[HRt]`/`[HPg]`, paragraph formatting becomes automatic paragraph styles
//! (`P1`, `P2`, … deduplicated), attributes/fonts become automatic text
//! styles (`T1`, …) used by `text:span`.  Headers/footers live in the master
//! page in styles.xml, so their automatic styles are written there with an
//! `M` prefix (which also keeps names unique in a flat .fodt).
//!
//! Reading builds a small DOM (namespace prefixes canonicalised), resolves
//! styles (default style → parent chain → style; spans nest) and produces a
//! balanced, normalized code stream like the .docx / RTF readers.


use crate::doc::*;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};

const MIME: &str = "application/vnd.oasis.opendocument.text";
const MIME_OTT: &str = "application/vnd.oasis.opendocument.text-template";

const NS: &[(&str, &str)] = &[
    ("office", "urn:oasis:names:tc:opendocument:xmlns:office:1.0"),
    ("style", "urn:oasis:names:tc:opendocument:xmlns:style:1.0"),
    ("text", "urn:oasis:names:tc:opendocument:xmlns:text:1.0"),
    ("table", "urn:oasis:names:tc:opendocument:xmlns:table:1.0"),
    ("draw", "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"),
    ("fo", "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"),
    ("xlink", "http://www.w3.org/1999/xlink"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("meta", "urn:oasis:names:tc:opendocument:xmlns:meta:1.0"),
    ("number", "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0"),
    ("svg", "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"),
];

fn ns_decls() -> String {
    NS.iter().map(|(p, u)| format!(" xmlns:{p}=\"{u}\"")).collect()
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        o.push_str(&esc_char(c));
    }
    o
}

fn esc_char(c: char) -> String {
    match c {
        '&' => "&amp;".into(),
        '<' => "&lt;".into(),
        '>' => "&gt;".into(),
        '"' => "&quot;".into(),
        c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => String::new(),
        '\u{fffe}' | '\u{ffff}' => String::new(),
        c => c.to_string(),
    }
}

/// WPU → "1.2500in".
fn inch(wpu: i32) -> String {
    format!("{:.4}in", wpu as f64 / WPU_PER_INCH as f64)
}

fn pt(cpt: u32) -> String {
    if cpt % 100 == 0 { format!("{}pt", cpt / 100) } else { format!("{}pt", cpt as f64 / 100.0) }
}

// ================================================================= write

#[derive(Clone)]
struct ParaFmt {
    just: Justify,
    left: i32,
    right: i32,
    spacing: u32,
    tabs: TabSet,
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

/// Codes that are pure formatting (not text-like) — as in docx.rs.
fn is_formatting(e: &El) -> bool {
    match e {
        El::Ch(_) => false,
        El::Code(c) => !matches!(
            c,
            Code::Tab | Code::Indent | Code::DblIndent | Code::MarRel | Code::HardSpace | Code::HardHyphen | Code::HRt | Code::HPg
        ),
    }
}

fn size_pct(a: AttrSet) -> u32 {
    if a.has(Attr::ExtraLarge) {
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
    }
}

/// `style:text-properties` attributes for an attribute set + font
/// (relative to the document's base font).  Empty = plain text.
fn text_props(a: AttrSet, font: &FontSpec, base: &FontSpec, fonts: &mut BTreeSet<String>) -> String {
    let mut s = String::new();
    if font.name != base.name && !font.name.is_empty() {
        fonts.insert(font.name.clone());
        let n = esc(&font.name);
        s.push_str(&format!(" style:font-name=\"{n}\" style:font-name-complex=\"{n}\""));
    }
    // Relative sizes (Fine…Extra Large) are written as absolute sizes like
    // docx.rs does — LibreOffice ignores percentages in automatic text
    // styles.  The reader maps exact 60/80/120/150/200 % of the paragraph
    // size in a span back to the attribute.
    let pct = size_pct(a);
    if font.size_cpt != base.size_cpt || pct != 100 {
        let v = pt(font.size_cpt * pct / 100);
        s.push_str(&format!(" fo:font-size=\"{v}\" style:font-size-complex=\"{v}\""));
    }
    if a.has(Attr::Bold) {
        s.push_str(" fo:font-weight=\"bold\" style:font-weight-complex=\"bold\"");
    }
    if a.has(Attr::Italic) {
        s.push_str(" fo:font-style=\"italic\" style:font-style-complex=\"italic\"");
    }
    if a.has(Attr::DblUnderline) {
        s.push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"double\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
    } else if a.has(Attr::Underline) {
        s.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
    }
    if a.has(Attr::Strikeout) {
        s.push_str(" style:text-line-through-style=\"solid\" style:text-line-through-type=\"single\"");
    }
    if a.has(Attr::SmallCaps) {
        s.push_str(" fo:font-variant=\"small-caps\"");
    }
    if a.has(Attr::Superscript) {
        s.push_str(" style:text-position=\"super 58%\"");
    } else if a.has(Attr::Subscript) {
        s.push_str(" style:text-position=\"sub 58%\"");
    }
    if a.has(Attr::Outline) {
        s.push_str(" style:text-outline=\"true\"");
    }
    if a.has(Attr::Shadow) {
        s.push_str(" fo:text-shadow=\"1pt 1pt\"");
    }
    if a.has(Attr::Redline) {
        s.push_str(" fo:color=\"#c00000\"");
    }
    s
}

/// Automatic-style registry (deduplicated by content).
#[derive(Default)]
struct Reg {
    pfx: &'static str,
    map: HashMap<String, String>,
    xml: String,
    n: HashMap<&'static str, usize>,
}

impl Reg {
    /// `rest` is everything after the family attribute, starting with
    /// further attributes or `>`.
    fn get(&mut self, family: &str, letter: &'static str, rest: &str) -> String {
        let key = format!("{family}|{rest}");
        if let Some(n) = self.map.get(&key) {
            return n.clone();
        }
        let c = self.n.entry(letter).or_insert(0);
        *c += 1;
        let name = format!("{}{}{}", self.pfx, letter, c);
        self.xml.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"{family}\"{rest}</style:style>"));
        self.map.insert(key, name.clone());
        name
    }
}

/// Paragraph output with span grouping and ODF whitespace rules.
struct POut {
    xml: String,
    span: Option<String>,
    buf: String,
    last_space: bool,
    s_run: usize,
    s_start: usize,
}

impl POut {
    fn new() -> Self {
        POut { xml: String::new(), span: None, buf: String::new(), last_space: true, s_run: 0, s_start: 0 }
    }
    fn flush(&mut self) {
        if !self.buf.is_empty() {
            match &self.span {
                Some(s) => self.xml.push_str(&format!("<text:span text:style-name=\"{s}\">{}</text:span>", self.buf)),
                None => self.xml.push_str(&self.buf),
            }
            self.buf.clear();
        }
        self.s_run = 0;
    }
    fn style(&mut self, s: Option<String>) {
        if self.span != s {
            self.flush();
            self.span = s;
        }
    }
    fn raw(&mut self, x: &str) {
        self.flush();
        self.xml.push_str(x);
        self.last_space = false;
    }
    fn ch(&mut self, c: char) {
        if c == ' ' {
            if self.last_space {
                self.s_run += 1;
                if self.s_run == 1 {
                    self.s_start = self.buf.len();
                }
                self.buf.truncate(self.s_start);
                if self.s_run == 1 {
                    self.buf.push_str("<text:s/>");
                } else {
                    self.buf.push_str(&format!("<text:s text:c=\"{}\"/>", self.s_run));
                }
            } else {
                self.buf.push(' ');
                self.last_space = true;
            }
        } else {
            self.buf.push_str(&esc_char(c));
            self.last_space = false;
            self.s_run = 0;
        }
    }
    fn tab(&mut self) {
        self.buf.push_str("<text:tab/>");
        self.last_space = false;
        self.s_run = 0;
    }
    fn finish(mut self) -> String {
        self.flush();
        self.xml
    }
}

struct St {
    attrs: AttrSet,
    font: FontSpec,
    pf: ParaFmt,
    rev: Option<(bool, String, String)>,
    /// open insertion region id
    ins: Option<String>,
}

struct W<'a> {
    doc: &'a Document,
    base: FontSpec,
    pf0: ParaFmt,
    reg: Reg,
    hreg: Reg,
    fonts: BTreeSet<String>,
    changes: String,
    nchange: usize,
    nfoot: usize,
    nend: usize,
    ntable: usize,
    hf: Vec<(u8, Occurrence, String)>,
}

fn change_info(author: &str, date: &str) -> String {
    let date = if date.is_empty() { "1970-01-01T00:00:00" } else { date };
    format!("<office:change-info><dc:creator>{}</dc:creator><dc:date>{}</dc:date></office:change-info>", esc(author), esc(date))
}

impl<'a> W<'a> {
    fn reg(&mut self, hdr: bool) -> &mut Reg {
        if hdr { &mut self.hreg } else { &mut self.reg }
    }

    fn tstyle(&mut self, st: &St, hdr: bool) -> Option<String> {
        let p = text_props(st.attrs, &st.font, &self.base, &mut self.fonts);
        if p.is_empty() { None } else { Some(self.reg(hdr).get("text", "T", &format!("><style:text-properties{p}/>"))) }
    }

    fn open_ins(&mut self, st: &mut St, out: &mut POut, author: &str, date: &str) {
        self.nchange += 1;
        let id = format!("ct{}", self.nchange);
        self.changes.push_str(&format!(
            "<text:changed-region xml:id=\"{id}\" text:id=\"{id}\"><text:insertion>{}</text:insertion></text:changed-region>",
            change_info(author, date)
        ));
        out.raw(&format!("<text:change-start text:change-id=\"{id}\"/>"));
        st.ins = Some(id);
    }

    #[allow(clippy::too_many_arguments)]
    fn deleted(&mut self, els: &[El], st: &mut St, out: &mut POut, hdr: bool, author: &str, date: &str) {
        let mut p = POut::new();
        let mut sk = false;
        self.inline(els, st, &mut p, hdr, true, &mut sk);
        let inner = p.finish();
        if hdr || inner.is_empty() {
            return;
        }
        self.nchange += 1;
        let id = format!("ct{}", self.nchange);
        self.changes.push_str(&format!(
            "<text:changed-region xml:id=\"{id}\" text:id=\"{id}\"><text:deletion>{}<text:p>{inner}</text:p></text:deletion></text:changed-region>",
            change_info(author, date)
        ));
        out.raw(&format!("<text:change text:change-id=\"{id}\"/>"));
    }

    /// Paragraph content (no paragraph-breaking codes in `els`).
    fn inline(&mut self, els: &[El], st: &mut St, out: &mut POut, hdr: bool, del: bool, skipping: &mut bool) {
        let mut k = 0;
        while k < els.len() {
            match &els[k] {
                El::Ch(c) => {
                    *skipping = false;
                    let s = self.tstyle(st, hdr);
                    out.style(s);
                    match c {
                        '\t' => out.tab(),
                        '\n' | '\r' => out.ch(' '),
                        c => out.ch(*c),
                    }
                }
                El::Code(code) => match code {
                    Code::Indent | Code::DblIndent | Code::MarRel if *skipping => {}
                    Code::Tab | Code::Indent | Code::DblIndent => {
                        *skipping = false;
                        let s = self.tstyle(st, hdr);
                        out.style(s);
                        out.tab();
                    }
                    Code::MarRel => {}
                    Code::HardSpace | Code::HardHyphen | Code::SoftHyphen => {
                        *skipping = false;
                        let s = self.tstyle(st, hdr);
                        out.style(s);
                        out.ch(match code {
                            Code::HardSpace => '\u{a0}',
                            Code::HardHyphen => '\u{2011}',
                            _ => '\u{ad}',
                        });
                    }
                    Code::Note { kind, els: nels } if !hdr && !del => {
                        let inner = self.body(nels, false);
                        let (class, num, pre) = if *kind == NoteKind::Foot {
                            self.nfoot += 1;
                            ("footnote", self.nfoot, "ftn")
                        } else {
                            self.nend += 1;
                            ("endnote", self.nend, "edn")
                        };
                        out.raw(&format!(
                            "<text:note text:id=\"{pre}{num}\" text:note-class=\"{class}\"><text:note-citation>{num}</text:note-citation><text:note-body>{inner}</text:note-body></text:note>"
                        ));
                    }
                    Code::HeadFoot { which, occ, els: hels } if !hdr && !del => {
                        if *occ != Occurrence::Discontinue
                            && !self.hf.iter().any(|(w, o, _)| *w == *which || (*w < 2) == (*which < 2) && *o == *occ)
                        {
                            let x = self.body(hels, true);
                            self.hf.push((*which, *occ, x));
                        }
                    }
                    Code::Comment(s) if !hdr && !del => {
                        let mut x = String::from("<office:annotation>");
                        if !self.doc.summary.author.is_empty() {
                            x.push_str(&format!("<dc:creator>{}</dc:creator>", esc(&self.doc.summary.author)));
                        }
                        for line in s.split('\n') {
                            let mut p = POut::new();
                            for c in line.chars() {
                                if c == '\t' { p.tab() } else { p.ch(c) }
                            }
                            x.push_str(&format!("<text:p>{}</text:p>", p.finish()));
                        }
                        x.push_str("</office:annotation>");
                        out.raw(&x);
                    }
                    Code::RevOn { del: true, author, date } if !del => {
                        let e = (k + 1..els.len()).find(|&x| matches!(els[x], El::Code(Code::RevOff { del: true }))).unwrap_or(els.len());
                        st.rev = if e < els.len() { None } else { Some((true, author.clone(), date.clone())) };
                        if let Some(id) = st.ins.take() {
                            out.raw(&format!("<text:change-end text:change-id=\"{id}\"/>"));
                        }
                        self.deleted(&els[k + 1..e], st, out, hdr, author, date);
                        k = e + 1;
                        continue;
                    }
                    Code::RevOn { del: false, author, date } if !del => {
                        if !hdr {
                            if let Some(id) = st.ins.take() {
                                out.raw(&format!("<text:change-end text:change-id=\"{id}\"/>"));
                            }
                            self.open_ins(st, out, author, date);
                        }
                        st.rev = Some((false, author.clone(), date.clone()));
                    }
                    Code::RevOff { del: false } if !del => {
                        if let Some(id) = st.ins.take() {
                            out.raw(&format!("<text:change-end text:change-id=\"{id}\"/>"));
                        }
                        st.rev = None;
                    }
                    Code::AttrOn(a) => st.attrs.set(*a, true),
                    Code::AttrOff(a) => st.attrs.set(*a, false),
                    other => apply_para(&mut st.pf, &mut st.font, other),
                },
            }
            k += 1;
        }
    }

    /// Paragraphs and tables for a list of elements.
    fn body(&mut self, els: &[El], hdr: bool) -> String {
        let mut pf = self.pf0.clone();
        self.body_pf(els, hdr, &mut pf)
    }

    /// Like `body`, starting from (and updating) paragraph format `pf` —
    /// table cells continue the surrounding format like in WP.
    fn body_pf(&mut self, els: &[El], hdr: bool, pf: &mut ParaFmt) -> String {
        let doc = self.doc;
        let init = &doc.init;
        let mut st = St { attrs: AttrSet::default(), font: self.base.clone(), pf: pf.clone(), rev: None, ins: None };
        let mut x = String::new();
        let n = els.len();
        let mut i = 0;
        let mut brk = false;
        loop {
            if i < n {
                if let El::Code(Code::TblDef { .. }) = els[i] {
                    if let Some(t) = crate::tables::parse(els, i) {
                        x.push_str(&self.table(els, &t, hdr, brk, &mut st.pf));
                        brk = false;
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
            let mut j = i;
            while j < n && !matches!(els[j], El::Code(Code::HRt | Code::HPg | Code::TblDef { .. })) {
                j += 1;
            }
            let ends_at_table = j < n && matches!(els[j], El::Code(Code::TblDef { .. }));
            let p = &els[i..j];
            // leading format codes → paragraph properties
            let mut para_just = None;
            let mut k = 0;
            while k < p.len() && p[k].code().is_some() && !matches!(p[k], El::Code(Code::Center | Code::FlushRight)) {
                if let El::Code(c) = &p[k] {
                    apply_para(&mut st.pf, &mut st.font, c);
                }
                k += 1;
            }
            if k < p.len() {
                match p[k] {
                    El::Code(Code::Center) => para_just = Some(Justify::Center),
                    El::Code(Code::FlushRight) => para_just = Some(Justify::Right),
                    _ => {}
                }
            }
            let align = match para_just.unwrap_or(st.pf.just) {
                Justify::Left => "start",
                Justify::Center => "center",
                Justify::Right => "end",
                Justify::Full => "justify",
            };
            // indents (as docx.rs)
            let mut ml = st.pf.left - init.left;
            let mut mr = st.pf.right - init.right;
            let mut hanging = 0;
            {
                let mut k = 0;
                while k < p.len() && is_formatting(&p[k]) {
                    k += 1;
                }
                let mut indents = 0;
                let mut dbl = false;
                while k < p.len() {
                    match p[k] {
                        El::Code(Code::Indent) => indents += 1,
                        El::Code(Code::DblIndent) => {
                            indents += 1;
                            dbl = true
                        }
                        El::Code(Code::MarRel) if indents > 0 => hanging += 600,
                        _ => break,
                    }
                    k += 1;
                }
                if indents > 0 {
                    ml += indents * 600;
                    if dbl {
                        mr += indents * 600;
                    }
                }
                if k > 0 && matches!(p[k - 1], El::Code(Code::MarRel)) && indents == 0 {
                    hanging = 600;
                }
            }
            let mut pp = format!(
                "<style:paragraph-properties fo:text-align=\"{align}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\" fo:text-indent=\"{}\" fo:margin-top=\"0in\" fo:margin-bottom=\"0in\"",
                inch(ml),
                inch(mr),
                inch(-hanging)
            );
            if st.pf.spacing != 100 {
                pp.push_str(&format!(" fo:line-height=\"{}%\"", st.pf.spacing));
            }
            if brk {
                pp.push_str(" fo:break-before=\"page\"");
            }
            if st.pf.tabs != TabSet::default_tabs() {
                let mut t = String::new();
                for s in &st.pf.tabs.stops {
                    let abs = if st.pf.tabs.relative { st.pf.left + s.pos } else { s.pos };
                    let pos = abs - init.left - ml;
                    if pos <= 0 {
                        continue;
                    }
                    let ty = match s.kind {
                        TabKind::Left => "",
                        TabKind::Center => " style:type=\"center\"",
                        TabKind::Right => " style:type=\"right\"",
                        TabKind::Decimal => " style:type=\"char\" style:char=\".\"",
                    };
                    let lead = if s.dot_leader { " style:leader-style=\"dotted\" style:leader-text=\".\"" } else { "" };
                    t.push_str(&format!("<style:tab-stop style:position=\"{}\"{ty}{lead}/>", inch(pos)));
                }
                pp.push_str(&format!("><style:tab-stops>{t}</style:tab-stops></style:paragraph-properties>"));
            } else {
                pp.push_str("/>");
            }
            let pname = self.reg(hdr).get("paragraph", "P", &format!(" style:parent-style-name=\"Standard\">{pp}"));

            let mut out = POut::new();
            let mut start = 0;
            match st.rev.clone() {
                Some((true, a, d)) => {
                    // a deletion running on from the previous paragraph
                    let e = p.iter().position(|e| matches!(e, El::Code(Code::RevOff { del: true }))).unwrap_or(p.len());
                    if e < p.len() {
                        st.rev = None;
                    }
                    self.deleted(&p[..e], &mut st, &mut out, hdr, &a, &d);
                    start = (e + 1).min(p.len());
                }
                Some((false, a, d)) if !hdr => self.open_ins(&mut st, &mut out, &a, &d),
                _ => {}
            }
            let mut skipping = true;
            self.inline(&p[start..], &mut st, &mut out, hdr, false, &mut skipping);
            if let Some(id) = st.ins.take() {
                out.raw(&format!("<text:change-end text:change-id=\"{id}\"/>"));
            }
            x.push_str(&format!("<text:p text:style-name=\"{pname}\">{}</text:p>", out.finish()));
            brk = j < n && matches!(els[j], El::Code(Code::HPg));
            if j >= n {
                break;
            }
            i = if ends_at_table { j } else { j + 1 };
        }
        *pf = st.pf;
        x
    }

    fn table(&mut self, els: &[El], t: &crate::tables::Table, hdr: bool, brk: bool, pf: &mut ParaFmt) -> String {
        self.ntable += 1;
        let ncols = t.rows.iter().map(|r| r.len()).max().unwrap_or(0).max(t.cols.len()).max(1);
        let widths: Vec<i32> = (0..ncols).map(|c| t.cols.get(c).copied().unwrap_or(1200).max(60)).collect();
        let total: i32 = widths.iter().sum();
        let brk = if brk { " fo:break-before=\"page\"" } else { "" };
        let ts = self.reg(hdr).get("table", "Tbl", &format!("><style:table-properties style:width=\"{}\" table:align=\"left\"{brk}/>", inch(total)));
        let mut x = format!("<table:table table:name=\"Table{}\" table:style-name=\"{ts}\">", self.ntable);
        for w in &widths {
            let cs = self.reg(hdr).get("table-column", "Col", &format!("><style:table-column-properties style:column-width=\"{}\"/>", inch(*w)));
            x.push_str(&format!("<table:table-column table:style-name=\"{cs}\"/>"));
        }
        let cell = self.reg(hdr).get("table-cell", "Cell", "><style:table-cell-properties fo:padding=\"0.04in\" fo:border=\"0.5pt solid #000000\"/>");
        let empty_row = vec![];
        let rows: Vec<&Vec<usize>> = if t.rows.is_empty() { vec![&empty_row] } else { t.rows.iter().collect() };
        for (r, row) in rows.iter().enumerate() {
            x.push_str("<table:table-row>");
            for c in 0..ncols {
                let inner = if c < row.len() {
                    let (a, b) = t.cell_range(els, r, c);
                    self.body_pf(&els[a..b], hdr, pf)
                } else {
                    String::new()
                };
                let inner = if inner.is_empty() { "<text:p/>".to_string() } else { inner };
                x.push_str(&format!("<table:table-cell table:style-name=\"{cell}\" office:value-type=\"string\">{inner}</table:table-cell>"));
            }
            x.push_str("</table:table-row>");
        }
        x.push_str("</table:table>");
        x
    }
}

/// All the XML pieces of a document, assembled into a package or a flat file.
struct Pieces {
    font_decls: String,
    common: String,
    auto_styles: String,
    master: String,
    auto_content: String,
    text: String,
    meta: String,
}

fn build(doc: &Document) -> Pieces {
    let init = &doc.init;
    let mut pf0 = ParaFmt { just: init.justify, left: init.left, right: init.right, spacing: init.line_spacing, tabs: init.tabs.clone() };
    let mut base = init.font.clone();
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            apply_para(&mut pf0, &mut base, c);
        }
    }
    let mut w = W {
        doc,
        base: base.clone(),
        pf0,
        reg: Reg::default(),
        hreg: Reg { pfx: "M", ..Default::default() },
        fonts: BTreeSet::new(),
        changes: String::new(),
        nchange: 0,
        nfoot: 0,
        nend: 0,
        ntable: 0,
        hf: vec![],
    };
    let body = w.body(&doc.els, false);
    w.fonts.insert(base.name.clone());

    // master page with headers / footers
    let mut hd: [Option<String>; 2] = [None, None];
    let mut ft: [Option<String>; 2] = [None, None];
    let (mut odd_h, mut odd_f) = (false, false);
    for (which, occ, x) in &w.hf {
        let slot = if *occ == Occurrence::Even { 1 } else { 0 };
        let x = if x.is_empty() { "<text:p/>".to_string() } else { x.clone() };
        if *which < 2 {
            hd[slot] = Some(x);
            odd_h |= *occ == Occurrence::Odd;
        } else {
            ft[slot] = Some(x);
            odd_f |= *occ == Occurrence::Odd;
        }
    }
    let mut master = String::from("<style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">");
    for (tag, arr, odd) in [("header", &hd, odd_h), ("footer", &ft, odd_f)] {
        if arr[0].is_some() || arr[1].is_some() {
            master.push_str(&format!("<style:{tag}>{}</style:{tag}>", arr[0].as_deref().unwrap_or("<text:p/>")));
            if arr[1].is_some() || odd {
                master.push_str(&format!("<style:{tag}-left>{}</style:{tag}-left>", arr[1].as_deref().unwrap_or("<text:p/>")));
            }
        }
    }
    master.push_str("</style:master-page>");
    let has_h = hd.iter().any(|x| x.is_some());
    let has_f = ft.iter().any(|x| x.is_some());
    let orient = if init.page_width > init.page_height { "landscape" } else { "portrait" };
    let mut auto_styles = format!(
        "<style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\" style:print-orientation=\"{orient}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\" style:writing-mode=\"lr-tb\"/><style:header-style>{}</style:header-style><style:footer-style>{}</style:footer-style></style:page-layout>",
        inch(init.page_width),
        inch(init.page_height),
        inch(init.top),
        inch(init.bottom),
        inch(init.left),
        inch(init.right),
        if has_h { "<style:header-footer-properties fo:min-height=\"0in\" fo:margin-bottom=\"0.1667in\"/>" } else { "" },
        if has_f { "<style:header-footer-properties fo:min-height=\"0in\" fo:margin-top=\"0.1667in\"/>" } else { "" },
    );
    auto_styles.push_str(&w.hreg.xml);

    let f = esc(&base.name);
    let sz = pt(base.size_cpt);
    let common = format!(
        "<style:default-style style:family=\"paragraph\"><style:paragraph-properties style:tab-stop-distance=\"0.5in\" style:writing-mode=\"page\"/><style:text-properties style:font-name=\"{f}\" fo:font-size=\"{sz}\" fo:language=\"de\" fo:country=\"DE\" style:font-name-asian=\"{f}\" style:font-size-asian=\"{sz}\" style:font-name-complex=\"{f}\" style:font-size-complex=\"{sz}\"/></style:default-style><style:style style:name=\"Standard\" style:family=\"paragraph\" style:class=\"text\"/>"
    );
    let mut font_decls = String::from("<office:font-face-decls>");
    for name in &w.fonts {
        let n = esc(name);
        font_decls.push_str(&format!("<style:font-face style:name=\"{n}\" svg:font-family=\"'{n}'\"/>"));
    }
    font_decls.push_str("</office:font-face-decls>");

    let mut text = String::new();
    if doc.track || !w.changes.is_empty() {
        text.push_str(&format!("<text:tracked-changes text:track-changes=\"{}\">{}</text:tracked-changes>", doc.track, w.changes));
    }
    text.push_str(&body);

    let s = &doc.summary;
    let mut meta = String::from("<meta:generator>Blue Screen Writer 51</meta:generator>");
    for (tag, v) in [("dc:title", &s.descriptive_name), ("dc:subject", &s.subject), ("dc:description", &s.comments), ("meta:initial-creator", &s.author)] {
        if !v.is_empty() {
            meta.push_str(&format!("<{tag}>{}</{tag}>", esc(v)));
        }
    }
    let creator = if s.typist.is_empty() { &s.author } else { &s.typist };
    if !creator.is_empty() {
        meta.push_str(&format!("<dc:creator>{}</dc:creator>", esc(creator)));
    }
    Pieces { font_decls, common, auto_styles, master, auto_content: w.reg.xml, text, meta }
}

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

fn content_xml(p: &Pieces) -> String {
    format!(
        "{XML_HEAD}<office:document-content{} office:version=\"1.3\">{}<office:automatic-styles>{}</office:automatic-styles><office:body><office:text>{}</office:text></office:body></office:document-content>",
        ns_decls(),
        p.font_decls,
        p.auto_content,
        p.text
    )
}

fn styles_xml(p: &Pieces) -> String {
    format!(
        "{XML_HEAD}<office:document-styles{} office:version=\"1.3\">{}<office:styles>{}</office:styles><office:automatic-styles>{}</office:automatic-styles><office:master-styles>{}</office:master-styles></office:document-styles>",
        ns_decls(),
        p.font_decls,
        p.common,
        p.auto_styles,
        p.master
    )
}

fn meta_xml(p: &Pieces) -> String {
    format!("{XML_HEAD}<office:document-meta{} office:version=\"1.3\"><office:meta>{}</office:meta></office:document-meta>", ns_decls(), p.meta)
}

fn write_package(doc: &Document, mime: &str) -> std::io::Result<Vec<u8>> {
    let p = build(doc);
    let manifest = format!(
        "{XML_HEAD}<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.3\" manifest:media-type=\"{mime}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/></manifest:manifest>"
    );
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file("mimetype", stored).map_err(std::io::Error::other)?;
        z.write_all(mime.as_bytes())?;
        for (name, data) in [
            ("META-INF/manifest.xml", manifest),
            ("content.xml", content_xml(&p)),
            ("styles.xml", styles_xml(&p)),
            ("meta.xml", meta_xml(&p)),
        ] {
            z.start_file(name, deflated).map_err(std::io::Error::other)?;
            z.write_all(data.as_bytes())?;
        }
        z.finish().map_err(std::io::Error::other)?;
    }
    Ok(buf.into_inner())
}

/// Write an OpenDocument Text package (.odt).
pub fn write_odt(doc: &Document) -> std::io::Result<Vec<u8>> {
    write_package(doc, MIME)
}

/// Write an OpenDocument Text template (.ott).
pub fn write_ott(doc: &Document) -> std::io::Result<Vec<u8>> {
    write_package(doc, MIME_OTT)
}

/// Write a flat OpenDocument Text file (.fodt).
pub fn write_fodt(doc: &Document) -> String {
    let p = build(doc);
    format!(
        "{XML_HEAD}<office:document{} office:version=\"1.3\" office:mimetype=\"{MIME}\"><office:meta>{}</office:meta>{}<office:styles>{}</office:styles><office:automatic-styles>{}{}</office:automatic-styles><office:master-styles>{}</office:master-styles><office:body><office:text>{}</office:text></office:body></office:document>",
        ns_decls(),
        p.meta,
        p.font_decls,
        p.common,
        p.auto_styles,
        p.auto_content,
        p.master,
        p.text
    )
}

// ================================================================= detect

fn package_mime(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 30 || &bytes[..4] != b"PK\x03\x04" {
        return None;
    }
    let u16at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]) as usize;
    let nl = u16at(26);
    let xl = u16at(28);
    if bytes.get(30..30 + nl) == Some(b"mimetype".as_slice()) && u16at(8) == 0 {
        let size = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as usize;
        let s = 30 + nl + xl;
        if let Some(d) = bytes.get(s..s.saturating_add(size.min(200))) {
            if let Ok(m) = std::str::from_utf8(d) {
                return Some(m.trim().to_string());
            }
        }
    }
    // not stored first: look inside the zip
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    z.by_name("content.xml").ok()?;
    let f = z.by_name("mimetype").ok()?;
    let mut s = String::new();
    f.take(200).read_to_string(&mut s).ok()?;
    Some(s.trim().to_string())
}

/// An OpenDocument Text package (.odt, or the .ott template).
pub fn is_odt(bytes: &[u8]) -> bool {
    matches!(package_mime(bytes).as_deref(), Some(MIME) | Some(MIME_OTT))
}

/// An OpenDocument Text template package (.ott).
pub fn is_ott(bytes: &[u8]) -> bool {
    package_mime(bytes).as_deref() == Some(MIME_OTT)
}

/// A flat OpenDocument Text XML file (.fodt).
pub fn is_fodt(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let h = head.trim_start_matches('\u{feff}').trim_start();
    if !h.starts_with('<') {
        return false;
    }
    let mut found = false;
    let mut i = 0;
    while let Some(k) = h[i..].find("<office:document") {
        let after = h[i + k + 16..].chars().next();
        if matches!(after, Some(c) if c.is_whitespace() || c == '>') {
            found = true;
            break;
        }
        i += k + 16;
    }
    found && (h.contains(&format!("\"{MIME}\"")) || h.contains(&format!("'{MIME}'")) || h.contains(&format!("\"{MIME_OTT}\"")))
}

// ================================================================= DOM

#[derive(Clone, Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    kids: Vec<Kid>,
}

#[derive(Clone, Debug)]
enum Kid {
    N(Node),
    T(String),
}

impl Node {
    fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.kids.iter().filter_map(|k| if let Kid::N(n) = k { Some(n) } else { None })
    }
    fn child(&self, name: &str) -> Option<&Node> {
        self.nodes().find(|n| n.name == name)
    }
    fn path(&self, p: &[&str]) -> Option<&Node> {
        let mut n = self;
        for s in p {
            n = n.child(s)?;
        }
        Some(n)
    }
}

const MAX_DEPTH: usize = 120;

fn qname(raw: &[u8], map: &HashMap<String, String>) -> String {
    let s = String::from_utf8_lossy(raw);
    let (pfx, local) = match s.find(':') {
        Some(p) => (&s[..p], &s[p + 1..]),
        None => ("", &s[..]),
    };
    if pfx == "xmlns" || s == "xmlns" {
        return s.into_owned();
    }
    match map.get(pfx) {
        Some(c) => format!("{c}:{local}"),
        None => s.into_owned(),
    }
}

fn make_node(e: &quick_xml::events::BytesStart, map: &mut HashMap<String, String>, root: bool) -> Node {
    let raw: Vec<(Vec<u8>, String)> = e
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|a| {
            let v = a.unescape_value().map(|v| v.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
            (a.key.as_ref().to_vec(), v)
        })
        .collect();
    if root {
        for (k, v) in &raw {
            let k = String::from_utf8_lossy(k);
            let pfx = if k == "xmlns" { Some("") } else { k.strip_prefix("xmlns:") };
            if let Some(p) = pfx {
                if let Some((c, _)) = NS.iter().find(|(_, u)| u == v) {
                    map.insert(p.to_string(), c.to_string());
                }
            }
        }
    }
    Node { name: qname(e.name().as_ref(), map), attrs: raw.iter().map(|(k, v)| (qname(k, map), v.clone())).collect(), kids: vec![] }
}

/// Parse XML into a tree (never fails; stops at the first error).
fn parse_dom(xml: &str) -> Node {
    let mut r = Reader::from_str(xml);
    let mut stack: Vec<Node> = vec![Node { name: "#doc".into(), ..Default::default() }];
    let mut map: HashMap<String, String> = HashMap::new();
    let mut seen_root = false;
    let mut skip = 0usize;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => {
                if skip > 0 || stack.len() > MAX_DEPTH {
                    skip += 1;
                    continue;
                }
                let n = make_node(&e, &mut map, !seen_root);
                seen_root = true;
                stack.push(n);
            }
            Ok(Event::Empty(e)) => {
                if skip > 0 || stack.len() > MAX_DEPTH {
                    continue;
                }
                let n = make_node(&e, &mut map, !seen_root);
                seen_root = true;
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Kid::N(n));
                }
            }
            Ok(Event::End(_)) => {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                if stack.len() > 1 {
                    let n = stack.pop().unwrap_or_default();
                    if let Some(top) = stack.last_mut() {
                        top.kids.push(Kid::N(n));
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 && stack.len() > 1 {
                    let s = t.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                    if let Some(top) = stack.last_mut() {
                        top.kids.push(Kid::T(s));
                    }
                }
            }
            Ok(Event::CData(c)) => {
                if skip == 0 && stack.len() > 1 {
                    let s = String::from_utf8_lossy(&c).into_owned();
                    if let Some(top) = stack.last_mut() {
                        top.kids.push(Kid::T(s));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let n = stack.pop().unwrap_or_default();
        if let Some(top) = stack.last_mut() {
            top.kids.push(Kid::N(n));
        }
    }
    stack.pop().unwrap_or_default()
}

/// The document element (first element child of the synthetic root).
fn doc_elem(n: &Node) -> Option<&Node> {
    n.nodes().next()
}

// ================================================================= styles

fn len_in(v: &str) -> Option<f64> {
    let v = v.trim();
    let idx = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(v.len());
    let num: f64 = v[..idx].parse().ok()?;
    if !num.is_finite() {
        return None;
    }
    let f = match v[idx..].trim() {
        "in" | "inch" => 1.0,
        "cm" => 1.0 / 2.54,
        "mm" => 1.0 / 25.4,
        "pt" => 1.0 / 72.0,
        "pc" => 1.0 / 6.0,
        "px" => 1.0 / 96.0,
        _ => return None,
    };
    let x = num * f;
    if x.abs() > 1000.0 { None } else { Some(x) }
}

fn len_wpu(v: &str) -> Option<i32> {
    len_in(v).map(|x| (x * WPU_PER_INCH as f64).round() as i32)
}

fn pct_of(v: &str) -> Option<f64> {
    let n: f64 = v.trim().strip_suffix('%')?.trim().parse().ok()?;
    if n.is_finite() && n.abs() < 10000.0 { Some(n) } else { None }
}

fn unquote(s: &str) -> String {
    s.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '\'' || c == '"').to_string()
}

#[derive(Clone, Default)]
struct SInfo {
    parent: Option<String>,
    ppr: Vec<(String, String)>,
    tabs: Option<Vec<TabStop>>,
    tpr: Vec<(String, String)>,
    col_w: Option<i32>,
    tbl_brk: bool,
}

fn sinfo(n: &Node) -> SInfo {
    let mut s = SInfo { parent: n.attr("style:parent-style-name").map(String::from), ..Default::default() };
    for k in n.nodes() {
        match k.name.as_str() {
            "style:paragraph-properties" => {
                s.ppr = k.attrs.clone();
                if let Some(ts) = k.child("style:tab-stops") {
                    let mut v = vec![];
                    for t in ts.nodes().filter(|t| t.name == "style:tab-stop") {
                        let Some(pos) = t.attr("style:position").and_then(len_wpu) else { continue };
                        let kind = match t.attr("style:type") {
                            Some("center") => TabKind::Center,
                            Some("right") => TabKind::Right,
                            Some("char") => TabKind::Decimal,
                            _ => TabKind::Left,
                        };
                        let lt = t.attr("style:leader-text").unwrap_or("");
                        let ls = t.attr("style:leader-style").unwrap_or("none");
                        let dot_leader = (!lt.trim().is_empty()) || (ls != "none" && !ls.is_empty());
                        v.push(TabStop { pos, kind, dot_leader });
                    }
                    if !v.is_empty() {
                        s.tabs = Some(v);
                    }
                }
            }
            "style:text-properties" => s.tpr = k.attrs.clone(),
            "style:table-column-properties" => s.col_w = k.attr("style:column-width").and_then(len_wpu),
            "style:table-properties" => s.tbl_brk = k.attr("fo:break-before") == Some("page"),
            _ => {}
        }
    }
    s
}

#[derive(Clone)]
struct ListLevel {
    level: usize,
    number: bool,
    bullet: String,
    prefix: String,
    suffix: String,
}

#[derive(Default)]
struct Styles {
    fonts: HashMap<String, String>,
    common: HashMap<String, SInfo>,
    auto_c: HashMap<String, SInfo>,
    auto_s: HashMap<String, SInfo>,
    defaults: HashMap<String, SInfo>,
    lists: HashMap<String, Vec<ListLevel>>,
    layouts: HashMap<String, Vec<(String, String)>>,
    masters: Vec<Node>,
}

#[derive(Clone, PartialEq, Debug)]
struct TS {
    attrs: AttrSet,
    font: FontSpec,
}

#[derive(Clone, Default)]
struct PP {
    align: Option<Justify>,
    ml: i32,
    mr: i32,
    ti: i32,
    lh: Option<u32>,
    bb: bool,
    ba: bool,
    tabs: Option<Vec<TabStop>>,
}

fn list_levels(n: &Node) -> Vec<ListLevel> {
    let mut v = vec![];
    for l in n.nodes() {
        let level = l.attr("text:level").and_then(|x| x.parse().ok()).unwrap_or(1);
        match l.name.as_str() {
            "text:list-level-style-number" => v.push(ListLevel {
                level,
                number: l.attr("style:num-format").is_some_and(|f| !f.is_empty()),
                bullet: String::new(),
                prefix: l.attr("style:num-prefix").unwrap_or("").to_string(),
                suffix: l.attr("style:num-suffix").unwrap_or("").to_string(),
            }),
            "text:list-level-style-bullet" | "text:list-level-style-image" => {
                let b = l.attr("text:bullet-char").and_then(|b| b.chars().next()).unwrap_or('•');
                let b = if ('\u{e000}'..='\u{f8ff}').contains(&b) || b.is_control() { '•' } else { b };
                v.push(ListLevel { level, number: false, bullet: b.to_string(), prefix: String::new(), suffix: String::new() })
            }
            _ => {}
        }
    }
    v
}

impl Styles {
    /// Load the style sections of a document element.  `content`: its
    /// automatic styles belong to content.xml (else styles.xml).
    fn load(&mut self, root: &Node, content: bool) {
        for sec in root.nodes() {
            match sec.name.as_str() {
                "office:font-face-decls" => {
                    for f in sec.nodes().filter(|f| f.name == "style:font-face") {
                        if let Some(n) = f.attr("style:name") {
                            let fam = f.attr("svg:font-family").map(unquote).filter(|s| !s.is_empty()).unwrap_or_else(|| n.to_string());
                            self.fonts.insert(n.to_string(), fam);
                        }
                    }
                }
                "office:styles" if !content => {
                    for s in sec.nodes() {
                        match s.name.as_str() {
                            "style:style" => {
                                if let (Some(n), Some(f)) = (s.attr("style:name"), s.attr("style:family")) {
                                    self.common.insert(format!("{f}|{n}"), sinfo(s));
                                }
                            }
                            "style:default-style" => {
                                if let Some(f) = s.attr("style:family") {
                                    self.defaults.insert(f.to_string(), sinfo(s));
                                }
                            }
                            "text:list-style" => {
                                if let Some(n) = s.attr("style:name") {
                                    self.lists.insert(n.to_string(), list_levels(s));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "office:automatic-styles" => {
                    for s in sec.nodes() {
                        match s.name.as_str() {
                            "style:style" => {
                                if let (Some(n), Some(f)) = (s.attr("style:name"), s.attr("style:family")) {
                                    let m = if content { &mut self.auto_c } else { &mut self.auto_s };
                                    m.insert(format!("{f}|{n}"), sinfo(s));
                                }
                            }
                            "style:page-layout" if !content => {
                                if let Some(n) = s.attr("style:name") {
                                    let a = s.child("style:page-layout-properties").map(|p| p.attrs.clone()).unwrap_or_default();
                                    self.layouts.insert(n.to_string(), a);
                                }
                            }
                            "text:list-style" => {
                                if let Some(n) = s.attr("style:name") {
                                    self.lists.insert(n.to_string(), list_levels(s));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "office:master-styles" if !content => {
                    for m in sec.nodes().filter(|m| m.name == "style:master-page") {
                        self.masters.push(m.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn get(&self, fam: &str, name: &str, hdr: bool) -> Option<&SInfo> {
        let key = format!("{fam}|{name}");
        let auto = if hdr { &self.auto_s } else { &self.auto_c };
        auto.get(&key).or_else(|| self.common.get(&key))
    }

    /// Style chain root-first (without the default style).
    fn chain(&self, fam: &str, name: Option<&str>, hdr: bool) -> Vec<&SInfo> {
        let mut v = vec![];
        let mut cur = name.map(String::from);
        while let Some(n) = cur {
            if v.len() > 16 {
                break;
            }
            match self.get(fam, &n, hdr) {
                Some(s) => {
                    cur = s.parent.clone();
                    v.push(s);
                }
                None => break,
            }
        }
        v.reverse();
        v
    }

    fn apply_tpr(&self, ts: &mut TS, tpr: &[(String, String)], span: bool) {
        let get = |k: &str| tpr.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        if let Some(n) = get("style:font-name") {
            ts.font.name = self.fonts.get(n).cloned().unwrap_or_else(|| n.to_string());
        } else if let Some(f) = get("fo:font-family") {
            let f = unquote(f);
            if !f.is_empty() {
                ts.font.name = f;
            }
        }
        const SIZES: [Attr; 5] = [Attr::Fine, Attr::Small, Attr::Large, Attr::VeryLarge, Attr::ExtraLarge];
        if let Some(v) = get("fo:font-size") {
            if let Some(p) = pct_of(v) {
                let rel = match p.round() as i64 {
                    60 => Some(Attr::Fine),
                    80 => Some(Attr::Small),
                    120 => Some(Attr::Large),
                    150 => Some(Attr::VeryLarge),
                    200 => Some(Attr::ExtraLarge),
                    _ => None,
                };
                if let Some(a) = rel {
                    for s in SIZES {
                        ts.attrs.set(s, false);
                    }
                    ts.attrs.set(a, true);
                } else if p > 0.0 {
                    ts.font.size_cpt = ((ts.font.size_cpt as f64 * p / 100.0).round() as u32).clamp(100, 100_000);
                }
            } else if let Some(x) = len_in(v) {
                if x > 0.0 {
                    let new = ((x * 7200.0).round() as u32).clamp(100, 100_000);
                    for s in SIZES {
                        ts.attrs.set(s, false);
                    }
                    // in a span: exactly 60/80/120/150/200 % of the
                    // paragraph size = a WP relative size attribute
                    let rel = [(60, Attr::Fine), (80, Attr::Small), (120, Attr::Large), (150, Attr::VeryLarge), (200, Attr::ExtraLarge)]
                        .into_iter()
                        .find(|(p, _)| span && new == ts.font.size_cpt * p / 100);
                    match rel {
                        Some((_, a)) => ts.attrs.set(a, true),
                        None => ts.font.size_cpt = new,
                    }
                }
            }
        }
        if let Some(v) = get("fo:font-weight") {
            ts.attrs.set(Attr::Bold, v == "bold" || v.parse::<u32>().is_ok_and(|n| n >= 600));
        }
        if let Some(v) = get("fo:font-style") {
            ts.attrs.set(Attr::Italic, v == "italic" || v == "oblique");
        }
        if let Some(v) = get("style:text-underline-style") {
            let on = v != "none";
            let dbl = on && get("style:text-underline-type") == Some("double");
            ts.attrs.set(Attr::Underline, on && !dbl);
            ts.attrs.set(Attr::DblUnderline, dbl);
        }
        if let Some(v) = get("style:text-line-through-style") {
            ts.attrs.set(Attr::Strikeout, v != "none" && get("style:text-line-through-type") != Some("none"));
        }
        if let Some(v) = get("fo:font-variant") {
            ts.attrs.set(Attr::SmallCaps, v == "small-caps");
        }
        if let Some(v) = get("style:text-position") {
            let first = v.split_whitespace().next().unwrap_or("0%");
            let (sup, sub) = match first {
                "super" => (true, false),
                "sub" => (false, true),
                x => {
                    let n = pct_of(x).unwrap_or(0.0);
                    (n > 0.0, n < 0.0)
                }
            };
            ts.attrs.set(Attr::Superscript, sup);
            ts.attrs.set(Attr::Subscript, sub);
        }
        if let Some(v) = get("style:text-outline") {
            ts.attrs.set(Attr::Outline, v == "true");
        }
        if let Some(v) = get("fo:text-shadow") {
            ts.attrs.set(Attr::Shadow, v != "none");
        }
    }

    /// Text state of a paragraph style (default style + chain).
    fn para_ts(&self, name: Option<&str>, hdr: bool) -> TS {
        let mut ts = TS { attrs: AttrSet::default(), font: FontSpec { name: "Times New Roman".into(), size_cpt: 1200 } };
        if let Some(d) = self.defaults.get("paragraph") {
            self.apply_tpr(&mut ts, &d.tpr, false);
        }
        // An automatic paragraph style (LibreOffice moves a size that
        // covers the whole paragraph there) may carry a relative size too.
        let auto = name.is_some_and(|n| {
            let key = format!("paragraph|{n}");
            if hdr { self.auto_s.contains_key(&key) } else { self.auto_c.contains_key(&key) }
        });
        let ch = self.chain("paragraph", name, hdr);
        let last = ch.len().saturating_sub(1);
        for (k, s) in ch.into_iter().enumerate() {
            self.apply_tpr(&mut ts, &s.tpr, auto && k == last && k > 0);
        }
        ts
    }

    fn span_ts(&self, base: &TS, name: Option<&str>, hdr: bool) -> TS {
        let mut ts = base.clone();
        for s in self.chain("text", name, hdr) {
            self.apply_tpr(&mut ts, &s.tpr, true);
        }
        ts
    }

    fn para_props(&self, name: Option<&str>, hdr: bool) -> PP {
        let mut p = PP::default();
        let mut layers: Vec<&SInfo> = self.defaults.get("paragraph").into_iter().collect();
        layers.extend(self.chain("paragraph", name, hdr));
        for s in layers {
            for (k, v) in &s.ppr {
                match k.as_str() {
                    "fo:text-align" => {
                        p.align = Some(match v.as_str() {
                            "center" => Justify::Center,
                            "end" | "right" => Justify::Right,
                            "justify" => Justify::Full,
                            _ => Justify::Left,
                        })
                    }
                    "fo:margin-left" => p.ml = len_wpu(v).unwrap_or(p.ml),
                    "fo:margin-right" => p.mr = len_wpu(v).unwrap_or(p.mr),
                    "fo:text-indent" => p.ti = len_wpu(v).unwrap_or(p.ti),
                    "fo:line-height" => {
                        p.lh = pct_of(v).filter(|x| *x > 0.0).map(|x| (x.round() as u32).min(1000));
                    }
                    "fo:break-before" => p.bb = v == "page",
                    "fo:break-after" => p.ba = v == "page",
                    _ => {}
                }
            }
            if s.tabs.is_some() {
                p.tabs = s.tabs.clone();
            }
        }
        p
    }
}

// ================================================================= read

struct Change {
    del: bool,
    author: String,
    date: String,
    node: Option<Node>,
}

struct Rd<'a> {
    s: &'a Styles,
    changes: &'a HashMap<String, Change>,
    init: &'a InitialFormat,
    hdr: bool,
    els: Vec<El>,
    attrs: AttrSet,
    font: FontSpec,
    ts: Vec<TS>,
    need_sep: bool,
    page_next: bool,
    just: Justify,
    left: i32,
    right: i32,
    spacing: u32,
    tabs: TabSet,
    last_space: bool,
    ins_open: bool,
    in_del: bool,
    prefix: Option<String>,
    in_table: usize,
}

fn is_fmt_code(e: &El) -> bool {
    matches!(
        e,
        El::Code(
            Code::Just(_) | Code::LRMargin { .. } | Code::LineSpacing(_) | Code::TabSet(_) | Code::Font(_) | Code::AttrOn(_) | Code::AttrOff(_)
        )
    )
}

/// `[RevOff][HRt]…[RevOn]` of the same insertion split at a paragraph
/// end by the writer → one insertion containing the `[HRt]`.
fn merge_revs(els: Vec<El>) -> Vec<El> {
    let n = els.len();
    let mut out = Vec::with_capacity(n);
    let mut last: Option<(String, String)> = None;
    let mut skip_on: Option<usize> = None;
    let mut i = 0;
    while i < n {
        match &els[i] {
            El::Code(Code::RevOn { del: false, author, date }) => {
                if skip_on == Some(i) {
                    skip_on = None;
                    i += 1;
                    continue;
                }
                last = Some((author.clone(), date.clone()));
            }
            El::Code(Code::RevOff { del: false }) => {
                if let Some((a, d)) = &last {
                    let mut j = i + 1;
                    if j < n && els[j] == El::Code(Code::HRt) {
                        j += 1;
                        while j < n && is_fmt_code(&els[j]) {
                            j += 1;
                        }
                        if matches!(&els.get(j), Some(El::Code(Code::RevOn { del: false, author, date })) if author == a && date == d) {
                            skip_on = Some(j);
                            i += 1;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        out.push(els[i].clone());
        i += 1;
    }
    out
}

/// The reader closes attributes lazily (at the next text), which puts
/// `[bold]` after the `[HRt]`: move such off-codes before the break, as WP
/// users would have typed it.
fn offs_before_breaks(els: Vec<El>) -> Vec<El> {
    let mut out: Vec<El> = Vec::with_capacity(els.len());
    let mut i = 0;
    while i < els.len() {
        if matches!(els[i], El::Code(Code::HRt | Code::HPg)) {
            let mut j = i + 1;
            while j < els.len() && matches!(els[j], El::Code(Code::Just(_) | Code::LRMargin { .. } | Code::LineSpacing(_) | Code::TabSet(_))) {
                j += 1;
            }
            let mut k = j;
            while k < els.len() && matches!(els[k], El::Code(Code::AttrOff(_))) {
                k += 1;
            }
            if k > j {
                out.extend_from_slice(&els[j..k]);
                out.extend_from_slice(&els[i..j]);
                i = k;
                continue;
            }
        }
        out.push(els[i].clone());
        i += 1;
    }
    out
}

fn is_foreign(name: &str) -> bool {
    ["draw:", "svg:", "dr3d:", "form:", "chart:", "math:", "office:binary-data", "office:forms", "presentation:"]
        .iter()
        .any(|p| name.starts_with(p))
}

/// Plain text of a node (for comments).
fn plain(n: &Node, out: &mut String) {
    for k in &n.kids {
        match k {
            Kid::T(t) => out.push_str(t),
            Kid::N(c) => match c.name.as_str() {
                "text:s" => {
                    let c = c.attr("text:c").and_then(|x| x.parse::<usize>().ok()).unwrap_or(1).min(1000);
                    out.push_str(&" ".repeat(c));
                }
                "text:tab" => out.push('\t'),
                "text:line-break" => out.push('\n'),
                "dc:creator" | "dc:date" | "meta:creator-initials" => {}
                _ => plain(c, out),
            },
        }
    }
}

fn collect_paras<'n>(n: &'n Node, out: &mut Vec<&'n Node>) {
    for c in n.nodes() {
        match c.name.as_str() {
            "text:p" | "text:h" => out.push(c),
            "office:change-info" => {}
            _ => collect_paras(c, out),
        }
    }
}

fn first_para(n: &Node) -> Option<&Node> {
    for c in n.nodes() {
        match c.name.as_str() {
            "text:p" | "text:h" => return Some(c),
            "text:tracked-changes" | "text:sequence-decls" => {}
            x if is_foreign(x) => {}
            _ => {
                if let Some(p) = first_para(c) {
                    return Some(p);
                }
            }
        }
    }
    None
}

impl<'a> Rd<'a> {
    fn new(s: &'a Styles, changes: &'a HashMap<String, Change>, init: &'a InitialFormat, hdr: bool) -> Self {
        Rd {
            s,
            changes,
            init,
            hdr,
            els: vec![],
            attrs: AttrSet::default(),
            font: init.font.clone(),
            ts: vec![],
            need_sep: false,
            page_next: false,
            just: init.justify,
            left: init.left,
            right: init.right,
            spacing: 100,
            tabs: TabSet::default_tabs(),
            last_space: true,
            ins_open: false,
            in_del: false,
            prefix: None,
            in_table: 0,
        }
    }

    fn sub(&self, hdr: bool) -> Rd<'a> {
        Rd::new(self.s, self.changes, self.init, hdr)
    }

    fn finish(mut self) -> Vec<El> {
        if self.ins_open {
            self.els.push(El::Code(Code::RevOff { del: false }));
        }
        let els = merge_revs(std::mem::take(&mut self.els));
        crate::rtf::finish_els(offs_before_breaks(els), self.attrs)
    }

    fn sync(&mut self) {
        let t = self.ts.last().cloned().unwrap_or_else(|| TS { attrs: AttrSet::default(), font: self.init.font.clone() });
        crate::rtf::sync_attrs(&mut self.els, &mut self.attrs, &mut self.font, t.attrs, t.font);
    }

    fn close_attrs(&mut self) {
        let f = self.font.clone();
        crate::rtf::sync_attrs(&mut self.els, &mut self.attrs, &mut self.font, AttrSet::default(), f);
    }

    fn put(&mut self, e: El) {
        self.sync();
        self.els.push(e);
    }

    fn sep(&mut self, page: bool) {
        if self.need_sep {
            self.els.push(El::Code(if page { Code::HPg } else { Code::HRt }));
        } else if page && !self.els.is_empty() {
            self.els.push(El::Code(Code::HPg));
        }
        self.need_sep = true;
    }

    fn blocks(&mut self, n: &Node) {
        for c in n.nodes() {
            self.block(c);
        }
    }

    fn block(&mut self, c: &Node) {
        match c.name.as_str() {
            "text:p" | "text:h" => self.para(c),
            "table:table" => self.table(c),
            "text:list" => self.list(c, None, 0),
            "text:tracked-changes" | "text:sequence-decls" | "text:variable-decls" | "text:user-field-decls" | "text:dde-connection-decls"
            | "table:calculation-settings" | "table:content-validations" | "table:label-ranges" | "text:soft-page-break" | "office:annotation"
            | "text:note-citation" => {}
            x if is_foreign(x) || x.ends_with("-source") || x.starts_with("office:change") => {}
            _ => self.blocks(c),
        }
    }

    fn list(&mut self, n: &Node, inherited: Option<&str>, level: usize) {
        let style = n.attr("text:style-name").or(inherited).map(String::from);
        let info = style.as_deref().and_then(|s| self.s.lists.get(s)).and_then(|v| v.iter().find(|l| l.level == level + 1)).cloned();
        let mut counter = 0;
        for item in n.nodes() {
            let header = match item.name.as_str() {
                "text:list-item" => false,
                "text:list-header" => true,
                _ => continue,
            };
            if !header {
                counter += 1;
                if let Some(sv) = item.attr("text:start-value").and_then(|v| v.parse::<usize>().ok()) {
                    counter = sv.min(100_000);
                }
            }
            self.prefix = if header {
                None
            } else {
                match &info {
                    Some(l) if l.number => Some(format!("{}{}{} ", l.prefix, counter, l.suffix)),
                    Some(l) if !l.number && !l.bullet.is_empty() => Some(format!("{} ", l.bullet)),
                    Some(_) => None,
                    None => Some("• ".into()),
                }
            };
            for c in item.nodes() {
                if c.name == "text:list" {
                    self.list(c, style.as_deref(), level + 1);
                } else {
                    self.block(c);
                }
            }
            self.prefix = None;
        }
    }

    fn para(&mut self, n: &Node) {
        let sn = n.attr("text:style-name");
        let pp = self.s.para_props(sn, self.hdr);
        let page = (pp.bb || std::mem::take(&mut self.page_next)) && self.in_table == 0;
        self.sep(page);
        self.page_next = pp.ba;
        let just = pp.align.unwrap_or(Justify::Left);
        if just != self.just {
            self.els.push(El::Code(Code::Just(just)));
            self.just = just;
        }
        let hang = pp.ti < 0 && (pp.ti + 600).abs() <= 12 && pp.ml + pp.ti >= 0;
        let base_ml = if hang { pp.ml + pp.ti } else { pp.ml };
        let (left, right) = (self.init.left + base_ml, self.init.right + pp.mr);
        if (left, right) != (self.left, self.right) {
            self.els.push(El::Code(Code::LRMargin { left, right }));
            self.left = left;
            self.right = right;
        }
        let sp = pp.lh.unwrap_or(100);
        if sp != self.spacing {
            self.els.push(El::Code(Code::LineSpacing(sp)));
            self.spacing = sp;
        }
        let tabs = match &pp.tabs {
            Some(v) => TabSet { stops: v.iter().map(|t| TabStop { pos: t.pos + pp.ml - base_ml, ..*t }).collect(), relative: true },
            None => TabSet::default_tabs(),
        };
        if tabs != self.tabs {
            self.els.push(El::Code(Code::TabSet(tabs.clone())));
            self.tabs = tabs;
        }
        self.ts.clear();
        self.ts.push(self.s.para_ts(sn, self.hdr));
        self.last_space = true;
        if hang {
            self.put(El::Code(Code::Indent));
            self.els.push(El::Code(Code::MarRel));
        } else if pp.ti > 0 {
            self.put(El::Code(Code::Tab));
        }
        if let Some(p) = self.prefix.take() {
            for c in p.chars() {
                self.put(El::Ch(c));
            }
        }
        self.inline(n);
    }

    fn text_char(&mut self, c: char) {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if !self.last_space {
                    self.put(El::Ch(' '));
                    self.last_space = true;
                }
            }
            '\u{a0}' => {
                self.put(El::Code(Code::HardSpace));
                self.last_space = false;
            }
            '\u{ad}' => {
                self.put(El::Code(Code::SoftHyphen));
                self.last_space = false;
            }
            '\u{2011}' => {
                self.put(El::Code(Code::HardHyphen));
                self.last_space = false;
            }
            c if c.is_control() => {}
            c => {
                self.put(El::Ch(c));
                self.last_space = false;
            }
        }
    }

    fn inline(&mut self, n: &Node) {
        for k in &n.kids {
            match k {
                Kid::T(t) => {
                    for c in t.chars() {
                        self.text_char(c);
                    }
                }
                Kid::N(e) => self.inline_el(e),
            }
        }
    }

    fn inline_el(&mut self, e: &Node) {
        match e.name.as_str() {
            "text:s" => {
                let c = e.attr("text:c").and_then(|x| x.parse::<usize>().ok()).unwrap_or(1).clamp(1, 1000);
                for _ in 0..c {
                    self.put(El::Ch(' '));
                }
                self.last_space = false;
            }
            "text:tab" => {
                self.put(El::Code(Code::Tab));
                self.last_space = false;
            }
            "text:line-break" => {
                self.put(El::Code(Code::HRt));
                self.last_space = false;
            }
            "text:span" => {
                let base = self.ts.last().cloned().unwrap_or_else(|| self.s.para_ts(None, self.hdr));
                let t = self.s.span_ts(&base, e.attr("text:style-name"), self.hdr);
                self.ts.push(t);
                self.inline(e);
                self.ts.pop();
            }
            "text:note" => {
                if self.in_del {
                    return;
                }
                self.sync();
                let kind = if e.attr("text:note-class") == Some("endnote") { NoteKind::End } else { NoteKind::Foot };
                let mut sub = self.sub(self.hdr);
                if let Some(b) = e.child("text:note-body") {
                    sub.blocks(b);
                }
                let mut els = sub.finish();
                while matches!(els.last(), Some(El::Code(Code::HRt))) {
                    els.pop();
                }
                self.els.push(El::Code(Code::Note { kind, els }));
                self.last_space = false;
            }
            "office:annotation" => {
                self.sync();
                let mut s = String::new();
                let mut first = true;
                for p in e.nodes() {
                    if matches!(p.name.as_str(), "dc:creator" | "dc:date" | "meta:creator-initials") {
                        continue;
                    }
                    let mut paras = vec![];
                    if p.name == "text:p" || p.name == "text:h" {
                        paras.push(p);
                    } else {
                        collect_paras(p, &mut paras);
                    }
                    for q in paras {
                        if !first {
                            s.push('\n');
                        }
                        first = false;
                        plain(q, &mut s);
                    }
                }
                self.els.push(El::Code(Code::Comment(s)));
            }
            "text:change-start" if !self.in_del => {
                if let Some(c) = e.attr("text:change-id").and_then(|id| self.changes.get(id)) {
                    if !c.del && !self.ins_open {
                        self.sync();
                        self.els.push(El::Code(Code::RevOn { del: false, author: c.author.clone(), date: c.date.clone() }));
                        self.ins_open = true;
                    }
                }
            }
            "text:change-end" if !self.in_del => {
                if let Some(c) = e.attr("text:change-id").and_then(|id| self.changes.get(id)) {
                    if !c.del && self.ins_open {
                        self.sync();
                        self.els.push(El::Code(Code::RevOff { del: false }));
                        self.ins_open = false;
                    }
                }
            }
            "text:change" if !self.in_del => {
                let changes = self.changes;
                let Some(c) = e.attr("text:change-id").and_then(|id| changes.get(id)) else { return };
                let Some(node) = c.node.as_ref().filter(|_| c.del) else { return };
                let reopen = self.ins_open;
                if reopen {
                    self.els.push(El::Code(Code::RevOff { del: false }));
                    self.ins_open = false;
                }
                self.close_attrs();
                self.els.push(El::Code(Code::RevOn { del: true, author: c.author.clone(), date: c.date.clone() }));
                let saved = (self.ts.clone(), self.last_space);
                self.in_del = true;
                let mut paras = vec![];
                collect_paras(node, &mut paras);
                for (k, p) in paras.into_iter().enumerate() {
                    if k > 0 {
                        self.els.push(El::Code(Code::HRt));
                    }
                    self.ts = vec![self.s.para_ts(p.attr("text:style-name"), self.hdr)];
                    self.last_space = true;
                    self.inline(p);
                }
                self.in_del = false;
                self.close_attrs();
                self.els.push(El::Code(Code::RevOff { del: true }));
                (self.ts, self.last_space) = saved;
                if reopen {
                    self.els.push(El::Code(Code::RevOn { del: false, author: String::new(), date: String::new() }));
                    // restore the author/date of the surrounding insertion
                    if let Some(El::Code(Code::RevOn { author, date, .. })) =
                        self.els.iter().rev().skip(1).find(|x| matches!(x, El::Code(Code::RevOn { del: false, .. }))).cloned()
                    {
                        if let Some(El::Code(Code::RevOn { author: a, date: d, .. })) = self.els.last_mut() {
                            *a = author;
                            *d = date;
                        }
                    }
                    self.ins_open = true;
                }
            }
            "text:change-start" | "text:change-end" | "text:change" => {}
            "text:soft-page-break" | "office:annotation-end" | "text:note-citation" | "text:bookmark" | "text:bookmark-start" | "text:bookmark-end"
            | "text:reference-mark" | "text:reference-mark-start" | "text:reference-mark-end" | "text:ruby-text" => {}
            "text:p" | "text:h" | "text:list" | "table:table" => {} // not allowed inline
            x if is_foreign(x) => {}
            _ => self.inline(e),
        }
    }

    fn table(&mut self, t: &Node) {
        fn rows<'n>(n: &'n Node, out: &mut Vec<&'n Node>) {
            for c in n.nodes() {
                match c.name.as_str() {
                    "table:table-row" => {
                        let rep = c.attr("table:number-rows-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 64);
                        for _ in 0..rep {
                            out.push(c);
                        }
                    }
                    "table:table-header-rows" | "table:table-rows" | "table:table-row-group" => rows(c, out),
                    _ => {}
                }
            }
        }
        fn cols(n: &Node, s: &Styles, hdr: bool, out: &mut Vec<Option<i32>>) {
            for c in n.nodes() {
                match c.name.as_str() {
                    "table:table-column" => {
                        let rep = c.attr("table:number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 64);
                        let w = c.attr("table:style-name").and_then(|sn| s.get("table-column", sn, hdr)).and_then(|i| i.col_w);
                        for _ in 0..rep {
                            if out.len() < 64 {
                                out.push(w);
                            }
                        }
                    }
                    "table:table-header-columns" | "table:table-columns" | "table:table-column-group" => cols(c, s, hdr, out),
                    _ => {}
                }
            }
        }
        let mut rv = vec![];
        rows(t, &mut rv);
        if self.in_table > 0 {
            // WP tables cannot nest: flatten into tabs / lines
            self.sep(false);
            self.need_sep = false;
            for (ri, row) in rv.iter().enumerate() {
                if ri > 0 {
                    self.els.push(El::Code(Code::HRt));
                }
                for (ci, cell) in row.nodes().filter(|c| c.name == "table:table-cell").enumerate() {
                    if ci > 0 {
                        self.put(El::Code(Code::Tab));
                    }
                    self.need_sep = false;
                    self.blocks(cell);
                }
            }
            self.need_sep = true;
            return;
        }
        let brk = t.attr("table:style-name").and_then(|sn| self.s.get("table", sn, self.hdr)).is_some_and(|i| i.tbl_brk);
        let page = brk || std::mem::take(&mut self.page_next);
        self.sep(page);
        self.close_attrs();
        let mut cv = vec![];
        cols(t, self.s, self.hdr, &mut cv);
        let def_at = self.els.len();
        self.els.push(El::Code(Code::TblDef { cols: vec![] }));
        self.in_table += 1;
        let mut max_cells = 0;
        for row in &rv {
            self.close_attrs();
            self.els.push(El::Code(Code::Row));
            let mut ncell = 0;
            for cell in row.nodes().filter(|c| c.name == "table:table-cell") {
                let rep = cell.attr("table:number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 64);
                for _ in 0..rep {
                    self.close_attrs();
                    self.els.push(El::Code(Code::Cell));
                    self.need_sep = false;
                    self.blocks(cell);
                    ncell += 1;
                }
            }
            max_cells = max_cells.max(ncell);
        }
        self.close_attrs();
        self.els.push(El::Code(Code::TblOff));
        self.in_table -= 1;
        let ncols = max_cells.max(1);
        let text_w = (self.init.page_width - self.init.left - self.init.right).max(1200);
        let known: Vec<i32> = cv.iter().flatten().copied().collect();
        let mut widths: Vec<i32> = if known.len() == cv.len() && !cv.is_empty() { known } else { vec![] };
        if widths.len() > ncols {
            widths.truncate(ncols);
        }
        if widths.len() < ncols {
            let rest = (text_w - widths.iter().sum::<i32>()).max(600 * (ncols - widths.len()) as i32);
            let w = rest / (ncols - widths.len()) as i32;
            widths.resize(ncols, w);
        }
        if let El::Code(Code::TblDef { cols }) = &mut self.els[def_at] {
            *cols = widths;
        }
        self.need_sep = false;
    }
}

fn convert(content: &Node, styles: Option<&Node>, meta: Option<&Node>) -> Document {
    let mut s = Styles::default();
    if let Some(st) = styles {
        s.load(st, false);
    }
    s.load(content, true);
    let mut d = Document::new();
    let mut init = InitialFormat::default();
    let master = s.masters.iter().find(|m| m.attr("style:name") == Some("Standard")).or(s.masters.first());
    if let Some(pl) = master.and_then(|m| m.attr("style:page-layout-name")).and_then(|n| s.layouts.get(n)) {
        for (k, v) in pl {
            let Some(w) = len_wpu(v) else { continue };
            match k.as_str() {
                "fo:page-width" if w > 600 => init.page_width = w,
                "fo:page-height" if w > 600 => init.page_height = w,
                "fo:margin-left" if w >= 0 => init.left = w,
                "fo:margin-right" if w >= 0 => init.right = w,
                "fo:margin-top" if w >= 0 => init.top = w,
                "fo:margin-bottom" if w >= 0 => init.bottom = w,
                _ => {}
            }
        }
    }
    init.font = s.para_ts(Some("Standard"), false).font;
    init.line_spacing = 100;
    init.tabs = TabSet::default_tabs();
    let text = content.path(&["office:body", "office:text"]);
    let mut changes = HashMap::new();
    if let Some(tc) = text.and_then(|t| t.child("text:tracked-changes")) {
        d.track = tc.attr("text:track-changes") != Some("false");
        for r in tc.nodes().filter(|r| r.name == "text:changed-region") {
            let Some(id) = r.attr("text:id").or_else(|| r.attr("xml:id")) else { continue };
            let Some(kind) = r.nodes().find(|k| matches!(k.name.as_str(), "text:insertion" | "text:deletion" | "text:format-change")) else { continue };
            let info = kind.child("office:change-info");
            let txt = |tag: &str| {
                let mut s = String::new();
                if let Some(n) = info.and_then(|i| i.child(tag)) {
                    plain(n, &mut s);
                }
                s
            };
            let del = kind.name == "text:deletion";
            if kind.name == "text:format-change" {
                continue;
            }
            changes.insert(id.to_string(), Change { del, author: txt("dc:creator"), date: txt("dc:date"), node: if del { Some(kind.clone()) } else { None } });
        }
    }
    init.justify = text.and_then(first_para).map(|p| s.para_props(p.attr("text:style-name"), false).align.unwrap_or(Justify::Left)).unwrap_or(Justify::Left);

    let mut hf: Vec<El> = vec![];
    if let Some(m) = master {
        let shown = |n: &&Node| n.attr("style:display") != Some("false");
        for (tag, base) in [("header", 0u8), ("footer", 2u8)] {
            let left = m.child(&format!("style:{tag}-left")).filter(shown);
            let main = m.child(&format!("style:{tag}")).filter(shown);
            for (node, which, occ) in [
                (main, base, if left.is_some() { Occurrence::Odd } else { Occurrence::Every }),
                (left, base + 1, Occurrence::Even),
            ] {
                let Some(node) = node else { continue };
                let mut rd = Rd::new(&s, &changes, &init, true);
                rd.blocks(node);
                let mut els = rd.finish();
                while matches!(els.last(), Some(El::Code(Code::HRt))) {
                    els.pop();
                }
                if !els.iter().all(|e| is_fmt_code(e) || *e == El::Code(Code::HRt)) {
                    hf.push(El::Code(Code::HeadFoot { which, occ, els }));
                }
            }
        }
    }
    let mut rd = Rd::new(&s, &changes, &init, false);
    if let Some(t) = text {
        rd.blocks(t);
    }
    let els = rd.finish();
    d.els = hf;
    d.els.extend(els);
    d.init = init;

    if let Some(m) = meta.and_then(|m| if m.name == "office:meta" { Some(m) } else { m.child("office:meta") }) {
        let get = |tag: &str| {
            let mut s = String::new();
            if let Some(n) = m.child(tag) {
                plain(n, &mut s);
            }
            s.trim().to_string()
        };
        d.summary.descriptive_name = get("dc:title");
        d.summary.subject = get("dc:subject");
        d.summary.comments = get("dc:description");
        let initial = get("meta:initial-creator");
        let creator = get("dc:creator");
        if initial.is_empty() {
            d.summary.author = creator;
        } else {
            if creator != initial {
                d.summary.typist = creator;
            }
            d.summary.author = initial;
        }
    }
    d
}

fn bad(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string())
}

/// Read an OpenDocument Text package (.odt / .ott).
pub fn read_odt(bytes: &[u8]) -> std::io::Result<Document> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(std::io::Error::other)?;
    let mut part = |name: &str| -> Option<String> {
        let f = z.by_name(name).ok()?;
        let mut v = Vec::new();
        f.take(256 << 20).read_to_end(&mut v).ok()?;
        Some(String::from_utf8_lossy(&v).into_owned())
    };
    let content = part("content.xml").ok_or_else(|| bad("not an OpenDocument text file (no content.xml)"))?;
    let styles = part("styles.xml");
    let meta = part("meta.xml");
    let c = parse_dom(&content);
    let s = styles.as_deref().map(parse_dom);
    let m = meta.as_deref().map(parse_dom);
    let croot = doc_elem(&c).ok_or_else(|| bad("content.xml is empty"))?;
    let sroot = s.as_ref().and_then(doc_elem);
    let mroot = m.as_ref().and_then(doc_elem);
    Ok(convert(croot, sroot, mroot))
}

/// Read a flat OpenDocument Text file (.fodt).
pub fn read_fodt(bytes: &[u8]) -> std::io::Result<Document> {
    let s = String::from_utf8_lossy(bytes);
    let dom = parse_dom(s.trim_start_matches('\u{feff}'));
    let root = doc_elem(&dom).filter(|r| r.name == "office:document").ok_or_else(|| bad("not a flat OpenDocument text file"))?;
    Ok(convert(root, Some(root), Some(root)))
}

// ================================================================= tests

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<El> {
        s.chars().map(El::Ch).collect()
    }
    fn code(c: Code) -> El {
        El::Code(c)
    }
    fn text(d: &Document) -> String {
        d.plain_text(0, d.len())
    }
    fn rt(d: &Document) -> Document {
        read_odt(&write_odt(d).unwrap()).unwrap()
    }
    /// Text written while `a` is on.
    fn attr_text(els: &[El], a: Attr) -> String {
        let mut on = false;
        let mut s = String::new();
        for e in els {
            match e {
                El::Code(Code::AttrOn(x)) if *x == a => on = true,
                El::Code(Code::AttrOff(x)) if *x == a => on = false,
                El::Ch(c) if on => s.push(*c),
                _ => {}
            }
        }
        s
    }
    fn part(bytes: &[u8], name: &str) -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut s = String::new();
        z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    }
    fn balanced(els: &[El]) {
        let mut d = Document { els: els.to_vec(), ..Default::default() };
        let before = d.els.clone();
        d.normalize_attrs();
        assert_eq!(before, d.els, "attributes not normalized");
        let mut st = AttrSet::default();
        for e in els {
            if let Some((a, on)) = e.code().and_then(|c| c.attr()) {
                st.set(a, on);
            }
        }
        assert!(st.is_empty(), "unbalanced attributes {els:?}");
    }

    fn sample() -> Document {
        let mut d = Document::new();
        d.els.extend(chars("Größe äöü ß – "));
        d.els.push(code(Code::AttrOn(Attr::Bold)));
        d.els.extend(chars("fett"));
        d.els.push(code(Code::AttrOff(Attr::Bold)));
        d.els.push(code(Code::HRt));
        d.els.extend(chars("Привет "));
        d.els.push(code(Code::AttrOn(Attr::Italic)));
        d.els.extend(chars("мир"));
        d.els.push(code(Code::AttrOff(Attr::Italic)));
        d.els.push(code(Code::HRt));
        d.els.extend(chars("Ça été "));
        d.els.push(code(Code::AttrOn(Attr::Underline)));
        d.els.extend(chars("souligné"));
        d.els.push(code(Code::AttrOff(Attr::Underline)));
        d.els.push(code(Code::Tab));
        d.els.push(code(Code::AttrOn(Attr::DblUnderline)));
        d.els.extend(chars("double"));
        d.els.push(code(Code::AttrOff(Attr::DblUnderline)));
        d.els.push(code(Code::HardSpace));
        d.els.push(code(Code::AttrOn(Attr::Strikeout)));
        d.els.extend(chars("weg"));
        d.els.push(code(Code::AttrOff(Attr::Strikeout)));
        d.els.push(code(Code::AttrOn(Attr::SmallCaps)));
        d.els.extend(chars("Kap"));
        d.els.push(code(Code::AttrOff(Attr::SmallCaps)));
        d.els.extend(chars("x"));
        d.els.push(code(Code::AttrOn(Attr::Superscript)));
        d.els.extend(chars("2"));
        d.els.push(code(Code::AttrOff(Attr::Superscript)));
        d.els.extend(chars("H"));
        d.els.push(code(Code::AttrOn(Attr::Subscript)));
        d.els.extend(chars("2"));
        d.els.push(code(Code::AttrOff(Attr::Subscript)));
        d.els.extend(chars("O Bind"));
        d.els.push(code(Code::HardHyphen));
        d.els.extend(chars("strich Trenn"));
        d.els.push(code(Code::SoftHyphen));
        d.els.extend(chars("bar"));
        d
    }

    #[test]
    fn roundtrip_text_and_attributes() {
        let d = sample();
        let d2 = rt(&d);
        assert_eq!(text(&d2), text(&d));
        // exact stream (Just is folded into init)
        assert_eq!(d2.els, d.els);
        assert_eq!(attr_text(&d2.els, Attr::Bold), "fett");
        assert_eq!(attr_text(&d2.els, Attr::Italic), "мир");
        assert_eq!(attr_text(&d2.els, Attr::DblUnderline), "double");
        balanced(&d2.els);
    }

    #[test]
    fn spaces_are_preserved() {
        let mut d = Document::from_text("  zwei vorne,   drei innen, eins hinten \nx");
        d.els.insert(5, code(Code::AttrOn(Attr::Bold)));
        d.els.insert(9, code(Code::AttrOff(Attr::Bold)));
        let bytes = write_odt(&d).unwrap();
        let c = part(&bytes, "content.xml");
        assert!(c.contains("<text:s text:c=\"2\"/>"), "{c}");
        assert_eq!(text(&rt(&d)), text(&d));
    }

    #[test]
    fn fonts_and_sizes() {
        let mut d = Document::from_text("Basis ");
        d.init.font = FontSpec { name: "Courier New".into(), size_cpt: 1200 };
        d.els.push(code(Code::Font(FontSpec { name: "Arial".into(), size_cpt: 1450 })));
        d.els.extend(chars("Arial"));
        d.els.push(code(Code::Font(FontSpec { name: "Courier New".into(), size_cpt: 1200 })));
        d.els.push(code(Code::AttrOn(Attr::Large)));
        d.els.extend(chars("groß"));
        d.els.push(code(Code::AttrOff(Attr::Large)));
        d.els.push(code(Code::AttrOn(Attr::Fine)));
        d.els.extend(chars("fein"));
        d.els.push(code(Code::AttrOff(Attr::Fine)));
        let bytes = write_odt(&d).unwrap();
        let st = part(&bytes, "styles.xml");
        assert!(st.contains("style:font-name=\"Courier New\"") && st.contains("fo:font-size=\"12pt\""), "{st}");
        let d2 = read_odt(&bytes).unwrap();
        assert_eq!(d2.init.font, d.init.font);
        assert_eq!(d2.els, d.els);
    }

    #[test]
    fn alignment_margins_spacing() {
        let mut d = Document::new();
        d.init.justify = Justify::Left;
        d.els.extend(chars("links"));
        d.els.push(code(Code::HRt));
        d.els.push(code(Code::Just(Justify::Full)));
        d.els.extend(chars("Blocksatz"));
        d.els.push(code(Code::HRt));
        d.els.push(code(Code::Just(Justify::Right)));
        d.els.push(code(Code::LRMargin { left: 1800, right: 1500 }));
        d.els.push(code(Code::LineSpacing(150)));
        d.els.extend(chars("rechts"));
        d.els.push(code(Code::HRt));
        d.els.push(code(Code::Center));
        d.els.extend(chars("mitte"));
        let d2 = rt(&d);
        assert_eq!(d2.init.justify, Justify::Left);
        assert_eq!(text(&d2), text(&d));
        assert!(d2.els.contains(&code(Code::Just(Justify::Full))));
        assert!(d2.els.contains(&code(Code::Just(Justify::Right))));
        assert!(d2.els.contains(&code(Code::Just(Justify::Center))));
        assert!(d2.els.contains(&code(Code::LRMargin { left: 1800, right: 1500 })));
        assert!(d2.els.contains(&code(Code::LineSpacing(150))));
        // hanging indent [->Indent][<-Mar Rel] survives
        let mut h = Document::from_text("x\n");
        h.els.push(code(Code::Indent));
        h.els.push(code(Code::MarRel));
        h.els.extend(chars("hängend"));
        let h2 = rt(&h);
        assert!(h2.els.windows(2).any(|w| w == [code(Code::Indent), code(Code::MarRel)]), "{:?}", h2.els);
    }

    #[test]
    fn tabs_and_tab_stops() {
        let mut d = Document::new();
        let ts = TabSet {
            stops: vec![
                TabStop { pos: 1200, kind: TabKind::Left, dot_leader: false },
                TabStop { pos: 3000, kind: TabKind::Center, dot_leader: false },
                TabStop { pos: 4800, kind: TabKind::Right, dot_leader: true },
                TabStop { pos: 6000, kind: TabKind::Decimal, dot_leader: false },
            ],
            relative: true,
        };
        d.els.push(code(Code::TabSet(ts.clone())));
        d.els.extend(chars("a"));
        d.els.push(code(Code::Tab));
        d.els.extend(chars("b"));
        d.els.push(code(Code::Tab));
        d.els.extend(chars("c"));
        let bytes = write_odt(&d).unwrap();
        let c = part(&bytes, "content.xml");
        assert!(c.contains("style:type=\"char\"") && c.contains("style:leader-text=\".\"") && c.contains("<text:tab/>"), "{c}");
        let d2 = read_odt(&bytes).unwrap();
        assert_eq!(text(&d2), "a\tb\tc");
        assert!(d2.els.contains(&code(Code::TabSet(ts))), "{:?}", d2.els);
    }

    #[test]
    fn page_setup_and_summary() {
        let mut d = Document::from_text("Seite");
        d.init.page_width = 10200;
        d.init.page_height = 13200;
        d.init.left = 1800;
        d.init.right = 900;
        d.init.top = 1500;
        d.init.bottom = 600;
        d.summary.descriptive_name = "Übersetzung".into();
        d.summary.subject = "Vertrag".into();
        d.summary.author = "Maria".into();
        let d2 = rt(&d);
        let (a, b) = (&d.init, &d2.init);
        assert_eq!((b.page_width, b.page_height, b.left, b.right, b.top, b.bottom), (a.page_width, a.page_height, a.left, a.right, a.top, a.bottom));
        assert_eq!(d2.summary.descriptive_name, "Übersetzung");
        assert_eq!(d2.summary.subject, "Vertrag");
        assert_eq!(d2.summary.author, "Maria");
        assert_eq!(d2.els, d.els);
    }

    fn table_doc() -> Document {
        let mut d = Document::from_text("Vor\n");
        let mut k = 0;
        for e in crate::tables::new_table(2, 2, 7200) {
            let is_cell = e == code(Code::Cell);
            d.els.push(e);
            if is_cell {
                k += 1;
                d.els.extend(chars(&format!("Zelle{k}")));
            }
        }
        d.els.extend(chars("Nach"));
        d
    }

    #[test]
    fn table_roundtrip() {
        let d = table_doc();
        let bytes = write_odt(&d).unwrap();
        assert!(part(&bytes, "content.xml").contains("<table:table-column"));
        let d2 = read_odt(&bytes).unwrap();
        assert_eq!(d2.els, d.els);
        let tp = d2.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        let t = crate::tables::parse(&d2.els, tp).unwrap();
        assert_eq!(t.cols, vec![3600, 3600]);
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn notes_roundtrip() {
        let mut d = Document::from_text("Text");
        d.els.push(code(Code::Note { kind: NoteKind::Foot, els: chars("Fußnote") }));
        d.els.extend(chars(" mehr"));
        d.els.push(code(Code::Note { kind: NoteKind::End, els: chars("Endnote") }));
        let bytes = write_odt(&d).unwrap();
        let c = part(&bytes, "content.xml");
        assert!(c.contains("text:note-class=\"footnote\"") && c.contains("text:note-class=\"endnote\"") && c.contains("<text:note-citation>1</text:note-citation>"));
        assert_eq!(read_odt(&bytes).unwrap().els, d.els);
    }

    #[test]
    fn header_footer_roundtrip() {
        let mut d = Document::from_text("Inhalt");
        d.els.insert(0, code(Code::HeadFoot { which: 2, occ: Occurrence::Every, els: chars("Fuß") }));
        let mut hd = vec![code(Code::AttrOn(Attr::Bold))];
        hd.extend(chars("Kopf"));
        hd.push(code(Code::AttrOff(Attr::Bold)));
        d.els.insert(0, code(Code::HeadFoot { which: 0, occ: Occurrence::Every, els: hd }));
        let bytes = write_odt(&d).unwrap();
        let st = part(&bytes, "styles.xml");
        assert!(st.contains("<style:header>") && st.contains("<style:footer>") && st.contains("style:name=\"MT1\""), "{st}");
        let d2 = read_odt(&bytes).unwrap();
        assert_eq!(d2.els, d.els);
        // even pages header → header-left
        let mut e = Document::from_text("x");
        e.els.insert(0, code(Code::HeadFoot { which: 1, occ: Occurrence::Even, els: chars("Gerade") }));
        let b = write_odt(&e).unwrap();
        assert!(part(&b, "styles.xml").contains("<style:header-left>"));
        let e2 = read_odt(&b).unwrap();
        assert_eq!(e2.els.len(), 2, "{:?}", e2.els);
        assert!(matches!(&e2.els[0], El::Code(Code::HeadFoot { which: 1, occ: Occurrence::Even, els }) if *els == chars("Gerade")));
    }

    fn rev(del: bool) -> El {
        code(Code::RevOn { del, author: "Maria".into(), date: "2026-11-11T10:00:00Z".into() })
    }

    #[test]
    fn tracked_changes_roundtrip() {
        let mut d = Document::from_text("ab");
        d.track = true;
        d.els.insert(1, rev(false));
        d.els.insert(2, El::Ch('X'));
        d.els.insert(3, code(Code::RevOff { del: false }));
        d.els.push(rev(true));
        d.els.push(code(Code::AttrOn(Attr::Bold)));
        d.els.extend(chars("Zz"));
        d.els.push(code(Code::AttrOff(Attr::Bold)));
        d.els.push(code(Code::RevOff { del: true }));
        d.els.extend(chars("c"));
        let bytes = write_odt(&d).unwrap();
        let c = part(&bytes, "content.xml");
        assert!(c.contains("<text:tracked-changes text:track-changes=\"true\">") && c.contains("<text:deletion>") && c.contains("<text:change text:change-id="), "{c}");
        let d2 = read_odt(&bytes).unwrap();
        assert!(d2.track);
        assert_eq!(d2.els, d.els);
        // an insertion spanning paragraphs stays one insertion
        let mut m = Document::from_text("a");
        m.els.push(rev(false));
        m.els.extend(chars("eins"));
        m.els.push(code(Code::HRt));
        m.els.extend(chars("zwei"));
        m.els.push(code(Code::RevOff { del: false }));
        m.els.extend(chars("b"));
        assert_eq!(rt(&m).els, m.els);
    }

    #[test]
    fn comment_roundtrip() {
        let mut d = Document::from_text("vor nach");
        d.els.insert(4, code(Code::Comment("Bitte prüfen\nzweite Zeile".into())));
        let bytes = write_odt(&d).unwrap();
        assert!(part(&bytes, "content.xml").contains("<office:annotation>"));
        assert_eq!(read_odt(&bytes).unwrap().els, d.els);
    }

    #[test]
    fn page_break_roundtrip() {
        let d = Document::from_text("eins\u{c}zwei\ndrei\u{c}");
        let bytes = write_odt(&d).unwrap();
        assert!(part(&bytes, "content.xml").contains("fo:break-before=\"page\""));
        assert_eq!(read_odt(&bytes).unwrap().els, d.els);
    }

    #[test]
    fn package_layout_and_detection() {
        let bytes = write_odt(&sample()).unwrap();
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(&bytes[38..38 + MIME.len()], MIME.as_bytes());
        assert!(is_odt(&bytes) && !is_ott(&bytes) && !is_fodt(&bytes));
        let ott = write_ott(&sample()).unwrap();
        assert!(is_odt(&ott) && is_ott(&ott));
        assert_eq!(read_odt(&ott).unwrap().els, sample().els);
        assert!(!is_odt(&crate::docx::write_docx(&sample()).unwrap()));
        assert!(!is_odt(b"PK\x03\x04garbage"));
        assert!(!is_odt(b""));
    }

    #[test]
    fn xml_is_well_formed() {
        let mut d = table_doc();
        d.els.push(code(Code::Note { kind: NoteKind::Foot, els: chars("n <&> \"q\"") }));
        d.els.insert(0, code(Code::HeadFoot { which: 0, occ: Occurrence::Odd, els: chars("K&K") }));
        d.els.push(code(Code::Comment("a<b".into())));
        d.els.push(rev(true));
        d.els.extend(chars("weg"));
        d.els.push(code(Code::RevOff { del: true }));
        d.summary.author = "A & B".into();
        let bytes = write_odt(&d).unwrap();
        for name in ["content.xml", "styles.xml", "meta.xml", "META-INF/manifest.xml"] {
            let x = part(&bytes, name);
            let mut r = Reader::from_str(&x);
            let mut depth = 0i32;
            let mut root = None;
            loop {
                match r.read_event() {
                    Ok(Event::Start(e)) => {
                        if root.is_none() {
                            root = Some(String::from_utf8_lossy(&e.to_vec()).into_owned());
                        }
                        depth += 1;
                    }
                    Ok(Event::End(_)) => depth -= 1,
                    Ok(Event::Eof) => break,
                    Ok(_) => {}
                    Err(e) => panic!("{name}: {e}\n{x}"),
                }
            }
            assert_eq!(depth, 0, "{name}");
            let root = root.unwrap();
            if name != "META-INF/manifest.xml" {
                assert!(root.contains("office:version=\"1.3\""), "{root}");
                // every prefix used must be declared on the root
                for (p, u) in NS {
                    if x.contains(&format!("<{p}:")) || x.contains(&format!(" {p}:")) {
                        assert!(root.contains(&format!("xmlns:{p}=\"{u}\"")), "{name}: {p}");
                    }
                }
            }
        }
    }

    #[test]
    fn flat_fodt_roundtrip() {
        let mut d = sample();
        d.els.extend([code(Code::HRt)]);
        d.els.extend(table_doc().els);
        d.els.insert(0, code(Code::HeadFoot { which: 0, occ: Occurrence::Every, els: chars("Kopf") }));
        d.els.push(code(Code::Note { kind: NoteKind::Foot, els: chars("Fuß") }));
        let x = write_fodt(&d);
        assert!(is_fodt(x.as_bytes()) && !is_odt(x.as_bytes()));
        assert!(!is_fodt(b"<?xml version=\"1.0\"?><office:document-content/>"));
        let d2 = read_fodt(x.as_bytes()).unwrap();
        assert_eq!(d2.els, d.els);
        assert!(read_fodt(b"<html/>").is_err());
    }

    const LO_STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.2">
 <office:font-face-decls><style:font-face style:name="Liberation Serif" svg:font-family="'Liberation Serif'" style:font-family-generic="roman"/><style:font-face style:name="Arial1" svg:font-family="Arial"/></office:font-face-decls>
 <office:styles>
  <style:default-style style:family="paragraph"><style:paragraph-properties style:tab-stop-distance="1.251cm"/><style:text-properties style:font-name="Liberation Serif" fo:font-size="12pt"/></style:default-style>
  <style:style style:name="Standard" style:family="paragraph" style:class="text"/>
  <style:style style:name="Text_20_body" style:display-name="Text body" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-top="0cm" fo:margin-bottom="0.247cm" fo:line-height="115%"/></style:style>
  <style:style style:name="Wichtig" style:family="paragraph" style:parent-style-name="Text_20_body"><style:text-properties fo:font-weight="bold"/></style:style>
  <style:style style:name="Emphasis" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>
 </office:styles>
 <office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="21.001cm" fo:page-height="29.7cm" fo:margin-top="2cm" fo:margin-bottom="20mm" fo:margin-left="2.5cm" fo:margin-right="1in"/></style:page-layout>
  <style:style style:name="MP1" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:text-align="center"/></style:style></office:automatic-styles>
 <office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p text:style-name="MP1">Kopfzeile <text:span text:style-name="Emphasis">kursiv</text:span></text:p></style:header><style:footer style:display="false"><text:p>versteckt</text:p></style:footer></style:master-page></office:master-styles>
</office:document-styles>"#;

    const LO_CONTENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" o:version="1.2">
 <o:automatic-styles>
  <style:style style:name="P1" style:family="paragraph" style:parent-style-name="Wichtig"><style:paragraph-properties fo:text-align="justify"/></style:style>
  <style:style style:name="P2" style:family="paragraph" style:parent-style-name="Text_20_body"><style:paragraph-properties fo:break-before="page"/><style:text-properties style:font-name="Arial1" fo:font-size="14pt"/></style:style>
  <style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="normal" style:text-underline-style="solid"/></style:style>
  <style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="2in"/></style:style>
  <text:list-style xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" style:name="L1"><text:list-level-style-number text:level="1" style:num-suffix="." style:num-format="1"/></text:list-style>
 </o:automatic-styles>
 <o:body><o:text>
  <t:sequence-decls><t:sequence-decl t:name="Table"/></t:sequence-decls>
  <t:p t:style-name="P1">Fett  <t:span t:style-name="T1">nicht   fett</t:span><t:s t:c="2"/>ende<t:soft-page-break/></t:p>
  <t:h t:outline-level="1">Überschrift<t:line-break/>zweite<draw:frame><draw:image/></draw:frame></t:h>
  <t:list t:style-name="L1"><t:list-item><t:p>eins</t:p></t:list-item><t:list-item><t:p>zwei</t:p></t:list-item></t:list>
  <t:p t:style-name="P2">Arial</t:p>
  <table:table><table:table-column table:style-name="co1" table:number-columns-repeated="2"/>
   <table:table-row><table:table-cell table:number-columns-spanned="2"><t:p>breit</t:p></table:table-cell><table:covered-table-cell/></table:table-row>
   <table:table-row><table:table-cell><t:p>a</t:p></table:table-cell><table:table-cell><t:p>b</t:p></table:table-cell></table:table-row>
  </table:table>
 </o:text></o:body>
</office:document-content>"#;

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for (n, data) in files {
                w.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(data.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn reads_libreoffice_style_document() {
        let bytes = zip_of(&[("mimetype", MIME), ("content.xml", LO_CONTENT), ("styles.xml", LO_STYLES)]);
        assert!(is_odt(&bytes));
        let d = read_odt(&bytes).unwrap();
        let i = &d.init;
        assert_eq!((i.page_width, i.page_height), (9922, 14031));
        assert_eq!((i.left, i.right, i.top, i.bottom), (1181, 1200, 945, 945));
        assert_eq!(i.font, FontSpec { name: "Liberation Serif".into(), size_cpt: 1200 });
        assert_eq!(i.justify, Justify::Full);
        // header with centered text and italic span; hidden footer ignored
        match &d.els[0] {
            El::Code(Code::HeadFoot { which: 0, occ: Occurrence::Every, els }) => {
                assert!(els.contains(&code(Code::Just(Justify::Center))));
                assert_eq!(attr_text(els, Attr::Italic), "kursiv");
            }
            e => panic!("{e:?}"),
        }
        assert!(!d.els.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: 2, .. }))));
        let t = text(&d);
        assert!(t.starts_with("Fett nicht fett  ende\nÜberschrift\nzweite\n1. eins\n2. zwei\u{c}Arial\n"), "{t:?}");
        // inherited bold (Wichtig → P1), switched off by span T1 which underlines
        assert_eq!(attr_text(&d.els, Attr::Bold), "Fett   ende");
        assert_eq!(attr_text(&d.els, Attr::Underline), "nicht fett");
        assert!(d.els.contains(&code(Code::LineSpacing(115))));
        assert!(d.els.contains(&code(Code::Font(FontSpec { name: "Arial".into(), size_cpt: 1400 }))));
        let tp = d.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        let tb = crate::tables::parse(&d.els, tp).unwrap();
        assert_eq!(tb.cols, vec![2400, 2400]);
        assert_eq!(tb.rows.iter().map(|r| r.len()).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(crate::rtf::tests::table_text(&d.els), "breit/a|b");
        balanced(&d.els);
    }

    #[test]
    fn never_panics_on_garbage() {
        let mut seed = 12345u64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let good = write_odt(&sample()).unwrap();
        for n in 0..200 {
            let mut b = good.clone();
            for _ in 0..(1 + n % 8) {
                let k = (rnd() as usize) % b.len();
                b[k] = rnd() as u8;
            }
            if n % 3 == 0 {
                b.truncate((rnd() as usize) % b.len());
            }
            let _ = is_odt(&b);
            let _ = read_odt(&b);
            let _ = is_fodt(&b);
            let _ = read_fodt(&b);
        }
        let x = write_fodt(&sample());
        for n in 0..200 {
            let mut b = x.clone().into_bytes();
            for _ in 0..(1 + n % 8) {
                let k = (rnd() as usize) % b.len();
                b[k] = b"<>/&\"' :x"[(rnd() % 9) as usize];
            }
            let _ = read_fodt(&b);
        }
        // pathological XML: deep nesting, bad entities, self-referencing deletion
        let deep = format!("{}{}", "<text:span>".repeat(5000), "x");
        let c = LO_CONTENT.replace("Fett  ", &deep);
        let _ = read_odt(&zip_of(&[("content.xml", &c)]));
        let _ = read_odt(&zip_of(&[("content.xml", "<office:document-content>&bogus;<<</")]));
        let selfref = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text><text:tracked-changes><text:changed-region text:id="c"><text:deletion><text:p>a<text:change text:change-id="c"/></text:p></text:deletion></text:changed-region></text:tracked-changes><text:p>x<text:change text:change-id="c"/><text:change-end text:change-id="zz"/></text:p></office:text></office:body></office:document-content>"#;
        let d = read_odt(&zip_of(&[("content.xml", selfref)])).unwrap();
        assert_eq!(text(&d), "xa");
        assert!(read_odt(b"not a zip").is_err());
        assert!(read_odt(&zip_of(&[("x", "y")])).is_err());
    }

    /// Writes target/odt_check/sample.{odt,ott,fodt} for checking with
    /// LibreOffice; when LibreOffice re-saved versions exist (`soffice
    /// --headless --convert-to odt --outdir lo sample.odt`, and `--outdir lof
    /// sample.fodt`) they must read back to the same code stream.
    #[test]
    fn libreoffice_check_files() {
        let mut d = Document::from_text("Größe äöü Привет été\tTab  zwei\n");
        d.els.insert(0, code(Code::HeadFoot { which: 0, occ: Occurrence::Every, els: chars("Kopf") }));
        d.els.insert(1, code(Code::HeadFoot { which: 2, occ: Occurrence::Every, els: chars("Fuß") }));
        d.els.push(code(Code::AttrOn(Attr::Bold)));
        d.els.extend(chars("FETT"));
        d.els.push(code(Code::AttrOff(Attr::Bold)));
        d.els.push(code(Code::Note { kind: NoteKind::Foot, els: chars("Fußnote") }));
        d.els.push(code(Code::Note { kind: NoteKind::End, els: chars("Endnote") }));
        d.els.push(code(Code::Comment("Kommentar".into())));
        d.els.push(rev(false));
        d.els.extend(chars("EINGEFUEGT"));
        d.els.push(code(Code::RevOff { del: false }));
        d.els.push(rev(true));
        d.els.extend(chars("GELOESCHT"));
        d.els.push(code(Code::RevOff { del: true }));
        d.els.push(code(Code::HRt));
        d.els.push(code(Code::TabSet(TabSet { stops: vec![TabStop { pos: 3000, kind: TabKind::Decimal, dot_leader: true }], relative: true })));
        d.els.extend(chars("x"));
        d.els.push(code(Code::Tab));
        d.els.extend(chars("1.5"));
        d.els.push(code(Code::HPg));
        d.els.push(code(Code::AttrOn(Attr::Large)));
        d.els.extend(chars("Seite zwei"));
        d.els.push(code(Code::AttrOff(Attr::Large)));
        d.els.push(code(Code::HRt));
        d.els.extend(table_doc().els);
        std::fs::create_dir_all("target/odt_check").unwrap();
        std::fs::write("target/odt_check/sample.odt", write_odt(&d).unwrap()).unwrap();
        std::fs::write("target/odt_check/sample.ott", write_ott(&d).unwrap()).unwrap();
        std::fs::write("target/odt_check/sample.fodt", write_fodt(&d)).unwrap();
        for f in ["target/odt_check/lo/sample.odt", "target/odt_check/lof/sample.odt"] {
            if let Ok(b) = std::fs::read(f) {
                let l = read_odt(&b).unwrap();
                assert_eq!(text(&l), text(&d));
                balanced(&l.els);
                let strip = |v: &[El]| -> Vec<El> {
                    v.iter()
                        .map(|e| match e {
                            El::Code(Code::RevOn { del, author, .. }) => El::Code(Code::RevOn { del: *del, author: author.clone(), date: String::new() }),
                            e => e.clone(),
                        })
                        .collect()
                };
                assert_eq!(strip(&l.els), strip(&d.els), "{f}");
            }
        }
    }
}
