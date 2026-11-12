//! HTML (.htm/.html, also Word- and LibreOffice-generated HTML) reader.
//!
//! A tolerant hand-written tokenizer feeds a `fmt_build::Builder`:
//! block elements become paragraphs (`[HRt]`), b/i/u/s/sup/sub and CSS
//! `font-weight` / `font-style` / `text-decoration` / `vertical-align`
//! become paired attribute codes, `align=` / `text-align` becomes
//! `[Just]`, headings are bold + Large/Very Large/Extra Large, tables
//! become `[Tbl Def]…[Tbl Off]`, lists become paragraphs with "• " /
//! "1. " (nested lists indented by Tab).  Character set from the BOM,
//! `<meta charset>`, `http-equiv` or the XML declaration, else detected.
//! Malformed input never panics: unknown or unbalanced tags are ignored.

use crate::doc::*;
use crate::fmt_build::Builder;
use crate::fmt_text::{self, Enc};

// ---------------------------------------------------------------------------
// entities

const LATIN1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf", "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro", "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest", "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute", "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute", "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute", "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil", "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
];

const GREEK_U: [&str; 25] = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa", "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi", "Rho", "", "Sigma", "Tau", "Upsilon", "Phi", "Chi", "Psi", "Omega"];
const GREEK_L: [&str; 25] = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigmaf", "sigma", "tau", "upsilon", "phi", "chi", "psi", "omega"];

const OTHER: &[(&str, char)] = &[
    ("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("apos", '\''),
    ("OElig", 'Œ'), ("oelig", 'œ'), ("Scaron", 'Š'), ("scaron", 'š'), ("Yuml", 'Ÿ'), ("fnof", 'ƒ'),
    ("circ", 'ˆ'), ("tilde", '˜'), ("ensp", '\u{2002}'), ("emsp", '\u{2003}'), ("thinsp", '\u{2009}'),
    ("zwnj", '\u{200c}'), ("zwj", '\u{200d}'), ("lrm", '\u{200e}'), ("rlm", '\u{200f}'),
    ("ndash", '–'), ("mdash", '—'), ("lsquo", '‘'), ("rsquo", '’'), ("sbquo", '‚'), ("ldquo", '“'),
    ("rdquo", '”'), ("bdquo", '„'), ("dagger", '†'), ("Dagger", '‡'), ("bull", '•'), ("hellip", '…'),
    ("permil", '‰'), ("prime", '′'), ("Prime", '″'), ("lsaquo", '‹'), ("rsaquo", '›'), ("oline", '‾'),
    ("frasl", '⁄'), ("euro", '€'), ("image", 'ℑ'), ("weierp", '℘'), ("real", 'ℜ'), ("trade", '™'),
    ("alefsym", 'ℵ'), ("larr", '←'), ("uarr", '↑'), ("rarr", '→'), ("darr", '↓'), ("harr", '↔'),
    ("crarr", '↵'), ("lArr", '⇐'), ("uArr", '⇑'), ("rArr", '⇒'), ("dArr", '⇓'), ("hArr", '⇔'),
    ("forall", '∀'), ("part", '∂'), ("exist", '∃'), ("empty", '∅'), ("nabla", '∇'), ("isin", '∈'),
    ("notin", '∉'), ("ni", '∋'), ("prod", '∏'), ("sum", '∑'), ("minus", '−'), ("lowast", '∗'),
    ("radic", '√'), ("prop", '∝'), ("infin", '∞'), ("ang", '∠'), ("and", '∧'), ("or", '∨'),
    ("cap", '∩'), ("cup", '∪'), ("int", '∫'), ("there4", '∴'), ("sim", '∼'), ("cong", '≅'),
    ("asymp", '≈'), ("ne", '≠'), ("equiv", '≡'), ("le", '≤'), ("ge", '≥'), ("sub", '⊂'),
    ("sup", '⊃'), ("nsub", '⊄'), ("sube", '⊆'), ("supe", '⊇'), ("oplus", '⊕'), ("otimes", '⊗'),
    ("perp", '⊥'), ("sdot", '⋅'), ("lceil", '⌈'), ("rceil", '⌉'), ("lfloor", '⌊'), ("rfloor", '⌋'),
    ("lang", '〈'), ("rang", '〉'), ("loz", '◊'), ("spades", '♠'), ("clubs", '♣'), ("hearts", '♥'),
    ("diams", '♦'), ("thetasym", 'ϑ'), ("upsih", 'ϒ'), ("piv", 'ϖ'), ("hyphen", '‐'), ("dash", '‐'),
    ("nbhy", '\u{2011}'), ("check", '✓'), ("star", '☆'), ("starf", '★'), ("numero", '№'),
];

/// A named entity (without `&` and `;`) → character.
pub fn entity(name: &str) -> Option<char> {
    if let Some(i) = LATIN1.iter().position(|n| *n == name) {
        return char::from_u32(0xA0 + i as u32);
    }
    if let Some(i) = GREEK_U.iter().position(|n| !n.is_empty() && *n == name) {
        return char::from_u32(0x391 + i as u32);
    }
    if let Some(i) = GREEK_L.iter().position(|n| *n == name) {
        return char::from_u32(0x3B1 + i as u32);
    }
    if let Some((_, c)) = OTHER.iter().find(|(n, _)| *n == name) {
        return Some(*c);
    }
    // a few common upper-case spellings (`&AMP;`, `&NBSP;`)
    let l = name.to_ascii_lowercase();
    if l != name && matches!(l.as_str(), "amp" | "lt" | "gt" | "quot" | "nbsp" | "copy" | "reg") {
        return entity(&l);
    }
    None
}

/// Decode an entity starting at `s[i] == '&'`; returns the text and the
/// number of chars consumed (0 = not an entity).
pub fn entity_at(s: &[char], i: usize) -> (Option<char>, usize) {
    let mut j = i + 1;
    if s.get(j) == Some(&'#') {
        j += 1;
        let hex = matches!(s.get(j), Some('x') | Some('X'));
        if hex {
            j += 1;
        }
        let st = j;
        while j < s.len() && j - st < 8 && (if hex { s[j].is_ascii_hexdigit() } else { s[j].is_ascii_digit() }) {
            j += 1;
        }
        if j == st {
            return (None, 0);
        }
        let num: String = s[st..j].iter().collect();
        let v = u32::from_str_radix(&num, if hex { 16 } else { 10 }).unwrap_or(0xFFFD);
        // Windows-1252 numeric references (&#150; etc.) as browsers do
        let c = if (0x80..0xA0).contains(&v) { fmt_text::byte_char(v as u8, Enc::Cp1252) } else { char::from_u32(v).filter(|&c| c != '\0').unwrap_or('\u{fffd}') };
        if s.get(j) == Some(&';') {
            j += 1;
        }
        return (Some(c), j - i);
    }
    let st = j;
    while j < s.len() && j - st < 12 && s[j].is_ascii_alphanumeric() {
        j += 1;
    }
    if j == st {
        return (None, 0);
    }
    let name: String = s[st..j].iter().collect();
    match entity(&name) {
        Some(c) => {
            if s.get(j) == Some(&';') {
                j += 1;
            }
            (Some(c), j - i)
        }
        None => (None, 0),
    }
}

/// Replace entities in a string.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let v: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < v.len() {
        if v[i] == '&' {
            let (c, n) = entity_at(&v, i);
            if let (Some(c), true) = (c, n > 0) {
                out.push(c);
                i += n;
                continue;
            }
        }
        out.push(v[i]);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// character set

fn find_label(lower: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(k) = lower[from..].find(key) {
        let s = from + k + key.len();
        let rest = lower[s..].trim_start_matches([' ', '=', '"', '\'']);
        let label: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')).collect();
        if !label.is_empty() {
            return Some(label);
        }
        from = s;
    }
    None
}

/// Decode the HTML bytes to text.
pub fn decode_html(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) || bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return fmt_text::decode_auto(bytes);
    }
    let head = &bytes[..bytes.len().min(4096)];
    let lower = String::from_utf8_lossy(head).to_ascii_lowercase();
    let label = find_label(&lower, "charset").or_else(|| if lower.starts_with("<?xml") { find_label(&lower, "encoding") } else { None });
    if let Some(enc) = label.as_deref().and_then(fmt_text::enc_for_label) {
        match enc {
            Enc::Utf8 => {
                if std::str::from_utf8(bytes).is_ok() {
                    return fmt_text::decode(bytes, Enc::Utf8);
                }
            }
            // a UTF-16 label in a file we could read as ASCII is wrong
            Enc::Utf16Le | Enc::Utf16Be => {}
            e => return fmt_text::decode(bytes, e),
        }
    }
    fmt_text::decode_auto(bytes)
}

