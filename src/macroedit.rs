//! The WP 5.1 macro editor (Home,Ctrl-F10 or Ctrl-F10 → 2 Edit): the macro
//! is shown as `{DISPLAY OFF}{Home}Text{Enter}` in a box; the cursor moves
//! over whole tokens.  Function keys insert their codes, Enter / Tab are
//! layout only (as in WP), Ctrl-V inserts the next key as a code (so Ctrl-V
//! Enter gives `{Enter}`), Ctrl-F10 records every key until pressed again,
//! Ctrl-PgUp lists the programming commands, Exit saves.
//!
//! The same editor edits a key's action in Setup → Keyboard Layout → Edit.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::ui::{Outcome, Screen, ScreenKind, Wp};
use crate::wpm::{char_word, func_code, word_char};

/// WP's names of the keystroke codes 0x8000–0x8074.
const KEY_NAMES: [&str; 117] = [
    "{Compose}", "{^A}", "{^B}", "{^C}", "{^D}", "{^E}", "{^F}", "{^G}", "{Home}", "{Tab}", "{Enter}",
    "{Del to EOL}", "{Del to EOP}", "{^M}", "{^N}", "{^O}", "{^P}", "{^Q}", "{^R}", "{^S}", "{^T}", "{^U}",
    "{^V}", "{Up}", "{Right}", "{Left}", "{Down}", "{Esc}", "{^\\}", "{^]}", "{Keyboard}", "{^_}",
    "{Cancel}", "{Search}", "{Help}", "{Indent}", "{List}", "{Bold}", "{Exit}", "{Underline}", "{End Field}",
    "{Save}", "{F11}", "{F12}", "{Setup}", "{Search Left}", "{Switch}", "{L/R Indent}", "{Date/Outline}",
    "{Center}", "{Print}", "{Format}", "{Merge Codes}", "{Retrieve}", "{Shft F11}", "{Shft F12}",
    "{Thesaurus}", "{Replace}", "{Reveal Codes}", "{Block}", "{Mark Text}", "{Flush Right}",
    "{Columns/Tables}", "{Style}", "{Graphics}", "{Macro}", "{Alt F11}", "{ALT F12}", "{Shell}", "{Spell}",
    "{Screen}", "{Move}", "{Text In/Out}", "{Tab Align}", "{Footnote}", "{Font}", "{Merge/Sort}",
    "{Macro Define}", "{Ctrl F11}", "{Ctrl F12}", "{Backspace}", "{Del}", "{Del Word}", "{Word Right}",
    "{Word Left}", "{End}", "{Home-Home-Left}", "{Invalid}", "{Goto}", "{Page Up}", "{Page Down}",
    "{Screen Down}", "{Screen Up}", "{Typeover}", "{Left Mar Rel}", "{HPg}", "{SHy}", "{-}", "{ }",
    "{Para Up}", "{Para Down}", "{Item Left}", "{Item Right}", "{Item Up}", "{Item Down}", "{Alt Home}",
    "{Del Word (Row)}", "{Menu Bar}", "{Block Append}", "{Block Move}", "{Block Copy}", "{INVALID}",
    "{MOUSE MOTION}", "{MOUSE L BUTTON}", "{MOUSE M BUTTON}", "{MOUSE R BUTTON}", "{INVALID}",
];

