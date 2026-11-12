//! Ctrl-F9, 2 Sort (or Block + Ctrl-F9): the WP 5.1 Sort screen and the
//! sort/select engine.  Record types Line, Paragraph, Merge (secondary
//! merge file) and Table; up to nine keys (alphanumeric or numeric; field /
//! line / word, negative = counted from the end); select statements with
//! `+` (OR), `*` (AND), `= <> > < >= <=`, `keyg` (any word).
//! Alphanumeric keys collate the German way (ä = a, ö = o, ü = u, ß = ss,
//! accents ignored, case only breaks ties).

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{HitAction, Outcome, Screen, Wp};
use std::cmp::Ordering;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecType {
    Line,
    Para,
    Merge,
    Table,
}

impl RecType {
    fn title(self) -> &'static str {
        match self {
            RecType::Line => "Sort by Line",
            RecType::Para => "Sort by Paragraph",
            RecType::Merge => "Sort Secondary Merge File",
            RecType::Table => "Sort Table",
        }
    }
    /// Column labels after "Key Typ" (field/line/word in screen order).
    fn labels(self) -> &'static [&'static str] {
        match self {
            RecType::Line => &["Field", "Word"],
            RecType::Para => &["Line", "Field", "Word"],
            RecType::Merge => &["Field", "Line", "Word"],
            RecType::Table => &["Cell ", "Line", "Word"],
        }
    }
    fn type_name(self) -> &'static str {
        match self {
            RecType::Line => "Line sort",
            RecType::Para => "Paragraph sort",
            RecType::Merge => "Merge sort",
            RecType::Table => "Table sort",
        }
    }
}

/// One sort key.  `field` is the field (line / paragraph sort), merge
/// field or table cell; `line` the line within it, `word` the word.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub numeric: bool,
    pub field: i32,
    pub line: i32,
    pub word: i32,
}

impl Key {
    pub fn new() -> Key {
        Key { numeric: false, field: 1, line: 1, word: 1 }
    }
    /// Values in screen order for the record type.
    fn values(&self, t: RecType) -> Vec<i32> {
        match t {
            RecType::Line => vec![self.field, self.word],
            RecType::Para => vec![self.line, self.field, self.word],
            RecType::Merge | RecType::Table => vec![self.field, self.line, self.word],
        }
    }
    fn set_value(&mut self, t: RecType, k: usize, v: i32) {
        let slot = match (t, k) {
            (RecType::Line, 0) => &mut self.field,
            (RecType::Line, _) => &mut self.word,
            (RecType::Para, 0) => &mut self.line,
            (RecType::Para, 1) => &mut self.field,
            (_, 0) => &mut self.field,
            (_, 1) => &mut self.line,
            _ => &mut self.word,
        };
        *slot = v;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SortDef {
    pub kind: RecType,
    pub keys: Vec<Key>,
    pub select: String,
    pub select_only: bool,
    pub descending: bool,
}

impl SortDef {
    pub fn new(kind: RecType) -> SortDef {
        SortDef { kind, keys: vec![Key::new()], select: String::new(), select_only: false, descending: false }
    }
}

// ================================================================ collation

/// German (DIN 5007-1) folding: lower case, umlauts = base letter, ß = ss,
/// accents dropped.
pub fn fold(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        for l in c.to_lowercase() {
            match l {
                'ä' | 'à' | 'á' | 'â' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => o.push('a'),
                'æ' => o.push_str("ae"),
                'ç' | 'ć' | 'č' => o.push('c'),
                'ď' | 'đ' => o.push('d'),
                'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => o.push('e'),
                'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => o.push('i'),
                'ł' | 'ľ' | 'ĺ' => o.push('l'),
                'ñ' | 'ń' | 'ň' => o.push('n'),
                'ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ø' | 'ō' | 'ő' => o.push('o'),
                'œ' => o.push_str("oe"),
                'ř' | 'ŕ' => o.push('r'),
                'ß' => o.push_str("ss"),
                'ś' | 'š' | 'ş' => o.push('s'),
                'ť' | 'ţ' => o.push('t'),
                'ü' | 'ù' | 'ú' | 'û' | 'ū' | 'ů' | 'ű' => o.push('u'),
                'ý' | 'ÿ' => o.push('y'),
                'ź' | 'ż' | 'ž' => o.push('z'),
                '\u{a0}' => o.push(' '),
                c => o.push(c),
            }
        }
    }
    o
}

/// Compare two texts the German way.
pub fn collate(a: &str, b: &str) -> Ordering {
    fold(a).cmp(&fold(b)).then_with(|| {
        // ties: unaccented before accented, lower case before upper case
        let la: String = a.to_lowercase();
        let lb: String = b.to_lowercase();
        la.cmp(&lb).then_with(|| b.cmp(a))
    })
}

/// A number in a key: `1234`, `-5`, `(5)`, `1.234,56`, `1,234.56`, `3,5`, `€ 12`.
pub fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    let neg_paren = s.starts_with('(') && s.contains(')');
    let start = s.find(|c: char| c.is_ascii_digit() || c == '-' || c == '.' || c == ',')?;
    let mut t = String::new();
    let mut neg = neg_paren;
    for (k, c) in s[start..].char_indices() {
        match c {
            '-' if k == 0 => neg = true,
            '0'..='9' | '.' | ',' => t.push(c),
            '\'' | ' ' | '\u{a0}' if !t.is_empty() => {}
            _ => break,
        }
    }
    let last_dot = t.rfind('.');
    let last_comma = t.rfind(',');
    let norm = match (last_dot, last_comma) {
        (Some(d), Some(c)) if c > d => t.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        (None, Some(c)) => {
            if t.matches(',').count() == 1 && t.len() - c - 1 != 3 {
                t.replace(',', ".")
            } else {
                t.replace(',', "")
            }
        }
        (Some(_), None) if t.matches('.').count() > 1 => t.replace('.', ""),
        _ => t,
    };
    let v: f64 = norm.trim_matches('.').parse().ok()?;
    Some(if neg { -v } else { v })
}

