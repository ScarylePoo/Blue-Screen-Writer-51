//! Speller (Ctrl-F2) and Thesaurus (Alt-F1), modelled on WordPerfect 5.1.
//!
//! * Dictionaries are Hunspell `.aff`/`.dic` pairs read with the pure-Rust
//!   `spellbook` crate; the thesaurus uses LibreOffice MyThes `th_*_v2.dat`
//!   files (the index is built in memory, no `.idx` needed).
//! * Files are searched, in this order, in `<exe dir>\dict` / `<exe dir>\thes`,
//!   `%APPDATA%\Blue Screen Writer 51\dict` / `…\thes`, `.\assets\…` and the source
//!   tree's `assets\…` (development).  A dictionary lives in
//!   `dict\<ll_CC>\<ll_CC>.aff/.dic` (or flat `dict\<ll_CC>.aff/.dic`).
//!   Everything is loaded lazily, once per language, and cached.
//! * Language: WP 5.1 uses two-letter language codes (`DE`, `US`, `UK`, `IT`,
//!   `FR`, `ES`, `RU`).  The current speller/thesaurus language defaults to
//!   `DE`; it is switched with option `7 Language` of the Speller's Check
//!   menu (an addition to WP's menu) and remembered in
//!   `%APPDATA%\Blue Screen Writer 51\spell_lang.txt`.
//! * Supplementary dictionary (words added with `3 Add`):
//!   `%APPDATA%\Blue Screen Writer 51\sup_<lang>.txt`, one word per line (UTF-8);
//!   `4 New Sup. Dictionary` switches to another file for the session.

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::PageBreak;
use crate::ui::{Outcome, Screen, Wp};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ================================================================ languages

pub struct Lang {
    pub code: &'static str,
    pub name: &'static str,
    pub dict: &'static str,
    pub thes: &'static str,
}

pub const LANGS: &[Lang] = &[
    Lang { code: "DE", name: "Deutsch", dict: "de_DE", thes: "th_de_DE_v2.dat" },
    Lang { code: "US", name: "English US", dict: "en_US", thes: "th_en_US_v2.dat" },
    Lang { code: "UK", name: "English UK", dict: "en_GB", thes: "th_en_US_v2.dat" },
    Lang { code: "IT", name: "Italiano", dict: "it_IT", thes: "th_it_IT_v2.dat" },
    Lang { code: "FR", name: "Français", dict: "fr_FR", thes: "th_fr_FR_v2.dat" },
    Lang { code: "ES", name: "Español", dict: "es_ES", thes: "th_es_ES_v2.dat" },
    Lang { code: "RU", name: "Русский", dict: "ru_RU", thes: "th_ru_RU_v2.dat" },
    Lang { code: "AR", name: "Arabic", dict: "ar", thes: "th_ar.dat" },
    // no Hebrew dictionary is shipped; put he_IL.aff/.dic into dict\he_IL to enable it
    Lang { code: "HE", name: "Hebrew", dict: "he_IL", thes: "th_he_IL.dat" },
];

pub fn lang_info(code: &str) -> &'static Lang {
    let c = code.to_ascii_uppercase();
    let c = match c.as_str() {
        "GE" | "DA" => "DE".to_string(),
        "EN" | "AM" => "US".to_string(),
        "GB" | "OZ" => "UK".to_string(),
        "SP" => "ES".to_string(),
        _ => c,
    };
    LANGS.iter().find(|l| l.code == c).unwrap_or(&LANGS[0])
}

static CUR_LANG: Mutex<Option<String>> = Mutex::new(None);

fn lang_file() -> PathBuf {
    crate::settings::config_dir().join("spell_lang.txt")
}

/// The language used by the Speller and Thesaurus (WP language code).
pub fn current_lang(_wp: &Wp) -> String {
    // a [Lang:XX] code in the document (Format: Other, Language) wins
    if let Some(l) = crate::fmtmenu::doc_lang_at_cursor(_wp) {
        if LANGS.iter().any(|x| x.code == l) {
            return l;
        }
    }
    let mut g = CUR_LANG.lock().unwrap();
    if g.is_none() {
        let saved = std::fs::read_to_string(lang_file()).ok().map(|s| s.trim().to_ascii_uppercase());
        *g = Some(match saved {
            Some(s) if LANGS.iter().any(|l| l.code == s) => s,
            _ => "US".into(),
        });
    }
    g.clone().unwrap()
}

pub fn set_lang(code: &str) {
    let code = lang_info(code).code.to_string();
    let _ = std::fs::create_dir_all(crate::settings::config_dir());
    let _ = std::fs::write(lang_file(), &code);
    *CUR_LANG.lock().unwrap() = Some(code);
}

// =========================================================== file locations

fn search_dirs(sub: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join(sub));
        }
    }
    v.push(crate::settings::config_dir().join(sub));
    v.push(PathBuf::from("assets").join(sub));
    v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(sub));
    v
}

fn find_file(sub: &str, rels: &[String]) -> Option<PathBuf> {
    for d in search_dirs(sub) {
        for r in rels {
            let p = d.join(r);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Read a text file: UTF-8 (BOM stripped) or, failing that, Latin-1.
fn read_text(p: &Path) -> std::io::Result<String> {
    let b = std::fs::read(p)?;
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b);
    Ok(match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    })
}

// ============================================================= dictionaries

type DictCache = Mutex<HashMap<String, Result<Arc<spellbook::Dictionary>, String>>>;
static DICTS: std::sync::LazyLock<DictCache> = std::sync::LazyLock::new(Default::default);

fn dict_paths(l: &Lang) -> Option<(PathBuf, PathBuf)> {
    let n = l.dict;
    let aff = find_file("dict", &[format!("{n}/{n}.aff"), format!("{n}.aff")])?;
    let dic = aff.with_extension("dic");
    dic.is_file().then_some((aff, dic))
}

/// The main dictionary for a language (loaded on first use).
pub fn dictionary(code: &str) -> Result<Arc<spellbook::Dictionary>, String> {
    let l = lang_info(code);
    let mut g = DICTS.lock().unwrap();
    g.entry(l.code.to_string())
        .or_insert_with(|| {
            let (aff, dic) = dict_paths(l).ok_or_else(|| format!("{}.dic not found", l.dict))?;
            let a = read_text(&aff).map_err(|e| format!("{}: {e}", aff.display()))?;
            let d = read_text(&dic).map_err(|e| format!("{}: {e}", dic.display()))?;
            spellbook::Dictionary::new(&a, &d)
                .map(Arc::new)
                .map_err(|e| format!("{}.aff invalid format: {e}", l.dict))
        })
        .clone()
}

static STEMS: std::sync::LazyLock<Mutex<HashMap<String, Arc<Vec<String>>>>> =
    std::sync::LazyLock::new(Default::default);

/// The stems listed in the .dic file (used for wildcard Look Up).
fn stems(code: &str) -> Arc<Vec<String>> {
    let l = lang_info(code);
    let mut g = STEMS.lock().unwrap();
    g.entry(l.code.to_string())
        .or_insert_with(|| {
            let mut v = Vec::new();
            if let Some((_, dic)) = dict_paths(l) {
                if let Ok(t) = read_text(&dic) {
                    for line in t.lines().skip(1) {
                        let w = line.split(['/', '\t']).next().unwrap_or("").trim();
                        if !w.is_empty() {
                            v.push(w.to_string());
                        }
                    }
                }
            }
            v.sort();
            v.dedup();
            Arc::new(v)
        })
        .clone()
}

/// `?` = one character, `*` = any run (WP's Look Up patterns).
fn wild_match(p: &[char], s: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some('*') => (0..=s.len()).any(|k| wild_match(&p[1..], &s[k..])),
        Some('?') => !s.is_empty() && wild_match(&p[1..], &s[1..]),
        Some(c) => !s.is_empty() && lower_eq(*c, s[0]) && wild_match(&p[1..], &s[1..]),
    }
}

