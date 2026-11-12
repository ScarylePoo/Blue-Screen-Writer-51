//! Export: RTF (for any word processor), clean UTF-8 HTML (for e-mail) and
//! plain text in a chosen encoding (UTF-8, Windows-1252, DOS 850,
//! Windows-1251) for clients with old systems.

use crate::doc::*;

// ===================================================================== RTF

struct Rtf<'a> {
    doc: &'a Document,
    out: String,
    fonts: Vec<String>,
    authors: Vec<String>,
    attrs: AttrSet,
    font: FontSpec,
    just: Justify,
    left: i32,
    right: i32,
    spacing: u32,
    tabs: TabSet,
    rev: Option<(bool, usize)>,
}

fn rtf_text(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '\\' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\line "),
            '\t' => out.push_str("\\tab "),
            '\u{a0}' => out.push_str("\\~"),
            '\u{ad}' => out.push_str("\\-"),
            c if (c as u32) < 0x20 => {}
            c if (c as u32) < 0x80 => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
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

impl<'a> Rtf<'a> {
    fn font_idx(&mut self, name: &str) -> usize {
        match self.fonts.iter().position(|f| f == name) {
            Some(k) => k,
            None => {
                self.fonts.push(name.to_string());
                self.fonts.len() - 1
            }
        }
    }

    /// Full character state after `\plain` (used after groups close).
    fn char_state(&mut self) -> String {
        let f = self.font_idx(&self.font.name.clone());
        let mut s = format!("\\plain\\f{f}\\fs{}", self.font.size_cpt * size_pct(self.attrs) / 5000);
        for a in self.attrs.iter() {
            s.push_str(attr_on(a));
        }
        if let Some((del, au)) = self.rev {
            s.push_str(&if del { format!("\\deleted\\revauthdel{au}") } else { format!("\\revised\\revauth{au}") });
        }
        s.push(' ');
        s
    }

    fn para_start(&mut self, els: &[El], i: usize, intbl: bool) {
        // apply paragraph-level codes at the start of the paragraph
        let mut k = i;
        let mut just = None;
        let mut indent = 0;
        let mut dbl = false;
        while k < els.len() {
            match &els[k] {
                El::Code(Code::Just(j)) => self.just = *j,
                El::Code(Code::LRMargin { left, right }) => {
                    self.left = *left;
                    self.right = *right;
                }
                El::Code(Code::LineSpacing(s)) => self.spacing = *s,
                El::Code(Code::TabSet(t)) => self.tabs = t.clone(),
                El::Code(Code::Center) => {
                    just = Some(Justify::Center);
                    break;
                }
                El::Code(Code::FlushRight) => {
                    just = Some(Justify::Right);
                    break;
                }
                El::Code(Code::Indent) => indent += 1,
                El::Code(Code::DblIndent) => {
                    indent += 1;
                    dbl = true
                }
                El::Code(Code::AttrOn(_) | Code::AttrOff(_) | Code::Font(_) | Code::Bookmark(_) | Code::Comment(_) | Code::HeadFoot { .. }) => {}
                _ => break,
            }
            k += 1;
        }
        let q = match just.unwrap_or(self.just) {
            Justify::Left => "\\ql",
            Justify::Center => "\\qc",
            Justify::Right => "\\qr",
            Justify::Full => "\\qj",
        };
        let li = self.left - self.doc.init.left + indent * 600;
        let ri = self.right - self.doc.init.right + if dbl { indent * 600 } else { 0 };
        let tw = |w: i32| w as i64 * 1440 / 1200;
        let mut s = format!("\\pard{}{q}\\li{}\\ri{}\\sl{}\\slmult1", if intbl { "\\intbl" } else { "" }, tw(li), tw(ri), 240 * self.spacing / 100);
        let base = if self.tabs.relative { 0 } else { -self.left };
        for t in &self.tabs.stops {
            let pos = t.pos + base;
            if pos <= 0 {
                continue;
            }
            match t.kind {
                TabKind::Left => {}
                TabKind::Center => s.push_str("\\tqc"),
                TabKind::Right => s.push_str("\\tqr"),
                TabKind::Decimal => s.push_str("\\tqdec"),
            }
            if t.dot_leader {
                s.push_str("\\tldot");
            }
            s.push_str(&format!("\\tx{}", tw(pos)));
        }
        self.out.push_str(&s);
        self.out.push(' ');
    }

    fn els(&mut self, els: &[El], intbl: bool) {
        let mut i = 0;
        let mut para_start = true;
        let mut leading = true;
        let mut link_depth = 0;
        while i < els.len() {
            if para_start {
                self.para_start(els, i, intbl);
                para_start = false;
                leading = true;
            }
            match &els[i] {
                El::Ch(c) => {
                    leading = false;
                    rtf_text(&mut self.out, &c.to_string())
                }
                El::Code(c) => match c {
                    Code::HRt => {
                        self.out.push_str("\\par\n");
                        para_start = true;
                    }
                    Code::HPg => {
                        self.out.push_str("\\par\\page\n");
                        para_start = true;
                    }
                    Code::Tab => {
                        leading = false;
                        self.out.push_str("\\tab ")
                    }
                    Code::Indent | Code::DblIndent | Code::MarRel if leading => {}
                    Code::Indent | Code::DblIndent => self.out.push_str("\\tab "),
                    Code::HardSpace => self.out.push_str("\\~"),
                    Code::HardHyphen => self.out.push_str("\\_"),
                    Code::SoftHyphen => self.out.push_str("\\-"),
                    Code::AttrOn(a) | Code::AttrOff(a) => {
                        let on = matches!(c, Code::AttrOn(_));
                        let before = size_pct(self.attrs);
                        self.attrs.set(*a, on);
                        if size_pct(self.attrs) != before {
                            self.out.push_str(&format!("\\fs{} ", self.font.size_cpt * size_pct(self.attrs) / 5000));
                        } else {
                            self.out.push_str(if on { attr_on(*a) } else { attr_off(*a) });
                            self.out.push(' ');
                        }
                    }
                    Code::Font(f) => {
                        self.font = f.clone();
                        let k = self.font_idx(&f.name);
                        self.out.push_str(&format!("\\f{k}\\fs{} ", f.size_cpt * size_pct(self.attrs) / 5000));
                    }
                    Code::Just(_) | Code::LRMargin { .. } | Code::LineSpacing(_) | Code::TabSet(_) | Code::Center | Code::FlushRight => {}
                    Code::Note { kind, els: n } => {
                        let alt = if *kind == NoteKind::End { "\\ftnalt" } else { "" };
                        self.out.push_str(&format!("{{\\super\\chftn}}{{\\footnote{alt}\\pard\\plain {{\\super\\chftn}} "));
                        let saved = (self.attrs, self.font.clone(), self.rev);
                        self.attrs = AttrSet::default();
                        self.rev = None;
                        self.els(n, false);
                        self.out.push('}');
                        (self.attrs, self.font, self.rev) = saved;
                        let st = self.char_state();
                        self.out.push_str(&st);
                    }
                    Code::HeadFoot { .. } => {}
                    Code::TblDef { .. } => {
                        if let Some(t) = crate::tables::parse(els, i) {
                            self.table(els, &t);
                            i = t.off + 1;
                            if i < els.len() && matches!(els[i], El::Code(Code::HRt)) {
                                i += 1;
                            }
                            para_start = true;
                            continue;
                        }
                    }
                    Code::RevOn { del, author, .. } => {
                        let au = match self.authors.iter().position(|a| a == author) {
                            Some(k) => k + 1,
                            None => {
                                self.authors.push(author.clone());
                                self.authors.len()
                            }
                        };
                        self.rev = Some((*del, au));
                        self.out.push_str(&if *del { format!("\\deleted\\revauthdel{au} ") } else { format!("\\revised\\revauth{au} ") });
                    }
                    Code::RevOff { del } => {
                        self.rev = None;
                        self.out.push_str(if *del { "\\deleted0 " } else { "\\revised0 " });
                    }
                    Code::Link(u) => {
                        let mut inst = String::new();
                        match u.strip_prefix('#') {
                            Some(a) => rtf_text(&mut inst, &format!("HYPERLINK \\\\l \"{a}\"")),
                            None => rtf_text(&mut inst, &format!("HYPERLINK \"{u}\"")),
                        }
                        self.out.push_str(&format!("{{\\field{{\\*\\fldinst{{{inst}}}}}{{\\fldrslt{{"));
                        link_depth += 1;
                    }
                    Code::LinkEnd if link_depth > 0 => {
                        link_depth -= 1;
                        self.out.push_str("}}}");
                        let st = self.char_state();
                        self.out.push_str(&st);
                    }
                    Code::Bookmark(n) => {
                        let mut t = String::new();
                        rtf_text(&mut t, n);
                        self.out.push_str(&format!("{{\\*\\bkmkstart {t}}}{{\\*\\bkmkend {t}}}"));
                    }
                    Code::Comment(s) => {
                        let mut t = String::new();
                        rtf_text(&mut t, s);
                        self.out.push_str(&format!("{{\\*\\atnid BSW}}{{\\*\\atnauthor BSW}}\\chatn {{\\*\\annotation\\pard\\plain {t}}}"));
                    }
                    _ => {}
                },
            }
            i += 1;
        }
        for _ in 0..link_depth {
            self.out.push_str("}}}");
        }
    }

    fn table(&mut self, els: &[El], t: &crate::tables::Table) {
        let tw = |w: i32| w as i64 * 1440 / 1200;
        for r in 0..t.rows.len() {
            let mut x = 0;
            let mut row = String::from("\\trowd\\trgaph108");
            for (c, _) in t.rows[r].iter().enumerate() {
                x += t.cols.get(c).copied().unwrap_or(1200);
                row.push_str(&format!("\\clbrdrt\\brdrs\\clbrdrl\\brdrs\\clbrdrb\\brdrs\\clbrdrr\\brdrs\\cellx{}", tw(x)));
            }
            self.out.push_str(&row);
            self.out.push('\n');
            for c in 0..t.rows[r].len() {
                let (a, b) = t.cell_range(els, r, c);
                self.els(&els[a..b], true);
                if a == b {
                    self.para_start(els, a, true);
                }
                self.out.push_str("\\cell ");
            }
            self.out.push_str(&row);
            self.out.push_str("\\row\n");
        }
        self.out.push_str("\\pard ");
    }
}

fn attr_on(a: Attr) -> &'static str {
    match a {
        Attr::Bold => "\\b",
        Attr::Italic => "\\i",
        Attr::Underline => "\\ul",
        Attr::DblUnderline => "\\uldb",
        Attr::Outline => "\\outl",
        Attr::Shadow => "\\shad",
        Attr::SmallCaps => "\\scaps",
        Attr::Redline => "\\cf1",
        Attr::Strikeout => "\\strike",
        Attr::Superscript => "\\super",
        Attr::Subscript => "\\sub",
        _ => "",
    }
}

