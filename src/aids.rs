//! Typing aids (Setup → Environment → Typing Aids), each switchable:
//! * language-aware smart quotes (DE „…“ ‚…‘, FR « … » with no-break spaces,
//!   IT/ES/RU «…», EN “…”), language from a `[Lang:XX]` code or the setting;
//! * spell-check as you type: misspelled words underlined red on screen
//!   (visible lines only; dictionaries load in the background);
//! * autocorrect of common typos (small built-in list per language plus the
//!   user's own list in `autocorrect.txt`).
//!
//! Also holds the extra settings of these features (recent files, line rate).

use crate::doc::*;
use crate::keys::{Func, Input};
use crate::menus::{Edit, Form, Item};
use crate::screens::*;
use crate::ui::{Screen, Wp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

// ================================================================ settings

#[derive(Clone, Debug)]
pub struct Ext {
    pub smart_quotes: bool,
    /// Quote style language: "AUTO" (= spelling language) or DE/FR/IT/ES/RU/US/UK.
    pub quote_lang: String,
    pub spell_as_you_type: bool,
    pub autocorrect: bool,
    /// Recently used documents, most recent first.
    pub recent: Vec<PathBuf>,
    /// Price per standard line (Normzeile) for Document Information, e.g. "1,20".
    pub line_rate: String,
    /// Characters per standard line (55 by default).
    pub line_chars: u32,
}

impl Default for Ext {
    fn default() -> Self {
        Ext {
            smart_quotes: true,
            quote_lang: "AUTO".into(),
            spell_as_you_type: true,
            autocorrect: true,
            recent: vec![],
            line_rate: String::new(),
            line_chars: 55,
        }
    }
}

impl Ext {
    /// One `key=value` line of settings.txt; false if not ours.
    pub fn load_kv(&mut self, k: &str, v: &str) -> bool {
        match k {
            "smart_quotes" => self.smart_quotes = v == "yes",
            "quote_lang" => self.quote_lang = v.to_ascii_uppercase(),
            "spell_as_you_type" => self.spell_as_you_type = v == "yes",
            "autocorrect" => self.autocorrect = v == "yes",
            "recent" => {
                if !v.is_empty() && self.recent.len() < crate::recent::MAX {
                    self.recent.push(PathBuf::from(v));
                }
            }
            "line_rate" => self.line_rate = v.to_string(),
            "line_chars" => self.line_chars = v.parse().unwrap_or(55).clamp(20, 200),
            _ => return false,
        }
        true
    }

    pub fn save_kv(&self) -> String {
        let yes = |b: bool| if b { "yes" } else { "no" };
        let mut t = format!(
            "smart_quotes={}\nquote_lang={}\nspell_as_you_type={}\nautocorrect={}\nline_rate={}\nline_chars={}\n",
            yes(self.smart_quotes),
            self.quote_lang,
            yes(self.spell_as_you_type),
            yes(self.autocorrect),
            self.line_rate,
            self.line_chars
        );
        for r in &self.recent {
            t.push_str(&format!("recent={}\n", r.display()));
        }
        t
    }
}

// ================================================================ language

/// The language of a `[Lang:XX]`-style code (any code whose Reveal Codes
/// text is `[Lang:XX]` / `[Language:XX]`).
fn code_lang(c: &Code) -> Option<String> {
    let t = crate::reveal::code_text(c);
    let rest = t.strip_prefix("[Lang:").or_else(|| t.strip_prefix("[Language:"))?;
    let l = rest.trim_end_matches(']').trim();
    let l: String = l.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!l.is_empty()).then(|| l.to_ascii_uppercase())
}

/// Language in effect at element `idx`: last `[Lang:XX]` code before it.
pub fn lang_code_at(els: &[El], idx: usize) -> Option<String> {
    els[..idx.min(els.len())].iter().rev().find_map(|e| e.code().and_then(code_lang))
}

/// Language for spelling at `idx` (WP code → our dictionary code).
pub fn spell_lang_at(wp: &Wp, els: &[El], idx: usize) -> String {
    match lang_code_at(els, idx) {
        Some(l) => crate::spell::lang_info(&l).code.to_string(),
        None => crate::spell::current_lang(wp),
    }
}

