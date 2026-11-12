//! Alt-F7, 1 Columns: WordPerfect 5.1 text columns.
//!
//! In the stream: `[Col Def:Newspaper;2;1",4";4.5",7.5"]` defines the
//! columns (edges in WPU from the left paper edge), `[Col On]` … `[Col Off]`
//! is the columned region.
//!
//! * **Newspaper**: text snakes down column 1, then column 2 … of the page
//!   body; `[HPg]` (Ctrl-Enter) is a column break.  Pass 1 of the layout
//!   wraps the lines for a column, pagination ([`paginate_row`],
//!   [`placed`]) decides which column (and page) each line lands in and
//!   moves it there; [`regroup`] then puts the lines of a page side by side
//!   into display rows (WP's "side-by-side columns display").
//! * **Parallel** (and parallel with block protect): groups of columns side
//!   by side.  `[HPg]` moves to the next column; in the last column
//!   Ctrl-Enter inserts `[Col Off][HRt][Col On]` (a new group), like WP.
//!   Pass 1 places the lines like table cells.

use crate::doc::*;
use crate::editor::Editor;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::{COL_WPU, DRow, Layout, LineEnd, PageBreak, VRow};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};
use std::collections::HashMap;

/// Default distance between columns: 0.5".
pub const DEFAULT_GAP: i32 = 600;

/// A columned region of the layout.
#[derive(Clone, Debug)]
pub struct ColBlock {
    pub kind: ColKind,
    /// (left, right) edge of each column, WPU from the paper edge.
    pub x: Vec<(i32, i32)>,
}

/// Where a line sits in text columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    /// Index into `Layout::col_blocks`.
    pub block: usize,
    /// Column (0-based) the line is shown/printed in.
    pub col: usize,
    /// Column the line was wrapped for (newspaper: may differ until moved).
    pub wrap: usize,
    /// Parallel: row group number within the block.
    pub group: usize,
    /// Line number within its column (parallel: within the group; newspaper:
    /// within the column on that page).
    pub row: usize,
}

pub fn kind_name(k: ColKind) -> &'static str {
    match k {
        ColKind::Newspaper => "Newspaper",
        ColKind::Parallel => "Parallel",
        ColKind::ParallelProtect => "Parallel/Block Pro",
    }
}

/// `[Col Def:Newspaper;2;1",4";4.5",7.5"]`
pub fn reveal_text(kind: ColKind, cols: &[(i32, i32)]) -> String {
    let m: Vec<String> = cols.iter().map(|(l, r)| format!("{},{}", fmt_inches(*l), fmt_inches(*r))).collect();
    format!("[Col Def:{};{};{}]", kind_name(kind), cols.len(), m.join(";"))
}

/// Evenly spaced columns between `left` and `right` (WPU from the paper edge).
pub fn even_columns(n: usize, gap: i32, left: i32, right: i32) -> Vec<(i32, i32)> {
    let n = n.max(1) as i32;
    let w = ((right - left) - (n - 1) * gap) / n;
    (0..n).map(|k| {
        let a = left + k * (w + gap);
        (a, a + w)
    }).collect()
}

/// Columns overlap or are empty?
pub fn overlap(cols: &[(i32, i32)]) -> bool {
    cols.iter().any(|(l, r)| r <= l) || cols.windows(2).any(|w| w[1].0 < w[0].1)
}

/// Codes that take no room on the line and do not end it.
fn plain_format(e: &El) -> bool {
    match e {
        El::Ch(_) => false,
        El::Code(c) => !matches!(
            c,
            Code::HRt
                | Code::HPg
                | Code::Tab
                | Code::Indent
                | Code::DblIndent
                | Code::MarRel
                | Code::Center
                | Code::FlushRight
                | Code::HardSpace
                | Code::HardHyphen
                | Code::SoftHyphen
                | Code::Note { .. }
                | Code::TblDef { .. }
                | Code::Row
                | Code::Cell
                | Code::TblOff
                | Code::ColOn
                | Code::ColOff
        ),
    }
}

/// Only format codes from `k` up to a `[Col On]`/`[Col Off]`?
pub fn col_code_ahead(els: &[El], mut k: usize) -> bool {
    while k < els.len() && plain_format(&els[k]) {
        k += 1;
    }
    matches!(els.get(k), Some(El::Code(Code::ColOn | Code::ColOff)))
}