fn lower_eq(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

pub fn sup_path(code: &str) -> PathBuf {
    crate::settings::config_dir().join(format!("sup_{}.txt", lang_info(code).code.to_ascii_lowercase()))
}

fn load_sup(p: &Path) -> HashSet<String> {
    read_text(p)
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn append_sup(p: &Path, w: &str) {
    use std::io::Write;
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{w}");
    }
}

// =========================================================== word iteration

/// Codes a word may run across (attributes, fonts, soft hyphens …).
/// Footnotes/endnotes and headers/footers carry their own text and end a
/// word; their nested text is not checked.
fn is_invisible(e: &El) -> bool {
    !matches!(e, El::Code(Code::Note { .. } | Code::HeadFoot { .. })) && crate::editor::is_invisible(e)
}

/// A word in the element stream: the indices of its characters (codes in
/// between, e.g. `[BOLD]`, are not part of it) and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub idx: Vec<usize>,
    pub text: String,
}

impl Word {
    pub fn start(&self) -> usize {
        self.idx[0]
    }
    pub fn end(&self) -> usize {
        self.idx.last().unwrap() + 1
    }
}

fn is_apos(c: char) -> bool {
    c == '\'' || c == '’'
}

/// Next visible element at or after `i` (skipping invisible codes).
fn next_visible(els: &[El], mut i: usize, to: usize) -> Option<usize> {
    while i < to {
        if !is_invisible(&els[i]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The first word starting at or after `from` and ending before `to`.
pub fn next_word(els: &[El], from: usize, to: usize) -> Option<Word> {
    let to = to.min(els.len());
    let mut i = from;
    while i < to && !matches!(els[i], El::Ch(c) if c.is_alphanumeric()) {
        i += 1;
    }
    if i >= to {
        return None;
    }
    let mut w = Word { idx: Vec::new(), text: String::new() };
    let mut j = i;
    while j < to {
        match &els[j] {
            El::Ch(c) if c.is_alphanumeric() => {
                w.idx.push(j);
                w.text.push(*c);
            }
            El::Ch(c) if is_apos(*c) => {
                let more = next_visible(els, j + 1, to)
                    .is_some_and(|k| matches!(els[k], El::Ch(n) if n.is_alphanumeric()));
                if !more {
                    break;
                }
                w.idx.push(j);
                w.text.push(*c);
            }
            e if inside_word(e) => {}
            _ => break,
        }
        j += 1;
    }
    Some(w)
}

/// Codes that may sit inside a word (bold in the middle of a word, a font
/// change, tracked-change marks, a soft hyphen…).  Anything structural —
/// table cells, notes, boxes, comments — ends the word.
fn inside_word(e: &El) -> bool {
    matches!(
        e,
        El::Code(
            Code::AttrOn(_)
                | Code::AttrOff(_)
                | Code::Font(_)
                | Code::RevOn { .. }
                | Code::RevOff { .. }
                | Code::SoftHyphen
                | Code::Link(_)
                | Code::LinkEnd
                | Code::StyleOn(_)
                | Code::StyleOff(_)
        )
    )
}

/// All words in `from..to`.
pub fn words(els: &[El], from: usize, to: usize) -> Vec<Word> {
    let mut v = Vec::new();
    let mut i = from;
    while let Some(w) = next_word(els, i, to) {
        i = w.end();
        v.push(w);
    }
    v
}

/// Start of the word containing (or immediately preceding) `pos`.
fn word_start(els: &[El], pos: usize) -> usize {
    let mut k = pos.min(els.len());
    // on a space right after a word: take that word
    if k < els.len() && !matches!(els[k], El::Ch(c) if c.is_alphanumeric()) {
        let mut b = k;
        while b > 0 && is_invisible(&els[b - 1]) {
            b -= 1;
        }
        if b > 0 && matches!(els[b - 1], El::Ch(c) if c.is_alphanumeric()) {
            k = b;
        } else {
            return k;
        }
    }
    while k > 0 {
        match &els[k - 1] {
            El::Ch(c) if c.is_alphanumeric() || is_apos(*c) => k -= 1,
            e if is_invisible(e) => k -= 1,
            _ => break,
        }
    }
    k
}

/// The word at `pos` (or the next one after it).
pub fn word_at(els: &[El], pos: usize) -> Option<Word> {
    next_word(els, word_start(els, pos), els.len())
}

/// Replace the characters of `w` with `new`, leaving every code (attribute
/// on/off, fonts …) where it is.  Returns the index just after the new word.
pub fn replace_word(els: &mut Vec<El>, w: &Word, new: &str) -> usize {
    let new: Vec<char> = new.chars().collect();
    let (n, m) = (w.idx.len(), new.len());
    for k in 0..n.min(m) {
        els[w.idx[k]] = El::Ch(new[k]);
    }
    let last = *w.idx.last().unwrap();
    if m > n {
        els.splice(last + 1..last + 1, new[n..].iter().map(|&c| El::Ch(c)));
        last + 1 + (m - n)
    } else {
        for &i in w.idx[m..].iter().rev() {
            els.remove(i);
        }
        last + 1 - (n - m)
    }
}

/// Delete the characters (not codes) in `from..to`; returns how many.
fn delete_chars(els: &mut Vec<El>, from: usize, to: usize) -> usize {
    let mut n = 0;
    let mut i = to.min(els.len());
    while i > from {
        i -= 1;
        if els[i].is_char() {
            els.remove(i);
            n += 1;
        }
    }
    n
}

fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_numeric())
}

fn has_letter(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic())
}

/// WP's irregular-case rules: THis→This, tHis→This, tHIS→THIS.
fn case_fix(w: &str) -> Option<String> {
    let c: Vec<char> = w.chars().collect();
    if c.len() < 3 || !c.iter().all(|c| c.is_alphabetic()) {
        return None;
    }
    let (u0, u1) = (c[0].is_uppercase(), c[1].is_uppercase());
    let rest_lower = c[2..].iter().any(|c| c.is_lowercase());
    let rest_upper = c[2..].iter().all(|c| c.is_uppercase());
    let cap = |s: &[char]| -> String {
        let mut out: String = s[0].to_uppercase().collect();
        out.extend(s[1..].iter().flat_map(|c| c.to_lowercase()));
        out
    };
    if u0 && u1 && rest_lower && c[2..].iter().all(|c| c.is_lowercase()) {
        Some(cap(&c))
    } else if !u0 && u1 && rest_lower && c[2..].iter().all(|c| c.is_lowercase()) {
        Some(cap(&c))
    } else if !u0 && u1 && rest_upper {
        Some(w.to_uppercase())
    } else {
        None
    }
}

// ================================================================== checker

pub struct Checker {
    pub lang: String,
    dict: Result<Arc<spellbook::Dictionary>, String>,
    pub sup_path: PathBuf,
    sup: HashSet<String>,
    skip: HashSet<String>,
    pub ignore_numbers: bool,
    pub double_check: bool,
    pub case_check: bool,
}

impl Checker {
    pub fn new(lang: &str) -> Checker {
        let sp = sup_path(lang);
        Checker {
            lang: lang_info(lang).code.into(),
            dict: dictionary(lang),
            sup: load_sup(&sp),
            sup_path: sp,
            skip: HashSet::new(),
            ignore_numbers: false,
            double_check: true,
            case_check: true,
        }
    }

    pub fn error(&self) -> Option<String> {
        self.dict.as_ref().err().cloned()
    }

    fn dict_ok(&self, w: &str) -> bool {
        match &self.dict {
            Ok(d) => d.check(w),
            Err(_) => true,
        }
    }

