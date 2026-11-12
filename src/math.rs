//! Math: Table Edit, 5 Math (cell formulas, `+` subtotal, `=` total, `*`
//! grand total, Copy Formula, Calculate) and Alt-F7, 3 Math columns
//! (`[Math Def]`, `[Math On]`/`[Math Off]`, operators `+ = * t T N !`).
//!
//! Like WP 5.1, formulas are calculated strictly from left to right
//! (`A1+B1*2` = (A1+B1)×2); use parentheses to change the order.

use crate::doc::*;
use crate::editor::Editor;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::tables::{self, Table};
use crate::ui::{Outcome, Screen, Wp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ============================================================ expressions

#[derive(Debug, Clone, PartialEq)]
enum T {
    Num(f64),
    Ref(String),
    Op(char),
}

fn tokens(f: &str) -> Option<Vec<T>> {
    let cs: Vec<char> = f.chars().collect();
    let mut v = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' || c == ',' {
            let mut s = String::new();
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.' || cs[i] == ',') {
                s.push(if cs[i] == ',' { '.' } else { cs[i] });
                i += 1;
            }
            v.push(T::Num(s.parse().ok()?));
        } else if c.is_ascii_alphabetic() {
            let mut s = String::new();
            while i < cs.len() && cs[i].is_ascii_alphanumeric() {
                s.push(cs[i].to_ascii_uppercase());
                i += 1;
            }
            v.push(T::Ref(s));
        } else if "+-*/()".contains(c) {
            v.push(T::Op(c));
            i += 1;
        } else {
            return None;
        }
    }
    Some(v)
}

struct Lr<'a> {
    t: Vec<T>,
    i: usize,
    get: &'a dyn Fn(&str) -> Option<f64>,
}