// ================================================================ records

fn piece_text(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(Code::HardSpace) => s.push('\u{a0}'),
            El::Code(Code::HardHyphen) => s.push('\u{2011}'),
            _ => {}
        }
    }
    s
}

fn split_on(els: &[El], is_sep: impl Fn(&El) -> bool) -> Vec<&[El]> {
    let mut v = vec![];
    let mut s = 0;
    for (i, e) in els.iter().enumerate() {
        if is_sep(e) {
            v.push(&els[s..i]);
            s = i + 1;
        }
    }
    v.push(&els[s..]);
    v
}

fn pick<T>(v: &[T], k: i32) -> Option<&T> {
    if k > 0 {
        v.get(k as usize - 1)
    } else if k < 0 {
        v.len().checked_sub((-k) as usize).and_then(|i| v.get(i))
    } else {
        None
    }
}

/// Words: separated by spaces, `/` and `-` (a leading minus stays).
fn words(s: &str) -> Vec<String> {
    let mut v = vec![];
    let mut w = String::new();
    for c in s.chars() {
        let sep = c == ' ' || c == '/' || (c == '-' && !w.is_empty());
        if sep {
            if !w.is_empty() {
                v.push(std::mem::take(&mut w));
            }
        } else {
            w.push(c);
        }
    }
    if !w.is_empty() {
        v.push(w);
    }
    v
}

fn is_tab(e: &El) -> bool {
    matches!(e, El::Code(Code::Tab | Code::Indent | Code::DblIndent))
}

/// The fields of a record body as element slices (fields → lines).
fn fields_of(kind: RecType, body: &[El]) -> Vec<Vec<&[El]>> {
    match kind {
        RecType::Line => split_on(body, is_tab).into_iter().map(|f| vec![f]).collect(),
        RecType::Para => {
            // "fields" here are lines; each line has tab fields
            split_on(body, |e| matches!(e, El::Code(Code::HRt))).into_iter().map(|l| split_on(l, is_tab)).collect()
        }
        RecType::Merge => {
            let mut v = vec![];
            let mut s = 0;
            let mut i = 0;
            while i < body.len() {
                if matches!(&body[i], El::Code(Code::Merge(n)) if n == "END FIELD") {
                    v.push(&body[s..i]);
                    i += 1;
                    if matches!(body.get(i), Some(El::Code(Code::HRt))) {
                        i += 1;
                    }
                    s = i;
                    continue;
                }
                if matches!(&body[i], El::Code(Code::Merge(n)) if n == "END RECORD") {
                    break;
                }
                i += 1;
            }
            if s < i {
                v.push(&body[s..i]);
            }
            v.into_iter().map(|f| split_on(f, |e| matches!(e, El::Code(Code::HRt)))).collect()
        }
        RecType::Table => {
            let cells = split_on(body, |e| matches!(e, El::Code(Code::Cell)));
            cells.into_iter().skip(1).map(|c| split_on(c, |e| matches!(e, El::Code(Code::HRt)))).collect()
        }
    }
}

/// Text of a key in a record.
pub fn key_text(kind: RecType, body: &[El], k: &Key) -> String {
    let f = fields_of(kind, body);
    let piece: Option<&[El]> = match kind {
        RecType::Line => pick(&f.iter().map(|x| x[0]).collect::<Vec<_>>(), k.field).copied(),
        RecType::Para => pick(&f, k.line).and_then(|l| pick(l, k.field).copied()),
        _ => pick(&f, k.field).and_then(|l| pick(l, k.line).copied()),
    };
    let Some(p) = piece else { return String::new() };
    let ws = words(&piece_text(p));
    pick(&ws, k.word).cloned().unwrap_or_default()
}

fn cmp_key(a: &str, b: &str, numeric: bool) -> Ordering {
    if numeric {
        let x = parse_number(a).unwrap_or(f64::NEG_INFINITY);
        let y = parse_number(b).unwrap_or(f64::NEG_INFINITY);
        x.partial_cmp(&y).unwrap_or(Ordering::Equal)
    } else {
        collate(a, b)
    }
}

// ================================================================ select

