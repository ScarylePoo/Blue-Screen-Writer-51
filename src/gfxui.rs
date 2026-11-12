//! Alt-F9 Graphics: the box menus, the `Definition:` screen, the box
//! `Options:` screen, `Graphics: Horizontal/Vertical Line` and the image
//! editor (move, scale, rotate, mirror, invert, black & white).

use crate::doc::*;
use crate::gfx::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{HitAction, Outcome, Screen, Wp};

// ------------------------------------------------------------ bottom-line questions

enum AskK {
    Choice(Vec<String>),
    Field(Vec<char>, bool),
}

/// A question on the bottom line of a full-screen form: a numbered
/// choice (`1 Left; 2 Right: 0`) or a typed value.
struct Ask {
    prompt: String,
    k: AskK,
    step: u32,
}

enum Answer {
    Pick(usize),
    Text(String),
}

impl Ask {
    fn choice(step: u32, prompt: &str, items: &[&str]) -> Ask {
        Ask { prompt: prompt.into(), k: AskK::Choice(items.iter().map(|s| s.to_string()).collect()), step }
    }
    fn field(step: u32, prompt: &str, init: &str) -> Ask {
        Ask { prompt: prompt.into(), k: AskK::Field(init.chars().collect(), true), step }
    }

    /// None = still asking; Some(None) = cancelled; Some(Some(a)) = answered.
    fn input(&mut self, i: &Input) -> Option<Option<Answer>> {
        match &mut self.k {
            AskK::Choice(items) => match i {
                Input::Char(c) => {
                    if let Some(d) = c.to_digit(10) {
                        if d >= 1 && d as usize <= items.len() {
                            return Some(Some(Answer::Pick(d as usize - 1)));
                        }
                        return Some(None);
                    }
                    let lc = c.to_ascii_lowercase();
                    items.iter().position(|s| s.chars().next().map(|f| f.to_ascii_lowercase()) == Some(lc)).map(|k| Some(Answer::Pick(k)))
                }
                Input::Func(_) => Some(None),
            },
            AskK::Field(buf, fresh) => match i {
                Input::Char(c) => {
                    if *fresh {
                        buf.clear();
                        *fresh = false;
                    }
                    buf.push(*c);
                    None
                }
                Input::Func(Func::Backspace) => {
                    *fresh = false;
                    buf.pop();
                    None
                }
                Input::Func(Func::Enter | Func::Exit | Func::Tab) => Some(Some(Answer::Text(buf.iter().collect()))),
                Input::Func(Func::Cancel | Func::Esc) => Some(None),
                _ => None,
            },
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        let mut c = g.text(0, row, &self.prompt, st);
        match &self.k {
            AskK::Choice(items) => {
                for (k, it) in items.iter().enumerate() {
                    let c0 = c;
                    c = g.text(c, row, &format!("{} {}", k + 1, it), st);
                    let key = char::from_digit((k + 1) as u32 % 10, 10).unwrap_or('0');
                    wp.hit(row, c0, c, HitAction::Input(Input::Char(key)));
                    if k + 1 < items.len() {
                        c = g.text(c, row, "; ", st);
                    }
                }
                c = g.text(c, row, ": ", st);
                g.text(c, row, "0", st);
                g.cursor = Some((c, row));
            }
            AskK::Field(buf, _) => {
                let s: String = buf.iter().collect();
                let e = g.text(c, row, &s, st);
                g.cursor = Some((e.min(g.cols - 1), row));
            }
        }
    }
}

fn measure(s: &str, old: i32) -> i32 {
    parse_measure(s).unwrap_or(old)
}

fn item_row(wp: &Wp, g: &mut Grid, row: usize, key: char, label: &str, val: &str, vcol: usize) {
    let st = wp.theme.normal;
    g.text(5, row, &format!("{key} - "), st);
    let end = g.text_mn(9, row, label, st, wp.theme.bold);
    wp.hit(row, 5, end.max(vcol + val.chars().count()), HitAction::Input(Input::Char(key)));
    g.text(vcol, row, val, st);
}

// ------------------------------------------------------------ Alt-F9

pub fn graphics_menu(wp: &mut Wp) {
    wp.push(MenuLine::new(&["~Figure", "~Table Box", "Text ~Box", "~User Box", "~Line", "~Equation"], |_wp, k| match k {
        4 => Some(line_menu()),
        _ => {
            let kind = [BoxKind::Figure, BoxKind::Table, BoxKind::Text, BoxKind::User, BoxKind::User, BoxKind::Equation][k];
            Some(box_menu(kind))
        }
    }));
}

fn box_menu(kind: BoxKind) -> Box<dyn Screen> {
    let name = kind.name();
    MenuLine::new(&["~Create", "~Edit", "~New Number", "~Options"], move |wp, k| match k {
        0 => Some(Box::new(DefScreen::new(new_box(wp, kind), None)) as Box<dyn Screen>),
        1 => {
            let list = boxes_of(&wp.ed_ref().doc, kind);
            let cur = wp.ed_ref().cursor;
            let dflt = list.iter().find(|(i, _)| *i >= cur).or(list.last()).map(|x| x.1.clone()).unwrap_or_else(|| "1".into());
            Some(Prompt::new(&format!("{name} number?"), &dflt, move |wp, s| {
                let s = s?;
                let list = boxes_of(&wp.ed_ref().doc, kind);
                let Some(&(idx, _)) = list.iter().find(|(_, n)| n.eq_ignore_ascii_case(s.trim())) else {
                    wp.flash(&format!("ERROR: {name} not found"));
                    return None;
                };
                let El::Code(Code::GBox(b)) = &wp.ed_ref().doc.els[idx] else { return None };
                let b = (**b).clone();
                wp.ed().cursor = idx + 1;
                Some(Box::new(DefScreen::new(b, Some(idx))) as Box<dyn Screen>)
            }) as Box<dyn Screen>)
        }
        2 => Some(Prompt::new(&format!("{name} number?"), "1", move |wp, s| {
            let n: u32 = s?.trim().parse().ok().filter(|n| *n > 0)?;
            wp.ed().insert_code(Code::BoxNewNum(kind, n));
            wp.after_edit();
            None
        }) as Box<dyn Screen>),
        _ => {
            let cur = wp.ed_ref().cursor;
            let o = wp.ed_ref().doc.els[..cur]
                .iter()
                .rev()
                .find_map(|e| match e {
                    El::Code(Code::BoxOpt(o)) if o.kind == kind => Some((**o).clone()),
                    _ => None,
                })
                .unwrap_or_else(|| BoxOpts::default_for(kind));
            Some(Box::new(OptScreen { o, ask: None, queue: vec![] }) as Box<dyn Screen>)
        }
    })
    .prefix(&format!("{name}: "))
}

/// A new box with WP's defaults; an equation spans the margins.
fn new_box(wp: &Wp, kind: BoxKind) -> GBox {
    let mut b = GBox::new(kind);
    if kind == BoxKind::Equation {
        let d = &wp.ed_ref().doc;
        b.width = d.init.page_width - d.init.left - d.init.right;
    }
    if kind != BoxKind::Figure {
        b.caption = vec![];
    }
    b
}

// ------------------------------------------------------------ Definition screen

pub struct DefScreen {
    b: GBox,
    target: Option<usize>,
    ask: Option<Ask>,
    queue: Vec<u32>,
}

const S_FILE: u32 = 1;
const S_CONTENTS: u32 = 2;
const S_ANCHOR: u32 = 4;
const S_VPOS: u32 = 5;
const S_VSET: u32 = 51;
const S_HPOS: u32 = 6;
const S_HMARGIN: u32 = 61;
const S_HSET: u32 = 62;
const S_SIZE: u32 = 7;
const S_WIDTH: u32 = 71;
const S_HEIGHT: u32 = 72;
const S_WRAP: u32 = 8;

impl DefScreen {
    pub fn new(b: GBox, target: Option<usize>) -> DefScreen {
        DefScreen { b, target, ask: None, queue: vec![] }
    }

