//! Styles (Alt-F8), paragraph numbering and outline (Shift-F5): the model
//! side — number formats, the numbering definitions, expansion of style
//! codes for layout/printing/export, and style library files.
//!
//! Layout never sees a style's codes in the document itself: the document
//! holds only `[Style On:Name]` … `[Style Off:Name]`.  `virtual_stream`
//! builds a copy in which every style code is followed by the style's codes
//! and every `[Par Num]` by its number text; layout runs on that copy and
//! the result is mapped back to document indices (`remap_layout`).  Editing
//! a style therefore changes every place it is used.

use crate::doc::*;
use crate::layout::Layout;

// ------------------------------------------------------------ numbering

impl NumFmt {
    /// The style character WP shows for this format (`1 a A i I`).
    pub fn ch(self) -> char {
        match self {
            NumFmt::Decimal => '1',
            NumFmt::LowerLetter => 'a',
            NumFmt::UpperLetter => 'A',
            NumFmt::LowerRoman => 'i',
            NumFmt::UpperRoman => 'I',
            NumFmt::Bullet => 'b',
        }
    }
    pub fn from_ch(c: char) -> Option<NumFmt> {
        Some(match c {
            '1' => NumFmt::Decimal,
            'a' => NumFmt::LowerLetter,
            'A' => NumFmt::UpperLetter,
            'i' => NumFmt::LowerRoman,
            'I' => NumFmt::UpperRoman,
            'b' => NumFmt::Bullet,
            _ => return None,
        })
    }
}