#[derive(Debug)]
enum Sel {
    Cmp { key: Option<usize>, op: String, val: String },
    Or(Box<Sel>, Box<Sel>),
    And(Box<Sel>, Box<Sel>),
}

fn sel_tokens(s: &str) -> Vec<String> {
    let cs: Vec<char> = s.chars().collect();
    let mut t = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if "()+*".contains(c) {
            t.push(c.to_string());
            i += 1;
        } else if "<>=".contains(c) {
            let mut op = c.to_string();
            if i + 1 < cs.len() && "<>=".contains(cs[i + 1]) {
                op.push(cs[i + 1]);
                i += 1;
            }
            t.push(op);
            i += 1;
        } else if c == '"' {
            let mut v = String::from("\"");
            i += 1;
            while i < cs.len() && cs[i] != '"' {
                v.push(cs[i]);
                i += 1;
            }
            i += 1;
            t.push(v);
        } else {
            let mut v = String::new();
            while i < cs.len() && !cs[i].is_whitespace() && !"()+*<>=".contains(cs[i]) {
                v.push(cs[i]);
                i += 1;
            }
            t.push(v);
        }
    }
    t
}

fn parse_select(s: &str) -> Result<Sel, String> {
    let t = sel_tokens(s);
    let mut i = 0;
    let r = sel_expr(&t, &mut i)?;
    if i < t.len() {
        return Err(format!("unexpected \"{}\"", t[i]));
    }
    Ok(r)
}

fn sel_expr(t: &[String], i: &mut usize) -> Result<Sel, String> {
    let mut a = sel_term(t, i)?;
    while *i < t.len() && (t[*i] == "+" || t[*i] == "*") {
        let op = t[*i].clone();
        *i += 1;
        let b = sel_term(t, i)?;
        a = if op == "+" { Sel::Or(Box::new(a), Box::new(b)) } else { Sel::And(Box::new(a), Box::new(b)) };
    }
    Ok(a)
}

fn sel_term(t: &[String], i: &mut usize) -> Result<Sel, String> {
    let tok = t.get(*i).ok_or("incomplete select statement")?;
    if tok == "(" {
        *i += 1;
        let e = sel_expr(t, i)?;
        if t.get(*i).map(|s| s.as_str()) != Some(")") {
            return Err("missing )".into());
        }
        *i += 1;
        return Ok(e);
    }
    let lk = tok.to_lowercase();
    let key = match lk.strip_prefix("key") {
        Some("g") => None,
        Some(n) => Some(n.parse::<usize>().map_err(|_| format!("bad key \"{tok}\""))?),
        None => return Err(format!("expected keyN, got \"{tok}\"")),
    };
    let op = t.get(*i + 1).filter(|o| ["=", "<>", ">", "<", ">=", "<=", "=>", "=<", "><"].contains(&o.as_str())).ok_or("missing operator")?.clone();
    let val = t.get(*i + 2).ok_or("missing value")?.trim_start_matches('"').to_string();
    *i += 3;
    let op = match op.as_str() {
        "=>" => ">=".into(),
        "=<" => "<=".into(),
        "><" => "<>".into(),
        _ => op,
    };
    Ok(Sel::Cmp { key, op, val })
}

fn sel_eval(s: &Sel, kind: RecType, body: &[El], keys: &[Key]) -> bool {
    match s {
        Sel::Or(a, b) => sel_eval(a, kind, body, keys) || sel_eval(b, kind, body, keys),
        Sel::And(a, b) => sel_eval(a, kind, body, keys) && sel_eval(b, kind, body, keys),
        Sel::Cmp { key, op, val } => {
            let test = |text: &str, numeric: bool| {
                let o = cmp_key(text, val, numeric);
                match op.as_str() {
                    "=" => o == Ordering::Equal || (!numeric && fold(text) == fold(val)),
                    "<>" => o != Ordering::Equal && !(!numeric && fold(text) == fold(val)),
                    ">" => o == Ordering::Greater,
                    "<" => o == Ordering::Less,
                    ">=" => o != Ordering::Less,
                    _ => o != Ordering::Greater,
                }
            };
            match key {
                Some(n) => match keys.get(n.wrapping_sub(1)) {
                    Some(k) => test(&key_text(kind, body, k), k.numeric),
                    None => false,
                },
                None => words(&piece_text(body)).iter().any(|w| test(w, false)),
            }
        }
    }
}

// ================================================================ sorting