    fn commit(self, wp: &mut Wp) {
        let code = Code::GBox(Box::new(self.b));
        let e = wp.ed();
        match self.target {
            Some(idx) if idx < e.doc.els.len() => {
                e.delete_range(idx, idx + 1, false);
                e.cursor = idx;
                e.insert_code(code);
            }
            _ => e.insert_code(code),
        }
        wp.after_edit();
    }

    fn start(&mut self, step: u32) {
        let b = &self.b;
        self.ask = Some(match step {
            S_FILE => Ask::field(S_FILE, "Enter filename: ", &b.file),
            S_CONTENTS => Ask::choice(S_CONTENTS, "Contents: ", &["None", "Graphic", "Text", "Equation"]),
            S_ANCHOR => Ask::choice(S_ANCHOR, "Anchor Type: ", &["Paragraph", "Page", "Character"]),
            S_VPOS => match b.anchor {
                Anchor::Para => {
                    let v = if let VPos::Set(v) = b.vpos { v } else { 0 };
                    Ask::field(S_VSET, "Offset from top of paragraph: ", &fmt_inches(v))
                }
                Anchor::Page => Ask::choice(S_VPOS, "Vertical Position: ", &["Full Page", "Top", "Center", "Bottom", "Set Position"]),
                Anchor::Char => Ask::choice(S_VPOS, "Character Box Alignment: ", &["Top", "Center", "Bottom", "Baseline"]),
            },
            S_VSET => {
                let v = if let VPos::Set(v) = b.vpos { v } else { 0 };
                Ask::field(S_VSET, "Offset from top of page: ", &fmt_inches(v))
            }
            S_HPOS => match b.anchor {
                Anchor::Page => Ask::choice(S_HPOS, "Horizontal Position: ", &["Margins", "Columns", "Set Position"]),
                _ => Ask::choice(S_HMARGIN, "Horizontal Position: ", &["Left", "Right", "Center", "Full"]),
            },
            S_HMARGIN => Ask::choice(S_HMARGIN, "Horizontal Position: ", &["Left", "Right", "Center", "Full"]),
            S_HSET => {
                let v = if let HPos::Set(v) = b.hpos { v } else { 1200 };
                Ask::field(S_HSET, "Offset from left of page: ", &fmt_inches(v))
            }
            S_SIZE => Ask::choice(S_SIZE, "", &["Set Width/Auto Height", "Set Height/Auto Width", "Set Both", "Auto Both"]),
            S_WIDTH => Ask::field(S_WIDTH, "Width = ", &fmt_inches(b.width)),
            S_HEIGHT => Ask::field(S_HEIGHT, "Height = ", &fmt_inches(b.height)),
            _ => Ask::choice(S_WRAP, "Wrap Text Around Box: ", &["Yes", "No"]),
        });
    }