pub fn roman(mut n: u32) -> String {
    let mut s = String::new();
    for (v, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

fn letters(n: u32) -> String {
    // a..z, aa..zz, …
    if n == 0 {
        return "0".into();
    }
    let k = (n - 1) / 26 + 1;
    let c = (b'a' + ((n - 1) % 26) as u8) as char;
    std::iter::repeat_n(c, k as usize).collect()
}

pub fn fmt_number(n: u32, f: NumFmt) -> String {
    match f {
        NumFmt::Decimal => n.to_string(),
        NumFmt::LowerLetter => letters(n),
        NumFmt::UpperLetter => letters(n).to_uppercase(),
        NumFmt::LowerRoman => roman(n),
        NumFmt::UpperRoman => roman(n).to_uppercase(),
        NumFmt::Bullet => String::new(),
    }
}

/// Placeholders `%1`…`%8` in a level pattern: (byte position, level 1..8).
fn placeholders(text: &str) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut v = vec![];
    for i in 0..b.len().saturating_sub(1) {
        if b[i] == b'%' && (b'1'..=b'9').contains(&b[i + 1]) {
            v.push((i, (b[i + 1] - b'0') as usize));
        }
    }
    v
}

/// A level from WP's display form (`1.`, `(a)`, `I.`, `.1`, `•`).
pub fn level_from_display(s: &str, k: usize, attach: bool, prev: Option<&NumLevel>, start: u32) -> NumLevel {
    let s = s.trim();
    let pos = s.char_indices().find(|(_, c)| matches!(c, '1' | 'a' | 'A' | 'i' | 'I'));
    match pos {
        None => NumLevel { fmt: NumFmt::Bullet, text: if s.is_empty() { "\u{2022}".into() } else { s.to_string() }, start },
        Some((p, c)) => {
            let pre = &s[..p];
            let post = &s[p + c.len_utf8()..];
            let mut text = format!("{pre}%{k}{post}");
            if attach {
                if let Some(pl) = prev {
                    let ph = placeholders(&pl.text);
                    let base = match ph.last() {
                        Some(&(i, _)) => pl.text[..i + 2].to_string(),
                        None => String::new(),
                    };
                    if !base.is_empty() {
                        text = format!("{base}{text}");
                    }
                }
            }
            NumLevel { fmt: NumFmt::from_ch(c).unwrap_or(NumFmt::Decimal), text, start }
        }
    }
}

/// WP's display form of level `k` (1-based): its own part with the style
/// character in place of the number.
pub fn level_display(l: &NumLevel, k: usize) -> String {
    if l.fmt == NumFmt::Bullet {
        return l.text.clone();
    }
    let ph = placeholders(&l.text);
    let own = ph.iter().position(|&(_, m)| m == k);
    let from = match own {
        Some(o) if o > 0 => ph[o - 1].0 + 2,
        _ => 0,
    };
    let part = &l.text[from..];
    part.replace(&format!("%{k}"), &l.fmt.ch().to_string())
}

/// Is level `k` attached to the previous level (legal style)?
pub fn level_attached(l: &NumLevel, k: usize) -> bool {
    placeholders(&l.text).iter().any(|&(_, m)| m < k)
}

impl ParNumDef {
    fn from_display(items: [&str; 8], attach: bool) -> ParNumDef {
        let mut levels: Vec<NumLevel> = vec![];
        for (k, s) in items.iter().enumerate() {
            let l = level_from_display(s, k + 1, attach && k > 0, levels.last(), 1);
            levels.push(l);
        }
        ParNumDef { levels, enter_inserts: true, auto_adjust: true, outline_style: String::new() }
    }

    /// Display forms of the 8 levels.
    pub fn display(&self) -> Vec<String> {
        self.levels.iter().enumerate().map(|(k, l)| level_display(l, k + 1)).collect()
    }

    /// Which preset (0 Paragraph, 1 Outline, 2 Legal, 3 Bullets) this is.
    pub fn preset_index(&self) -> Option<usize> {
        (0..4).find(|&k| preset(k).levels.iter().zip(&self.levels).all(|(a, b)| a.fmt == b.fmt && a.text == b.text))
    }

    /// "Starting Paragraph Number" in legal style, e.g. `1` or `3.2`.
    pub fn start_text(&self) -> String {
        let last = self.levels.iter().rposition(|l| l.start != 1).unwrap_or(0);
        self.levels[..=last].iter().map(|l| l.start.to_string()).collect::<Vec<_>>().join(".")
    }

    pub fn set_start_text(&mut self, s: &str) {
        for l in self.levels.iter_mut() {
            l.start = 1;
        }
        for (k, p) in s.split(['.', ',', ' ']).filter(|p| !p.is_empty()).enumerate().take(8) {
            if let Ok(v) = p.trim().parse::<u32>() {
                self.levels[k].start = v;
            } else if let Some(c) = p.trim().chars().next() {
                // letters / roman numerals typed as starting values
                let v = if c.is_ascii_alphabetic() && p.len() == 1 { (c.to_ascii_lowercase() as u8 - b'a' + 1) as u32 } else { 1 };
                self.levels[k].start = v;
            }
        }
    }
}

/// Predefined numbering formats of the Paragraph Number Definition screen.
pub fn preset(k: usize) -> ParNumDef {
    match k {
        0 => ParNumDef::from_display(["1.", "a.", "i.", "(1)", "(a)", "(i)", "1)", "a)"], false),
        2 => ParNumDef::from_display(["1", ".1", ".1", ".1", ".1", ".1", ".1", ".1"], true),
        3 => ParNumDef::from_display(["\u{2022}", "o", "-", "\u{25a0}", "*", "+", "\u{b7}", "x"], false),
        _ => ParNumDef::from_display(["I.", "A.", "1.", "a.", "(1)", "(a)", "i)", "a)"], false),
    }
}

/// WP 5.1's initial definition: Outline style.
pub fn default_def() -> ParNumDef {
    preset(1)
}

/// Paragraph number counters for one pass over a document.
#[derive(Clone)]
pub struct Numberer {
    pub def: ParNumDef,
    counters: [u32; 8],
}

impl Default for Numberer {
    fn default() -> Self {
        Numberer::new(default_def())
    }
}

impl Numberer {
    pub fn new(def: ParNumDef) -> Numberer {
        let mut n = Numberer { def, counters: [0; 8] };
        n.reset();
        n
    }
    fn reset(&mut self) {
        for k in 0..8 {
            self.counters[k] = self.def.levels.get(k).map(|l| l.start).unwrap_or(1).saturating_sub(1);
        }
    }
    pub fn set_def(&mut self, d: &ParNumDef) {
        self.def = d.clone();
        self.reset();
    }
    /// Count a paragraph number at `level` (1..8) and return its text.
    pub fn next(&mut self, level: u8) -> String {
        let l = (level.clamp(1, 8) - 1) as usize;
        self.counters[l] += 1;
        for m in l + 1..8 {
            self.counters[m] = self.def.levels.get(m).map(|x| x.start).unwrap_or(1).saturating_sub(1);
        }
        self.text(l)
    }
    fn text(&self, l: usize) -> String {
        let Some(lv) = self.def.levels.get(l) else { return String::new() };
        if lv.fmt == NumFmt::Bullet {
            return lv.text.clone();
        }
        let mut out = String::new();
        let mut last = 0;
        for (i, m) in placeholders(&lv.text) {
            out.push_str(&lv.text[last..i]);
            let fmt = self.def.levels.get(m - 1).map(|x| x.fmt).unwrap_or(NumFmt::Decimal);
            let fmt = if fmt == NumFmt::Bullet { NumFmt::Decimal } else { fmt };
            out.push_str(&fmt_number(self.counters[m - 1].max(1), fmt));
            last = i + 2;
        }
        out.push_str(&lv.text[last..]);
        out
    }
}

/// Level of an automatic `[Par Num:Auto]` at `k`: one more than the
/// number of tabs/indents since the start of the paragraph.
pub fn auto_level(els: &[El], k: usize) -> u8 {
    let mut tabs = 0u8;
    for e in els[..k.min(els.len())].iter().rev() {
        match e {
            El::Code(Code::HRt | Code::HPg | Code::Cell | Code::Row) => break,
            El::Code(Code::Tab | Code::Indent | Code::DblIndent) => tabs = tabs.saturating_add(1),
            _ => {}
        }
    }
    (tabs + 1).min(8)
}

pub fn par_num_level(els: &[El], k: usize) -> u8 {
    match els.get(k) {
        Some(El::Code(Code::ParNum(Some(l)))) => (*l).clamp(1, 8),
        _ => auto_level(els, k),
    }
}

pub fn def_to_str(d: &ParNumDef) -> String {
    let lv: Vec<String> = d.levels.iter().map(|l| format!("{}{}\u{1e}{}", l.fmt.ch(), l.start, l.text)).collect();
    format!("{}\u{1d}{}{}{}", lv.join("\u{1f}"), d.enter_inserts as u8, d.auto_adjust as u8, d.outline_style)
}

pub fn str_to_def(s: &str) -> ParNumDef {
    let mut d = default_def();
    let (lv, rest) = s.split_once('\u{1d}').unwrap_or((s, "11"));
    let mut levels = vec![];
    for part in lv.split('\u{1f}') {
        let (head, text) = part.split_once('\u{1e}').unwrap_or((part, ""));
        let mut it = head.chars();
        let fmt = it.next().and_then(NumFmt::from_ch).unwrap_or(NumFmt::Decimal);
        let start = it.as_str().parse().unwrap_or(1);
        levels.push(NumLevel { fmt, text: text.to_string(), start });
    }
    while levels.len() < 8 {
        levels.push(d.levels[levels.len()].clone());
    }
    levels.truncate(8);
    d.levels = levels;
    let mut it = rest.chars();
    d.enter_inserts = it.next() != Some('0');
    d.auto_adjust = it.next() != Some('0');
    d.outline_style = it.as_str().to_string();
    d
}

// ------------------------------------------------------------- styles

pub fn find_style<'a>(doc: &'a Document, name: &str) -> Option<&'a StyleDef> {
    doc.styles.iter().find(|s| s.name == name)
}

