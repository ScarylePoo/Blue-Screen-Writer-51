//! Shift-F8 Format: the Line, Page, Document and Other screens with all
//! their sub-screens (Line Numbering, Page Numbering, Page Number
//! Position, Paper Size/Type, Labels, Suppress, Printer Functions, Border
//! Options), the hyphenation prompt, Block Protect and the Home-key codes
//! (hard tabs, `[ISRt]`, `[/]`).

use crate::doc::*;
use crate::fmt::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::menus::{Edit, Form, Item};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};
use std::cell::RefCell;
use std::rc::Rc;

fn yn(b: bool) -> String {
    if b { "Yes".into() } else { "No".into() }
}

fn is_yes(s: &str) -> bool {
    s.trim().to_lowercase().starts_with(['y', 'j'])
}

fn fs(wp: &Wp) -> crate::editor::FormatState {
    let e = wp.ed_ref();
    e.format_state_at(e.cursor)
}

fn fx(wp: &Wp) -> FmtState {
    let e = wp.ed_ref();
    state_at(&e.doc, e.cursor)
}

fn ins(wp: &mut Wp, f: Fmt) {
    wp.ed().insert_format(Code::Fmt(f));
}

fn item(key: char, label: &'static str, value: impl Fn(&Wp) -> Vec<String> + 'static, edit: Edit) -> Item {
    Item { key, labels: vec![label], value: Rc::new(value), edit }
}

fn item2(key: char, labels: Vec<&'static str>, value: impl Fn(&Wp) -> Vec<String> + 'static, edit: Edit) -> Item {
    Item { key, labels, value: Rc::new(value), edit }
}

fn fields(f: impl Fn(&mut Wp, Vec<String>) + 'static) -> Edit {
    Edit::Fields(Rc::new(f))
}

fn parse_pct(s: &str) -> Option<u32> {
    s.trim().trim_end_matches('%').trim().parse().ok()
}

/// A Form that does something when it is left (F7, Enter, 0) — used for
/// screens whose settings become one code, like Line Numbering.
pub struct FormOnClose {
    form: Box<Form>,
    on_close: Option<Box<dyn FnOnce(&mut Wp)>>,
}

impl FormOnClose {
    pub fn new(form: Box<Form>, f: impl FnOnce(&mut Wp) + 'static) -> Box<FormOnClose> {
        Box::new(FormOnClose { form, on_close: Some(Box::new(f)) })
    }
}

impl Screen for FormOnClose {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let out = self.form.input(wp, i);
        if matches!(out, Outcome::Close | Outcome::CloseAll) && !matches!(i, Input::Func(Func::Cancel)) {
            if let Some(f) = self.on_close.take() {
                f(wp);
                wp.after_edit();
            }
        }
        out
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        self.form.draw(wp, g)
    }
}

// =================================================================== Line

pub fn format_line() -> Box<dyn Screen> {
    let items = vec![
        item('1', "~Hyphenation", |wp| vec![yn(fs(wp).hyph)], Edit::YesNo(Rc::new(|wp, y| wp.ed().insert_format(Code::Hyphenation(y))))),
        item2(
            '2',
            vec!["Hyphenation ~Zone -  Left", "                     Right"],
            |wp| {
                let z = fx(wp).hzone;
                vec![format!("{}%", z.0), format!("{}%", z.1)]
            },
            fields(|wp, v| {
                let z = fx(wp).hzone;
                let l = parse_pct(&v[0]).unwrap_or(z.0 as u32).min(100) as u8;
                let r = parse_pct(&v[1]).unwrap_or(z.1 as u32).min(100) as u8;
                if (l, r) != z {
                    ins(wp, Fmt::HZone { left: l, right: r });
                }
            }),
        ),
        item(
            '3',
            "~Justification",
            |wp| vec![fs(wp).just.name().into()],
            Edit::Choice(
                vec!["Left", "Center", "Right", "Full"],
                Rc::new(|wp, k| {
                    let j = [Justify::Left, Justify::Center, Justify::Right, Justify::Full][k];
                    wp.ed().insert_format(Code::Just(j));
                }),
            ),
        ),
        item(
            '4',
            "Line ~Height",
            |wp| vec![fx(wp).ln_height.map(fmt_inches).unwrap_or_else(|| "Auto".into())],
            Edit::Choice(
                vec!["Auto", "Fixed"],
                Rc::new(|wp, k| {
                    if k == 0 {
                        ins(wp, Fmt::LnHeight(None));
                    } else {
                        let f = fs(wp);
                        let cur = fx(wp).ln_height.unwrap_or((f.font.size_cpt as i32 * 12 / 72).max(1));
                        wp.push(Prompt::new("", &fmt_inches(cur), |wp, s| {
                            if let Some(h) = s.and_then(|s| parse_measure(&s)).filter(|h| *h > 0) {
                                ins(wp, Fmt::LnHeight(Some(h)));
                                wp.after_edit();
                            }
                            None
                        }));
                    }
                }),
            ),
        ),
        item(
            '5',
            "Line ~Numbering",
            |wp| vec![yn(fx(wp).ln_num.is_some())],
            Edit::Sub(Rc::new(|_| {
                YesNo::new("Line Numbering?", false, |wp, y| {
                    if !y {
                        if fx(wp).ln_num.is_some() {
                            ins(wp, Fmt::LnNum(None));
                            wp.after_edit();
                        }
                        return None;
                    }
                    Some(line_numbering(wp))
                })
            })),
        ),
        item(
            '6',
            "Line ~Spacing",
            |wp| {
                let s = fs(wp).spacing;
                vec![if s % 100 == 0 { format!("{}", s / 100) } else { format!("{}", s as f64 / 100.0) }]
            },
            fields(|wp, v| {
                if let Ok(x) = v[0].trim().replace(',', ".").parse::<f64>() {
                    if x > 0.0 && x < 100.0 {
                        wp.ed().insert_format(Code::LineSpacing((x * 100.0).round() as u32));
                    }
                }
            }),
        ),
        item2(
            '7',
            vec!["~Margins - Left", "            Right"],
            |wp| {
                let f = fs(wp);
                vec![fmt_inches(f.left), fmt_inches(f.right)]
            },
            fields(|wp, v| {
                let f = fs(wp);
                let l = parse_measure(&v[0]).unwrap_or(f.left);
                let r = parse_measure(&v[1]).unwrap_or(f.right);
                if (l, r) != (f.left, f.right) {
                    wp.ed().insert_format(Code::LRMargin { left: l, right: r });
                }
            }),
        ),
        item('8', "~Tab Set", |wp| vec![crate::reveal::tabs_text(&fs(wp).tabs)], Edit::Sub(Rc::new(|wp| Box::new(crate::tabset::TabRuler::new(wp))))),
        item('9', "~Widow/Orphan Protection", |wp| vec![yn(fs(wp).widow)], Edit::YesNo(Rc::new(|wp, y| wp.ed().insert_format(Code::WidowOrphan(y))))),
    ];
    Form::new("Format: Line", items)
}