// ------------------------------------------------------------ layout pass 1

/// Pass-1 state of an open column region.
pub struct ColRun {
    pub block: usize,
    pub kind: ColKind,
    pub x: Vec<(i32, i32)>,
    pub cur: usize,
    group: usize,
    /// Parallel: first visual row of the current group.
    base: usize,
    in_col: usize,
    max: usize,
    row: usize,
}

impl ColRun {
    pub fn start(def: Option<&(ColKind, Vec<(i32, i32)>)>, blocks: &mut Vec<ColBlock>, next_vrow: usize) -> Option<ColRun> {
        let (kind, x) = def?;
        if x.is_empty() {
            return None;
        }
        blocks.push(ColBlock { kind: *kind, x: x.clone() });
        Some(ColRun { block: blocks.len() - 1, kind: *kind, x: x.clone(), cur: 0, group: 0, base: next_vrow, in_col: 0, max: 0, row: 0 })
    }

    pub fn parallel(&self) -> bool {
        self.kind != ColKind::Newspaper
    }

    /// Column edges for the line starting at element `start`.
    pub fn edges(&mut self, start: usize, hints: &HashMap<usize, usize>) -> (i32, i32) {
        if !self.parallel() {
            if let Some(&c) = hints.get(&start) {
                self.cur = c;
            }
        }
        self.cur = self.cur.min(self.x.len() - 1);
        self.x[self.cur]
    }

    pub fn line_col(&self) -> LineCol {
        LineCol { block: self.block, col: self.cur, wrap: self.cur, group: self.group, row: self.in_col }
    }

    /// Parallel: visual row of the next line.
    pub fn next_vrow(&mut self) -> usize {
        let v = self.base + self.in_col;
        self.in_col += 1;
        self.max = self.max.max(self.in_col);
        self.row = self.in_col;
        v
    }

    /// `[HPg]`: next column (parallel: after the last one, a new group).
    pub fn column_break(&mut self) {
        if !self.parallel() {
            return;
        }
        self.in_col = 0;
        self.cur += 1;
        if self.cur >= self.x.len() {
            self.cur = 0;
            self.group += 1;
            self.base += self.max;
            self.max = 0;
        }
    }

    /// `[Col Off]`: continue below the region.
    pub fn finish(self, next_vrow: &mut usize) {
        if self.parallel() {
            *next_vrow = self.base + self.max;
        }
    }
}

// --------------------------------------------------------------- pagination

/// Newspaper columns being filled on the current page.
pub struct Flow {
    block: usize,
    ncols: usize,
    /// Top of the columns on this page.
    y0: i32,
    col: usize,
    row: usize,
    /// Lowest bottom of any column on this page.
    ymax: i32,
}

impl Flow {
    pub fn new_page(&mut self, y: i32) {
        self.y0 = y;
        self.ymax = y;
        self.col = 0;
        self.row = 0;
    }
}

fn newspaper_block(l: &Layout, line_ids: &[usize]) -> Option<usize> {
    match line_ids {
        [li] => l.line_col.get(*li).copied().flatten().filter(|c| l.col_blocks[c.block].kind == ColKind::Newspaper).map(|c| c.block),
        _ => None,
    }
}

