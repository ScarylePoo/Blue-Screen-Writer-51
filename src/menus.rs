//! Function-key menus: Font, Format, Setup, Date, Help, Compose, Print…

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};
use std::rc::Rc;

pub fn not_yet(wp: &mut Wp, what: &str) {
    wp.flash(&format!("{what} — not available yet in this version"));
}

// ================================================================== Font

pub fn font_menu(wp: &mut Wp) {
    let block = wp.ed_ref().block_anchor.is_some();
    let items: &[&str] = if block {
        &["~Size", "~Appearance"]
    } else {
        &["~Size", "~Appearance", "~Normal", "~Base Font", "Print ~Color"]
    };
    wp.push(MenuLine::new(items, move |wp, k| match k {
        0 => Some(size_menu()),
        1 => Some(appearance_menu()),
        2 => {
            normal_attrs(wp);
            None
        }
        3 => Some(base_font_screen(wp)),
        _ => {
            not_yet(wp, "Print Color");
            None
        }
    }));
}

fn apply_attr(wp: &mut Wp, a: Attr) {
    if wp.ed_ref().block_anchor.is_some() {
        wp.ed().attr_block(a);
    } else {
        wp.ed().toggle_attr(a);
    }
    wp.after_edit();
}

fn size_menu() -> Box<dyn Screen> {
    let mut m = MenuLine::new(
        &["Sup~rscpt", "~Subscpt", "~Fine", "S~mall", "~Large", "~Vry Large", "~Ext Large"],
        |wp, k| {
            let a = [Attr::Superscript, Attr::Subscript, Attr::Fine, Attr::Small, Attr::Large, Attr::VeryLarge, Attr::ExtraLarge][k];
            apply_attr(wp, a);
            None
        },
    );
    m.prefix = String::new();
    m
}

fn appearance_menu() -> Box<dyn Screen> {
    MenuLine::new(
        &["~Bold", "~Undln", "~Dbl Und", "~Italc", "~Outln", "Shado~w", "Sm ~Cap", "~Redln", "~Stkout"],
        |wp, k| {
            let a = [
                Attr::Bold,
                Attr::Underline,
                Attr::DblUnderline,
                Attr::Italic,
                Attr::Outline,
                Attr::Shadow,
                Attr::SmallCaps,
                Attr::Redline,
                Attr::Strikeout,
            ][k];
            apply_attr(wp, a);
            None
        },
    )
}

/// Ctrl-F8, 3 Normal: move past every off code / end all attributes.
fn normal_attrs(wp: &mut Wp) {
    let e = wp.ed();
    loop {
        match e.doc.els.get(e.cursor) {
            Some(El::Code(Code::AttrOff(_))) => e.cursor += 1,
            _ => break,
        }
    }
    let on = e.doc.attrs_at(e.cursor);
    for a in on.iter() {
        e.insert_code(Code::AttrOff(a));
    }
    // and re-open them after the cursor so following text keeps formatting
    let c = e.cursor;
    let mut idx = c;
    for a in on.iter() {
        e.doc.els.insert(idx, El::Code(Code::AttrOn(a)));
        idx += 1;
    }
    e.relayout();
}

fn base_font_screen(wp: &mut Wp) -> Box<dyn Screen> {
    let cur = wp.ed_ref().format_state_at(wp.ed_ref().cursor).font;
    Box::new(crate::fonts::FontList::new(cur))
}

// ================================================================ Format

type ValFn = Rc<dyn Fn(&Wp) -> Vec<String>>;
type ApplyFn = Rc<dyn Fn(&mut Wp, Vec<String>)>;

