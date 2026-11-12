//! The program state and the main editing screen.

use crate::doc::*;
use crate::editor::{Editor, is_invisible};
use crate::fileio;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::{COL_WPU, LineEnd, PageBreak};
use crate::reveal::code_text;
use crate::screens::*;
use std::path::{Path, PathBuf};

pub enum Outcome {
    Keep,
    Close,
    /// Close every open menu/screen and return to the document (F7 Exit
    /// from inside nested WP menus).
    CloseAll,
    Replace(Box<dyn Screen>),
    Push(Box<dyn Screen>),
}

/// A clickable area registered while drawing.
#[derive(Clone, Debug)]
pub struct Hit {
    pub row: usize,
    pub c0: usize,
    pub c1: usize,
    pub action: HitAction,
}

#[derive(Clone, Debug)]
pub enum HitAction {
    /// Feed this input (e.g. a menu choice '2').
    Input(Input),
    /// Select list entry n (double-click opens it).
    Select(usize),
    /// Put the document cursor at element index n.
    Cursor(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MouseKind {
    Down,
    Drag,
    Up,
    Double,
    Right,
    Wheel(i32),
}

/// What kind of screen is on top (for the macro commands {STATE} and
/// {SYSTEM}menu~).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenKind {
    Other,
    /// A `Yes (No)` question.
    Question,
    /// A list (List Files, keyboard list …).
    List,
    /// A menu, with WP's menu number when known.
    Menu(u16),
    /// A prompt on the status line.
    Prompt,
    /// Editing nested text (footnote, header …).
    SubEdit,
    Help,
    /// The macro prompt layer (not a real screen).
    Overlay,
    /// Waits for a physical key (keyboard layout editing).
    KeyCapture,
}

pub trait Screen {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome;
    fn kind(&self) -> ScreenKind {
        ScreenKind::Other
    }
    /// A list entry was clicked (Hit::Select).  `double` = double-click.
    fn mouse_select(&mut self, _wp: &mut Wp, _n: usize, _double: bool) -> Outcome {
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid);
    /// Full-screen screens hide the document.
    fn full_screen(&self) -> bool {
        false
    }
    /// Graphical screens (View Document) draw pixels over the text grid.
    fn draw_pixels(&self, _wp: &Wp, _buf: &mut [u32], _w: usize, _h: usize, _cell_h: usize) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub normal: Style,
    pub bold: Style,
    pub block: Style,
    pub code: Style,
    pub code_cursor: Style,
    pub page_break: Style,
    pub ruler: Style,
    pub screen_bg: u32,
    /// Per-attribute colours, indexed by `Attr as usize` (Setup: Colors).
    pub attr: [Style; 16],
    pub bold_underline: Style,
    pub other_combo: Style,
}

/// A PC text attribute byte (high nibble background, low nibble
/// foreground) → Style.
pub fn attr_byte(b: u8) -> Style {
    Style::new(pal::ALL[(b & 0x0F) as usize], pal::ALL[((b >> 4) & 0x07) as usize])
}

impl Theme {
    /// WordPerfect 5.1's factory colour set.
    pub fn wp_blue() -> Theme {
        let n = Style::new(pal::LGRAY, pal::BLUE);
        let mut attr = [n; 16];
        attr[Attr::Bold as usize] = Style::new(pal::WHITE, pal::BLUE);
        attr[Attr::Underline as usize] = Style::new(pal::LCYAN, pal::BLUE);
        attr[Attr::DblUnderline as usize] = Style::new(pal::LCYAN, pal::BLUE);
        attr[Attr::Italic as usize] = Style::new(pal::LMAGENTA, pal::BLUE);
        attr[Attr::Redline as usize] = Style::new(pal::LRED, pal::BLUE);
        attr[Attr::Strikeout as usize] = Style::new(pal::LGREEN, pal::BLUE);
        Theme {
            normal: n,
            bold: Style::new(pal::WHITE, pal::BLUE),
            block: Style::new(pal::BLUE, pal::LGRAY),
            code: Style::new(pal::LCYAN, pal::BLUE),
            code_cursor: Style::new(pal::BLUE, pal::CYAN),
            page_break: n,
            ruler: n,
            screen_bg: pal::BLUE.0,
            attr,
            bold_underline: Style::new(pal::WHITE, pal::BLUE),
            other_combo: Style::new(pal::BLACK, pal::LGRAY),
        }
    }

    /// Black & white (monochrome screen).
    pub fn mono() -> Theme {
        let n = Style::new(pal::LGRAY, pal::BLACK);
        let b = Style::new(pal::WHITE, pal::BLACK);
        let r = Style::new(pal::BLACK, pal::LGRAY);
        let mut attr = [n; 16];
        attr[Attr::Bold as usize] = b;
        attr[Attr::Underline as usize] = n;
        Theme {
            normal: n,
            bold: b,
            block: r,
            code: b,
            code_cursor: r,
            page_break: n,
            ruler: n,
            screen_bg: pal::BLACK.0,
            attr,
            bold_underline: b,
            other_combo: r,
        }
    }

    /// Her own colours, decoded from her WP{WP}.SET (table @0x6A5).
    pub fn hers() -> Theme {
        let n = attr_byte(0x1F);
        let mut attr = [n; 16];
        let set = |attr: &mut [Style; 16], a: Attr, b: u8| attr[a as usize] = attr_byte(b);
        set(&mut attr, Attr::Underline, 0x70);
        set(&mut attr, Attr::Strikeout, 0x30);
        set(&mut attr, Attr::Bold, 0x1B);
        set(&mut attr, Attr::DblUnderline, 0x31);
        set(&mut attr, Attr::Redline, 0x74);
        set(&mut attr, Attr::Shadow, 0x71);
        set(&mut attr, Attr::Italic, 0x1E);
        set(&mut attr, Attr::SmallCaps, 0x34);
        set(&mut attr, Attr::Outline, 0x35);
        set(&mut attr, Attr::Subscript, 0x74);
        set(&mut attr, Attr::Superscript, 0x75);
        set(&mut attr, Attr::Fine, 0x50);
        set(&mut attr, Attr::Small, 0x57);
        set(&mut attr, Attr::Large, 0x04);
        set(&mut attr, Attr::VeryLarge, 0x03);
        set(&mut attr, Attr::ExtraLarge, 0x07);
        Theme {
            normal: n,
            bold: attr_byte(0x1B),
            block: attr_byte(0x47),
            code: attr_byte(0x1B),
            code_cursor: attr_byte(0x47),
            page_break: n,
            ruler: n,
            screen_bg: pal::BLUE.0,
            attr,
            bold_underline: attr_byte(0x7F),
            other_combo: attr_byte(0x70),
        }
    }

    /// Her Doc 2 colours (second table in her settings file): white on cyan,
    /// so she always sees which document she is in.
    pub fn hers_doc2() -> Theme {
        let mut t = Theme::hers();
        t.normal = attr_byte(0x3F);
        t.screen_bg = pal::CYAN.0;
        t.page_break = t.normal;
        t.ruler = t.normal;
        t.code = attr_byte(0x31);
        let set = |t: &mut Theme, a: Attr, b: u8| t.attr[a as usize] = attr_byte(b);
        for a in Attr::ALL {
            t.attr[a as usize] = t.normal;
        }
        set(&mut t, Attr::Underline, 0x71);
        set(&mut t, Attr::Strikeout, 0x2E);
        set(&mut t, Attr::Bold, 0x1F);
        set(&mut t, Attr::DblUnderline, 0x71);
        set(&mut t, Attr::Italic, 0x1E);
        set(&mut t, Attr::Redline, 0x34);
        t.bold = attr_byte(0x1F);
        t.bold_underline = attr_byte(0x20);
        t.other_combo = attr_byte(0x6F);
        t
    }