impl Lr<'_> {
    fn operand(&mut self) -> Option<f64> {
        match self.t.get(self.i).cloned()? {
            T::Num(n) => {
                self.i += 1;
                Some(n)
            }
            T::Ref(r) => {
                self.i += 1;
                Some((self.get)(&r).unwrap_or(0.0))
            }
            T::Op('-') => {
                self.i += 1;
                Some(-self.operand()?)
            }
            T::Op('+') => {
                self.i += 1;
                self.operand()
            }
            T::Op('(') => {
                self.i += 1;
                let v = self.seq()?;
                if self.t.get(self.i) != Some(&T::Op(')')) {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            _ => None,
        }
    }
    /// operand (op operand)* — strictly left to right
    fn seq(&mut self) -> Option<f64> {
        let mut acc = self.operand()?;
        while let Some(T::Op(op)) = self.t.get(self.i).cloned() {
            if op == ')' {
                break;
            }
            self.i += 1;
            let b = self.operand()?;
            acc = match op {
                '+' => acc + b,
                '-' => acc - b,
                '*' => acc * b,
                '/' => {
                    if b == 0.0 {
                        return None;
                    }
                    acc / b
                }
                _ => return None,
            };
        }
        Some(acc)
    }
}

/// Evaluate a WP math formula left to right; references resolved by `get`.
pub fn eval_lr(f: &str, get: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let t = tokens(f)?;
    if t.is_empty() {
        return None;
    }
    let mut p = Lr { t, i: 0, get };
    let v = p.seq()?;
    (p.i == p.t.len()).then_some(v)
}

/// "B12" → (row 11, col 1).
pub fn parse_ref(s: &str) -> Option<(usize, usize)> {
    let s = s.trim().to_ascii_uppercase();
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &s[letters.len()..];
    if letters.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut col = 0usize;
    for c in letters.chars() {
        col = col * 26 + (c as usize - 'A' as usize + 1);
    }
    let row: usize = digits.parse().ok()?;
    (row >= 1).then(|| (row - 1, col - 1))
}

/// Shift the cell references of a formula (Copy Formula).
pub fn shift_formula(f: &str, dr: isize, dc: isize) -> String {
    let cs: Vec<char> = f.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i].is_ascii_alphabetic() {
            let mut s = String::new();
            while i < cs.len() && cs[i].is_ascii_alphanumeric() {
                s.push(cs[i]);
                i += 1;
            }
            match parse_ref(&s) {
                Some((r, c)) => {
                    let nr = (r as isize + dr).max(0) as usize;
                    let nc = (c as isize + dc).max(0) as usize;
                    out.push_str(&format!("{}{}", tables::col_letter(nc), nr + 1));
                }
                None => out.push_str(&s),
            }
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

/// Format a result: `digits` decimals, minus sign or parentheses.
pub fn fmt_num(v: f64, digits: u8, paren: bool) -> String {
    let d = digits.min(4) as usize;
    let s = format!("{:.d$}", v.abs());
    let neg = v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0');
    match (neg, paren) {
        (true, true) => format!("({s})"),
        (true, false) => format!("-{s}"),
        _ => s,
    }
}

fn number_of(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() || !t.chars().any(|c| c.is_ascii_digit()) || t.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    crate::sort::parse_number(t)
}

fn chars_of(els: &[El]) -> String {
    els.iter()
        .filter_map(|e| match e {
            El::Ch(c) => Some(*c),
            El::Code(Code::HardSpace) => Some(' '),
            _ => None,
        })
        .collect()
}

// ============================================================ table math

fn cell_formula(els: &[El], t: &Table, r: usize, c: usize) -> Option<String> {
    let (a, b) = t.cell_range(els, r, c);
    els[a..b].iter().find_map(|e| match e {
        El::Code(Code::Formula(f)) => Some(f.clone()),
        _ => None,
    })
}

/// Set (or with None remove) the formula of a cell.
fn set_formula(els: &mut Vec<El>, t: &Table, r: usize, c: usize, f: Option<String>) {
    let (a, b) = t.cell_range(els, r, c);
    if let Some(k) = (a..b).find(|&k| matches!(els[k], El::Code(Code::Formula(_)))) {
        els.remove(k);
    }
    if let Some(f) = f.filter(|f| !f.trim().is_empty()) {
        els.insert(a, El::Code(Code::Formula(f.trim().to_ascii_uppercase())));
    }
}

/// Recalculate every formula of the table whose [Tbl Def] is at `def`.
pub fn calc_table(els: &mut Vec<El>, def: usize) {
    let Some(t) = tables::parse(els, def) else { return };
    let mut vals: HashMap<(usize, usize), f64> = HashMap::new();
    let mut forms: HashMap<(usize, usize), String> = HashMap::new();
    for (r, row) in t.rows.iter().enumerate() {
        for c in 0..row.len() {
            let (a, b) = t.cell_range(els, r, c);
            if let Some(f) = cell_formula(els, &t, r, c) {
                forms.insert((r, c), f);
            } else if let Some(v) = number_of(&chars_of(&els[a..b])) {
                vals.insert((r, c), v);
            }
        }
    }
    let mut results: HashMap<(usize, usize), Option<f64>> = HashMap::new();
    for _pass in 0..2 {
        for (r, row) in t.rows.iter().enumerate() {
            for c in 0..row.len() {
                let Some(f) = forms.get(&(r, c)) else { continue };
                let kind = |rr: usize| forms.get(&(rr, c)).map(|s| s.as_str());
                let v = match f.as_str() {
                    "+" => {
                        let mut s = 0.0;
                        for rr in (0..r).rev() {
                            match kind(rr) {
                                Some("+") => break,
                                Some("=") | Some("*") => {}
                                _ => s += vals.get(&(rr, c)).copied().unwrap_or(0.0),
                            }
                        }
                        Some(s)
                    }
                    "=" => {
                        let mut s = 0.0;
                        for rr in (0..r).rev() {
                            match kind(rr) {
                                Some("=") => break,
                                Some("+") => s += vals.get(&(rr, c)).copied().unwrap_or(0.0),
                                _ => {}
                            }
                        }
                        Some(s)
                    }
                    "*" => {
                        let mut s = 0.0;
                        for rr in (0..r).rev() {
                            match kind(rr) {
                                Some("*") => break,
                                Some("=") => s += vals.get(&(rr, c)).copied().unwrap_or(0.0),
                                _ => {}
                            }
                        }
                        Some(s)
                    }
                    f => eval_lr(f, &|name| parse_ref(name).and_then(|rc| vals.get(&rc).copied())),
                };
                match v {
                    Some(x) => {
                        vals.insert((r, c), x);
                    }
                    None => {
                        vals.remove(&(r, c));
                    }
                }
                results.insert((r, c), v);
            }
        }
    }
    // write the values, last cell first (indices shift)
    let mut cells: Vec<_> = results.into_iter().collect();
    cells.sort_by(|a, b| b.0.cmp(&a.0));
    for ((r, c), v) in cells {
        let (a, b) = t.cell_range(els, r, c);
        let text = match v {
            Some(x) => fmt_num(x, 2, false),
            None => "??".into(),
        };
        let mut keep: Vec<El> = els[a..b].iter().filter(|e| matches!(e, El::Code(c) if !matches!(c, Code::HRt | Code::Tab | Code::HardSpace | Code::HardHyphen))).cloned().collect();
        let at = keep.iter().position(|e| matches!(e, El::Code(Code::Formula(_)))).map(|k| k + 1).unwrap_or(0);
        let tail = keep.split_off(at);
        keep.extend(text.chars().map(El::Ch));
        keep.extend(tail);
        els.splice(a..b, keep);
    }
}

fn cur_table(wp: &Wp) -> Option<(Table, usize, usize)> {
    let e = wp.ed_ref();
    let t = tables::table_at(&e.doc.els, e.cursor)?;
    let (r, c) = t.locate(e.cursor)?;
    Some((t, r, c))
}

/// Apply a change to the table at the cursor, recalculate, keep the cell.
fn edit_table(wp: &mut Wp, f: impl FnOnce(&mut Vec<El>, &Table, usize, usize)) {
    let Some((t, r, c)) = cur_table(wp) else { return };
    let e = wp.ed();
    let mut els = e.doc.els.clone();
    f(&mut els, &t, r, c);
    calc_table(&mut els, t.def);
    e.set_els(els);
    if let Some(t) = tables::parse(&e.doc.els, t.def) {
        tables::goto_cell(e, &t, r, c);
    }
    wp.after_edit();
}

/// Table Edit, 5 Math.
pub fn table_math_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Calculate", "~Formula", "Co~py Formula", "+", "=", "*"], |wp, k| match k {
        0 => {
            edit_table(wp, |_, _, _, _| {});
            None
        }
        1 => {
            let cur = cur_table(wp).and_then(|(t, r, c)| cell_formula(&wp.ed_ref().doc.els, &t, r, c)).unwrap_or_default();
            Some(Prompt::new("Formula:", &cur, |wp, s| {
                let s = s?;
                if !s.trim().is_empty() && !matches!(s.trim(), "+" | "=" | "*") && eval_lr(&s, &|_| Some(1.0)).is_none() {
                    wp.flash("ERROR: Invalid formula");
                    return None;
                }
                edit_table(wp, move |els, t, r, c| set_formula(els, t, r, c, Some(s)));
                None
            }) as Box<dyn Screen>)
        }
        2 => {
            let src = cur_table(wp).and_then(|(t, r, c)| cell_formula(&wp.ed_ref().doc.els, &t, r, c).map(|f| (f, r, c)));
            let Some((f, r0, c0)) = src else {
                wp.flash("ERROR: No formula in this cell");
                return None;
            };
            Some(MenuLine::new(&["~Cell", "~Down", "~Right"], move |_wp, k| {
                if k == 0 {
                    return Some(Prompt::new("Destination cell:", "", move |wp, s| {
                        let (r, c) = parse_ref(&s?)?;
                        let nf = shift_formula(&f, r as isize - r0 as isize, c as isize - c0 as isize);
                        edit_table(wp, move |els, t, _, _| {
                            if r < t.rows.len() && c < t.rows[r].len() {
                                set_formula(els, t, r, c, Some(nf));
                            }
                        });
                        None
                    }) as Box<dyn Screen>);
                }
                Some(Prompt::new("Number of times to copy formula:", "1", move |wp, s| {
                    let n: usize = s?.trim().parse().ok()?;
                    edit_table(wp, move |els, t, _, _| {
                        // from the far end backwards, so indices stay valid
                        for i in (1..=n).rev() {
                            let (r, c) = if k == 1 { (r0 + i, c0) } else { (r0, c0 + i) };
                            if r < t.rows.len() && c < t.rows[r].len() {
                                let (dr, dc) = (r as isize - r0 as isize, c as isize - c0 as isize);
                                let nf = if matches!(f.as_str(), "+" | "=" | "*") { f.clone() } else { shift_formula(&f, dr, dc) };
                                set_formula(els, t, r, c, Some(nf));
                            }
                        }
                    });
                    None
                }) as Box<dyn Screen>)
            }).prefix("Copy Formula To: ") as Box<dyn Screen>)
        }
        _ => {
            let f = ["+", "=", "*"][k - 3].to_string();
            edit_table(wp, move |els, t, r, c| set_formula(els, t, r, c, Some(f)));
            None
        }
    })
    .prefix("Math: ")
}