pub enum Edit {
    /// Values typed in place (one per sub-row).
    Fields(ApplyFn),
    YesNo(Rc<dyn Fn(&mut Wp, bool)>),
    Choice(Vec<&'static str>, Rc<dyn Fn(&mut Wp, usize)>),
    Sub(Rc<dyn Fn(&mut Wp) -> Box<dyn Screen>>),
    /// Leave the menus and do something in the document (e.g. open the
    /// Initial Codes editing screen).
    Action(Rc<dyn Fn(&mut Wp)>),
    None,
}

pub struct Item {
    pub key: char,
    pub labels: Vec<&'static str>,
    pub value: ValFn,
    pub edit: Edit,
}

pub struct Form {
    pub title: String,
    pub items: Vec<Item>,
    /// (item, current sub-field, buffers)
    editing: Option<(usize, usize, Vec<String>, bool)>,
    choice: Option<usize>,
    pub footer: String,
    /// Screen column of the values (WP: 44 on most Format screens).
    pub value_col: usize,
}

impl Form {
    pub fn new(title: &str, items: Vec<Item>) -> Box<Form> {
        Box::new(Form { title: title.into(), items, editing: None, choice: None, footer: "Selection: 0".into(), value_col: 44 })
    }

    fn rows_of(&self) -> Vec<(usize, usize)> {
        // (item, sub) per screen row
        let mut v = vec![];
        for (i, it) in self.items.iter().enumerate() {
            for s in 0..it.labels.len().max(1) {
                v.push((i, s));
            }
        }
        v
    }
}

impl Screen for Form {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::Menu(0)
    }
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if let Some(ci) = self.choice {
            if let Edit::Choice(opts, f) = &self.items[ci].edit {
                let n = opts.len();
                match i {
                    Input::Char(c) if c.is_ascii_digit() => {
                        let d = c.to_digit(10).unwrap() as usize;
                        if d >= 1 && d <= n {
                            let f = f.clone();
                            f(wp, d - 1);
                            wp.after_edit();
                        }
                        self.choice = None;
                    }
                    Input::Char(c) => {
                        let lc = c.to_ascii_lowercase();
                        if let Some(k) = opts.iter().position(|o| o.to_ascii_lowercase().starts_with(lc)) {
                            let f = f.clone();
                            f(wp, k);
                            wp.after_edit();
                        }
                        self.choice = None;
                    }
                    _ => self.choice = None,
                }
            }
            return Outcome::Keep;
        }
        if let Some((it, sub, bufs, fresh)) = &mut self.editing {
            match i {
                Input::Char(c) => {
                    if *fresh {
                        bufs[*sub].clear();
                        *fresh = false;
                    }
                    bufs[*sub].push(*c);
                }
                Input::Func(Func::Backspace) => {
                    *fresh = false;
                    bufs[*sub].pop();
                }
                Input::Func(Func::Enter) | Input::Func(Func::Down) | Input::Func(Func::Tab) => {
                    if *sub + 1 < bufs.len() {
                        *sub += 1;
                        *fresh = true;
                    } else {
                        let item = &self.items[*it];
                        if let Edit::Fields(f) = &item.edit {
                            let f = f.clone();
                            let vals = bufs.clone();
                            self.editing = None;
                            f(wp, vals);
                            wp.after_edit();
                        }
                        self.editing = None;
                    }
                }
                Input::Func(Func::Up) => {
                    if *sub > 0 {
                        *sub -= 1;
                        *fresh = true;
                    }
                }
                Input::Func(Func::Cancel) => self.editing = None,
                Input::Func(Func::Exit) => {
                    // F7 while typing a value accepts it and returns to the
                    // menu (a second F7 then leaves the menus).
                    let item = &self.items[*it];
                    if let Edit::Fields(f) = &item.edit {
                        let f = f.clone();
                        let vals = bufs.clone();
                        self.editing = None;
                        f(wp, vals);
                        wp.after_edit();
                    }
                    self.editing = None;
                }
                _ => {}
            }
            return Outcome::Keep;
        }
        match i {
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Enter) | Input::Char('0') | Input::Char(' ') => {
                if matches!(i, Input::Func(Func::Exit)) {
                    // F7 returns all the way to the document
                    return Outcome::CloseAll;
                }
                Outcome::Close
            }
            Input::Char(c) => {
                let lc = c.to_ascii_lowercase();
                let idx = self.items.iter().position(|it| it.key.to_ascii_lowercase() == lc).or_else(|| {
                    // mnemonic: first letter of label
                    self.items.iter().position(|it| it.labels.first().and_then(|l| l.chars().find(|c| c.is_alphabetic())).map(|c| c.to_ascii_lowercase()) == Some(lc))
                });
                if let Some(k) = idx {
                    match &self.items[k].edit {
                        Edit::Fields(_) => {
                            let vals = (self.items[k].value)(wp);
                            self.editing = Some((k, 0, vals, true));
                        }
                        Edit::YesNo(f) => {
                            let f = f.clone();
                            return Outcome::Push(YesNo::new(&self.items[k].labels[0].trim_end_matches(':').replace('~', ""), false, move |wp, y| {
                                f(wp, y);
                                wp.after_edit();
                                None
                            }));
                        }
                        Edit::Choice(_, _) => self.choice = Some(k),
                        Edit::Sub(f) => {
                            let s = f(wp);
                            return Outcome::Push(s);
                        }
                        Edit::Action(f) => {
                            let f = f.clone();
                            wp.screens.clear();
                            f(wp);
                            return Outcome::Close;
                        }
                        Edit::None => {}
                    }
                }
                Outcome::Keep
            }
            _ => Outcome::Keep,
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, &self.title, st);
        let mut row = 2usize;
        let mut cursor = None;
        for (k, it) in self.items.iter().enumerate() {
            let vals = (it.value)(wp);
            for (s, label) in it.labels.iter().enumerate() {
                if s == 0 {
                    g.text(5, row, &format!("{} - ", it.key), st);
                    let end = g.text_mn(9, row, label, st, hi);
                    wp.hit(row, 5, end.max(60), crate::ui::HitAction::Input(Input::Char(it.key)));
                } else {
                    g.text_mn(9, row, label, st, hi);
                }
                let mut v = vals.get(s).cloned().unwrap_or_default();
                if let Some((ei, es, bufs, _)) = &self.editing {
                    if *ei == k {
                        v = bufs[s].clone();
                        if *es == s {
                            cursor = Some((self.value_col + v.chars().count(), row));
                        }
                    }
                }
                g.text(self.value_col, row, &v, st);
                row += 1;
            }
            if row + 2 < g.rows && it.labels.len() == 1 {
                row += 1;
            } else {
                row += 1;
            }
        }
        let fr = g.rows - 1;
        if let Some(ci) = self.choice {
            if let Edit::Choice(opts, _) = &self.items[ci].edit {
                let mut c = 0;
                for (n, o) in opts.iter().enumerate() {
                    c = g.text(c, fr, &format!("{} ", n + 1), st);
                    c = g.text(c, fr, o, st);
                    if n + 1 < opts.len() {
                        c = g.text(c, fr, "; ", st);
                    }
                }
                let c = g.text(c, fr, ": 0", st);
                g.cursor = Some((c - 1, fr));
            }
        } else {
            let c = g.text(0, fr, &self.footer, st);
            g.cursor = cursor.or(Some((c - 1, fr)));
        }
    }
}