fn attr_off(a: Attr) -> &'static str {
    match a {
        Attr::Bold => "\\b0",
        Attr::Italic => "\\i0",
        Attr::Underline | Attr::DblUnderline => "\\ulnone",
        Attr::Outline => "\\outl0",
        Attr::Shadow => "\\shad0",
        Attr::SmallCaps => "\\scaps0",
        Attr::Redline => "\\cf0",
        Attr::Strikeout => "\\strike0",
        Attr::Superscript | Attr::Subscript => "\\nosupersub",
        _ => "",
    }
}

/// The document as RTF (Unicode-escaped, readable by Word, WordPad, LibreOffice).
pub fn to_rtf(doc: &Document) -> String {
    let mut base = doc.init.font.clone();
    for e in &doc.initial_codes {
        if let El::Code(Code::Font(f)) = e {
            base = f.clone();
        }
    }
    let mut w = Rtf {
        doc,
        out: String::new(),
        fonts: vec![base.name.clone()],
        authors: vec![],
        attrs: AttrSet::default(),
        font: base.clone(),
        just: doc.init.justify,
        left: doc.init.left,
        right: doc.init.right,
        spacing: doc.init.line_spacing,
        tabs: doc.init.tabs.clone(),
        rev: None,
    };
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            match c {
                Code::Just(j) => w.just = *j,
                Code::LRMargin { left, right } => {
                    w.left = *left;
                    w.right = *right
                }
                Code::LineSpacing(s) => w.spacing = *s,
                Code::TabSet(t) => w.tabs = t.clone(),
                _ => {}
            }
        }
    }
    // headers and footers first (RTF wants them at the section start)
    let mut hf = String::new();
    for e in &doc.els {
        if let El::Code(Code::HeadFoot { which, occ, els }) = e {
            if *occ == Occurrence::Discontinue {
                continue;
            }
            let kind = if *which < 2 { "header" } else { "footer" };
            let side = match occ {
                Occurrence::Odd => "r",
                Occurrence::Even => "l",
                _ => "",
            };
            let saved = std::mem::take(&mut w.out);
            w.els(els, false);
            let body = std::mem::replace(&mut w.out, saved);
            hf.push_str(&format!("{{\\{kind}{side}\\pard\\plain {body}\\par}}\n"));
        }
    }
    let st = w.char_state();
    w.out.push_str(&st);
    w.els(&doc.els, false);
    let body = std::mem::take(&mut w.out);
    let tw = |v: i32| v as i64 * 1440 / 1200;
    let i = &doc.init;
    let mut s = String::from("{\\rtf1\\ansi\\ansicpg1252\\uc1\\deff0\n{\\fonttbl");
    for (k, f) in w.fonts.iter().enumerate() {
        let mut n = String::new();
        rtf_text(&mut n, f);
        s.push_str(&format!("{{\\f{k}\\fnil\\fcharset0 {n};}}"));
    }
    s.push_str("}\n{\\colortbl;\\red192\\green0\\blue0;}\n");
    if !w.authors.is_empty() {
        s.push_str("{\\*\\revtbl {Unknown;}");
        for a in &w.authors {
            let mut n = String::new();
            rtf_text(&mut n, a);
            s.push_str(&format!("{{{n};}}"));
        }
        s.push_str("}\n");
    }
    let sm = &doc.summary;
    if !sm.descriptive_name.is_empty() || !sm.author.is_empty() {
        let mut t = String::new();
        rtf_text(&mut t, &sm.descriptive_name);
        let mut a = String::new();
        rtf_text(&mut a, &sm.author);
        s.push_str(&format!("{{\\info{{\\title {t}}}{{\\author {a}}}}}\n"));
    }
    s.push_str(&format!(
        "\\paperw{}\\paperh{}\\margl{}\\margr{}\\margt{}\\margb{}\\facingp\\ftnbj\\aenddoc\\ftnnar\\aftnnar\n",
        tw(i.page_width),
        tw(i.page_height),
        tw(i.left),
        tw(i.right),
        tw(i.top),
        tw(i.bottom)
    ));
    s.push_str(&hf);
    s.push_str(&body);
    s.push_str("\\par\n}");
    s
}