/// (open double, close double, open single, close single, no-break spaces inside?)
pub fn quote_style(lang: &str) -> (char, char, char, char, bool) {
    match crate::spell::lang_info(lang).code {
        "DE" => ('„', '“', '‚', '‘', false),
        "FR" => ('«', '»', '‹', '›', true),
        "IT" | "ES" => ('«', '»', '“', '”', false),
        "RU" => ('«', '»', '„', '“', false),
        _ => ('“', '”', '‘', '’', false),
    }
}

fn quote_lang_at(wp: &Wp, idx: usize) -> String {
    let els = &wp.ed_ref().doc.els;
    if let Some(l) = lang_code_at(els, idx) {
        return l;
    }
    match wp.settings.ext.quote_lang.as_str() {
        // the language of the paragraph being typed (falls back to the
        // spelling language while it is too short to tell)
        "AUTO" | "" => {
            let default = crate::spell::current_lang(wp);
            detect_paragraph_lang(els, idx, &default)
        }
        l => l.to_string(),
    }
}

/// The element before `idx` that is visible (skips attribute codes etc.).
fn prev_visible(els: &[El], idx: usize) -> Option<&El> {
    els[..idx.min(els.len())].iter().rev().find(|e| !crate::editor::is_invisible(e))
}

/// Does a quote typed at `idx` open (true) or close?
pub fn opens_at(els: &[El], idx: usize) -> bool {
    match prev_visible(els, idx) {
        None => true,
        Some(El::Ch(c)) => c.is_whitespace() || "([{<-–—/„‚«‹“‘\u{a0}".contains(*c),
        Some(El::Code(Code::HRt | Code::HPg | Code::Tab | Code::Indent | Code::DblIndent | Code::HardSpace | Code::MarRel)) => true,
        Some(_) => false,
    }
}

/// Elements to insert for a typed `"` or `'` at `idx`.
pub fn smart_quote(lang: &str, els: &[El], idx: usize, c: char) -> Vec<El> {
    let (od, cd, os, cs, nbsp) = quote_style(lang);
    let open = opens_at(els, idx);
    if c == '\'' {
        // apostrophe after a letter: l’acqua, don’t, geht’s
        if matches!(prev_visible(els, idx), Some(El::Ch(p)) if p.is_alphanumeric()) {
            return vec![El::Ch('’')];
        }
        return vec![El::Ch(if open { os } else { cs })];
    }
    let q = if open { od } else { cd };
    match (nbsp, open) {
        (true, true) => vec![El::Ch(q), El::Code(Code::HardSpace)],
        (true, false) => {
            if matches!(prev_visible(els, idx), Some(El::Code(Code::HardSpace))) {
                vec![El::Ch(q)]
            } else {
                vec![El::Code(Code::HardSpace), El::Ch(q)]
            }
        }
        _ => vec![El::Ch(q)],
    }
}

// =============================================================== autocorrect