    fn next(&mut self) {
        if let Some(s) = (!self.queue.is_empty()).then(|| self.queue.remove(0)) {
            self.start(s);
        }
    }

    fn apply(&mut self, wp: &mut Wp, step: u32, a: Answer) {
        let b = &mut self.b;
        match (step, a) {
            (S_FILE, Answer::Text(s)) => {
                let s = s.trim().to_string();
                if s.is_empty() {
                    return;
                }
                let p = wp.resolve_path(&s);
                match std::fs::read(&p) {
                    Ok(bytes) => {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(s.clone());
                        if crate::raster::sniff(&bytes).is_some() && crate::raster::decode(&bytes).is_some() {
                            b.set_image(&name, bytes);
                        } else if b.kind == BoxKind::Equation {
                            b.equation = String::from_utf8_lossy(&bytes).replace(['\r', '\n'], " ");
                            b.contents = Contents::Equation;
                            b.file = name.to_uppercase();
                        } else if let Ok(d) = crate::fileio::load(&p) {
                            b.text = d.els;
                            b.contents = Contents::Text;
                            b.file = name.to_uppercase();
                            b.auto |= 1;
                        } else {
                            wp.flash("ERROR: Incorrect format");
                        }
                    }
                    Err(_) => wp.flash(&format!("ERROR: File not found -- {s}")),
                }
            }
            (S_CONTENTS, Answer::Pick(k)) => {
                b.contents = [Contents::Empty, Contents::Image, Contents::Text, Contents::Equation][k];
            }
            (S_ANCHOR, Answer::Pick(k)) => {
                b.anchor = [Anchor::Para, Anchor::Page, Anchor::Char][k];
                b.vpos = match b.anchor {
                    Anchor::Para => VPos::Set(0),
                    Anchor::Page => VPos::Top,
                    Anchor::Char => VPos::Baseline,
                };
                if b.anchor == Anchor::Char && b.hpos == HPos::Full {
                    b.hpos = HPos::Right;
                }
            }
            (S_VPOS, Answer::Pick(k)) => {
                if b.anchor == Anchor::Char {
                    b.vpos = [VPos::Top, VPos::Center, VPos::Bottom, VPos::Baseline][k];
                } else if k == 4 {
                    self.queue.insert(0, S_VSET);
                } else {
                    b.vpos = [VPos::Full, VPos::Top, VPos::Center, VPos::Bottom][k];
                }
            }
            (S_VSET, Answer::Text(s)) => {
                let old = if let VPos::Set(v) = b.vpos { v } else { 0 };
                b.vpos = VPos::Set(measure(&s, old));
            }
            (S_HPOS, Answer::Pick(k)) => match k {
                0 | 1 => self.queue.insert(0, S_HMARGIN),
                _ => self.queue.insert(0, S_HSET),
            },
            (S_HMARGIN, Answer::Pick(k)) => {
                b.hpos = [HPos::Left, HPos::Right, HPos::Center, HPos::Full][k];
            }
            (S_HSET, Answer::Text(s)) => {
                let old = if let HPos::Set(v) = b.hpos { v } else { 1200 };
                b.hpos = HPos::Set(measure(&s, old));
            }
            (S_SIZE, Answer::Pick(k)) => {
                b.auto = [1, 2, 0, 3][k];
                match k {
                    0 => self.queue.insert(0, S_WIDTH),
                    1 => self.queue.insert(0, S_HEIGHT),
                    2 => {
                        self.queue.insert(0, S_HEIGHT);
                        self.queue.insert(0, S_WIDTH);
                    }
                    _ => b.auto_height(),
                }
            }
            (S_WIDTH, Answer::Text(s)) => {
                b.width = measure(&s, b.width).max(120);
                if b.hpos == HPos::Full {
                    b.hpos = HPos::Right;
                }
                b.auto_height();
            }
            (S_HEIGHT, Answer::Text(s)) => {
                b.height = measure(&s, b.height).max(120);
                b.auto_height();
            }
            (S_WRAP, Answer::Pick(k)) => b.wrap = k == 0,
            _ => {}
        }
    }