/// Before a visual row is placed at `y`: newspaper columns choose the
/// column (setting `brk` when a new page is needed); parallel columns
/// with block protect keep a group together.
#[allow(clippy::too_many_arguments)]
pub fn paginate_row(l: &Layout, line_ids: &[usize], flow: &mut Option<Flow>, y: &mut i32, height: i32, limit: i32, first_on_page: bool, brk: &mut bool) {
    let news = newspaper_block(l, line_ids);
    if flow.as_ref().is_some_and(|f| Some(f.block) != news) {
        // region ended: continue below the longest column
        let f = flow.take().unwrap();
        *y = (*y).max(f.ymax);
        *brk = *y + height > limit && !first_on_page;
    }
    if let Some(b) = news {
        let f = flow.get_or_insert(Flow { block: b, ncols: l.col_blocks[b].x.len(), y0: *y, col: 0, row: 0, ymax: *y });
        *brk = false;
        if *y + height > limit {
            if *y > f.y0 {
                if f.col + 1 < f.ncols {
                    f.col += 1;
                    f.row = 0;
                    *y = f.y0;
                } else {
                    *brk = true;
                }
            } else if f.col == 0 && !first_on_page {
                *brk = true;
            }
        }
        return;
    }
    // parallel with block protect: the whole group must fit
    let Some(&first) = line_ids.iter().min() else { return };
    let Some(lc) = l.line_col.get(first).copied().flatten() else { return };
    if l.col_blocks[lc.block].kind != ColKind::ParallelProtect || line_ids.iter().any(|&li| l.line_col[li].is_some_and(|c| c.row != 0)) {
        return;
    }
    let mut rows: HashMap<usize, i32> = HashMap::new();
    let mut k = first;
    while k < l.lines.len() {
        match l.line_col[k] {
            Some(c) if c.block == lc.block && c.group == lc.group => {
                let h = rows.entry(c.row).or_insert(0);
                *h = (*h).max(l.lines[k].height);
            }
            _ => break,
        }
        k += 1;
    }
    let gh: i32 = rows.values().sum();
    if *y + gh > limit && !first_on_page {
        *brk = true;
    }
}

/// After a newspaper line was placed (and `y` advanced): move it into its
/// column; a `[HPg]` goes to the next column.  Returns true when a column
/// break needs a new page.
pub fn placed(l: &mut Layout, line_ids: &[usize], flow: &mut Option<Flow>, y: &mut i32) -> bool {
    let Some(f) = flow.as_mut() else { return false };
    if newspaper_block(l, line_ids) != Some(f.block) {
        return false;
    }
    let li = line_ids[0];
    let x = l.col_blocks[f.block].x.clone();
    let lc = l.line_col[li].as_mut().unwrap();
    let from = lc.wrap.min(x.len() - 1);
    lc.col = f.col;
    lc.row = f.row;
    f.row += 1;
    f.ymax = f.ymax.max(*y);
    if f.col != from {
        let dx = x[f.col].0 - x[from].0;
        let dc = x[f.col].0 / COL_WPU - x[from].0 / COL_WPU;
        let line = &mut l.lines[li];
        for c in line.cells.iter_mut() {
            c.col += dc;
        }
        for k in line.caret.iter_mut() {
            k.0 += dc;
            k.1 += dx;
        }
        line.right += dx;
    }
    if l.lines[li].term == LineEnd::HardPage {
        if f.col + 1 < f.ncols {
            f.col += 1;
            f.row = 0;
            *y = f.y0;
        } else {
            return true;
        }
    }
    false
}

/// Newspaper lines placed in a column of another width than they were
/// wrapped for: hints (line start → column) for another layout pass.
pub fn rewrap_hints(l: &Layout) -> Option<HashMap<usize, usize>> {
    let mut bad = false;
    let mut h = HashMap::new();
    for (li, lc) in l.line_col.iter().enumerate() {
        let Some(c) = lc else { continue };
        let b = &l.col_blocks[c.block];
        if b.kind != ColKind::Newspaper {
            continue;
        }
        let w = |k: usize| b.x.get(k).map(|(a, z)| z - a).unwrap_or(0);
        if c.col != c.wrap && w(c.col) != w(c.wrap) {
            bad = true;
        }
        h.insert(l.lines[li].start, c.col);
    }
    if bad { Some(h) } else { None }
}