/// The programming commands: code, name, syntax.
pub const COMMANDS: &[(u16, &str, &str)] = &[
    (0xFC00, "{INVALID}", "(not displayable)"),
    (0xFC01, "{ASSIGN}", "{ASSIGN}var~expr~"),
    (0xFC02, "{BELL}", "{BELL}"),
    (0xFC03, "{BREAK}", "{BREAK}"),
    (0xFC04, "{CALL}", "{CALL}label~"),
    (0xFC05, "{CANCEL OFF}", "{CANCEL OFF}"),
    (0xFC06, "{CANCEL ON}", "{CANCEL ON}"),
    (0xFC07, "{CASE}", "{CASE}expr~cs1~lb1~...~"),
    (0xFC08, "{CASE CALL}", "{CASE CALL}expr~cs1~lb1~...~"),
    (0xFC09, "{CHAIN}", "{CHAIN}macroname~"),
    (0xFC0A, "{CHAR}", "{CHAR}var~message~"),
    (0xFC0B, "{;}", "{;}comment~"),
    (0xFC0C, "{DISPLAY OFF}", "{DISPLAY OFF}"),
    (0xFC0D, "{DISPLAY ON}", "{DISPLAY ON}"),
    (0xFC0E, "{ELSE}", "{ELSE}"),
    (0xFC0F, "{END FOR}", "{END FOR}"),
    (0xFC10, "{END IF}", "{END IF}"),
    (0xFC11, "{END WHILE}", "{END WHILE}"),
    (0xFC12, "{FOR}", "{FOR}var~start~stop~step~"),
    (0xFC13, "{FOR EACH}", "{FOR EACH}var~expr1~...exprN~~"),
    (0xFC14, "{GO}", "{GO}label~"),
    (0xFC15, "{IF}", "{IF}expr~"),
    (0xFC16, "{LABEL}", "{LABEL}label~"),
    (0xFC17, "{LOOK}", "{LOOK}var~"),
    (0xFC18, "{NEST}", "{NEST}macroname~"),
    (0xFC19, "{NEXT}", "{NEXT}"),
    (0xFC1A, "{SHELL MACRO}", "{SHELL MACRO}macroname~"),
    (0xFC1B, "{ON CANCEL}", "{ON CANCEL}action~"),
    (0xFC1C, "{ON ERROR}", "{ON ERROR}action~"),
    (0xFC1D, "{ON NOT FOUND}", "{ON NOT FOUND}action~"),
    (0xFC1E, "{PAUSE}", "{PAUSE}"),
    (0xFC1F, "{PROMPT}", "{PROMPT}message~"),
    (0xFC20, "{QUIT}", "{QUIT}"),
    (0xFC21, "{RESTART}", "{RESTART}"),
    (0xFC22, "{RETURN}", "{RETURN}"),
    (0xFC23, "{RETURN CANCEL}", "{RETURN CANCEL}"),
    (0xFC24, "{RETURN ERROR}", "{RETURN ERROR}"),
    (0xFC25, "{RETURN NOT FOUND}", "{RETURN NOT FOUND}"),
    (0xFC26, "{SPEED}", "{SPEED}100ths second~"),
    (0xFC27, "{STEP ON}", "{STEP ON}"),
    (0xFC28, "{TEXT}", "{TEXT}var~message~"),
    (0xFC29, "{STATE}", "{STATE}"),
    (0xFC2A, "{WAIT}", "{WAIT}10ths second~"),
    (0xFC2B, "{WHILE}", "{WHILE}expr~"),
    (0xFC2C, "{Macro Commands}", "{Macro Commands}"),
    (0xFC2D, "{STEP OFF}", "{STEP OFF}"),
    (0xFC2E, "{ORIGINAL KEY}", "{ORIGINAL KEY}"),
    (0xFC2F, "{IF EXISTS}", "{IF EXISTS}var~"),
    (0xFC30, "{MENU OFF}", "{MENU OFF}"),
    (0xFC31, "{MENU ON}", "{MENU ON}"),
    (0xFC32, "{STATUS PROMPT}", "{STATUS PROMPT}message~"),
    (0xFC33, "{INPUT}", "{INPUT}message~"),
    (0xFC34, "{VARIABLE}", "{VARIABLE}var~"),
    (0xFC35, "{SYSTEM}", "{SYSTEM}sysvar~"),
    (0xFC36, "{MID}", "{MID}var~offset~count~"),
    (0xFC37, "{NTOK}", "{NTOK}number~"),
    (0xFC38, "{KTON}", "{KTON}key~"),
    (0xFC39, "{LEN}", "{LEN}var~"),
    (0xFC3A, "{~}", "{~}"),
    (0xFC3B, "{PAUSE KEY}", "{PAUSE KEY}key~"),
    (0xFC3C, "{UNKNOWN}", "(not displayable)"),
    (0xFC3D, "{OTHERWISE}", "{OTHERWISE}"),
    (0xFC3E, "{SHELL ASSIGN}", "{SHELL ASSIGN}shellvar~expr~"),
    (0xFC3F, "{SHELL VARIABLE}", "{SHELL VARIABLE}shellvar~"),
];