/// The placeholder for the user's text in a paired style's codes.
pub fn text_marker() -> El {
    El::Code(Code::Comment(String::new()))
}

/// A style's codes as edited on the Codes screen (paired: on, [Comment], off).
pub fn style_edit_els(s: &StyleDef) -> Vec<El> {
    let mut v = s.on.clone();
    if s.kind != StyleKind::Open {
        v.push(text_marker());
        v.extend(s.off.iter().cloned());
    }
    v
}

/// Split edited codes back into on/off at the `[Comment]` placeholder.
pub fn set_style_els(s: &mut StyleDef, els: Vec<El>) {
    match els.iter().position(|e| *e == text_marker()) {
        Some(p) if s.kind != StyleKind::Open => {
            s.on = els[..p].to_vec();
            s.off = els[p + 1..].to_vec();
        }
        _ => {
            s.on = els.into_iter().filter(|e| *e != text_marker()).collect();
            s.off = vec![];
        }
    }
}

fn is_style_code(e: &El) -> bool {
    matches!(e, El::Code(Code::StyleOn(_) | Code::StyleOff(_) | Code::OpenStyle(_) | Code::ParNum(_)))
}

pub fn needs_virtual(doc: &Document) -> bool {
    doc.els.iter().any(is_style_code)
}

/// The document with style codes expanded and paragraph numbers as text.
pub struct Virtual {
    pub doc: Document,
    /// For every virtual element: the document element it belongs to.
    pub src: Vec<usize>,
    /// Is the virtual element the document element itself?
    pub real: Vec<bool>,
    /// Virtual index of every document element.
    pub rv: Vec<usize>,
}

