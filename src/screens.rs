//! Generic WP-style interaction pieces: the status-line prompt, the
//! `Yes (No)` question and the one-line numbered menu.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::ui::{Outcome, Screen, ScreenKind, Wp};

pub type Next = Option<Box<dyn Screen>>;

// ----------------------------------------------------------------- prompt

pub struct Prompt {
    pub label: String,
    pub text: Vec<char>,
    pub pos: usize,
    pub first_key: bool,
    pub on_done: Option<Box<dyn FnOnce(&mut Wp, Option<String>) -> Next>>,
    /// If true the first typed character replaces the suggested text
    /// (WP behaviour for prompts like "Document to be saved:").
    pub replace_on_type: bool,
    /// Search/replace prompts: function keys insert codes (`[HRt]`,
    /// `[Tab]`, `[BOLD]` …) and Esc / F2 start the search.
    pub codes: bool,
    /// "Define macro:" — an Alt-letter answers `ALTx` at once.
    pub alt_letters: bool,
}

impl Prompt {
    pub fn new(label: &str, initial: &str, f: impl FnOnce(&mut Wp, Option<String>) -> Next + 'static) -> Box<Prompt> {
        let text: Vec<char> = initial.chars().collect();
        Box::new(Prompt {
            label: label.to_string(),
            pos: text.len(),
            text,
            first_key: true,
            on_done: Some(Box::new(f)),
            replace_on_type: true,
            codes: false,
            alt_letters: false,
        })
    }
    pub fn with_codes(mut self: Box<Self>) -> Box<Self> {
        self.codes = true;
        self
    }
    pub fn with_alt_letters(mut self: Box<Self>) -> Box<Self> {
        self.alt_letters = true;
        self
    }

    /// The code text a function key types into a search prompt.
    fn code_for(&self, f: Func) -> Option<String> {
        let text: String = self.text.iter().collect();
        let toggle = |on: &str, off: &str| {
            if text.matches(on).count() > text.matches(off).count() { off.to_string() } else { on.to_string() }
        };
        Some(match f {
            Func::Enter => "[HRt]".into(),
            Func::Tab => "[Tab]".into(),
            Func::HardPage => "[HPg]".into(),
            Func::Indent => "[\u{2192}Indent]".into(),
            Func::DblIndent => "[\u{2192}Indent\u{2190}]".into(),
            Func::MarRel => "[\u{2190}Mar Rel]".into(),
            Func::Center => "[Center]".into(),
            Func::FlushRight => "[Flsh Rgt]".into(),
            Func::HardSpace => "[ ]".into(),
            Func::Bold => toggle("[BOLD]", "[bold]"),
            Func::Underline => toggle("[UND]", "[und]"),
            _ => return None,
        })
    }

    fn finish(&mut self, wp: &mut Wp, ok: bool) -> Outcome {
        let s: String = self.text.iter().collect();
        let cb = self.on_done.take().unwrap();
        match cb(wp, if ok { Some(s) } else { None }) {
            Some(n) => Outcome::Replace(n),
            None => Outcome::Close,
        }
    }
    pub fn keep_text(mut self: Box<Self>) -> Box<Self> {
        self.replace_on_type = false;
        self
    }
}