fn line_numbering(wp: &Wp) -> Box<dyn Screen> {
    let st = Rc::new(RefCell::new(fx(wp).ln_num.unwrap_or_default()));
    let s = st.clone();
    let get = move |f: fn(&LineNum) -> String| {
        let s = s.clone();
        move |_: &Wp| vec![f(&s.borrow())]
    };
    let set = |st: &Rc<RefCell<LineNum>>, f: fn(&mut LineNum, &str)| {
        let st = st.clone();
        fields(move |_, v| f(&mut st.borrow_mut(), &v[0]))
    };
    let s1 = st.clone();
    let s5 = st.clone();
    let items = vec![
        item('1', "~Count Blank Lines", get(|n| yn(n.count_blank)), Edit::YesNo(Rc::new(move |_, y| s1.borrow_mut().count_blank = y))),
        item('2', "Number ~Every n Lines, where n is", get(|n| n.every.to_string()), set(&st, |n, v| n.every = v.trim().parse().unwrap_or(n.every).max(1))),
        item('3', "~Position of Number from Left Edge", get(|n| fmt_inches(n.pos)), set(&st, |n, v| n.pos = parse_measure(v).unwrap_or(n.pos).max(0))),
        item('4', "~Starting Number", get(|n| n.start.to_string()), set(&st, |n, v| n.start = v.trim().parse().unwrap_or(n.start))),
        item('5', "~Restart Numbering on Each Page", get(|n| yn(n.restart)), Edit::YesNo(Rc::new(move |_, y| s5.borrow_mut().restart = y))),
    ];
    let st2 = st.clone();
    FormOnClose::new(Form::new("Format: Line Numbering", items), move |wp| {
        let n = st2.borrow().clone();
        ins(wp, Fmt::LnNum(Some(n)));
    })
}

// =================================================================== Page

pub fn format_page() -> Box<dyn Screen> {
    let items = vec![
        item(
            '1',
            "~Center Page (top to bottom)",
            |wp| {
                let e = wp.ed_ref();
                let li = e.layout.line_of(e.cursor);
                let pg = e.layout.lines[li].page;
                let on = e.layout.lines.iter().filter(|l| l.page == pg).any(|l| (l.start..l.end).any(|k| matches!(e.doc.els.get(k), Some(El::Code(Code::CenterPage)))));
                vec![yn(on)]
            },
            Edit::YesNo(Rc::new(|wp, y| {
                if y {
                    wp.ed().insert_format(Code::CenterPage)
                }
            })),
        ),
        item('2', "~Force Odd/Even Page", |_| vec![], Edit::Choice(vec!["Odd", "Even"], Rc::new(|wp, k| ins(wp, Fmt::Force { odd: k == 0 })))),
        item('3', "~Headers", |_| vec![], Edit::Sub(Rc::new(|_| crate::subedit::headfoot_menu(0)))),
        item('4', "F~ooters", |_| vec![], Edit::Sub(Rc::new(|_| crate::subedit::headfoot_menu(2)))),
        item2(
            '5',
            vec!["~Margins - Top", "            Bottom"],
            |wp| {
                let f = fs(wp);
                vec![fmt_inches(f.top), fmt_inches(f.bottom)]
            },
            fields(|wp, v| {
                let f = fs(wp);
                let t = parse_measure(&v[0]).unwrap_or(f.top);
                let b = parse_measure(&v[1]).unwrap_or(f.bottom);
                if (t, b) != (f.top, f.bottom) {
                    wp.ed().insert_format(Code::TBMargin { top: t, bottom: b });
                }
            }),
        ),
        item('6', "~Page Numbering", |_| vec![], Edit::Sub(Rc::new(|_| page_numbering()))),
        item2(
            '7',
            vec!["Paper ~Size", "      Type"],
            |wp| {
                let (w, h, ty) = cur_paper(wp);
                vec![format!("{} x {}", fmt_inches(w), fmt_inches(h)), ty]
            },
            Edit::Sub(Rc::new(|wp| Box::new(PaperList::new(wp)))),
        ),
        item('8', "S~uppress (this page only)", |_| vec![], Edit::Sub(Rc::new(|_| suppress_form()))),
    ];
    Form::new("Format: Page", items)
}

fn cur_paper(wp: &Wp) -> (i32, i32, String) {
    match fx(wp).paper {
        Some(p) => {
            let ty = if p.landscape() { format!("{} - Wide", p.ty) } else { p.ty.clone() };
            (p.w, p.h, ty)
        }
        None => {
            let i = &wp.ed_ref().doc.init;
            (i.page_width, i.page_height, if i.page_width > i.page_height { "Standard - Wide".into() } else { "Standard".into() })
        }
    }
}

fn page_numbering() -> Box<dyn Screen> {
    let items = vec![
        item(
            '1',
            "~New Page Number",
            |wp| {
                let e = wp.ed_ref();
                let infos = page_infos(&e.doc, &e.layout);
                let li = e.layout.line_of(e.cursor);
                let pg = e.layout.lines[li].page as usize;
                vec![infos.get(pg.saturating_sub(1)).map(|i| i.num_text()).unwrap_or_else(|| "1".into())]
            },
            fields(|wp, v| {
                let s = v[0].trim();
                if let Ok(n) = s.parse::<u32>() {
                    wp.ed().insert_format(Code::PageNumber(n.max(1)));
                } else if let Some(n) = parse_roman(s) {
                    ins(wp, Fmt::PgNumRoman(n));
                }
            }),
        ),
        item(
            '2',
            "Page Number ~Style",
            |wp| vec![caret_text(&fx(wp).pgnum_style)],
            fields(|wp, v| {
                let s = v[0].replace("^B", "\u{2}").replace("^b", "\u{2}");
                if !s.is_empty() {
                    ins(wp, Fmt::PgNumStyle(s));
                }
            }),
        ),
        item(
            '3',
            "~Insert Page Number",
            |_| vec![],
            Edit::Action(Rc::new(|wp| {
                let style = fx(wp).pgnum_style;
                wp.ed().insert_code(Code::Fmt(Fmt::InsertPgNum(style)));
                wp.after_edit();
            })),
        ),
        item(
            '4',
            "Page Number ~Position",
            |wp| {
                let e = wp.ed_ref();
                let mut p = 0;
                for el in e.doc.initial_codes.iter().chain(e.doc.els[..e.cursor.min(e.doc.els.len())].iter()) {
                    if let El::Code(Code::PageNumberPos(x)) = el {
                        p = *x;
                    }
                }
                vec![if p == 0 { "No page numbering".into() } else { crate::reveal::page_num_pos_name(p).into() }]
            },
            Edit::Sub(Rc::new(|_| Box::new(PagePos))),
        ),
    ];
    Form::new("Format: Page Numbering", items)
}