// ==================================================================== HTML

fn html_esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\u{a0}' => o.push_str("&nbsp;"),
            c => o.push(c),
        }
    }
    o
}

const HTML_TAGS: [(Attr, &str, &str); 8] = [
    (Attr::Bold, "<b>", "</b>"),
    (Attr::Italic, "<i>", "</i>"),
    (Attr::Underline, "<u>", "</u>"),
    (Attr::DblUnderline, "<u style=\"text-decoration-style:double\">", "</u>"),
    (Attr::Strikeout, "<s>", "</s>"),
    (Attr::Superscript, "<sup>", "</sup>"),
    (Attr::Subscript, "<sub>", "</sub>"),
    (Attr::SmallCaps, "<span style=\"font-variant:small-caps\">", "</span>"),
];

struct Html<'a> {
    doc: &'a Document,
    notes: Vec<(NoteKind, String)>,
    base: FontSpec,
}

#[derive(Clone, PartialEq)]
struct Inline {
    attrs: AttrSet,
    font: FontSpec,
    rev: Option<bool>,
    link: Option<String>,
}

impl<'a> Html<'a> {
    fn open(&self, st: &Inline) -> String {
        let mut s = String::new();
        if let Some(u) = &st.link {
            s.push_str(&format!("<a href=\"{}\">", html_esc(u)));
        }
        match st.rev {
            Some(true) => s.push_str("<del>"),
            Some(false) => s.push_str("<ins>"),
            None => {}
        }
        let pct = size_pct(st.attrs);
        if st.font != self.base || pct != 100 || st.attrs.has(Attr::Redline) {
            let mut style = String::new();
            if st.font.name != self.base.name {
                style.push_str(&format!("font-family:'{}';", html_esc(&st.font.name)));
            }
            if st.font.size_cpt != self.base.size_cpt || pct != 100 {
                let pt = st.font.size_cpt as f64 * pct as f64 / 10000.0;
                style.push_str(&format!("font-size:{}pt;", (pt * 10.0).round() / 10.0));
            }
            if st.attrs.has(Attr::Redline) {
                style.push_str("color:#c00000;");
            }
            s.push_str(&format!("<span style=\"{style}\">"));
        }
        for (a, o, _) in HTML_TAGS {
            if st.attrs.has(a) {
                s.push_str(o);
            }
        }
        s
    }

