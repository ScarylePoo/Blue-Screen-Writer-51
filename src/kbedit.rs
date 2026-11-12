//! Setup → 5 Keyboard Layout: the list of .WPK keyboards
//! (`1 Select; 2 Delete; 3 Rename; 4 Create; 5 Copy; 6 Original; 7 Edit;
//! 8 Map; N Name search`), the `Keyboard: Edit` screen (`Key  Action
//! Description`) and the `Keyboard: Map` screen.  Keyboards are written
//! as real WP 5.1 .WPK files (wpk::encode).

use crate::grid::*;
use crate::keys::{Func, Input, K, KeyIn, Mods};
use crate::macroedit::{MacroEdit, word_name, words_text};
use crate::screens::*;
use crate::ui::{HitAction, Outcome, Screen, ScreenKind, Wp};
use crate::wpk::{KeyDef, key_label};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    /// The answer to "Name search:" in the keyboard list.
    static NAME_SEARCH: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Bumped whenever keyboard files are created, renamed or deleted.
static FILES_GEN: AtomicUsize = AtomicUsize::new(1);

fn files_changed() {
    FILES_GEN.fetch_add(1, Ordering::Relaxed);
}

/// Called for every physical key before the keyboard layout translates
/// it: a "Key:" prompt takes the key itself.
pub fn capture_key(wp: &mut Wp, ki: KeyIn) -> bool {
    if wp.screens.last().map(|s| s.kind()) != Some(ScreenKind::KeyCapture) {
        return false;
    }
    let _ = ki;
    // last_key is already set; the screen reads it.
    wp.input(Input::Char('\u{0}'));
    true
}

/// "Key: " — waits for one physical key.
struct KeyCapture {
    label: String,
    on_key: Option<Box<dyn FnOnce(&mut Wp, KeyIn) -> Next>>,
}

fn key_prompt(label: &str, f: impl FnOnce(&mut Wp, KeyIn) -> Next + 'static) -> Box<dyn Screen> {
    Box::new(KeyCapture { label: label.into(), on_key: Some(Box::new(f)) })
}

impl Screen for KeyCapture {
    fn kind(&self) -> ScreenKind {
        ScreenKind::KeyCapture
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let key = match (i, wp.last_key.take()) {
            (Input::Char('\u{0}'), Some(k)) => k,
            _ => return Outcome::Close, // a macro or Cancel
        };
        // the Cancel key of the active layout cancels
        if is_cancel_key(wp, key) {
            return Outcome::Close;
        }
        let f = self.on_key.take().unwrap();
        match f(wp, key) {
            Some(n) => Outcome::Replace(n),
            None => Outcome::Close,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let c = g.text(0, row, &self.label, wp.theme.normal);
        g.cursor = Some((c, row));
    }
}

fn is_cancel_key(wp: &Wp, k: KeyIn) -> bool {
    let i = if let Some(kb) = &wp.keyboard {
        match kb.map.get(&k) {
            Some(crate::wpk::KeyAction::Input(i)) => Some(i.clone()),
            Some(_) => None,
            None => crate::keys::default_map(k),
        }
    } else if wp.alternate_keys {
        crate::keys::alternate_map(k)
    } else {
        crate::keys::default_map(k)
    };
    i == Some(Input::Func(Func::Cancel))
}

// =============================================================== the list

fn keyboard_dir(wp: &Wp) -> PathBuf {
    wp.macro_dirs.iter().find(|d| d.is_dir()).cloned().or_else(|| wp.macro_dirs.first().cloned()).unwrap_or_else(|| wp.cwd.clone())
}

fn wpk_name(s: &str) -> String {
    let s = s.trim();
    if s.to_ascii_lowercase().ends_with(".wpk") { s.to_string() } else { format!("{}.WPK", s.to_uppercase()) }
}

pub struct KeyboardList {
    files: RefCell<Vec<PathBuf>>,
    seen: Cell<usize>,
    sel: Cell<usize>,
    dirs: Vec<PathBuf>,
}

fn scan(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = vec![];
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wpk")) {
                    files.push(p);
                }
            }
        }
    }
    files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_uppercase()));
    files.dedup();
    files
}