/// Format: Page Number Position — the picture of the nine positions.
struct PagePos;

impl Screen for PagePos {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char(c @ '1'..='9') => {
                let d = c.to_digit(10).unwrap() as u8;
                wp.ed().insert_format(Code::PageNumberPos(if d == 9 { 0 } else { d }));
                wp.after_edit();
                Outcome::Close
            }
            Input::Func(Func::Exit) => Outcome::CloseAll,
            Input::Char('0') | Input::Func(Func::Enter | Func::Cancel | Func::Esc) => Outcome::Close,
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        let rows = [
            "Format: Page Number Position",
            "",
            "",
            "      Every Page               Alternating Pages",
            "     ┌───────────┐        ┌───────────┐ ┌───────────┐",
            "     │ 1   2   3 │        │ 4         │ │         4 │",
            "     │           │        │           │ │           │",
            "     │           │        │           │ │           │",
            "     │           │        │ Even      │ │       Odd │",
            "     │           │        │           │ │           │",
            "     │           │        │           │ │           │",
            "     │ 5   6   7 │        │ 8         │ │         8 │",
            "     └───────────┘        └───────────┘ └───────────┘",
            "",
            "",
            "     9 - No Page Numbers",
        ];
        for (r, t) in rows.iter().enumerate() {
            g.text(0, r, t, st);
        }
        for (d, c, r) in [('1', 7, 5), ('2', 11, 5), ('3', 15, 5), ('4', 27, 5), ('5', 7, 11), ('6', 11, 11), ('7', 15, 11), ('8', 27, 11), ('9', 5, 15)] {
            wp.hit(r, c, c + 1, crate::ui::HitAction::Input(Input::Char(d)));
        }
        g.text(0, 18, "Selection: ", st);
        g.text(11, 18, "0", st);
        g.cursor = Some((11, 18));
    }
}

fn suppress_form() -> Box<dyn Screen> {
    let bits = Rc::new(RefCell::new(0u8));
    let opt = |key: char, label: &'static str, mask: u8, bits: &Rc<RefCell<u8>>| {
        let b1 = bits.clone();
        let b2 = bits.clone();
        item(
            key,
            label,
            move |_| vec![yn(*b1.borrow() & mask == mask)],
            Edit::YesNo(Rc::new(move |_, y| {
                let mut b = b2.borrow_mut();
                if y {
                    *b |= mask
                } else {
                    *b &= !mask
                }
            })),
        )
    };
    let all = SUP_PGNUM | 4 | 8 | 0x10 | 0x20;
    let items = vec![
        opt('1', "Suppress ~All Page Numbering, Headers and Footers", all, &bits),
        opt('2', "Suppress ~Headers and Footers", 4 | 8 | 0x10 | 0x20, &bits),
        opt('3', "Print Page Number at ~Bottom Center", SUP_PG_BC, &bits),
        opt('4', "Suppress ~Page Numbering", SUP_PGNUM, &bits),
        opt('5', "Suppress Header ~A", 4, &bits),
        opt('6', "Suppress Header ~B", 8, &bits),
        opt('7', "Suppress Footer A", 0x10, &bits),
        opt('8', "Suppress Footer B", 0x20, &bits),
    ];
    let mut f = Form::new("Format: Suppress (this page only)", items);
    f.value_col = 62;
    FormOnClose::new(f, move |wp| {
        let b = *bits.borrow();
        if b != 0 {
            wp.ed().insert_format(Code::Suppress(b));
        }
    })
}

// ------------------------------------------------------- Paper Size/Type

/// Format: Paper Size/Type — the list of paper definitions.
pub struct PaperList {
    sel: usize,
}

impl PaperList {
    pub fn new(wp: &Wp) -> PaperList {
        let defs = load_papers();
        let (w, h, _) = cur_paper(wp);
        let sel = defs.iter().position(|d| d.paper.w == w && d.paper.h == h).unwrap_or(0);
        PaperList { sel }
    }
}

fn paper_line(d: &PaperDef) -> String {
    let size = format!("{} x {}", fmt_inches(d.paper.w), fmt_inches(d.paper.h));
    let lab = match &d.paper.labels {
        Some(l) => format!("{} x {}", l.cols, l.rows),
        None => String::new(),
    };
    let name: String = d.name.chars().take(30).collect();
    format!("{name:<31}{size:<18}{:<14}{lab}", d.paper.ty.chars().take(13).collect::<String>())
}

/// Insert the code for a paper definition.
pub fn select_paper(wp: &mut Wp, p: &Paper) {
    ins(wp, Fmt::Paper(p.clone()));
    // A paper set before any text is also the document's page size.
    let e = wp.ed();
    if !e.doc.els[..e.cursor.min(e.doc.els.len())].iter().any(|x| x.is_char()) && p.labels.is_none() {
        e.doc.init.page_width = p.w;
        e.doc.init.page_height = p.h;
    }
    e.relayout();
}