/// Put the newspaper lines of each page side by side into display rows.
pub fn regroup(l: &mut Layout) {
    let news_of = |l: &Layout, v: usize| -> Option<(usize, usize)> {
        match &l.vrows[v] {
            VRow::Lines(ls) => newspaper_block(l, ls).map(|b| (b, ls[0])),
            _ => None,
        }
    };
    if !(0..l.vrows.len()).any(|v| news_of(l, v).is_some()) {
        return;
    }
    // how each page was entered (soft / hard break)
    let mut page_kind: HashMap<u32, PageBreak> = HashMap::new();
    let mut pending: Option<PageBreak> = None;
    for d in &l.drows {
        match *d {
            DRow::Break(k) => pending = Some(k),
            DRow::V(v) => {
                if let Some(k) = pending.take() {
                    page_kind.entry(l.vrow_page[v]).or_insert(k);
                }
            }
            DRow::Comment { .. } => {}
        }
    }
    let mut vrows = vec![];
    let mut vy = vec![];
    let mut vp = vec![];
    let mut v = 0;
    while v < l.vrows.len() {
        let Some((b, _)) = news_of(l, v) else {
            vrows.push(l.vrows[v].clone());
            vy.push(l.vrow_y[v]);
            vp.push(l.vrow_page[v]);
            v += 1;
            continue;
        };
        let page = l.vrow_page[v];
        let mut members = vec![];
        while v < l.vrows.len() && l.vrow_page[v] == page && news_of(l, v).map(|x| x.0) == Some(b) {
            members.push(news_of(l, v).unwrap().1);
            v += 1;
        }
        let maxrow = members.iter().map(|&li| l.line_col[li].unwrap().row).max().unwrap_or(0);
        for r in 0..=maxrow {
            let mut ls: Vec<usize> = members.iter().copied().filter(|&li| l.line_col[li].unwrap().row == r).collect();
            if ls.is_empty() {
                continue;
            }
            ls.sort_by_key(|&li| l.line_col[li].unwrap().col);
            vy.push(ls.iter().map(|&li| l.lines[li].y).min().unwrap_or(0));
            vp.push(page);
            vrows.push(VRow::Lines(ls));
        }
    }
    let mut line_vrow = vec![0; l.lines.len()];
    for (k, vr) in vrows.iter().enumerate() {
        if let VRow::Lines(ls) = vr {
            for &li in ls {
                line_vrow[li] = k;
            }
        }
    }
    let mut drows = vec![];
    let mut vrow_drow = vec![0; vrows.len()];
    for k in 0..vrows.len() {
        if k > 0 && vp[k] != vp[k - 1] {
            drows.push(DRow::Break(page_kind.get(&vp[k]).copied().unwrap_or(PageBreak::Soft)));
        }
        vrow_drow[k] = drows.len();
        drows.push(DRow::V(k));
    }
    l.vrows = vrows;
    l.vrow_y = vy;
    l.vrow_page = vp;
    l.line_vrow = line_vrow;
    l.drows = drows;
    l.vrow_drow = vrow_drow;
}

// ------------------------------------------------------------------ editing

/// Column number (1-based) of the cursor for the status line.
pub fn status_col(e: &Editor) -> Option<usize> {
    let li = e.layout.line_of(e.cursor);
    e.layout.line_col.get(li).copied().flatten().map(|c| c.col + 1)
}

/// Left/right edge of the cursor's column (for the Reveal Codes ruler).
pub fn cursor_edges(e: &Editor) -> Option<(i32, i32)> {
    let li = e.layout.line_of(e.cursor);
    let c = e.layout.line_col.get(li).copied().flatten()?;
    e.layout.col_blocks.get(c.block)?.x.get(c.col).copied()
}

pub enum GoCol {
    Next,
    Prev,
    First,
    Last,
}

/// Ctrl-Home →/← (and Ctrl-Home, Home, →/←): move to another column.
pub fn goto_column(e: &mut Editor, how: GoCol) -> bool {
    e.ensure_layout();
    let lay = &e.layout;
    let li = lay.line_of(e.cursor);
    let Some(lc) = lay.line_col.get(li).copied().flatten() else { return false };
    let ncols = lay.col_blocks[lc.block].x.len();
    let target = match how {
        GoCol::Next => lc.col + 1,
        GoCol::Prev => match lc.col.checked_sub(1) {
            Some(c) => c,
            None => return false,
        },
        GoCol::First => 0,
        GoCol::Last => ncols - 1,
    };
    if target >= ncols || target == lc.col {
        return false;
    }
    let page = lay.lines[li].page;
    let parallel = lay.col_blocks[lc.block].kind != ColKind::Newspaper;
    // lines of the target column in the same group / on the same page
    let cands: Vec<usize> = (0..lay.lines.len())
        .filter(|&k| {
            lay.line_col[k].is_some_and(|c| c.block == lc.block && c.col == target && if parallel { c.group == lc.group } else { lay.lines[k].page == page })
        })
        .collect();
    if cands.is_empty() {
        return false;
    }
    let v = lay.line_vrow[li];
    let t = cands.iter().copied().find(|&k| lay.line_vrow[k] == v).unwrap_or_else(|| *cands.iter().filter(|&&k| lay.line_vrow[k] <= v).last().unwrap_or(&cands[0]));
    let x = &lay.col_blocks[lc.block].x;
    let cur_col = lay.lines[li].caret_of(e.cursor).0;
    let goal = cur_col - x[lc.col].0 / COL_WPU + x[target].0 / COL_WPU;
    e.cursor = lay.idx_at_col(t, goal);
    true
}