// ============================================================ math columns

/// Is `pos` inside a [Math On] … [Math Off] area?
pub fn in_math_area(els: &[El], pos: usize) -> bool {
    els[..pos.min(els.len())].iter().rev().find_map(|e| match e {
        El::Code(Code::Math(on)) => Some(*on),
        _ => None,
    }) == Some(true)
}

/// Typing `+ = * t T N !` right after a tab in a math area inserts the
/// math operator.  Returns true if it did.
pub fn type_operator(e: &mut Editor, c: char) -> bool {
    if !"+=*tTN!".contains(c) || e.block_anchor.is_some() || e.cursor == 0 {
        return false;
    }
    if !matches!(e.doc.els.get(e.cursor - 1), Some(El::Code(Code::Tab))) || !in_math_area(&e.doc.els, e.cursor) {
        return false;
    }
    e.insert_code(Code::MathOp(c));
    true
}

fn math_def_at(els: &[El], pos: usize) -> Vec<MathCol> {
    els[..pos.min(els.len())]
        .iter()
        .rev()
        .find_map(|e| match e {
            El::Code(Code::MathDef(d)) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(MathCol::defaults)
}

#[derive(Default, Clone, Copy)]
struct Acc {
    sub: f64,
    tot: f64,
    grand: f64,
}

/// Calculate every math area: results go before their `+ = * !` operator.
pub fn calc_columns(els: &mut Vec<El>) {
    // collect lines inside math areas
    let mut def = MathCol::defaults();
    let mut on = false;
    let mut edits: Vec<(usize, usize, String)> = vec![]; // (from, to) chars replaced by text
    let mut acc: Vec<Acc> = vec![Acc::default(); 24];
    let mut i = 0;
    let n = els.len();
    while i < n {
        match &els[i] {
            El::Code(Code::MathDef(d)) => {
                def = d.clone();
                i += 1;
                continue;
            }
            El::Code(Code::Math(o)) => {
                on = *o;
                if on {
                    acc = vec![Acc::default(); 24];
                }
                i += 1;
                continue;
            }
            _ => {}
        }
        if !on {
            i += 1;
            continue;
        }
        // one line: i .. next HRt / Math code
        let mut j = i;
        while j < n && !matches!(els[j], El::Code(Code::HRt | Code::HPg | Code::Math(_) | Code::MathDef(_))) {
            j += 1;
        }
        // segments after each tab
        let tabs: Vec<usize> = (i..j).filter(|&k| matches!(els[k], El::Code(Code::Tab))).collect();
        let mut line_vals: HashMap<usize, f64> = HashMap::new();
        let segs: Vec<(usize, usize, usize)> = tabs
            .iter()
            .enumerate()
            .map(|(k, &t)| (k, t + 1, tabs.get(k + 1).copied().unwrap_or(j)))
            .filter(|(k, _, _)| *k < 24)
            .collect();
        let kind_of = |k: usize| def.get(k).map(|d| d.kind).unwrap_or(2);
        let add_num = |acc: &mut Vec<Acc>, k: usize, v: f64, level: u8| {
            for kk in [k, k + 1] {
                if kk == k || kind_of(kk) == 3 {
                    if let Some(a) = acc.get_mut(kk) {
                        match level {
                            0 => a.sub += v,
                            1 => a.tot += v,
                            _ => a.grand += v,
                        }
                    }
                }
            }
        };
        for calc_pass in [false, true] {
            for &(k, s, e) in &segs {
                let kind = kind_of(k);
                if kind == 1 || (kind == 0) != calc_pass {
                    continue;
                }
                let op = (s..e).find_map(|x| match els[x] {
                    El::Code(Code::MathOp(c)) => Some((x, c)),
                    _ => None,
                });
                let num = number_of(&chars_of(&els[s..e]));
                let d = def.get(k).cloned().unwrap_or(MathCol { kind: 2, paren: false, digits: 2, formula: String::new() });
                let result = match op {
                    None => {
                        if let Some(v) = num {
                            line_vals.insert(k, v);
                            add_num(&mut acc, k, v, 0);
                        }
                        None
                    }
                    Some((_, 'N')) => {
                        let v = -num.unwrap_or(0.0);
                        line_vals.insert(k, v);
                        add_num(&mut acc, k, v, 0);
                        None
                    }
                    Some((_, 't')) => {
                        let v = num.unwrap_or(0.0);
                        line_vals.insert(k, v);
                        add_num(&mut acc, k, v, 1);
                        None
                    }
                    Some((_, 'T')) => {
                        let v = num.unwrap_or(0.0);
                        line_vals.insert(k, v);
                        add_num(&mut acc, k, v, 2);
                        None
                    }
                    Some((x, '+')) => {
                        let v = acc[k].sub;
                        acc[k].sub = 0.0;
                        add_num(&mut acc, k, v, 1);
                        Some((x, v))
                    }
                    Some((x, '=')) => {
                        let v = acc[k].tot;
                        acc[k].tot = 0.0;
                        add_num(&mut acc, k, v, 2);
                        Some((x, v))
                    }
                    Some((x, '*')) => {
                        let v = acc[k].grand;
                        acc[k].grand = 0.0;
                        Some((x, v))
                    }
                    Some((x, '!')) => {
                        let f = d.formula.trim().to_ascii_uppercase();
                        let cols = |want: u8| -> Vec<f64> { line_vals.iter().filter(|(kk, _)| kind_of(**kk) == want).map(|(_, v)| *v).collect() };
                        let avg = |v: Vec<f64>| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
                        let v = match f.as_str() {
                            "+" => Some(cols(2).iter().sum()),
                            "=" => Some(cols(3).iter().sum()),
                            "+/" => Some(avg(cols(2))),
                            "=/" => Some(avg(cols(3))),
                            f => eval_lr(f, &|name| {
                                let c = name.chars().next()?;
                                (name.len() == 1).then(|| line_vals.get(&((c as u8 - b'A') as usize)).copied().unwrap_or(0.0))
                            }),
                        };
                        let v = v.unwrap_or(0.0);
                        line_vals.insert(k, v);
                        add_num(&mut acc, k, v, 0);
                        Some((x, v))
                    }
                    _ => None,
                };
                if let Some((x, v)) = result {
                    edits.push((s, x, fmt_num(v, d.digits, d.paren)));
                }
            }
        }
        i = j + 1;
    }
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    for (s, x, text) in edits {
        let mut keep: Vec<El> = els[s..x].iter().filter(|e| !e.is_char()).cloned().collect();
        let mut v: Vec<El> = text.chars().map(El::Ch).collect();
        v.append(&mut keep);
        els.splice(s..x, v);
    }
}

/// Alt-F7, 3 Math.
pub fn math_columns_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~On", "O~ff", "~Define", "~Calculate"], |wp, k| match k {
        0 | 1 => {
            wp.ed().insert_code(Code::Math(k == 0));
            wp.after_edit();
            None
        }
        2 => {
            let e = wp.ed_ref();
            let cols = math_def_at(&e.doc.els, e.cursor);
            Some(Box::new(MathDefScreen { cols: Rc::new(RefCell::new(cols)), row: 0, col: 0 }) as Box<dyn Screen>)
        }
        _ => {
            let e = wp.ed();
            let mut els = e.doc.els.clone();
            calc_columns(&mut els);
            let cur = e.cursor;
            e.set_els(els);
            e.cursor = cur.min(e.doc.els.len());
            wp.after_edit();
            None
        }
    })
    .prefix("Math: ")
}