    fn close(&self, st: &Inline) -> String {
        let mut s = String::new();
        for (a, _, c) in HTML_TAGS.iter().rev() {
            if st.attrs.has(*a) {
                s.push_str(c);
            }
        }
        let pct = size_pct(st.attrs);
        if st.font != self.base || pct != 100 || st.attrs.has(Attr::Redline) {
            s.push_str("</span>");
        }
        match st.rev {
            Some(true) => s.push_str("</del>"),
            Some(false) => s.push_str("</ins>"),
            None => {}
        }
        if st.link.is_some() {
            s.push_str("</a>");
        }
        s
    }

    /// Paragraphs of `els`; `state` carries attributes across paragraphs.
    fn els(&mut self, els: &[El], st: &mut Inline, just: &mut Justify, in_cell: bool) -> String {
        let mut out = String::new();
        let mut i = 0;
        let n = els.len();
        while i < n || (i == 0 && n == 0) {
            if i < n {
                if let El::Code(Code::TblDef { .. }) = els[i] {
                    if let Some(t) = crate::tables::parse(els, i) {
                        out.push_str("<table style=\"border-collapse:collapse\">\n");
                        for r in 0..t.rows.len() {
                            out.push_str("<tr>");
                            for c in 0..t.rows[r].len() {
                                let (a, b) = t.cell_range(els, r, c);
                                let w = t.cols.get(c).copied().unwrap_or(1200) as f64 / 1200.0 * 96.0;
                                let mut j2 = *just;
                                let inner = self.els(&els[a..b], st, &mut j2, true);
                                out.push_str(&format!("<td style=\"border:1px solid #000;padding:2px 6px;vertical-align:top;width:{}px\">{inner}</td>", w.round()));
                            }
                            out.push_str("</tr>\n");
                        }
                        out.push_str("</table>\n");
                        i = t.off + 1;
                        if i < n && matches!(els[i], El::Code(Code::HRt)) {
                            i += 1;
                        }
                        continue;
                    }
                }
            }
            let mut j = i;
            while j < n && !matches!(els[j], El::Code(Code::HRt | Code::HPg | Code::TblDef { .. })) {
                j += 1;
            }
            // paragraph alignment / indent
            let mut pj = None;
            let mut indent = 0;
            let mut k = i;
            while k < j {
                match &els[k] {
                    El::Code(Code::Just(x)) => *just = *x,
                    El::Code(Code::Center) => {
                        pj = Some(Justify::Center);
                        break;
                    }
                    El::Code(Code::FlushRight) => {
                        pj = Some(Justify::Right);
                        break;
                    }
                    El::Code(Code::Indent | Code::DblIndent) => indent += 1,
                    El::Code(Code::AttrOn(_) | Code::AttrOff(_) | Code::Font(_) | Code::LRMargin { .. } | Code::LineSpacing(_) | Code::TabSet(_) | Code::Bookmark(_)) => {}
                    _ => break,
                }
                k += 1;
            }
            let align = match pj.unwrap_or(*just) {
                Justify::Left => "",
                Justify::Center => "text-align:center;",
                Justify::Right => "text-align:right;",
                Justify::Full => "text-align:justify;",
            };
            let ind = if indent > 0 { format!("margin-left:{}em;", indent * 2) } else { String::new() };
            let style = format!("{align}{ind}");
            let tag = if in_cell { "div" } else { "p" };
            if style.is_empty() {
                out.push_str(&format!("<{tag}>"));
            } else {
                out.push_str(&format!("<{tag} style=\"{style}\">"));
            }
            out.push_str(&self.open(st));
            let mut empty = true;
            let mut leading = true;
            for e in &els[i..j] {
                let mut next = st.clone();
                let mut emit = String::new();
                match e {
                    El::Ch(c) => {
                        emit = html_esc(&c.to_string());
                        leading = false;
                    }
                    El::Code(c) => match c {
                        Code::Tab => {
                            emit = "\t".into();
                            leading = false;
                        }
                        Code::Indent | Code::DblIndent if !leading => emit = "\t".into(),
                        Code::HardSpace => emit = "&nbsp;".into(),
                        Code::HardHyphen => emit = "\u{2011}".into(),
                        Code::SoftHyphen => emit = "&shy;".into(),
                        Code::AttrOn(a) => next.attrs.set(*a, true),
                        Code::AttrOff(a) => next.attrs.set(*a, false),
                        Code::Font(f) => next.font = f.clone(),
                        Code::RevOn { del, .. } => next.rev = Some(*del),
                        Code::RevOff { .. } => next.rev = None,
                        Code::Link(u) => next.link = Some(u.clone()),
                        Code::LinkEnd => next.link = None,
                        Code::Just(x) => *just = *x,
                        Code::Bookmark(b) => emit = format!("<a id=\"{}\"></a>", html_esc(b)),
                        Code::Comment(s) => emit = format!("<!-- {} -->", s.replace("--", "- -")),
                        Code::Note { kind, els: nels } => {
                            let mut nst = Inline { attrs: AttrSet::default(), font: self.base.clone(), rev: None, link: None };
                            let mut nj = Justify::Left;
                            let body = self.els(nels, &mut nst, &mut nj, true);
                            self.notes.push((*kind, body));
                            let num = self.notes.iter().filter(|(k, _)| k == kind).count();
                            let (pre, label) = if *kind == NoteKind::Foot { ("fn", num.to_string()) } else { ("en", roman(num)) };
                            emit = format!("<sup><a href=\"#{pre}{num}\" id=\"{pre}ref{num}\">{label}</a></sup>");
                        }
                        _ => {}
                    },
                }
                if next != *st {
                    out.push_str(&self.close(st));
                    *st = next;
                    out.push_str(&self.open(st));
                }
                if !emit.is_empty() {
                    empty = false;
                    out.push_str(&emit);
                }
            }
            out.push_str(&self.close(st));
            if empty && !in_cell {
                out.push_str("<br>");
            }
            out.push_str(&format!("</{tag}>\n"));
            if j < n && matches!(els[j], El::Code(Code::HPg)) {
                out.push_str("<div style=\"page-break-after:always\"></div>\n");
            }
            if j >= n {
                break;
            }
            i = if matches!(els[j], El::Code(Code::TblDef { .. })) { j } else { j + 1 };
            if i >= n {
                break;
            }
        }
        let _ = self.doc;
        out
    }
}