/// Ctrl-Enter: a column break; in the last parallel column a new group.
pub fn hard_page(e: &mut Editor) {
    e.ensure_layout();
    let li = e.layout.line_of(e.cursor);
    if let Some(lc) = e.layout.line_col.get(li).copied().flatten() {
        let b = &e.layout.col_blocks[lc.block];
        if b.kind != ColKind::Newspaper && lc.col + 1 >= b.x.len() {
            e.insert_els(&[El::Code(Code::ColOff), El::Code(Code::HRt), El::Code(Code::ColOn)]);
            return;
        }
    }
    e.insert_code(Code::HPg);
}

/// The `[Col Def]` in effect at `pos`.
pub fn def_at(els: &[El], pos: usize) -> Option<(ColKind, Vec<(i32, i32)>)> {
    els[..pos.min(els.len())].iter().rev().find_map(|e| match e {
        El::Code(Code::ColDef { kind, cols }) => Some((*kind, cols.clone())),
        _ => None,
    })
}

/// Alt-F7, 1: `Columns: 1 On; 2 Off; 3 Define: 0`
pub fn columns_menu_line() -> Box<dyn Screen> {
    MenuLine::new(&["~On", "O~ff", "~Define"], |wp, k| match k {
        0 => {
            if def_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).is_none() {
                wp.flash("ERROR: No text columns defined");
                return None;
            }
            wp.ed().insert_code(Code::ColOn);
            wp.after_edit();
            None
        }
        1 => {
            wp.ed().insert_code(Code::ColOff);
            wp.after_edit();
            None
        }
        _ => Some(Box::new(ColDefScreen::new(wp)) as Box<dyn Screen>),
    })
    .prefix("Columns: ")
}

// ---------------------------------------------------- Text Column Definition

#[derive(Clone, Copy, PartialEq, Debug)]
enum Field {
    Num,
    Gap,
    /// (column, right edge?)
    Margin(usize, bool),
}

/// The `Text Column Definition` screen.
pub struct ColDefScreen {
    kind: ColKind,
    gap: i32,
    cols: Vec<(i32, i32)>,
    left: i32,
    right: i32,
    edit: Option<(Field, String, bool)>,
    choosing_type: bool,
}

impl ColDefScreen {
    pub fn new(wp: &mut Wp) -> ColDefScreen {
        let e = wp.ed();
        let f = e.format_state_at(e.cursor);
        let left = f.left;
        let right = e.doc.init.page_width - f.right;
        match def_at(&e.doc.els, e.cursor) {
            Some((kind, cols)) if !cols.is_empty() => {
                let gap = if cols.len() > 1 { cols[1].0 - cols[0].1 } else { DEFAULT_GAP };
                ColDefScreen { kind, gap, cols, left, right, edit: None, choosing_type: false }
            }
            _ => ColDefScreen { kind: ColKind::Newspaper, gap: DEFAULT_GAP, cols: even_columns(2, DEFAULT_GAP, left, right), left, right, edit: None, choosing_type: false },
        }
    }

    fn value(&self, f: Field) -> String {
        match f {
            Field::Num => self.cols.len().to_string(),
            Field::Gap => fmt_inches(self.gap),
            Field::Margin(c, r) => self.cols.get(c).map(|m| fmt_inches(if r { m.1 } else { m.0 })).unwrap_or_default(),
        }
    }