    /// Style for text with the given attributes (WP picks the colour of a
    /// single attribute, a special one for bold+underline, and "other
    /// combinations" otherwise).  Real bold/italic/underline typography is
    /// added on top.
    pub fn for_attrs(&self, a: AttrSet) -> Style {
        // tracked changes: insertions green + underline, deletions red + strikeout
        if a.has(Attr::Redline) && a.has(Attr::Strikeout) {
            return Style::new(pal::LRED, self.normal.bg).flags(F_STRIKE);
        }
        if a.has(Attr::Redline) && a.has(Attr::Underline) {
            return Style::new(pal::LGREEN, self.normal.bg).flags(F_UNDER);
        }
        let n = a.iter().count();
        let mut s = match n {
            0 => self.normal,
            1 => self.attr[a.iter().next().unwrap() as usize],
            2 if a.has(Attr::Bold) && a.has(Attr::Underline) => self.bold_underline,
            _ => self.other_combo,
        };
        if a.has(Attr::Bold) {
            s.flags |= F_BOLD;
        }
        if a.has(Attr::Italic) {
            s.flags |= F_ITALIC;
        }
        if a.has(Attr::Underline) {
            s.flags |= F_UNDER;
        }
        if a.has(Attr::DblUnderline) {
            s.flags |= F_DUNDER;
        }
        if a.has(Attr::Strikeout) {
            s.flags |= F_STRIKE;
        }
        s
    }
}

pub struct Wp {
    pub docs: [Editor; 2],
    pub cur: usize,
    pub(crate) screens: Vec<Box<dyn Screen>>,
    pub message: Option<String>,
    pub quit: bool,
    pub theme: Theme,
    /// Default directory (F5, "=").
    pub cwd: PathBuf,
    pub(crate) home_count: u8,
    /// Esc repeat value (WP default 8).
    pub repeat_value: u32,
    pub(crate) pending_repeat: Option<u32>,
    pub last_search: Vec<El>,
    pub last_replace: Vec<El>,
    pub cols: usize,
    pub rows: usize,
    /// Macro playback queue (front = next input).
    pub queue: std::collections::VecDeque<Input>,
    pub macro_dirs: Vec<PathBuf>,
    /// Macro recording (Ctrl-F10): name + recorded inputs.
    pub recording: Option<(String, String, Vec<Input>)>,
    pub keyboard: Option<crate::wpk::Keyboard>,
    pub alternate_keys: bool,
    pub settings: crate::settings::Settings,
    /// The running macro, if any.
    pub macro_vm: Option<Box<crate::wpm::MacroVm>>,
    pub print_copies: u32,
    pub last_backup: std::time::Instant,
    pub timers: crate::history::Timers,
    /// Clickable areas of the last drawn screen.
    pub hits: std::cell::RefCell<Vec<Hit>>,
    drag_from: Option<usize>,
    /// Ctrl-F3, 1 Window: lines in the top (Doc 1) window when split.
    pub split: Option<usize>,
    /// The physical key behind the input being processed (for screens
    /// that assign keys: Setup → Keyboard Layout → Edit).
    pub last_key: Option<crate::keys::KeyIn>,
}

impl Wp {
    pub fn new(cols: usize, rows: usize) -> Wp {
        Wp {
            docs: [Editor::new(), Editor::new()].map(|mut e| {
                e.doc.id = new_doc_id();
                e
            }),
            cur: 0,
            screens: Vec::new(),
            message: None,
            quit: false,
            theme: Theme::wp_blue(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            home_count: 0,
            repeat_value: 8,
            pending_repeat: None,
            last_search: Vec::new(),
            last_replace: Vec::new(),
            cols,
            rows,
            queue: Default::default(),
            macro_dirs: Vec::new(),
            recording: None,
            keyboard: None,
            alternate_keys: true,
            settings: crate::settings::Settings::default(),
            macro_vm: None,
            print_copies: 1,
            last_backup: std::time::Instant::now(),
            timers: Default::default(),
            hits: Default::default(),
            drag_from: None,
            split: None,
            last_key: None,
        }
    }

    /// Offer recovery copies left by a crash or power failure.
    pub fn offer_recoveries(&mut self) {
        let list = crate::history::pending_recoveries();
        for r in list.into_iter().rev() {
            let name = r.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(unnamed document)".into());
            let (d, t) = crate::filelist::fmt_time(r.when);
            let q = format!("Recover {name} (autosaved {d} {t})?");
            self.push(YesNo::new(&q, true, move |wp, yes| {
                if yes {
                    // into an empty document slot, or Doc 1
                    let slot = (0..2).find(|&k| wp.docs[k].is_empty() && wp.docs[k].path.is_none()).unwrap_or(0);
                    let mut ed = Editor::with_doc(r.doc.clone(), r.path.clone());
                    ed.modified = true;
                    ed.settle();
                    wp.docs[slot] = ed;
                    wp.cur = slot;
                    wp.apply_doc_theme();
                    crate::history::record(&wp.docs[slot].doc, r.path.as_deref(), "recovered");
                } else {
                    crate::history::record(&r.doc, r.path.as_deref(), "discarded");
                }
                let _ = std::fs::remove_file(&r.file);
                wp.after_edit();
                None
            }));
        }
    }

    pub fn printer_name(&self) -> String {
        if !self.settings.printer.is_empty() {
            return self.settings.printer.clone();
        }
        crate::print::winprint::default_printer().unwrap_or_else(|| "(no printer)".into())
    }

    /// Let the topmost graphical screen draw pixels.
    pub fn draw_pixels(&self, buf: &mut [u32], w: usize, h: usize, cell_h: usize) -> bool {
        match self.screens.last() {
            Some(s) => s.draw_pixels(self, buf, w, h, cell_h),
            None => false,
        }
    }

    pub fn date_format(&self) -> String {
        self.settings.date_format.clone()
    }

    pub fn set_date_format(&mut self, f: &str) {
        self.settings.date_format = f.to_string();
        let _ = self.settings.save();
    }

    pub fn german_dates(&self) -> bool {
        self.settings.german_dates
    }

    pub fn ed(&mut self) -> &mut Editor {
        &mut self.docs[self.cur]
    }

    pub fn ed_ref(&self) -> &Editor {
        &self.docs[self.cur]
    }

    pub fn flash(&mut self, msg: &str) {
        self.message = Some(msg.to_string());
    }

    pub fn push(&mut self, s: Box<dyn Screen>) {
        self.screens.push(s);
    }

    pub fn has_screens(&self) -> bool {
        !self.screens.is_empty()
    }

    /// The home directory (Setup → Location of Files → 7 Documents), if set
    /// and present.  F5 always suggests it.
    pub fn home_dir(&self) -> Option<PathBuf> {
        self.settings.default_dir.clone().filter(|d| d.is_dir())
    }

    pub fn resolve_path(&self, s: &str) -> PathBuf {
        let s = s.trim().trim_matches('"');
        let p = Path::new(s);
        if p.is_absolute() || s.starts_with("\\\\") || s.starts_with("//") {
            p.to_path_buf()
        } else {
            self.cwd.join(p)
        }
    }

    /// Process one input (from the keyboard or a macro).
    pub fn input(&mut self, i: Input) {
        self.input_inner(i);
        // keep the view in place after anything (a retrieved file, a closed
        // screen, a font change…), not just after typing
        if self.screens.is_empty() {
            let rows = self.doc_rows();
            let cols = self.cols as i32;
            let e = self.ed();
            e.ensure_layout();
            scroll_into_view(e, rows, cols);
        }
    }

    fn input_inner(&mut self, i: Input) {
        if let Some((_, _, rec)) = &mut self.recording {
            if i != Input::Func(Func::MacroDefine) {
                rec.push(i.clone());
            }
        }
        // Esc repeat count handling
        if let Some(n) = self.pending_repeat.take() {
            match &i {
                Input::Char(c) if c.is_ascii_digit() && self.screens.is_empty() => {
                    let v = n.saturating_mul(10).saturating_add(c.to_digit(10).unwrap());
                    self.pending_repeat = Some(if n == u32::MAX { c.to_digit(10).unwrap() } else { v });
                    self.message = Some(format!("Repeat Value = {}", self.pending_repeat.unwrap()));
                    return;
                }
                Input::Func(Func::Cancel) => {
                    self.message = None;
                    return;
                }
                Input::Func(Func::Enter) => {
                    self.message = None;
                    return;
                }
                _ => {
                    self.message = None;
                    let count = if n == u32::MAX { self.repeat_value } else { n };
                    for _ in 0..count.min(100_000) {
                        self.input_one(i.clone());
                    }
                    return;
                }
            }
        }
        self.input_one(i);
    }

    fn input_one(&mut self, i: Input) {
        if let Some(mut top) = self.screens.pop() {
            // Screens pushed by the handler itself stay on top of it.
            let base = self.screens.len();
            let out = top.input(self, &i);
            let base = base.min(self.screens.len());
            match out {
                Outcome::Keep => self.screens.insert(base, top),
                Outcome::Close => {}
                Outcome::CloseAll => self.screens.clear(),
                Outcome::Replace(n) => self.screens.insert(base, n),
                Outcome::Push(n) => {
                    self.screens.insert(base, top);
                    self.screens.insert(base + 1, n);
                }
            }
            return;
        }
        self.message = None;
        self.edit_input(i);
    }

    // ------------------------------------------------------- edit screen

    pub(crate) fn edit_input(&mut self, i: Input) {
        let Some(i) = crate::aids::before_input(self, i) else {
            self.after_edit();
            return;
        };
        let home = std::mem::replace(&mut self.home_count, 0);
        match i {
            Input::Char(c) => {
                if home > 0 {
                    match c {
                        ' ' => {
                            self.ed().insert_code(Code::HardSpace);
                            return;
                        }
                        '-' => {
                            self.ed().insert_code(Code::HardHyphen);
                            return;
                        }
                        '/' => {
                            // [/] cancel hyphenation of the next word
                            self.ed().insert_code(Code::Fmt(crate::fmt::Fmt::CancelHyph));
                            self.after_edit();
                            return;
                        }
                        _ => {}
                    }
                }
                if self.ed_ref().block_anchor.is_some() {
                    // With Block on, typing a character extends the block
                    // to its next occurrence ( . ? ! = end of sentence).
                    let e = self.ed();
                    let from = e.cursor;
                    if let Some(i) = e.find(&[El::Ch(c)], from, true, false) {
                        e.cursor = i + 1;
                    }
                } else if crate::math::type_operator(self.ed(), c) {
                    // math operator typed in a math column
                } else if self.ed_ref().doc.track {
                    crate::track::insert_tracked(self.ed(), El::Ch(c));
                } else {
                    self.ed().insert_char(c);
                }
            }
            Input::Func(f) => self.edit_func(f, home),
        }
        self.after_edit();
    }

    pub fn after_edit(&mut self) {
        let rows = self.doc_rows();
        let cols = self.cols as i32;
        let e = self.ed();
        e.ensure_layout();
        scroll_into_view(e, rows, cols);
        crate::fmtmenu::maybe_ask_hyph(self);
    }

    /// Colours of document `k`'s window.
    pub fn theme_for(&self, k: usize) -> Theme {
        match (self.settings.colors.as_str(), k) {
            ("wp", _) => Theme::wp_blue(),
            ("mono", _) => Theme::mono(),
            (_, 1) => Theme::hers_doc2(),
            _ => Theme::hers(),
        }
    }

    /// Doc 1 and Doc 2 have their own screen colours (her setup).
    pub fn apply_doc_theme(&mut self) {
        self.theme = match (self.settings.colors.as_str(), self.cur) {
            ("wp", _) => Theme::wp_blue(),
            ("mono", _) => Theme::mono(),
            (_, 1) => Theme::hers_doc2(),
            _ => Theme::hers(),
        };
    }

    /// The window size changed: new grid dimensions.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        for e in self.docs.iter_mut() {
            e.top_row = e.top_row.min(e.layout.drows.len());
        }
        self.after_edit();
    }

    /// Text lines on one page of the current document (for "fit page").
    pub fn page_lines(&self) -> usize {
        let d = &self.ed_ref().doc;
        let lh = (d.init.font.size_cpt as i64 * 12 / 72).max(1) as i32;
        ((d.init.page_height - d.init.top - d.init.bottom).max(lh) / lh) as usize
    }

    /// Rows of the (upper) document window.
    pub fn doc_rows(&self) -> usize {
        let top = self.doc_top();
        if self.ed_ref().reveal {
            return self.rows.saturating_sub(3 + top + self.reveal_window_rows());
        }
        match self.split {
            Some(n) if self.cur == 0 => n,
            Some(n) => self.rows.saturating_sub(n + 2 + top),
            None => self.rows - 1 - top,
        }
    }

    /// First screen row of the document (1 or 2 when the menu bar stays visible).
    pub fn doc_top(&self) -> usize {
        crate::menubar::bar_rows(self)
    }

    /// Screen row of the status line below the document window.
    pub fn status_row(&self) -> usize {
        if self.split.is_some() && !self.ed_ref().reveal {
            return self.rows - 1;
        }
        self.doc_top() + self.doc_rows()
    }