/// How a macro word is shown in the editor.
pub fn word_name(w: u16) -> String {
    match w {
        0x0020..=0x007E => (w as u8 as char).to_string(),
        0x0100..=0x0CFF => match word_char(w) {
            Some(c) => c.to_string(),
            None => format!("{{WP {},{}}}", w >> 8, w & 0xFF),
        },
        0x8000..=0x8074 => KEY_NAMES[(w - 0x8000) as usize].to_string(),
        0xFC00..=0xFC3F => COMMANDS[(w - 0xFC00) as usize].1.to_string(),
        0xFD00..=0xFDFF => format!("{{KEY MACRO {}}}", w & 0xFF),
        0xFE00..=0xFEFF => format!("{{ALT {}}}", ((w & 0xFF) as u8 as char).to_ascii_uppercase()),
        0xFF00..=0xFF09 => format!("{{VAR {}}}", w & 0xF),
        _ => format!("{{{w:04X}}}"),
    }
}

/// The whole macro as one line of text (for tests and the key list).
pub fn words_text(words: &[u16]) -> String {
    words
        .iter()
        .map(|&w| match w {
            0x000A => "\n".to_string(),
            0x0009 => "\t".to_string(),
            0x000D => String::new(),
            _ => word_name(w),
        })
        .collect()
}

/// `{IF EXISTS}` → `IF EXISTS` (for sorting and name search).
fn bare(name: &str) -> String {
    name.trim_matches(|c| c == '{' || c == '}').to_ascii_uppercase()
}

/// An input → the word the editor inserts for it.
fn input_code(i: &Input) -> Option<u16> {
    match i {
        Input::Char(c) => char_word(*c),
        Input::Func(Func::AltLetter(c)) => Some(0xFE00 | c.to_ascii_uppercase() as u16),
        Input::Func(Func::CloseWindow) => None,
        Input::Func(f) => Some(func_code(*f)),
    }
}

#[derive(Clone, PartialEq)]
enum Mode {
    Select,
    Action,
    Desc { buf: Vec<char>, old: String },
}

type SaveFn = Box<dyn FnOnce(&mut Wp, String, Vec<u16>)>;

pub struct MacroEdit {
    title: String,
    /// "File" (macro) or "Key" (keyboard).
    file_label: String,
    file: String,
    desc: String,
    words: Vec<u16>,
    cur: usize,
    mode: Mode,
    literal_next: bool,
    literal_mode: bool,
    home: u8,
    want_col: Option<usize>,
    top: usize,
    modified: bool,
    /// Ctrl-PgUp command list: selected entry, name-search text.
    list: Option<(usize, String)>,
    on_save: Option<SaveFn>,
}

impl MacroEdit {
    pub fn new(title: &str, file_label: &str, file: &str, desc: &str, words: Vec<u16>, on_save: impl FnOnce(&mut Wp, String, Vec<u16>) + 'static) -> Box<MacroEdit> {
        let words: Vec<u16> = words.into_iter().filter(|&w| w != 0x000D).collect();
        Box::new(MacroEdit {
            title: title.into(),
            file_label: file_label.into(),
            file: file.into(),
            desc: desc.into(),
            words,
            cur: 0,
            mode: Mode::Action,
            literal_next: false,
            literal_mode: false,
            home: 0,
            want_col: None,
            top: 0,
            modified: false,
            list: None,
            on_save: Some(Box::new(on_save)),
        })
    }