impl Screen for PaperList {
    fn full_screen(&self) -> bool {
        true
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n.min(load_papers().len().saturating_sub(1));
        if double { self.input(wp, &Input::Char('1')) } else { Outcome::Keep }
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let mut defs = load_papers();
        let n = defs.len();
        self.sel = self.sel.min(n.saturating_sub(1));
        match i {
            Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
            Input::Char('1') | Input::Char('s') | Input::Char('S') | Input::Func(Func::Enter) => {
                if let Some(d) = defs.get(self.sel) {
                    let p = d.paper.clone();
                    select_paper(wp, &p);
                    wp.after_edit();
                }
                return Outcome::Close;
            }
            Input::Char('2') | Input::Char('a') | Input::Char('A') => {
                return Outcome::Push(paper_type_pick(Rc::new(|wp, ty: String| Some(paper_size_pick(wp, ty)))));
            }
            Input::Char('3') | Input::Char('c') | Input::Char('C') => {
                if let Some(d) = defs.get(self.sel).cloned() {
                    defs.insert(self.sel + 1, d);
                    save_papers(&defs);
                }
            }
            Input::Char('4') | Input::Char('d') | Input::Char('D') => {
                if n > 1 {
                    let k = self.sel;
                    return Outcome::Push(YesNo::new("Delete paper definition?", false, move |_wp, y| {
                        if y {
                            let mut v = load_papers();
                            if k < v.len() {
                                v.remove(k);
                                save_papers(&v);
                            }
                        }
                        None
                    }));
                }
            }
            Input::Char('5') | Input::Char('e') | Input::Char('E') => {
                return Outcome::Push(paper_edit(self.sel));
            }
            Input::Char('n') | Input::Char('N') => {
                return Outcome::Push(Prompt::new("(Name Search)", "", move |wp, s| {
                    if let Some(s) = s {
                        let lc = s.to_lowercase();
                        if let Some(k) = defs.iter().position(|d| d.name.to_lowercase().starts_with(&lc)) {
                            let p = defs[k].paper.clone();
                            select_paper(wp, &p);
                            wp.after_edit();
                            wp.screens.clear();
                        }
                    }
                    None
                }));
            }
            Input::Func(Func::Exit) => return Outcome::CloseAll,
            Input::Char('0') | Input::Func(Func::Cancel | Func::Esc) => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        // definitions may have been edited in a sub-screen
        let defs = load_papers();
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, "Format: Paper Size/Type", st);
        g.text(0, 2, "Paper type and Orientation     Paper Size        Type          Labels", st);
        let top = 3;
        let room = g.rows.saturating_sub(top + 2);
        let first = self.sel.saturating_sub(room.saturating_sub(1));
        for (r, (k, d)) in defs.iter().enumerate().skip(first).take(room).enumerate() {
            let s = paper_line(d);
            g.text(0, top + r, &s, if k == self.sel { wp.theme.block } else { st });
            wp.hit(top + r, 0, g.cols, crate::ui::HitAction::Select(k));
        }
        let c = g.text_mn(0, g.rows - 1, "1 ~Select; 2 ~Add; 3 ~Copy; 4 ~Delete; 5 ~Edit; ~N Name Search: ", st, wp.theme.bold);
        g.text(c, g.rows - 1, "1", st);
        g.cursor = Some((c, g.rows - 1));
    }
}

type TyNext = Rc<dyn Fn(&mut Wp, String) -> Next>;

/// A numbered full-screen pick list (Format: Paper Type / Paper Size).
struct PickList {
    title: String,
    rows: Vec<(char, String)>,
    on_pick: Rc<dyn Fn(&mut Wp, usize) -> Next>,
}

impl Screen for PickList {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char(c) => {
                let lc = c.to_ascii_lowercase();
                if let Some(k) = self.rows.iter().position(|r| r.0 == lc) {
                    let f = self.on_pick.clone();
                    return match f(wp, k) {
                        Some(n) => Outcome::Replace(n),
                        None => Outcome::Close,
                    };
                }
                if lc == '0' {
                    return Outcome::Close;
                }
                Outcome::Keep
            }
            Input::Func(Func::Exit) => Outcome::CloseAll,
            Input::Func(Func::Cancel | Func::Esc | Func::Enter) => Outcome::Close,
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, &self.title, st);
        for (k, (key, t)) in self.rows.iter().enumerate() {
            let r = 2 + 2 * k;
            if r + 2 >= g.rows {
                break;
            }
            g.text(5, r, &format!("{key} - {t}"), st);
            wp.hit(r, 5, 60, crate::ui::HitAction::Input(Input::Char(*key)));
        }
        let c = g.text(0, g.rows - 1, "Selection: ", st);
        g.text(c, g.rows - 1, "0", st);
        g.cursor = Some((c, g.rows - 1));
    }
}

const PAPER_TYPES: [&str; 9] = ["Standard", "Bond", "Letterhead", "Labels", "Envelope", "Transparency", "Cardstock", "[ALL OTHERS]", "Other"];

fn paper_type_pick(then: TyNext) -> Box<dyn Screen> {
    let rows = PAPER_TYPES.iter().enumerate().map(|(k, t)| ((b'1' + k as u8) as char, t.to_string())).collect();
    Box::new(PickList {
        title: "Format: Paper Type".into(),
        rows,
        on_pick: Rc::new(move |_wp, k| {
            let then = then.clone();
            if k == 8 {
                return Some(Prompt::new("Other form type: ", "", move |wp, s| {
                    let s = s.filter(|s| !s.trim().is_empty())?;
                    then(wp, s.trim().to_string())
                }) as Box<dyn Screen>);
            }
            let ty = PAPER_TYPES[k].to_string();
            Some(Box::new(Deferred(Some(Box::new(move |wp: &mut Wp| then(wp, ty))))) as Box<dyn Screen>)
        }),
    })
}

/// Runs a continuation on the next draw/input cycle (lets a pick list
/// replace itself with whatever the continuation returns).
struct Deferred(Option<Box<dyn FnOnce(&mut Wp) -> Next>>);

impl Screen for Deferred {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let _ = i;
        match self.0.take().and_then(|f| f(wp)) {
            Some(s) => Outcome::Replace(s),
            None => Outcome::Close,
        }
    }
    fn draw(&self, _wp: &Wp, _g: &mut Grid) {}
}

const SIZES: [(&str, f64, f64, bool); 9] = [
    ("Standard                  (8.5\" x 11\")", 8.5, 11.0, false),
    ("Standard Landscape        (11\" x 8.5\")", 11.0, 8.5, false),
    ("Legal                     (8.5\" x 14\")", 8.5, 14.0, false),
    ("Legal Landscape           (14\" x 8.5\")", 14.0, 8.5, false),
    ("Envelope                  (9.5\" x 4\")", 9.5, 4.0, false),
    ("Half Sheet                (5.5\" x 8.5\")", 5.5, 8.5, false),
    ("US Government             (8\" x 11\")", 8.0, 11.0, false),
    ("A4                        (210mm x 297mm)", 210.0, 297.0, true),
    ("A4 Landscape              (297mm x 210mm)", 297.0, 210.0, true),
];