    /// Ctrl-F3, 1 Window: "Number of lines in this window:" — split or
    /// restore the screen.
    pub fn set_window(&mut self, lines: usize) {
        let full = self.rows - 1;
        self.split = if lines == 0 || lines >= full - 2 { None } else { Some(lines.max(2)) };
        if self.split.is_some() && self.cur == 1 {
            // the number refers to the current (lower) window
            self.split = Some(full.saturating_sub(lines + 1).max(2));
        }
        for k in 0..2 {
            let cur = self.cur;
            self.cur = k;
            self.after_edit();
            self.cur = cur;
        }
    }

    /// Setup → Display → Reveal Codes Window Size (WP default 10 on 25 lines).
    pub fn reveal_window_rows(&self) -> usize {
        (self.settings.reveal_rows * self.rows / 25).clamp(1, self.rows.saturating_sub(6).max(1))
    }

    /// A new, empty document with the default initial codes.
    pub fn fresh_editor(&self) -> Editor {
        let mut e = Editor::new();
        e.doc.id = new_doc_id();
        e.doc.initial_codes = self.settings.initial_code_els();
        e.relayout();
        e
    }

    /// Timed backup (Setup → Environment → Backup): every N minutes write
    /// modified documents to the backup directory as WP{WP}.BK1 / .BK2.
    pub fn maybe_backup(&mut self) {
        // crash-recovery copies every 30 s while there are unsaved changes
        if self.timers.last_recovery.elapsed() >= crate::history::RECOVERY_EVERY {
            self.timers.last_recovery = std::time::Instant::now();
            for (k, e) in self.docs.iter().enumerate() {
                if e.modified {
                    crate::history::write_recovery(e, k);
                }
            }
        }
        // autosave named Word documents to their own file
        let am = self.settings.autosave_minutes;
        if am > 0 && self.timers.last_autosave.elapsed() >= std::time::Duration::from_secs(am as u64 * 60) && self.screens.is_empty() {
            self.timers.last_autosave = std::time::Instant::now();
            for k in 0..2 {
                let e = &self.docs[k];
                let Some(p) = e.path.clone() else { continue };
                let is_docx = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("docx"));
                if e.modified && is_docx {
                    if fileio::save(&p, &e.doc).is_ok() {
                        crate::history::record(&self.docs[k].doc, Some(&p), "autosave");
                        crate::history::clear_recovery(&self.docs[k], k);
                        self.docs[k].modified = false;
                        if k == self.cur {
                            self.flash("* Autosaved *");
                        }
                    }
                }
            }
        }
        let mins = self.settings.backup_minutes;
        if mins == 0 {
            return;
        }
        if self.last_backup.elapsed() < std::time::Duration::from_secs(mins as u64 * 60) {
            return;
        }
        self.last_backup = std::time::Instant::now();
        let dir = self.settings.backup_dir();
        let _ = std::fs::create_dir_all(&dir);
        for (k, e) in self.docs.iter().enumerate() {
            if e.modified {
                let p = dir.join(format!("BSW51.BK{}.docx", k + 1));
                let _ = fileio::save(&p, &e.doc);
            }
        }
    }

    pub(crate) fn edit_func(&mut self, f: Func, home: u8) {
        use Func::*;
        // Home,Tab / Home,Enter …: hard tabs and [ISRt]
        if home > 0 && self.ed_ref().block_anchor.is_none() {
            if let Some(c) = crate::fmtmenu::home_code(f) {
                self.ed().insert_code(c);
                return;
            }
        }
        match f {
            Home => {
                self.home_count = home + 1;
                return;
            }
            Left => {
                let e = self.ed();
                match home {
                    0 => e.left(),
                    1 => {
                        // left edge of screen
                        e.line_start();
                    }
                    2 => e.line_start(),
                    _ => e.line_start_before_codes(),
                }
            }
            Right => {
                let e = self.ed();
                if home > 0 { e.line_end() } else { e.right() }
            }
            Up => {
                let rows = self.doc_rows();
                let e = self.ed();
                match home {
                    0 => e.up(),
                    1 => {
                        // top of screen, then up a screen
                        let li = e.cur_line();
                        let top_line = top_line_of(e);
                        if li > top_line {
                            for _ in 0..li - top_line {
                                e.up();
                            }
                        } else {
                            for _ in 0..rows {
                                e.up();
                            }
                        }
                    }
                    2 => e.doc_start(),
                    _ => e.doc_start_before_codes(),
                }
            }
            Down => {
                let rows = self.doc_rows();
                let e = self.ed();
                match home {
                    0 => e.down(),
                    1 => {
                        let li = e.cur_line();
                        let bottom = (top_line_of(e) + rows).saturating_sub(1).min(e.layout.lines.len() - 1);
                        if li < bottom {
                            for _ in 0..bottom - li {
                                e.down();
                            }
                        } else {
                            for _ in 0..rows {
                                e.down();
                            }
                        }
                    }
                    _ => e.doc_end(),
                }
            }
            ScreenUp => {
                self.edit_func(Up, 1);
                return;
            }
            ScreenDown => {
                self.edit_func(Down, 1);
                return;
            }
            WordLeft => self.ed().word_left(),
            WordRight => self.ed().word_right(),
            ParaUp => self.ed().para_up(),
            ParaDown => self.ed().para_down(),
            End => self.ed().line_end(),
            PgUp => self.ed().page_move(-1),
            PgDn => self.ed().page_move(1),
            GoTo => self.push(goto_prompt()),
            Enter if crate::links::enter_on_link(self) => {}
            Enter if crate::styles_ui::enter(self) => {}
            Enter => {
                if self.ed().block_anchor.is_some() {
                    // Enter extends block to next HRt
                    self.ed().para_down();
                } else {
                    self.ed().insert_code(Code::HRt);
                }
            }
            HardPage => crate::columns::hard_page(self.ed()),
            Tab if crate::tables::tab_in_table(self.ed(), true) => {}
            Tab if crate::styles_ui::tab(self, true) => {}
            MarRel if crate::styles_ui::tab(self, false) => {}
            MarRel if crate::tables::tab_in_table(self.ed(), false) => {}
            Tab => {
                if self.ed().typeover {
                    self.ed().right();
                } else {
                    self.ed().insert_code(Code::Tab);
                }
            }
            MarRel => self.ed().insert_code(Code::MarRel),
            Indent => self.ed().insert_code(Code::Indent),
            DblIndent => self.ed().insert_code(Code::DblIndent),
            Center => {
                if self.ed().block_anchor.is_some() {
                    self.push(YesNo::new("[Just:Center]?", false, |wp, yes| {
                        if yes {
                            if let Some((s, e)) = wp.ed().block_range() {
                                let ed = wp.ed();
                                let restore = ed.format_state_at(e).just;
                                ed.block_anchor = None;
                                ed.cursor = e;
                                ed.insert_code(Code::Just(restore));
                                ed.cursor = s;
                                ed.insert_code(Code::Just(Justify::Center));
                            }
                        }
                        None
                    }));
                } else {
                    self.ed().insert_code(Code::Center);
                }
            }
            FlushRight => {
                if self.ed().block_anchor.is_some() {
                    self.push(YesNo::new("[Just:Right]?", false, |wp, yes| {
                        if yes {
                            if let Some((s, e)) = wp.ed().block_range() {
                                let ed = wp.ed();
                                let restore = ed.format_state_at(e).just;
                                ed.block_anchor = None;
                                ed.cursor = e;
                                ed.insert_code(Code::Just(restore));
                                ed.cursor = s;
                                ed.insert_code(Code::Just(Justify::Right));
                            }
                        }
                        None
                    }));
                } else if self.ed_ref().cursor > 0 && self.ed_ref().doc.els.get(self.ed_ref().cursor - 1) == Some(&El::Code(Code::FlushRight)) {
                    // Alt-F6 twice: flush right with dot leader
                    let e = self.ed();
                    let k = e.cursor - 1;
                    e.doc.els[k] = El::Code(Code::FlushRightDot);
                    e.modified = true;
                    e.relayout();
                } else {
                    self.ed().insert_code(Code::FlushRight);
                }
            }
            HardSpace => self.ed().insert_code(Code::HardSpace),
            SoftHyphen => self.ed().insert_code(Code::SoftHyphen),
            Typeover => {
                let e = self.ed();
                e.typeover = !e.typeover;
            }
            Backspace if self.ed_ref().doc.track && home == 0 => {
                let e = self.ed();
                if e.block_anchor.is_some() {
                    crate::track::delete_block_tracked(e);
                } else if let Some(t) = e.backspace_target() {
                    crate::track::delete_tracked(e, t, true);
                }
            }
            Del if self.ed_ref().doc.track && home == 0 => {
                let e = self.ed();
                if e.block_anchor.is_some() {
                    crate::track::delete_block_tracked(e);
                } else if let Some(t) = e.del_target() {
                    crate::track::delete_tracked(e, t, false);
                }
            }
            Backspace => {
                let e = self.ed();
                if home > 0 {
                    e.delete_word_left();
                } else if e.block_anchor.is_some() {
                    self.ask_delete_block();
                } else if !e.reveal && e.cursor > 0 && is_invisible(&e.doc.els[e.cursor - 1]) {
                    let i = e.cursor - 1;
                    self.ask_delete_code(i, true);
                } else if e.backspace_target().is_some_and(|t| crate::tables::is_structure(&e.doc.els[t]) || matches!(e.doc.els[t], El::Code(Code::TblDef { .. }))) {
                    let t = e.backspace_target().unwrap();
                    self.ask_delete_code(t, true);
                } else if let Some(t) = e.backspace_target() {
                    if e.typeover && e.doc.els[t].is_char() {
                        e.cursor = t;
                        e.doc.els[t] = El::Ch(' ');
                    } else {
                        e.delete_at(t);
                    }
                }
            }
            Del => {
                let e = self.ed();
                if home > 0 {
                    e.delete_word_right();
                } else if e.block_anchor.is_some() {
                    self.ask_delete_block();
                } else if !e.reveal && e.cursor < e.doc.els.len() && is_invisible(&e.doc.els[e.cursor]) {
                    let i = e.cursor;
                    self.ask_delete_code(i, false);
                } else if e.del_target().is_some_and(|t| crate::tables::is_structure(&e.doc.els[t]) || matches!(e.doc.els[t], El::Code(Code::TblDef { .. }))) {
                    let t = e.del_target().unwrap();
                    self.ask_delete_code(t, false);
                } else if let Some(t) = e.del_target() {
                    e.cursor = t;
                    e.delete_at(t);
                }
            }
            DelWord => self.ed().delete_word(),
            DelEol => self.ed().delete_eol(),
            DelEop => self.push(YesNo::new("Delete Remainder of page?", false, |wp, y| {
                if y {
                    wp.ed().delete_eop();
                }
                None
            })),
            Esc => {
                if home == 0 {
                    self.pending_repeat = Some(u32::MAX);
                    self.message = Some(format!("Repeat Value = {}", self.repeat_value));
                }
            }
            Cancel => {
                if self.ed().block_anchor.is_some() {
                    self.ed().block_anchor = None;
                } else {
                    self.open_undelete();
                }
            }
            Bold | Underline => {
                let a = if f == Bold { Attr::Bold } else { Attr::Underline };
                if self.ed().block_anchor.is_some() {
                    self.ed().attr_block(a);
                } else {
                    self.ed().toggle_attr(a);
                }
            }
            Block => {
                let e = self.ed();
                e.block_anchor = if e.block_anchor.is_some() { None } else { Some(e.cursor) };
            }
            RevealCodes => {
                let e = self.ed();
                e.reveal = !e.reveal;
                if !e.reveal {
                    e.settle();
                }
            }
            Exit => self.exit_flow(),
            CloseWindow => {
                // Clicking the window's close button: same as F7 per doc,
                // but ending with exit.
                self.exit_flow();
            }
            Save => self.save_flow(false),
            Retrieve => self.retrieve_prompt(),
            List => crate::filelist::open_dir_prompt(self),
            SearchFwd => self.search_prompt(true),
            SearchBack => self.search_prompt(false),
            Replace => self.replace_flow(),
            Switch => {
                if self.ed().block_anchor.is_some() {
                    self.push(MenuLine::new(&["~Uppercase", "~Lowercase"], |wp, k| {
                        wp.ed().block_case(k == 0);
                        None
                    }));
                } else {
                    let r = self.ed_ref().reveal;
                    self.cur = 1 - self.cur;
                    self.docs[self.cur].reveal = r;
                    self.apply_doc_theme();
                }
            }
            Move => self.move_menu(),
            Font => crate::menus::font_menu(self),
            Format => crate::menus::format_menu(self),
            Setup => crate::menus::setup_menu(self),
            Help => crate::menus::help(self),
            Macro => crate::macros::run_prompt(self),
            MacroDefine if home > 0 => crate::macros::edit_prompt(self),
            MacroDefine => crate::macros::define_toggle(self),
            MenuBar => crate::menubar::open(self, None),
            MacroCommands if self.recording.is_some() => self.push(crate::wpm::macro_commands_menu()),
            MacroCommands => self.push(crate::wpm::assign_var_screen()),
            ItemLeft | ItemRight | ItemUp | ItemDown => self.item_move(f),
            AltHome => {
                self.home_count = home + 1;
                return;
            }
            Keyboard => crate::setup::keyboard_layout(self),
            AltLetter(c) => crate::macros::run_alt(self, c),
            Compose => self.push(crate::menus::compose()),
            DateOutline => crate::menus::date_menu(self),
            Print => crate::menus::print_menu(self),
            Spell => crate::spell::start(self),
            Thesaurus => crate::spell::thesaurus(self),
            Screen => crate::menus::screen_menu(self),
            TextInOut => crate::menus::text_in_out(self),
            MarkText => crate::marktext::menu(self),
            ColumnsTable => crate::tables::columns_tables_menu(self),
            Style => crate::styles_ui::open_styles(self),
            Graphics => crate::gfxui::graphics_menu(self),
            Footnote => crate::subedit::footnote_menu(self),
            MergeSort => crate::merge::merge_sort_menu(self),
            MergeCodes => crate::merge::merge_codes_menu(self),
            EndField => crate::merge::end_field(self),
            TabAlign => crate::fmtmenu::tab_align(self),
            Shell => crate::shell::menu(self),
            CtrlLetter('Z') => {
                if !self.ed().undo() {
                    self.flash("Nothing to undo");
                }
            }
            CtrlLetter('Y') => {
                if !self.ed().redo() {
                    self.flash("Nothing to redo");
                }
            }
            CtrlLetter(_) => {}
            Other(code) if crate::clipboard::handle(self, code) => {}
            Other(code) => self.flash(&format!("Key {code:#06x} not available yet")),
        }
    }

    /// Alt-arrows: next cell in a table, otherwise word / paragraph.
    fn item_move(&mut self, f: Func) {
        use crate::tables;
        let e = self.ed();
        if let Some(t) = tables::table_at(&e.doc.els, e.cursor) {
            if let Some((r, c)) = t.locate(e.cursor) {
                let (nr, nc) = match f {
                    Func::ItemLeft => (r, c.saturating_sub(1)),
                    Func::ItemRight => (r, c + 1),
                    Func::ItemUp => (r.saturating_sub(1), c),
                    _ => (r + 1, c),
                };
                tables::goto_cell(e, &t, nr, nc);
                return;
            }
        }
        match f {
            Func::ItemLeft => e.word_left(),
            Func::ItemRight => e.word_right(),
            Func::ItemUp => e.para_up(),
            _ => e.para_down(),
        }
    }

    /// Normal view: Backspace/Del ran into a hidden code →
    /// "Delete [BOLD]? No (Yes)".  No just moves past it.
    fn ask_delete_code(&mut self, i: usize, backwards: bool) {
        let El::Code(c) = &self.ed_ref().doc.els[i] else { return };
        if crate::tables::is_structure(&self.ed_ref().doc.els[i]) {
            // table cells/rows can only be changed in Table Edit
            return;
        }
        if let Code::TblDef { .. } = c {
            self.push(YesNo::new("Delete table structure (text remains)?", false, move |wp, yes| {
                if yes {
                    crate::tables::remove_structure(wp.ed(), i);
                    wp.after_edit();
                }
                None
            }));
            return;
        }
        let q = format!("Delete {}?", code_text(c));
        self.push(YesNo::new(&q, false, move |wp, yes| {
            let e = wp.ed();
            if yes {
                e.delete_at(i);
            } else if backwards {
                e.cursor = i;
            } else {
                e.cursor = i + 1;
            }
            wp.after_edit();
            None
        }));
    }

    fn ask_delete_block(&mut self) {
        self.push(YesNo::new("Delete Block?", false, |wp, y| {
            if y {
                wp.ed().delete_block();
            }
            None
        }));
    }

    fn open_undelete(&mut self) {
        if self.ed_ref().undelete.is_empty() {
            return;
        }
        self.push(Box::new(Undelete { which: 0 }));
    }

    fn move_menu(&mut self) {
        if self.ed_ref().block_anchor.is_some() {
            let m = MenuLine::new(&["~Block", "~Tabular Column", "~Rectangle"], |_wp, k| {
                if k != 0 {
                    return None;
                }
                Some(MenuLine::new(&["~Move", "~Copy", "~Delete", "~Append"], |wp, k| {
                    let els = wp.ed_ref().block_els().unwrap_or_default();
                    match k {
                        0 => {
                            wp.ed().delete_block();
                            crate::clipboard::export_block(wp, &els);
                            wp.ed().clipboard = Some(els);
                            wp.flash("Move cursor; press Enter to retrieve.");
                            Some(Box::new(RetrievePending) as Box<dyn Screen>)
                        }
                        1 => {
                            wp.ed().block_anchor = None;
                            crate::clipboard::export_block(wp, &els);
                            wp.ed().clipboard = Some(els);
                            wp.flash("Move cursor; press Enter to retrieve.");
                            Some(Box::new(RetrievePending) as Box<dyn Screen>)
                        }
                        2 => {
                            wp.ed().delete_block();
                            None
                        }
                        _ => {
                            wp.ed().block_anchor = None;
                            Some(Prompt::new("Append to:", "", move |wp, s| {
                                if let Some(s) = s {
                                    let p = wp.resolve_path(&s);
                                    match fileio::append(&p, &els) {
                                        Ok(()) => {}
                                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                                    }
                                }
                                None
                            }) as Box<dyn Screen>)
                        }
                    }
                }) as Box<dyn Screen>)
            });
            self.push(m.prefix(""));
        } else {
            let m = MenuLine::new(&["~Sentence", "~Paragraph", "~Page", "~Retrieve"], |wp, k| {
                if k == 3 {
                    return Some(MenuLine::new(&["~Block", "~Tabular Column", "~Rectangle"], |wp, _| {
                        if let Some(c) = wp.ed_ref().clipboard.clone() {
                            wp.ed().insert_els(&c);
                        }
                        None
                    }) as Box<dyn Screen>);
                }
                let e = wp.ed();
                let (s, t) = match k {
                    0 => sentence_bounds(e),
                    1 => para_bounds(e),
                    _ => page_bounds(e),
                };
                e.block_anchor = Some(s);
                e.cursor = t;
                Some(MenuLine::new(&["~Move", "~Copy", "~Delete", "~Append"], |wp, k| {
                    let els = wp.ed_ref().block_els().unwrap_or_default();
                    match k {
                        0 | 1 => {
                            if k == 0 {
                                wp.ed().delete_block();
                            } else {
                                wp.ed().block_anchor = None;
                            }
                            crate::clipboard::export_block(wp, &els);
                            wp.ed().clipboard = Some(els);
                            wp.flash("Move cursor; press Enter to retrieve.");
                            Some(Box::new(RetrievePending) as Box<dyn Screen>)
                        }
                        2 => {
                            wp.ed().delete_block();
                            None
                        }
                        _ => {
                            wp.ed().block_anchor = None;
                            None
                        }
                    }
                }) as Box<dyn Screen>)
            });
            self.push(m);
        }
    }

    // ---------------------------------------------------------- file flows

    pub fn doc_label(&self) -> String {
        match &self.ed_ref().path {
            Some(p) => p.display().to_string(),
            None => String::new(),
        }
    }

    /// F10 (and the save step of F7).  `then_exit` continues the Exit flow.
    pub fn save_flow(&mut self, then_exit: bool) {
        if self.ed_ref().block_anchor.is_some() {
            // Block save: "Block name:"
            let els = self.ed_ref().block_els().unwrap_or_default();
            self.ed().block_anchor = None;
            self.push(Prompt::new("Block name:", "", move |wp, s| {
                if let Some(s) = s.filter(|s| !s.trim().is_empty()) {
                    let p = wp.resolve_path(&s);
                    let mut d = Document::new();
                    d.els = els;
                    if let Err(e) = fileio::save(&p, &d) {
                        wp.flash(&format!("ERROR: {e}"));
                    }
                }
                None
            }));
            return;
        }
        let label = save_label(self.ed_ref().path.as_deref());
        self.push(Prompt::new("Document to be saved:", &label, move |wp, s| {
            let s = s?;
            if s.trim().is_empty() {
                return None;
            }
            let p = fileio::with_default_ext(&wp.resolve_path(&s));
            let same = wp.ed_ref().path.as_ref() == Some(&p);
            if p.exists() {
                let q = format!("Replace {}?", p.display());
                let p2 = p.clone();
                let _ = same;
                return Some(YesNo::new(&q, false, move |wp, yes| {
                    if yes {
                        wp.do_save(&p2);
                        if then_exit {
                            return wp.exit_question();
                        }
                        None
                    } else {
                        // WP asks again for a name
                        wp.save_flow(then_exit);
                        None
                    }
                }));
            }
            wp.do_save(&p);
            if then_exit {
                return wp.exit_question();
            }
            None
        }));
    }

    pub fn do_save(&mut self, p: &Path) {
        self.flash(&format!("Saving {}", p.display()));
        let res = fileio::save(p, &self.ed_ref().doc);
        match res {
            Ok(()) => {
                let cur = self.cur;
                let e = self.ed();
                e.path = Some(p.to_path_buf());
                e.modified = false;
                crate::recent::remember(self, p);
                let e = self.ed();
                crate::history::record(&e.doc, Some(p), "save");
                crate::history::clear_recovery(&self.docs[cur], cur);
                self.timers.last_autosave = std::time::Instant::now();
                self.message = None;
            }
            Err(e) => self.flash(&format!("ERROR: Can't save file — {e}")),
        }
    }

    pub fn exit_flow(&mut self) {
        if self.ed_ref().block_anchor.is_some() {
            self.ed().block_anchor = None;
        }
        let modified = self.ed_ref().modified;
        let q = "Save document?";
        self.push(YesNo::new(q, modified || !self.ed_ref().is_empty(), |wp, yes| {
            if yes {
                wp.save_flow(true);
                None
            } else {
                wp.exit_question()
            }
        }));
    }

    fn exit_question(&mut self) -> Next {
        let other = 1 - self.cur;
        let q = if self.docs[other].is_empty() && self.docs[other].path.is_none() {
            "Exit Blue Screen Writer 51?".to_string()
        } else {
            format!("Exit doc {}?", self.cur + 1)
        };
        Some(YesNo::new(&q, false, move |wp, yes| {
            // leaving this document: unsaved changes are kept in the
            // history as "discarded" so nothing is ever truly lost
            let cur = wp.cur;
            if wp.docs[cur].modified && !wp.docs[cur].is_empty() {
                let p = wp.docs[cur].path.clone();
                crate::history::record(&wp.docs[cur].doc, p.as_deref(), "discarded");
            }
            crate::history::clear_recovery(&wp.docs[cur], cur);
            if yes {
                let other = 1 - wp.cur;
                if wp.docs[other].is_empty() && wp.docs[other].path.is_none() {
                    wp.quit = true;
                } else {
                    wp.docs[wp.cur] = wp.fresh_editor();
                    wp.cur = other;
                    wp.apply_doc_theme();
                }
            } else {
                wp.docs[wp.cur] = wp.fresh_editor();
            }
            None
        }))
    }

    pub fn retrieve_prompt(&mut self) {
        let p = Prompt::new("Document to be retrieved:", "", |wp, s| {
            let s = s?;
            if s.trim().is_empty() {
                return None;
            }
            let p = wp.resolve_path(&s);
            wp.retrieve(&p);
            None
        });
        let rp = crate::recent::RecentPrompt::new(self, p);
        self.push(rp);
    }

    /// Retrieve a file: into the empty document, or inserted at the cursor
    /// (WP asks "Retrieve into current document?").
    pub fn retrieve(&mut self, p: &Path) {
        if fileio::is_encrypted(p) {
            crate::password::open_encrypted(self, p.to_path_buf());
            return;
        }
        match fileio::load(p) {
            Ok(doc) => self.retrieve_loaded(doc, p),
            Err(e) => self.flash(&format!("ERROR: File not found — {} ({e})", p.display())),
        }
    }

    /// Put a loaded document into the empty window, or insert it.
    pub fn retrieve_loaded(&mut self, mut doc: Document, p: &Path) {
        crate::recent::remember(self, p);
        {
            {
                if crate::history::on_open(&mut doc, p) {
                    self.flash("Note: changed by another program since last saved here — revision kept");
                }
                if self.ed_ref().is_empty() && self.ed_ref().path.is_none() {
                    let reveal = self.ed_ref().reveal;
                    let mut ed = Editor::with_doc(doc, Some(p.to_path_buf()));
                    ed.reveal = reveal;
                    ed.settle();
                    self.docs[self.cur] = ed;
                } else {
                    let p2 = p.to_path_buf();
                    self.push(YesNo::new("Retrieve into current document?", false, move |wp, yes| {
                        if yes {
                            wp.ed().insert_els(&doc.els);
                        }
                        let _ = p2;
                        None
                    }));
                }
            }
        }
    }

    // -------------------------------------------------------------- search

    fn search_prompt(&mut self, forward: bool) {
        let label = if forward { "-> Srch:" } else { "<- Srch:" };
        let init = els_to_prompt(&self.last_search);
        let mut p = Prompt::new(label, &init, move |wp, s| {
            let s = s?;
            let needle = prompt_to_els(&s);
            if needle.is_empty() {
                // nothing to look for: nothing found (macros rely on this)
                wp.flash("* Not found *");
                return None;
            }
            wp.last_search = needle.clone();
            wp.do_search(forward);
            None
        });
        p.replace_on_type = true;
        self.push(p.with_codes());
    }

    pub fn do_search(&mut self, forward: bool) -> bool {
        let needle = self.last_search.clone();
        let e = self.ed();
        let from = if forward { e.cursor } else { e.cursor.saturating_sub(1) };
        let found = e.find(&needle, from, forward, false);
        match found {
            Some(i) => {
                // WP leaves the cursor *after* the match when searching forward
                e.cursor = if forward { i + needle.len() } else { i };
                true
            }
            None => {
                self.flash("* Not found *");
                false
            }
        }
    }

    fn replace_flow(&mut self) {
        self.push(YesNo::new("w/Confirm?", false, |_wp, confirm| {
            Some(Prompt::new("-> Srch:", "", move |wp, s| {
                let s = s?;
                let needle = prompt_to_els(&s);
                if needle.is_empty() {
                    return None;
                }
                wp.last_search = needle;
                Some(Prompt::new("Replace with:", "", move |wp, r| {
                    let r = r?;
                    wp.last_replace = prompt_to_els(&r);
                    if confirm {
                        if wp.do_search(true) {
                            return Some(Box::new(ReplaceConfirm) as Box<dyn Screen>);
                        }
                        None
                    } else {
                        let mut n = 0;
                        let start = wp.ed_ref().cursor;
                        let rep = wp.last_replace.clone();
                        let needle = wp.last_search.clone();
                        let e = wp.ed();
                        let mut from = e.cursor;
                        while let Some(i) = e.find(&needle, from, true, false) {
                            e.delete_range(i, i + needle.len(), false);
                            e.cursor = i;
                            e.insert_els(&rep);
                            from = i + rep.len();
                            n += 1;
                            if n > 1_000_000 {
                                break;
                            }
                        }
                        let _ = start;
                        let _ = n;
                        None
                    }
                }).with_codes())
            }).with_codes())
        }));
    }
}