// ---------------------------------------------------------------------------
// tokenizer

#[derive(Debug, PartialEq)]
enum Tok {
    Text(String),
    /// name (lower case), attributes (lower-case names, decoded values), self-closing
    Open(String, Vec<(String, String)>, bool),
    Close(String),
}

const RAW: [&str; 5] = ["script", "style", "title", "textarea", "xmp"];

fn tokenize(s: &[char]) -> Vec<Tok> {
    let mut out = vec![];
    let mut text = String::new();
    let mut i = 0;
    let n = s.len();
    let starts = |i: usize, p: &str| -> bool {
        let mut k = i;
        for pc in p.chars() {
            match s.get(k) {
                Some(c) if c.to_ascii_lowercase() == pc => k += 1,
                _ => return false,
            }
        }
        true
    };
    while i < n {
        let c = s[i];
        if c != '<' {
            if c == '&' {
                let (e, k) = entity_at(s, i);
                if let (Some(e), true) = (e, k > 0) {
                    text.push(e);
                    i += k;
                    continue;
                }
            }
            text.push(c);
            i += 1;
            continue;
        }
        // comment
        if starts(i, "<!--") {
            let mut k = i + 4;
            while k < n && !(s[k] == '-' && s.get(k + 1) == Some(&'-') && s.get(k + 2) == Some(&'>')) {
                k += 1;
            }
            i = (k + 3).min(n);
            continue;
        }
        // <!DOCTYPE>, <![if …]>, <![endif]>, <?xml …?>
        if starts(i, "<!") || starts(i, "<?") {
            let mut k = i + 2;
            while k < n && s[k] != '>' {
                k += 1;
            }
            i = (k + 1).min(n);
            continue;
        }
        let closing = s.get(i + 1) == Some(&'/');
        let mut k = i + 1 + closing as usize;
        let name_start = k;
        while k < n && (s[k].is_alphanumeric() || matches!(s[k], ':' | '-' | '_')) {
            k += 1;
        }
        if k == name_start {
            // not a tag: a literal '<'
            text.push('<');
            i += 1;
            continue;
        }
        let name: String = s[name_start..k].iter().collect::<String>().to_ascii_lowercase();
        // attributes
        let mut attrs = vec![];
        let mut self_close = false;
        loop {
            while k < n && s[k].is_whitespace() {
                k += 1;
            }
            if k >= n {
                break;
            }
            if s[k] == '>' {
                k += 1;
                break;
            }
            if s[k] == '/' {
                self_close = true;
                k += 1;
                continue;
            }
            let an_st = k;
            while k < n && !s[k].is_whitespace() && !matches!(s[k], '=' | '>' | '/') {
                k += 1;
            }
            if k == an_st {
                k += 1;
                continue;
            }
            let an: String = s[an_st..k].iter().collect::<String>().to_ascii_lowercase();
            while k < n && s[k].is_whitespace() {
                k += 1;
            }
            let mut val = String::new();
            if k < n && s[k] == '=' {
                k += 1;
                while k < n && s[k].is_whitespace() {
                    k += 1;
                }
                if k < n && (s[k] == '"' || s[k] == '\'') {
                    let q = s[k];
                    k += 1;
                    let st = k;
                    while k < n && s[k] != q {
                        k += 1;
                    }
                    val = s[st..k.min(n)].iter().collect();
                    k = (k + 1).min(n);
                } else {
                    let st = k;
                    while k < n && !s[k].is_whitespace() && s[k] != '>' {
                        k += 1;
                    }
                    val = s[st..k].iter().collect();
                }
            }
            attrs.push((an, decode_entities(&val)));
        }
        i = k;
        if !text.is_empty() {
            out.push(Tok::Text(std::mem::take(&mut text)));
        }
        if closing {
            out.push(Tok::Close(name));
            continue;
        }
        let raw = RAW.contains(&name.as_str()) && !self_close;
        out.push(Tok::Open(name.clone(), attrs, self_close));
        if raw {
            // raw text up to </name
            let close = format!("</{name}");
            let mut e = i;
            while e < n && !starts(e, &close) {
                e += 1;
            }
            let body: String = s[i..e].iter().collect();
            if name == "title" || name == "textarea" {
                out.push(Tok::Text(decode_entities(&body)));
            }
            out.push(Tok::Close(name));
            let mut k = e;
            while k < n && s[k] != '>' {
                k += 1;
            }
            i = (k + 1).min(n);
        }
    }
    if !text.is_empty() {
        out.push(Tok::Text(text));
    }
    out
}