    /// Is the word acceptable?
    pub fn check(&self, w: &str) -> bool {
        if !has_letter(w) || (self.ignore_numbers && has_digit(w)) {
            return true;
        }
        let w = w.replace('’', "'");
        if self.skip.contains(&w) || self.sup.contains(&w) || self.sup.contains(&w.to_lowercase()) {
            return true;
        }
        if self.dict_ok(&w) {
            return true;
        }
        // Elisions: l'acqua, dell'anno, qu'il, d'Artagnan …
        if let Some(p) = w.find('\'') {
            let (a, b) = (&w[..=p], &w[p + 1..]);
            let a_ok = self.dict_ok(a) || a.chars().count() <= 6;
            if a_ok && (b.is_empty() || self.check(b)) {
                return true;
            }
        }
        false
    }

    pub fn suggest(&self, w: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(d) = &self.dict {
            let w2 = w.replace('’', "'");
            d.suggest(&w2, &mut out);
            if out.is_empty() {
                if let Some(p) = w2.find('\'') {
                    let (a, b) = (&w2[..=p], &w2[p + 1..]);
                    let mut o2 = Vec::new();
                    d.suggest(b, &mut o2);
                    out = o2.into_iter().map(|s| format!("{a}{s}")).collect();
                }
            }
        }
        out.retain(|s| !s.starts_with('-') && !s.ends_with('-'));
        out.dedup();
        out
    }

    /// Words matching a pattern (`?`, `*`), or suggestions for a plain word.
    pub fn look_up(&self, pat: &str) -> Vec<String> {
        let pat = pat.trim();
        if pat.contains(['?', '*']) {
            let p: Vec<char> = pat.chars().collect();
            let st = stems(&self.lang);
            let mut v: Vec<String> = st
                .iter()
                .filter(|s| wild_match(&p, &s.chars().collect::<Vec<_>>()))
                .take(500)
                .cloned()
                .collect();
            v.extend(self.sup.iter().filter(|s| wild_match(&p, &s.chars().collect::<Vec<_>>())).cloned());
            // Inflected forms are not stems: fill in up to two `?` with
            // every letter of the language and ask the dictionary.
            let q = p.iter().filter(|&&c| c == '?').count();
            if !pat.contains('*') && (1..=2).contains(&q) {
                let mut alpha: Vec<char> = st.iter().flat_map(|s| s.chars()).filter(|c| c.is_alphabetic()).collect();
                alpha.sort_unstable();
                alpha.dedup();
                let mut cands = vec![String::new()];
                for &c in &p {
                    cands = if c == '?' {
                        cands.iter().flat_map(|s| alpha.iter().map(move |a| format!("{s}{a}"))).collect()
                    } else {
                        cands.into_iter().map(|mut s| { s.push(c); s }).collect()
                    };
                }
                let mut found: Vec<String> = cands.into_iter().filter(|w| self.dict_ok(w)).collect();
                found.retain(|w| !v.contains(w));
                found.extend(v);
                v = found;
            }
            v
        } else {
            let mut v = Vec::new();
            if self.check(pat) {
                v.push(pat.to_string());
            }
            v.extend(self.suggest(pat).into_iter().filter(|s| s != pat));
            v
        }
    }

    fn add(&mut self, w: &str) {
        let w = w.replace('’', "'");
        append_sup(&self.sup_path, &w);
        self.sup.insert(w);
    }

    fn set_sup(&mut self, p: PathBuf) {
        self.sup = load_sup(&p);
        self.sup_path = p;
    }
}

// ============================================================ drawing utils

/// Draw the document into rows `0..n` with the word `hl` highlighted and
/// element `focus` in view.  Returns the screen position of `focus`.
fn draw_doc(wp: &Wp, g: &mut Grid, n: usize, focus: usize, hl: &[usize]) -> Option<(usize, usize)> {
    let e = wp.ed_ref();
    let th = &wp.theme;
    for r in 0..n.min(g.rows) {
        g.fill_row(r, 0, g.cols, ' ', th.normal);
    }
    let lines = &e.layout.lines;
    if lines.is_empty() || n == 0 {
        return None;
    }
    // display items: (line index, is page-break row)
    let mut items: Vec<(usize, bool)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        items.push((i, false));
        if l.break_after.is_some() {
            items.push((i, true));
        }
    }
    let fl = e.layout.line_of(focus);
    let fpos = items.iter().position(|&(i, b)| i == fl && !b).unwrap_or(0);
    let top = fpos.saturating_sub(n / 3).min(items.len().saturating_sub(n));
    let (fcol, _) = lines[fl].caret_of(focus);
    let left = if fcol >= g.cols as i32 - 5 { fcol - g.cols as i32 / 2 } else { 0 };
    let mut at = None;
    for (r, &(li, brk)) in items.iter().skip(top).take(n).enumerate() {
        let l = &lines[li];
        if brk {
            let ch = if l.break_after == Some(PageBreak::Hard) { '═' } else { '─' };
            g.fill_row(r, 0, g.cols, ch, th.page_break);
            continue;
        }
        for c in &l.cells {
            let x = c.col - left;
            if x < 0 || x as usize >= g.cols {
                continue;
            }
            let mut st = th.for_attrs(c.attrs);
            if hl.contains(&c.el) {
                st = th.block;
            }
            g.put(x as usize, r, c.ch, st);
        }
        if li == fl {
            at = Some(((fcol - left).clamp(0, g.cols as i32 - 1) as usize, r));
        }
    }
    at
}

fn bottom(wp: &Wp, g: &mut Grid, text: &str) -> usize {
    let row = g.rows - 1;
    g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
    g.text_mn(0, row, text, wp.theme.normal, wp.theme.bold)
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn letter_index(c: char) -> Option<usize> {
    let c = c.to_ascii_uppercase();
    c.is_ascii_uppercase().then(|| (c as u8 - b'A') as usize)
}

// ================================================================== Speller

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Word,
    Page,
    Doc,
    Block,
}

struct Run {
    kind: Kind,
    pos: usize,
    end: usize,
    count: usize,
    prev: Option<Word>,
}

#[derive(Clone, PartialEq)]
enum Ask {
    SupName,
    LookUp,
}

#[derive(Clone, PartialEq)]
enum St {
    Menu,
    Lang,
    NotFound,
    Double,
    Case(String),
    Edit { len0: usize },
    Ask(Ask, String),
    Done(usize),
}

pub struct Spell {
    st: St,
    /// state to return to when the Ask prompt is finished
    back: Box<St>,
    chk: Checker,
    run: Option<Run>,
    word: Option<Word>,
    list: Vec<String>,
    page: usize,
    msg: Option<String>,
    /// checkers of the other languages (kept while the Speller runs, so
    /// "Skip" lists survive switching between paragraphs)
    others: Vec<Checker>,
    /// the user picked the language (Check menu 7): no detection
    fixed: bool,
}

const PER_PAGE: usize = 24;

impl Spell {
    fn new(wp: &Wp) -> Spell {
        let chk = Checker::new(&current_lang(wp));
        let msg = chk.error().map(|e| format!("ERROR: {e}"));
        Spell { st: St::Menu, back: Box::new(St::Menu), chk, run: None, word: None, list: Vec::new(), page: 0, msg, others: Vec::new(), fixed: false }
    }

    /// Make `self.chk` the checker for `lang` (options carry over).
    fn use_lang(&mut self, lang: &str) {
        let code = lang_info(lang).code;
        if self.chk.lang == code {
            return;
        }
        let k = self.others.iter().position(|c| c.lang == code);
        let mut next = match k {
            Some(k) => self.others.remove(k),
            None => Checker::new(code),
        };
        if next.error().is_some() {
            return; // dictionary missing: stay with the current one
        }
        next.ignore_numbers = self.chk.ignore_numbers;
        next.double_check = self.chk.double_check;
        next.case_check = self.chk.case_check;
        let prev = std::mem::replace(&mut self.chk, next);
        self.others.push(prev);
    }