/// Text typed in a search prompt → elements.  Codes can be typed as their
/// Reveal Codes names in brackets, e.g. `[BOLD]`, `[HRt]`, `[Tab]`.
pub fn prompt_to_els(s: &str) -> Vec<El> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        if rest.starts_with('[') {
            if let Some(end) = rest.find(']') {
                let name = &rest[1..end];
                let code = match name {
                    "HRt" => Some(Code::HRt),
                    "HPg" => Some(Code::HPg),
                    "Tab" => Some(Code::Tab),
                    "Center" => Some(Code::Center),
                    "Flsh Rgt" => Some(Code::FlushRight),
                    " " => Some(Code::HardSpace),
                    "-" => Some(Code::HardHyphen),
                    "\u{2192}Indent" => Some(Code::Indent),
                    "\u{2192}Indent\u{2190}" => Some(Code::DblIndent),
                    "\u{2190}Mar Rel" => Some(Code::MarRel),
                    _ => {
                        if let Some(a) = Attr::from_mnemonic(name) {
                            if name.chars().any(|c| c.is_uppercase()) && name == name.to_uppercase() {
                                Some(Code::AttrOn(a))
                            } else {
                                Some(Code::AttrOff(a))
                            }
                        } else {
                            None
                        }
                    }
                };
                if let Some(c) = code {
                    out.push(El::Code(c));
                    rest = &rest[end + 1..];
                    continue;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(El::Ch(c));
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub fn els_to_prompt(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(c) => s.push_str(&code_text(c)),
        }
    }
    s
}

fn sentence_bounds(e: &mut Editor) -> (usize, usize) {
    let els = &e.doc.els;
    let is_end = |x: &El| matches!(x, El::Ch('.' | '!' | '?')) || matches!(x, El::Code(Code::HRt));
    let mut s = e.cursor;
    while s > 0 && !is_end(&els[s - 1]) {
        s -= 1;
    }
    while s < els.len() && matches!(els[s], El::Ch(' ')) {
        s += 1;
    }
    let mut t = e.cursor;
    while t < els.len() && !is_end(&els[t]) {
        t += 1;
    }
    t = (t + 1).min(els.len());
    while t < els.len() && matches!(els[t], El::Ch(' ')) {
        t += 1;
    }
    (s, t)
}

fn para_bounds(e: &mut Editor) -> (usize, usize) {
    let els = &e.doc.els;
    let mut s = e.cursor;
    while s > 0 && !matches!(els[s - 1], El::Code(Code::HRt)) {
        s -= 1;
    }
    let mut t = e.cursor;
    while t < els.len() && !matches!(els[t], El::Code(Code::HRt)) {
        t += 1;
    }
    (s, (t + 1).min(els.len()))
}

fn page_bounds(e: &mut Editor) -> (usize, usize) {
    let li = e.cur_line();
    let page = e.layout.lines[li].page;
    let s = e.layout.lines.iter().find(|l| l.page == page).map(|l| l.start).unwrap_or(0);
    let t = e.layout.lines.iter().rev().find(|l| l.page == page).map(|l| l.end).unwrap_or(e.doc.len());
    (s, t)
}

fn goto_prompt() -> Box<dyn Screen> {
    Box::new(GoTo)
}

/// Ctrl-Home "Go to": a page number + Enter, a character, ↑/↓.
struct GoTo;
impl Screen for GoTo {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char(c) if c.is_ascii_digit() => {
                let first = *c;
                Outcome::Replace(Prompt::new("Go to", &first.to_string(), |wp, s| {
                    if let Some(n) = s.and_then(|s| s.trim().parse::<u32>().ok()) {
                        wp.ed().goto_page(n);
                    }
                    None
                }).keep_text())
            }
            Input::Char(c) => {
                let e = wp.ed();
                if let Some(i) = e.find(&[El::Ch(*c)], e.cursor + 1, true, false) {
                    e.cursor = i + 1;
                }
                Outcome::Close
            }
            Input::Func(Func::Up) => {
                wp.ed().page_top();
                Outcome::Close
            }
            Input::Func(Func::Down) => {
                wp.ed().page_bottom();
                Outcome::Close
            }
            // text columns: next / previous column; Home: first / last
            Input::Func(Func::Right) => {
                crate::columns::goto_column(wp.ed(), crate::columns::GoCol::Next);
                Outcome::Close
            }
            Input::Func(Func::Left) => {
                crate::columns::goto_column(wp.ed(), crate::columns::GoCol::Prev);
                Outcome::Close
            }
            Input::Func(Func::Home) => Outcome::Replace(Box::new(crate::columns::GoToHome)),
            Input::Func(Func::GoTo) => {
                // Ctrl-Home twice: back to the position before last jump
                Outcome::Close
            }
            Input::Func(Func::Enter) | Input::Func(Func::Cancel) => Outcome::Close,
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let c = g.text(0, row, "Go to ", wp.theme.normal);
        g.cursor = Some((c, row));
    }
}