const BUILTIN: &[(&str, &[(&str, &str)])] = &[
    (
        "DE",
        &[
            ("nciht", "nicht"),
            ("udn", "und"),
            ("dsa", "das"),
            ("eien", "eine"),
            ("acuh", "auch"),
            ("jetz", "jetzt"),
            ("vieleicht", "vielleicht"),
            ("Standart", "Standard"),
            ("Addresse", "Adresse"),
            ("agressiv", "aggressiv"),
            ("nähmlich", "nämlich"),
            ("Rythmus", "Rhythmus"),
            ("seperat", "separat"),
            ("Reperatur", "Reparatur"),
            ("wiederrum", "wiederum"),
            ("Vorraussetzung", "Voraussetzung"),
            ("vorraus", "voraus"),
            ("Ergebniss", "Ergebnis"),
            ("Übersetzugn", "Übersetzung"),
            ("bezüglic", "bezüglich"),
        ],
    ),
    (
        "FR",
        &[
            ("aevc", "avec"),
            ("dnas", "dans"),
            ("puor", "pour"),
            ("qeu", "que"),
            ("apparament", "apparemment"),
            ("developpement", "développement"),
            ("connection", "connexion"),
            ("addresse", "adresse"),
            ("language", "langage"),
        ],
    ),
    (
        "IT",
        &[
            ("perchè", "perché"),
            ("poichè", "poiché"),
            ("affinchè", "affinché"),
            ("benchè", "benché"),
            ("sopratutto", "soprattutto"),
            ("propio", "proprio"),
            ("daccordo", "d’accordo"),
            ("pò", "po’"),
            ("qual'è", "qual è"),
            ("qual’è", "qual è"),
        ],
    ),
    (
        "ES",
        &[
            ("tambien", "también"),
            ("porqe", "porque"),
            ("qeu", "que"),
            ("despues", "después"),
            ("asi", "así"),
            ("ademas", "además"),
            ("aqui", "aquí"),
        ],
    ),
    (
        "RU",
        &[
            ("сдесь", "здесь"),
            ("вобщем", "в общем"),
            ("извените", "извините"),
            ("агенство", "агентство"),
            ("прийдти", "прийти"),
            ("зделать", "сделать"),
            ("щас", "сейчас"),
        ],
    ),
    (
        "US",
        &[
            ("teh", "the"),
            ("adn", "and"),
            ("recieve", "receive"),
            ("seperate", "separate"),
            ("occured", "occurred"),
            ("definately", "definitely"),
            ("accomodate", "accommodate"),
            ("wich", "which"),
            ("untill", "until"),
            ("thier", "their"),
        ],
    ),
];

pub fn user_list_path() -> PathBuf {
    crate::settings::config_dir().join("autocorrect.txt")
}

thread_local! {
    static USER: RefCell<Option<(std::time::SystemTime, Vec<(Option<String>, String, String)>)>> = const { RefCell::new(None) };
}

/// The user's list: lines `wrong=right` or `XX:wrong=right` (language XX).
fn user_list() -> Vec<(Option<String>, String, String)> {
    let p = user_list_path();
    let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
    USER.with(|u| {
        let mut u = u.borrow_mut();
        if let Some((t, l)) = &*u {
            if *t == mtime {
                return l.clone();
            }
        }
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let l = parse_user_list(&text);
        *u = Some((mtime, l.clone()));
        l
    })
}

fn parse_user_list(text: &str) -> Vec<(Option<String>, String, String)> {
    let mut v = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((a, b)) = line.split_once('=') else { continue };
        let (lang, a) = match a.split_once(':') {
            Some((l, w)) if l.len() == 2 && l.chars().all(|c| c.is_ascii_alphabetic()) => (Some(l.to_ascii_uppercase()), w),
            _ => (None, a),
        };
        v.push((lang, a.trim().to_string(), b.trim().to_string()));
    }
    v
}