    /// Language for the word starting at `at` (explicit [Lang] code, else
    /// the paragraph's detected language; Cyrillic words are Russian).
    fn lang_for(&mut self, wp: &Wp, at: usize, word: &str) -> String {
        let default = current_lang(wp);
        let els = &wp.ed_ref().doc.els;
        let para = if self.fixed {
            self.chk.lang.clone()
        } else if let Some(l) = crate::aids::lang_code_at(els, at) {
            lang_info(&l).code.to_string()
        } else {
            crate::aids::detect_paragraph_lang_with(els, at, &default, &|l, w| Some(dictionary(l).map(|d| d.check(w)).unwrap_or(false)))
        };
        crate::aids::lang_for_word(&para, &default, word)
    }

    fn begin(&mut self, wp: &mut Wp, kind: Kind) {
        if let Some(e) = self.chk.error() {
            self.msg = Some(format!("ERROR: {e}"));
            return;
        }
        let e = wp.ed();
        e.ensure_layout();
        let len = e.doc.els.len();
        let (pos, end) = match kind {
            Kind::Doc => (0, len),
            Kind::Block => {
                let r = e.block_range().unwrap_or((0, len));
                e.block_anchor = None;
                r
            }
            Kind::Page => {
                let li = e.layout.line_of(e.cursor);
                let pg = e.layout.lines[li].page;
                let s = e.layout.lines.iter().find(|l| l.page == pg).map(|l| l.start).unwrap_or(0);
                let t = e.layout.lines.iter().rev().find(|l| l.page == pg).map(|l| l.end).unwrap_or(len);
                (s, t)
            }
            Kind::Word => {
                let s = word_start(&e.doc.els, e.cursor);
                let t = next_word(&e.doc.els, s, len).map(|w| w.end()).unwrap_or(len);
                (s, t)
            }
        };
        self.run = Some(Run { kind, pos, end, count: 0, prev: None });
        self.advance(wp);
    }

    /// Scan forward until a problem is found or the range is finished.
    fn advance(&mut self, wp: &mut Wp) {
        self.list.clear();
        self.page = 0;
        self.word = None;
        let Some(mut run) = self.run.take() else {
            self.st = St::Menu;
            return;
        };
        loop {
            let end = run.end.min(wp.ed_ref().doc.els.len());
            let Some(w) = next_word(&wp.ed_ref().doc.els, run.pos, end) else { break };
            run.pos = w.end();
            run.count += 1;
            let prev = run.prev.replace(w.clone());
            if !has_letter(&w.text) || (self.chk.ignore_numbers && has_digit(&w.text)) {
                continue;
            }
            // Arabic/Hebrew: no dictionary installed for them yet
            if crate::bidi::has_rtl(&w.text) && !dict_exists_for_rtl(&w.text) {
                continue;
            }
            // each paragraph in its own language
            let lang = self.lang_for(wp, w.start(), &w.text);
            self.use_lang(&lang);
            if self.chk.case_check && !self.chk.check(&w.text) {
                if let Some(f) = case_fix(&w.text) {
                    if self.chk.check(&f) {
                        self.focus(wp, &w);
                        self.word = Some(w);
                        self.st = St::Case(f);
                        self.run = Some(run);
                        return;
                    }
                }
            }
            if !self.chk.check(&w.text) {
                self.focus(wp, &w);
                self.list = self.chk.suggest(&w.text);
                self.word = Some(w);
                self.st = St::NotFound;
                self.run = Some(run);
                return;
            }
            if self.chk.double_check {
                if let Some(p) = prev {
                    let els = &wp.ed_ref().doc.els;
                    let between_spaces = (p.end()..w.start()).all(|i| match &els[i] {
                        El::Ch(c) => c.is_whitespace(),
                        e => is_invisible(e),
                    });
                    if between_spaces && has_letter(&p.text) && p.text.to_lowercase() == w.text.to_lowercase() {
                        self.focus(wp, &w);
                        self.word = Some(w);
                        self.st = St::Double;
                        self.run = Some(run);
                        return;
                    }
                }
            }
        }
        // finished
        if run.kind == Kind::Word {
            // WP moves on to the next word and keeps the Check menu up
            let e = wp.ed();
            let len = e.doc.els.len();
            e.cursor = next_word(&e.doc.els, run.pos.min(len), len).map(|w| w.start()).unwrap_or(run.pos.min(len));
            self.st = St::Menu;
        } else {
            self.st = St::Done(run.count);
        }
    }

    fn focus(&self, wp: &mut Wp, w: &Word) {
        wp.ed().cursor = w.start();
    }

    /// Replace the current word with `new` and continue.
    fn replace(&mut self, wp: &mut Wp, new: &str) {
        let Some(w) = self.word.take() else { return };
        let e = wp.ed();
        let len0 = e.doc.els.len();
        let after = replace_word(&mut e.doc.els, &w, new);
        let delta = e.doc.els.len() as isize - len0 as isize;
        e.modified = true;
        e.cursor = after;
        e.relayout();
        if let Some(r) = self.run.as_mut() {
            r.pos = after;
            r.end = (r.end as isize + delta).max(0) as usize;
            r.prev = next_word(&e.doc.els, w.start(), after.max(w.start() + 1).min(e.doc.els.len()));
        }
        self.advance(wp);
    }

    fn delete_second(&mut self, wp: &mut Wp) {
        let Some(w) = self.word.take() else { return };
        let Some(r) = self.run.as_mut() else { return };
        let e = wp.ed();
        // delete the whitespace before the second word and the word itself
        let mut s = w.start();
        while s > 0 {
            match &e.doc.els[s - 1] {
                El::Ch(c) if c.is_whitespace() => s -= 1,
                x if is_invisible(x) => s -= 1,
                _ => break,
            }
        }
        let n = delete_chars(&mut e.doc.els, s, w.end());
        e.modified = true;
        e.relayout();
        r.end = r.end.saturating_sub(n);
        r.count = r.count.saturating_sub(1);
        r.pos = w.end() - n;
        e.cursor = r.pos.min(e.doc.els.len());
        self.advance(wp);
    }

    fn start_edit(&mut self, wp: &mut Wp) {
        let len0 = wp.ed_ref().doc.els.len();
        if let Some(w) = &self.word {
            wp.ed().cursor = w.start();
        }
        self.st = St::Edit { len0 };
    }

    fn finish_edit(&mut self, wp: &mut Wp, len0: usize) {
        let e = wp.ed();
        e.ensure_layout();
        let delta = e.doc.els.len() as isize - len0 as isize;
        let cur = e.cursor;
        if let (Some(r), Some(w)) = (self.run.as_mut(), self.word.as_ref()) {
            r.end = ((r.end as isize) + delta).max(0) as usize;
            // re-check from the start of the edited word
            let s = word_start(&e.doc.els, w.start().min(cur).min(e.doc.els.len()));
            r.pos = s;
            r.prev = None;
            r.count = r.count.saturating_sub(1);
        }
        self.advance(wp);
    }

    fn skip(&mut self, wp: &mut Wp, always: bool) {
        if always {
            if let Some(w) = &self.word {
                self.chk.skip.insert(w.text.replace('’', "'"));
            }
        }
        self.advance(wp);
    }

    fn do_look_up(&mut self, pat: &str) {
        self.list = self.chk.look_up(pat);
        self.page = 0;
        if self.list.is_empty() {
            self.msg = Some("Word not found".into());
        }
    }

    fn pick(&mut self, wp: &mut Wp, c: char) -> bool {
        let Some(k) = letter_index(c) else { return false };
        let i = self.page * PER_PAGE + k;
        if k < PER_PAGE && i < self.list.len() {
            let w = self.list[i].clone();
            self.replace(wp, &w);
            return true;
        }
        false
    }