/// Split into (prefix, record bodies, separators after each, suffix).
fn split_records(els: &[El], kind: RecType) -> (Vec<El>, Vec<Vec<El>>, Vec<Vec<El>>) {
    let n = els.len();
    let mut prefix = vec![];
    let mut bodies = vec![];
    let mut seps = vec![];
    let mut i = 0;
    match kind {
        RecType::Line => {
            let mut s = 0;
            for (k, e) in els.iter().enumerate() {
                if matches!(e, El::Code(Code::HRt)) {
                    bodies.push(els[s..k].to_vec());
                    seps.push(vec![e.clone()]);
                    s = k + 1;
                }
            }
            if s < n {
                bodies.push(els[s..].to_vec());
                seps.push(vec![]);
            }
        }
        RecType::Para => {
            while i < n && matches!(els[i], El::Code(Code::HRt)) {
                prefix.push(els[i].clone());
                i += 1;
            }
            let mut s = i;
            while i < n {
                if matches!(els[i], El::Code(Code::HRt)) {
                    let mut j = i;
                    while j < n && matches!(els[j], El::Code(Code::HRt)) {
                        j += 1;
                    }
                    if j - i >= 2 || j == n {
                        bodies.push(els[s..i].to_vec());
                        seps.push(els[i..j].to_vec());
                        s = j;
                    }
                    i = j;
                    continue;
                }
                i += 1;
            }
            if s < n {
                bodies.push(els[s..].to_vec());
                seps.push(vec![]);
            }
        }
        RecType::Merge => {
            // a {FIELD NAMES} record stays first
            let first_cmd = els.iter().find_map(|e| match e {
                El::Code(Code::Merge(n)) => Some(n.as_str()),
                El::Ch(c) if !c.is_whitespace() => Some(""),
                _ => None,
            });
            let mut s = 0;
            let mut in_names = first_cmd == Some("FIELD NAMES");
            while i < n {
                if matches!(&els[i], El::Code(Code::Merge(c)) if c == "END RECORD") {
                    let mut j = i + 1;
                    while j < n && matches!(els[j], El::Code(Code::HPg | Code::HRt)) {
                        j += 1;
                    }
                    if in_names {
                        prefix.extend_from_slice(&els[s..j]);
                        in_names = false;
                    } else {
                        bodies.push(els[s..=i].to_vec());
                        seps.push(els[i + 1..j].to_vec());
                    }
                    s = j;
                    i = j;
                    continue;
                }
                i += 1;
            }
            if els[s..].iter().any(|e| !matches!(e, El::Code(Code::HRt | Code::HPg))) {
                bodies.push(els[s..].to_vec());
                seps.push(vec![]);
            } else {
                // trailing returns stay at the end
                if let Some(l) = seps.last_mut() {
                    l.extend_from_slice(&els[s..]);
                } else {
                    prefix.extend_from_slice(&els[s..]);
                }
            }
        }
        RecType::Table => {
            let mut s = None;
            for (k, e) in els.iter().enumerate() {
                if matches!(e, El::Code(Code::Row)) {
                    if let Some(a) = s {
                        bodies.push(els[a..k].to_vec());
                        seps.push(vec![]);
                    } else {
                        prefix.extend_from_slice(&els[..k]);
                    }
                    s = Some(k);
                }
            }
            if let Some(a) = s {
                bodies.push(els[a..].to_vec());
                seps.push(vec![]);
            } else {
                prefix = els.to_vec();
            }
        }
    }
    (prefix, bodies, seps)
}

/// Sort (and/or select) a range of elements.  For tables pass the rows
/// (from the first [Row] to before [Tbl Off]).
pub fn sort_els(els: &[El], def: &SortDef) -> Result<Vec<El>, String> {
    let sel = if def.select.trim().is_empty() { None } else { Some(parse_select(&def.select)?) };
    let (prefix, bodies, seps) = split_records(els, def.kind);
    let keys = if def.keys.is_empty() { vec![Key::new()] } else { def.keys.clone() };
    let mut recs: Vec<(Vec<String>, Vec<El>)> = bodies
        .into_iter()
        .filter(|b| sel.as_ref().is_none_or(|s| sel_eval(s, def.kind, b, &keys)))
        .map(|b| (keys.iter().map(|k| key_text(def.kind, &b, k)).collect(), b))
        .collect();
    if !(def.select_only && sel.is_some()) {
        recs.sort_by(|a, b| {
            let mut o = Ordering::Equal;
            for (k, key) in keys.iter().enumerate() {
                o = o.then_with(|| cmp_key(&a.0[k], &b.0[k], key.numeric));
            }
            if def.descending { o.reverse() } else { o }
        });
    }
    let mut out = prefix;
    let default_sep = seps.iter().find(|s| !s.is_empty()).cloned().unwrap_or_default();
    let nrec = recs.len();
    for (k, (_, b)) in recs.into_iter().enumerate() {
        out.extend(b);
        // separators stay in place; a record that moves off the end gets one
        let sep = match seps.get(k) {
            Some(s) if !s.is_empty() || k + 1 == nrec => s.clone(),
            _ => default_sep.clone(),
        };
        if k + 1 == nrec && seps.len() > nrec {
            out.extend(seps.last().cloned().unwrap_or_default());
        } else {
            out.extend(sep);
        }
    }
    Ok(out)
}

// ================================================================ UI

thread_local! {
    static LAST: std::cell::RefCell<Option<SortDef>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Debug)]
enum Src {
    /// the document (whole, or a block / table range)
    Screen,
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Menu,
    Keys { key: usize, col: usize, fresh: bool },
    Select { pos: usize },
    Choice(u8),
    View,
}