fn paper_size_pick(_wp: &mut Wp, ty: String) -> Box<dyn Screen> {
    let mut rows: Vec<(char, String)> = SIZES.iter().enumerate().map(|(k, s)| ((b'1' + k as u8) as char, s.0.to_string())).collect();
    rows.push(('o', "Other".into()));
    Box::new(PickList {
        title: "Format: Paper Size                 Width  Height".into(),
        rows,
        on_pick: Rc::new(move |_wp, k| {
            let ty = ty.clone();
            let add = move |wp: &mut Wp, w: i32, h: i32| -> Next {
                let mut v = load_papers();
                let name = format!("{ty}{}", if w > h { " Landscape" } else { "" });
                let labels = (ty == "Labels").then(Labels::default);
                v.push(PaperDef { name, paper: Paper { w, h, ty: ty.clone(), labels } });
                save_papers(&v);
                let _ = wp;
                Some(paper_edit(v.len() - 1))
            };
            if k == 9 {
                return Some(Prompt::new("Width: ", "8.5\"", move |_wp, s| {
                    let w = s.and_then(|s| parse_measure(&s)).filter(|w| *w > 0)?;
                    Some(Prompt::new("Height: ", "11\"", move |wp, s| {
                        let h = s.and_then(|s| parse_measure(&s)).filter(|h| *h > 0)?;
                        add(wp, w, h)
                    }) as Box<dyn Screen>)
                }) as Box<dyn Screen>);
            }
            let (_, a, b, metric) = SIZES[k];
            let (w, h) = if metric { (mm(a), mm(b)) } else { ((a * 1200.0) as i32, (b * 1200.0) as i32) };
            Some(Box::new(Deferred(Some(Box::new(move |wp: &mut Wp| add(wp, w, h))))) as Box<dyn Screen>)
        }),
    })
}

/// Format: Edit Paper Definition for list entry `k`.
fn paper_edit(k: usize) -> Box<dyn Screen> {
    let v = load_papers();
    let def = Rc::new(RefCell::new(v.get(k).cloned().unwrap_or_else(|| default_papers().remove(0))));
    let d1 = def.clone();
    let d2 = def.clone();
    let d3 = def.clone();
    let d4 = def.clone();
    let d5 = def.clone();
    let d6 = def.clone();
    let d7 = def.clone();
    let items = vec![
        item(
            '1',
            "Paper ~Size",
            move |_| {
                let d = d1.borrow();
                vec![format!("{} x {}", fmt_inches(d.paper.w), fmt_inches(d.paper.h))]
            },
            fields(move |_, v| {
                let mut d = d2.borrow_mut();
                let s = v[0].to_lowercase();
                if let Some((a, b)) = s.split_once('x') {
                    if let (Some(w), Some(h)) = (parse_measure(a), parse_measure(b)) {
                        if w > 0 && h > 0 {
                            d.paper.w = w;
                            d.paper.h = h;
                        }
                    }
                }
            }),
        ),
        item(
            '2',
            "Paper ~Type",
            move |_| vec![d3.borrow().paper.ty.clone()],
            Edit::Choice(
                PAPER_TYPES[..8].to_vec(),
                Rc::new(move |_, k| {
                    d4.borrow_mut().paper.ty = PAPER_TYPES[k].into();
                }),
            ),
        ),
        item(
            '8',
            "~Labels",
            move |_| vec![yn(d5.borrow().paper.labels.is_some())],
            Edit::Sub(Rc::new(move |_| {
                let d = d6.clone();
                let has = d.borrow().paper.labels.is_some();
                YesNo::new("Labels?", has, move |_, y| {
                    if !y {
                        d.borrow_mut().paper.labels = None;
                        return None;
                    }
                    Some(labels_form(d.clone()))
                })
            })),
        ),
        item(
            'N',
            "~Name",
            {
                let d = def.clone();
                move |_| vec![d.borrow().name.clone()]
            },
            fields(move |_, v| {
                if !v[0].trim().is_empty() {
                    d7.borrow_mut().name = v[0].trim().to_string();
                }
            }),
        ),
    ];
    FormOnClose::new(Form::new("Format: Edit Paper Definition", items), move |_| {
        let mut v = load_papers();
        let d = def.borrow().clone();
        if k < v.len() {
            v[k] = d;
        } else {
            v.push(d);
        }
        save_papers(&v);
    })
}

fn labels_form(def: Rc<RefCell<PaperDef>>) -> Box<dyn Screen> {
    let lab = Rc::new(RefCell::new(def.borrow().paper.labels.clone().unwrap_or_default()));
    type G = fn(&Labels) -> Vec<String>;
    type S = fn(&mut Labels, &[String]);
    let mk = |key: char, labels: Vec<&'static str>, g: G, s: S, lab: &Rc<RefCell<Labels>>| {
        let a = lab.clone();
        let b = lab.clone();
        item2(key, labels, move |_| g(&a.borrow()), fields(move |_, v| s(&mut b.borrow_mut(), &v)))
    };
    let m = |v: &[String], i: usize, d: i32| v.get(i).and_then(|s| parse_measure(s)).unwrap_or(d);
    let _ = m;
    let items = vec![
        mk(
            '1',
            vec!["Label ~Size", "               Width", "               Height"],
            |l| vec![String::new(), fmt_inches(l.w), fmt_inches(l.h)],
            |l, v| {
                l.w = v.get(1).and_then(|s| parse_measure(s)).unwrap_or(l.w).max(120);
                l.h = v.get(2).and_then(|s| parse_measure(s)).unwrap_or(l.h).max(120);
            },
            &lab,
        ),
        mk(
            '2',
            vec!["~Number of Labels", "               Columns", "               Rows"],
            |l| vec![String::new(), l.cols.to_string(), l.rows.to_string()],
            |l, v| {
                l.cols = v.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(l.cols).max(1);
                l.rows = v.get(2).and_then(|s| s.trim().parse().ok()).unwrap_or(l.rows).max(1);
            },
            &lab,
        ),
        mk(
            '3',
            vec!["~Top Left Corner", "               Top", "               Left"],
            |l| vec![String::new(), fmt_inches(l.top), fmt_inches(l.left)],
            |l, v| {
                l.top = v.get(1).and_then(|s| parse_measure(s)).unwrap_or(l.top);
                l.left = v.get(2).and_then(|s| parse_measure(s)).unwrap_or(l.left);
            },
            &lab,
        ),
        mk(
            '4',
            vec!["~Distance Between Labels", "               Column", "               Row"],
            |l| vec![String::new(), fmt_inches(l.gap_col), fmt_inches(l.gap_row)],
            |l, v| {
                l.gap_col = v.get(1).and_then(|s| parse_measure(s)).unwrap_or(l.gap_col);
                l.gap_row = v.get(2).and_then(|s| parse_measure(s)).unwrap_or(l.gap_row);
            },
            &lab,
        ),
        mk(
            '5',
            vec!["Label ~Margins", "               Left", "               Right", "               Top", "               Bottom"],
            |l| vec![String::new(), fmt_inches(l.ml), fmt_inches(l.mr), fmt_inches(l.mt), fmt_inches(l.mb)],
            |l, v| {
                l.ml = v.get(1).and_then(|s| parse_measure(s)).unwrap_or(l.ml);
                l.mr = v.get(2).and_then(|s| parse_measure(s)).unwrap_or(l.mr);
                l.mt = v.get(3).and_then(|s| parse_measure(s)).unwrap_or(l.mt);
                l.mb = v.get(4).and_then(|s| parse_measure(s)).unwrap_or(l.mb);
            },
            &lab,
        ),
    ];
    FormOnClose::new(Form::new("Format: Labels", items), move |_| {
        let mut d = def.borrow_mut();
        d.paper.labels = Some(lab.borrow().clone());
        d.paper.ty = "Labels".into();
    })
}