    fn more(&mut self) {
        if (self.page + 1) * PER_PAGE < self.list.len() {
            self.page += 1;
        } else {
            self.page = 0;
        }
    }

    fn word_count(wp: &Wp) -> usize {
        let els = &wp.ed_ref().doc.els;
        words(els, 0, els.len()).len()
    }

    fn stop(&mut self) {
        let n = self.run.take().map(|r| r.count).unwrap_or(0);
        self.word = None;
        self.st = St::Done(n);
    }
}

fn is_quit(i: &Input) -> bool {
    matches!(i, Input::Func(Func::Cancel | Func::Esc | Func::Exit))
}

impl Screen for Spell {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let had_msg = self.msg.take();
        match self.st.clone() {
            St::Menu => match i {
                Input::Char(c) => match c.to_ascii_lowercase() {
                    '1' | 'w' => self.begin(wp, Kind::Word),
                    '2' | 'p' => self.begin(wp, Kind::Page),
                    '3' | 'd' => self.begin(wp, Kind::Doc),
                    '4' | 'n' => {
                        self.back = Box::new(St::Menu);
                        self.st = St::Ask(Ask::SupName, self.chk.sup_path.display().to_string());
                    }
                    '5' | 'l' => {
                        self.back = Box::new(St::Menu);
                        self.st = St::Ask(Ask::LookUp, String::new());
                    }
                    '6' | 'c' => self.st = St::Done(Self::word_count(wp)),
                    '7' => self.st = St::Lang,
                    '0' => return Outcome::Close,
                    _ => self.msg = had_msg,
                },
                Input::Func(Func::Enter) | Input::Func(Func::Cancel | Func::Esc | Func::Exit) => {
                    wp.after_edit();
                    return Outcome::Close;
                }
                _ => {}
            },
            St::Lang => {
                if let Input::Char(c) = i {
                    if let Some(d) = c.to_digit(10) {
                        if d >= 1 && (d as usize) <= LANGS.len() {
                            set_lang(LANGS[d as usize - 1].code);
                            self.chk = Checker::new(LANGS[d as usize - 1].code);
                            self.fixed = true;
                            self.msg = self.chk.error().map(|e| format!("ERROR: {e}"));
                        }
                    }
                }
                self.st = St::Menu;
            }
            St::Ask(kind, mut text) => match i {
                Input::Char(c) => {
                    text.push(*c);
                    self.st = St::Ask(kind, text);
                }
                Input::Func(Func::Backspace) => {
                    text.pop();
                    self.st = St::Ask(kind, text);
                }
                Input::Func(Func::Enter) => match kind {
                    Ask::SupName => {
                        if !text.trim().is_empty() {
                            let p = wp.resolve_path(&text);
                            self.chk.set_sup(p);
                        }
                        self.st = *self.back.clone();
                    }
                    Ask::LookUp => {
                        if text.trim().is_empty() {
                            if *self.back == St::Menu {
                                self.list.clear();
                            }
                            self.st = *self.back.clone();
                        } else {
                            self.do_look_up(&text);
                            if *self.back == St::NotFound {
                                self.st = St::NotFound;
                            } else {
                                self.st = St::Ask(Ask::LookUp, String::new());
                            }
                        }
                    }
                },
                Input::Func(Func::Cancel | Func::Esc | Func::Exit) => {
                    if *self.back == St::Menu {
                        self.list.clear();
                    }
                    self.st = *self.back.clone();
                }
                _ => {}
            },
            St::NotFound => {
                let digits = self.word.as_ref().is_some_and(|w| has_digit(&w.text));
                match i {
                    Input::Char(c) if c.is_ascii_digit() => match (digits, *c) {
                        (false, '1') | (true, '1') => self.skip(wp, false),
                        (false, '2') | (true, '2') => self.skip(wp, true),
                        (false, '3') => {
                            if let Some(w) = &self.word {
                                let t = w.text.clone();
                                self.chk.add(&t);
                            }
                            self.advance(wp);
                        }
                        (false, '4') | (true, '4') => self.start_edit(wp),
                        (false, '5') => {
                            self.back = Box::new(St::NotFound);
                            let t = self.word.as_ref().map(|w| w.text.clone()).unwrap_or_default();
                            self.st = St::Ask(Ask::LookUp, t);
                        }
                        (false, '6') | (true, '3') => {
                            self.chk.ignore_numbers = true;
                            if digits {
                                self.advance(wp);
                            }
                        }
                        _ => {}
                    },
                    Input::Char(c) => {
                        self.pick(wp, *c);
                    }
                    Input::Func(Func::Enter) => self.more(),
                    Input::Func(Func::Left | Func::Right) => self.start_edit(wp),
                    _ if is_quit(i) => self.stop(),
                    _ => {}
                }
            }
            St::Double => match i {
                Input::Char('1' | '2') => self.advance(wp),
                Input::Char('3') => self.delete_second(wp),
                Input::Char('4') => self.start_edit(wp),
                Input::Func(Func::Left | Func::Right) => self.start_edit(wp),
                Input::Char('5') => {
                    self.chk.double_check = false;
                    self.advance(wp);
                }
                _ if is_quit(i) => self.stop(),
                _ => {}
            },
            St::Case(fix) => match i {
                Input::Char('1' | '2') => self.advance(wp),
                Input::Char('3') => self.replace(wp, &fix),
                Input::Char('4') => self.start_edit(wp),
                Input::Func(Func::Left | Func::Right) => self.start_edit(wp),
                Input::Char('5') => {
                    self.chk.case_check = false;
                    // the word may still be misspelt: re-check it
                    if let (Some(r), Some(w)) = (self.run.as_mut(), self.word.as_ref()) {
                        r.pos = w.start();
                        r.count = r.count.saturating_sub(1);
                    }
                    self.advance(wp);
                }
                _ if is_quit(i) => self.stop(),
                _ => {}
            },
            St::Edit { len0 } => match i {
                Input::Func(Func::Exit) | Input::Func(Func::Spell) => self.finish_edit(wp, len0),
                Input::Func(Func::Cancel | Func::Esc) if wp.ed_ref().block_anchor.is_none() => {
                    self.finish_edit(wp, len0)
                }
                _ => {
                    wp.edit_input(i.clone());
                }
            },
            St::Done(_) => {
                wp.after_edit();
                return Outcome::Close;
            }
        }
        wp.ed().ensure_layout();
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let th = &wp.theme;
        let st = th.normal;
        let rows = g.rows;
        let split = rows / 2;
        let show_list = matches!(self.st, St::NotFound | St::Double | St::Case(_))
            || (!self.list.is_empty() && matches!(self.st, St::Ask(..) | St::Menu))
            || matches!(&self.st, St::Ask(Ask::LookUp, _) if *self.back == St::NotFound);
        if show_list {
            let (focus, hl) = match &self.word {
                Some(w) => (w.start(), w.idx.clone()),
                None => (wp.ed_ref().cursor, vec![]),
            };
            let cur = draw_doc(wp, g, split, focus, &hl);
            g.cursor = cur;
            for r in split..rows {
                g.fill_row(r, 0, g.cols, ' ', st);
            }
            g.fill_row(split, 0, g.cols, '═', st);
            if matches!(self.st, St::NotFound | St::Ask(..) | St::Menu) {
                let colw = g.cols / 4;
                let start = self.page * PER_PAGE;
                for (k, w) in self.list.iter().skip(start).take(PER_PAGE).enumerate() {
                    let r = split + 2 + k / 4;
                    if r >= rows - 2 {
                        break;
                    }
                    let x = (k % 4) * colw + 1;
                    let c = g.text(x, r, &format!("{}.", (b'A' + k as u8) as char), th.bold);
                    g.text(c + 1, r, &truncate(w, colw - 5), st);
                }
                if self.list.len() > PER_PAGE {
                    g.text(0, rows - 2, "Press Enter for more words", st);
                }
            }
        }
        // bottom line
        let row = rows - 1;
        let menu: String = match &self.st {
            St::Menu => {
                let l = lang_info(&self.chk.lang);
                if !show_list {
                    g.fill_row(rows - 2, 0, g.cols, ' ', st);
                    let t = format!("Language: {} {}  (7 Change Language)", l.code, l.name);
                    let t = self.msg.clone().unwrap_or(t);
                    g.text(0, rows - 2, &t, st);
                }
                "Check: 1 ~Word; 2 ~Page; 3 ~Document; 4 ~New Sup. Dictionary; 5 ~Look Up; 6 ~Count: 0".into()
            }
            St::Lang => {
                g.fill_row(rows - 2, 0, g.cols, ' ', st);
                let names: Vec<String> = LANGS.iter().map(|l| l.name.to_string()).collect();
                g.text(0, rows - 2, &truncate(&names.join(" · "), g.cols), st);
                let items: Vec<String> = LANGS.iter().enumerate().map(|(k, l)| format!("{} {}", k + 1, l.code)).collect();
                format!("Language: {}: 0", items.join("; "))
            }
            St::NotFound => {
                if self.word.as_ref().is_some_and(|w| has_digit(&w.text)) {
                    "Not Found: 1 2 Skip; 3 Ignore Words Containing Numbers; 4 Edit: 0".into()
                } else {
                    "Not Found: 1 Skip Once; 2 Skip; 3 Add; 4 Edit; 5 Look Up; 6 Ignore Numbers: 0".into()
                }
            }
            St::Double => "Double Word: 1 2 Skip; 3 Delete 2nd; 4 Edit; 5 Disable Double Word Checking".into(),
            St::Case(_) => "Irregular Case: 1 2 Skip; 3 Replace; 4 Edit; 5 Disable Case Checking".into(),
            St::Edit { .. } => "Spell Edit:  Press Exit when done".into(),
            St::Ask(Ask::SupName, t) => format!("Supplemental dictionary name: {t}"),
            St::Ask(Ask::LookUp, t) => format!("Word or word pattern: {t}"),
            St::Done(n) => format!("Word count: {n}  Press any key to continue"),
        };
        if let St::Edit { .. } = self.st {
            // keep the editor's own cursor; just replace the status line
            g.fill_row(row, 0, g.cols, ' ', st);
            g.text(0, row, &menu, st);
            return;
        }
        let c = bottom(wp, g, &menu);
        if let (Some(m), false) = (&self.msg, matches!(self.st, St::Menu)) {
            g.fill_row(rows - 2, 0, g.cols, ' ', st);
            g.text(0, rows - 2, m, st);
        }
        if !matches!(self.st, St::Done(_)) {
            let col = if menu.ends_with(": 0") { c - 1 } else { c };
            g.cursor = Some((col.min(g.cols - 1), row));
        } else {
            g.cursor = None;
        }
    }
}