impl KeyboardList {
    pub fn new(wp: &Wp) -> KeyboardList {
        let mut dirs = wp.macro_dirs.clone();
        if dirs.is_empty() {
            dirs.push(wp.cwd.clone());
        }
        let files = scan(&dirs);
        let sel = wp.settings.keyboard.as_ref().and_then(|k| files.iter().position(|f| f == k)).unwrap_or(0);
        KeyboardList { files: RefCell::new(files), seen: Cell::new(FILES_GEN.load(Ordering::Relaxed)), sel: Cell::new(sel), dirs }
    }

    fn refresh(&self) {
        if let Some(n) = NAME_SEARCH.with(|n| n.borrow_mut().take()) {
            self.select_name(&n);
        }
        let g = FILES_GEN.load(Ordering::Relaxed);
        if g != self.seen.get() {
            let cur = self.selected();
            let files = scan(&self.dirs);
            let sel = cur.and_then(|c| files.iter().position(|f| *f == c)).unwrap_or(0);
            self.sel.set(sel.min(files.len().saturating_sub(1)));
            *self.files.borrow_mut() = files;
            self.seen.set(g);
        }
    }

    fn selected(&self) -> Option<PathBuf> {
        self.files.borrow().get(self.sel.get()).cloned()
    }

    fn select_name(&self, name: &str) {
        let n = name.to_uppercase();
        if let Some(k) = self.files.borrow().iter().position(|f| f.file_name().is_some_and(|x| x.to_string_lossy().to_uppercase().starts_with(&n))) {
            self.sel.set(k);
        }
    }
}

/// Ask for a new keyboard file name; `f` gets the full path.
fn name_prompt(label: &str, init: &str, f: impl FnOnce(&mut Wp, PathBuf) -> Next + 'static) -> Box<dyn Screen> {
    Prompt::new(label, init, move |wp, s| {
        let s = s?;
        if s.trim().is_empty() {
            return None;
        }
        let name = wpk_name(&s);
        let p = if name.contains(['\\', '/', ':']) { wp.resolve_path(&name) } else { keyboard_dir(wp).join(&name) };
        if p.exists() {
            wp.flash(&format!("ERROR: File already exists -- {}", p.display()));
            return None;
        }
        f(wp, p)
    })
}