// =============================================================== Document

pub fn format_document() -> Box<dyn Screen> {
    let items = vec![
        item2(
            '1',
            vec!["Display ~Pitch - Automatic", "                  Width"],
            |wp| {
                let o = &wp.ed_ref().doc.opts;
                vec![yn(o.pitch_auto), fmt_inches(o.pitch)]
            },
            fields(|wp, v| {
                let e = wp.ed();
                e.doc.opts.pitch_auto = is_yes(&v[0]);
                if let Some(p) = parse_measure(&v[1]).filter(|p| *p > 0) {
                    e.doc.opts.pitch = p;
                }
                e.modified = true;
            }),
        ),
        item('2', "~Initial Codes", |_| vec![], Edit::Action(Rc::new(crate::subedit::edit_initial_codes))),
        item(
            '3',
            "Initial Base ~Font",
            |wp| {
                let f = &wp.ed_ref().doc.init.font;
                vec![format!("{} {}pt", f.name, f.size_cpt as f64 / 100.0)]
            },
            Edit::Sub(Rc::new(|wp| {
                let cur = wp.ed_ref().doc.init.font.clone();
                let mut l = crate::fonts::FontList::new(cur);
                l.set_initial = true;
                Box::new(l)
            })),
        ),
        item(
            '4',
            "~Redline Method",
            |wp| vec![wp.ed_ref().doc.opts.redline_name().into()],
            Edit::Choice(
                vec!["Printer Dependent", "Left", "Alternating"],
                Rc::new(|wp, k| {
                    let e = wp.ed();
                    e.doc.opts.redline = k as u8;
                    e.modified = true;
                }),
            ),
        ),
        item('5', "~Summary", |_| vec![], Edit::Sub(Rc::new(|_| crate::menus::summary_form()))),
    ];
    Form::new("Format: Document", items)
}

// ================================================================== Other

pub fn format_other() -> Box<dyn Screen> {
    let items = vec![
        item('1', "~Advance", |_| vec![], Edit::Sub(Rc::new(|_| advance_menu()))),
        item(
            '2',
            "~Conditional End of Page",
            |_| vec![],
            Edit::Sub(Rc::new(|_| {
                Prompt::new("Number of Lines to Keep Together: ", "", |wp, s| {
                    if let Some(n) = s.and_then(|s| s.trim().parse::<u32>().ok()).filter(|n| *n > 0) {
                        wp.ed().insert_code(Code::Fmt(Fmt::CndlEop(n)));
                        wp.after_edit();
                    }
                    None
                })
            })),
        ),
        item2(
            '3',
            vec!["~Decimal/Align Character", "    Thousands' Separator"],
            |wp| {
                let s = fx(wp);
                vec![s.dec.to_string(), s.thou.to_string()]
            },
            fields(|wp, v| {
                let s = fx(wp);
                let d = v[0].trim().chars().next().unwrap_or(s.dec);
                let t = v[1].chars().next().unwrap_or(s.thou);
                if (d, t) != (s.dec, s.thou) {
                    ins(wp, Fmt::DecAlign { dec: d, thou: t });
                }
            }),
        ),
        item(
            '4',
            "~Language",
            |wp| vec![fx(wp).lang],
            fields(|wp, v| {
                let l: String = v[0].trim().chars().filter(|c| c.is_ascii_alphabetic()).take(2).collect::<String>().to_ascii_uppercase();
                if l.len() == 2 && l != fx(wp).lang {
                    ins(wp, Fmt::Lang(l));
                }
            }),
        ),
        item('5', "~Overstrike", |_| vec![], Edit::Sub(Rc::new(|_| overstrike_menu()))),
        item('6', "~Printer Functions", |_| vec![], Edit::Sub(Rc::new(|_| printer_functions()))),
        item2(
            '7',
            vec!["~Underline - Spaces", "            Tabs"],
            |wp| {
                let u = fx(wp).undrln;
                vec![yn(u.0), yn(u.1)]
            },
            fields(|wp, v| {
                let u = fx(wp).undrln;
                let n = (is_yes(&v[0]), is_yes(&v[1]));
                if n != u {
                    ins(wp, Fmt::Undrln { spaces: n.0, tabs: n.1 });
                }
            }),
        ),
        item('8', "~Border Options", |_| vec![], Edit::Sub(Rc::new(|wp| border_options(wp)))),
    ];
    Form::new("Format: Other", items)
}

fn advance_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Up", "~Down", "L~ine", "~Left", "~Right", "~Position"], |_wp, k| {
        let (a, label) = [
            (Adv::Up, "Adv. up "),
            (Adv::Down, "Adv. down "),
            (Adv::ToLine, "Adv. to line "),
            (Adv::Left, "Adv. left "),
            (Adv::Right, "Adv. right "),
            (Adv::ToPos, "Adv. to position "),
        ][k];
        Some(Prompt::new(label, "0\"", move |wp, s| {
            if let Some(v) = s.and_then(|s| parse_measure(&s)) {
                wp.ed().insert_code(Code::Fmt(Fmt::Adv(a, v.abs())));
                wp.after_edit();
            }
            None
        }) as Box<dyn Screen>)
    })
    .prefix("Advance: ")
}