pub fn format_menu(wp: &mut Wp) {
    if crate::fmtmenu::block_protect(wp) {
        return;
    }
    let items = vec![
        Item { key: '1', labels: vec!["~Line"], value: Rc::new(|_| vec![]), edit: Edit::Sub(Rc::new(|_| format_line())) },
        Item { key: '2', labels: vec!["~Page"], value: Rc::new(|_| vec![]), edit: Edit::Sub(Rc::new(|_| format_page())) },
        Item { key: '3', labels: vec!["~Document"], value: Rc::new(|_| vec![]), edit: Edit::Sub(Rc::new(|_| format_document())) },
        Item { key: '4', labels: vec!["~Other"], value: Rc::new(|_| vec![]), edit: Edit::Sub(Rc::new(|_| format_other())) },
    ];
    let mut f = Form::new("Format", items);
    f.footer = "Selection: 0".into();
    wp.push(Box::new(FormatOverview { form: f }));
}

/// The Format main screen shows a summary of each group under the items.
struct FormatOverview {
    form: Box<Form>,
}

impl Screen for FormatOverview {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::Menu(155)
    }
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match self.form.input(wp, i) {
            Outcome::Push(s) => Outcome::Push(s),
            o => o,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Format", st);
        let groups: [(&str, &str, [&str; 8]); 4] = [
            ("1", "~Line", [
                "Hyphenation", "Line Spacing", "Justification", "Margins Left/Right", "Line Height", "Tab Set", "Line Numbering",
                "Widow/Orphan Protection",
            ]),
            ("2", "~Page", [
                "Center Page (top to bottom)", "Page Numbering", "Force Odd/Even Page", "Paper Size/Type", "Headers", "Suppress", "Footers",
                "Margins Top/Bottom",
            ]),
            ("3", "~Document", ["Display Pitch", "Redline Method", "Initial Codes/Font", "Summary", "", "", "", ""]),
            ("4", "~Other", [
                "Advance", "Overstrike", "Conditional End of Page", "Printer Functions", "Decimal Characters", "Underline Spaces/Tabs",
                "Language", "Border Options",
            ]),
        ];
        let mut row = 2;
        for (k, name, subs) in groups {
            g.text(5, row, &format!("{k} - "), st);
            let end = g.text_mn(9, row, name, st, hi);
            wp.hit(row, 5, end.max(40), crate::ui::HitAction::Input(Input::Char(k.chars().next().unwrap())));
            row += 1;
            row += 1;
            for pair in subs.chunks(2) {
                if pair[0].is_empty() {
                    break;
                }
                g.text(10, row, pair[0], st);
                g.text(43, row, pair[1], st);
                row += 1;
            }
            row += 1;
            if row >= g.rows - 1 {
                break;
            }
        }
        let fr = g.rows - 1;
        let c = g.text(0, fr, "Selection: 0", st);
        g.cursor = Some((c - 1, fr));
    }
}