    /// Open the macro file `path` (new if missing) in the editor.
    pub fn open_file(wp: &mut Wp, path: std::path::PathBuf, desc: Option<String>) {
        let (old_desc, words) = match crate::wpm::load_wpm(&path) {
            Ok(f) => (f.description, f.words),
            Err(_) => (String::new(), vec![0xFC0C]), // WP starts new macros with {DISPLAY OFF}
        };
        let name = path.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
        let desc = desc.unwrap_or(old_desc);
        let changed_desc = !path.exists();
        let mut ed = MacroEdit::new("Macro: Edit", "File", &name, &desc, words, move |wp, desc, words| {
            match std::fs::write(&path, crate::wpm::encode_wpm(&desc, &words)) {
                Ok(()) => wp.flash(&format!("Macro saved: {}", path.display())),
                Err(e) => wp.flash(&format!("ERROR: Can't write {} -- {e}", path.display())),
            }
        });
        ed.modified = changed_desc;
        ed.cur = ed.words.len();
        wp.push(ed);
    }

    fn box_geom(rows: usize, cols: usize) -> (usize, usize, usize, usize) {
        // inner area: x0, y0, width, height
        let y0 = 8;
        let h = rows.saturating_sub(y0 + 3).max(1);
        let w = cols.saturating_sub(12).max(10);
        (6, y0, w, h)
    }

    /// Token positions: (line, col) for every cursor slot 0..=len, plus
    /// the drawn tokens.
    fn layout(&self, width: usize) -> (Vec<(usize, usize)>, Vec<(usize, usize, String)>) {
        let mut pos = Vec::with_capacity(self.words.len() + 1);
        let mut items = Vec::new();
        let (mut line, mut col) = (0usize, 0usize);
        for &w in &self.words {
            match w {
                0x000A => {
                    pos.push((line, col));
                    line += 1;
                    col = 0;
                }
                0x0009 => {
                    pos.push((line, col));
                    col += 4 - col % 4;
                    if col >= width {
                        line += 1;
                        col = 0;
                    }
                }
                _ => {
                    let t = word_name(w);
                    let tw = t.chars().count();
                    if col + tw > width && col > 0 {
                        line += 1;
                        col = 0;
                    }
                    pos.push((line, col));
                    items.push((line, col, t));
                    col += tw;
                }
            }
        }
        pos.push((line, col));
        (pos, items)
    }

    fn insert(&mut self, w: u16) {
        self.words.insert(self.cur, w);
        self.cur += 1;
        self.modified = true;
        self.want_col = None;
    }

    fn vertical(&mut self, width: usize, delta: isize, height: usize) {
        let (pos, _) = self.layout(width);
        let (line, col) = pos[self.cur];
        let want = *self.want_col.get_or_insert(col);
        let last = pos.last().unwrap().0;
        let target = (line as isize + delta).clamp(0, last as isize) as usize;
        if target == line {
            self.cur = if delta < 0 { 0 } else { self.words.len() };
            return;
        }
        let mut best = None;
        for (i, &(l, c)) in pos.iter().enumerate() {
            if l == target && (c <= want || best.is_none()) {
                best = Some(i);
            }
        }
        if let Some(b) = best {
            self.cur = b;
        }
        let _ = height;
    }

    fn line_edge(&mut self, width: usize, end: bool) {
        let (pos, _) = self.layout(width);
        let line = pos[self.cur].0;
        let it = pos.iter().enumerate().filter(|(_, p)| p.0 == line).map(|(i, _)| i);
        self.cur = if end { it.max().unwrap_or(self.cur) } else { it.min().unwrap_or(self.cur) };
    }

    fn scroll(&mut self, width: usize, height: usize) {
        let (pos, _) = self.layout(width);
        let line = pos[self.cur].0;
        if line < self.top {
            self.top = line;
        } else if line >= self.top + height {
            self.top = line + 1 - height;
        }
    }

    fn save(&mut self, wp: &mut Wp) -> Outcome {
        if let Some(f) = self.on_save.take() {
            f(wp, self.desc.clone(), self.words.clone());
        }
        Outcome::Close
    }