fn overstrike_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Create", "~Edit"], |wp, k| {
        let mut at: Option<usize> = None;
        let mut init = String::new();
        if k == 1 {
            let e = wp.ed_ref();
            let els = &e.doc.els;
            let after = (e.cursor..els.len()).find(|&i| matches!(els[i], El::Code(Code::Fmt(Fmt::Ovrstk(_)))));
            let before = (0..e.cursor.min(els.len())).rev().find(|&i| matches!(els[i], El::Code(Code::Fmt(Fmt::Ovrstk(_)))));
            match after.or(before) {
                Some(i) => {
                    at = Some(i);
                    if let El::Code(Code::Fmt(Fmt::Ovrstk(s))) = &els[i] {
                        init = s.clone();
                    }
                }
                None => {
                    wp.flash("* Not found *");
                    return None;
                }
            }
        }
        Some(
            Prompt::new("", &init, move |wp, s| {
                let s = s.filter(|s| !s.is_empty())?;
                let e = wp.ed();
                match at {
                    Some(i) => {
                        e.doc.els[i] = El::Code(Code::Fmt(Fmt::Ovrstk(s)));
                        e.modified = true;
                        e.relayout();
                    }
                    None => e.insert_code(Code::Fmt(Fmt::Ovrstk(s))),
                }
                wp.after_edit();
                None
            })
            .keep_text() as Box<dyn Screen>,
        )
    })
}

/// The character an overstrike shows: base + accents composed where
/// possible (e´ → é), otherwise the base character.
pub fn overstrike_char(s: &str) -> char {
    let mut it = s.chars();
    let Some(mut base) = it.next() else { return ' ' };
    for c in it {
        let acc = match c {
            '´' | '\u{301}' => '\'',
            '`' | '\u{300}' => '`',
            '¨' | '\u{308}' => '"',
            '^' | '\u{302}' => '^',
            '~' | '\u{303}' => '~',
            '¸' | '\u{327}' => ',',
            '/' => '/',
            other => other,
        };
        match crate::menus::compose_pair(base, acc) {
            Some(x) => base = x,
            None => {
                if c == '_' || c == '-' {
                    continue;
                }
            }
        }
    }
    base
}

fn spacing_parse(s: &str, cur: Spacing) -> Spacing {
    let t = s.trim().to_lowercase();
    if t.starts_with('n') {
        Spacing::Normal
    } else if t.starts_with('o') {
        Spacing::Optimal
    } else {
        parse_pct(&t).map(|p| Spacing::Percent(p.clamp(50, 250) as u16)).unwrap_or(cur)
    }
}

fn printer_functions() -> Box<dyn Screen> {
    let items = vec![
        item('1', "~Kerning", |wp| vec![yn(fx(wp).kern)], Edit::YesNo(Rc::new(|wp, y| ins(wp, Fmt::Kern(y))))),
        item(
            '2',
            "~Printer Command",
            |_| vec![],
            Edit::Sub(Rc::new(|_| {
                MenuLine::new(&["~Command", "~Filename"], |_wp, k| {
                    Some(Prompt::new(if k == 0 { "Command: " } else { "Document name: " }, "", move |wp, s| {
                        let s = s.filter(|s| !s.is_empty())?;
                        let v = if k == 1 { format!("File={s}") } else { s };
                        wp.ed().insert_code(Code::Fmt(Fmt::PtrCmnd(v)));
                        wp.after_edit();
                        None
                    }) as Box<dyn Screen>)
                })
            })),
        ),
        item2(
            '3',
            vec!["~Word Spacing", "    Letter Spacing"],
            |wp| {
                let s = fx(wp).wrd_ltr;
                vec![s.0.name(), s.1.name()]
            },
            fields(|wp, v| {
                let s = fx(wp).wrd_ltr;
                let n = (spacing_parse(&v[0], s.0), spacing_parse(&v[1], s.1));
                if n != s {
                    ins(wp, Fmt::WrdLtr { word: n.0, letter: n.1 });
                }
            }),
        ),
        item2(
            '4',
            vec!["Word Spacing ~Justification Limits", "      Compressed to (0% - 100%)", "      Expanded to (100% - unlimited)"],
            |wp| {
                let j = fx(wp).just_lim;
                vec![String::new(), format!("{}%", j.0), format!("{}%", j.1)]
            },
            fields(|wp, v| {
                let j = fx(wp).just_lim;
                let c = parse_pct(&v[1]).map(|x| x.min(100) as u16).unwrap_or(j.0);
                let e = parse_pct(&v[2]).map(|x| x.min(9999) as u16).unwrap_or(j.1);
                if (c, e) != j {
                    ins(wp, Fmt::JustLim { compress: c, expand: e });
                }
            }),
        ),
        item2(
            '5',
            vec!["~Baseline Placement for Typesetters", "      (First baseline at top margin)"],
            |wp| vec![yn(fx(wp).bline)],
            Edit::YesNo(Rc::new(|wp, y| ins(wp, Fmt::Bline(y)))),
        ),
        item2(
            '6',
            vec!["~Leading Adjustment", "      Primary   - [SRt]", "      Secondary - [HRt]"],
            |wp| {
                let l = fx(wp).leading;
                vec![String::new(), fmt_inches(l.0), fmt_inches(l.1)]
            },
            fields(|wp, v| {
                let l = fx(wp).leading;
                let p = parse_measure(&v[1]).unwrap_or(l.0);
                let s = parse_measure(&v[2]).unwrap_or(l.1);
                if (p, s) != l {
                    ins(wp, Fmt::LeadingAdj { primary: p, secondary: s });
                }
            }),
        ),
    ];
    let mut f = Form::new("Format: Printer Functions", items);
    f.value_col = 50;
    f
}