/// "Move cursor; press Enter to retrieve."
pub(crate) struct RetrievePending;
impl Screen for RetrievePending {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Enter) => {
                if let Some(c) = wp.ed_ref().clipboard.clone() {
                    wp.ed().insert_els(&c);
                }
                wp.message = None;
                wp.after_edit();
                Outcome::Close
            }
            Input::Func(Func::Cancel) => {
                wp.message = None;
                Outcome::Close
            }
            Input::Func(f) if is_motion(*f) => {
                wp.edit_input(i.clone());
                wp.flash("Move cursor; press Enter to retrieve.");
                Outcome::Keep
            }
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        g.text(0, row, "Move cursor; press Enter to retrieve.", wp.theme.normal);
        // the document cursor stays visible
        let _ = g;
    }
}

fn is_motion(f: Func) -> bool {
    use Func::*;
    matches!(
        f,
        Left | Right | Up | Down | WordLeft | WordRight | Home | End | PgUp | PgDn | ScreenUp | ScreenDown | GoTo | ParaUp | ParaDown
    )
}

/// Replace with confirmation: "Confirm? No (Yes)".
struct ReplaceConfirm;
impl Screen for ReplaceConfirm {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let yes = match i {
            Input::Char(c) if c.eq_ignore_ascii_case(&'y') || c.eq_ignore_ascii_case(&'j') => true,
            Input::Func(Func::Cancel) | Input::Func(Func::Exit) => return Outcome::Close,
            _ => false,
        };
        let n = wp.last_search.len();
        if yes {
            let rep = wp.last_replace.clone();
            let e = wp.ed();
            let start = e.cursor - n;
            e.delete_range(start, start + n, false);
            e.cursor = start;
            e.insert_els(&rep);
        }
        if wp.do_search(true) {
            wp.after_edit();
            Outcome::Keep
        } else {
            wp.message = None;
            Outcome::Close
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let c = g.text(0, row, "Confirm? ", wp.theme.normal);
        let c2 = g.text(c, row, "No", wp.theme.bold);
        g.text(c2, row, " (Yes)", wp.theme.normal);
    }
}