// ---------------------------------------------------------------------------
// conversion

const VOID: [&str; 15] = ["br", "hr", "img", "meta", "link", "input", "col", "area", "base", "wbr", "param", "source", "basefont", "embed", "track"];
const BLOCK: [&str; 32] = [
    "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "ul", "ol", "dl", "dt", "dd", "blockquote", "pre", "address", "center", "section", "article", "header", "footer", "nav", "aside", "main", "figure", "figcaption", "form", "fieldset", "table", "hr", "body",
];

#[derive(Default, Clone)]
struct Open {
    tag: String,
    on: AttrSet,
    off: AttrSet,
    just: Option<Justify>,
    skip: bool,
    mono: bool,
    pre: bool,
}

struct ListCtx {
    ordered: bool,
    next: i64,
}

struct TableCtx {
    def_at: Option<usize>,
    rows: usize,
    in_cell: bool,
    span_extra: usize,
    widths: Vec<i32>,
    col_widths: Vec<i32>,
    width_ok: bool,
}

struct Conv {
    b: Builder,
    stack: Vec<Open>,
    lists: Vec<ListCtx>,
    tables: Vec<TableCtx>,
    space: bool,
    /// attributes in effect where the pending space was seen
    space_attrs: AttrSet,
    quote: usize,
    title: String,
    in_title: bool,
    in_head: bool,
    base: FontSpec,
    /// footnote/endnote texts by (endnote?, number), from their definitions
    notes: std::collections::HashMap<(bool, u32), Vec<El>>,
}

fn parse_style(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase()))
        })
        .collect()
}

fn just_of(v: &str) -> Option<Justify> {
    Some(match v.trim() {
        "center" | "middle" => Justify::Center,
        "right" | "end" => Justify::Right,
        "justify" => Justify::Full,
        "left" | "start" => Justify::Left,
        _ => return None,
    })
}