fn border_options(wp: &Wp) -> Box<dyn Screen> {
    let v = Rc::new(RefCell::new(fx(wp).brdr));
    // (key, labels, value slots: (index, is_percent))
    let spec: [(char, Vec<&'static str>, Vec<(usize, bool)>); 6] = [
        ('1', vec!["~Single -       Width", "                   Shading"], vec![(0, false), (1, true)]),
        ('2', vec!["~Double -       Width", "                   Shading", "                   Spacing Between"], vec![(2, false), (3, true), (4, false)]),
        ('3', vec!["D~ashed -       Width", "                   Shading", "                   Dash Spacing", "                   Dash Length"], vec![(5, false), (6, true), (7, false), (8, false)]),
        ('4', vec!["D~otted -       Width", "                   Shading", "                   Dot Spacing"], vec![(9, false), (10, true), (11, false)]),
        ('5', vec!["~Thick -        Width", "                   Shading"], vec![(12, false), (13, true)]),
        ('6', vec!["~Extra Thick -  Width", "                   Shading"], vec![(14, false), (15, true)]),
    ];
    let mut items = vec![];
    for (key, labels, slots) in spec {
        let a = v.clone();
        let b = v.clone();
        let s1 = slots.clone();
        items.push(item2(
            key,
            labels,
            move |_| {
                let v = a.borrow();
                s1.iter().map(|&(i, pct)| if pct { format!("{}%", v.get(i).copied().unwrap_or(100)) } else { fmt_inches(v.get(i).copied().unwrap_or(0)) }).collect()
            },
            fields(move |_, vals| {
                let mut v = b.borrow_mut();
                for (k, &(i, pct)) in slots.iter().enumerate() {
                    if i >= v.len() {
                        continue;
                    }
                    let s = vals.get(k).map(|s| s.as_str()).unwrap_or("");
                    if pct {
                        if let Some(p) = parse_pct(s) {
                            v[i] = p.min(100) as i32;
                        }
                    } else if let Some(m) = parse_measure(s) {
                        v[i] = m.max(0);
                    }
                }
            }),
        ));
    }
    let orig = v.borrow().clone();
    FormOnClose::new(Form::new("Format: Border Options", items), move |wp| {
        let n = v.borrow().clone();
        if n != orig {
            ins(wp, Fmt::BrdrOpt(n));
        }
    })
}

// ============================================================ edit screen

/// Shift-F8 with Block on: `Protect block? No (Yes)`.  Returns true when
/// handled.
pub fn block_protect(wp: &mut Wp) -> bool {
    if wp.ed_ref().block_anchor.is_none() {
        return false;
    }
    wp.push(YesNo::new("Protect block?", false, |wp, y| {
        if y {
            if let Some((s, e)) = wp.ed().block_range() {
                let ed = wp.ed();
                ed.block_anchor = None;
                ed.cursor = e;
                ed.insert_code(Code::Fmt(Fmt::BlockPro(false)));
                ed.cursor = s;
                ed.insert_code(Code::Fmt(Fmt::BlockPro(true)));
                wp.after_edit();
            }
        }
        None
    }));
    true
}

/// Home followed by a key: hard tabs, `[ISRt]`, `[/]`.
pub fn home_code(f: Func) -> Option<Code> {
    Some(Code::Fmt(match f {
        Func::Tab => Fmt::HardTab(TabKind::Left),
        Func::Center => Fmt::HardTab(TabKind::Center),
        Func::FlushRight => Fmt::HardTab(TabKind::Right),
        Func::TabAlign => Fmt::HardTab(TabKind::Decimal),
        Func::Enter => Fmt::ISRt,
        _ => return None,
    }))
}

/// Ctrl-F6 Tab Align: `[Dec Tab]` and the status message.
pub fn tab_align(wp: &mut Wp) {
    let dec = fx(wp).dec;
    wp.ed().insert_code(Code::Fmt(Fmt::DecTab));
    wp.flash(&format!("Align char = {dec}"));
}

/// The language code in effect at the cursor, if a `[Lang]` code set one.
pub fn doc_lang_at_cursor(wp: &Wp) -> Option<String> {
    let e = wp.ed_ref();
    let mut l = None;
    for el in e.doc.initial_codes.iter().chain(e.doc.els[..e.cursor.min(e.doc.els.len())].iter()) {
        if let El::Code(Code::Fmt(Fmt::Lang(x))) = el {
            l = Some(x.clone());
        }
    }
    l
}

/// German month/day names for the date: from `[Lang]` when present,
/// otherwise from Setup.
pub fn german_dates(wp: &Wp) -> bool {
    match doc_lang_at_cursor(wp) {
        Some(l) => l == "DE" || l == "SD",
        None => wp.german_dates(),
    }
}

// ------------------------------------------------------ hyphenation prompt

/// After an edit: if layout wants the user to place a hyphen in a word
/// before the cursor, ask (`Position hyphen; Press ESC  Überset-zung`).
pub fn maybe_ask_hyph(wp: &mut Wp) {
    if wp.has_screens() || crate::hyph::prompt_mode() == 0 {
        return;
    }
    let e = wp.ed_ref();
    let cur = e.cursor;
    let Some(&(a, we, sug)) = e.layout.hyph_ask.iter().find(|x| x.1 < cur) else { return };
    if we > e.doc.els.len() {
        return;
    }
    let letters: Vec<usize> = (a..we).filter(|&k| e.doc.els[k].is_char()).collect();
    if letters.len() < 2 {
        return;
    }
    let pos = letters.iter().position(|&k| k == sug).unwrap_or(letters.len() / 2).clamp(1, letters.len() - 1);
    wp.push(Box::new(HyphPrompt { a, letters, pos }));
}

struct HyphPrompt {
    a: usize,
    letters: Vec<usize>,
    /// break before letters[pos]
    pos: usize,
}

impl HyphPrompt {
    fn hyphenate(&self, wp: &mut Wp) {
        let at = self.letters[self.pos];
        let e = wp.ed();
        let cur = e.cursor;
        e.cursor = at;
        e.insert_code(Code::SoftHyphen);
        e.cursor = if cur >= at { cur + 1 } else { cur };
    }
    fn cancel(&self, wp: &mut Wp) {
        let e = wp.ed();
        let cur = e.cursor;
        e.cursor = self.a;
        e.insert_code(Code::Fmt(Fmt::CancelHyph));
        e.cursor = if cur >= self.a { cur + 1 } else { cur };
    }
}

impl Screen for HyphPrompt {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let alt = wp.alternate_keys;
        let done = match i {
            Input::Func(Func::Left) => {
                self.pos = (self.pos - 1).max(1);
                None
            }
            Input::Func(Func::Right) => {
                self.pos = (self.pos + 1).min(self.letters.len() - 1);
                None
            }
            Input::Func(Func::Esc) | Input::Func(Func::Enter) => Some(true),
            // with the alternate keyboard the Esc key sends Cancel
            Input::Func(Func::Cancel) => Some(alt),
            Input::Func(Func::Help) if alt => Some(false),
            Input::Char('/') | Input::Func(Func::Exit) => Some(false),
            _ => None,
        };
        match done {
            Some(true) => self.hyphenate(wp),
            Some(false) => self.cancel(wp),
            None => return Outcome::Keep,
        }
        wp.after_edit();
        Outcome::Close
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        let c = g.text(0, row, "Position hyphen; Press ESC  ", st);
        let e = wp.ed_ref();
        let mut word = String::new();
        let mut cur = c;
        for (k, &el) in self.letters.iter().enumerate() {
            if k == self.pos {
                cur = c + word.chars().count();
                word.push('-');
            }
            if let El::Ch(ch) = e.doc.els[el] {
                word.push(ch);
            }
        }
        g.text(c, row, &word, st);
        g.cursor = Some((cur, row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overstrike() {
        assert_eq!(overstrike_char("e´"), 'é');
        assert_eq!(overstrike_char("a\u{308}"), 'ä');
        assert_eq!(overstrike_char("x"), 'x');
    }
}