fn outline_style_els<'a>(doc: &'a Document, def: &ParNumDef, level: u8) -> Option<&'a [El]> {
    if def.outline_style.is_empty() {
        return None;
    }
    let want = format!("{} {}", def.outline_style, level);
    doc.styles.iter().find(|s| s.name == want).map(|s| s.on.as_slice())
}

pub fn virtual_stream(doc: &Document) -> Option<Virtual> {
    if !needs_virtual(doc) {
        return None;
    }
    let mut v = Virtual { doc: Document::new(), src: vec![], real: vec![], rv: Vec::with_capacity(doc.els.len()) };
    v.doc.init = doc.init.clone();
    v.doc.initial_codes = doc.initial_codes.clone();
    let mut num = Numberer::default();
    for e in &doc.initial_codes {
        if let El::Code(Code::ParNumDef(d)) = e {
            num.set_def(d);
        }
    }
    let mut out: Vec<El> = Vec::with_capacity(doc.els.len() + 64);
    let mut src = Vec::with_capacity(doc.els.len() + 64);
    let mut real = Vec::with_capacity(doc.els.len() + 64);
    fn push_num(out: &mut Vec<El>, src: &mut Vec<usize>, real: &mut Vec<bool>, k: usize, num: &mut Numberer, lvl: u8, doc: &Document) {
        if let Some(st) = outline_style_els(doc, &num.def.clone(), lvl) {
            for e in st {
                if !matches!(e, El::Code(Code::ParNum(_))) {
                    out.push(e.clone());
                    src.push(k);
                    real.push(false);
                }
            }
        }
        for c in num.next(lvl).chars() {
            out.push(El::Ch(c));
            src.push(k);
            real.push(false);
        }
    }
    for (k, e) in doc.els.iter().enumerate() {
        v.rv.push(out.len());
        out.push(e.clone());
        src.push(k);
        real.push(true);
        let El::Code(c) = e else { continue };
        let exp: Option<&[El]> = match c {
            Code::StyleOn(n) | Code::OpenStyle(n) => find_style(doc, n).map(|s| s.on.as_slice()),
            Code::StyleOff(n) => find_style(doc, n).filter(|s| s.kind != StyleKind::Open).map(|s| s.off.as_slice()),
            Code::ParNumDef(d) => {
                num.set_def(d);
                None
            }
            Code::ParNum(_) => {
                let lvl = par_num_level(&out, out.len() - 1);
                push_num(&mut out, &mut src, &mut real, k, &mut num, lvl, doc);
                None
            }
            _ => None,
        };
        if let Some(exp) = exp {
            for x in exp {
                match x {
                    El::Code(Code::StyleOn(_) | Code::StyleOff(_) | Code::OpenStyle(_) | Code::Note { .. } | Code::HeadFoot { .. }) => {}
                    El::Code(Code::ParNum(_)) => {
                        out.push(x.clone());
                        src.push(k);
                        real.push(false);
                        let lvl = par_num_level(&out, out.len() - 1);
                        push_num(&mut out, &mut src, &mut real, k, &mut num, lvl, doc);
                    }
                    El::Code(Code::ParNumDef(d)) => {
                        num.set_def(d);
                    }
                    _ => {
                        out.push(x.clone());
                        src.push(k);
                        real.push(false);
                    }
                }
            }
        }
    }
    // the ParNum codes inside the copy must not be expanded again
    for e in out.iter_mut() {
        if matches!(e, El::Code(Code::ParNum(_))) {
            *e = El::Code(Code::Comment(String::new()));
        }
    }
    v.doc.els = out;
    v.src = src;
    v.real = real;
    Some(v)
}

/// The virtual stream as a plain document (for printing): style codes and
/// paragraph numbers already replaced by what they stand for.
pub fn flat_document(doc: &Document) -> Option<Document> {
    let v = virtual_stream(doc)?;
    let mut d = doc.clone();
    d.styles.clear();
    d.els = v
        .doc
        .els
        .into_iter()
        .filter(|e| !matches!(e, El::Code(Code::StyleOn(_) | Code::StyleOff(_) | Code::OpenStyle(_))))
        .collect();
    Some(d)
}