pub fn add_user_entry(wrong: &str, right: &str) -> std::io::Result<()> {
    let p = user_list_path();
    let mut t = std::fs::read_to_string(&p).unwrap_or_else(|_| "# Autocorrect: wrong=right   (or DE:wrong=right for one language)\n".into());
    if !t.ends_with('\n') {
        t.push('\n');
    }
    t.push_str(&format!("{}={}\n", wrong.trim(), right.trim()));
    std::fs::create_dir_all(crate::settings::config_dir())?;
    std::fs::write(p, t)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The correction for a typed word, if any.
pub fn correction(lang: &str, word: &str, user: &[(Option<String>, String, String)]) -> Option<String> {
    let code = crate::spell::lang_info(lang).code;
    let code = if code == "UK" { "US" } else { code };
    let builtin = BUILTIN.iter().find(|(l, _)| *l == code).map(|(_, v)| *v).unwrap_or(&[]);
    let pairs = user
        .iter()
        .filter(|(l, _, _)| l.as_deref().is_none_or(|l| crate::spell::lang_info(l).code == code))
        .map(|(_, a, b)| (a.as_str(), b.as_str()))
        .chain(builtin.iter().copied());
    let lower = word.to_lowercase();
    for (a, b) in pairs {
        if a == word {
            return Some(b.to_string());
        }
        // "Nciht" at sentence start → "Nicht"
        if a.to_lowercase() == lower && a.chars().next().is_some_and(|c| c.is_lowercase()) && word.chars().next().is_some_and(|c| c.is_uppercase()) && word.chars().skip(1).all(|c| !c.is_uppercase()) {
            return Some(capitalize(b));
        }
    }
    // TWo INitial CApitals → Two
    let c: Vec<char> = word.chars().collect();
    if c.len() >= 3 && c[0].is_uppercase() && c[1].is_uppercase() && c[2..].iter().all(|x| x.is_lowercase()) {
        let mut s: String = c[..1].iter().collect();
        s.extend(c[1..].iter().flat_map(|x| x.to_lowercase()));
        return Some(s);
    }
    None
}

/// The word that ends right before `idx` (letters/digits/apostrophes;
/// attribute codes inside are skipped), as a spell::Word.
fn word_before(els: &[El], idx: usize) -> Option<crate::spell::Word> {
    let mut k = idx.min(els.len());
    let mut idxs = vec![];
    while k > 0 {
        match &els[k - 1] {
            El::Ch(c) if c.is_alphanumeric() || *c == '\'' || *c == '’' => idxs.push(k - 1),
            e if crate::editor::is_invisible(e) && !matches!(e, El::Code(Code::Link(_) | Code::LinkEnd)) => {}
            _ => break,
        }
        k -= 1;
    }
    idxs.reverse();
    while idxs.first().is_some_and(|&i| matches!(els[i], El::Ch('\'' | '’'))) {
        idxs.remove(0);
    }
    if idxs.is_empty() {
        return None;
    }
    let text = idxs.iter().map(|&i| if let El::Ch(c) = els[i] { c } else { ' ' }).collect();
    Some(crate::spell::Word { idx: idxs, text })
}

/// Correct the word before the cursor (called when a word ends).
pub fn autocorrect_now(wp: &mut Wp) -> bool {
    let cur = wp.ed_ref().cursor;
    let Some(w) = word_before(&wp.ed_ref().doc.els, cur) else { return false };
    if w.end() != cur && wp.ed_ref().doc.els[w.end()..cur].iter().any(|e| !crate::editor::is_invisible(e)) {
        return false;
    }
    let lang = spell_lang_at(wp, &wp.ed_ref().doc.els, cur);
    let Some(new) = correction(&lang, &w.text, &user_list()) else { return false };
    let e = wp.ed();
    e.snapshot();
    let old_len = e.doc.els.len();
    crate::spell::replace_word(&mut e.doc.els, &w, &new);
    e.cursor = (cur + e.doc.els.len()).saturating_sub(old_len);
    e.modified = true;
    e.relayout();
    true
}

/// Typing hook for the edit screen.  Returns the input to process
/// normally, or None if it was handled here.
pub fn before_input(wp: &mut Wp, i: Input) -> Option<Input> {
    let ext = wp.settings.ext.clone();
    let e = wp.ed_ref();
    if e.block_anchor.is_some() || e.typeover || wp.home_count > 0 {
        return Some(i);
    }
    let tracked = e.doc.track;
    let word_end = match &i {
        Input::Char(c) => !(c.is_alphanumeric() || *c == '\'' || *c == '’' || *c == '-'),
        Input::Func(Func::Enter | Func::Tab | Func::Indent | Func::DblIndent) => true,
        _ => false,
    };
    if word_end && ext.autocorrect && !tracked {
        autocorrect_now(wp);
    }
    if let Input::Char(c @ ('"' | '\'')) = i {
        if ext.smart_quotes {
            let cur = wp.ed_ref().cursor;
            let lang = quote_lang_at(wp, cur);
            let els = smart_quote(&lang, &wp.ed_ref().doc.els, cur, c);
            if tracked {
                for el in els {
                    crate::track::insert_tracked(wp.ed(), el);
                }
            } else if els.len() == 1 {
                if let El::Ch(q) = els[0] {
                    return Some(Input::Char(q));
                }
            } else {
                for el in els {
                    match el {
                        El::Ch(q) => wp.ed().insert_char(q),
                        El::Code(cd) => wp.ed().insert_code(cd),
                    }
                }
            }
            return None;
        }
    }
    Some(i)
}

// ===================================================== spell-as-you-type

struct SpellCache {
    lang: String,
    checker: crate::spell::Checker,
    words: HashMap<String, bool>,
    made: std::time::Instant,
}

thread_local! {
    static CHECKERS: RefCell<Vec<SpellCache>> = const { RefCell::new(Vec::new()) };
}

static READY: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static LOADING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Is the dictionary loaded?  If not, start loading it in the background.
pub fn dict_ready(lang: &str) -> bool {
    if READY.lock().unwrap().iter().any(|l| l == lang) {
        return true;
    }
    // headless test runs: load right away so screenshots are deterministic
    if crate::links::HEADLESS.load(std::sync::atomic::Ordering::Relaxed) {
        let ok = crate::spell::dictionary(lang).is_ok();
        if ok {
            READY.lock().unwrap().push(lang.to_string());
        }
        return ok;
    }
    let mut loading = LOADING.lock().unwrap();
    if !loading.iter().any(|l| l == lang) {
        loading.push(lang.to_string());
        let l = lang.to_string();
        let _ = std::thread::Builder::new().name("dict".into()).spawn(move || {
            let ok = crate::spell::dictionary(&l).is_ok();
            if ok {
                READY.lock().unwrap().push(l);
            }
        });
    }
    false
}

/// Is `word` spelled correctly in `lang`?  None while the dictionary loads.
pub fn word_ok(lang: &str, word: &str) -> Option<bool> {
    if lang == "CJK" {
        return None;
    }
    if !dict_ready(lang) {
        return None;
    }
    CHECKERS.with(|c| {
        let mut c = c.borrow_mut();
        // refresh now and then (words added to the supplementary dictionary)
        c.retain(|s| s.made.elapsed() < std::time::Duration::from_secs(20));
        let k = match c.iter().position(|s| s.lang == lang) {
            Some(k) => k,
            None => {
                c.push(SpellCache { lang: lang.into(), checker: crate::spell::Checker::new(lang), words: HashMap::new(), made: std::time::Instant::now() });
                c.len() - 1
            }
        };
        let s = &mut c[k];
        if s.checker.error().is_some() {
            return Some(true);
        }
        if let Some(r) = s.words.get(word) {
            return Some(*r);
        }
        let r = s.checker.check(word);
        if s.words.len() > 20_000 {
            s.words.clear();
        }
        s.words.insert(word.to_string(), r);
        Some(r)
    })
}

/// Element indices of misspelled characters on a screen line
/// (`a..b`); the word being typed at the cursor is left alone.
pub fn misspelled(wp: &Wp, a: usize, b: usize) -> Vec<usize> {
    if !wp.settings.ext.spell_as_you_type {
        return vec![];
    }
    let e = wp.ed_ref();
    let els = &e.doc.els;
    let mut out = vec![];
    // An explicit [Lang:XX] code wins; otherwise the paragraph's language is
    // detected (she writes German, English, French, Italian, Spanish and
    // Russian side by side).
    let explicit = lang_code_at(els, a).map(|l| crate::spell::lang_info(&l).code.to_string());
    let default = crate::spell::current_lang(wp);
    let lang = explicit.unwrap_or_else(|| detect_paragraph_lang(els, a, &default));
    for w in crate::spell::words(els, a, b.min(els.len())) {
        if w.end() == e.cursor || w.text.chars().count() < 2 || w.text.chars().any(|c| c.is_numeric()) {
            continue;
        }
        // all-capital abbreviations (ILS, OACI, l'OACI) are not marked
        let core = w.text.rsplit(['\'', '\u{2019}']).next().unwrap_or(&w.text);
        if core.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
            continue;
        }
        let wl = lang_for_word(&lang, &default, &w.text);
        if word_ok(&wl, &w.text) == Some(false) && word_ok(&default, &w.text) != Some(true) {
            out.extend(w.idx.iter().copied());
        }
    }
    out
}