/// Ctrl-F2.
pub fn start(wp: &mut Wp) {
    let mut s = Spell::new(wp);
    if wp.ed_ref().block_anchor.is_some() {
        s.begin(wp, Kind::Block);
    }
    wp.push(Box::new(s));
}

// =============================================================== Thesaurus

/// A MyThes thesaurus held in memory with an index of its headwords.
pub struct Thesaurus {
    data: String,
    index: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct Meaning {
    pub pos: String,
    pub words: Vec<String>,
}

impl Thesaurus {
    pub fn parse(raw: &[u8]) -> Thesaurus {
        let first_nl = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
        let enc = String::from_utf8_lossy(&raw[..first_nl]).trim().to_ascii_uppercase();
        let data = if enc.contains("UTF") {
            String::from_utf8_lossy(raw).into_owned()
        } else {
            raw.iter().map(|&b| b as char).collect()
        };
        let mut index = HashMap::new();
        let mut off = data.find('\n').map(|i| i + 1).unwrap_or(data.len());
        let mut pending = 0usize;
        while off < data.len() {
            let end = data[off..].find('\n').map(|i| off + i).unwrap_or(data.len());
            let line = &data[off..end];
            if pending > 0 {
                pending -= 1;
            } else if let Some((w, n)) = line.trim_end().rsplit_once('|') {
                if let Ok(n) = n.parse::<usize>() {
                    index.entry(w.to_lowercase()).or_insert(off);
                    pending = n;
                }
            }
            off = end + 1;
        }
        Thesaurus { data, index }
    }

    pub fn contains(&self, w: &str) -> bool {
        self.index.contains_key(&clean(w).to_lowercase())
    }

    pub fn lookup(&self, w: &str) -> Option<(String, Vec<Meaning>)> {
        let off = *self.index.get(&w.trim().to_lowercase())?;
        let mut lines = self.data[off..].lines();
        let head = lines.next()?.trim_end();
        let (hw, n) = head.rsplit_once('|')?;
        let n: usize = n.parse().ok()?;
        let mut v = Vec::new();
        for l in lines.take(n) {
            let mut parts = l.trim_end().split('|');
            let pos = parts.next().unwrap_or("").to_string();
            let words: Vec<String> = parts.filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
            if !words.is_empty() {
                v.push(Meaning { pos, words });
            }
        }
        Some((hw.to_string(), v))
    }
}

/// Strip annotations like "(ugs.)" from a thesaurus entry.
fn clean(w: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in w.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1i32).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

static THES: std::sync::LazyLock<Mutex<HashMap<String, Option<Arc<Thesaurus>>>>> =
    std::sync::LazyLock::new(Default::default);

pub fn thesaurus_for(code: &str) -> Option<Arc<Thesaurus>> {
    let l = lang_info(code);
    let mut g = THES.lock().unwrap();
    g.entry(l.thes.to_string())
        .or_insert_with(|| {
            let p = find_file("thes", &[l.thes.to_string()])?;
            let raw = std::fs::read(p).ok()?;
            Some(Arc::new(Thesaurus::parse(&raw)))
        })
        .clone()
}

/// Look a word up, trying a few spellings (as typed, lower case,
/// capitalised).
fn thes_lookup(t: &Thesaurus, w: &str) -> Option<(String, Vec<Meaning>)> {
    let w = w.replace('’', "'");
    t.lookup(&w).or_else(|| {
        // strip an elided article: l'acqua → acqua
        w.find('\'').and_then(|p| t.lookup(&w[p + 1..]))
    })
}

struct Col {
    head: String,
    /// (group number on first entry, pos label, entry text)
    lines: Vec<Line>,
    scroll: usize,
}

#[derive(Clone)]
enum Line {
    Group(usize, String),
    Entry(Option<usize>, String, bool),
    Blank,
}

impl Col {
    fn new(head: String, ms: Vec<Meaning>, th: &Thesaurus) -> Col {
        let mut lines = Vec::new();
        for (gi, m) in ms.iter().enumerate() {
            if gi > 0 {
                lines.push(Line::Blank);
            }
            let pos = m.pos.trim();
            if !pos.is_empty() && pos != "-" {
                lines.push(Line::Group(gi + 1, pos.to_string()));
            }
            for (k, w) in m.words.iter().enumerate() {
                let num = (k == 0 && (pos.is_empty() || pos == "-")).then_some(gi + 1);
                lines.push(Line::Entry(num, w.clone(), th.contains(w)));
            }
        }
        Col { head, lines, scroll: 0 }
    }
}

#[derive(Clone, PartialEq)]
enum TSt {
    Main,
    Replace,
    Ask(String),
    View,
}

pub struct Thes {
    th: Option<Arc<Thesaurus>>,
    cols: Vec<Col>,
    active: usize,
    first: usize,
    word: Option<Word>,
    st: TSt,
    msg: Option<String>,
}

const THES_DOC_ROWS: usize = 5;

impl Thes {
    fn body_rows(rows: usize) -> usize {
        rows.saturating_sub(THES_DOC_ROWS + 2)
    }