/// Map a layout of the virtual stream back onto the document's indices.
pub fn remap_layout(l: &mut Layout, v: &Virtual) {
    let vn = v.src.len();
    let n = v.rv.len();
    let dpos = |x: usize| -> usize {
        if x >= vn {
            n
        } else if v.real[x] {
            v.src[x]
        } else {
            v.src[x] + 1
        }
    };
    for line in &mut l.lines {
        let (vs, ve) = (line.start, line.end);
        let (s, e) = (dpos(vs), dpos(ve).max(dpos(vs)));
        let last = line.caret.last().copied().unwrap_or((0, 0));
        let mut caret = Vec::with_capacity(e - s + 1);
        for d in s..=e {
            let rvd = if d >= n { vn } else { v.rv[d] };
            let k = if rvd <= ve { rvd.saturating_sub(vs) } else { ve.saturating_sub(vs) };
            caret.push(line.caret.get(k).copied().unwrap_or(last));
        }
        line.start = s;
        line.end = e;
        line.caret = caret;
        for c in &mut line.cells {
            c.el = v.src.get(c.el).copied().unwrap_or(n);
        }
        for t in &mut line.tab_kinds {
            t.0 = v.src.get(t.0).copied().unwrap_or(n);
        }
    }
    l.note_nums = l.note_nums.iter().map(|(k, x)| (v.src.get(*k).copied().unwrap_or(*k), *x)).collect();
}

/// For .docx export: style codes stay (they become Word styles), and of
/// the style's own codes only what shapes the text (returns, tabs, text)
/// is added; open styles are expanded completely.
pub fn docx_stream(doc: &Document) -> Vec<El> {
    if !needs_virtual(doc) {
        return doc.els.clone();
    }
    let structural = |e: &El| matches!(e, El::Ch(_) | El::Code(Code::HRt | Code::HPg | Code::Tab | Code::Indent | Code::DblIndent | Code::MarRel | Code::HardSpace | Code::ParNum(_)));
    let mut out = Vec::with_capacity(doc.els.len());
    for e in &doc.els {
        out.push(e.clone());
        match e {
            El::Code(Code::StyleOn(n)) => {
                if let Some(s) = find_style(doc, n) {
                    out.extend(s.on.iter().filter(|x| structural(x)).cloned());
                }
            }
            El::Code(Code::StyleOff(n)) => {
                if let Some(s) = find_style(doc, n) {
                    out.extend(s.off.iter().filter(|x| structural(x)).cloned());
                }
            }
            El::Code(Code::OpenStyle(n)) => {
                if let Some(s) = find_style(doc, n) {
                    out.extend(s.on.iter().filter(|x| !matches!(x, El::Code(Code::StyleOn(_) | Code::StyleOff(_) | Code::OpenStyle(_)))).cloned());
                }
            }
            _ => {}
        }
    }
    out
}

/// Is the cursor in outline mode (after an `[Outline On]`)?
pub fn outline_mode(doc: &Document, idx: usize) -> bool {
    doc.els[..idx.min(doc.els.len())]
        .iter()
        .rev()
        .find_map(|e| match e {
            El::Code(Code::Outline(on)) => Some(*on),
            _ => None,
        })
        .unwrap_or(false)
}

