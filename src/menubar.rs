//! Alt-= : WP 5.1's pull-down menu bar
//! (`File Edit Search Layout Mark Tools Font Graphics Help`).
//!
//! Every item stands for the keystrokes that do the same thing (exactly
//! like WP 5.1, where the pull-downs only feed the function-key menus), so
//! the menus never get out of step with the keyboard.  Mouse: click on the
//! bar or an item; the right mouse button opens the bar.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::ui::{HitAction, Outcome, Screen, ScreenKind, Wp};

/// When an item can be chosen (others are shown in `[ ]`).
#[derive(Clone, Copy, PartialEq)]
enum When {
    Always,
    Block,
    NoBlock,
}

enum Act {
    /// Feed these inputs (as if typed).
    Keys(&'static [Input]),
    /// A submenu.
    Sub(&'static [Mi]),
    /// Call a function.
    Call(fn(&mut Wp)),
    /// A separator line.
    Sep,
}

struct Mi {
    /// Label with `~` before the mnemonic letter.
    label: &'static str,
    /// Shortcut key shown on the right.
    key: &'static str,
    act: Act,
    when: When,
}

const fn it(label: &'static str, key: &'static str, keys: &'static [Input]) -> Mi {
    Mi { label, key, act: Act::Keys(keys), when: When::Always }
}
const fn blk(label: &'static str, key: &'static str, keys: &'static [Input]) -> Mi {
    Mi { label, key, act: Act::Keys(keys), when: When::Block }
}
const fn nob(label: &'static str, key: &'static str, keys: &'static [Input]) -> Mi {
    Mi { label, key, act: Act::Keys(keys), when: When::NoBlock }
}
const fn sub(label: &'static str, key: &'static str, items: &'static [Mi]) -> Mi {
    Mi { label, key, act: Act::Sub(items), when: When::Always }
}
const fn call(label: &'static str, key: &'static str, f: fn(&mut Wp)) -> Mi {
    Mi { label, key, act: Act::Call(f), when: When::Always }
}
const SEP: Mi = Mi { label: "", key: "", act: Act::Sep, when: When::Always };

const fn f(x: Func) -> Input {
    Input::Func(x)
}
const fn c(x: char) -> Input {
    Input::Char(x)
}

use Func as F;

// ------------------------------------------------------------------ File

const TEXT_IN: &[Mi] = &[
    it("DOS ~Text (CR/LF to HRt)", "", &[f(F::TextInOut), c('1'), c('2')]),
    it("DOS Text (CR/LF to ~SRt)", "", &[f(F::TextInOut), c('1'), c('3')]),
    it("S~preadsheet", "", &[f(F::TextInOut), c('5')]),
];
const TEXT_OUT: &[Mi] = &[
    it("DOS ~Text", "", &[f(F::TextInOut), c('1'), c('1')]),
    it("~Word (.docx)", "", &[f(F::TextInOut), c('3'), c('1')]),
    it("~PDF", "", &[f(F::TextInOut), c('3'), c('2')]),
    it("~Generic (plain text)", "", &[f(F::TextInOut), c('3'), c('3')]),
];
const PASSWORD: &[Mi] = &[
    it("~Add/Change", "", &[f(F::TextInOut), c('2'), c('1')]),
    it("~Remove", "", &[f(F::TextInOut), c('2'), c('2')]),
];
const SETUP: &[Mi] = &[
    it("~Mouse", "", &[f(F::Setup), c('1')]),
    it("~Display", "", &[f(F::Setup), c('2')]),
    it("~Environment", "", &[f(F::Setup), c('3')]),
    it("~Initial Settings", "", &[f(F::Setup), c('4')]),
    it("~Keyboard Layout", "", &[f(F::Setup), c('5')]),
    it("~Location of Files", "", &[f(F::Setup), c('6')]),
];
const FILE: &[Mi] = &[
    it("~Retrieve", "Shft-F10", &[f(F::Retrieve)]),
    it("~Save", "F10", &[f(F::Save)]),
    SEP,
    sub("Text ~In", "Ctrl-F5", TEXT_IN),
    sub("Text ~Out", "Ctrl-F5", TEXT_OUT),
    SEP,
    sub("Pass~word", "Ctrl-F5", PASSWORD),
    it("Re~visions", "Ctrl-F5", &[f(F::TextInOut), c('6')]),
    it("List ~Files", "F5", &[f(F::List)]),
    it("Su~mmary", "", &[f(F::Format), c('3'), c('5')]),
    SEP,
    it("~Print", "Shft-F7", &[f(F::Print)]),
    sub("Se~tup", "Shft-F1", SETUP),
    SEP,
    it("~Go to Shell", "Ctrl-F1", &[f(F::Shell)]),
    it("E~xit", "F7", &[f(F::Exit)]),
];

// ------------------------------------------------------------------ Edit

const APPEND: &[Mi] = &[blk("To ~File", "", &[f(F::Move), c('1'), c('4')])];
const SELECT: &[Mi] = &[
    nob("~Sentence", "", &[f(F::Move), c('1')]),
    nob("~Paragraph", "", &[f(F::Move), c('2')]),
    nob("P~age", "", &[f(F::Move), c('3')]),
];
const COMMENT: &[Mi] = &[
    it("~Create", "", &[f(F::TextInOut), c('4'), c('1')]),
    it("~Edit", "", &[f(F::TextInOut), c('4'), c('2')]),
    it("Convert to ~Text", "", &[f(F::TextInOut), c('4'), c('3')]),
];
const CASE: &[Mi] = &[blk("To ~Upper", "", &[f(F::Switch), c('1')]), blk("To ~Lower", "", &[f(F::Switch), c('2')])];
const EDIT: &[Mi] = &[
    blk("~Move (Cut)", "", &[f(F::Move), c('1'), c('1')]),
    blk("~Copy", "", &[f(F::Move), c('1'), c('2')]),
    nob("~Paste", "", &[f(F::Move), c('4'), c('1')]),
    sub("~Append", "", APPEND),
    blk("~Delete", "Del", &[f(F::Del)]),
    Mi { label: "~Undelete", key: "F1", act: Act::Keys(&[f(F::Cancel)]), when: When::NoBlock },
    it("U~ndo", "Ctrl-Z", &[f(F::CtrlLetter('Z'))]),
    SEP,
    it("~Block", "Alt-F4", &[f(F::Block)]),
    sub("S~elect", "", SELECT),
    SEP,
    sub("C~omment", "Ctrl-F5", COMMENT),
    sub("Con~vert Case", "Shft-F3", CASE),
    SEP,
    it("~Switch Document", "Shft-F3", &[f(F::Switch)]),
    it("~Window", "Ctrl-F3", &[f(F::Screen), c('1')]),
    it("~Reveal Codes", "Alt-F3", &[f(F::RevealCodes)]),
];

// ---------------------------------------------------------------- Search

const EXTENDED: &[Mi] = &[
    it("~Forward", "Home,F2", &[f(F::Home), f(F::SearchFwd)]),
    it("~Backward", "Home,Shft-F2", &[f(F::Home), f(F::SearchBack)]),
    call("~Next", "", |wp| search_again(wp, true)),
    call("~Previous", "", |wp| search_again(wp, false)),
    it("~Replace", "Home,Alt-F2", &[f(F::Home), f(F::Replace)]),
];
const SEARCH: &[Mi] = &[
    it("~Forward", "F2", &[f(F::SearchFwd)]),
    it("~Backward", "Shft-F2", &[f(F::SearchBack)]),
    call("~Next", "", |wp| search_again(wp, true)),
    call("~Previous", "", |wp| search_again(wp, false)),
    it("~Replace", "Alt-F2", &[f(F::Replace)]),
    sub("~Extended", "", EXTENDED),
    SEP,
    it("~Go to", "Ctrl-Home", &[f(F::GoTo)]),
];

fn search_again(wp: &mut Wp, forward: bool) {
    if wp.last_search.is_empty() {
        wp.input(Input::Func(if forward { Func::SearchFwd } else { Func::SearchBack }));
    } else {
        wp.do_search(forward);
        wp.after_edit();
    }
}

// ---------------------------------------------------------------- Layout

const COLUMNS: &[Mi] = &[
    it("~On", "", &[f(F::ColumnsTable), c('1'), c('1')]),
    it("O~ff", "", &[f(F::ColumnsTable), c('1'), c('2')]),
    it("~Define", "", &[f(F::ColumnsTable), c('1'), c('3')]),
];
const TABLES: &[Mi] = &[
    it("~Create", "", &[f(F::ColumnsTable), c('2'), c('1')]),
    it("~Edit", "", &[f(F::ColumnsTable), c('2'), c('2')]),
];
const MATH: &[Mi] = &[
    it("~On", "", &[f(F::ColumnsTable), c('3'), c('1')]),
    it("O~ff", "", &[f(F::ColumnsTable), c('3'), c('2')]),
    it("~Define", "", &[f(F::ColumnsTable), c('3'), c('3')]),
    it("~Calculate", "", &[f(F::ColumnsTable), c('3'), c('4')]),
];
const FOOTNOTE: &[Mi] = &[
    it("~Create", "", &[f(F::Footnote), c('1'), c('1')]),
    it("~Edit", "", &[f(F::Footnote), c('1'), c('2')]),
    it("~New Number", "", &[f(F::Footnote), c('1'), c('3')]),
    it("~Options", "", &[f(F::Footnote), c('1'), c('4')]),
];
const ENDNOTE: &[Mi] = &[
    it("~Create", "", &[f(F::Footnote), c('2'), c('1')]),
    it("~Edit", "", &[f(F::Footnote), c('2'), c('2')]),
    it("~New Number", "", &[f(F::Footnote), c('2'), c('3')]),
    it("~Options", "", &[f(F::Footnote), c('2'), c('4')]),
    it("~Placement", "", &[f(F::Footnote), c('3')]),
];
const JUSTIFY: &[Mi] = &[
    it("~Left", "", &[f(F::Format), c('1'), c('3'), c('1'), f(F::Exit)]),
    it("~Center", "", &[f(F::Format), c('1'), c('3'), c('2'), f(F::Exit)]),
    it("~Right", "", &[f(F::Format), c('1'), c('3'), c('3'), f(F::Exit)]),
    it("~Full", "", &[f(F::Format), c('1'), c('3'), c('4'), f(F::Exit)]),
];
const ALIGN: &[Mi] = &[
    it("~Indent ->", "F4", &[f(F::Indent)]),
    it("I~ndent -><-", "Shft-F4", &[f(F::DblIndent)]),
    it("~Margin Rel <-", "Shft-Tab", &[f(F::MarRel)]),
    it("~Center", "Shft-F6", &[f(F::Center)]),
    it("~Flush Right", "Alt-F6", &[f(F::FlushRight)]),
    it("~Tab Align", "Ctrl-F6", &[f(F::TabAlign)]),
    it("Hard ~Page", "Ctrl-Enter", &[f(F::HardPage)]),
];
const LAYOUT: &[Mi] = &[
    it("~Line", "Shft-F8", &[f(F::Format), c('1')]),
    it("~Page", "Shft-F8", &[f(F::Format), c('2')]),
    it("~Document", "Shft-F8", &[f(F::Format), c('3')]),
    it("~Other", "Shft-F8", &[f(F::Format), c('4')]),
    SEP,
    sub("~Columns", "Alt-F7", COLUMNS),
    sub("~Tables", "Alt-F7", TABLES),
    sub("~Math", "Alt-F7", MATH),
    SEP,
    sub("~Footnote", "Ctrl-F7", FOOTNOTE),
    sub("~Endnote", "Ctrl-F7", ENDNOTE),
    SEP,
    sub("~Justify", "", JUSTIFY),
    sub("~Align", "", ALIGN),
    SEP,
    it("~Styles", "Alt-F8", &[f(F::Style)]),
];

// ------------------------------------------------------------------ Mark

const XREF: &[Mi] = &[
    it("~Reference", "", &[f(F::MarkText), c('1'), c('1')]),
    it("~Target", "", &[f(F::MarkText), c('1'), c('2')]),
    it("~Both", "", &[f(F::MarkText), c('1'), c('3')]),
];
const TOA: &[Mi] = &[
    blk("Mark ~Short", "", &[f(F::MarkText), c('4')]),
    blk("Mark ~Full", "", &[f(F::MarkText), c('4')]),
    it("~Edit Full", "", &[f(F::MarkText), c('4')]),
];
const MASTER: &[Mi] = &[
    it("~Expand", "", &[f(F::MarkText), c('6'), c('3')]),
    it("~Condense", "", &[f(F::MarkText), c('6'), c('4')]),
];
const COMPARE: &[Mi] = &[
    it("~Compare with Disk Document", "", &[f(F::MarkText), c('7'), c('7')]),
    it("~Accept All Changes", "", &[f(F::MarkText), c('7'), c('5')]),
    it("~Reject All Changes", "", &[f(F::MarkText), c('7'), c('6')]),
];
const MARK: &[Mi] = &[
    it("~Index", "Alt-F5", &[f(F::MarkText), c('3')]),
    blk("Table of ~Contents", "", &[f(F::MarkText), c('1')]),
    blk("~List", "", &[f(F::MarkText), c('2')]),
    sub("Cross-~Reference", "Alt-F5", XREF),
    sub("Table of ~Authorities", "", TOA),
    SEP,
    it("~Define", "Alt-F5", &[f(F::MarkText), c('5')]),
    it("~Generate", "", &[f(F::MarkText), c('6')]),
    SEP,
    sub("~Master Documents", "", MASTER),
    it("~Subdocument", "Alt-F5", &[f(F::MarkText), c('2')]),
    SEP,
    it("~Track Changes", "Alt-F5", &[f(F::MarkText), c('7')]),
    sub("Document C~ompare", "", COMPARE),
];

// ----------------------------------------------------------------- Tools

const MACRO: &[Mi] = &[it("~Define", "Ctrl-F10", &[f(F::MacroDefine)]), it("E~xecute", "Alt-F10", &[f(F::Macro)])];
const OUTLINE: &[Mi] = &[
    it("~On", "", &[f(F::DateOutline), c('4'), c('1')]),
    it("O~ff", "", &[f(F::DateOutline), c('4'), c('2')]),
    it("~Move Family", "", &[f(F::DateOutline), c('4'), c('3')]),
    it("~Copy Family", "", &[f(F::DateOutline), c('4'), c('4')]),
    it("~Delete Family", "", &[f(F::DateOutline), c('4'), c('5')]),
];
const MERGE_CODES: &[Mi] = &[
    it("~Field", "", &[f(F::MergeCodes), c('1')]),
    it("~End Record", "", &[f(F::MergeCodes), c('2')]),
    it("~Input", "", &[f(F::MergeCodes), c('3')]),
    it("~Page Off", "", &[f(F::MergeCodes), c('4')]),
    it("~Next Record", "", &[f(F::MergeCodes), c('5')]),
    it("~More", "", &[f(F::MergeCodes), c('6')]),
];
const TOOLS: &[Mi] = &[
    it("Sp~ell", "Ctrl-F2", &[f(F::Spell)]),
    it("T~hesaurus", "Alt-F1", &[f(F::Thesaurus)]),
    SEP,
    sub("M~acro", "", MACRO),
    SEP,
    nob("Date ~Text", "Shft-F5", &[f(F::DateOutline), c('1')]),
    nob("Date ~Code", "Shft-F5", &[f(F::DateOutline), c('2')]),
    nob("Date ~Format", "Shft-F5", &[f(F::DateOutline), c('3')]),
    SEP,
    sub("~Outline", "Shft-F5", OUTLINE),
    nob("~Paragraph Number", "Shft-F5", &[f(F::DateOutline), c('5')]),
    nob("~Define", "Shft-F5", &[f(F::DateOutline), c('6')]),
    SEP,
    sub("Me~rge Codes", "Shft-F9", MERGE_CODES),
    it("~Merge", "Ctrl-F9", &[f(F::MergeSort), c('1')]),
    it("~Sort", "Ctrl-F9", &[f(F::MergeSort), c('2')]),
    SEP,
    it("~Line Draw", "Ctrl-F3", &[f(F::Screen), c('2')]),
];

// ------------------------------------------------------------------ Font

const APPEARANCE: &[Mi] = &[
    it("~Bold", "F6", &[f(F::Font), c('2'), c('1')]),
    it("~Underline", "F8", &[f(F::Font), c('2'), c('2')]),
    it("~Double Underline", "", &[f(F::Font), c('2'), c('3')]),
    it("~Italics", "", &[f(F::Font), c('2'), c('4')]),
    it("~Outline", "", &[f(F::Font), c('2'), c('5')]),
    it("Sh~adow", "", &[f(F::Font), c('2'), c('6')]),
    it("Small ~Cap", "", &[f(F::Font), c('2'), c('7')]),
    it("~Redline", "", &[f(F::Font), c('2'), c('8')]),
    it("~Strikeout", "", &[f(F::Font), c('2'), c('9')]),
];
const FONT: &[Mi] = &[
    nob("Base F~ont", "Ctrl-F8", &[f(F::Font), c('4')]),
    SEP,
    nob("~Normal", "Ctrl-F8", &[f(F::Font), c('3')]),
    sub("~Appearance", "Ctrl-F8", APPEARANCE),
    SEP,
    it("Su~perscript", "", &[f(F::Font), c('1'), c('1')]),
    it("Su~bscript", "", &[f(F::Font), c('1'), c('2')]),
    SEP,
    it("~Fine", "", &[f(F::Font), c('1'), c('3')]),
    it("~Small", "", &[f(F::Font), c('1'), c('4')]),
    it("~Large", "", &[f(F::Font), c('1'), c('5')]),
    it("~Very Large", "", &[f(F::Font), c('1'), c('6')]),
    it("~Extra Large", "", &[f(F::Font), c('1'), c('7')]),
    SEP,
    nob("Print ~Color", "Ctrl-F8", &[f(F::Font), c('5')]),
    it("C~haracters", "Ctrl-V", &[f(F::Compose)]),
];

// -------------------------------------------------------------- Graphics

macro_rules! box_menu {
    ($name:ident, $n:literal) => {
        const $name: &[Mi] = &[
            it("~Create", "", &[f(F::Graphics), c($n), c('1')]),
            it("~Edit", "", &[f(F::Graphics), c($n), c('2')]),
            it("~New Number", "", &[f(F::Graphics), c($n), c('3')]),
            it("~Options", "", &[f(F::Graphics), c($n), c('4')]),
        ];
    };
}
box_menu!(FIGURE, '1');
box_menu!(TABLE_BOX, '2');
box_menu!(TEXT_BOX, '3');
box_menu!(USER_BOX, '4');
const EQUATION: &[Mi] = &[
    it("~Create", "", &[f(F::Graphics), c('6'), c('1')]),
    it("~Edit", "", &[f(F::Graphics), c('6'), c('2')]),
    it("~New Number", "", &[f(F::Graphics), c('6'), c('3')]),
    it("~Options", "", &[f(F::Graphics), c('6'), c('4')]),
];
const GLINE: &[Mi] = &[
    it("Create ~Horizontal", "", &[f(F::Graphics), c('5'), c('1')]),
    it("Create ~Vertical", "", &[f(F::Graphics), c('5'), c('2')]),
    it("Edit H~orizontal", "", &[f(F::Graphics), c('5'), c('3')]),
    it("Edit V~ertical", "", &[f(F::Graphics), c('5'), c('4')]),
];
const GRAPHICS: &[Mi] = &[
    sub("~Figure", "Alt-F9", FIGURE),
    sub("~Table Box", "Alt-F9", TABLE_BOX),
    sub("Text ~Box", "Alt-F9", TEXT_BOX),
    sub("~User Box", "Alt-F9", USER_BOX),
    sub("~Equation", "Alt-F9", EQUATION),
    sub("~Line", "Alt-F9", GLINE),
];

// ------------------------------------------------------------------ Help

const HELP: &[Mi] = &[
    it("~Help", "F3", &[f(F::Help)]),
    call("~Index", "", |wp| crate::help::open_index(wp, 'A')),
    call("~Template", "", crate::help::open_template),
];

const BAR: [(&str, &[Mi]); 9] = [
    ("~File", FILE),
    ("~Edit", EDIT),
    ("~Search", SEARCH),
    ("~Layout", LAYOUT),
    ("~Mark", MARK),
    ("~Tools", TOOLS),
    ("F~ont", FONT),
    ("~Graphics", GRAPHICS),
    ("~Help", HELP),
];

/// Screen columns of the bar entries (start, end).
fn bar_cols() -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut col = 1;
    for (label, _) in BAR {
        let w = width(label);
        v.push((col, col + w));
        col += w + 2;
    }
    v
}

/// The bar entry under a screen column of the top row.
pub fn item_at(col: usize) -> Option<usize> {
    bar_cols().iter().position(|&(a, b)| col + 1 >= a && col < b + 1)
}

/// Rows the visible menu bar takes above the document.
pub fn bar_rows(wp: &Wp) -> usize {
    if wp.settings.menu_bar_visible { 1 + usize::from(wp.settings.menu_bar_separator) } else { 0 }
}

fn mnemonic(label: &str) -> Option<char> {
    let mut it = label.chars();
    while let Some(c) = it.next() {
        if c == '~' {
            return it.next().map(|c| c.to_ascii_lowercase());
        }
    }
    None
}

impl Wp {
    /// Setup → Display → Menu Options → Menu Letter Display.
    pub fn mnemonic_style(&self) -> Style {
        letter_style(self, self.theme.normal)
    }
}

fn letter_style(wp: &Wp, base: Style) -> Style {
    match wp.settings.menu_letters.as_str() {
        "underline" => base.flags(F_UNDER),
        "reverse" => base.reverse(),
        "none" => base,
        _ => {
            if base == wp.theme.normal {
                wp.theme.bold
            } else {
                // a highlighted (reverse) background: a contrasting colour
                let light = base.bg == pal::WHITE || base.bg == pal::LGRAY || base.bg == pal::CYAN || base.bg == pal::LCYAN;
                Style { fg: if light { pal::RED } else { pal::WHITE }, ..base }.flags(F_BOLD)
            }
        }
    }
}

fn bar_style(wp: &Wp) -> Style {
    wp.theme.normal.reverse()
}

/// The shortcut as it is on the active keyboard (Alternate: F1 Help,
/// Esc Cancel).
fn shown_key(wp: &Wp, k: &'static str) -> &'static str {
    if wp.keyboard.is_none() && wp.alternate_keys {
        match k {
            "F1" => return "Esc",
            "F3" => return "F1",
            _ => {}
        }
    }
    k
}

/// Draw the bar on the top row; `sel` = highlighted entry.
pub fn draw_bar(wp: &Wp, g: &mut Grid, sel: Option<usize>) {
    let st = bar_style(wp);
    g.fill_row(0, 0, g.cols, ' ', st);
    for (k, ((label, _), (c0, c1))) in BAR.iter().zip(bar_cols()).enumerate() {
        let s = if sel == Some(k) { wp.theme.normal } else { st };
        g.put(c0 - 1, 0, ' ', s);
        g.text_mn(c0, 0, label, s, letter_style(wp, s));
        g.put(c1, 0, ' ', s);
        if sel.is_some() {
            wp.hit(0, c0 - 1, c1 + 1, HitAction::Select(BAR_HIT + k));
        }
    }
    if wp.settings.menu_bar_separator && g.rows > 1 {
        g.fill_row(1, 0, g.cols, '─', wp.theme.normal);
    }
}

const BAR_HIT: usize = 10_000;
const LEVEL_HIT: usize = 1_000;

/// Open the menu bar (Alt-=); `pull` = open this pull-down at once.
pub fn open(wp: &mut Wp, pull: Option<usize>) {
    let mut m = MenuBar { top: pull.unwrap_or(0), path: Vec::new() };
    if pull.is_some() {
        m.path.push(first_item(BAR[m.top].1));
    }
    wp.push(Box::new(m));
}

fn first_item(items: &[Mi]) -> usize {
    items.iter().position(|i| !matches!(i.act, Act::Sep)).unwrap_or(0)
}

pub struct MenuBar {
    top: usize,
    /// Selected item in the open pull-down (path[0]) and its submenus.
    path: Vec<usize>,
}

impl MenuBar {
    /// Items of menu level `lvl` (0 = the pull-down).
    fn items(&self, lvl: usize) -> &'static [Mi] {
        let mut items = BAR[self.top].1;
        for &k in &self.path[..lvl] {
            if let Act::Sub(s) = items[k].act {
                items = s;
            }
        }
        items
    }

    fn enabled(wp: &Wp, m: &Mi) -> bool {
        let block = wp.ed_ref().block_anchor.is_some();
        match m.when {
            When::Always => true,
            When::Block => block,
            When::NoBlock => !block,
        }
    }

    fn step(&mut self, delta: isize) {
        let Some(lvl) = self.path.len().checked_sub(1) else { return };
        let items = self.items(lvl);
        let n = items.len() as isize;
        let mut k = self.path[lvl] as isize;
        for _ in 0..n {
            k = (k + delta).rem_euclid(n);
            if !matches!(items[k as usize].act, Act::Sep) {
                break;
            }
        }
        self.path[lvl] = k as usize;
    }

    fn switch_top(&mut self, delta: isize) {
        let open = !self.path.is_empty();
        self.top = (self.top as isize + delta).rem_euclid(BAR.len() as isize) as usize;
        self.path.clear();
        if open {
            self.path.push(first_item(BAR[self.top].1));
        }
    }

    /// Choose the selected item of the deepest level.
    fn choose(&mut self, wp: &mut Wp) -> Outcome {
        let Some(lvl) = self.path.len().checked_sub(1) else {
            self.path.push(first_item(BAR[self.top].1));
            return Outcome::Keep;
        };
        let m = &self.items(lvl)[self.path[lvl]];
        if !Self::enabled(wp, m) {
            return Outcome::Keep;
        }
        match m.act {
            Act::Sub(s) => {
                self.path.push(first_item(s));
                Outcome::Keep
            }
            Act::Sep => Outcome::Keep,
            Act::Keys(keys) => {
                for k in keys {
                    wp.input(k.clone());
                }
                Outcome::Close
            }
            Act::Call(func) => {
                func(wp);
                Outcome::Close
            }
        }
    }

    /// Mnemonic letter at the current level.
    fn letter(&mut self, wp: &mut Wp, ch: char) -> Outcome {
        let lc = ch.to_ascii_lowercase();
        match self.path.len().checked_sub(1) {
            None => {
                if let Some(k) = BAR.iter().position(|(l, _)| mnemonic(l) == Some(lc)) {
                    self.top = k;
                    self.path.push(first_item(BAR[k].1));
                }
                Outcome::Keep
            }
            Some(lvl) => {
                let items = self.items(lvl);
                let pos = items.iter().position(|m| mnemonic(m.label) == Some(lc)).or_else(|| {
                    // numbers choose by position too
                    let d = ch.to_digit(10)? as usize;
                    items.iter().enumerate().filter(|(_, m)| !matches!(m.act, Act::Sep)).nth(d.checked_sub(1)?).map(|x| x.0)
                });
                match pos {
                    Some(k) => {
                        self.path[lvl] = k;
                        self.choose(wp)
                    }
                    None => Outcome::Keep,
                }
            }
        }
    }

    /// Left edge column of the box for level `lvl`, and its width.
    fn geometry(&self, cols: usize) -> Vec<(usize, usize, usize)> {
        // (x, y, inner width) per level
        let mut out = Vec::new();
        let (x0, _) = bar_cols()[self.top];
        let mut x = x0.saturating_sub(1);
        let mut y = 1usize;
        for lvl in 0..self.path.len() {
            let items = self.items(lvl);
            let w = items.iter().map(|m| width(m.label) + if m.key.is_empty() { 0 } else { width(m.key) + 2 } + 3).max().unwrap_or(10).max(8);
            if x + w + 2 > cols {
                x = cols.saturating_sub(w + 2);
            }
            out.push((x, y, w));
            if lvl + 1 < self.path.len() {
                // the submenu opens beside the chosen item
                let sel = self.path[lvl];
                y += sel;
                x += w + 2;
            }
        }
        out
    }
}

impl Screen for MenuBar {
    fn kind(&self) -> ScreenKind {
        ScreenKind::Menu(0x806B)
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::MenuBar) | Input::Func(Func::Exit) => Outcome::Close,
            Input::Func(Func::Cancel) | Input::Func(Func::Esc) => {
                if self.path.pop().is_some() { Outcome::Keep } else { Outcome::Close }
            }
            Input::Func(Func::Left) => {
                if self.path.len() > 1 {
                    self.path.pop();
                } else {
                    self.switch_top(-1);
                }
                Outcome::Keep
            }
            Input::Func(Func::Right) => {
                if let Some(lvl) = self.path.len().checked_sub(1) {
                    if let Act::Sub(s) = self.items(lvl)[self.path[lvl]].act {
                        self.path.push(first_item(s));
                        return Outcome::Keep;
                    }
                }
                self.switch_top(1);
                Outcome::Keep
            }
            Input::Func(Func::Down) => {
                if self.path.is_empty() {
                    self.path.push(first_item(BAR[self.top].1));
                } else {
                    self.step(1);
                }
                Outcome::Keep
            }
            Input::Func(Func::Up) => {
                if !self.path.is_empty() {
                    self.step(-1);
                }
                Outcome::Keep
            }
            Input::Func(Func::Home) | Input::Func(Func::PgUp) => {
                if let Some(lvl) = self.path.len().checked_sub(1) {
                    self.path[lvl] = first_item(self.items(lvl));
                }
                Outcome::Keep
            }
            Input::Func(Func::End) | Input::Func(Func::PgDn) => {
                if let Some(lvl) = self.path.len().checked_sub(1) {
                    let n = self.items(lvl).len();
                    self.path[lvl] = n - 1;
                }
                Outcome::Keep
            }
            Input::Func(Func::Enter) | Input::Char(' ') => self.choose(wp),
            Input::Char(c) => self.letter(wp, *c),
            Input::Func(Func::AltLetter(c)) => {
                // Alt + letter on the bar opens that pull-down
                self.path.clear();
                self.letter(wp, *c)
            }
            _ => Outcome::Keep,
        }
    }

    fn mouse_select(&mut self, wp: &mut Wp, n: usize, _double: bool) -> Outcome {
        if n >= BAR_HIT {
            let k = n - BAR_HIT;
            if k == self.top && !self.path.is_empty() {
                self.path.clear();
            } else {
                self.top = k;
                self.path = vec![first_item(BAR[k].1)];
            }
            return Outcome::Keep;
        }
        let (lvl, k) = (n / LEVEL_HIT, n % LEVEL_HIT);
        if lvl < self.path.len() {
            self.path.truncate(lvl + 1);
            self.path[lvl] = k;
            return self.choose(wp);
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        draw_bar(wp, g, Some(self.top));
        let st = wp.theme.normal;
        let sel_st = bar_style(wp);
        for (lvl, (x, y, w)) in self.geometry(g.cols).into_iter().enumerate() {
            let items = self.items(lvl);
            let y1 = y + items.len() + 1;
            if y1 >= g.rows {
                continue;
            }
            for yy in y..=y1 {
                g.fill_row(yy, x, x + w + 2, ' ', st);
            }
            g.frame(x, y, x + w + 1, y1, st, false);
            for (k, m) in items.iter().enumerate() {
                let row = y + 1 + k;
                if let Act::Sep = m.act {
                    g.put(x, row, '├', st);
                    g.fill_row(row, x + 1, x + w + 1, '─', st);
                    g.put(x + w + 1, row, '┤', st);
                    continue;
                }
                let selected = self.path.get(lvl) == Some(&k);
                let s = if selected { sel_st } else { st };
                g.fill_row(row, x + 1, x + w + 1, ' ', s);
                let on = MenuBar::enabled(wp, m);
                if on {
                    g.text_mn(x + 2, row, m.label, s, letter_style(wp, s));
                } else {
                    let plain: String = m.label.chars().filter(|&c| c != '~').collect();
                    g.text(x + 1, row, &format!("[{plain}]"), s);
                }
                let key = shown_key(wp, m.key);
                let right = if let Act::Sub(_) = m.act { format!("{key} \u{25BA}") } else { key.to_string() };
                let right = right.trim_start();
                if !right.is_empty() {
                    g.text(x + w + 1 - width(right) - 1, row, right, s);
                }
                wp.hit(row, x + 1, x + w + 1, HitAction::Select(lvl * LEVEL_HIT + k));
            }
        }
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', st);
        let help = if self.path.is_empty() {
            "Press a highlighted letter or Enter to open a menu; arrows move; Cancel leaves."
        } else {
            "Arrows move; Enter or the highlighted letter chooses; Cancel goes back."
        };
        g.text(0, row, help, st);
        g.cursor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;

    fn screen(wp: &Wp) -> String {
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>().trim_end().to_string() + "\n").collect()
    }

    #[test]
    fn bar_and_pulldown() {
        let mut wp = Wp::new(80, 25);
        wp.input(Input::Func(Func::MenuBar));
        let s = screen(&wp);
        assert!(s.lines().next().unwrap().contains("File  Edit  Search  Layout  Mark  Tools  Font  Graphics  Help"), "{s}");
        wp.input(Input::Char('f'));
        let s = screen(&wp);
        assert!(s.contains("Retrieve") && s.contains("Shft-F10") && s.contains("Go to Shell"), "{s}");
        // Layout → Justify → Center inserts [Just:Center]
        wp.input(Input::Func(Func::Right));
        wp.input(Input::Func(Func::Right));
        wp.input(Input::Func(Func::Right));
        wp.input(Input::Char('j'));
        wp.input(Input::Char('c'));
        assert!(!wp.has_screens(), "menus closed");
        assert!(wp.ed_ref().doc.els.iter().any(|e| matches!(e, crate::doc::El::Code(crate::doc::Code::Just(crate::doc::Justify::Center)))));
    }

    #[test]
    fn mouse_and_cancel() {
        let mut wp = Wp::new(80, 25);
        for ch in "abc".chars() {
            wp.input(Input::Char(ch));
        }
        // right button opens the bar, a click on "Font" its pull-down
        wp.mouse(crate::ui::MouseKind::Right, 10, 10);
        let _ = screen(&wp);
        let (c0, _) = bar_cols()[6];
        wp.mouse(crate::ui::MouseKind::Down, c0, 0);
        let s = screen(&wp);
        assert!(s.contains("Superscript") && s.contains("Extra Large"), "{s}");
        wp.input(Input::Func(Func::Cancel));
        wp.input(Input::Func(Func::Cancel));
        assert!(!wp.has_screens());
    }

    #[test]
    fn visible_bar_moves_document_down() {
        let mut wp = Wp::new(80, 25);
        wp.settings.menu_bar_visible = true;
        for ch in "Hello".chars() {
            wp.input(Input::Char(ch));
        }
        wp.after_edit();
        let s = screen(&wp);
        let lines: Vec<&str> = s.lines().collect();
        assert!(lines[0].contains("File"), "{s}");
        assert!(lines[1].trim_start().starts_with("Hello"), "{s}");
        assert!(lines[24].contains("Doc 1"), "{s}");
    }
}