/// HTML length → WPU (px at 96 dpi, pt, in, cm, mm, % of `full`).
fn length_wpu(v: &str, full: i32) -> Option<i32> {
    let v = v.trim().to_ascii_lowercase();
    let num: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let x: f64 = num.parse().ok()?;
    let unit = v[num.len()..].trim();
    let w = match unit {
        "" | "px" => x * 12.5,
        "pt" => x * 1200.0 / 72.0,
        "in" => x * 1200.0,
        "cm" => x * 1200.0 / 2.54,
        "mm" => x * 120.0 / 2.54,
        "%" => x * full as f64 / 100.0,
        _ => return None,
    };
    let w = w.round() as i32;
    (w > 0 && w < 1200 * 40).then_some(w)
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

impl Conv {
    fn recompute(&mut self) {
        let mut a = AttrSet::default();
        let mut just = Justify::Left;
        let mut mono = false;
        for o in &self.stack {
            a.0 |= o.on.0;
            a.0 &= !o.off.0;
            if let Some(j) = o.just {
                just = j;
            }
            mono |= o.mono;
        }
        self.b.attrs = a;
        self.b.just = just;
        self.b.font = if mono { FontSpec { name: "Courier New".into(), size_cpt: self.base.size_cpt } } else { self.base.clone() };
    }

    fn skipping(&self) -> bool {
        self.in_head || self.stack.iter().any(|o| o.skip)
    }

    fn pre(&self) -> bool {
        self.stack.iter().any(|o| o.pre)
    }

    /// Text outside table cells (between `<table>` and `<td>`) is dropped.
    fn table_gap(&self) -> bool {
        self.tables.last().is_some_and(|t| !t.in_cell)
    }

    /// End the current paragraph if it has content.
    fn block_break(&mut self) {
        if !self.b.at_para_start() {
            self.b.para();
        }
        self.space = false;
    }

    /// Write a pending (collapsed) space with the attributes it had.
    fn emit_space(&mut self) {
        if self.space && !self.b.at_para_start() {
            let now = self.b.attrs;
            self.b.attrs = AttrSet(self.space_attrs.0 & now.0);
            self.b.ch(' ');
            self.b.attrs = now;
        }
        self.space = false;
    }

    fn put(&mut self, c: char) {
        if self.b.at_para_start() && self.quote > 0 && !self.b.in_table() {
            for _ in 0..self.quote.min(8) {
                self.b.code(Code::Indent);
            }
        }
        self.b.ch(c);
    }

    fn text(&mut self, t: &str) {
        if self.in_title {
            self.title.push_str(t);
            return;
        }
        if self.skipping() || self.table_gap() {
            // Word: `<p><o:p>&nbsp;</o:p></p>` is an empty line
            if !self.in_head && self.stack.last().is_some_and(|o| o.tag == "o:p") && !self.stack[..self.stack.len() - 1].iter().any(|o| o.skip) && t.contains('\u{a0}') && t.chars().all(char::is_whitespace) && self.b.at_para_start() && !self.table_gap() {
                self.b.para();
            }
            return;
        }
        if self.pre() {
            for c in t.chars() {
                match c {
                    '\r' => {}
                    '\n' => {
                        self.b.para();
                    }
                    c => self.put(c),
                }
            }
            return;
        }
        for c in t.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                if !self.space {
                    self.space_attrs = self.b.attrs;
                }
                self.space = true;
                continue;
            }
            self.emit_space();
            self.space = false;
            self.put(c);
        }
    }

    fn pop_to(&mut self, idx: usize) {
        while self.stack.len() > idx {
            let o = self.stack.pop().unwrap_or_default();
            self.after_close(&o.tag);
        }
        self.recompute();
    }

    /// Close the innermost open `tag`, unless a boundary element is
    /// closer to the top of the stack.
    fn close_tag(&mut self, tag: &str, boundary: &[&str]) {
        for i in (0..self.stack.len()).rev() {
            if self.stack[i].tag == tag {
                self.pop_to(i);
                return;
            }
            if boundary.contains(&self.stack[i].tag.as_str()) {
                return;
            }
        }
    }

    /// Structural effects of closing an element.
    fn after_close(&mut self, tag: &str) {
        match tag {
            "td" | "th" => {
                if let Some(t) = self.tables.last_mut() {
                    t.in_cell = false;
                }
            }
            "table" => self.end_table(),
            "ul" | "ol" => {
                self.lists.pop();
                self.block_break();
            }
            "blockquote" => {
                self.block_break();
                self.quote = self.quote.saturating_sub(1);
            }
            t if BLOCK.contains(&t) || t == "tr" => self.block_break(),
            _ => {}
        }
    }

    fn flush_spans(&mut self) {
        if let Some(t) = self.tables.last_mut() {
            let n = std::mem::take(&mut t.span_extra);
            for _ in 0..n {
                self.b.cell();
            }
        }
    }

    fn end_table(&mut self) {
        self.flush_spans();
        let Some(t) = self.tables.pop() else { return };
        if let Some(at) = t.def_at {
            let widths = if !t.col_widths.is_empty() { t.col_widths } else if t.width_ok { t.widths } else { vec![] };
            if !widths.is_empty() {
                if let Some(El::Code(Code::TblDef { cols })) = self.b.els.get_mut(at) {
                    *cols = widths;
                }
            }
        }
        self.b.table_end();
        self.space = false;
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)], self_close: bool) {
        let style = attr(attrs, "style").map(parse_style).unwrap_or_default();
        let sty = |k: &str| style.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        match name {
            "head" => {
                self.in_head = true;
                return;
            }
            "body" => {
                self.in_head = false;
                return;
            }
            "title" => {
                self.in_title = true;
                return;
            }
            "br" => {
                if !self.skipping() && !self.table_gap() {
                    self.b.para();
                    self.space = false;
                }
                return;
            }
            "hr" => {
                if !self.skipping() {
                    self.block_break();
                    self.b.text(&"─".repeat(40));
                    self.b.para();
                }
                return;
            }
            "a" if !self_close => {
                if let Some((end, num)) = attr(attrs, "href").and_then(note_key) {
                    if let Some(els) = self.notes.remove(&(end, num)) {
                        if !self.skipping() && !self.table_gap() {
                            self.emit_space();
                            self.b.note(if end { NoteKind::End } else { NoteKind::Foot }, els);
                        }
                        self.stack.push(Open { tag: "a".into(), skip: true, ..Default::default() });
                        return;
                    }
                }
            }
            "img" => {
                if let Some(alt) = attr(attrs, "alt").filter(|a| !a.trim().is_empty()) {
                    let alt = format!("[{}]", alt.trim());
                    self.text(&alt);
                }
                return;
            }
            "col" => {
                if let (Some(t), Some(w)) = (self.tables.last_mut(), attr(attrs, "width").or(sty("width"))) {
                    let span: usize = attr(attrs, "span").and_then(|s| s.parse().ok()).unwrap_or(1).clamp(1, 64);
                    if let Some(w) = length_wpu(w, 7800) {
                        for _ in 0..span {
                            t.col_widths.push(w);
                        }
                    }
                }
                return;
            }
            _ => {}
        }
        if VOID.contains(&name) {
            return;
        }
        // implied end tags
        if BLOCK.contains(&name) {
            self.close_tag("p", &["td", "th", "li", "table", "blockquote", "div"]);
        }
        match name {
            "li" => self.close_tag("li", &["ul", "ol", "table"]),
            "dt" | "dd" => {
                self.close_tag("dt", &["dl", "table"]);
                self.close_tag("dd", &["dl", "table"]);
            }
            "td" | "th" => {
                self.close_tag("td", &["tr", "table"]);
                self.close_tag("th", &["tr", "table"]);
            }
            "tr" => self.close_tag("tr", &["table"]),
            _ => {}
        }
        if self_close {
            return;
        }
        let mut o = Open { tag: name.to_string(), ..Default::default() };
        let size = |n: &str| match n {
            "h1" => Some(Attr::ExtraLarge),
            "h2" => Some(Attr::VeryLarge),
            "h3" => Some(Attr::Large),
            _ => None,
        };
        match name {
            "b" | "strong" => o.on.set(Attr::Bold, true),
            "i" | "em" | "cite" | "dfn" | "var" => o.on.set(Attr::Italic, true),
            "u" | "ins" => o.on.set(Attr::Underline, true),
            "s" | "strike" | "del" => o.on.set(Attr::Strikeout, true),
            "sup" => o.on.set(Attr::Superscript, true),
            "sub" => o.on.set(Attr::Subscript, true),
            "small" => o.on.set(Attr::Small, true),
            "big" => o.on.set(Attr::Large, true),
            "code" | "tt" | "kbd" | "samp" => o.mono = true,
            "pre" | "xmp" | "listing" | "plaintext" => {
                o.mono = true;
                o.pre = true;
            }
            "center" => o.just = Some(Justify::Center),
            "th" => o.on.set(Attr::Bold, true),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                o.on.set(Attr::Bold, true);
                if let Some(a) = size(name) {
                    o.on.set(a, true);
                }
            }
            "script" | "style" | "o:p" | "caption" | "noscript" | "template" | "select" | "object" | "iframe" | "svg" | "math" => o.skip = true,
            "textarea" => o.pre = true,
            _ => {}
        }
        if let Some(j) = attr(attrs, "align").and_then(just_of) {
            if name != "table" && name != "img" {
                o.just = Some(j);
            }
        }
        if let Some(j) = sty("text-align").and_then(just_of) {
            o.just = Some(j);
        }
        if let Some(w) = sty("font-weight") {
            let bold = matches!(w, "bold" | "bolder") || w.parse::<u32>().is_ok_and(|n| n >= 600);
            if bold {
                o.on.set(Attr::Bold, true);
            } else if matches!(w, "normal" | "lighter") || w.parse::<u32>().is_ok() {
                o.off.set(Attr::Bold, true);
            }
        }
        if let Some(s) = sty("font-style") {
            if s.starts_with("italic") || s.starts_with("oblique") {
                o.on.set(Attr::Italic, true);
            } else if s == "normal" {
                o.off.set(Attr::Italic, true);
            }
        }
        for k in ["text-decoration", "text-decoration-line"] {
            if let Some(d) = sty(k) {
                if d.contains("underline") {
                    o.on.set(if d.contains("double") { Attr::DblUnderline } else { Attr::Underline }, true);
                }
                if d.contains("line-through") {
                    o.on.set(Attr::Strikeout, true);
                }
                if d.contains("none") {
                    o.off.set(Attr::Underline, true);
                    o.off.set(Attr::Strikeout, true);
                }
            }
        }
        if let Some(v) = sty("vertical-align") {
            if v.starts_with("super") {
                o.on.set(Attr::Superscript, true);
            } else if v.starts_with("sub") {
                o.on.set(Attr::Subscript, true);
            }
        }
        if sty("font-variant").is_some_and(|v| v.contains("small-caps")) {
            o.on.set(Attr::SmallCaps, true);
        }
        if sty("display") == Some("none") || sty("mso-hide") == Some("all") {
            o.skip = true;
        }
        if sty("font-family").is_some_and(|f| f.contains("courier") || f.contains("monospace")) {
            o.mono = true;
        }
        let tabs = sty("mso-tab-count").and_then(|v| v.parse::<usize>().ok());

        // structural effects of opening
        let skipping = self.skipping();
        if BLOCK.contains(&name) && !skipping {
            self.block_break();
        }
        match name {
            "ul" | "ol" if !skipping => {
                let start = attr(attrs, "start").and_then(|s| s.trim().parse().ok()).unwrap_or(1);
                self.lists.push(ListCtx { ordered: name == "ol", next: start });
            }
            "blockquote" if !skipping => self.quote += 1,
            "table" if !skipping => {
                let before = self.b.els.len();
                self.b.table_start(vec![]);
                let def_at = (self.b.els.len() > before && matches!(self.b.els.last(), Some(El::Code(Code::TblDef { .. })))).then(|| self.b.els.len() - 1);
                self.tables.push(TableCtx { def_at, rows: 0, in_cell: false, span_extra: 0, widths: vec![], col_widths: vec![], width_ok: true });
            }
            "tr" if !skipping && !self.tables.is_empty() => {
                self.flush_spans();
                self.b.row();
                if let Some(t) = self.tables.last_mut() {
                    t.rows += 1;
                    t.in_cell = false;
                }
            }
            "td" | "th" if !skipping && !self.tables.is_empty() => {
                self.flush_spans();
                if self.tables.last().is_some_and(|t| t.rows == 0) {
                    self.b.row();
                    if let Some(t) = self.tables.last_mut() {
                        t.rows = 1;
                    }
                }
                self.b.cell();
                let span: usize = attr(attrs, "colspan").and_then(|s| s.trim().parse().ok()).unwrap_or(1).clamp(1, 64);
                let w = attr(attrs, "width").or(sty("width")).and_then(|w| length_wpu(w, 7800));
                if let Some(t) = self.tables.last_mut() {
                    t.in_cell = true;
                    t.span_extra = span - 1;
                    if t.rows == 1 {
                        match w {
                            Some(w) if span == 1 => t.widths.push(w),
                            _ => t.width_ok = false,
                        }
                    }
                }
                self.space = false;
            }
            _ => {}
        }
        self.stack.push(o);
        self.recompute();
        if name == "li" && !self.skipping() {
            let depth = self.lists.len().max(1);
            for _ in 1..depth.min(8) {
                self.b.code_fmt(Code::Tab);
            }
            let marker = match self.lists.last_mut() {
                Some(l) if l.ordered => {
                    let v = attr(attrs, "value").and_then(|s| s.trim().parse().ok()).unwrap_or(l.next);
                    l.next = v + 1;
                    format!("{v}. ")
                }
                _ => "• ".to_string(),
            };
            self.put_str(&marker);
            self.space = false;
        }
        if let Some(n) = tabs {
            if !self.skipping() && !self.table_gap() {
                for _ in 0..n.min(20) {
                    self.b.code_fmt(Code::Tab);
                }
                self.space = false;
            }
            if let Some(top) = self.stack.last_mut() {
                top.skip = true;
            }
        }
    }

    fn put_str(&mut self, s: &str) {
        for c in s.chars() {
            self.put(c);
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "head" => {
                self.in_head = false;
                return;
            }
            "title" => {
                self.in_title = false;
                return;
            }
            "p" => {
                // `</p>` without `<p>`: an (empty) paragraph in browsers
                if !self.stack.iter().any(|o| o.tag == "p") {
                    self.block_break();
                    return;
                }
            }
            _ => {}
        }
        let boundary: &[&str] = match name {
            "td" | "th" | "tr" => &["table"],
            "li" => &["ul", "ol", "table"],
            "table" => &[],
            "p" => &["td", "th", "li", "table", "blockquote"],
            _ => &["td", "th", "table"],
        };
        self.close_tag(name, boundary);
    }
}