fn format_line() -> Box<dyn Screen> {
    crate::fmtmenu::format_line()
}

fn format_page() -> Box<dyn Screen> {
    crate::fmtmenu::format_page()
}

fn format_document() -> Box<dyn Screen> {
    crate::fmtmenu::format_document()
}

pub(crate) fn summary_form() -> Box<dyn Screen> {
    let field = |key: char, label: &'static str, get: fn(&Summary) -> &String, set: fn(&mut Summary, String)| Item {
        key,
        labels: vec![label],
        value: Rc::new(move |wp: &Wp| vec![get(&wp.ed_ref().doc.summary).clone()]),
        edit: Edit::Fields(Rc::new(move |wp: &mut Wp, v: Vec<String>| {
            let e = wp.ed();
            set(&mut e.doc.summary, v[0].clone());
            e.modified = true;
        })),
    };
    let items = vec![
        field('1', "~Descriptive Name", |s| &s.descriptive_name, |s, v| s.descriptive_name = v),
        field('2', "~Subject/Account", |s| &s.subject, |s, v| s.subject = v),
        field('3', "~Author", |s| &s.author, |s, v| s.author = v),
        field('4', "~Typist", |s| &s.typist, |s, v| s.typist = v),
        field('5', "~Comments", |s| &s.comments, |s, v| s.comments = v),
    ];
    Form::new("Document Summary", items)
}

fn format_other() -> Box<dyn Screen> {
    crate::fmtmenu::format_other()
}

// ================================================================= Setup

pub fn setup_menu(wp: &mut Wp) {
    crate::setup::setup_menu(wp);
}