/// The numbering definition in effect at `idx`.
pub fn def_at(doc: &Document, idx: usize) -> ParNumDef {
    doc.initial_codes
        .iter()
        .chain(doc.els[..idx.min(doc.els.len())].iter())
        .rev()
        .find_map(|e| match e {
            El::Code(Code::ParNumDef(d)) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(default_def)
}

/// The paired style the cursor is inside of (innermost), with the index
/// of its `[Style Off]`.
pub fn open_style_at(doc: &Document, idx: usize) -> Option<(String, Option<usize>)> {
    let mut stack: Vec<String> = vec![];
    for e in &doc.els[..idx.min(doc.els.len())] {
        match e {
            El::Code(Code::StyleOn(n)) => stack.push(n.clone()),
            El::Code(Code::StyleOff(n)) => {
                if let Some(p) = stack.iter().rposition(|x| x == n) {
                    stack.remove(p);
                }
            }
            _ => {}
        }
    }
    let name = stack.pop()?;
    let off = doc.els[idx.min(doc.els.len())..].iter().position(|e| *e == El::Code(Code::StyleOff(name.clone()))).map(|p| p + idx);
    Some((name, off))
}

// ------------------------------------------------------------ families

/// The outline family at the cursor: (start, end) element range of the
/// paragraph with the paragraph number and all following paragraphs whose
/// numbers are on a deeper level (or that have no number).
pub fn family_range(doc: &Document, idx: usize) -> Option<(usize, usize)> {
    let els = &doc.els;
    let para_start = |i: usize| els[..i.min(els.len())].iter().rposition(|e| matches!(e, El::Code(Code::HRt | Code::HPg))).map(|p| p + 1).unwrap_or(0);
    let para_end = |i: usize| els[i.min(els.len())..].iter().position(|e| matches!(e, El::Code(Code::HRt | Code::HPg))).map(|p| p + i + 1).unwrap_or(els.len());
    let num_in = |s: usize, e: usize| (s..e).find(|&k| matches!(els[k], El::Code(Code::ParNum(_))));
    // the numbered paragraph at or before the cursor
    let mut s = para_start(idx);
    loop {
        let e = para_end(s);
        if num_in(s, e).is_some() {
            break;
        }
        if s == 0 {
            return None;
        }
        s = para_start(s - 1);
    }
    let level = par_num_level(els, num_in(s, para_end(s))?);
    let mut e = para_end(s);
    while e < els.len() {
        let ne = para_end(e);
        match num_in(e, ne) {
            Some(k) if par_num_level(els, k) <= level => break,
            _ => {}
        }
        e = ne;
    }
    Some((s, e))
}

// ------------------------------------------------------------ libraries

/// Read a style library: a WP 5.x `.STY` file or one we saved.
pub fn load_library(p: &std::path::Path) -> Result<Vec<StyleDef>, String> {
    let b = std::fs::read(p).map_err(|e| e.to_string())?;
    if b.starts_with(b"\xFFWPC") {
        return crate::wp5::read_styles(&b);
    }
    let s = String::from_utf8_lossy(&b);
    if let Some(rest) = s.strip_prefix("#WPSTYLES 1") {
        return Ok(rest.lines().filter_map(|l| l.strip_prefix("#style|")).filter_map(crate::stream::str_to_style).collect());
    }
    // any document we can open: take its style list
    let d = crate::fileio::load(p).map_err(|e| e.to_string())?;
    Ok(d.styles)
}

pub fn save_library(p: &std::path::Path, styles: &[StyleDef]) -> std::io::Result<()> {
    let mut t = String::from("#WPSTYLES 1\n");
    for s in styles {
        t.push_str(&format!("#style|{}\n", crate::stream::style_to_str(s)));
    }
    std::fs::write(p, t)
}

/// Merge library styles into the document's list.  Returns the names that
/// already existed (replaced only when `replace`).
pub fn merge_styles(doc: &mut Document, lib: Vec<StyleDef>, replace: bool) -> usize {
    let mut n = 0;
    for s in lib {
        match doc.styles.iter_mut().find(|x| x.name == s.name) {
            Some(x) => {
                if replace {
                    *x = s;
                    n += 1;
                }
            }
            None => {
                doc.styles.push(s);
                n += 1;
            }
        }
    }
    doc.styles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{MonoMetrics, layout};

    fn text(s: &str) -> Vec<El> {
        s.chars().map(El::Ch).collect()
    }

    #[test]
    fn number_formats() {
        let mut n = Numberer::new(preset(1));
        assert_eq!(n.next(1), "I.");
        assert_eq!(n.next(2), "A.");
        assert_eq!(n.next(2), "B.");
        assert_eq!(n.next(3), "1.");
        assert_eq!(n.next(1), "II.");
        assert_eq!(n.next(2), "A.");
        let mut n = Numberer::new(preset(2));
        assert_eq!(n.next(1), "1");
        assert_eq!(n.next(2), "1.1");
        assert_eq!(n.next(3), "1.1.1");
        assert_eq!(n.next(2), "1.2");
        let mut n = Numberer::new(preset(0));
        assert_eq!(n.next(1), "1.");
        assert_eq!(n.next(4), "(1)");
        assert_eq!(n.next(3), "i.");
        assert_eq!(Numberer::new(preset(3)).next(1), "\u{2022}");
        assert_eq!(preset(2).display()[1], ".1");
        assert_eq!(preset(1).display()[4], "(1)");
        assert_eq!(preset(1).preset_index(), Some(1));
    }

    #[test]
    fn def_roundtrip() {
        let mut d = preset(2);
        d.levels[0].start = 3;
        d.outline_style = "Document".into();
        d.enter_inserts = false;
        assert_eq!(str_to_def(&def_to_str(&d)), d);
        let mut d2 = preset(0);
        d2.set_start_text("3.2");
        assert_eq!(d2.levels[0].start, 3);
        assert_eq!(d2.levels[1].start, 2);
        assert_eq!(d2.start_text(), "3.2");
    }

    fn styled_doc() -> Document {
        let mut d = Document::new();
        d.styles.push(StyleDef {
            name: "Head".into(),
            kind: StyleKind::Paired,
            description: "Heading".into(),
            on: vec![El::Code(Code::AttrOn(Attr::Bold)), El::Ch('>')],
            off: vec![El::Code(Code::AttrOff(Attr::Bold)), El::Code(Code::HRt)],
            enter: EnterAction::Off,
        });
        d.els.push(El::Code(Code::StyleOn("Head".into())));
        d.els.extend(text("Title"));
        d.els.push(El::Code(Code::StyleOff("Head".into())));
        d.els.extend(text("Body"));
        d
    }

    #[test]
    fn style_expands_in_layout() {
        let d = styled_doc();
        let l = layout(&d, &MonoMetrics);
        // ">Title" on line 1 in bold, "Body" on its own line (the style's HRt)
        assert_eq!(l.lines.len(), 2, "{:?}", l.lines.iter().map(|x| (x.start, x.end)).collect::<Vec<_>>());
        let t: String = l.lines[0].cells.iter().map(|c| c.ch).collect();
        assert_eq!(t, ">Title");
        assert!(l.lines[0].cells[1].attrs.has(Attr::Bold));
        assert_eq!(l.lines[0].cells[0].el, 0);
        assert_eq!(l.lines[1].start, 7);
        let t: String = l.lines[1].cells.iter().map(|c| c.ch).collect();
        assert_eq!(t, "Body");
        assert!(!l.lines[1].cells[0].attrs.has(Attr::Bold));
        assert_eq!(l.line_of(7), 1);
        assert_eq!(l.line_of(3), 0);
        // caret entries cover start..=end
        assert_eq!(l.lines[0].caret.len(), l.lines[0].end - l.lines[0].start + 1);
        // printing sees the same
        let f = flat_document(&d).unwrap();
        assert_eq!(f.plain_text(0, f.len()), ">Title\nBody");
    }

    #[test]
    fn par_nums_in_layout() {
        let mut d = Document::new();
        for (tabs, t) in [(0, "eins"), (1, "zwei"), (1, "drei"), (0, "vier")] {
            for _ in 0..tabs {
                d.els.push(El::Code(Code::Tab));
            }
            d.els.push(El::Code(Code::ParNum(None)));
            d.els.push(El::Code(Code::Indent));
            d.els.extend(text(t));
            d.els.push(El::Code(Code::HRt));
        }
        let l = layout(&d, &MonoMetrics);
        let lines: Vec<String> = l.lines.iter().map(|x| x.cells.iter().map(|c| c.ch).collect::<String>()).collect();
        assert_eq!(lines[0], "I.eins");
        assert_eq!(lines[1], "A.zwei");
        assert_eq!(lines[2], "B.drei");
        assert_eq!(lines[3], "II.vier");
        // number sits at the tab, text at the next tab stop
        assert_eq!(l.lines[1].cells[0].col, 15);
        let f = flat_document(&d).unwrap();
        assert!(f.plain_text(0, f.len()).contains("II."));
    }

    #[test]
    fn family() {
        let mut d = Document::new();
        for (tabs, t) in [(0, "a"), (1, "b"), (1, "c"), (0, "d")] {
            for _ in 0..tabs {
                d.els.push(El::Code(Code::Tab));
            }
            d.els.push(El::Code(Code::ParNum(None)));
            d.els.extend(text(t));
            d.els.push(El::Code(Code::HRt));
        }
        let (s, e) = family_range(&d, 1).unwrap();
        assert_eq!((s, e), (0, 11));
        let (s, e) = family_range(&d, 5).unwrap();
        assert_eq!((s, e), (3, 7));
    }

    #[test]
    fn edit_els_split() {
        let d = styled_doc();
        let mut s = d.styles[0].clone();
        let els = style_edit_els(&s);
        assert_eq!(els.len(), 5);
        set_style_els(&mut s, els);
        assert_eq!(s, d.styles[0]);
    }
}