/// F1 Cancel → Undelete: shows the deleted text at the cursor highlighted.
struct Undelete {
    which: usize,
}
impl Screen for Undelete {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = wp.ed_ref().undelete.len();
        match i {
            Input::Char('1') | Input::Char('r') | Input::Char('R') => {
                let w = self.which;
                wp.ed().undelete_restore(w);
                wp.after_edit();
                Outcome::Close
            }
            Input::Char('2') | Input::Char('p') | Input::Char('P') | Input::Func(Func::Up) => {
                self.which = (self.which + 1) % n.max(1);
                Outcome::Keep
            }
            Input::Func(Func::Down) => {
                self.which = (self.which + n.max(1) - 1) % n.max(1);
                Outcome::Keep
            }
            _ => Outcome::Close,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        // preview the deleted text at the cursor position
        if let Some((col, row)) = g.cursor {
            if let Some(els) = wp.ed_ref().undelete.get(self.which) {
                let mut c = col;
                for e in els.iter().take(200) {
                    let ch = match e {
                        El::Ch(c) => *c,
                        El::Code(Code::HRt) => break,
                        El::Code(Code::HardSpace) => ' ',
                        _ => continue,
                    };
                    g.put(c, row, ch, wp.theme.block);
                    c += 1;
                }
            }
        }
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let mut c = g.text(0, row, "Undelete: ", wp.theme.normal);
        c = g.text(c, row, "1 ", wp.theme.normal);
        c = g.text_mn(c, row, "~Restore; 2 ~Previous Deletion: ", wp.theme.normal, wp.theme.bold);
        g.text(c, row, "0", wp.theme.normal);
        g.cursor = Some((c, row));
    }
}

// ------------------------------------------------------------------ draw

fn top_line_of(e: &Editor) -> usize {
    // first line whose display row is at or below top_row
    let l = &e.layout;
    for d in e.top_row..l.drows.len() {
        if let crate::layout::DRow::V(v) = l.drows[d] {
            if let Some(crate::layout::VRow::Lines(ls)) = l.vrows.get(v) {
                if let Some(&li) = ls.iter().min() {
                    return li;
                }
            }
        }
    }
    l.lines.len().saturating_sub(1)
}

fn display_row_of_line(e: &Editor, li: usize) -> usize {
    e.layout.drow_of_line(li)
}

pub fn scroll_into_view(e: &mut Editor, rows: usize, cols: i32) {
    // A wide screen shows the page centred.  When the screen is narrower
    // than the paper, the blank paper margins are trimmed equally on both
    // sides, so the text moves towards the left edge as the letters grow;
    // once the text reaches the left edge it stays there and the right side
    // scrolls with the cursor.
    // On screen every letter takes a whole column, so lines in a small or
    // proportional font run wider than the paper: the width to show is the
    // widest line plus a right margin as wide as the left one.
    let lm_col = (e.doc.init.left / COL_WPU - 1).max(0);
    let widest = e.layout.lines.iter().filter_map(|l| l.cells.last().map(|c| c.col + 1)).max().unwrap_or(0);
    let page_cols = (e.doc.init.page_width / COL_WPU + 1).max(widest + lm_col + 1);
    let base_left = if cols > page_cols { -((cols - page_cols) / 2) } else { ((page_cols - cols) / 2).min(lm_col) };
    let li = e.layout.line_of(e.cursor);
    let r = display_row_of_line(e, li);
    if r < e.top_row {
        e.top_row = r;
    } else if r >= e.top_row + rows {
        e.top_row = r + 1 - rows;
    }
    let col = e.layout.lines[li].caret_of(e.cursor).0;
    if base_left < 0 || (col >= base_left && col < base_left + cols - 2) {
        e.left_col = base_left;
        if col >= e.left_col + cols {
            e.left_col = col - cols + 10;
        }
    } else if col < e.left_col {
        e.left_col = (col - 10).max(0);
    } else if col >= e.left_col + cols {
        e.left_col = col - cols + 10;
    }
}

/// Text-mode table border row: ╔═══╤═══╗ / ╟───┼───╢ / ╚═══╧═══╝
fn border_chars(kind: crate::layout::BorderKind) -> (char, char, char, char) {
    use crate::layout::BorderKind::*;
    match kind {
        Top => ('╔', '═', '╤', '╗'),
        Mid => ('╟', '─', '┼', '╢'),
        Bottom => ('╚', '═', '╧', '╝'),
    }
}

impl Wp {
    /// Register a clickable area (called from drawing code).
    pub fn hit(&self, row: usize, c0: usize, c1: usize, action: HitAction) {
        self.hits.borrow_mut().push(Hit { row, c0, c1, action });
    }

    /// Handle a mouse event at text cell (col, row).
    pub fn mouse(&mut self, kind: MouseKind, col: usize, row: usize) {
        if let MouseKind::Wheel(n) = kind {
            let f = if n < 0 { Func::Down } else { Func::Up };
            for _ in 0..(n.unsigned_abs() * 3).min(30) {
                self.input(Input::Func(f));
            }
            return;
        }
        if kind == MouseKind::Right {
            if self.screens.is_empty() {
                // WP 5.1: the right mouse button brings up the menu bar
                crate::menubar::open(self, None);
            } else {
                self.input(Input::Func(Func::Cancel));
            }
            return;
        }
        let hit = self.hits.borrow().iter().rev().find(|h| h.row == row && col >= h.c0 && col < h.c1).cloned();
        match (kind, hit) {
            (MouseKind::Down | MouseKind::Double, Some(Hit { action: HitAction::Input(i), .. })) => {
                if kind == MouseKind::Down {
                    self.input(i);
                }
            }
            (MouseKind::Down | MouseKind::Double, Some(Hit { action: HitAction::Select(n), .. })) => {
                if let Some(mut top) = self.screens.pop() {
                    let base = self.screens.len();
                    let out = top.mouse_select(self, n, kind == MouseKind::Double);
                    let base = base.min(self.screens.len());
                    match out {
                        Outcome::Keep => self.screens.insert(base, top),
                        Outcome::Close => {}
                        Outcome::CloseAll => self.screens.clear(),
                        Outcome::Replace(s) => self.screens.insert(base, s),
                        Outcome::Push(s) => {
                            self.screens.insert(base, top);
                            self.screens.insert(base + 1, s);
                        }
                    }
                }
            }
            (_, Some(Hit { action: HitAction::Cursor(i), .. })) if self.screens.is_empty() => {
                self.click_cursor(i, kind);
            }
            (MouseKind::Down, None) if self.screens.is_empty() && row < self.doc_top() => {
                crate::menubar::open(self, crate::menubar::item_at(col));
            }
            (_, None) if self.screens.is_empty() => {
                // split screen: a click in the other window switches to it
                let mut row = row;
                if let Some(n) = self.split.filter(|_| !self.ed_ref().reveal) {
                    let k = if row < n { 0 } else if row > n { 1 } else { self.cur };
                    if k != self.cur && kind == MouseKind::Down {
                        self.cur = k;
                        self.apply_doc_theme();
                    }
                    if self.cur == 1 {
                        row = row.saturating_sub(n + 1);
                    }
                }
                if let Some(i) = self.doc_index_at(col, row) {
                    self.click_cursor(i, kind);
                }
            }
            _ => {}
        }
        if kind == MouseKind::Up {
            self.drag_from = None;
        }
    }