impl Screen for KeyboardList {
    fn full_screen(&self) -> bool {
        true
    }
    fn kind(&self) -> ScreenKind {
        ScreenKind::List
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel.set(n.min(self.files.borrow().len().saturating_sub(1)));
        if double { self.input(wp, &Input::Char('1')) } else { Outcome::Keep }
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        self.refresh();
        let n = self.files.borrow().len();
        let sel = self.selected();
        let fname = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match i {
            Input::Func(Func::Down) => self.sel.set((self.sel.get() + 1).min(n.saturating_sub(1))),
            Input::Func(Func::Up) => self.sel.set(self.sel.get().saturating_sub(1)),
            Input::Char('1') | Input::Char('s') | Input::Char('S') | Input::Func(Func::Enter) => {
                if let Some(p) = sel {
                    match crate::wpk::load(&p) {
                        Ok(_) => crate::setup::change(wp, |s| s.keyboard = Some(p)),
                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                    }
                }
                return Outcome::Close;
            }
            Input::Char('2') | Input::Char('d') | Input::Char('D') => {
                if let Some(p) = sel {
                    let q = format!("Delete {}?", fname(&p));
                    return Outcome::Push(YesNo::new(&q, false, move |wp, yes| {
                        if yes {
                            if wp.settings.keyboard.as_ref() == Some(&p) {
                                crate::setup::change(wp, |s| s.keyboard = None);
                            }
                            if let Err(e) = std::fs::remove_file(&p) {
                                wp.flash(&format!("ERROR: {e}"));
                            }
                            files_changed();
                        }
                        None
                    }));
                }
            }
            Input::Char('3') | Input::Char('r') | Input::Char('R') => {
                if let Some(p) = sel {
                    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    return Outcome::Push(name_prompt("New name:", &stem, move |wp, to| {
                        match std::fs::rename(&p, &to) {
                            Ok(()) => {
                                if wp.settings.keyboard.as_ref() == Some(&p) {
                                    crate::setup::change(wp, |s| s.keyboard = Some(to));
                                }
                            }
                            Err(e) => wp.flash(&format!("ERROR: Can't rename file -- {e}")),
                        }
                        files_changed();
                        None
                    }));
                }
            }
            Input::Char('4') | Input::Char('c') | Input::Char('C') => {
                return Outcome::Push(name_prompt("Keyboard Filename:", "", |wp, p| {
                    if let Some(d) = p.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    if let Err(e) = std::fs::write(&p, crate::wpk::encode(&[])) {
                        wp.flash(&format!("ERROR: {e}"));
                        return None;
                    }
                    files_changed();
                    Some(KbEdit::open(&p, Vec::new()))
                }));
            }
            Input::Char('5') | Input::Char('y') | Input::Char('Y') => {
                if let Some(p) = sel {
                    return Outcome::Push(name_prompt("Copy to:", "", move |wp, to| {
                        if let Err(e) = std::fs::copy(&p, &to) {
                            wp.flash(&format!("ERROR: {e}"));
                        }
                        files_changed();
                        None
                    }));
                }
            }
            Input::Char('6') | Input::Char('o') | Input::Char('O') => {
                crate::setup::change(wp, |s| {
                    s.keyboard = None;
                    s.alternate_keys = false;
                });
                wp.flash("Original keyboard");
                return Outcome::Close;
            }
            Input::Char('7') | Input::Char('e') | Input::Char('E') | Input::Char('8') | Input::Char('m') | Input::Char('M') => {
                let map = matches!(i, Input::Char('8' | 'm' | 'M'));
                if let Some(p) = sel {
                    match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|d| crate::wpk::read_defs(&d)) {
                        Ok(defs) => {
                            return Outcome::Push(if map { KbMap::open(&p, defs) } else { KbEdit::open(&p, defs) });
                        }
                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                    }
                }
            }
            Input::Char('a') | Input::Char('A') => {
                crate::setup::change(wp, |s| {
                    s.keyboard = None;
                    s.alternate_keys = true;
                });
                wp.flash("Alternate keyboard (F1 Help, Esc Cancel, F3 Repeat)");
                return Outcome::Close;
            }
            Input::Char('n') | Input::Char('N') => {
                // name search: the list selects the first matching name
                return Outcome::Push(Prompt::new("Name search:", "", |_, s| {
                    NAME_SEARCH.with(|n| *n.borrow_mut() = s);
                    None
                }));
            }
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Char('0') => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        self.refresh();
        // a pending "Name search:" prompt selects as you type
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, "Setup: Keyboard Layout", wp.theme.bold);
        let files = self.files.borrow();
        let max = g.rows.saturating_sub(5);
        let start = self.sel.get().saturating_sub(max.saturating_sub(1));
        for (k, f) in files.iter().enumerate().skip(start).take(max) {
            let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mark = if wp.settings.keyboard.as_ref() == Some(f) { '*' } else { ' ' };
            let row = 2 + k - start;
            let s = if k == self.sel.get() { wp.theme.block } else { st };
            g.text(1, row, &format!("{mark} {name:<14} {}", f.parent().map(|p| p.display().to_string()).unwrap_or_default()), s);
            wp.hit(row, 0, g.cols, HitAction::Select(k));
        }
        if files.is_empty() {
            g.text(2, 2, "No .WPK keyboard files yet -- 4 Create makes one.", st);
        }
        let r = g.rows - 1;
        let hi = wp.mnemonic_style();
        g.text_mn(0, r - 1, "1 ~Select; 2 ~Delete; 3 ~Rename; 4 ~Create; 5 Cop~y; 6 ~Original;", st, hi);
        let c = g.text_mn(0, r, "7 ~Edit; 8 ~Map; ~A Alternate (built in); ~N Name search: ", st, hi);
        g.text(c, r, "1", st);
        g.cursor = Some((c, r));
    }
}