thread_local! {
    /// paragraph text hash → detected language
    static PARA_LANG: RefCell<HashMap<u64, String>> = RefCell::new(HashMap::new());
}

/// Guess the language of the paragraph containing element `at`: Cyrillic
/// letters mean Russian; otherwise the dictionary that knows most of its
/// first words.  Falls back to `default` while dictionaries load.
pub fn detect_paragraph_lang(els: &[El], at: usize, default: &str) -> String {
    detect_paragraph_lang_with(els, at, default, &|l, w| word_ok(l, w))
}

/// Which dictionary a word belongs to inside a paragraph of language
/// `para`: Cyrillic words are Russian; Latin words in a Russian paragraph
/// use the default language.
pub fn lang_for_word(para: &str, default: &str, word: &str) -> String {
    // Chinese/Japanese/Korean: no dictionary (never marked)
    if word.chars().any(crate::cjk::is_cjk) {
        return "CJK".into();
    }
    // right-to-left scripts have their own (optional) dictionaries
    if word.chars().any(|c| ('\u{590}'..='\u{5ff}').contains(&c)) {
        return "HE".into();
    }
    if crate::bidi::has_rtl(word) {
        return "AR".into();
    }
    let cyr = word.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c));
    if cyr && para != "RU" {
        "RU".into()
    } else if !cyr && para == "RU" {
        if default == "RU" { "DE".into() } else { default.to_string() }
    } else {
        para.to_string()
    }
}