    /// Entry lines of the active column visible on screen, with letters.
    fn visible_entries(&self, rows: usize) -> Vec<(usize, String)> {
        let Some(c) = self.cols.get(self.active) else { return vec![] };
        let h = Self::body_rows(rows).saturating_sub(1);
        c.lines
            .iter()
            .enumerate()
            .skip(c.scroll)
            .take(h)
            .filter_map(|(i, l)| match l {
                Line::Entry(_, w, _) => Some((i, w.clone())),
                _ => None,
            })
            .take(26)
            .collect()
    }

    fn add(&mut self, w: &str) -> bool {
        let Some(th) = self.th.clone() else {
            self.msg = Some("ERROR: Thesaurus file not found".into());
            return false;
        };
        let cw = clean(w);
        match thes_lookup(&th, &cw) {
            Some((hw, ms)) => {
                // show the headword with the capitalisation used in the entries
                let hw = ms
                    .iter()
                    .flat_map(|m| m.words.iter())
                    .map(|w| clean(w))
                    .find(|w| w.to_lowercase() == hw.to_lowercase())
                    .unwrap_or(hw);
                let col = Col::new(hw, ms, &th);
                if self.cols.is_empty() {
                    self.cols.push(col);
                    self.active = 0;
                } else {
                    self.cols.truncate(self.active + 1);
                    self.cols.push(col);
                    self.active += 1;
                }
                self.fix_first();
                true
            }
            None => {
                self.msg = Some("Word not found".into());
                false
            }
        }
    }

    fn fix_first(&mut self) {
        if self.active < self.first {
            self.first = self.active;
        } else if self.active >= self.first + 3 {
            self.first = self.active - 2;
        }
    }

    fn replace(&mut self, wp: &mut Wp, new: &str) {
        let mut new = clean(new);
        if let Some(w) = &self.word {
            let orig = &w.text;
            if orig.chars().count() > 1 && orig.chars().all(|c| !c.is_lowercase()) {
                new = new.to_uppercase();
            } else if orig.chars().next().is_some_and(|c| c.is_uppercase()) {
                let mut cs = new.chars();
                if let Some(f) = cs.next() {
                    new = f.to_uppercase().chain(cs).collect();
                }
            }
            let e = wp.ed();
            let after = replace_word(&mut e.doc.els, w, &new);
            e.cursor = after;
            e.modified = true;
            e.relayout();
        } else {
            let els: Vec<El> = new.chars().map(El::Ch).collect();
            wp.ed().insert_els(&els);
        }
        wp.after_edit();
    }
}

impl Screen for Thes {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        self.msg = None;
        let rows = wp.rows;
        match self.st.clone() {
            TSt::Main => match i {
                Input::Char('1') => {
                    if !self.cols.is_empty() {
                        self.st = TSt::Replace
                    }
                }
                Input::Char('2') => self.st = TSt::View,
                Input::Char('3') => self.st = TSt::Ask(String::new()),
                Input::Char('4') => {
                    if !self.cols.is_empty() {
                        self.cols.remove(self.active);
                        if self.active >= self.cols.len() && self.active > 0 {
                            self.active -= 1;
                        }
                        self.fix_first();
                        if self.cols.is_empty() {
                            self.st = TSt::Ask(String::new());
                        }
                    }
                }
                Input::Char('0') => return Outcome::Close,
                Input::Char(c) => {
                    if let Some(k) = letter_index(*c) {
                        if let Some((_, w)) = self.visible_entries(rows).get(k).cloned() {
                            self.add(&w);
                        }
                    }
                }
                Input::Func(Func::Left) => {
                    self.active = self.active.saturating_sub(1);
                    self.fix_first();
                }
                Input::Func(Func::Right) => {
                    if self.active + 1 < self.cols.len() {
                        self.active += 1;
                        self.fix_first();
                    }
                }
                Input::Func(Func::Up | Func::Down | Func::PgUp | Func::PgDn | Func::ScreenUp | Func::ScreenDown) => {
                    let h = Self::body_rows(rows).saturating_sub(1).max(1);
                    if let Some(c) = self.cols.get_mut(self.active) {
                        let max = c.lines.len().saturating_sub(h);
                        c.scroll = match i {
                            Input::Func(Func::Up) => c.scroll.saturating_sub(1),
                            Input::Func(Func::Down) => (c.scroll + 1).min(max),
                            Input::Func(Func::PgUp | Func::ScreenUp) => c.scroll.saturating_sub(h),
                            _ => (c.scroll + h).min(max),
                        };
                    }
                }
                _ if is_quit(i) || matches!(i, Input::Func(Func::Enter)) => return Outcome::Close,
                _ => {}
            },
            TSt::Replace => {
                if let Input::Char(c) = i {
                    if let Some(k) = letter_index(*c) {
                        if let Some((_, w)) = self.visible_entries(rows).get(k).cloned() {
                            self.replace(wp, &w);
                            return Outcome::Close;
                        }
                    }
                }
                self.st = TSt::Main;
            }
            TSt::Ask(mut t) => match i {
                Input::Char(c) => {
                    t.push(*c);
                    self.st = TSt::Ask(t);
                }
                Input::Func(Func::Backspace) => {
                    t.pop();
                    self.st = TSt::Ask(t);
                }
                Input::Func(Func::Enter) => {
                    if t.trim().is_empty() {
                        if self.cols.is_empty() {
                            return Outcome::Close;
                        }
                        self.st = TSt::Main;
                    } else if self.add(t.trim()) {
                        self.st = TSt::Main;
                    } else {
                        self.st = TSt::Ask(String::new());
                    }
                }
                _ if is_quit(i) => {
                    if self.cols.is_empty() {
                        return Outcome::Close;
                    }
                    self.st = TSt::Main;
                }
                _ => {}
            },
            TSt::View => match i {
                Input::Func(Func::Left | Func::Right | Func::Up | Func::Down | Func::PgUp | Func::PgDn | Func::WordLeft | Func::WordRight | Func::Home | Func::End | Func::ScreenUp | Func::ScreenDown) => {
                    wp.edit_input(i.clone());
                }
                _ => self.st = TSt::Main,
            },
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let th = &wp.theme;
        let st = th.normal;
        let rows = g.rows;
        if self.st == TSt::View {
            g.fill_row(rows - 1, 0, g.cols, ' ', st);
            g.text(0, rows - 1, "Press Exit when done", st);
            return;
        }
        let (focus, hl) = match &self.word {
            Some(w) => (w.start(), w.idx.clone()),
            None => (wp.ed_ref().cursor, vec![]),
        };
        draw_doc(wp, g, THES_DOC_ROWS, focus, &hl);
        for r in THES_DOC_ROWS..rows {
            g.fill_row(r, 0, g.cols, ' ', st);
        }
        g.fill_row(THES_DOC_ROWS, 0, g.cols, '═', st);
        let colw = g.cols / 3;
        let h = Self::body_rows(rows);
        let letters: HashMap<usize, char> = self
            .visible_entries(rows)
            .iter()
            .enumerate()
            .map(|(k, (li, _))| (*li, (b'A' + k as u8) as char))
            .collect();
        for (ci, c) in self.cols.iter().enumerate().skip(self.first).take(3) {
            let x = (ci - self.first) * colw;
            let top = THES_DOC_ROWS + 1;
            g.text(x + 3, top, &truncate(&c.head, colw - 4), th.bold);
            for (r, (li, l)) in c.lines.iter().enumerate().skip(c.scroll).take(h - 1).enumerate() {
                let y = top + 1 + r;
                match l {
                    Line::Blank => {}
                    Line::Group(n, pos) => {
                        g.text(x, y, &format!("{n:>2}"), st);
                        g.text(x + 5, y, &truncate(pos, colw - 6), st);
                    }
                    Line::Entry(n, w, bullet) => {
                        if let Some(n) = n {
                            g.text(x, y, &format!("{n:>2}"), st);
                        }
                        if ci == self.active {
                            if let Some(ch) = letters.get(&li) {
                                g.put(x + 3, y, *ch, th.bold);
                            }
                        }
                        if *bullet {
                            g.put(x + 5, y, '•', st);
                        }
                        let avail = colw - 7;
                        let shown = if w.chars().count() > avail { clean(w) } else { w.clone() };
                        g.text(x + 6, y, &truncate(&shown, avail), st);
                    }
                }
            }
        }
        let row = rows - 1;
        let text = match &self.st {
            TSt::Replace => "Press letter for word".to_string(),
            TSt::Ask(t) => format!("Word: {t}"),
            _ => self.msg.clone().unwrap_or_else(|| "1 Replace Word; 2 View Doc; 3 Look Up Word; 4 Clear Column: 0".into()),
        };
        let mut c = bottom(wp, g, &text);
        if let (Some(m), TSt::Ask(_)) = (&self.msg, &self.st) {
            g.fill_row(row - 1, 0, g.cols, ' ', st);
            g.text(0, row - 1, m, st);
        }
        if text.ends_with(": 0") {
            c -= 1;
        }
        g.cursor = Some((c.min(g.cols - 1), row));
    }
}

/// Alt-F1.
pub fn thesaurus(wp: &mut Wp) {
    let lang = current_lang(wp);
    let th = thesaurus_for(&lang);
    let word = {
        let e = wp.ed_ref();
        word_at(&e.doc.els, e.cursor).filter(|w| {
            // only the word actually under/next to the cursor
            w.start() <= e.cursor + 1 && has_letter(&w.text)
        })
    };
    let mut t = Thes { th, cols: Vec::new(), active: 0, first: 0, word: word.clone(), st: TSt::Main, msg: None };
    if t.th.is_none() {
        wp.flash(&format!("ERROR: {} not found", lang_info(&lang).thes));
        return;
    }
    match &word {
        Some(w) if t.add(&w.text) => {}
        _ => t.st = TSt::Ask(String::new()),
    }
    wp.push(Box::new(t));
}

// ==================================================================== tests

#[cfg(test)]
mod tests {
    use super::*;