fn roman(mut n: usize) -> String {
    let t = [(10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
    let mut s = String::new();
    for (v, r) in t {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// The document as a clean, self-contained UTF-8 HTML page (for e-mail).
pub fn to_html(doc: &Document) -> String {
    let mut base = doc.init.font.clone();
    let mut just = doc.init.justify;
    for e in &doc.initial_codes {
        match e {
            El::Code(Code::Font(f)) => base = f.clone(),
            El::Code(Code::Just(j)) => just = *j,
            _ => {}
        }
    }
    let mut h = Html { doc, notes: vec![], base: base.clone() };
    let mut st = Inline { attrs: AttrSet::default(), font: base.clone(), rev: None, link: None };
    let body = h.els(&doc.els, &mut st, &mut just, false);
    let title = if doc.summary.descriptive_name.is_empty() { "Document".to_string() } else { doc.summary.descriptive_name.clone() };
    let pt = base.size_cpt as f64 / 100.0;
    let mut s = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>\nbody {{ font-family: '{}', serif; font-size: {pt}pt; }}\np, div {{ margin: 0; white-space: pre-wrap; tab-size: 4; }}\n.notes {{ font-size: 90%; }}\n</style>\n</head>\n<body>\n",
        html_esc(&title),
        html_esc(&base.name)
    );
    s.push_str(&body);
    for (kind, pre) in [(NoteKind::Foot, "fn"), (NoteKind::End, "en")] {
        let list: Vec<&String> = h.notes.iter().filter(|(k, _)| *k == kind).map(|(_, b)| b).collect();
        if list.is_empty() {
            continue;
        }
        let ty = if kind == NoteKind::End { " type=\"i\"" } else { "" };
        s.push_str(&format!("<hr>\n<ol class=\"notes\"{ty}>\n"));
        for (k, b) in list.iter().enumerate() {
            s.push_str(&format!("<li id=\"{pre}{}\">{b} <a href=\"#{pre}ref{}\">↩</a></li>\n", k + 1, k + 1));
        }
        s.push_str("</ol>\n");
    }
    s.push_str("</body>\n</html>\n");
    s
}

// ============================================================= plain text

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEnc {
    Utf8,
    Win1252,
    Dos850,
    Win1251,
}

impl TextEnc {
    pub const ALL: [TextEnc; 4] = [TextEnc::Utf8, TextEnc::Win1252, TextEnc::Dos850, TextEnc::Win1251];
    pub fn name(self) -> &'static str {
        match self {
            TextEnc::Utf8 => "UTF-8",
            TextEnc::Win1252 => "Windows-1252 (Western)",
            TextEnc::Dos850 => "DOS 850",
            TextEnc::Win1251 => "Windows-1251 (Cyrillic)",
        }
    }
}

/// Simple substitutes for characters a code page lacks.
fn fallback(c: char) -> &'static str {
    match c {
        '“' | '”' | '„' | '«' | '»' | '‟' => "\"",
        '‘' | '’' | '‚' | '‹' | '›' => "'",
        '–' | '—' | '‑' | '−' => "-",
        '…' => "...",
        '€' => "EUR",
        '\u{a0}' | '\u{202f}' | '\u{2009}' => " ",
        '•' => "*",
        '™' => "(TM)",
        '\u{ad}' => "",
        _ => "?",
    }
}

/// Encode text for a code page; returns the bytes and how many characters
/// had to be replaced.
pub fn encode(s: &str, enc: TextEnc) -> (Vec<u8>, usize) {
    if enc == TextEnc::Utf8 {
        return (s.as_bytes().to_vec(), 0);
    }
    let table: Vec<char> = (0x80u8..=0xFF)
        .map(|b| match enc {
            TextEnc::Dos850 => crate::fileio::CP850_HIGH.chars().nth((b - 0x80) as usize).unwrap_or('?'),
            TextEnc::Win1251 => crate::rtf::decode_byte(b, 1251),
            _ => crate::rtf::decode_byte(b, 1252),
        })
        .collect();
    let mut out = Vec::with_capacity(s.len());
    let mut lost = 0;
    for c in s.chars() {
        if (c as u32) < 0x80 {
            out.push(c as u8);
        } else if let Some(k) = table.iter().position(|&t| t == c) {
            out.push(0x80 + k as u8);
        } else {
            let f = fallback(c);
            if f == "?" {
                lost += 1;
            }
            for fc in f.chars() {
                match table.iter().position(|&t| t == fc) {
                    Some(k) if (fc as u32) >= 0x80 => out.push(0x80 + k as u8),
                    _ => out.push(fc as u8),
                }
            }
        }
    }
    (out, lost)
}

/// The document as plain text with CR/LF line ends in the given encoding.
pub fn to_text(doc: &Document, enc: TextEnc) -> (Vec<u8>, usize) {
    let s = doc.plain_text(0, doc.len()).replace('\n', "\r\n");
    encode(&s, enc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut d = Document::from_text("Grüße „Welt“ — Привет\nZweite Zeile\tTab");
        d.els.insert(0, El::Code(Code::AttrOn(Attr::Bold)));
        d.els.insert(6, El::Code(Code::AttrOff(Attr::Bold)));
        d.els.insert(0, El::Code(Code::Just(Justify::Center)));
        d.els.push(El::Code(Code::Note { kind: NoteKind::Foot, els: "Fußnote".chars().map(El::Ch).collect() }));
        d.els.push(El::Code(Code::Link("https://example.org/?a=1&b=2".into())));
        d.els.extend("Link".chars().map(El::Ch));
        d.els.push(El::Code(Code::LinkEnd));
        d
    }

    #[test]
    fn rtf_roundtrip_through_reader() {
        let mut d = sample();
        d.els.push(El::Code(Code::HRt));
        let mut t = crate::tables::new_table(2, 1, 4800);
        let k = t.iter().position(|e| *e == El::Code(Code::Cell)).unwrap();
        t.insert(k + 1, El::Ch('A'));
        d.els.extend(t);
        let r = to_rtf(&d);
        assert!(r.starts_with("{\\rtf1"));
        assert_eq!(r.matches('{').count() - r.matches("\\{").count(), r.matches('}').count() - r.matches("\\}").count(), "{r}");
        let d2 = crate::rtf::read(r.as_bytes()).unwrap();
        let txt = d2.plain_text(0, d2.len());
        assert!(txt.contains("Grüße „Welt“ — Привет"), "{txt:?}\n{r}");
        assert!(txt.contains("LinkA") || txt.contains("Link\nA") || txt.contains("A"), "{txt:?}");
        assert!(d2.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d2.els.iter().any(|e| matches!(e, El::Code(Code::Note { .. }))));
        assert!(d2.els.iter().any(|e| matches!(e, El::Code(Code::TblDef { .. }))), "{:?}", d2.els);
    }

    #[test]
    fn html_clean() {
        let h = to_html(&sample());
        assert!(h.contains("<meta charset=\"utf-8\">"));
        assert!(h.contains("<p style=\"text-align:center;\"><b>Grüße</b>"), "{h}");
        assert!(h.contains("<a href=\"https://example.org/?a=1&amp;b=2\">Link</a>"), "{h}");
        assert!(h.contains("id=\"fn1\"><div>Fußnote"), "{h}");
        assert_eq!(h.matches("<b>").count(), h.matches("</b>").count());
    }

    #[test]
    fn encodings() {
        let d = Document::from_text("Grüße „x“ €\nЖ");
        let (b, lost) = to_text(&d, TextEnc::Win1252);
        assert_eq!(lost, 1); // Ж
        assert_eq!(crate::fileio::decode_1252(&b), "Grüße „x“ €\r\n?");
        let (b, _) = to_text(&d, TextEnc::Dos850);
        assert_eq!(crate::fileio::decode_cp(&b, crate::fileio::CP850_HIGH), "Grüße \"x\" EUR\r\n?");
        let (b, lost) = to_text(&Document::from_text("Жена «да»"), TextEnc::Win1251);
        assert_eq!(lost, 0);
        assert_eq!(b[0], 0xC6);
        let (b, _) = to_text(&d, TextEnc::Utf8);
        assert_eq!(String::from_utf8(b).unwrap(), "Grüße „x“ €\r\nЖ");
    }
}