/// As `detect_paragraph_lang`, with the spelling check supplied by the caller.
pub fn detect_paragraph_lang_with(els: &[El], at: usize, default: &str, ok: &dyn Fn(&str, &str) -> Option<bool>) -> String {
    let is_end = |e: &El| matches!(e, El::Code(Code::HRt | Code::HPg));
    let mut s = at.min(els.len());
    while s > 0 && !is_end(&els[s - 1]) {
        s -= 1;
    }
    let mut t = at.min(els.len());
    while t < els.len() && !is_end(&els[t]) && t - s < 4000 {
        t += 1;
    }
    let words: Vec<String> = crate::spell::words(els, s, t)
        .into_iter()
        .map(|w| w.text)
        .filter(|w| w.chars().count() >= 2 && !w.chars().any(|c| c.is_numeric()) && !w.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()))
        .take(40)
        .collect();
    if words.is_empty() {
        return default.to_string();
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for w in &words {
        for b in w.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0x20;
    }
    if let Some(l) = PARA_LANG.with(|m| m.borrow().get(&h).cloned()) {
        return l;
    }
    let letters: Vec<char> = words.iter().flat_map(|w| w.chars()).filter(|c| c.is_alphabetic()).collect();
    let cyr = letters.iter().filter(|c| ('\u{400}'..='\u{4ff}').contains(*c)).count();
    if cyr * 2 > letters.len() {
        return "RU".into();
    }
    let mut best = (0usize, default.to_string());
    let mut all_ready = true;
    for cand in ["DE", "US", "FR", "IT", "ES"] {
        let mut hits = 0;
        for w in &words {
            match ok(cand, w) {
                Some(true) => hits += 1,
                Some(false) => {}
                None => {
                    all_ready = false;
                    break;
                }
            }
        }
        // the default language wins ties
        if hits > best.0 || (hits == best.0 && cand == default) {
            best = (hits, cand.to_string());
        }
    }
    if all_ready {
        PARA_LANG.with(|m| {
            let mut m = m.borrow_mut();
            if m.len() > 5000 {
                m.clear();
            }
            m.insert(h, best.1.clone());
        });
    }
    best.1
}

// =================================================================== setup

fn set(wp: &mut Wp, f: impl FnOnce(&mut Ext)) {
    let mut s = wp.settings.clone();
    f(&mut s.ext);
    if let Err(e) = s.save() {
        wp.flash(&format!("ERROR: cannot save settings — {e}"));
    }
    wp.settings = s;
}

fn yn(b: bool) -> Vec<String> {
    vec![if b { "Yes".into() } else { "No".into() }]
}

const QLANGS: [&str; 8] = ["AUTO", "DE", "FR", "IT", "ES", "RU", "US", "UK"];