    fn list_entries() -> Vec<(u16, &'static str, &'static str)> {
        let mut v: Vec<_> = COMMANDS.iter().filter(|c| !matches!(c.0, 0xFC00 | 0xFC2C | 0xFC3C)).copied().collect();
        v.sort_by_key(|c| bare(c.1));
        v
    }

    fn list_input(&mut self, i: &Input, height: usize) -> Outcome {
        let entries = Self::list_entries();
        let Some((sel, search)) = &mut self.list else { return Outcome::Keep };
        match i {
            Input::Func(Func::Down) => *sel = (*sel + 1).min(entries.len() - 1),
            Input::Func(Func::Up) => *sel = sel.saturating_sub(1),
            Input::Func(Func::PgDn | Func::ScreenDown) => *sel = (*sel + height).min(entries.len() - 1),
            Input::Func(Func::PgUp | Func::ScreenUp) => *sel = sel.saturating_sub(height),
            Input::Func(Func::Home) => *sel = 0,
            Input::Func(Func::End) => *sel = entries.len() - 1,
            Input::Func(Func::Backspace) => {
                search.pop();
            }
            Input::Char(c) => {
                search.push(c.to_ascii_uppercase());
                let s = search.clone();
                if let Some(k) = entries.iter().position(|e| bare(e.1).starts_with(&s)) {
                    *sel = k;
                }
            }
            Input::Func(Func::Enter) => {
                let w = entries[*sel].0;
                self.list = None;
                self.insert(w);
            }
            Input::Func(Func::Cancel | Func::Esc | Func::Exit | Func::MacroCommands) => self.list = None,
            _ => {}
        }
        Outcome::Keep
    }

    fn action_input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let (_, _, width, height) = Self::box_geom(wp.rows, wp.cols);
        if self.list.is_some() {
            return self.list_input(i, height);
        }
        if std::mem::take(&mut self.literal_next) {
            if let Some(w) = input_code(i) {
                self.insert(w);
            }
            return Outcome::Keep;
        }
        if self.literal_mode {
            if *i == Input::Func(Func::MacroDefine) {
                self.literal_mode = false;
                wp.message = None;
            } else if let Some(w) = input_code(i) {
                self.insert(w);
            }
            return Outcome::Keep;
        }
        let home = std::mem::take(&mut self.home);
        let vertical = matches!(i, Input::Func(Func::Up | Func::Down));
        match i {
            Input::Char(c) => {
                if let Some(w) = char_word(*c) {
                    self.insert(w);
                }
            }
            Input::Func(f) => match f {
                Func::Home => self.home = home + 1,
                Func::Left if home > 0 => self.line_edge(width, false),
                Func::Right if home > 0 => self.line_edge(width, true),
                Func::Up if home >= 2 => self.cur = 0,
                Func::Down if home >= 2 => self.cur = self.words.len(),
                Func::Up if home == 1 => self.vertical(width, -(height as isize - 1), height),
                Func::Down if home == 1 => self.vertical(width, height as isize - 1, height),
                Func::Left => self.cur = self.cur.saturating_sub(1),
                Func::Right => self.cur = (self.cur + 1).min(self.words.len()),
                Func::Up => self.vertical(width, -1, height),
                Func::Down => self.vertical(width, 1, height),
                Func::PgUp | Func::ScreenUp => self.vertical(width, -(height as isize), height),
                Func::PgDn | Func::ScreenDown => self.vertical(width, height as isize, height),
                Func::End => self.line_edge(width, true),
                Func::GoTo => self.cur = 0,
                Func::WordLeft => {
                    self.cur = self.cur.saturating_sub(1);
                    while self.cur > 0 && word_char(self.words[self.cur - 1]).is_some_and(|c| c.is_alphanumeric()) {
                        self.cur -= 1;
                    }
                }
                Func::WordRight => {
                    while self.cur < self.words.len() && word_char(self.words[self.cur]).is_some_and(|c| c.is_alphanumeric()) {
                        self.cur += 1;
                    }
                    self.cur = (self.cur + 1).min(self.words.len());
                }
                Func::Backspace => {
                    if self.cur > 0 {
                        self.cur -= 1;
                        self.words.remove(self.cur);
                        self.modified = true;
                    }
                }
                Func::Del | Func::DelWord => {
                    if self.cur < self.words.len() {
                        self.words.remove(self.cur);
                        self.modified = true;
                    }
                }
                Func::DelEol => {
                    let c = self.cur;
                    self.line_edge(width, true);
                    let e = self.cur;
                    self.words.drain(c..e);
                    self.cur = c;
                    self.modified = true;
                }
                Func::Enter => self.insert(0x000A),
                Func::Tab => self.insert(0x0009),
                Func::Exit => return self.save(wp),
                Func::Cancel => {
                    self.mode = Mode::Select;
                    return Outcome::Keep;
                }
                Func::Compose => self.literal_next = true,
                Func::MacroDefine => {
                    self.literal_mode = true;
                    wp.flash("Press Macro Define to enable editing");
                }
                Func::MacroCommands => self.list = Some((0, String::new())),
                Func::CloseWindow => return self.save(wp),
                _ => {
                    if let Some(w) = input_code(i) {
                        self.insert(w);
                    }
                }
            },
        }
        if !vertical {
            self.want_col = None;
        }
        self.scroll(width, height);
        Outcome::Keep
    }
}