pub struct SortScreen {
    def: SortDef,
    src: Src,
    dst: Option<PathBuf>,
    /// element range of the source on screen (block / table rows)
    range: Option<(usize, usize)>,
    mode: Mode,
    sel_text: Vec<char>,
}

fn default_kind(els: &[El], in_table: bool) -> RecType {
    if in_table {
        RecType::Table
    } else if els.iter().any(|e| matches!(e, El::Code(Code::Merge(n)) if n == "END RECORD")) {
        RecType::Merge
    } else {
        RecType::Line
    }
}

fn make(wp: &Wp, src: Src, dst: Option<PathBuf>, range: Option<(usize, usize)>, table: bool) -> SortScreen {
    let els = match &src {
        Src::Screen => wp.ed_ref().doc.els.clone(),
        Src::File(p) => crate::fileio::load(p).map(|d| d.els).unwrap_or_default(),
    };
    let kind = default_kind(&els, table);
    let mut def = LAST.with(|l| l.borrow().clone()).unwrap_or_else(|| SortDef::new(kind));
    if table {
        def.kind = RecType::Table;
    } else if def.kind == RecType::Table || kind == RecType::Merge {
        def.kind = kind;
    }
    let sel_text = def.select.chars().collect();
    SortScreen { def, src, dst, range, mode: Mode::Menu, sel_text }
}

/// Table rows range (first [Row] .. [Tbl Off]) at the cursor.
fn table_rows(wp: &Wp) -> Option<(usize, usize)> {
    let e = wp.ed_ref();
    let t = crate::tables::table_at(&e.doc.els, e.cursor)?;
    Some((*t.row_codes.first()?, t.off))
}

/// Block on + Ctrl-F9: sort the block (or the table the cursor is in).
pub fn open_block(wp: &mut Wp) {
    let range = wp.ed_ref().block_range();
    let trows = table_rows(wp);
    wp.ed().block_anchor = None;
    let (range, table) = match trows {
        Some(r) => (Some(r), true),
        None => (range, false),
    };
    let s = make(wp, Src::Screen, None, range, table);
    wp.push(Box::new(s));
}

/// Ctrl-F9, 2: `Input file to sort: (Screen)` → `Output file for sort: (Screen)`.
pub fn open_prompts(_wp: &mut Wp) -> Next {
    Some(Prompt::new("Input file to sort:", "(Screen)", |_wp, s| {
        let input = s?;
        Some(Prompt::new("Output file for sort:", "(Screen)", move |wp, s| {
            let output = s?;
            let screen = |x: &str| x.trim().is_empty() || x.trim().eq_ignore_ascii_case("(screen)");
            let src = if screen(&input) { Src::Screen } else { Src::File(wp.resolve_path(&input)) };
            if let Src::File(p) = &src {
                if !p.exists() {
                    wp.flash(&format!("ERROR: File not found — {}", p.display()));
                    return None;
                }
            }
            let dst = if screen(&output) { None } else { Some(wp.resolve_path(&output)) };
            let (range, table) = match (&src, table_rows(wp)) {
                (Src::Screen, Some(r)) => (Some(r), true),
                _ => (None, false),
            };
            Some(Box::new(make(wp, src, dst, range, table)) as Box<dyn Screen>)
        }) as Box<dyn Screen>)
    }))
}

impl SortScreen {
    fn ncols(&self) -> usize {
        2 + self.def.kind.labels().len()
    }

    fn perform(&mut self, wp: &mut Wp) {
        self.def.select = self.sel_text.iter().collect();
        LAST.with(|l| *l.borrow_mut() = Some(self.def.clone()));
        let (els, range) = match &self.src {
            Src::Screen => {
                let all = &wp.ed_ref().doc.els;
                let (a, b) = self.range.unwrap_or((0, all.len()));
                (all[a..b].to_vec(), Some((a, b)))
            }
            Src::File(p) => match crate::fileio::load(p) {
                Ok(d) => (d.els, None),
                Err(e) => {
                    wp.flash(&format!("ERROR: {e}"));
                    return;
                }
            },
        };
        let sorted = match sort_els(&els, &self.def) {
            Ok(s) => s,
            Err(e) => {
                wp.flash(&format!("ERROR: Select statement — {e}"));
                return;
            }
        };
        match (&self.dst, range) {
            (Some(p), _) => {
                let mut d = Document::new();
                d.els = sorted;
                if let Err(e) = crate::fileio::save(p, &d) {
                    wp.flash(&format!("ERROR: {e}"));
                }
            }
            (None, Some((a, b))) => {
                let e = wp.ed();
                let mut all = e.doc.els.clone();
                all.splice(a..b, sorted);
                e.set_els(all);
                e.cursor = a.min(e.doc.els.len());
                e.settle();
            }
            (None, None) => {
                wp.ed().insert_els(&sorted);
            }
        }
        wp.after_edit();
    }