    fn els(s: &str) -> Vec<El> {
        crate::ui::prompt_to_els(s)
    }

    #[test]
    fn word_iteration_skips_codes() {
        let e = els("Ein [BOLD]Feh[bold]lr, l'acqua 3D-Text.");
        let w = words(&e, 0, e.len());
        let t: Vec<&str> = w.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(t, ["Ein", "Fehlr", "l'acqua", "3D", "Text"]);
        let mut e2 = els("Wort");
        e2.insert(2, El::Code(Code::Note { kind: NoteKind::Foot, els: els("Nota") }));
        assert_eq!(words(&e2, 0, e2.len()).len(), 2);
        assert_eq!(word_at(&e, 6).unwrap().text, "Fehlr");
        assert_eq!(word_at(&e, 3).unwrap().text, "Ein");
    }

    #[test]
    fn replace_keeps_codes() {
        let mut e = els("Ein [BOLD]Feh[bold]lr hier");
        let w = word_at(&e, 5).unwrap();
        let end = replace_word(&mut e, &w, "Fehler");
        assert_eq!(crate::ui::els_to_prompt(&e), "Ein [BOLD]Feh[bold]ler hier");
        assert_eq!(e[end], El::Ch(' '));
        let w = word_at(&e, 5).unwrap();
        replace_word(&mut e, &w, "Fe");
        assert_eq!(crate::ui::els_to_prompt(&e), "Ein [BOLD]Fe[bold] hier");
    }

    #[test]
    fn case_rules() {
        assert_eq!(case_fix("THis").as_deref(), Some("This"));
        assert_eq!(case_fix("tHis").as_deref(), Some("This"));
        assert_eq!(case_fix("tHIS").as_deref(), Some("THIS"));
        assert_eq!(case_fix("This"), None);
    }

    #[test]
    fn wildcards() {
        let p: Vec<char> = "h?us*".chars().collect();
        assert!(wild_match(&p, &"Hausboot".chars().collect::<Vec<_>>()));
        assert!(!wild_match(&p, &"Hus".chars().collect::<Vec<_>>()));
    }

    #[test]
    fn german() {
        let c = Checker::new("DE");
        assert!(c.error().is_none(), "{:?}", c.error());
        assert!(c.check("Übersetzung"));
        assert!(c.check("Übersetzungsbüro"));
        assert!(!c.check("Ubersetzung"));
        let t = std::time::Instant::now();
        let s = c.suggest("Ubersetzung");
        eprintln!("DE suggest {:?} in {:?}", s, t.elapsed());
        assert!(s.iter().any(|w| w == "Übersetzung"), "{s:?}");
        assert!(c.look_up("Ü?ersetzung").iter().any(|w| w == "Übersetzung"));
        let s = c.suggest("Fehlr");
        assert!(s.iter().any(|w| w == "Fehler"), "{s:?}");
    }

    #[test]
    fn russian_italian_french() {
        let ru = Checker::new("RU");
        assert!(ru.check("перевод"));
        assert!(!ru.check("перивод"));
        let s = ru.suggest("перивод");
        assert!(s.iter().any(|w| w == "перевод"), "{s:?}");
        let it = Checker::new("IT");
        assert!(it.check("l'acqua"));
        assert!(it.check("traduzione"));
        let s = it.suggest("traduzzione");
        assert!(s.iter().any(|w| w == "traduzione"), "{s:?}");
        let fr = Checker::new("FR");
        assert!(fr.check("l'eau"));
        assert!(fr.check("qu'il"));
        let es = Checker::new("ES");
        assert!(es.check("traducción"));
        let en = Checker::new("US");
        assert!(en.check("translation") && !en.check("translaton"));
    }

    #[test]
    fn thesaurus_lookup() {
        let raw = "UTF-8\nhaus|2\n-|Gebäude|Heim (ugs.)\n(Subst.)|Familie\nheim|1\n-|Haus\n";
        let t = Thesaurus::parse(raw.as_bytes());
        let (hw, ms) = t.lookup("Haus").unwrap();
        assert_eq!(hw, "haus");
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].words, ["Gebäude", "Heim (ugs.)"]);
        assert!(t.contains("Heim (ugs.)"));
        assert!(t.lookup("Gebäude").is_none());
        assert_eq!(clean("Heim (ugs.)"), "Heim");

        let de = thesaurus_for("DE").expect("German thesaurus");
        let (_, ms) = de.lookup("Übersetzung").expect("Übersetzung");
        assert!(ms.iter().any(|m| m.words.iter().any(|w| w.contains("Translation"))), "{ms:?}");
        let en = thesaurus_for("US").expect("English thesaurus");
        assert!(en.lookup("house").is_some());
        let ru = thesaurus_for("RU").expect("Russian thesaurus");
        assert!(ru.lookup("дом").is_some());
        let it = thesaurus_for("IT").expect("Italian thesaurus");
        assert!(it.lookup("casa").is_some());
    }
}

/// Is there a dictionary for the right-to-left script of `word`?
/// (Arabic "AR" / Hebrew "HE" — not shipped yet; a user can add one.)
pub fn dict_exists_for_rtl(word: &str) -> bool {
    let code = if word.chars().any(|c| ('\u{590}'..='\u{5ff}').contains(&c)) { "HE" } else { "AR" };
    LANGS.iter().any(|l| l.code == code) && crate::aids::dict_ready(code)
}