/// Setup → Environment → Typing Aids.
pub fn setup_screen() -> Box<dyn Screen> {
    let items = vec![
        Item {
            key: '1',
            labels: vec!["~Smart Quotes"],
            value: Rc::new(|wp| yn(wp.settings.ext.smart_quotes)),
            edit: Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.smart_quotes = y))),
        },
        Item {
            key: '2',
            labels: vec!["~Quote Style Language", "  (a [Lang] code in the text wins)"],
            value: Rc::new(|wp| {
                let l = wp.settings.ext.quote_lang.clone();
                let (a, b, _, _, sp) = quote_style(if l == "AUTO" { "DE" } else { &l });
                let ex = if sp { format!("{a} … {b}") } else { format!("{a}…{b}") };
                vec![if l == "AUTO" { "Same as spelling language".into() } else { format!("{l}  {ex}") }]
            }),
            edit: Edit::Choice(
                vec!["Auto (spelling language)", "Deutsch „…“", "Français « … »", "Italiano «…»", "Español «…»", "Russian «…»", "US English “…”", "UK English “…”"],
                Rc::new(|wp, k| set(wp, |s| s.quote_lang = QLANGS[k].into())),
            ),
        },
        Item {
            key: '3',
            labels: vec!["S~pell Check As You Type", "  (red underline)"],
            value: Rc::new(|wp| yn(wp.settings.ext.spell_as_you_type)),
            edit: Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.spell_as_you_type = y))),
        },
        Item {
            key: '4',
            labels: vec!["~Autocorrect Typing Errors"],
            value: Rc::new(|wp| yn(wp.settings.ext.autocorrect)),
            edit: Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.autocorrect = y))),
        },
        Item {
            key: '5',
            labels: vec!["Add to Autocorrect ~List", "  (your list file)"],
            value: Rc::new(|_| vec![format!("{} entries", user_list().len()), user_list_path().display().to_string()]),
            edit: Edit::Sub(Rc::new(|_| {
                Prompt::new("Wrong word:", "", |_wp, s| {
                    let wrong = s.filter(|s| !s.trim().is_empty())?;
                    Some(Prompt::new("Replace with:", "", move |wp, s| {
                        if let Some(right) = s.filter(|s| !s.trim().is_empty()) {
                            match add_user_entry(&wrong, &right) {
                                Ok(()) => wp.flash(&format!("Autocorrect: {} → {}", wrong.trim(), right.trim())),
                                Err(e) => wp.flash(&format!("ERROR: {e}")),
                            }
                        }
                        None
                    }) as Box<dyn Screen>)
                }) as Box<dyn Screen>
            })),
        },
    ];
    let mut f = Form::new("Setup: Typing Aids", items);
    f.footer = "Selection: 0".into();
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn els(s: &str) -> Vec<El> {
        Document::from_text(s).els
    }

    #[test]
    fn quotes_per_language() {
        let e = els("Er sagte ");
        assert_eq!(smart_quote("DE", &e, e.len(), '"'), vec![El::Ch('„')]);
        let e = els("Er sagte „Hallo");
        assert_eq!(smart_quote("DE", &e, e.len(), '"'), vec![El::Ch('“')]);
        assert_eq!(smart_quote("DE", &e, e.len(), '\''), vec![El::Ch('’')]);
        let e = els("Il dit ");
        assert_eq!(smart_quote("FR", &e, e.len(), '"'), vec![El::Ch('«'), El::Code(Code::HardSpace)]);
        let e = els("Il dit « oui");
        assert_eq!(smart_quote("FR", &e, e.len(), '"'), vec![El::Code(Code::HardSpace), El::Ch('»')]);
        assert_eq!(smart_quote("IT", &els(""), 0, '"'), vec![El::Ch('«')]);
        assert_eq!(smart_quote("RU", &els("да"), 2, '"'), vec![El::Ch('»')]);
        assert_eq!(smart_quote("US", &els("("), 1, '"'), vec![El::Ch('“')]);
        assert_eq!(smart_quote("US", &els("say "), 4, '\''), vec![El::Ch('‘')]);
    }

    #[test]
    fn lang_from_code() {
        let mut e = els("ab");
        e.insert(1, El::Code(Code::Unknown { name: "Lang:FR".into(), data: vec![] }));
        assert_eq!(lang_code_at(&e, 3).as_deref(), Some("FR"));
        assert_eq!(lang_code_at(&e, 1), None);
    }

    #[test]
    fn corrections() {
        let user = parse_user_list("# x\nbzw=beziehungsweise\nFR:ca=ça\n");
        assert_eq!(correction("DE", "nciht", &user).as_deref(), Some("nicht"));
        assert_eq!(correction("DE", "Nciht", &user).as_deref(), Some("Nicht"));
        assert_eq!(correction("DE", "bzw", &user).as_deref(), Some("beziehungsweise"));
        assert_eq!(correction("DE", "ca", &user), None);
        assert_eq!(correction("FR", "ca", &user).as_deref(), Some("ça"));
        assert_eq!(correction("IT", "perchè", &user).as_deref(), Some("perché"));
        assert_eq!(correction("US", "DOcument", &user).as_deref(), Some("Document"));
        assert_eq!(correction("US", "NATO", &user), None);
        assert_eq!(correction("DE", "Haus", &user), None);
    }

    #[test]
    fn typing_hooks() {
        let mut wp = Wp::new(80, 25);
        wp.settings.ext.quote_lang = "DE".into();
        wp.ed().insert_code(Code::Unknown { name: "Lang:DE".into(), data: vec![] });
        for c in "er sagte \"nciht\" ".chars() {
            wp.input(Input::Char(c));
        }
        let d = &wp.ed_ref().doc;
        assert_eq!(d.plain_text(0, d.len()), "er sagte „nicht“ ");
        // French with no-break spaces
        let mut wp = Wp::new(80, 25);
        wp.settings.ext.quote_lang = "FR".into();
        for c in "\"oui\"".chars() {
            wp.input(Input::Char(c));
        }
        let d = &wp.ed_ref().doc;
        assert_eq!(d.plain_text(0, d.len()), "«\u{a0}oui\u{a0}»");
        // switched off
        let mut wp = Wp::new(80, 25);
        wp.settings.ext.smart_quotes = false;
        wp.settings.ext.autocorrect = false;
        for c in "\"teh\" ".chars() {
            wp.input(Input::Char(c));
        }
        let d = &wp.ed_ref().doc;
        assert_eq!(d.plain_text(0, d.len()), "\"teh\" ");
    }

    #[test]
    fn settings_kv_roundtrip() {
        let mut x = Ext::default();
        x.smart_quotes = false;
        x.quote_lang = "FR".into();
        x.recent = vec![PathBuf::from("C:\\a b\\c.docx")];
        x.line_rate = "1,20".into();
        let mut y = Ext::default();
        for l in x.save_kv().lines() {
            let (k, v) = l.split_once('=').unwrap();
            assert!(y.load_kv(k, v));
        }
        assert_eq!(format!("{x:?}"), format!("{y:?}"));
    }
}

#[cfg(test)]
mod lang_detect_tests {
    use super::*;
    #[test]
    fn table_cells_detect_german() {
        crate::links::HEADLESS.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut d = Document::from_text("Erster Absatz mit etwas Text.\n");
        let mut t = crate::tables::new_table(3, 2, 7200);
        let words = ["Deutsch", "Englisch", "Russisch", "Vertrag", "contract", "договор"];
        let mut k = 0;
        let mut out = vec![];
        for e in t.drain(..) {
            let c = e == El::Code(Code::Cell);
            out.push(e);
            if c {
                out.extend(words[k].chars().map(El::Ch));
                k += 1;
            }
        }
        d.els.extend(out);
        let w: Vec<String> = crate::spell::words(&d.els, 0, d.els.len()).into_iter().map(|w| w.text).collect();
        assert!(w.contains(&"Deutsch".to_string()) && w.contains(&"contract".to_string()), "{w:?}");
        let at = d.els.iter().position(|e| *e == El::Ch('D')).unwrap();
        assert_eq!(detect_paragraph_lang(&d.els, at, "US"), "DE");
    }
}