    fn key_input(&mut self, i: &Input, key: usize, col: usize, fresh: bool) -> Mode {
        let ncols = self.ncols();
        let t = self.def.kind;
        let nkeys = self.def.keys.len();
        let mv = |key: usize, col: usize| Mode::Keys { key, col, fresh: true };
        match i {
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Enter) => return Mode::Menu,
            Input::Func(Func::Right) | Input::Func(Func::Tab) => {
                if col + 1 < ncols {
                    return mv(key, col + 1);
                }
                if key + 1 < nkeys {
                    return mv(key + 1, 1);
                }
                if nkeys < 9 {
                    let k = self.def.keys[key].clone();
                    self.def.keys.push(Key { numeric: false, ..k });
                    return mv(key + 1, 1);
                }
            }
            Input::Func(Func::Left) | Input::Func(Func::MarRel) => {
                if col > 1 {
                    return mv(key, col - 1);
                }
                if key > 0 {
                    return mv(key - 1, ncols - 1);
                }
            }
            Input::Func(Func::Up) if key >= 3 => return mv(key - 3, col),
            Input::Func(Func::Down) if key + 3 < nkeys => return mv(key + 3, col),
            Input::Func(Func::Del) | Input::Func(Func::DelEol) if nkeys > 1 && key > 0 => {
                self.def.keys.remove(key);
                return mv(key.min(self.def.keys.len() - 1), col);
            }
            Input::Char(c) if col == 1 => match c.to_ascii_lowercase() {
                'a' => self.def.keys[key].numeric = false,
                'n' => self.def.keys[key].numeric = true,
                _ => {}
            },
            Input::Char(c) if col >= 2 && (c.is_ascii_digit() || *c == '-') => {
                let cur = self.def.keys[key].values(t)[col - 2];
                let s = if fresh { c.to_string() } else { format!("{cur}{c}") };
                let v = if s == "-" { -1 } else { s.parse::<i32>().unwrap_or(cur) };
                let v = if v == 0 { 1 } else { v.clamp(-99, 999) };
                self.def.keys[key].set_value(t, col - 2, v);
                return Mode::Keys { key, col, fresh: false };
            }
            Input::Func(Func::Backspace) if col >= 2 => {
                self.def.keys[key].set_value(t, col - 2, 1);
                return mv(key, col);
            }
            _ => {}
        }
        Mode::Keys { key, col, fresh }
    }
}