impl Screen for MacroEdit {
    fn full_screen(&self) -> bool {
        true
    }
    fn kind(&self) -> ScreenKind {
        ScreenKind::Menu(237)
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match self.mode.clone() {
            Mode::Action => self.action_input(wp, i),
            Mode::Desc { mut buf, old } => {
                match i {
                    Input::Char(c) => {
                        if buf.len() < 39 {
                            buf.push(*c);
                        }
                    }
                    Input::Func(Func::Backspace) => {
                        buf.pop();
                    }
                    Input::Func(Func::Enter | Func::Exit | Func::Tab) => {
                        let d: String = buf.iter().collect();
                        if d != old {
                            self.modified = true;
                        }
                        self.desc = d;
                        self.mode = Mode::Action;
                        return Outcome::Keep;
                    }
                    Input::Func(Func::Cancel | Func::Esc) => {
                        self.desc = old;
                        self.mode = Mode::Select;
                        return Outcome::Keep;
                    }
                    _ => {}
                }
                self.desc = buf.iter().collect();
                self.mode = Mode::Desc { buf, old };
                Outcome::Keep
            }
            Mode::Select => match i {
                Input::Char('1') | Input::Char('d') | Input::Char('D') => {
                    self.mode = Mode::Desc { buf: self.desc.chars().collect(), old: self.desc.clone() };
                    Outcome::Keep
                }
                Input::Char('2') | Input::Char('a') | Input::Char('A') => {
                    self.mode = Mode::Action;
                    Outcome::Keep
                }
                Input::Func(Func::Exit) | Input::Func(Func::CloseWindow) => self.save(wp),
                Input::Func(Func::Cancel | Func::Enter) | Input::Char('0') => {
                    if self.modified {
                        wp.flash("Macro not saved");
                    }
                    Outcome::Close
                }
                _ => Outcome::Keep,
            },
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, &self.title, hi);
        g.text(5, 2, &self.file_label, st);
        g.text(21, 2, &self.file, st);
        g.text(2, 4, "1", hi);
        g.text_mn(3, 4, " - ~Description", st, wp.mnemonic_style());
        g.text(21, 4, &self.desc, st);
        g.text(2, 6, "2", hi);
        g.text_mn(3, 6, " - ~Action", st, wp.mnemonic_style());
        wp.hit(4, 2, 60, crate::ui::HitAction::Input(Input::Char('1')));
        wp.hit(6, 2, 20, crate::ui::HitAction::Input(Input::Char('2')));
        let (x0, y0, width, height) = Self::box_geom(g.rows, g.cols);
        g.frame(x0 - 1, y0 - 1, x0 + width, y0 + height, st, true);
        let (pos, items) = self.layout(width);
        for (line, col, t) in items {
            if line < self.top || line >= self.top + height {
                continue;
            }
            let room = width.saturating_sub(col);
            let t: String = t.chars().take(room).collect();
            g.text(x0 + col, y0 + line - self.top, &t, st);
        }
        let r = g.rows - 1;
        match &self.mode {
            Mode::Action => {
                let (l, c) = pos[self.cur];
                if l >= self.top && l < self.top + height {
                    g.cursor = Some((x0 + c.min(width - 1), y0 + l - self.top));
                }
                if self.literal_mode {
                    g.text(0, r, "Press Macro Define to enable editing", st);
                } else {
                    let c = g.text(0, r, "Ctrl-PgUp", hi);
                    let c = g.text(c, r, " for macro commands;  Press ", st);
                    let c = g.text(c, r, "Exit", hi);
                    g.text(c, r, " when done", st);
                }
            }
            Mode::Desc { buf, .. } => {
                g.cursor = Some((21 + buf.len(), 4));
            }
            Mode::Select => {
                let c = g.text(0, r, "Selection: ", hi);
                g.text(c, r, "0", st);
                g.cursor = Some((c, r));
            }
        }
        if let Some((sel, _)) = &self.list {
            let entries = Self::list_entries();
            let lx = g.cols.saturating_sub(34);
            let (ly, lh) = (y0, height);
            for y in ly - 1..=ly + lh {
                g.fill_row(y, lx - 1, g.cols - 1, ' ', st);
            }
            g.frame(lx - 1, ly - 1, g.cols - 2, ly + lh, st, false);
            let start = if *sel >= lh { sel + 1 - lh } else { 0 };
            for (k, e) in entries.iter().enumerate().skip(start).take(lh) {
                let s = if k == *sel { wp.theme.block } else { st };
                let row = ly + k - start;
                g.fill_row(row, lx, g.cols - 2, ' ', s);
                g.text(lx, row, e.1, s);
            }
            g.fill_row(r, 0, g.cols, ' ', st);
            let c = g.text(0, r, entries[*sel].2, hi);
            g.text(c, r, "   Enter inserts; type to search", st);
            g.cursor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_tokens_and_save() {
        let dir = std::env::temp_dir().join(format!("macroedit_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ALTX.WPM");
        std::fs::write(&p, crate::wpm::encode_wpm("old", &[0xFC0C, 0x8008, b'a' as u16, 0x800A])).unwrap();
        let mut wp = Wp::new(80, 25);
        MacroEdit::open_file(&mut wp, p.clone(), None);
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let s: String = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>() + "\n").collect();
        assert!(s.contains("{DISPLAY OFF}{Home}a{Enter}") && s.contains("ALTX.WPM") && s.contains("old"), "{s}");
        // cursor at the end: Left moves over {Enter} as one token
        wp.input(Input::Func(Func::Left));
        wp.input(Input::Func(Func::Backspace)); // deletes 'a'
        wp.input(Input::Char('b'));
        wp.input(Input::Func(Func::Bold)); // a function key inserts its code
        wp.input(Input::Func(Func::Compose)); // Ctrl-V: next key literally
        wp.input(Input::Func(Func::Enter));
        wp.input(Input::Func(Func::Enter)); // plain Enter = layout line break
        wp.input(Input::Func(Func::MacroCommands));
        for c in "ASS".chars() {
            wp.input(Input::Char(c));
        }
        wp.input(Input::Func(Func::Enter));
        wp.input(Input::Func(Func::Exit));
        assert!(!wp.has_screens());
        let f = crate::wpm::load_wpm(&p).unwrap();
        assert_eq!(f.description, "old");
        assert_eq!(words_text(&f.words), "{DISPLAY OFF}{Home}b{Bold}{Enter}\n{ASSIGN}{Enter}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