// ================================================================== Date

pub const MONTHS_EN: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const MONTHS_DE: [&str; 12] =
    ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
pub const DAYS_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
pub const DAYS_DE: [&str; 7] = ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"];

/// Expand a WP 5.1 date format string.
/// 1 day, 2 month no., 3 month word, 4 year, 5 yy, 6 weekday,
/// 7 hour(24), 8 hour(12), 9 minute, 0 am/pm; `%` = leading zero,
/// `$` = abbreviated (3 letters).
pub fn format_date(fmt: &str, german: bool) -> String {
    let (y, mo, d, h, mi, wd) = crate::sys::now_local();
    let months = if german { MONTHS_DE } else { MONTHS_EN };
    let days = if german { DAYS_DE } else { DAYS_EN };
    let mut out = String::new();
    let mut pad = false;
    let mut abbr = false;
    for c in fmt.chars() {
        let num = |v: u32, pad: bool| if pad { format!("{v:02}") } else { format!("{v}") };
        let word = |w: &str, abbr: bool| if abbr { w.chars().take(3).collect() } else { w.to_string() };
        match c {
            '%' => {
                pad = true;
                continue;
            }
            '$' => {
                abbr = true;
                continue;
            }
            '1' => out.push_str(&num(d, pad)),
            '2' => out.push_str(&num(mo, pad)),
            '3' => out.push_str(&word(months[(mo - 1) as usize], abbr)),
            '4' => out.push_str(&y.to_string()),
            '5' => out.push_str(&format!("{:02}", y % 100)),
            '6' => out.push_str(&word(days[wd as usize], abbr)),
            '7' => out.push_str(&num(h, pad)),
            '8' => out.push_str(&num(if h % 12 == 0 { 12 } else { h % 12 }, pad)),
            '9' => out.push_str(&format!("{mi:02}")),
            '0' => out.push_str(if h < 12 { "am" } else { "pm" }),
            c => out.push(c),
        }
        pad = false;
        abbr = false;
    }
    out
}

pub fn date_menu(wp: &mut Wp) {
    wp.push(MenuLine::new(
        &["Date ~Text", "Date ~Code", "Date ~Format", "~Outline", "~Para Num", "~Define"],
        |wp, k| match k {
            0 => {
                let s = format_date(&wp.date_format(), crate::fmtmenu::german_dates(wp));
                for c in s.chars() {
                    wp.ed().insert_char(c);
                }
                wp.after_edit();
                None
            }
            1 => {
                let f = wp.date_format();
                wp.ed().insert_code(Code::DateCode(f));
                wp.after_edit();
                None
            }
            2 => {
                let cur = wp.date_format();
                Some(Prompt::new("Date Format:", &cur, |wp, s| {
                    if let Some(s) = s {
                        wp.set_date_format(&s);
                    }
                    None
                }).keep_text() as Box<dyn Screen>)
            }
            3 => Some(crate::styles_ui::outline_menu()),
            4 => Some(crate::styles_ui::para_num_prompt()),
            _ => Some(crate::styles_ui::define_screen(wp)),
        },
    ));
}

// =============================================================== Compose

pub struct Compose {
    first: Option<char>,
}

pub fn compose() -> Box<dyn Screen> {
    Box::new(Compose { first: None })
}