    fn size_text(&self) -> String {
        let b = &self.b;
        let w = fmt_inches(b.width);
        let h = fmt_inches(b.height);
        let wide = if b.auto & 2 != 0 { "(wide)" } else { "wide" };
        let high = if b.auto & 1 != 0 { "(high)" } else { "high" };
        let w = if b.anchor != Anchor::Char && b.hpos == HPos::Full { "Full".to_string() } else { w };
        format!("{w} {wide} x {h} {high}")
    }
}

impl Screen for DefScreen {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if let Some(ask) = &mut self.ask {
            if let Some(res) = ask.input(i) {
                let step = ask.step;
                self.ask = None;
                match res {
                    Some(a) => {
                        self.apply(wp, step, a);
                        self.next();
                    }
                    None => self.queue.clear(),
                }
            }
            return Outcome::Keep;
        }
        let c = match i {
            Input::Char(c) => c.to_ascii_lowercase(),
            Input::Func(Func::Cancel | Func::Esc) => return Outcome::Close,
            Input::Func(Func::Exit | Func::Enter) => {
                let me = std::mem::replace(self, DefScreen::new(GBox::new(BoxKind::Figure), None));
                me.commit(wp);
                return Outcome::Close;
            }
            _ => return Outcome::Keep,
        };
        let key = match c {
            'f' => '1',
            'o' => '2',
            'c' => '3',
            't' => '4',
            'v' => '5',
            'h' => '6',
            's' => '7',
            'w' => '8',
            'e' => '9',
            x => x,
        };
        match key {
            '0' | ' ' => {
                let me = std::mem::replace(self, DefScreen::new(GBox::new(BoxKind::Figure), None));
                me.commit(wp);
                return Outcome::Close;
            }
            '1' => self.start(S_FILE),
            '2' => self.start(S_CONTENTS),
            '4' => self.start(S_ANCHOR),
            '5' => self.start(S_VPOS),
            '6' => {
                if self.b.anchor != Anchor::Char {
                    self.start(S_HPOS)
                }
            }
            '7' => self.start(S_SIZE),
            '8' => self.start(S_WRAP),
            '3' => {
                let mut b = self.b.clone();
                let target = self.target;
                let els = if b.caption.is_empty() { vec![El::Code(Code::BoxNum)] } else { std::mem::take(&mut b.caption) };
                crate::subedit::open(wp, "Box Caption", els, false, move |wp, els| {
                    b.caption = els;
                    wp.push(Box::new(DefScreen::new(b, target)));
                });
                return Outcome::Close;
            }
            '9' => {
                let b = self.b.clone();
                let target = self.target;
                match b.contents {
                    Contents::Image => return Outcome::Replace(Box::new(ImageEdit::new(b, target))),
                    Contents::Equation => {
                        let els: Vec<El> = b.equation.chars().map(El::Ch).collect();
                        let mut b2 = b;
                        crate::subedit::open(wp, "Box Equation", els, false, move |wp, els| {
                            let d = Document { els: els.clone(), ..Default::default() };
                            b2.equation = d.plain_text(0, els.len()).replace(['\n', '\u{c}'], " ");
                            wp.push(Box::new(DefScreen::new(b2, target)));
                        });
                        return Outcome::Close;
                    }
                    _ => {
                        let mut b2 = b;
                        let els = std::mem::take(&mut b2.text);
                        crate::subedit::open(wp, "Box", els, false, move |wp, els| {
                            b2.text = els;
                            b2.contents = Contents::Text;
                            wp.push(Box::new(DefScreen::new(b2, target)));
                        });
                        return Outcome::Close;
                    }
                }
            }
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        let b = &self.b;
        g.text(0, 0, &format!("Definition: {}", b.kind.name()), st);
        let anchor = match b.anchor {
            Anchor::Para => "Paragraph",
            Anchor::Page => "Page",
            Anchor::Char => "Character",
        };
        let vpos = match b.vpos {
            VPos::Full => "Full Page".into(),
            VPos::Top => "Top".into(),
            VPos::Center => "Center".into(),
            VPos::Bottom => "Bottom".into(),
            VPos::Baseline => "Baseline".into(),
            VPos::Set(v) => fmt_inches(v),
        };
        let hpos = if b.anchor == Anchor::Char {
            String::new()
        } else {
            let s = match b.hpos {
                HPos::Left => "Left".into(),
                HPos::Right => "Right".into(),
                HPos::Center => "Center".into(),
                HPos::Full => "Full".into(),
                HPos::Set(v) => format!("Set Position {}", fmt_inches(v)),
            };
            if b.anchor == Anchor::Page && !matches!(b.hpos, HPos::Set(_)) { format!("Margin, {s}") } else { s }
        };
        let cap = crate::reveal::preview(&expand_caption(&b.caption, &BoxOpts::default_for(b.kind), "1"));
        let contents = if b.contents == Contents::Image && b.data.is_none() { "Graphic on Disk" } else { b.contents.name() };
        let rows: [(char, &str, String); 9] = [
            ('1', "~Filename", b.file.clone()),
            ('2', "C~ontents", contents.into()),
            ('3', "~Caption", cap),
            ('4', "Anchor ~Type", anchor.into()),
            ('5', "~Vertical Position", vpos),
            ('6', "~Horizontal Position", hpos),
            ('7', "~Size", self.size_text()),
            ('8', "~Wrap Text Around Box", if b.wrap { "Yes".into() } else { "No".into() }),
            ('9', "~Edit", String::new()),
        ];
        for (k, (key, label, val)) in rows.iter().enumerate() {
            let v: String = val.chars().take(g.cols.saturating_sub(35)).collect();
            item_row(wp, g, 2 + 2 * k, *key, label, &v, 34);
        }
        match &self.ask {
            Some(a) => a.draw(wp, g),
            None => {
                let c = g.text(0, g.rows - 1, "Selection: 0", st);
                g.cursor = Some((c - 1, g.rows - 1));
            }
        }
    }
}

// ------------------------------------------------------------ Options screen

pub struct OptScreen {
    o: BoxOpts,
    ask: Option<Ask>,
    queue: Vec<u32>,
}

const BORDERS: [&str; 7] = BORDER_NAMES;

impl OptScreen {
    fn start(&mut self, step: u32) {
        let o = &self.o;
        let sides = ["Left", "Right", "Top", "Bottom"];
        self.ask = Some(match step {
            10..=13 => Ask::choice(step, "", &BORDERS),
            20..=23 => Ask::field(step, &format!("Outside Border Space - {}: ", sides[(step - 20) as usize]), &fmt_inches(o.outside[(step - 20) as usize])),
            30..=33 => Ask::field(step, &format!("Inside Border Space - {}: ", sides[(step - 30) as usize]), &fmt_inches(o.inside[(step - 30) as usize])),
            4 | 5 => Ask::choice(step, "", &NUM_NAMES),
            6 => Ask::field(6, "Caption Number Style: ", &o.cap_style),
            7 => Ask::choice(7, "Caption Position: ", &["Below Box", "Above Box"]),
            71 => Ask::choice(71, "Caption: ", &["Outside of Border", "Inside of Border"]),
            8 => Ask::field(8, "Minimum Offset from Paragraph: ", &fmt_inches(o.min_offset)),
            _ => Ask::field(9, "Gray Shading (% of black): ", &o.gray.to_string()),
        });
    }