    /// Store a typed value; false if it was not acceptable.
    fn commit(&mut self, f: Field, s: &str) -> Result<(), &'static str> {
        match f {
            Field::Num => {
                let n: usize = s.trim().parse().map_err(|_| "")?;
                if !(1..=24).contains(&n) {
                    return Err("ERROR: Too many columns");
                }
                self.cols = even_columns(n, self.gap, self.left, self.right);
            }
            Field::Gap => {
                let g = parse_measure(s).ok_or("")?.max(0);
                let n = self.cols.len();
                let c = even_columns(n, g, self.left, self.right);
                if overlap(&c) {
                    return Err("ERROR: Margins are not wide enough to add column");
                }
                self.gap = g;
                self.cols = c;
            }
            Field::Margin(c, r) => {
                let v = parse_measure(s).ok_or("")?;
                if let Some(m) = self.cols.get_mut(c) {
                    if r {
                        m.1 = v
                    } else {
                        m.0 = v
                    }
                }
            }
        }
        Ok(())
    }

    fn next_field(f: Field, n: usize, back: bool) -> Option<Field> {
        let Field::Margin(c, r) = f else { return None };
        let k = c * 2 + r as usize;
        let k = if back { k.checked_sub(1)? } else { k + 1 };
        (k < n * 2).then_some(Field::Margin(k / 2, k % 2 == 1))
    }

    fn finish(&self, wp: &mut Wp) -> Next {
        if overlap(&self.cols) {
            wp.flash("ERROR: Text columns can't overlap");
            return None;
        }
        let e = wp.ed();
        // replace a definition right before the cursor
        if e.cursor > 0 && matches!(e.doc.els.get(e.cursor - 1), Some(El::Code(Code::ColDef { .. }))) {
            e.doc.els.remove(e.cursor - 1);
            e.cursor -= 1;
        }
        e.insert_code(Code::ColDef { kind: self.kind, cols: self.cols.clone() });
        wp.after_edit();
        Some(columns_menu_line())
    }
}

impl Screen for ColDefScreen {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if self.choosing_type {
            self.choosing_type = false;
            let k = match i {
                Input::Char('1' | 'n' | 'N') => Some(ColKind::Newspaper),
                Input::Char('2' | 'p' | 'P') => Some(ColKind::Parallel),
                Input::Char('3' | 'b' | 'B') => Some(ColKind::ParallelProtect),
                _ => None,
            };
            if let Some(k) = k {
                self.kind = k;
            }
            return Outcome::Keep;
        }
        if let Some((f, buf, fresh)) = &mut self.edit {
            let f = *f;
            match i {
                Input::Char(c) => {
                    if *fresh {
                        buf.clear();
                        *fresh = false;
                    }
                    buf.push(*c);
                }
                Input::Func(Func::Backspace) => {
                    *fresh = false;
                    buf.pop();
                }
                Input::Func(Func::Cancel | Func::Esc) => self.edit = None,
                Input::Func(Func::Enter | Func::Down | Func::Up | Func::Tab | Func::Right | Func::Left | Func::Exit) => {
                    let s = buf.clone();
                    if !s.trim().is_empty() && s != self.value(f) {
                        if let Err(m) = self.commit(f, &s) {
                            if !m.is_empty() {
                                wp.flash(m);
                            }
                        }
                    }
                    let back = matches!(i, Input::Func(Func::Up | Func::Left));
                    let next = if matches!(i, Input::Func(Func::Exit)) { None } else { Self::next_field(f, self.cols.len(), back) };
                    self.edit = next.map(|n| (n, self.value(n), true));
                    if matches!(i, Input::Func(Func::Exit)) {
                        return match self.finish(wp) {
                            Some(n) => Outcome::Replace(n),
                            None => Outcome::Keep,
                        };
                    }
                }
                _ => {}
            }
            return Outcome::Keep;
        }
        match i {
            Input::Char('1' | 't' | 'T') => self.choosing_type = true,
            Input::Char('2' | 'n' | 'N') => self.edit = Some((Field::Num, self.value(Field::Num), true)),
            Input::Char('3' | 'd' | 'D') => self.edit = Some((Field::Gap, self.value(Field::Gap), true)),
            Input::Char('4' | 'm' | 'M') => self.edit = Some((Field::Margin(0, false), self.value(Field::Margin(0, false)), true)),
            Input::Func(Func::Exit) | Input::Func(Func::Enter) | Input::Char('0') | Input::Char(' ') => {
                return match self.finish(wp) {
                    Some(n) => Outcome::Replace(n),
                    None => Outcome::Keep,
                };
            }
            Input::Func(Func::Cancel | Func::Esc) => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Text Column Definition", st);
        let items = [(2, '1', "~Type"), (4, '2', "~Number of Columns"), (6, '3', "~Distance Between Columns"), (8, '4', "~Margins")];
        for (row, key, label) in items {
            g.text(5, row, &format!("{key} - "), st);
            let end = g.text_mn(9, row, label, st, hi);
            wp.hit(row, 5, end.max(60), crate::ui::HitAction::Input(Input::Char(key)));
        }
        let kind = match self.kind {
            ColKind::Newspaper => "Newspaper",
            ColKind::Parallel => "Parallel",
            ColKind::ParallelProtect => "Parallel with Block Protect",
        };
        g.text(44, 2, kind, st);
        let shown = |f: Field| -> String {
            match &self.edit {
                Some((e, buf, _)) if *e == f => buf.clone(),
                _ => self.value(f),
            }
        };
        g.text(44, 4, &shown(Field::Num), st);
        g.text(44, 6, &shown(Field::Gap), st);
        g.text(5, 10, "Column    Left      Right     Column    Left      Right", st);
        let mut cursor = None;
        for k in 0..24usize {
            let (c0, row) = if k < 12 { (5, 11 + k) } else { (35, 11 + k - 12) };
            if row >= g.rows.saturating_sub(1) {
                continue;
            }
            g.text(c0 - 1, row, &format!("{:>3}:", k + 1), st);
            if k < self.cols.len() {
                for (r, x) in [(false, c0 + 10), (true, c0 + 20)] {
                    let f = Field::Margin(k, r);
                    let v = shown(f);
                    g.text(x, row, &v, st);
                    if matches!(&self.edit, Some((e, _, _)) if *e == f) {
                        cursor = Some((x + v.chars().count(), row));
                    }
                }
            }
        }
        if let Some((f, buf, _)) = &self.edit {
            match f {
                Field::Num => cursor = Some((44 + buf.chars().count(), 4)),
                Field::Gap => cursor = Some((44 + buf.chars().count(), 6)),
                _ => {}
            }
        }
        let fr = g.rows - 1;
        if self.choosing_type {
            let c = g.text(0, fr, "Column Type: ", st);
            let mut c = c;
            for (n, o) in ["~Newspaper", "~Parallel", "Parallel with ~Block Protect"].iter().enumerate() {
                c = g.text(c, fr, &format!("{} ", n + 1), st);
                c = g.text_mn(c, fr, o, st, hi);
                if n < 2 {
                    c = g.text(c, fr, "; ", st);
                }
            }
            let c = g.text(c, fr, ": 0", st);
            g.cursor = Some((c - 1, fr));
        } else {
            let c = g.text(0, fr, "Selection: 0", st);
            g.cursor = cursor.or(Some((c - 1, fr)));
        }
    }
}