// ============================================================ shared state

struct KbState {
    path: PathBuf,
    defs: Vec<KeyDef>,
    sel: usize,
    modified: bool,
}

type St = Rc<RefCell<KbState>>;

fn save(wp: &mut Wp, st: &St) {
    let s = st.borrow();
    if !s.modified {
        return;
    }
    match std::fs::write(&s.path, crate::wpk::encode(&s.defs)) {
        Ok(()) => {
            if wp.settings.keyboard.as_ref() == Some(&s.path) {
                // the active keyboard changed: load it again
                let set = wp.settings.clone();
                set.apply(wp);
            }
        }
        Err(e) => wp.flash(&format!("ERROR: Can't write {} -- {e}", s.path.display())),
    }
}

fn name_of(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// The action column: a character, a key name, or the start of a macro.
fn action_text(d: &KeyDef) -> String {
    match d.words.as_slice() {
        [] => "(nothing)".into(),
        [w] => word_name(*w),
        _ => words_text(&d.words).replace(['\n', '\t'], " "),
    }
}

/// Edit the action of definition `idx` in the macro editor.
fn edit_action(st: &St, idx: usize) -> Box<dyn Screen> {
    let (label, words, desc) = {
        let s = st.borrow();
        let d = &s.defs[idx];
        (key_label(&d.key), d.words.clone(), d.desc.clone())
    };
    let st2 = st.clone();
    MacroEdit::new("Key: Action", "Key", &label, &desc, words, move |_wp, desc, words| {
        let mut s = st2.borrow_mut();
        if let Some(d) = s.defs.get_mut(idx) {
            d.words = words;
            d.desc = desc;
        }
        s.modified = true;
    })
}

/// Add (or find) the definition for `key`; returns its index.
fn def_for(st: &St, key: KeyIn, original: bool) -> usize {
    let mut s = st.borrow_mut();
    if let Some(k) = s.defs.iter().position(|d| d.key == key) {
        return k;
    }
    let words = if original { crate::wpk::default_word(&key).filter(|&w| w != 0xFFFF && w != 0).map(|w| vec![w]).unwrap_or_default() } else { vec![] };
    s.defs.push(KeyDef { key, words, desc: String::new() });
    s.modified = true;
    s.defs.len() - 1
}

/// The menu choices shared by Edit and Map.  Returns None if `i` is not one.
fn common_choice(st: &St, wp: &mut Wp, i: &Input, edit_screen: bool) -> Option<Outcome> {
    let idx = st.borrow().sel;
    let has = idx < st.borrow().defs.len();
    let c = match i {
        Input::Char(c) => c.to_ascii_lowercase(),
        _ => return None,
    };
    let out = match (c, edit_screen) {
        // Description
        ('2', true) | ('d', true) | ('3', false) | ('d', false) if has => {
            let old = st.borrow().defs[idx].desc.clone();
            let st = st.clone();
            Outcome::Push(Prompt::new("Description:", &old, move |_wp, s| {
                let s = s?;
                let mut b = st.borrow_mut();
                b.defs[idx].desc = s.chars().take(39).collect();
                b.modified = true;
                None
            }))
        }
        // Original
        ('3', true) | ('o', true) | ('4', false) | ('o', false) if has => {
            let mut b = st.borrow_mut();
            b.defs.remove(idx);
            b.sel = b.sel.min(b.defs.len().saturating_sub(1));
            b.modified = true;
            Outcome::Keep
        }
        _ => return None,
    };
    let _ = wp;
    Some(out)
}

// ============================================================ Keyboard: Edit

pub struct KbEdit {
    st: St,
}

impl KbEdit {
    pub fn open(path: &Path, defs: Vec<KeyDef>) -> Box<dyn Screen> {
        Box::new(KbEdit { st: Rc::new(RefCell::new(KbState { path: path.to_path_buf(), defs, sel: 0, modified: false })) })
    }
}

impl Screen for KbEdit {
    fn full_screen(&self) -> bool {
        true
    }
    fn kind(&self) -> ScreenKind {
        ScreenKind::Menu(248)
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.st.borrow_mut().sel = n;
        if double { self.input(wp, &Input::Char('1')) } else { Outcome::Keep }
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let st = self.st.clone();
        let n = st.borrow().defs.len();
        let idx = st.borrow().sel;
        match i {
            Input::Func(Func::Down) => st.borrow_mut().sel = (idx + 1).min(n.saturating_sub(1)),
            Input::Func(Func::Up) => st.borrow_mut().sel = idx.saturating_sub(1),
            Input::Func(Func::PgDn) => st.borrow_mut().sel = (idx + 10).min(n.saturating_sub(1)),
            Input::Func(Func::PgUp) => st.borrow_mut().sel = idx.saturating_sub(10),
            Input::Func(Func::Exit) => {
                save(wp, &st);
                return Outcome::Close;
            }
            Input::Func(Func::Cancel) | Input::Char('0') => {
                if st.borrow().modified {
                    return Outcome::Push(YesNo::new("Save changes to the keyboard?", true, move |wp, yes| {
                        if yes {
                            save(wp, &st);
                        }
                        // close the edit screen too
                        wp.screens.pop();
                        None
                    }));
                }
                return Outcome::Close;
            }
            Input::Char('1') | Input::Char('a') | Input::Char('A') | Input::Func(Func::Enter) if idx < n => {
                return Outcome::Push(edit_action(&st, idx));
            }
            Input::Char('4') | Input::Char('c') | Input::Char('C') => {
                return Outcome::Push(key_prompt("Key:", move |_wp, key| {
                    let k = def_for(&st, key, false);
                    st.borrow_mut().sel = k;
                    Some(edit_action(&st, k))
                }));
            }
            Input::Char('5') | Input::Char('m') | Input::Char('M') if idx < n => {
                return Outcome::Push(key_prompt("Key:", move |wp, key| {
                    let mut b = st.borrow_mut();
                    if b.defs.iter().any(|d| d.key == key) {
                        wp.flash("ERROR: That key is already defined -- choose Original first");
                        return None;
                    }
                    b.defs[idx].key = key;
                    b.modified = true;
                    None
                }));
            }
            Input::Char('6') | Input::Char('s') | Input::Char('S') if idx < n => {
                return Outcome::Push(
                    Prompt::new("Define macro:", "", move |wp, s| {
                        let name = s?;
                        if name.trim().is_empty() {
                            return None;
                        }
                        let path = crate::macros::macro_path(wp, &name);
                        let b = st.borrow();
                        let d = &b.defs[idx];
                        if let Err(e) = std::fs::write(&path, crate::wpm::encode_wpm(&d.desc, &d.words)) {
                            wp.flash(&format!("ERROR: {e}"));
                        }
                        None
                    })
                    .with_alt_letters(),
                );
            }
            Input::Char('7') | Input::Char('r') | Input::Char('R') => {
                return Outcome::Push(key_prompt("Key:", move |_wp, key| {
                    Some(Prompt::new("Macro:", "", move |wp, s| {
                        let name = s?;
                        let Some(p) = crate::macros::find_macro(wp, &name) else {
                            wp.flash(&format!("ERROR: File not found -- {}", crate::macros::macro_file_name(&name)));
                            return None;
                        };
                        match crate::wpm::load_wpm(&p) {
                            Ok(f) => {
                                let k = def_for(&st, key, false);
                                let mut b = st.borrow_mut();
                                b.defs[k].words = f.words.into_iter().filter(|&w| w != 0x000D).collect();
                                b.defs[k].desc = f.description;
                                b.sel = k;
                                b.modified = true;
                            }
                            Err(e) => wp.flash(&format!("ERROR: {e}")),
                        }
                        None
                    }) as Box<dyn Screen>)
                }));
            }
            _ => {
                if let Some(o) = common_choice(&st, wp, i, true) {
                    return o;
                }
            }
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let s = self.st.borrow();
        g.clear(st);
        g.text(0, 0, "Keyboard: Edit", wp.theme.bold);
        g.text(2, 2, "Name:", st);
        g.text(8, 2, &name_of(&s.path), st);
        let und = st.flags(F_UNDER);
        g.text(2, 4, "Key", und);
        g.text(17, 4, "Action", und);
        g.text(35, 4, "Description", und);
        let max = g.rows.saturating_sub(9);
        let start = s.sel.saturating_sub(max.saturating_sub(1));
        for (k, d) in s.defs.iter().enumerate().skip(start).take(max) {
            let row = 5 + k - start;
            let sty = if k == s.sel { wp.theme.block } else { st };
            if k == s.sel {
                g.fill_row(row, 2, g.cols - 2, ' ', sty);
            }
            g.text(2, row, &key_label(&d.key), sty);
            let a: String = action_text(d).chars().take(16).collect();
            g.text(17, row, &a, sty);
            let dsc: String = d.desc.chars().take(g.cols.saturating_sub(37)).collect();
            g.text(35, row, &dsc, sty);
            wp.hit(row, 0, g.cols, HitAction::Select(k));
        }
        let r = g.rows - 1;
        let hi = wp.mnemonic_style();
        let c = g.text_mn(0, r, "1 ~Action; 2 ~Dscrptn; 3 ~Original; 4 ~Create; 5 ~Move; Macro: 6 ~Save; 7 ~Retrieve: ", st, hi);
        g.text(c, r, "0", st);
        g.cursor = Some((c, r));
    }
}

// ============================================================= Keyboard: Map

/// The rows of the map: (screen row, modifiers, key characters).
const MAP_ROWS: [(usize, bool, bool, &str); 5] = [
    (4, false, true, "ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890-=\\`[];',./"),
    (7, true, false, "ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]_"),
    (10, false, false, " !\"#$%&'()*+,-./0123456789:;<=>?@"),
    (13, false, false, "ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`"),
    (16, false, false, "abcdefghijklmnopqrstuvwxyz{|}~"),
];

fn map_key(row: usize, col: usize) -> Option<KeyIn> {
    let (_, ctrl, alt, keys) = MAP_ROWS.get(row)?;
    let c = keys.chars().nth(col)?;
    let c = if *ctrl || *alt { c.to_ascii_lowercase() } else { c };
    Some(KeyIn { k: K::Char(c), m: Mods { shift: false, ctrl: *ctrl, alt: *alt } })
}

/// The one-character action shown under a key in the map.
fn map_char(words: &[u16]) -> char {
    match words {
        [] => ' ',
        [w @ (0x20..=0x7E | 0x100..=0xCFF)] => crate::wpm::word_char(*w).unwrap_or('?'),
        [0xFE00..=0xFEFF] => ' ',
        [_] => 'C',
        _ => 'M',
    }
}

pub struct KbMap {
    st: St,
    row: usize,
    col: usize,
}

impl KbMap {
    pub fn open(path: &Path, defs: Vec<KeyDef>) -> Box<dyn Screen> {
        Box::new(KbMap { st: Rc::new(RefCell::new(KbState { path: path.to_path_buf(), defs, sel: usize::MAX, modified: false })), row: 0, col: 0 })
    }

    fn current(&self) -> Option<KeyIn> {
        map_key(self.row, self.col)
    }

    /// Point the shared selection at the current key's definition.
    fn sync_sel(&self) {
        let key = self.current();
        let mut s = self.st.borrow_mut();
        s.sel = s.defs.iter().position(|d| Some(d.key) == key).unwrap_or(usize::MAX);
    }
}

impl Screen for KbMap {
    fn full_screen(&self) -> bool {
        true
    }
    fn kind(&self) -> ScreenKind {
        ScreenKind::Menu(304)
    }
    fn mouse_select(&mut self, _wp: &mut Wp, n: usize, _double: bool) -> Outcome {
        self.row = n / 100;
        self.col = n % 100;
        self.sync_sel();
        Outcome::Keep
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let st = self.st.clone();
        let len = |r: usize| MAP_ROWS[r].3.chars().count();
        match i {
            Input::Func(Func::Right) => self.col = (self.col + 1).min(len(self.row) - 1),
            Input::Func(Func::Left) => self.col = self.col.saturating_sub(1),
            Input::Func(Func::Down) => {
                self.row = (self.row + 1).min(MAP_ROWS.len() - 1);
                self.col = self.col.min(len(self.row) - 1);
            }
            Input::Func(Func::Up) => {
                self.row = self.row.saturating_sub(1);
                self.col = self.col.min(len(self.row) - 1);
            }
            Input::Func(Func::Home) => self.col = 0,
            Input::Func(Func::End) => self.col = len(self.row) - 1,
            Input::Func(Func::Exit) => {
                save(wp, &st);
                return Outcome::Close;
            }
            Input::Func(Func::Cancel) | Input::Char('0') => return Outcome::Close,
            // 1 Key: the next key pressed gives its original action
            Input::Char('1') | Input::Char('k') | Input::Char('K') => {
                let Some(target) = self.current() else { return Outcome::Keep };
                return Outcome::Push(key_prompt("Key:", move |_wp, key| {
                    let k = def_for(&st, target, true);
                    let w = crate::wpk::default_word(&key).filter(|&w| w != 0xFFFF).map(|w| vec![w]).unwrap_or_default();
                    let mut b = st.borrow_mut();
                    b.defs[k].words = w;
                    b.modified = true;
                    None
                }));
            }
            // 2 Macro: edit the key's action as a macro
            Input::Char('2') | Input::Char('m') | Input::Char('M') => {
                let Some(target) = self.current() else { return Outcome::Keep };
                let k = def_for(&st, target, true);
                return Outcome::Push(edit_action(&st, k));
            }
            // 5 Compose: type the character the key should produce
            Input::Char('5') | Input::Char('c') | Input::Char('C') => {
                let Some(target) = self.current() else { return Outcome::Keep };
                return Outcome::Push(Prompt::new("Character:", "", move |_wp, s| {
                    let c = s?.chars().next()?;
                    let w = crate::wpm::char_word(c)?;
                    let k = def_for(&st, target, true);
                    let mut b = st.borrow_mut();
                    b.defs[k].words = vec![w];
                    b.modified = true;
                    None
                }));
            }
            _ => {
                self.sync_sel();
                if let Some(o) = common_choice(&st, wp, i, false) {
                    self.sync_sel();
                    return o;
                }
            }
        }
        self.sync_sel();
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        let s = self.st.borrow();
        g.clear(st);
        g.text(0, 0, "Keyboard: Map", hi);
        g.text(2, 2, "Name:", st);
        g.text(8, 2, &name_of(&s.path), st);
        g.text(60, 7, "C = Command", st);
        g.text(60, 8, "M = Keyboard Macro", st);
        for (r, (row, ctrl, alt, keys)) in MAP_ROWS.iter().enumerate() {
            let label = if *alt { "  Alt  Key" } else if *ctrl { "  Ctrl Key" } else { "       Key" };
            g.text(0, *row, label, st);
            g.text(7, row + 1, "Action", st);
            for (c, ch) in keys.chars().enumerate() {
                let x = 17 + c;
                let key = map_key(r, c).unwrap();
                let def = s.defs.iter().find(|d| d.key == key);
                let words = match def {
                    Some(d) => d.words.clone(),
                    None => crate::wpk::default_word(&key).filter(|&w| w != 0xFFFF).map(|w| vec![w]).unwrap_or_default(),
                };
                let sel = r == self.row && c == self.col;
                let sty = if sel { wp.theme.block } else { st };
                g.put(x, *row, ch, sty);
                let a = map_char(&words);
                g.put(x, row + 1, a, if def.is_some() { hi } else { st });
                wp.hit(*row, x, x + 1, HitAction::Select(r * 100 + c));
                wp.hit(row + 1, x, x + 1, HitAction::Select(r * 100 + c));
            }
        }
        let und = st.flags(F_UNDER);
        g.text(2, 20, "Key", und);
        g.text(17, 20, "Action", und);
        g.text(35, 20, "Description", und);
        if let Some(key) = self.current() {
            g.text(2, 21, &key_label(&key), st);
            if let Some(d) = s.defs.iter().find(|d| d.key == key) {
                let a: String = action_text(d).chars().take(16).collect();
                g.text(17, 21, &a, st);
                g.text(35, 21, &d.desc, st);
            } else if let Some(w) = crate::wpk::default_word(&key).filter(|&w| w != 0xFFFF && w != 0) {
                g.text(17, 21, &word_name(w), st);
                g.text(35, 21, "(original)", st);
            }
        }
        let r = g.rows - 1;
        let mh = wp.mnemonic_style();
        let c = g.text_mn(0, r, "1 ~Key; 2 ~Macro; 3 ~Description; 4 ~Original; 5 ~Compose: ", st, mh);
        g.text(c, r, "0", st);
        g.cursor = Some((c, r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(wp: &mut Wp, k: K, m: Mods) {
        crate::dispatch_key(wp, KeyIn { k, m });
    }

    #[test]
    fn create_assign_save_and_use() {
        let dir = std::env::temp_dir().join(format!("kbedit_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // never touch the real settings
        let _ = crate::settings::CONFIG_OVERRIDE.set(std::env::temp_dir().join("bsw51_test_config"));
        let mut wp = Wp::new(80, 25);
        wp.macro_dirs = vec![dir.clone()];
        crate::setup::keyboard_layout(&mut wp);
        let none = Mods::default();
        let ctrl = Mods { ctrl: true, ..none };
        press(&mut wp, K::Char('4'), none); // Create
        for c in "mine".chars() {
            press(&mut wp, K::Char(c), none);
        }
        press(&mut wp, K::Enter, none);
        // Keyboard: Edit → 4 Create, Key: Ctrl-K → action editor
        press(&mut wp, K::Char('4'), none);
        press(&mut wp, K::Char('k'), ctrl);
        // type "hi", then Exit (saves the action), then Exit (saves the .WPK)
        press(&mut wp, K::Char('h'), none);
        press(&mut wp, K::Char('i'), none);
        press(&mut wp, K::F(7), none);
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let s: String = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>() + "\n").collect();
        assert!(s.contains("Keyboard: Edit") && s.contains("Ctrl-K") && s.contains("hi"), "{s}");
        press(&mut wp, K::F(7), none);
        let p = dir.join("MINE.WPK");
        let defs = crate::wpk::read_defs(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].words, vec![b'h' as u16, b'i' as u16]);
        // select it and use the key
        press(&mut wp, K::Char('1'), none);
        assert_eq!(wp.settings.keyboard.as_deref(), Some(p.as_path()));
        assert!(!wp.has_screens());
        press(&mut wp, K::Char('k'), ctrl);
        crate::pump(&mut wp);
        assert_eq!(wp.ed_ref().doc.plain_text(0, wp.ed_ref().doc.len()), "hi");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn map_screen_draws() {
        let mut wp = Wp::new(80, 25);
        wp.push(KbMap::open(Path::new("X.WPK"), vec![KeyDef { key: KeyIn { k: K::Char('q'), m: Mods { alt: true, ..Default::default() } }, words: vec![b'@' as u16], desc: "at".into() }]));
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let row5: String = (0..80).map(|c| g.get(c, 5).unwrap().ch).collect();
        assert_eq!(row5.chars().nth(17 + 16), Some('@'), "{row5}");
    }
}