    fn apply(&mut self, step: u32, a: Answer) {
        let o = &mut self.o;
        match (step, a) {
            (10..=13, Answer::Pick(k)) => o.border[(step - 10) as usize] = k as u8,
            (20..=23, Answer::Text(s)) => o.outside[(step - 20) as usize] = measure(&s, o.outside[(step - 20) as usize]).max(0),
            (30..=33, Answer::Text(s)) => o.inside[(step - 30) as usize] = measure(&s, o.inside[(step - 30) as usize]).max(0),
            (4, Answer::Pick(k)) => o.num1 = k as u8,
            (5, Answer::Pick(k)) => o.num2 = k as u8,
            (6, Answer::Text(s)) => o.cap_style = s,
            (7, Answer::Pick(k)) => o.cap_pos = k as u8,
            (71, Answer::Pick(k)) => o.cap_inside = k == 1,
            (8, Answer::Text(s)) => o.min_offset = measure(&s, o.min_offset).max(0),
            (9, Answer::Text(s)) => o.gray = s.trim().trim_end_matches('%').parse::<u32>().unwrap_or(o.gray as u32).min(100) as u8,
            _ => {}
        }
    }
}

impl Screen for OptScreen {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if let Some(ask) = &mut self.ask {
            if let Some(res) = ask.input(i) {
                let step = ask.step;
                self.ask = None;
                match res {
                    Some(a) => {
                        self.apply(step, a);
                        if let Some(s) = (!self.queue.is_empty()).then(|| self.queue.remove(0)) {
                            self.start(s);
                        }
                    }
                    None => self.queue.clear(),
                }
            }
            return Outcome::Keep;
        }
        let c = match i {
            Input::Char(c) => c.to_ascii_lowercase(),
            Input::Func(Func::Cancel | Func::Esc) => return Outcome::Close,
            Input::Func(Func::Exit | Func::Enter) => '0',
            _ => return Outcome::Keep,
        };
        match c {
            '0' | ' ' => {
                wp.ed().insert_code(Code::BoxOpt(Box::new(self.o.clone())));
                wp.after_edit();
                return Outcome::Close;
            }
            '1' | 'b' => {
                self.queue = vec![11, 12, 13];
                self.start(10);
            }
            '2' | 'o' => {
                self.queue = vec![21, 22, 23];
                self.start(20);
            }
            '3' | 'i' => {
                self.queue = vec![31, 32, 33];
                self.start(30);
            }
            '4' | 'f' => self.start(4),
            '5' | 's' => self.start(5),
            '6' | 'c' => self.start(6),
            '7' | 'p' => {
                self.queue = vec![71];
                self.start(7);
            }
            '8' | 'm' => self.start(8),
            '9' | 'g' => self.start(9),
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        let o = &self.o;
        g.text(0, 0, &format!("Options: {}", o.kind.name()), st);
        let sides = ["Left", "Right", "Top", "Bottom"];
        let mut row = 2;
        let vcol = 40;
        let groups: [(char, &str, Vec<String>); 3] = [
            ('1', "~Border Style", o.border.iter().map(|b| BORDER_NAMES[(*b as usize).min(6)].to_string()).collect()),
            ('2', "~Outside Border Space", o.outside.iter().map(|v| fmt_inches(*v)).collect()),
            ('3', "~Inside Border Space", o.inside.iter().map(|v| fmt_inches(*v)).collect()),
        ];
        for (key, label, vals) in groups.iter() {
            item_row(wp, g, row, *key, label, "", vcol);
            row += 1;
            for (k, s) in sides.iter().enumerate() {
                g.text(14, row, s, st);
                g.text(vcol, row, &vals[k], st);
                row += 1;
            }
        }
        let cap_pos = format!("{} box, {} borders", if o.cap_pos == 1 { "Above" } else { "Below" }, if o.cap_inside { "Inside" } else { "Outside" });
        let rest: [(char, &str, String); 6] = [
            ('4', "~First Level Numbering Method", NUM_NAMES[(o.num1 as usize).min(3)].into()),
            ('5', "~Second Level Numbering Method", NUM_NAMES[(o.num2 as usize).min(3)].into()),
            ('6', "~Caption Number Style", o.cap_style.clone()),
            ('7', "~Position of Caption", cap_pos),
            ('8', "~Minimum Offset from Paragraph", fmt_inches(o.min_offset)),
            ('9', "~Gray Shading (% of black)", format!("{}%", o.gray)),
        ];
        for (key, label, val) in rest.iter() {
            if row + 1 >= g.rows {
                break;
            }
            item_row(wp, g, row, *key, label, val, vcol);
            row += 1;
        }
        match &self.ask {
            Some(a) => a.draw(wp, g),
            None => {
                let c = g.text(0, g.rows - 1, "Selection: 0", st);
                g.cursor = Some((c - 1, g.rows - 1));
            }
        }
    }
}

// ------------------------------------------------------------ lines

fn line_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Horizontal", "~Vertical", "Edit Line: Horizontal", "Edit Line: Vertical"], |wp, k| {
        let vertical = k % 2 == 1;
        if k < 2 {
            let d = &wp.ed_ref().doc;
            let fs = wp.ed_ref().format_state_at(wp.ed_ref().cursor);
            let len = if vertical { d.init.page_height - d.init.top - d.init.bottom } else { d.init.page_width - fs.left - fs.right };
            return Some(Box::new(LineScreen { l: GLine::new(vertical, len), target: None, ask: None }) as Box<dyn Screen>);
        }
        let e = wp.ed_ref();
        let is = |el: &El| matches!(el, El::Code(Code::GLine(l)) if l.vertical == vertical);
        let found = e.doc.els[..e.cursor].iter().rposition(is).or_else(|| e.doc.els[e.cursor..].iter().position(is).map(|p| p + e.cursor));
        let Some(idx) = found else {
            wp.flash(&format!("ERROR: {} line not found", if vertical { "Vertical" } else { "Horizontal" }));
            return None;
        };
        let El::Code(Code::GLine(l)) = &e.doc.els[idx] else { return None };
        Some(Box::new(LineScreen { l: l.clone(), target: Some(idx), ask: None }) as Box<dyn Screen>)
    })
    .prefix("Create Line: ")
}