/// Digraphs for Compose (Ctrl-2 / Ctrl-V), WP 5.1 style: two characters
/// in either order.
pub fn compose_pair(a: char, b: char) -> Option<char> {
    let table: &[(&str, char)] = &[
        ("a\"", 'ä'), ("o\"", 'ö'), ("u\"", 'ü'), ("A\"", 'Ä'), ("O\"", 'Ö'), ("U\"", 'Ü'), ("e\"", 'ë'), ("i\"", 'ï'), ("y\"", 'ÿ'),
        ("E\"", 'Ë'), ("I\"", 'Ï'), ("ss", 'ß'), ("a'", 'á'), ("e'", 'é'), ("i'", 'í'), ("o'", 'ó'), ("u'", 'ú'), ("y'", 'ý'),
        ("A'", 'Á'), ("E'", 'É'), ("I'", 'Í'), ("O'", 'Ó'), ("U'", 'Ú'), ("a`", 'à'), ("e`", 'è'), ("i`", 'ì'), ("o`", 'ò'),
        ("u`", 'ù'), ("A`", 'À'), ("E`", 'È'), ("I`", 'Ì'), ("O`", 'Ò'), ("U`", 'Ù'), ("a^", 'â'), ("e^", 'ê'), ("i^", 'î'),
        ("o^", 'ô'), ("u^", 'û'), ("A^", 'Â'), ("E^", 'Ê'), ("I^", 'Î'), ("O^", 'Ô'), ("U^", 'Û'), ("n~", 'ñ'), ("N~", 'Ñ'),
        ("a~", 'ã'), ("o~", 'õ'), ("A~", 'Ã'), ("O~", 'Õ'), ("c,", 'ç'), ("C,", 'Ç'), ("ae", 'æ'), ("AE", 'Æ'), ("oe", 'œ'),
        ("OE", 'Œ'), ("o/", 'ø'), ("O/", 'Ø'), ("ao", 'å'), ("AO", 'Å'), ("!!", '¡'), ("??", '¿'), ("<<", '«'), (">>", '»'),
        ("--", '–'), ("-=", '—'), ("c/", '¢'), ("L-", '£'), ("Y=", '¥'), ("E=", '€'), ("co", '©'), ("ro", '®'), ("tm", '™'),
        ("so", '§'), ("P|", '¶'), ("+-", '±'), ("12", '½'), ("14", '¼'), ("34", '¾'), ("*.", '•'), ("..", '…'), ("^o", '°'),
        ("x*", '×'), (":-", '÷'), ("<=", '≤'), (">=", '≥'), ("=/", '≠'), (",,", '„'), ("''", '\u{201d}'), ("``", '\u{201c}'),
        ("'<", '\u{2018}'), ("'>", '\u{2019}'), ("**", '●'), ("*o", '◦'), ("*O", '○'), ("/2", '½'), ("/4", '¼'),
    ];
    for (k, v) in table {
        let mut it = k.chars();
        let (x, y) = (it.next().unwrap(), it.next().unwrap());
        if (x == a && y == b) || (x == b && y == a) {
            return Some(*v);
        }
    }
    None
}

impl Screen for Compose {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char(c) => {
                if let Some(a) = self.first {
                    // "set,number" form handled via digits
                    if let Some(ch) = compose_pair(a, *c) {
                        wp.ed().insert_char(ch);
                        wp.after_edit();
                    }
                    return Outcome::Close;
                }
                if c.is_ascii_digit() {
                    // WP character set number: "1,23"
                    let first = *c;
                    return Outcome::Replace(Prompt::new("Key =", &first.to_string(), |wp, s| {
                        if let Some(s) = s {
                            if let Some((set, n)) = s.split_once(',') {
                                if let (Ok(set), Ok(n)) = (set.trim().parse::<u8>(), n.trim().parse::<u16>()) {
                                    if let Some(ch) = crate::wpchars::wp_to_unicode(set, n) {
                                        wp.ed().insert_char(ch);
                                        wp.after_edit();
                                    }
                                }
                            }
                        }
                        None
                    }).keep_text());
                }
                self.first = Some(*c);
                Outcome::Keep
            }
            _ => Outcome::Close,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let mut c = g.text(0, row, "Key = ", wp.theme.normal);
        if let Some(a) = self.first {
            c = g.text(c, row, &a.to_string(), wp.theme.normal);
        }
        g.cursor = Some((c, row));
    }
}

// ================================================================== Help