/// Ctrl-Home, Home: waits for ←/→ (first/last column).
pub struct GoToHome;

impl Screen for GoToHome {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Left) => {
                goto_column(wp.ed(), GoCol::First);
            }
            Input::Func(Func::Right) => {
                goto_column(wp.ed(), GoCol::Last);
            }
            _ => {}
        }
        Outcome::Close
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let c = g.text(0, row, "Go to ", wp.theme.normal);
        g.cursor = Some((c, row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{MonoMetrics, layout};

    fn two_cols() -> Code {
        Code::ColDef { kind: ColKind::Newspaper, cols: even_columns(2, DEFAULT_GAP, 1200, 9000) }
    }

    fn doc_with(kind: ColKind, body: &[El]) -> Document {
        let mut d = Document::new();
        d.init.page_width = 10200;
        d.init.page_height = 13200;
        d.els.push(El::Code(Code::ColDef { kind, cols: even_columns(2, DEFAULT_GAP, 1200, 9000) }));
        d.els.push(El::Code(Code::ColOn));
        d.els.extend(body.iter().cloned());
        d.els.push(El::Code(Code::ColOff));
        d
    }

    fn text_els(s: &str) -> Vec<El> {
        Document::from_text(s).els
    }

    #[test]
    fn reveal_and_even() {
        let c = even_columns(2, 600, 1200, 9000);
        assert_eq!(c, vec![(1200, 4800), (5400, 9000)]);
        let Code::ColDef { kind, cols } = two_cols() else { panic!() };
        assert_eq!(reveal_text(kind, &cols), "[Col Def:Newspaper;2;1\",4\";4.5\",7.5\"]");
    }

    #[test]
    fn newspaper_flows_into_column_two() {
        // 11" page, 1" margins: 54 lines per column
        let d = doc_with(ColKind::Newspaper, &text_els(&"line\n".repeat(70)));
        let l = layout(&d, &MonoMetrics);
        let cols: Vec<_> = l.line_col.iter().flatten().copied().collect();
        assert!(cols.len() >= 70);
        assert_eq!(l.lines[69].page, 1, "70 short lines fit on one page in two columns");
        let first = 0;
        assert_eq!(l.line_col[first].unwrap().col, 0);
        // line 54 (0-based in the column region) is at the top of column 2
        let li = first + 54;
        assert_eq!(l.line_col[li].unwrap().col, 1, "{:?}", l.line_col[li]);
        assert_eq!(l.lines[li].y, l.lines[first].y);
        assert_eq!(l.lines[li].cells[0].col, 45, "column 2 starts at 4.5\"");
        // side by side on screen: same display row
        assert_eq!(l.line_vrow[li], l.line_vrow[first]);
        // 140 lines need a second page
        let d = doc_with(ColKind::Newspaper, &text_els(&"line\n".repeat(140)));
        let l = layout(&d, &MonoMetrics);
        assert_eq!(l.pages, 2);
        assert!(l.drows.iter().any(|r| matches!(r, DRow::Break(PageBreak::Soft))));
    }

    #[test]
    fn newspaper_wraps_to_column_width() {
        let d = doc_with(ColKind::Newspaper, &text_els(&"word ".repeat(30)));
        let l = layout(&d, &MonoMetrics);
        // 3" column = 30 characters, "word " → 6 words per line
        assert_eq!(l.lines[1].cells.len(), 30);
    }

    #[test]
    fn column_break() {
        let mut body = text_els("one\ntwo");
        body.push(El::Code(Code::HPg));
        body.extend(text_els("three\n"));
        let d = doc_with(ColKind::Newspaper, &body);
        let l = layout(&d, &MonoMetrics);
        let find = |s: &str| l.lines.iter().position(|x| x.cells.iter().map(|c| c.ch).collect::<String>() == s).unwrap();
        let (one, t) = (find("one"), find("three"));
        assert_eq!(l.line_col[find("two")].unwrap().col, 0);
        assert_eq!(l.line_col[t].unwrap().col, 1);
        assert_eq!(l.lines[t].y, l.lines[one].y);
        assert_eq!(l.lines[t].page, 1);
        assert_eq!(l.pages, 1);
    }

    #[test]
    fn parallel_groups_side_by_side() {
        let mut body = text_els("left one\nleft two");
        body.push(El::Code(Code::HPg));
        body.extend(text_els("right"));
        body.push(El::Code(Code::ColOff));
        body.push(El::Code(Code::HRt));
        body.push(El::Code(Code::ColOn));
        body.extend(text_els("next"));
        let d = doc_with(ColKind::Parallel, &body);
        let l = layout(&d, &MonoMetrics);
        let t = |s: &str| l.lines.iter().position(|x| x.cells.iter().map(|c| c.ch).collect::<String>().trim_end() == s).unwrap();
        assert_eq!(l.line_vrow[t("left one")], l.line_vrow[t("right")]);
        assert_eq!(l.lines[t("right")].cells[0].col, 45);
        assert!(l.lines[t("next")].y > l.lines[t("left two")].y);
        assert_eq!(l.lines[t("next")].cells[0].col, 10);
        assert_eq!(l.pages, 1);
    }

    #[test]
    fn unequal_columns_rewrap() {
        let mut d = Document::new();
        d.init.page_width = 10200;
        d.init.page_height = 13200;
        d.els.push(El::Code(Code::ColDef { kind: ColKind::Newspaper, cols: vec![(1200, 3600), (4200, 9000)] }));
        d.els.push(El::Code(Code::ColOn));
        d.els.push(El::Code(Code::HPg));
        d.els.extend(text_els(&"word ".repeat(20)));
        let l = layout(&d, &MonoMetrics);
        let li = l.lines.iter().position(|x| !x.cells.is_empty()).unwrap();
        assert_eq!(l.line_col[li].unwrap().col, 1);
        // column 2 is 4" wide = 40 characters
        assert_eq!(l.lines[li].end - l.lines[li].start, 40);
        assert_eq!(l.lines[li].cells[0].col, 35);
    }
}