pub struct LineScreen {
    l: GLine,
    target: Option<usize>,
    ask: Option<Ask>,
}

impl LineScreen {
    fn start(&mut self, step: u32) {
        let l = &self.l;
        self.ask = Some(match (step, l.vertical) {
            (1, false) => Ask::choice(1, "Horizontal Position: ", &["Left", "Right", "Center", "Full", "Set Position"]),
            (1, true) => Ask::choice(1, "Horizontal Position: ", &["Left", "Right", "Between Columns", "Set Position"]),
            (11, _) => Ask::field(11, "Offset from left of page: ", &fmt_inches(l.x)),
            (2, false) => Ask::choice(2, "Vertical Position: ", &["Baseline", "Set Position"]),
            (2, true) => Ask::choice(2, "Vertical Position: ", &["Full Page", "Top", "Center", "Bottom", "Set Position"]),
            (21, _) => Ask::field(21, "Offset from top of page: ", &fmt_inches(l.y)),
            (3, _) => Ask::field(3, "Length of Line: ", &fmt_inches(l.length)),
            (4, _) => Ask::field(4, "Width of Line: ", &fmt_fine(l.width)),
            _ => Ask::field(5, "Gray Shading (% of black): ", &l.gray.to_string()),
        });
    }

    /// Returns a follow-up step.
    fn apply(&mut self, step: u32, a: Answer) -> Option<u32> {
        let l = &mut self.l;
        match (step, a) {
            (1, Answer::Pick(k)) => {
                let set = if l.vertical { 3 } else { 4 };
                l.halign = (k as u8).min(set);
                if l.halign == set {
                    return Some(11);
                }
            }
            (11, Answer::Text(s)) => l.x = measure(&s, l.x),
            (2, Answer::Pick(k)) => {
                l.valign = if l.vertical { k as u8 } else if k == 0 { 0 } else { 4 };
                if l.valign == 4 {
                    return Some(21);
                }
            }
            (21, Answer::Text(s)) => l.y = measure(&s, l.y),
            (3, Answer::Text(s)) => l.length = measure(&s, l.length).max(0),
            (4, Answer::Text(s)) => l.width = measure(&s, l.width).max(1),
            (5, Answer::Text(s)) => l.gray = s.trim().trim_end_matches('%').parse::<u32>().unwrap_or(l.gray as u32).min(100) as u8,
            _ => {}
        }
        None
    }
}