pub fn help(wp: &mut Wp) {
    crate::help::open(wp);
}

// ================================================================= Print

pub fn print_menu(wp: &mut Wp) {
    crate::print::print_menu(wp);
}

pub fn print_file(wp: &mut Wp, p: &std::path::Path) {
    crate::print::print_file(wp, p);
}

// ================================================================ Screen

pub fn screen_menu(wp: &mut Wp) {
    wp.ed().relayout();
    wp.push(
        MenuLine::new(&["~Window", "~Line Draw", "~Rewrite"], |wp, k| {
        match k {
            0 => {
                let cur = wp.doc_rows();
                return Some(Prompt::new("Number of lines in this window:", &cur.to_string(), |wp, s| {
                    if let Some(n) = s.and_then(|s| s.trim().parse::<usize>().ok()) {
                        wp.set_window(n);
                    }
                    None
                }) as Box<dyn Screen>);
            }
            1 => return Some(Box::new(crate::linedraw::LineDraw::new()) as Box<dyn Screen>),
            _ => wp.ed().relayout(),
        }
        None
    })
        .func_pick(Func::Screen, 2),
    );
}

// ============================================================ Text In/Out

pub fn text_in_out(wp: &mut Wp) {
    if wp.ed_ref().block_anchor.is_some() {
        wp.push(YesNo::new("Create a comment?", false, |wp, yes| {
            if yes {
                if let Some(els) = wp.ed_ref().block_els() {
                    let mut d = Document::new();
                    d.els = els;
                    let text = d.plain_text(0, d.len());
                    wp.ed().delete_block();
                    wp.ed().insert_code(Code::Comment(text));
                    wp.after_edit();
                }
            }
            None
        }));
        return;
    }
    wp.push(MenuLine::new(&["~Dos Text", "~Password", "Save ~As", "~Comment", "~Spreadsheet", "~Revisions", "Document ~Info", "~Link/Bookmark"], |wp, k| match k {
        5 => {
            crate::revisions::open(wp);
            None
        }
        1 => Some(crate::password::menu()),
        0 => Some(MenuLine::new(&["~Save", "~Retrieve (CR/LF to [HRt])", "Retrieve (CR/LF to [SRt] in HZone)"], |wp, k| {
            if k == 0 {
                let label = wp.doc_label();
                let label = std::path::Path::new(&label).with_extension("txt").display().to_string();
                Some(Prompt::new("Document to be saved (DOS Text):", &label, |wp, s| {
                    let s = s?;
                    let p = wp.resolve_path(&s);
                    let d = wp.ed_ref().doc.plain_text(0, wp.ed_ref().doc.len()).replace('\n', "\r\n");
                    if let Err(e) = std::fs::write(&p, d) {
                        wp.flash(&format!("ERROR: {e}"));
                    }
                    None
                }) as Box<dyn Screen>)
            } else {
                Some(Prompt::new("Document to be retrieved:", "", |wp, s| {
                    let s = s?;
                    let p = wp.resolve_path(&s);
                    match std::fs::read(&p) {
                        Ok(b) => {
                            let t = crate::fileio::decode_text(&b).replace("\r\n", "\n");
                            let d = Document::from_text(&t);
                            wp.ed().insert_els(&d.els);
                        }
                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                    }
                    None
                }) as Box<dyn Screen>)
            }
        }) as Box<dyn Screen>),
        2 => Some(crate::docinfo::save_as_menu()),
        6 => {
            crate::docinfo::open(wp);
            None
        }
        7 => Some(crate::docinfo::link_menu()),
        3 => Some(crate::comments::menu()),
        _ => {
            not_yet(wp, "This Text In/Out option");
            None
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compose_works() {
        assert_eq!(compose_pair('a', '"'), Some('ä'));
        assert_eq!(compose_pair('"', 'a'), Some('ä'));
        assert_eq!(compose_pair('s', 's'), Some('ß'));
    }
}