impl Screen for Prompt {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Prompt
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let first = std::mem::replace(&mut self.first_key, false);
        match i {
            Input::Char(c) => {
                if first && self.replace_on_type {
                    self.text.clear();
                    self.pos = 0;
                }
                self.text.insert(self.pos, *c);
                self.pos += 1;
            }
            Input::Func(Func::AltLetter(c)) if self.alt_letters => {
                self.text = format!("ALT{}", c.to_ascii_uppercase()).chars().collect();
                return self.finish(wp, true);
            }
            Input::Func(Func::SearchFwd | Func::SearchBack | Func::Esc) if self.codes => return self.finish(wp, true),
            Input::Func(f) if self.codes && self.code_for(*f).is_some() => {
                if first && self.replace_on_type {
                    self.text.clear();
                    self.pos = 0;
                }
                for c in self.code_for(*f).unwrap().chars() {
                    self.text.insert(self.pos, c);
                    self.pos += 1;
                }
            }
            Input::Func(f) => match f {
                Func::Enter | Func::SearchFwd => {
                    let s: String = self.text.iter().collect();
                    let cb = self.on_done.take().unwrap();
                    return match cb(wp, Some(s)) {
                        Some(n) => Outcome::Replace(n),
                        None => Outcome::Close,
                    };
                }
                Func::Cancel | Func::Esc => {
                    let cb = self.on_done.take().unwrap();
                    return match cb(wp, None) {
                        Some(n) => Outcome::Replace(n),
                        None => Outcome::Close,
                    };
                }
                Func::Left => self.pos = self.pos.saturating_sub(1),
                Func::Right => self.pos = (self.pos + 1).min(self.text.len()),
                Func::Home => self.pos = 0,
                Func::End => self.pos = self.text.len(),
                Func::Backspace => {
                    if self.pos > 0 {
                        self.pos -= 1;
                        self.text.remove(self.pos);
                    }
                }
                Func::Del => {
                    if self.pos < self.text.len() {
                        self.text.remove(self.pos);
                    }
                }
                Func::DelEol => self.text.truncate(self.pos),
                Func::WordLeft => {
                    while self.pos > 0 && self.text[self.pos - 1] == ' ' {
                        self.pos -= 1
                    }
                    while self.pos > 0 && self.text[self.pos - 1] != ' ' {
                        self.pos -= 1
                    }
                }
                Func::WordRight => {
                    while self.pos < self.text.len() && self.text[self.pos] != ' ' {
                        self.pos += 1
                    }
                    while self.pos < self.text.len() && self.text[self.pos] == ' ' {
                        self.pos += 1
                    }
                }
                _ => {}
            },
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        let c = g.text(0, row, &self.label, st);
        let c = if self.label.ends_with(' ') || self.label.is_empty() { c } else { g.text(c, row, " ", st) };
        // horizontal scroll if long
        let avail = g.cols.saturating_sub(c + 1);
        let start = if self.pos > avail { self.pos - avail } else { 0 };
        let s: String = self.text[start..].iter().take(avail).collect();
        g.text(c, row, &s, st);
        g.cursor = Some((c + self.pos - start, row));
    }
}

// ---------------------------------------------------------------- yes/no

pub struct YesNo {
    pub question: String,
    pub default_yes: bool,
    pub on_done: Option<Box<dyn FnOnce(&mut Wp, bool) -> Next>>,
    pub on_cancel: Option<Box<dyn FnOnce(&mut Wp) -> Next>>,
}

impl YesNo {
    pub fn new(question: &str, default_yes: bool, f: impl FnOnce(&mut Wp, bool) -> Next + 'static) -> Box<YesNo> {
        Box::new(YesNo { question: question.into(), default_yes, on_done: Some(Box::new(f)), on_cancel: None })
    }
    pub fn on_cancel(mut self: Box<Self>, f: impl FnOnce(&mut Wp) -> Next + 'static) -> Box<Self> {
        self.on_cancel = Some(Box::new(f));
        self
    }
}

impl Screen for YesNo {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Question
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let ans = match i {
            Input::Char(c) => match c.to_ascii_lowercase() {
                'y' | 'j' => Some(true),
                'n' => Some(false),
                _ => None,
            },
            Input::Func(Func::Enter) => Some(self.default_yes),
            Input::Func(Func::Cancel) | Input::Func(Func::Esc) => {
                if let Some(cb) = self.on_cancel.take() {
                    return match cb(wp) {
                        Some(n) => Outcome::Replace(n),
                        None => Outcome::Close,
                    };
                }
                return Outcome::Close;
            }
            // Any other key = the default ("No" for "No (Yes)" questions)
            Input::Func(Func::Exit) => None,
            _ => Some(self.default_yes),
        };
        match ans {
            Some(a) => {
                let cb = self.on_done.take().unwrap();
                match cb(wp, a) {
                    Some(n) => Outcome::Replace(n),
                    None => Outcome::Close,
                }
            }
            None => Outcome::Keep,
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        let c = g.text(0, row, &self.question, st);
        let c = g.text(c, row, " ", st);
        let (a, b) = if self.default_yes { ("Yes", "No") } else { ("No", "Yes") };
        let c2 = g.text(c, row, a, wp.theme.bold);
        let c3 = g.text(c2, row, &format!(" ({b})"), st);
        let key = |s: &str| crate::ui::HitAction::Input(Input::Char(if s == "Yes" { 'y' } else { 'n' }));
        wp.hit(row, c, c2, key(a));
        wp.hit(row, c2 + 1, c3, key(b));
        g.cursor = Some((c, row));
    }
}

// ------------------------------------------------------------ menu line