impl Screen for LineScreen {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if let Some(ask) = &mut self.ask {
            if let Some(res) = ask.input(i) {
                let step = ask.step;
                self.ask = None;
                if let Some(a) = res {
                    if let Some(n) = self.apply(step, a) {
                        self.start(n);
                    }
                }
            }
            return Outcome::Keep;
        }
        let c = match i {
            Input::Char(c) => c.to_ascii_lowercase(),
            Input::Func(Func::Cancel | Func::Esc) => return Outcome::Close,
            Input::Func(Func::Exit | Func::Enter) => '0',
            _ => return Outcome::Keep,
        };
        match c {
            '0' | ' ' => {
                let code = Code::GLine(self.l.clone());
                let e = wp.ed();
                match self.target {
                    Some(idx) if idx < e.doc.els.len() => {
                        e.delete_range(idx, idx + 1, false);
                        e.cursor = idx;
                        e.insert_code(code);
                    }
                    _ => e.insert_code(code),
                }
                wp.after_edit();
                return Outcome::Close;
            }
            '1' | 'h' => self.start(1),
            '2' | 'v' => self.start(2),
            '3' | 'l' => self.start(3),
            '4' | 'w' => self.start(4),
            '5' | 'g' => self.start(5),
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        let l = &self.l;
        g.text(0, 0, if l.vertical { "Graphics: Vertical Line" } else { "Graphics: Horizontal Line" }, st);
        let len = if l.length > 0 { fmt_inches(l.length) } else { "Auto".into() };
        let rows: [(char, &str, String); 5] = [
            ('1', "~Horizontal Position", l.halign_name()),
            ('2', "~Vertical Position", l.valign_name()),
            ('3', "~Length of Line", len),
            ('4', "~Width of Line", fmt_fine(l.width)),
            ('5', "~Gray Shading (% of black)", format!("{}%", l.gray)),
        ];
        for (k, (key, label, val)) in rows.iter().enumerate() {
            item_row(wp, g, 2 + 2 * k, *key, label, val, 40);
        }
        match &self.ask {
            Some(a) => a.draw(wp, g),
            None => {
                let c = g.text(0, g.rows - 1, "Selection: 0", st);
                g.cursor = Some((c - 1, g.rows - 1));
            }
        }
    }
}

// ------------------------------------------------------------ image editor

pub struct ImageEdit {
    b: GBox,
    target: Option<usize>,
    /// step in percent (move/scale) or degrees (rotate)
    step: i32,
    ask: Option<Ask>,
    queue: Vec<u32>,
}

const STEPS: [i32; 4] = [1, 5, 10, 25];

impl ImageEdit {
    pub fn new(b: GBox, target: Option<usize>) -> ImageEdit {
        ImageEdit { b, target, step: 10, ask: None, queue: vec![] }
    }

    fn start(&mut self, step: u32) {
        let im = &self.b.img;
        self.ask = Some(match step {
            1 => Ask::field(1, "Horizontal: ", &fmt_inches(im.dx)),
            2 => Ask::field(2, "Vertical: ", &fmt_inches(im.dy)),
            3 => Ask::field(3, "Scale X: ", &im.sx.to_string()),
            4 => Ask::field(4, "Scale Y: ", &im.sy.to_string()),
            5 => Ask::field(5, "Enter number of degrees (0-360): ", &im.rot.to_string()),
            _ => Ask::choice(6, "Mirror image: ", &["No", "Yes"]),
        });
    }

    fn apply(&mut self, step: u32, a: Answer) {
        let im = &mut self.b.img;
        let num = |s: &str, d: i32| s.trim().trim_end_matches('%').parse::<i32>().unwrap_or(d);
        match (step, a) {
            (1, Answer::Text(s)) => im.dx = measure(&s, im.dx),
            (2, Answer::Text(s)) => im.dy = measure(&s, im.dy),
            (3, Answer::Text(s)) => im.sx = num(&s, im.sx).clamp(1, 10000),
            (4, Answer::Text(s)) => im.sy = num(&s, im.sy).clamp(1, 10000),
            (5, Answer::Text(s)) => im.rot = num(&s, im.rot).rem_euclid(360),
            (6, Answer::Pick(k)) => im.mirror = k == 1,
            _ => {}
        }
    }
}