/// `#sdfootnote1sym`, `#_ftn1`, `sdendnote2`, `edn2` → (endnote?, number).
fn note_key(href: &str) -> Option<(bool, u32)> {
    let h = href.trim().trim_start_matches('#').trim_start_matches('_').to_ascii_lowercase();
    let (end, rest) = if let Some(r) = h.strip_prefix("sdfootnote").or_else(|| h.strip_prefix("ftn")) {
        (false, r)
    } else if let Some(r) = h.strip_prefix("sdendnote").or_else(|| h.strip_prefix("edn")) {
        (true, r)
    } else {
        return None;
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &rest[digits.len()..];
    if !(rest.is_empty() || rest == "sym" || rest == "anc") {
        return None;
    }
    digits.parse().ok().map(|n| (end, n))
}

/// What a `<div>` holds that does not belong into the running text.
enum Extract {
    Note(bool, u32),
    /// 0 = header, 2 = footer (LibreOffice `<div title="header">`)
    HeadFoot(u8),
    Drop,
}

fn extract_kind(attrs: &[(String, String)]) -> Option<Extract> {
    if let Some(k) = attr(attrs, "id").and_then(note_key) {
        return Some(Extract::Note(k.0, k.1));
    }
    match attr(attrs, "title").map(|t| t.to_ascii_lowercase()).as_deref() {
        Some("header") => return Some(Extract::HeadFoot(0)),
        Some("footer") => return Some(Extract::HeadFoot(2)),
        _ => {}
    }
    let style = attr(attrs, "style").unwrap_or("").to_ascii_lowercase();
    if style.contains("mso-element:header") {
        return Some(Extract::HeadFoot(0));
    }
    if style.contains("mso-element:footer") {
        return Some(Extract::HeadFoot(2));
    }
    if style.contains("mso-element:footnote-separator") || style.contains("mso-element:endnote-separator") || style.contains("mso-element:footnote-continuation") {
        return Some(Extract::Drop);
    }
    None
}

fn new_conv(base: &FontSpec) -> Conv {
    Conv { b: Builder::new(base.clone()), stack: vec![], lists: vec![], tables: vec![], space: false, space_attrs: AttrSet::default(), quote: 0, title: String::new(), in_title: false, in_head: false, base: base.clone(), notes: Default::default() }
}

fn run(c: &mut Conv, toks: Vec<Tok>) {
    for t in toks {
        match t {
            Tok::Text(s) => c.text(&s),
            Tok::Open(n, a, sc) => c.open(&n, &a, sc),
            Tok::Close(n) => c.close(&n),
        }
    }
    c.pop_to(0);
    while !c.tables.is_empty() {
        c.end_table();
    }
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let text = decode_html(bytes);
    let chars: Vec<char> = text.chars().collect();
    let toks = tokenize(&chars);
    let base = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    // take note texts and headers/footers out of the running text
    let mut main = vec![];
    let mut notes = std::collections::HashMap::new();
    let mut heads: Vec<El> = vec![];
    let mut it = toks.into_iter();
    let mut drop_hr = false;
    while let Some(t) = it.next() {
        match &t {
            Tok::Open(n, a, _) if n == "div" && attr(a, "style").is_some_and(|s| s.contains("footnote-list") || s.contains("endnote-list")) => drop_hr = true,
            Tok::Open(n, _, _) if n == "hr" && drop_hr => {
                drop_hr = false;
                continue;
            }
            _ => {}
        }
        let kind = match &t {
            Tok::Open(n, a, false) if n == "div" => extract_kind(a),
            _ => None,
        };
        let Some(kind) = kind else {
            main.push(t);
            continue;
        };
        let mut depth = 1;
        let mut inner = vec![];
        for t in it.by_ref() {
            match &t {
                Tok::Open(n, _, false) if n == "div" => depth += 1,
                Tok::Close(n) if n == "div" => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            inner.push(t);
        }
        if let Extract::Note(..) = kind {
            // drop the back-reference number (the first <a>…</a>)
            if let Some(a) = inner.iter().position(|t| matches!(t, Tok::Open(n, _, false) if n == "a")) {
                let e = inner[a..].iter().position(|t| matches!(t, Tok::Close(n) if n == "a")).map(|k| a + k).unwrap_or(a);
                inner.drain(a..=e);
            }
        }
        let mut sub = new_conv(&base);
        run(&mut sub, inner);
        let els = sub.b.finish();
        match kind {
            Extract::Note(end, n) => {
                notes.insert((end, n), els);
            }
            Extract::HeadFoot(which) => {
                if !els.is_empty() && !heads.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) {
                    heads.push(El::Code(Code::HeadFoot { which, occ: Occurrence::Every, els }));
                }
            }
            Extract::Drop => {}
        }
    }
    let mut c = new_conv(&base);
    c.notes = notes;
    run(&mut c, main);
    let mut d = c.b.into_document(base);
    d.summary.descriptive_name = c.title.split_whitespace().collect::<Vec<_>>().join(" ");
    if !heads.is_empty() {
        heads.extend(std::mem::take(&mut d.els));
        d.els = heads;
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    fn rd(s: &str) -> Document {
        read(s.as_bytes()).unwrap()
    }
    fn has(d: &Document, c: Code) -> bool {
        d.els.contains(&El::Code(c))
    }

    #[test]
    fn entity_tables() {
        assert_eq!(entity("nbsp"), Some('\u{a0}'));
        assert_eq!(entity("yuml"), Some('ÿ'));
        assert_eq!(entity("szlig"), Some('ß'));
        assert_eq!(entity("Omega"), Some('Ω'));
        assert_eq!(entity("sigmaf"), Some('ς'));
        assert_eq!(entity("omega"), Some('ω'));
        assert_eq!(decode_entities("a&amp;b &euro;&#8222;&#x44f;&#150;&unknown; &"), "a&b €„я–&unknown; &");
    }

    #[test]
    fn paragraphs_attrs_headings() {
        let d = rd("<html><head><title>Mein  Titel</title><style>p{color:red}</style></head><body>\
            <h1>Überschrift</h1><p>Grüße <b>fett</b> <i>kursiv</i> <u>unter</u> <s>durch</s> H<sub>2</sub>O x<sup>2</sup></p>\
            <p align=center>Привет мир</p><p style='text-align:right'>rechts</p></body></html>");
        assert_eq!(d.summary.descriptive_name, "Mein Titel");
        assert_eq!(debug_text(&d.els), "Überschrift\nGrüße fett kursiv unter durch H2O x2\nПривет мир\nrechts");
        for a in [Attr::Bold, Attr::ExtraLarge, Attr::Italic, Attr::Underline, Attr::Strikeout, Attr::Subscript, Attr::Superscript] {
            assert!(has(&d, Code::AttrOn(a)), "{a:?}");
        }
        assert!(has(&d, Code::Just(Justify::Center)));
        assert!(has(&d, Code::Just(Justify::Right)));
    }

    #[test]
    fn whitespace_and_breaks() {
        let d = rd("<p>  a\n   b<br>c </p>\n<div>d</div>text&nbsp;x");
        assert_eq!(d.plain_text(0, d.len()), "a b\nc\nd\ntext\u{a0}x");
    }

    #[test]
    fn table() {
        let d = rd("<p>vor</p><table border=1><tr><th width=100>A</th><th width=200>B</th></tr>\
            <tr><td>1 ä</td><td><p>2</p><p>3</p></td></tr></table><p>nach</p>");
        assert_eq!(debug_text(&d.els), "vor\n[Tbl]\n[Row]|A|B\n[Row]|1 ä|2\n3[TblOff]nach");
        let at = d.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        assert_eq!(d.els[at], El::Code(Code::TblDef { cols: vec![1250, 2500] }));
        let t = crate::tables::parse(&d.els, at).unwrap();
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn colspan_and_unclosed_cells() {
        let d = rd("<table><tr><td colspan=2>x<tr><td>a<td>b</table>");
        assert_eq!(debug_text(&d.els), "[Tbl]\n[Row]|x|\n[Row]|a|b[TblOff]");
    }

    #[test]
    fn lists() {
        let d = rd("<ul><li>eins<li>zwei<ol start=3><li>drei</li><li>vier</ol></li></ul><p>Ende");
        assert_eq!(d.plain_text(0, d.len()), "• eins\n• zwei\n\t3. drei\n\t4. vier\nEnde");
    }

    #[test]
    fn word_html() {
        let html = "<html xmlns:o=\"urn:schemas-microsoft-com:office:office\"><head>\
            <meta http-equiv=Content-Type content=\"text/html; charset=windows-1252\">\
            <!--[if gte mso 9]><xml><w:WordDocument></w:WordDocument></xml><![endif]--></head>\
            <body lang=DE><div class=WordSection1><p class=MsoNormal><b><span style='font-size:14.0pt'>Fett</span></b><o:p></o:p></p>\
            <p class=MsoNormal><o:p>&nbsp;</o:p></p>\
            <p class=MsoListParagraph style='text-indent:-18.0pt;mso-list:l0 level1 lfo1'><![if !supportLists]>\
            <span style='font-family:Symbol'>&#183;<span style='font:7.0pt \"Times New Roman\"'>&nbsp;&nbsp; </span></span><![endif]>Punkt</p>\
            <p class=MsoNormal>A<span style='mso-tab-count:1'>&nbsp;&nbsp;&nbsp; </span>B <span style='font-weight:normal'>n</span></p></div></body></html>";
        let mut bytes: Vec<u8> = html.bytes().collect();
        bytes.extend_from_slice(b"<p>\xe4\xf6\xfc\xdf</p>");
        let d = read(&bytes).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "Fett\n\n·\u{a0}\u{a0} Punkt\nA\tB n\näöüß");
        assert_eq!(d.els[0], El::Code(Code::AttrOn(Attr::Bold)));
    }

    #[test]
    fn charset_1251() {
        let mut bytes = b"<html><head><meta charset=\"windows-1251\"></head><body><p>".to_vec();
        bytes.extend([0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]); // Привет
        bytes.extend_from_slice(b"</p></body></html>");
        let d = read(&bytes).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "Привет");
        // UTF-8 without any declaration
        let d = rd("<p>Grüße – Привет</p>");
        assert_eq!(d.plain_text(0, d.len()), "Grüße – Привет");
    }

    #[test]
    fn pre_and_quote() {
        let d = rd("<pre>a  b\n  c</pre><blockquote><p>zitat</p></blockquote>");
        assert_eq!(d.plain_text(0, d.len()), "a  b\n  c\n\tzitat");
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::Font(f)) if f.name == "Courier New")));
    }

    #[test]
    fn notes_and_headers() {
        let lo = "<body><div title=\"header\"><p>Kopf</p></div><p>Text<a class=\"sdfootnoteanc\" href=\"#sdfootnote1sym\"><sup>1</sup></a> weiter.</p>\
            <div id=\"sdfootnote1\"><p><a class=\"sdfootnotesym\" href=\"#sdfootnote1anc\">1</a>Fußnote.</p></div></body>";
        assert_eq!(debug_text(&rd(lo).els), "[Header:Kopf]Text[Footnote:Fußnote.] weiter.");
        let word = "<body><p class=MsoNormal>Wort<a style='mso-footnote-id:ftn1' href=\"#_ftn1\" name=\"_ftnref1\"><span class=MsoFootnoteReference>[1]</span></a></p>\
            <div style='mso-element:footnote-list'><![if !supportFootnotes]><br clear=all><hr align=left size=1 width=\"33%\"><![endif]>\
            <div style='mso-element:footnote' id=ftn1><p class=MsoFootnoteText><a style='mso-footnote-id:ftn1' href=\"#_ftnref1\" name=\"_ftn1\">[1]</a> Die Note.</p></div></div></body>";
        let d = rd(word);
        assert_eq!(debug_text(&d.els), "Wort[Footnote:Die Note.]");
    }

    #[test]
    fn never_panics() {
        let samples = [
            "<", "<<<>>>", "<p", "<p a=\"", "<!--", "<![if", "&#", "&#x", "&#99999999999;", "<table><td><table><tr><td>x</table>",
            "</td></tr></table></ul></li>", "<ul><li><ul><li><ul>", "<script>", "<td colspan=999999>", "<a href='x>y</a>",
            "<p style=\"font-weight:\">x", "<h1><h2><h3>", "<col width=99999999%>", "<ol start=-5><li>x",
        ];
        for s in samples {
            let _ = read(s.as_bytes());
        }
        let full = "<html><head><title>x</title></head><body><h1>Ü</h1><table><tr><td width=50%>ä</td><td>б</td></tr></table><ul><li>a</li></ul></body></html>";
        for n in 0..full.len() {
            if full.is_char_boundary(n) {
                let _ = read(&full.as_bytes()[..n]);
            }
        }
        let mut garbage = vec![];
        let mut x: u32 = 12345;
        for _ in 0..5000 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            garbage.push(b"<>/=\"' &;#abcdptrlu\n\x80\xff"[(x >> 16) as usize % 22]);
        }
        let _ = read(&garbage);
    }

    #[test]
    fn libreoffice_html() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/sample_lo.html");
        let Ok(bytes) = std::fs::read(p) else { return };
        let d = read(&bytes).unwrap();
        let t = debug_text(&d.els);
        assert!(t.starts_with("[Header:Kopfzeile][Footer:Fußzeile]Zentrierte Überschrift\nGrüße aus München: ÄÖÜäöüß. fett kursiv unterstrichen doppelt durch."), "{t}");
        assert!(t.contains("Привет мир") && t.contains("[Footnote:Text der Fußnote.]") && t.contains("[Row]|Zelle A1|Zelle B1") && t.ends_with("[TblOff]Ende."), "{t}");
        assert_eq!(d.init.justify, Justify::Center);
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Full))));
    }

    #[test]
    fn sample_file() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/sample.html");
        let Ok(bytes) = std::fs::read(p) else { return };
        let d = read(&bytes).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Grüße") && t.contains("Привет"), "{t}");
        assert!(t.contains("[Tbl]"), "{t}");
    }
}