impl Screen for SortScreen {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match self.mode {
            Mode::Keys { key, col, fresh } => {
                self.mode = self.key_input(i, key, col, fresh);
                return Outcome::Keep;
            }
            Mode::Select { pos } => {
                let mut pos = pos.min(self.sel_text.len());
                match i {
                    Input::Func(Func::Exit) | Input::Func(Func::Enter) | Input::Func(Func::Cancel) => {
                        self.def.select = self.sel_text.iter().collect();
                        if self.def.select.trim().is_empty() {
                            self.def.select_only = false;
                        }
                        self.mode = Mode::Menu;
                        return Outcome::Keep;
                    }
                    Input::Char(c) => {
                        self.sel_text.insert(pos, *c);
                        pos += 1;
                    }
                    Input::Func(Func::Left) => pos = pos.saturating_sub(1),
                    Input::Func(Func::Right) => pos = (pos + 1).min(self.sel_text.len()),
                    Input::Func(Func::Home) => pos = 0,
                    Input::Func(Func::End) => pos = self.sel_text.len(),
                    Input::Func(Func::Backspace) if pos > 0 => {
                        pos -= 1;
                        self.sel_text.remove(pos);
                    }
                    Input::Func(Func::Del) if pos < self.sel_text.len() => {
                        self.sel_text.remove(pos);
                    }
                    Input::Func(Func::DelEol) => self.sel_text.truncate(pos),
                    _ => {}
                }
                self.mode = Mode::Select { pos };
                return Outcome::Keep;
            }
            Mode::Choice(which) => {
                let k = match i {
                    Input::Char(c) => match (which, c.to_ascii_lowercase()) {
                        (_, d @ '1'..='3') => d as u8 - b'0',
                        (5, 's') | (6, 'a') | (7, 'm') => 1,
                        (5, 'o') | (6, 'd') | (7, 'l') => 2,
                        (7, 'p') => 3,
                        _ => 0,
                    },
                    _ => 0,
                };
                match (which, k) {
                    (5, 1) => self.def.select_only = false,
                    (5, 2) => self.def.select_only = true,
                    (6, 1) => self.def.descending = false,
                    (6, 2) => self.def.descending = true,
                    (7, 1..=3) if self.def.kind != RecType::Table => {
                        self.def.kind = [RecType::Merge, RecType::Line, RecType::Para][k as usize - 1];
                    }
                    _ => {}
                }
                self.mode = Mode::Menu;
                return Outcome::Keep;
            }
            Mode::View => {
                match i {
                    Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Enter) => self.mode = Mode::Menu,
                    Input::Func(f) if matches!(f, Func::Up | Func::Down | Func::PgUp | Func::PgDn | Func::ScreenUp | Func::ScreenDown | Func::Left | Func::Right | Func::Home | Func::End | Func::GoTo) => {
                        wp.edit_input(i.clone());
                    }
                    _ => {}
                }
                return Outcome::Keep;
            }
            Mode::Menu => {}
        }
        let c = match i {
            Input::Char(c) => c.to_ascii_lowercase(),
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Enter) => return Outcome::Close,
            _ => return Outcome::Keep,
        };
        match c {
            '1' | 'p' => {
                self.perform(wp);
                return Outcome::Close;
            }
            '2' | 'v' => self.mode = Mode::View,
            '3' | 'k' => self.mode = Mode::Keys { key: 0, col: 1, fresh: true },
            '4' | 's' => self.mode = Mode::Select { pos: self.sel_text.len() },
            '5' | 'a' => {
                if !self.sel_text.iter().collect::<String>().trim().is_empty() {
                    self.mode = Mode::Choice(5);
                }
            }
            '6' | 'o' => self.mode = Mode::Choice(6),
            '7' | 't' => {
                if self.def.kind != RecType::Table {
                    self.mode = Mode::Choice(7);
                }
            }
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        let rows = g.rows;
        let base = rows.saturating_sub(13);
        for r in base..rows {
            g.fill_row(r, 0, g.cols, ' ', st);
        }
        // ---- divider with title
        let title = format!(" {} ", self.def.kind.title());
        let w = g.cols;
        let left = (w.saturating_sub(title.chars().count())) / 2;
        let mut line: String = "-".repeat(left);
        line.push_str(&title);
        while line.chars().count() < w {
            line.push('-');
        }
        g.text(0, base, &line, st);
        // ---- key table header
        let labels = self.def.kind.labels();
        let mut head = String::from("Key Typ");
        for l in labels {
            head.push(' ');
            head.push_str(l);
        }
        let block_w = 27;
        // label start columns (inside a block)
        let mut cols_at = vec![0usize, 4usize];
        let mut x = 7usize;
        for l in labels {
            x += 1;
            cols_at.push(x);
            x += l.len();
        }
        for b in 0..3 {
            g.text(b * block_w, base + 2, &head, st);
        }
        let mut cursor = None;
        for (k, key) in self.def.keys.iter().enumerate() {
            let r = base + 3 + k / 3;
            let x0 = (k % 3) * block_w;
            g.text(x0 + 1, r, &(k + 1).to_string(), st);
            let mut vals = vec![(k + 1).to_string(), if key.numeric { "n".into() } else { "a".into() }];
            vals.extend(key.values(self.def.kind).iter().map(|v| v.to_string()));
            for (c, v) in vals.iter().enumerate().skip(1) {
                let cx = x0 + cols_at[c] + 1;
                let s = if matches!(self.mode, Mode::Keys { key: kk, col, .. } if kk == k && col == c) {
                    cursor = Some((cx, r));
                    wp.theme.block
                } else {
                    st
                };
                g.text(cx, r, v, s);
            }
        }
        // ---- select
        g.text(1, base + 6, "Select", st);
        let sel: String = self.sel_text.iter().collect();
        g.text(1, base + 7, &sel, st);
        if let Mode::Select { pos } = self.mode {
            cursor = Some((1 + pos.min(self.sel_text.len()), base + 7));
        }
        // ---- action / order / type
        g.text(1, base + 9, "Action", st);
        g.text(28, base + 9, "Order", st);
        g.text(54, base + 9, "Type", st);
        let action = if sel.trim().is_empty() {
            "Sort"
        } else if self.def.select_only {
            "Select Only"
        } else {
            "Select and Sort"
        };
        g.text(1, base + 10, action, st);
        g.text(28, base + 10, if self.def.descending { "Descending" } else { "Ascending" }, st);
        g.text(54, base + 10, self.def.kind.type_name(), st);
        // ---- bottom line
        let row = rows - 1;
        g.fill_row(row, 0, g.cols, ' ', st);
        match self.mode {
            Mode::Keys { .. } => {
                g.text(0, row, "Type: a = Alphanumeric; n = Numeric;  Use arrows;  Press Exit when done", st);
            }
            Mode::Select { .. } => {
                g.text(0, row, "+(OR), *(AND), =, <>, >, <, >=, <=;  Press Exit when done", st);
            }
            Mode::View => {
                g.text(0, row, "Press Exit when done", st);
                cursor = g.cursor;
            }
            Mode::Choice(which) => {
                let (pre, items): (&str, &[&str]) = match which {
                    5 => ("Action: ", &["~Select and Sort", "Select ~Only"]),
                    6 => ("Order: ", &["~Ascending", "~Descending"]),
                    _ => ("Type: ", &["~Merge", "~Line", "~Paragraph"]),
                };
                let mut c = g.text(0, row, pre, st);
                for (k, it) in items.iter().enumerate() {
                    let c0 = c;
                    c = g.text(c, row, &format!("{} ", k + 1), st);
                    c = g.text_mn(c, row, it, st, hi);
                    wp.hit(row, c0, c, HitAction::Input(Input::Char((b'1' + k as u8) as char)));
                    if k + 1 < items.len() {
                        c = g.text(c, row, "; ", st);
                    }
                }
                let c = g.text(c, row, ": ", st);
                g.text(c, row, "0", st);
                cursor = Some((c, row));
            }
            Mode::Menu => {
                let items = ["~Perform Action", "~View", "~Keys", "~Select", "~Action", "~Order", "~Type"];
                let mut c = 0;
                for (k, it) in items.iter().enumerate() {
                    let c0 = c;
                    c = g.text(c, row, &format!("{} ", k + 1), st);
                    c = g.text_mn(c, row, it, st, hi);
                    wp.hit(row, c0, c, HitAction::Input(Input::Char((b'1' + k as u8) as char)));
                    if k + 1 < items.len() {
                        c = g.text(c, row, "; ", st);
                    }
                }
                let c = g.text(c, row, ": ", st);
                g.text(c, row, "0", st);
                cursor = Some((c, row));
            }
        }
        g.cursor = cursor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<El> {
        Document::from_text(s).els
    }
    fn text(v: &[El]) -> String {
        let mut d = Document::new();
        d.els = v.to_vec();
        d.plain_text(0, d.len())
    }

    #[test]
    fn german_order() {
        let mut d = SortDef::new(RecType::Line);
        let s = sort_els(&lines("Zander\nÖl\nStraße\nApfel\nÄrger\nStrasse\nober\nAdler\nmüde\nMuster\n"), &d).unwrap();
        assert_eq!(text(&s), "Adler\nApfel\nÄrger\nmüde\nMuster\nober\nÖl\nStrasse\nStraße\nZander\n");
        d.descending = true;
        let s = sort_els(&lines("b\na\nc"), &d).unwrap();
        assert_eq!(text(&s), "c\nb\na");
    }

    #[test]
    fn numeric_fields_words_select() {
        let mut d = SortDef::new(RecType::Line);
        d.keys = vec![Key { numeric: true, field: 2, line: 1, word: 1 }];
        let s = sort_els(&lines("x\t100\ny\t9\nz\t-3\nw\t1.234,5\n"), &d).unwrap();
        assert_eq!(text(&s), "z\t-3\ny\t9\nx\t100\nw\t1.234,5\n");
        // last word of field 1 (surname), select
        d.keys = vec![Key { numeric: false, field: 1, line: 1, word: -1 }];
        d.select = "key1<>Weber".into();
        let s = sort_els(&lines("Hans Weber\nAnna Zorn\nKarl Ähm\n"), &d).unwrap();
        assert_eq!(text(&s), "Karl Ähm\nAnna Zorn\n");
        d.select = "keyg=anna + key1=weber".into();
        let s = sort_els(&lines("Hans Weber\nAnna Zorn\nKarl Ähm\n"), &d).unwrap();
        assert_eq!(text(&s), "Hans Weber\nAnna Zorn\n");
        assert!(parse_select("key1 = ").is_err());
        assert_eq!(parse_number("(1.234,50)"), Some(-1234.5));
        assert_eq!(parse_number("1,234.50"), Some(1234.5));
    }

    #[test]
    fn paragraphs_merge_table() {
        let d = SortDef::new(RecType::Para);
        let s = sort_els(&lines("Zweiter\nAbsatz\n\nErster\n\n\nDritter"), &d).unwrap();
        assert_eq!(text(&s), "Dritter\n\nErster\n\n\nZweiter\nAbsatz");
        // merge records
        let m = |s: &str| El::Code(Code::Merge(s.into()));
        let mut els = vec![];
        for (a, b) in [("Weiß", "3"), ("Bauer", "1")] {
            els.extend(lines(a));
            els.push(m("END FIELD"));
            els.push(El::Code(Code::HRt));
            els.extend(lines(b));
            els.push(m("END FIELD"));
            els.push(El::Code(Code::HRt));
            els.push(m("END RECORD"));
            els.push(El::Code(Code::HPg));
        }
        let mut d = SortDef::new(RecType::Merge);
        let s = sort_els(&els, &d).unwrap();
        assert!(text(&s).starts_with("Bauer\n1\n"), "{:?}", text(&s));
        d.keys = vec![Key { numeric: true, field: 2, line: 1, word: 1 }];
        d.descending = true;
        let s = sort_els(&els, &d).unwrap();
        assert!(text(&s).starts_with("Weiß"));
        // table rows by cell 2
        let mut t = crate::tables::new_table(2, 3, 7200);
        let vals = ["c", "3", "a", "1", "b", "2"];
        let mut k = 0;
        let mut i = 0;
        while i < t.len() {
            if t[i] == El::Code(Code::Cell) {
                t.insert(i + 1, El::Ch(vals[k].chars().next().unwrap()));
                k += 1;
            }
            i += 1;
        }
        let tb = crate::tables::parse(&t, 0).unwrap();
        let rows = &t[tb.row_codes[0]..tb.off];
        let d = SortDef { kind: RecType::Table, keys: vec![Key { numeric: true, field: 2, line: 1, word: 1 }], ..SortDef::new(RecType::Table) };
        let s = sort_els(rows, &d).unwrap();
        let txt: String = s.iter().filter_map(|e| if let El::Ch(c) = e { Some(*c) } else { None }).collect();
        assert_eq!(txt, "a1b2c3");
    }
}