impl Screen for ImageEdit {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, _wp: &mut Wp, i: &Input) -> Outcome {
        if let Some(ask) = &mut self.ask {
            if let Some(res) = ask.input(i) {
                let step = ask.step;
                self.ask = None;
                match res {
                    Some(a) => {
                        self.apply(step, a);
                        if let Some(s) = (!self.queue.is_empty()).then(|| self.queue.remove(0)) {
                            self.start(s);
                        }
                    }
                    None => self.queue.clear(),
                }
            }
            return Outcome::Keep;
        }
        let (bw, bh) = (self.b.width, self.b.height);
        let st = self.step;
        let im = &mut self.b.img;
        match i {
            Input::Func(Func::Exit | Func::Enter | Func::Cancel | Func::Esc) | Input::Char('0') => {
                return Outcome::Replace(Box::new(DefScreen::new(self.b.clone(), self.target)));
            }
            Input::Func(Func::Left) => im.dx -= bw * st / 100,
            Input::Func(Func::Right) => im.dx += bw * st / 100,
            Input::Func(Func::Up) => im.dy += bh * st / 100,
            Input::Func(Func::Down) => im.dy -= bh * st / 100,
            Input::Func(Func::PgUp) => {
                im.sx = (im.sx + st).min(10000);
                im.sy = (im.sy + st).min(10000);
            }
            Input::Func(Func::PgDn) => {
                im.sx = (im.sx - st).max(1);
                im.sy = (im.sy - st).max(1);
            }
            Input::Char('+') | Input::Func(Func::ScreenDown) => im.rot = (im.rot - st).rem_euclid(360),
            Input::Char('-') | Input::Func(Func::ScreenUp) => im.rot = (im.rot + st).rem_euclid(360),
            Input::Func(Func::Typeover) => {
                let k = STEPS.iter().position(|s| *s == self.step).unwrap_or(2);
                self.step = STEPS[(k + 1) % STEPS.len()];
            }
            Input::Func(Func::GoTo) | Input::Func(Func::Home) => *im = ImgEdit::default(),
            Input::Char('1' | 'm' | 'M') => {
                self.queue = vec![2];
                self.start(1);
            }
            Input::Char('2' | 's' | 'S') => {
                self.queue = vec![4];
                self.start(3);
            }
            Input::Char('3' | 'r' | 'R') => {
                self.queue = vec![6];
                self.start(5);
            }
            Input::Char('4' | 'i' | 'I') => im.invert = !im.invert,
            Input::Char('5' | 'b' | 'B') => im.bw = !im.bw,
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        match &self.ask {
            Some(a) => a.draw(wp, g),
            None => {
                let row = g.rows - 1;
                let c = g.text_mn(0, row, "1 ~Move; 2 ~Scale; 3 ~Rotate; 4 ~Invert; 5 ~B&W: ", st, wp.theme.bold);
                g.text(c, row, "0", st);
                let right = format!("({}%)", self.step);
                g.text(g.cols.saturating_sub(right.len() + 1), row, &right, st);
                g.cursor = Some((c, row));
            }
        }
    }

    fn draw_pixels(&self, _wp: &Wp, buf: &mut [u32], w: usize, h: usize, cell_h: usize) -> bool {
        let avail_h = h.saturating_sub(cell_h + 4);
        for p in buf[..w * avail_h.min(h)].iter_mut() {
            *p = 0x202020;
        }
        let (bw, bh) = (self.b.width.max(1) as f64, self.b.height.max(1) as f64);
        let k = ((w as f64 - 40.0) / bw).min((avail_h as f64 - 40.0) / bh).max(0.01);
        let (pw, ph) = ((bw * k) as usize, (bh * k) as usize);
        let (x0, y0) = ((w - pw.min(w)) / 2, (avail_h - ph.min(avail_h)) / 2);
        for y in y0..(y0 + ph).min(avail_h) {
            for x in x0..(x0 + pw).min(w) {
                buf[y * w + x] = 0xFFFFFF;
            }
        }
        let Some(data) = self.b.data.as_ref() else { return true };
        let Some(r) = crate::raster::decode(data) else { return true };
        let (nw, nh) = crate::raster::dims(data).unwrap_or((r.w as u32, r.h as u32));
        let (fw, fh) = (self.b.width, self.b.height);
        let kk = (fw as f64 / nw.max(1) as f64).min(fh as f64 / nh.max(1) as f64);
        let im = &self.b.img;
        let dw = (nw as f64 * kk * im.sx as f64 / 100.0) as i32;
        let dh = (nh as f64 * kk * im.sy as f64 / 100.0) as i32;
        let p = crate::raster::Placed {
            clip: (0, 0, fw, fh),
            img: Some(r),
            color: 0,
            dst: ((fw - dw) / 2 + im.dx, (fh - dh) / 2 - im.dy, dw.max(1), dh.max(1)),
            rot: im.rot,
            mirror: im.mirror,
            invert: im.invert,
            bw: im.bw,
        };
        let px = p.compose(pw, ph);
        for y in 0..ph {
            for x in 0..pw {
                let (bx, by) = (x0 + x, y0 + y);
                if bx < w && by < avail_h {
                    buf[by * w + bx] = crate::raster::over(buf[by * w + bx], px[y * pw + x]);
                }
            }
        }
        true
    }
}