    fn click_cursor(&mut self, i: usize, kind: MouseKind) {
        let drag_from = self.drag_from;
        let e = &mut self.docs[self.cur];
        match kind {
            MouseKind::Down => {
                e.block_anchor = None;
                e.cursor = i.min(e.doc.els.len());
                let c = e.cursor;
                self.drag_from = Some(c);
            }
            MouseKind::Drag => {
                if let Some(from) = drag_from {
                    if from != i {
                        e.block_anchor = Some(from);
                    }
                    e.cursor = i.min(e.doc.els.len());
                }
            }
            MouseKind::Double => {
                // select the word
                let els = &e.doc.els;
                let is_w = |x: &El| matches!(x, El::Ch(c) if c.is_alphanumeric());
                let mut a = i.min(els.len());
                while a > 0 && is_w(&els[a - 1]) {
                    a -= 1;
                }
                let mut b = i.min(els.len());
                while b < els.len() && is_w(&els[b]) {
                    b += 1;
                }
                if b > a {
                    e.block_anchor = Some(a);
                    e.cursor = b;
                }
            }
            _ => {}
        }
        self.after_edit();
    }

    /// Element index under a screen cell of the document window.
    pub(crate) fn doc_index_at(&self, col: usize, row: usize) -> Option<usize> {
        let row = row.checked_sub(self.doc_top())?;
        if row >= self.doc_rows() {
            return None;
        }
        let e = self.ed_ref();
        let lay = &e.layout;
        let d = e.top_row + row;
        let crate::layout::DRow::V(v) = *lay.drows.get(d)? else { return None };
        let crate::layout::VRow::Lines(ls) = &lay.vrows[v] else { return None };
        let c = col as i32 + e.left_col;
        let li = *ls.iter().min_by_key(|&&l| {
            let line = &lay.lines[l];
            let a = line.caret.first().map(|x| x.0).unwrap_or(0);
            let b = line.caret.last().map(|x| x.0).unwrap_or(a);
            if c < a { a - c } else if c > b { c - b } else { 0 }
        })?;
        Some(lay.idx_at_col(li, c))
    }

    pub fn draw(&self, g: &mut Grid) {
        self.hits.borrow_mut().clear();
        g.clear(self.theme.normal);
        let full = self.screens.iter().any(|s| s.full_screen());
        if !full {
            self.draw_editor(g);
        }
        // draw screens bottom-up (only from the last full screen)
        let start = self.screens.iter().rposition(|s| s.full_screen()).unwrap_or(0);
        for s in &self.screens[start..] {
            s.draw(self, g);
        }
    }

    fn draw_editor(&self, g: &mut Grid) {
        let doc_rows = self.doc_rows();
        // the menu bar (when it stays visible) takes the top row(s)
        let top = self.doc_top();
        if top > 0 {
            crate::menubar::draw_bar(self, g, None);
        }
        if let Some(n) = self.split.filter(|_| !self.ed_ref().reveal) {
            // split screen: Doc 1 on top, a ruler, Doc 2 below
            let (t0, h0) = (top, n);
            let (t1, h1) = (top + n + 1, self.rows.saturating_sub(n + 2 + top));
            let c0 = self.draw_doc(g, 0, t0, h0, self.cur == 0);
            let c1 = self.draw_doc(g, 1, t1, h1, self.cur == 1);
            self.draw_ruler_for(g, top + n, 0);
            g.cursor = if self.cur == 0 { c0 } else { c1 };
            self.draw_status(g, self.rows - 1);
            return;
        }
        let e = self.ed_ref();
        g.cursor = self.draw_doc(g, self.cur, top, doc_rows, true);
        // ---- status line
        let status_row = top + doc_rows;
        self.draw_status(g, status_row);

        // ---- reveal codes
        if e.reveal {
            self.draw_ruler(g, status_row + 1);
            self.draw_reveal(g, status_row + 2, status_row + 2 + self.reveal_window_rows());
        }
    }

    /// Draw document `k` into screen rows `top..top+height`; returns the
    /// cursor position if `with_cursor`.
    fn draw_doc(&self, g: &mut Grid, k: usize, top: usize, height: usize, with_cursor: bool) -> Option<(usize, usize)> {
        let e = &self.docs[k];
        let th_owned = self.theme_for(k);
        let th = &th_owned;
        // window background in this document's colour
        for r in top..(top + height).min(g.rows) {
            g.fill_row(r, 0, g.cols, ' ', th.normal);
        }
        let block = e.block_range();
        let cursor_li = e.layout.line_of(e.cursor);
        let left = e.left_col;
        // ---- document text rows (by display row: text, table borders, page breaks)
        let mut cursor_screen: Option<(usize, usize)> = None;
        let lay = &e.layout;
        for row in top..top + height {
            let d = e.top_row + (row - top);
            let Some(dr) = lay.drows.get(d) else { break };
            match *dr {
                crate::layout::DRow::Break(k) => {
                    let ch = if k == PageBreak::Hard { '═' } else { '─' };
                    g.fill_row(row, 0, g.cols, ch, th.page_break);
                }
                crate::layout::DRow::Comment { el, row: r } => crate::comments::draw_row(g, &e.doc, el, r as usize, row, left, th.normal),
                crate::layout::DRow::V(v) => match &lay.vrows[v] {
                    crate::layout::VRow::Border { table, kind } => {
                        let xs = &lay.tables[*table].x;
                        let (l0, h, mid, r0) = border_chars(*kind);
                        let c0 = xs[0] / COL_WPU - left;
                        let c1 = xs[xs.len() - 1] / COL_WPU - left;
                        for c in c0..=c1 {
                            if c >= 0 && (c as usize) < g.cols {
                                g.put(c as usize, row, h, th.normal);
                            }
                        }
                        for (k, x) in xs.iter().enumerate() {
                            let c = x / COL_WPU - left;
                            if c >= 0 && (c as usize) < g.cols {
                                let ch = if k == 0 { l0 } else if k == xs.len() - 1 { r0 } else { mid };
                                g.put(c as usize, row, ch, th.normal);
                            }
                        }
                    }
                    crate::layout::VRow::Gap(_) => {}
                    crate::layout::VRow::Lines(ls) => {
                        // table rows get vertical rules
                        let tbl = ls.first().and_then(|&li| {
                            let t = lay.lines[li].start;
                            let _ = t;
                            lay.vrows[..v].iter().rev().find_map(|vr| match vr {
                                crate::layout::VRow::Border { table, kind } if *kind != crate::layout::BorderKind::Bottom => Some(*table),
                                crate::layout::VRow::Border { .. } => Some(usize::MAX),
                                _ => None,
                            })
                        });
                        if let Some(t) = tbl.filter(|t| *t != usize::MAX) {
                            let xs = &lay.tables[t].x;
                            for (k, x) in xs.iter().enumerate() {
                                let c = x / COL_WPU - left;
                                if c >= 0 && (c as usize) < g.cols {
                                    let ch = if k == 0 || k == xs.len() - 1 { '║' } else { '│' };
                                    g.put(c as usize, row, ch, th.normal);
                                }
                            }
                        }
                        for &li in ls {
                            let l = &lay.lines[li];
                            self.draw_line(g, e, l, row, left, block, th);
                            if li == cursor_li && with_cursor {
                                let (cc, _) = l.caret_of(e.cursor);
                                let x = (cc - left).clamp(0, g.cols as i32 - 1) as usize;
                                cursor_screen = Some((x, row));
                            }
                        }
                    }
                },
            }
        }
        if top == self.doc_top() {
            crate::gfx::draw_outlines(g, e, height, th.normal);
        }

        cursor_screen
    }