/// The Math Definition screen.
struct MathDefScreen {
    cols: Rc<RefCell<Vec<MathCol>>>,
    /// 0 type, 1 negative numbers, 2 digits
    row: usize,
    col: usize,
}

impl Screen for MathDefScreen {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Exit) => {
                let cols = self.cols.borrow().clone();
                let e = wp.ed();
                if e.cursor > 0 && matches!(e.doc.els.get(e.cursor - 1), Some(El::Code(Code::MathDef(_)))) {
                    let k = e.cursor - 1;
                    let mut els = e.doc.els.clone();
                    els[k] = El::Code(Code::MathDef(cols));
                    e.set_els(els);
                } else {
                    e.insert_code(Code::MathDef(cols));
                }
                wp.after_edit();
                return Outcome::CloseAll;
            }
            Input::Func(Func::Cancel) => return Outcome::Close,
            Input::Func(Func::Left) => self.col = self.col.saturating_sub(1),
            Input::Func(Func::Right) | Input::Func(Func::Tab) => self.col = (self.col + 1).min(23),
            Input::Func(Func::Up) => self.row = self.row.saturating_sub(1),
            Input::Func(Func::Down) | Input::Func(Func::Enter) => self.row = (self.row + 1).min(2),
            Input::Func(Func::Home) => self.col = 0,
            Input::Func(Func::End) => self.col = 23,
            Input::Char(c) => {
                let mut cols = self.cols.borrow_mut();
                let col = self.col;
                match (self.row, *c) {
                    (0, '0') => {
                        let calc = cols.iter().filter(|x| x.kind == 0).count();
                        if cols[col].kind != 0 && calc >= 4 {
                            wp.flash("ERROR: Only 4 calculation columns");
                            return Outcome::Keep;
                        }
                        cols[col].kind = 0;
                        let cur = cols[col].formula.clone();
                        let shared = self.cols.clone();
                        let label = format!("Formula for column {}:", (b'A' + col as u8) as char);
                        return Outcome::Push(
                            Prompt::new(&label, &cur, move |wp, s| {
                                if let Some(s) = s {
                                    let ok = matches!(s.trim(), "+" | "=" | "+/" | "=/") || eval_lr(&s, &|_| Some(1.0)).is_some();
                                    if ok {
                                        shared.borrow_mut()[col].formula = s.trim().to_ascii_uppercase();
                                    } else {
                                        wp.flash("ERROR: Invalid formula");
                                    }
                                }
                                None
                            })
                            .keep_text(),
                        );
                    }
                    (0, d @ '1'..='3') => {
                        cols[col].kind = d as u8 - b'0';
                        cols[col].formula.clear();
                        self.col = (col + 1).min(23);
                    }
                    (1, '(') => {
                        cols[col].paren = true;
                        self.col = (col + 1).min(23);
                    }
                    (1, '-') => {
                        cols[col].paren = false;
                        self.col = (col + 1).min(23);
                    }
                    (2, d @ '0'..='4') => {
                        cols[col].digits = d as u8 - b'0';
                        self.col = (col + 1).min(23);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, "Math Definition           Use arrow keys to position cursor", st);
        let x0 = 26;
        let cols = self.cols.borrow();
        let letters: String = (0..24).map(|k| format!("{} ", (b'A' + k as u8) as char)).collect();
        g.text(0, 2, "Columns", st);
        g.text(x0, 2, letters.trim_end(), wp.theme.bold);
        g.text(0, 4, "Type", st);
        g.text(0, 6, "Negative Numbers", st);
        g.text(0, 8, "Number of Digits to", st);
        g.text(0, 9, "  the Right (0-4)", st);
        for (k, c) in cols.iter().enumerate().take(24) {
            let x = x0 + 2 * k;
            g.text(x, 4, &c.kind.to_string(), st);
            g.text(x, 6, if c.paren { "(" } else { "-" }, st);
            g.text(x, 8, &c.digits.to_string(), st);
        }
        g.text(0, 11, "Calculation   1", st);
        g.text(0, 12, "  Formulas    2", st);
        g.text(14, 13, "3", st);
        g.text(14, 14, "4", st);
        for (n, (k, c)) in cols.iter().enumerate().filter(|(_, c)| c.kind == 0).take(4).enumerate() {
            g.text(19, 11 + n, &format!("{}  {}", (b'A' + k as u8) as char, c.formula), st);
        }
        let r0 = g.rows.saturating_sub(9).max(16);
        g.text(0, r0, "Type of Column:", st);
        g.text(0, r0 + 1, "      0 = Calculation    1 = Text     2 = Numeric    3 = Total", st);
        g.text(0, r0 + 3, "Negative Numbers", st);
        g.text(0, r0 + 4, "      ( = Parentheses (50.00)         - = Minus Sign  -50.00", st);
        g.text(0, g.rows - 1, "Press Exit when done", st);
        let row = [4, 6, 8][self.row];
        g.cursor = Some((x0 + 2 * self.col, row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(vals: &[&[&str]]) -> Vec<El> {
        let rows = vals.len();
        let cols = vals[0].len();
        let mut v = vec![El::Code(Code::TblDef { cols: vec![1200; cols] })];
        for r in 0..rows {
            v.push(El::Code(Code::Row));
            for c in 0..cols {
                v.push(El::Code(Code::Cell));
                let s = vals[r][c];
                if let Some(f) = s.strip_prefix('=').filter(|f| !f.is_empty()) {
                    v.push(El::Code(Code::Formula(f.into())));
                } else if s == "=" {
                    v.push(El::Code(Code::Formula("=".into())));
                } else {
                    v.extend(s.chars().map(El::Ch));
                }
            }
        }
        v.push(El::Code(Code::TblOff));
        v
    }

    fn cell(els: &[El], r: usize, c: usize) -> String {
        let t = tables::parse(els, 0).unwrap();
        let (a, b) = t.cell_range(els, r, c);
        chars_of(&els[a..b])
    }

    #[test]
    fn formulas_left_to_right() {
        assert_eq!(eval_lr("1+2*2", &|_| None), Some(6.0));
        assert_eq!(eval_lr("1+(2*2)", &|_| None), Some(5.0));
        assert_eq!(eval_lr("-3+1", &|_| None), Some(-2.0));
        assert_eq!(eval_lr("4/0", &|_| None), None);
        assert_eq!(shift_formula("A1+B1*2", 1, 0), "A2+B2*2");
        assert_eq!(shift_formula("A1+B1", 0, 2), "C1+D1");
        assert_eq!(fmt_num(-50.0, 2, true), "(50.00)");
    }

    #[test]
    fn table_sum() {
        let mut els = table(&[&["Miete", "500"], &["Strom", "45,50"], &["Summe", "=+"], &["Zins", "=B2*2"], &["Zwei", "=A1+B1"]]);
        calc_table(&mut els, 0);
        assert_eq!(cell(&els, 2, 1), "545.50");
        assert_eq!(cell(&els, 3, 1), "91.00");
        assert_eq!(cell(&els, 4, 1), "500.00");
        // recalculation after a change replaces the old value
        let t = tables::parse(&els, 0).unwrap();
        let (a, b) = t.cell_range(&els, 0, 1);
        els.splice(a..b, "600".chars().map(El::Ch));
        calc_table(&mut els, 0);
        assert_eq!(cell(&els, 2, 1), "645.50");
        // totals
        let mut els = table(&[&["1"], &["2"], &["=+"], &["10"], &["=+"], &["="], &["=*"]]);
        calc_table(&mut els, 0);
        assert_eq!(cell(&els, 2, 0), "3.00");
        assert_eq!(cell(&els, 4, 0), "10.00");
        assert_eq!(cell(&els, 5, 0), "13.00");
        assert_eq!(cell(&els, 6, 0), "13.00");
    }

    #[test]
    fn math_columns() {
        // A numeric, B calculation (A*2)
        let mut def = MathCol::defaults();
        def[1] = MathCol { kind: 0, paren: true, digits: 1, formula: "A*2".into() };
        let mut els = vec![El::Code(Code::MathDef(def)), El::Code(Code::Math(true))];
        for (a, op) in [("10", true), ("-4", true)] {
            els.push(El::Ch('x'));
            els.push(El::Code(Code::Tab));
            els.extend(a.chars().map(El::Ch));
            if op {
                els.push(El::Code(Code::Tab));
                els.push(El::Code(Code::MathOp('!')));
            }
            els.push(El::Code(Code::HRt));
        }
        els.push(El::Code(Code::Tab));
        els.push(El::Code(Code::MathOp('+')));
        els.push(El::Code(Code::HRt));
        els.push(El::Code(Code::Math(false)));
        calc_columns(&mut els);
        let mut d = Document::new();
        d.els = els.clone();
        let t = d.plain_text(0, d.len());
        assert_eq!(t, "x\t10\t20.0\nx\t-4\t(8.0)\n\t6.00\n");
        // recalculating is stable
        calc_columns(&mut els);
        d.els = els;
        assert_eq!(d.plain_text(0, d.len()), t);
    }
}