/// A one-line WP menu such as
/// `1 Size; 2 Appearance; 3 Normal; 4 Base Font; 5 Print Color: 0`.
/// Items are given as strings with `~` before the mnemonic letter.
pub struct MenuLine {
    pub prefix: String,
    pub items: Vec<String>,
    pub default: String,
    pub on_pick: Option<Box<dyn FnOnce(&mut Wp, usize) -> Next>>,
    pub on_cancel: Option<Box<dyn FnOnce(&mut Wp) -> Next>>,
    /// Pressing this function key chooses item n (Ctrl-F3 twice = Rewrite).
    pub func_pick: Option<(Func, usize)>,
}

impl MenuLine {
    pub fn func_pick(mut self: Box<Self>, f: Func, n: usize) -> Box<Self> {
        self.func_pick = Some((f, n));
        self
    }

    pub fn new(items: &[&str], f: impl FnOnce(&mut Wp, usize) -> Next + 'static) -> Box<MenuLine> {
        Box::new(MenuLine {
            prefix: String::new(),
            items: items.iter().map(|s| s.to_string()).collect(),
            default: "0".into(),
            on_pick: Some(Box::new(f)),
            on_cancel: None,
            func_pick: None,
        })
    }
    pub fn prefix(mut self: Box<Self>, p: &str) -> Box<Self> {
        self.prefix = p.into();
        self
    }
    pub fn default_choice(mut self: Box<Self>, d: &str) -> Box<Self> {
        self.default = d.into();
        self
    }

    fn mnemonic(item: &str) -> Option<char> {
        let mut it = item.chars();
        while let Some(c) = it.next() {
            if c == '~' {
                return it.next().map(|c| c.to_ascii_lowercase());
            }
        }
        None
    }
}

impl Screen for MenuLine {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Menu(0)
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let pick = match i {
            Input::Char(c) => {
                if let Some(d) = c.to_digit(10) {
                    if d >= 1 && (d as usize) <= self.items.len() {
                        Some(d as usize - 1)
                    } else if d == 0 {
                        None.or(Some(usize::MAX))
                    } else {
                        None
                    }
                } else {
                    let lc = c.to_ascii_lowercase();
                    self.items.iter().position(|it| Self::mnemonic(it) == Some(lc))
                }
            }
            Input::Func(f) if self.func_pick.is_some_and(|(pf, _)| pf == *f) => self.func_pick.map(|p| p.1),
            Input::Func(Func::Enter) | Input::Func(Func::Cancel) | Input::Func(Func::Esc) | Input::Func(Func::Exit) => {
                Some(usize::MAX)
            }
            _ => None,
        };
        match pick {
            Some(usize::MAX) => {
                if let Some(cb) = self.on_cancel.take() {
                    return match cb(wp) {
                        Some(n) => Outcome::Replace(n),
                        None => Outcome::Close,
                    };
                }
                Outcome::Close
            }
            Some(k) => {
                let cb = self.on_pick.take().unwrap();
                match cb(wp, k) {
                    Some(n) => Outcome::Replace(n),
                    None => Outcome::Close,
                }
            }
            None => Outcome::Keep,
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        // a menu longer than the screen is wide continues on a second line
        let len = |it: &String| it.chars().filter(|&c| c != '~').count() + 4;
        let total = self.prefix.chars().count() + self.items.iter().map(len).sum::<usize>() + 3;
        let mut row = if total > g.cols { g.rows - 2 } else { g.rows - 1 };
        let st = wp.theme.normal;
        let hi = wp.mnemonic_style();
        g.fill_row(row, 0, g.cols, ' ', st);
        let mut c = g.text(0, row, &self.prefix, st);
        for (k, it) in self.items.iter().enumerate() {
            if c + len(it) + 4 > g.cols && row + 1 < g.rows {
                row += 1;
                g.fill_row(row, 0, g.cols, ' ', st);
                c = 0;
            }
            let c0 = c;
            c = g.text(c, row, &format!("{} ", k + 1), st);
            c = g.text_mn(c, row, it, st, hi);
            let key = char::from_digit((k + 1) as u32 % 10, 10).unwrap_or('0');
            wp.hit(row, c0, c, crate::ui::HitAction::Input(Input::Char(key)));
            if k + 1 < self.items.len() {
                c = g.text(c, row, "; ", st);
            }
        }
        c = g.text(c, row, ": ", st);
        g.text(c, row, &self.default, st);
        g.cursor = Some((c, row));
    }
}

/// A transient message on the status line (e.g. "* Please wait *").
pub fn message_next(wp: &mut Wp, msg: &str) -> Next {
    wp.flash(msg);
    None
}