    fn draw_line(&self, g: &mut Grid, e: &Editor, l: &crate::layout::Line, row: usize, left: i32, block: Option<(usize, usize)>, th: &Theme) {
        let mut jp: Option<bool> = None;
        for c in &l.cells {
            let x = c.col - left;
            if x < 0 || x as usize >= g.cols {
                continue;
            }
            let mut st = th.for_attrs(c.attrs);
            if let Some((bs, be)) = block {
                if c.el >= bs && c.el < be {
                    st = th.block.flags(st.flags & (F_UNDER | F_BOLD | F_ITALIC));
                }
            }
            if crate::cjk::is_cjk(c.ch) && *jp.get_or_insert_with(|| crate::cjk::para_japanese(&e.doc.els, l.start)) {
                st = st.flags(st.flags | F_JA);
            }
            g.put(x as usize, row, c.ch, st);
            // right half of a double-width character: same colours, no glyph
            if crate::cjk::is_wide(c.ch) && (x as usize) + 1 < g.cols {
                g.put(x as usize + 1, row, '\0', st);
            }
        }
        // spell check as you type: red wavy underline
        for idx in crate::aids::misspelled(self, l.start, l.end) {
            for c in l.cells.iter().filter(|c| c.el == idx) {
                let x = c.col - left;
                if x >= 0 && (x as usize) < g.cols {
                    if let Some(cell) = g.get_mut(x as usize, row) {
                        cell.flags |= F_SPELL;
                    }
                }
            }
        }
        // block highlight across tab gaps / line ends
        if let Some((bs, be)) = block {
            for idx in l.start..l.end {
                if idx >= bs && idx < be {
                    if let El::Code(Code::Tab | Code::Indent | Code::DblIndent | Code::HRt) = e.doc.els[idx] {
                        let (c0, _) = l.caret_of(idx);
                        let (c1, _) = if idx + 1 <= l.end { l.caret_of(idx + 1) } else { (c0 + 1, 0) };
                        let c1 = if matches!(e.doc.els[idx], El::Code(Code::HRt)) { c0 + 1 } else { c1 };
                        for x in c0..c1.max(c0 + 1) {
                            let sx = x - left;
                            if sx >= 0 && (sx as usize) < g.cols {
                                if let Some(cell) = g.get_mut(sx as usize, row) {
                                    if cell.ch == ' ' {
                                        cell.fg = th.block.fg;
                                        cell.bg = th.block.bg;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_status(&self, g: &mut Grid, row: usize) {
        let th = &self.theme;
        let e = self.ed_ref();
        g.fill_row(row, 0, g.cols, ' ', th.normal);
        // left part
        let left_text: String = if let Some(m) = &self.message {
            m.clone()
        } else if e.block_anchor.is_some() {
            String::new()
        } else if e.typeover {
            "Typeover".into()
        } else if crate::math::in_math_area(&e.doc.els, e.cursor) {
            "Math".into()
        } else if crate::styles::outline_mode(&e.doc, e.cursor) {
            "Outline".into()
        } else if self.settings.filename_on_status {
            self.doc_label()
        } else {
            String::new()
        };
        let left_text = if e.doc.track && self.message.is_none() { format!("[Track] {left_text}") } else { left_text };
        if e.block_anchor.is_some() && self.message.is_none() {
            g.text(0, row, "Block on", th.normal.flags(F_BLINK));
        } else if self.recording.is_some() && self.message.is_none() {
            g.text(0, row, "Macro Def", th.normal.flags(F_BLINK));
        } else {
            let maxw = (g.cols / 2).max(42);
            let shown: String = if left_text.chars().count() > maxw {
                let n = left_text.chars().count();
                left_text.chars().skip(n - maxw).collect()
            } else {
                left_text
            };
            g.text(0, row, &shown, th.normal);
        }
        // right part: Doc 1 Pg 1 Ln 1" Pos 1"
        let li = e.layout.line_of(e.cursor);
        let l = &e.layout.lines[li];
        let (_, pos) = l.caret_of(e.cursor);
        let attrs = e.doc.attrs_at(e.cursor);
        let ln = fmt_status(l.y);
        let ps = fmt_status(pos);
        let caps = false;
        let pos_word = if caps { "POS" } else { "Pos" };
        let col = crate::columns::status_col(e).map(|c| format!("Col {c} ")).unwrap_or_default();
        let s1 = format!("{col}Doc {} Pg {} Ln {} {} ", self.cur + 1, l.page, ln, pos_word);
        let total = s1.chars().count() + ps.chars().count();
        let start = g.cols.saturating_sub(total + 1);
        let c = g.text(start, row, &s1, th.normal);
        let mut pst = th.for_attrs(attrs);
        if attrs.is_empty() {
            pst = th.normal;
        }
        g.text(c, row, &ps, pst);
    }

    fn draw_ruler(&self, g: &mut Grid, row: usize) {
        self.draw_ruler_for(g, row, self.cur);
    }

    fn draw_ruler_for(&self, g: &mut Grid, row: usize, k: usize) {
        let th = &self.theme;
        let e = &self.docs[k];
        let fs = e.format_state_at(e.cursor);
        let left = e.left_col;
        g.fill_row(row, 0, g.cols, '▀', Style::new(pal::LGRAY, pal::BLUE));
        // WP 5.1: tab stops shown as ▲, margins as [ ] (or { } when a tab
        // falls on the margin).
        let base = if fs.tabs.relative { fs.left } else { 0 };
        // in text columns: the margins of the cursor's column
        let (lm, rm) = crate::columns::cursor_edges(e).unwrap_or((fs.left, e.doc.init.page_width - fs.right));
        let lcol = lm / COL_WPU - left;
        let rcol = rm / COL_WPU - left;
        let mut tabcols = Vec::new();
        for t in &fs.tabs.stops {
            let c = (t.pos + base) / COL_WPU - left;
            tabcols.push(c);
            if c >= 0 && (c as usize) < g.cols {
                let ch = match t.kind {
                    crate::doc::TabKind::Left => '▲',
                    crate::doc::TabKind::Center => 'C',
                    crate::doc::TabKind::Right => 'R',
                    crate::doc::TabKind::Decimal => 'D',
                };
                g.put(c as usize, row, ch, th.ruler);
            }
        }
        if lcol >= 0 && (lcol as usize) < g.cols {
            g.put(lcol as usize, row, if tabcols.contains(&lcol) { '{' } else { '[' }, th.ruler);
        }
        if rcol >= 0 && (rcol as usize) < g.cols {
            g.put(rcol as usize, row, if tabcols.contains(&rcol) { '}' } else { ']' }, th.ruler);
        }
    }

    fn draw_reveal(&self, g: &mut Grid, top: usize, bottom: usize) {
        let th = &self.theme;
        let e = self.ed_ref();
        let height = bottom.saturating_sub(top);
        if height == 0 {
            return;
        }
        // Build reveal rows from layout lines around the cursor.
        struct Tok {
            text: String,
            st: Style,
            idx: Option<usize>,
        }
        let cursor_li = e.layout.line_of(e.cursor);
        let from_li = cursor_li.saturating_sub(height);
        let to_li = (cursor_li + height + 1).min(e.layout.lines.len());
        let mut rows: Vec<Vec<Tok>> = Vec::new();
        let mut cursor_row = 0usize;
        let block = e.block_range();
        for li in from_li..to_li {
            let l = &e.layout.lines[li];
            let mut toks: Vec<Tok> = Vec::new();
            for idx in l.start..l.end {
                if e.block_anchor == Some(idx) {
                    toks.push(Tok { text: "[Block]".into(), st: th.code, idx: None });
                }
                let el = &e.doc.els[idx];
                let mut t = match el {
                    // a typed hyphen is WP's "hyphen character" [-]
                    El::Ch('-') => Tok { text: "[-]".into(), st: th.code, idx: Some(idx) },
                    El::Ch(c) => Tok { text: c.to_string(), st: th.normal, idx: Some(idx) },
                    El::Code(Code::Tab) => Tok { text: crate::reveal::tab_text(l.tab_kind(idx)).into(), st: th.code, idx: Some(idx) },
                    El::Code(c @ Code::Note { .. }) => {
                        let n = e.layout.note_nums.get(&idx).copied().unwrap_or(0);
                        Tok { text: code_text(c).replacen('#', &n.to_string(), 1), st: th.code, idx: Some(idx) }
                    }
                    El::Code(c @ Code::GBox(_)) => {
                        let n = crate::gfx::box_number(&e.layout, idx);
                        Tok { text: code_text(c).replacen('#', &n, 1), st: th.code, idx: Some(idx) }
                    }
                    El::Code(c) => Tok { text: code_text(c), st: th.code, idx: Some(idx) },
                };
                if let Some((bs, be)) = block {
                    if idx >= bs && idx < be {
                        t.st = th.block;
                    }
                }
                toks.push(t);
            }
            if e.block_anchor == Some(l.end) && l.end == e.doc.els.len() {
                toks.push(Tok { text: "[Block]".into(), st: th.code, idx: None });
            }
            if l.term == LineEnd::Soft {
                toks.push(Tok { text: "[SRt]".into(), st: th.code, idx: None });
            }
            // a hard return that ends a page shows as [HRt-SPg]
            let hrt_spg = l.break_after == Some(PageBreak::Soft) && l.term == LineEnd::Hard;
            if hrt_spg {
                if let Some(t) = toks.iter_mut().rev().find(|t| t.text == "[HRt]") {
                    t.text = "[HRt-SPg]".into();
                }
            }
            if li + 1 == e.layout.lines.len() && e.cursor == e.doc.els.len() {
                toks.push(Tok { text: " ".into(), st: th.normal, idx: Some(e.doc.els.len()) });
            } else if l.term == LineEnd::Eof && l.start == l.end {
                // nothing
            }
            // wrap into screen rows
            let mut cur: Vec<Tok> = Vec::new();
            let mut w = 0usize;
            let mut chunks: Vec<Vec<Tok>> = Vec::new();
            for t in toks {
                let tw = t.text.chars().count();
                if w + tw > g.cols && !cur.is_empty() {
                    chunks.push(std::mem::take(&mut cur));
                    w = 0;
                }
                w += tw;
                cur.push(t);
            }
            chunks.push(cur);
            for ch in chunks {
                if ch.iter().any(|t| t.idx == Some(e.cursor)) {
                    cursor_row = rows.len();
                }
                rows.push(ch);
            }
            // [SPg]/[HPg] page breaks in reveal codes
            if l.break_after == Some(PageBreak::Soft) && !hrt_spg {
                rows.push(vec![Tok { text: "[SPg]".into(), st: th.code, idx: None }]);
            }
        }
        // choose window so cursor row is visible (roughly centred like WP)
        let start = if cursor_row >= height / 2 { (cursor_row - height / 2).min(rows.len().saturating_sub(height)) } else { 0 };
        for (r, toks) in rows.iter().skip(start).take(height).enumerate() {
            let mut c = 0usize;
            for t in toks {
                let mut st = t.st;
                let is_cur = t.idx == Some(e.cursor);
                if is_cur {
                    st = th.normal.reverse();
                }
                let c0 = c;
                c = g.text(c, top + r, &t.text, st);
                if let Some(i) = t.idx {
                    self.hit(top + r, c0, c, HitAction::Cursor(i));
                }
            }
        }
    }
}

/// The name F10 offers.  A document opened from an old or foreign format
/// (BRIEF.WP5, LETTER.DOC, a WP file without extension …) is offered as a
/// new .docx next to it, so the original file is never overwritten with a
/// Word document under its old name.
pub fn save_label(path: Option<&std::path::Path>) -> String {
    let Some(p) = path else { return String::new() };
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    const WRITTEN: [&str; 12] = ["docx", "txt", "asc", "wps", "rtf", "odt", "ott", "fodt", "htm", "html", "md", "markdown"];
    if WRITTEN.contains(&ext.as_str()) {
        p.display().to_string()
    } else {
        p.with_extension("docx").display().to_string()
    }
}

#[cfg(test)]
mod save_label_tests {
    use std::path::Path;
    #[test]
    fn old_formats_become_docx() {
        assert_eq!(super::save_label(Some(Path::new("C:\\x\\BRIEF.WP5"))), "C:\\x\\BRIEF.docx");
        assert_eq!(super::save_label(Some(Path::new("LETTER.DOC"))), "LETTER.docx");
        assert_eq!(super::save_label(Some(Path::new("BRIEF"))), "BRIEF.docx");
        assert_eq!(super::save_label(Some(Path::new("Vertrag.docx"))), "Vertrag.docx");
        assert_eq!(super::save_label(Some(Path::new("notes.txt"))), "notes.txt");
    }
}
