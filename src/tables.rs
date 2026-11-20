//! Alt-F7 Columns/Table → Tables: create tables and the Table Edit screen.
//!
//! A table lives in the text stream as
//! `[Tbl Def:…][Row][Cell]text[Cell]text[Row][Cell]…[Tbl Off]`.

use crate::doc::*;
use crate::editor::Editor;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

/// Structure of one table: element indices.
#[derive(Debug, Clone)]
pub struct Table {
    pub def: usize,
    pub off: usize,
    pub cols: Vec<i32>,
    /// rows → cells → index of the [Cell] code
    pub rows: Vec<Vec<usize>>,
    /// index of each [Row] code
    pub row_codes: Vec<usize>,
}

impl Table {
    /// Content range of a cell (after its [Cell] code up to the next table code).
    pub fn cell_range(&self, els: &[El], r: usize, c: usize) -> (usize, usize) {
        let start = self.rows[r][c] + 1;
        let mut end = start;
        while end < els.len() && !matches!(els[end], El::Code(Code::Cell | Code::Row | Code::TblOff)) {
            end += 1;
        }
        (start, end)
    }

    /// (row, col) of the cell containing element index `i`.
    pub fn locate(&self, i: usize) -> Option<(usize, usize)> {
        let mut best = None;
        for (r, row) in self.rows.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                if cell < i {
                    best = Some((r, c));
                }
            }
        }
        best
    }
}

/// What a cell is with regard to joined cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Normal,
    /// The first cell of a joined area of `cols` × `rows` cells.
    Anchor { cols: usize, rows: usize },
    /// Hidden under the joined cell at (r, c).
    Covered { r: usize, c: usize },
}

impl Table {
    /// The `[Join]` code of cell (r, c): (element index, cols, rows).  It
    /// sits among the codes at the start of the cell.
    pub fn join_of(&self, els: &[El], r: usize, c: usize) -> Option<(usize, usize, usize)> {
        let mut i = self.rows[r][c] + 1;
        while let Some(El::Code(code)) = els.get(i) {
            match code {
                Code::Join { cols, rows } => return Some((i, (*cols).max(1) as usize, (*rows).max(1) as usize)),
                Code::Row | Code::Cell | Code::TblOff | Code::TblDef { .. } | Code::HRt | Code::Tab => break,
                _ => i += 1,
            }
        }
        None
    }

    /// Every cell's role (rows × cells).  Joins are clipped to the table.
    pub fn slots(&self, els: &[El]) -> Vec<Vec<Slot>> {
        let mut out: Vec<Vec<Slot>> = self.rows.iter().map(|r| vec![Slot::Normal; r.len()]).collect();
        for r in 0..self.rows.len() {
            for c in 0..self.rows[r].len() {
                if out[r][c] != Slot::Normal {
                    continue;
                }
                let Some((_, cols, rows)) = self.join_of(els, r, c) else { continue };
                let cols = cols.min(self.rows[r].len() - c);
                let rows = rows.min(self.rows.len() - r);
                if cols * rows <= 1 {
                    continue;
                }
                for rr in r..r + rows {
                    for cc in c..(c + cols).min(self.rows[rr].len()) {
                        if (rr, cc) != (r, c) && out[rr][cc] == Slot::Normal {
                            out[rr][cc] = Slot::Covered { r, c };
                        }
                    }
                }
                out[r][c] = Slot::Anchor { cols, rows };
            }
        }
        out
    }

    /// The `[Cell Lines]` code of cell (r, c): (element index, settings).
    /// Like `[Join]` it sits among the codes at the start of the cell.
    pub fn fmt_of(&self, els: &[El], r: usize, c: usize) -> Option<(usize, CellFmt)> {
        let mut i = self.rows[r][c] + 1;
        while let Some(El::Code(code)) = els.get(i) {
            match code {
                Code::CellFmt(f) => return Some((i, *f)),
                Code::Row | Code::Cell | Code::TblOff | Code::TblDef { .. } | Code::HRt | Code::Tab => break,
                _ => i += 1,
            }
        }
        None
    }

    fn has_joins(&self, els: &[El]) -> bool {
        els[self.def..self.off.min(els.len())].iter().any(|e| matches!(e, El::Code(Code::Join { .. })))
    }
}

/// The cursor may not rest in a cell hidden under a joined cell.  Returns
/// where it should go instead: `forward` = keep moving right (the next
/// visible cell, or behind the table), `Some(false)` = keep moving left
/// (the end of the previous visible cell), `None` = the joined cell itself.
pub fn leave_covered(els: &[El], cursor: usize, forward: Option<bool>) -> Option<usize> {
    // cheap test first: is there a [Join] in the table around the cursor?
    let mut d = cursor.min(els.len());
    loop {
        if d == 0 {
            return None;
        }
        d -= 1;
        match els[d] {
            El::Code(Code::TblOff) => return None,
            El::Code(Code::TblDef { .. }) => break,
            _ => {}
        }
    }
    let t = parse(els, d)?;
    if cursor > t.off || !t.has_joins(els) {
        return None;
    }
    let (r, c) = t.locate(cursor)?;
    let slots = t.slots(els);
    let Slot::Covered { r: ar, c: ac } = slots[r][c] else { return None };
    let flat: Vec<(usize, usize)> = (0..t.rows.len()).flat_map(|r| (0..t.rows[r].len()).map(move |c| (r, c))).collect();
    let k = flat.iter().position(|&x| x == (r, c))?;
    let visible = |&&(r, c): &&(usize, usize)| !matches!(slots[r][c], Slot::Covered { .. });
    Some(match forward {
        None => t.cell_range(els, ar, ac).0,
        Some(true) => match flat[k..].iter().find(visible) {
            Some(&(r, c)) => t.cell_range(els, r, c).0,
            None => (t.off + 1).min(els.len()),
        },
        Some(false) => match flat[..k].iter().rev().find(visible) {
            Some(&(r, c)) => t.cell_range(els, r, c).1,
            None => t.def,
        },
    })
}

/// The table-wide settings of the table whose `[Tbl Def]` is at `def`
/// (the `[Tbl Opt]` code right behind it; the defaults without one).
pub fn opt_at(els: &[El], def: usize) -> TblOpt {
    match els.get(def + 1) {
        Some(El::Code(Code::TblOpt(o))) => *o,
        _ => TblOpt::default(),
    }
}

/// Store the table-wide settings: a `[Tbl Opt]` code behind the
/// `[Tbl Def]`, or none when they are the defaults.  Returns by how much
/// the elements behind it moved (-1, 0, 1).
fn write_opt(els: &mut Vec<El>, def: usize, opt: TblOpt) -> isize {
    let has = matches!(els.get(def + 1), Some(El::Code(Code::TblOpt(_))));
    match (has, opt.is_default()) {
        (true, true) => {
            els.remove(def + 1);
            -1
        }
        (true, false) => {
            els[def + 1] = El::Code(Code::TblOpt(opt));
            0
        }
        (false, true) => 0,
        (false, false) => {
            els.insert(def + 1, El::Code(Code::TblOpt(opt)));
            1
        }
    }
}

/// Store the lines and shading of cell (r, c): a `[Cell Lines]` code at
/// the start of the cell (behind its `[Join]`), or none when nothing is
/// set.  Only elements behind the start of this cell move.
fn write_fmt(els: &mut Vec<El>, t: &Table, r: usize, c: usize, f: CellFmt) {
    match (t.fmt_of(els, r, c), f.is_default()) {
        (Some((i, _)), true) => {
            els.remove(i);
        }
        (Some((i, _)), false) => els[i] = El::Code(Code::CellFmt(f)),
        (None, true) => {}
        (None, false) => {
            let at = t.join_of(els, r, c).map(|j| j.0 + 1).unwrap_or(t.rows[r][c] + 1);
            els.insert(at, El::Code(Code::CellFmt(f)));
        }
    }
}

/// The table containing position `pos`, if any.
pub fn table_at(els: &[El], pos: usize) -> Option<Table> {
    // find the last [Tbl Def] before pos that is not closed before pos
    let mut def = None;
    for (i, e) in els.iter().enumerate().take(pos.min(els.len())) {
        match e {
            El::Code(Code::TblDef { .. }) => def = Some(i),
            El::Code(Code::TblOff) => def = None,
            _ => {}
        }
    }
    let def = def?;
    parse(els, def)
}

pub fn parse(els: &[El], def: usize) -> Option<Table> {
    let El::Code(Code::TblDef { cols }) = &els[def] else { return None };
    let mut rows: Vec<Vec<usize>> = vec![];
    let mut row_codes = vec![];
    let mut off = els.len();
    for (i, e) in els.iter().enumerate().skip(def + 1) {
        match e {
            El::Code(Code::Row) => {
                rows.push(vec![]);
                row_codes.push(i);
            }
            El::Code(Code::Cell) => {
                if rows.is_empty() {
                    rows.push(vec![]);
                    row_codes.push(i);
                }
                rows.last_mut().unwrap().push(i);
            }
            El::Code(Code::TblOff) => {
                off = i;
                break;
            }
            El::Code(Code::TblDef { .. }) => {
                off = i;
                break;
            }
            _ => {}
        }
    }
    Some(Table { def, off, cols: cols.clone(), rows, row_codes })
}

pub fn col_letter(c: usize) -> String {
    let mut s = String::new();
    let mut n = c + 1;
    while n > 0 {
        let r = (n - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

/// Elements for a new table.
pub fn new_table(cols: usize, rows: usize, width: i32) -> Vec<El> {
    let w = width / cols.max(1) as i32;
    let mut v = vec![El::Code(Code::TblDef { cols: vec![w; cols] })];
    for _ in 0..rows {
        v.push(El::Code(Code::Row));
        for _ in 0..cols {
            v.push(El::Code(Code::Cell));
        }
    }
    v.push(El::Code(Code::TblOff));
    v
}

/// Move the cursor to the start of cell (r, c).
pub fn goto_cell(e: &mut Editor, t: &Table, r: usize, c: usize) {
    let r = r.min(t.rows.len().saturating_sub(1));
    let c = c.min(t.rows[r].len().saturating_sub(1));
    let (s, _) = t.cell_range(&e.doc.els, r, c);
    e.cursor = s;
    e.settle();
}

/// Tab / Shift-Tab inside a table: next / previous cell.  Returns false
/// when the cursor is not in a table.
pub fn tab_in_table(e: &mut Editor, forward: bool) -> bool {
    let Some(t) = table_at(&e.doc.els, e.cursor) else { return false };
    let Some((r, c)) = t.locate(e.cursor) else { return false };
    // (cells hidden under a joined cell are skipped)
    let slots = t.slots(&e.doc.els);
    let flat: Vec<(usize, usize)> = t
        .rows
        .iter()
        .enumerate()
        .flat_map(|(r, row)| (0..row.len()).map(move |c| (r, c)))
        .filter(|&(rr, cc)| (rr, cc) == (r, c) || !matches!(slots[rr][cc], Slot::Covered { .. }))
        .collect();
    let k = flat.iter().position(|&x| x == (r, c)).unwrap_or(0);
    let next = if forward { k + 1 } else { k.wrapping_sub(1) };
    if let Some(&(nr, nc)) = flat.get(next) {
        goto_cell(e, &t, nr, nc);
    } else if forward {
        // past the last cell: leave the table
        e.cursor = (t.off + 1).min(e.doc.els.len());
        e.settle();
    }
    true
}

/// Is element `i` a table structure code that must not be deleted by
/// ordinary editing?
pub fn is_structure(e: &El) -> bool {
    matches!(e, El::Code(Code::Row | Code::Cell | Code::TblOff))
}

/// The codes a table is made of: its structure and the settings that
/// belong to it (joined cells, lines, options).
fn is_table_code(e: &El) -> bool {
    matches!(e, El::Code(Code::Row | Code::Cell | Code::TblOff | Code::TblDef { .. } | Code::Join { .. } | Code::TblOpt(_) | Code::CellFmt(_)))
}

/// Is the element before `k` the start of a row or of the table (so that
/// the code at `k` is the first of its row)?
fn first_in_row(els: &[El], k: usize) -> bool {
    let mut p = k;
    while p > 0 && matches!(els[p - 1], El::Code(Code::TblOpt(_))) {
        p -= 1;
    }
    p > 0 && matches!(els[p - 1], El::Code(Code::Row | Code::TblDef { .. }))
}

/// For the range `from..to`: which elements are structure codes of a table
/// that lies only partly inside the range.  Those must stay when the range
/// is deleted, and must not be copied (a table is deleted or copied as a
/// whole, or only the text in its cells).  `None` when there are none.
pub fn protected_in(els: &[El], from: usize, to: usize) -> Option<Vec<bool>> {
    let to = to.min(els.len());
    if from >= to || !els[from..to].iter().any(is_table_code) {
        return None;
    }
    let mut flags = vec![false; to - from];
    let mut any = false;
    let mut i = 0;
    while i < to {
        if let El::Code(Code::TblDef { .. }) = els[i] {
            // this table: def .. off (or the next [Tbl Def] / the end)
            let mut off = i + 1;
            while off < els.len() && !matches!(els[off], El::Code(Code::TblOff | Code::TblDef { .. })) {
                off += 1;
            }
            let closed = matches!(els.get(off), Some(El::Code(Code::TblOff)));
            let last = if closed { off } else { off.saturating_sub(1) };
            let whole = i >= from && last < to;
            if !whole {
                for k in i.max(from)..(last + 1).min(to) {
                    if is_table_code(&els[k]) {
                        flags[k - from] = true;
                        any = true;
                    }
                }
            }
            i = if closed { off + 1 } else { off };
        } else {
            i += 1;
        }
    }
    any.then_some(flags)
}

/// The text of `from..to` for copying: structure codes of partly covered
/// tables become tabs (between cells) and hard returns (between rows).
pub fn copy_range(els: &[El], from: usize, to: usize) -> Vec<El> {
    let to = to.min(els.len());
    let Some(flags) = protected_in(els, from, to) else { return els[from..to].to_vec() };
    let mut out: Vec<El> = Vec::new();
    for k in from..to {
        if !flags[k - from] {
            out.push(els[k].clone());
            continue;
        }
        let first_in_row = first_in_row(els, k);
        match &els[k] {
            El::Code(Code::Cell) if !first_in_row && !out.is_empty() => out.push(El::Code(Code::Tab)),
            El::Code(Code::Row) if !first_in_row && !out.is_empty() => out.push(El::Code(Code::HRt)),
            El::Code(Code::TblOff) if !out.is_empty() => out.push(El::Code(Code::HRt)),
            _ => {}
        }
    }
    out
}

/// Delete [Tbl Def]: turn the table into ordinary text (cells → tabs,
/// rows → hard returns).
pub fn remove_structure(e: &mut Editor, def: usize) {
    let Some(t) = parse(&e.doc.els, def) else { return };
    let mut out: Vec<El> = Vec::new();
    let els = std::mem::take(&mut e.doc.els);
    for (i, el) in els.into_iter().enumerate() {
        if i == t.def || (i == t.off && t.off < usize::MAX) {
            if i == t.off {
                out.push(El::Code(Code::HRt));
            }
            continue;
        }
        if i > t.def && i < t.off {
            match el {
                El::Code(Code::Row) => {
                    if t.row_codes.first() != Some(&i) {
                        out.push(El::Code(Code::HRt));
                    }
                    continue;
                }
                El::Code(Code::Cell) => {
                    let first_in_row = t.rows.iter().any(|r| r.first() == Some(&i));
                    if !first_in_row {
                        out.push(El::Code(Code::Tab));
                    }
                    continue;
                }
                // what only means something in a table goes with it
                El::Code(Code::Join { .. } | Code::TblOpt(_) | Code::CellFmt(_)) => continue,
                _ => {}
            }
        }
        out.push(el);
    }
    e.doc.els = out;
    e.cursor = def.min(e.doc.els.len());
    e.modified = true;
    e.relayout();
}

// ------------------------------------------------------------- Alt-F7

pub fn columns_tables_menu(wp: &mut Wp) {
    if table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).is_some() {
        wp.push(Box::new(TableEdit::default()));
        return;
    }
    wp.push(MenuLine::new(&["~Columns", "~Tables", "~Math"], |wp, k| match k {
        1 => Some(MenuLine::new(&["~Create", "~Edit"], |wp, k| {
            if k == 1 {
                if table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).is_some() {
                    return Some(Box::new(TableEdit::default()) as Box<dyn Screen>);
                }
                wp.flash("ERROR: Not in a table");
                return None;
            }
            Some(Prompt::new("Number of Columns:", "3", |_wp, s| {
                let cols: usize = s?.trim().parse().ok().filter(|c| (1..=32).contains(c))?;
                Some(Prompt::new("Number of Rows:", "1", move |wp, s| {
                    let rows: usize = s?.trim().parse().ok().filter(|r| (1..=32765).contains(r))?;
                    let e = wp.ed();
                    let f = e.format_state_at(e.cursor);
                    let width = e.doc.init.page_width - f.left - f.right;
                    let els = new_table(cols, rows, width);
                    // a table starts on its own line
                    if e.cursor > 0 && !matches!(e.doc.els.get(e.cursor - 1), Some(El::Code(Code::HRt))) {
                        e.insert_code(Code::HRt);
                    }
                    let at = e.cursor;
                    e.insert_els(&els);
                    e.relayout();
                    if let Some(t) = parse(&e.doc.els, at) {
                        goto_cell(e, &t, 0, 0);
                    }
                    wp.after_edit();
                    Some(Box::new(TableEdit::default()) as Box<dyn Screen>)
                }) as Box<dyn Screen>)
            }) as Box<dyn Screen>)
        }) as Box<dyn Screen>),
        0 => Some(crate::columns::columns_menu_line()),
        2 => Some(crate::math::math_columns_menu()),
        _ => {
            crate::menus::not_yet(wp, "Math");
            None
        }
    }));
}

/// The Table Edit screen (Alt-F7 inside a table).
#[derive(Default)]
pub struct TableEdit {
    /// Block on (Alt-F4): the cell where the block of cells started.
    block: Option<(usize, usize)>,
}

/// All cell contents of a table (rows × cells).
fn contents(els: &[El], t: &Table) -> Vec<Vec<Vec<El>>> {
    (0..t.rows.len())
        .map(|r| {
            (0..t.rows[r].len())
                .map(|c| {
                    let (a, b) = t.cell_range(els, r, c);
                    els[a..b].to_vec()
                })
                .collect()
        })
        .collect()
}

/// Write the table back from its cell contents.
fn rebuild(e: &mut Editor, t: &Table, cells: Vec<Vec<Vec<El>>>) {
    let mut v = vec![El::Code(Code::TblDef { cols: t.cols.clone() })];
    // what stands between [Tbl Def] and the first row stays ([Tbl Opt])
    let first = t.row_codes.first().copied().unwrap_or(t.off).min(e.doc.els.len());
    v.extend(e.doc.els[(t.def + 1).min(first)..first].iter().cloned());
    for row in cells {
        v.push(El::Code(Code::Row));
        for cell in row {
            v.push(El::Code(Code::Cell));
            v.extend(cell);
        }
    }
    v.push(El::Code(Code::TblOff));
    let end = (t.off + 1).min(e.doc.els.len());
    e.doc.els.splice(t.def..end, v);
    e.modified = true;
    e.relayout();
}

/// The cells between `a` and `b` (opposite corners): first and last row,
/// first and last column.  Joined cells that reach into the area belong
/// to it as a whole.
fn area(t: &Table, slots: &[Vec<Slot>], a: (usize, usize), b: (usize, usize)) -> (usize, usize, usize, usize) {
    let (mut r0, mut r1) = (a.0.min(b.0), a.0.max(b.0).min(t.rows.len() - 1));
    let (mut c0, mut c1) = (a.1.min(b.1), a.1.max(b.1).min(t.cols.len() - 1));
    loop {
        let before = (r0, r1, c0, c1);
        for r in 0..t.rows.len() {
            for c in 0..t.rows[r].len() {
                let (ar, ac) = match slots[r][c] {
                    Slot::Covered { r, c } => (r, c),
                    _ => (r, c),
                };
                let Slot::Anchor { cols, rows } = slots[ar][ac] else { continue };
                let hit = ar <= r1 && ar + rows > r0 && ac <= c1 && ac + cols > c0;
                if hit {
                    r0 = r0.min(ar);
                    r1 = r1.max(ar + rows - 1);
                    c0 = c0.min(ac);
                    c1 = c1.max(ac + cols - 1);
                }
            }
        }
        if before == (r0, r1, c0, c1) {
            return before;
        }
    }
}

/// Table Edit, 3 Lines: which lines of the cell (or of the block of cells).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Left,
    Right,
    Top,
    Bottom,
    Inside,
    Outside,
    All,
}

const WHICH: [Which; 7] = [Which::Left, Which::Right, Which::Top, Which::Bottom, Which::Inside, Which::Outside, Which::All];

/// Table Edit, 3 Lines: give lines of the current cell — or, with Block
/// on, of the cells from `from` to the current one — the style `style`
/// (`None`: switch the shading of these cells on or off instead).  For a
/// block, Left/Right/Top/Bottom/Outside are the edges of the block as a
/// whole and Inside the lines between its cells.
///
/// A line between two cells is stored once: as the `Left` of the cell on
/// its right or the `Top` of the cell below it (the place the layout looks
/// first, see `layout::table_rules`), and only at the right and bottom
/// edge of the table as `Right` / `Bottom`.  A line that gets the default
/// style is not stored at all.
fn set_lines(wp: &mut Wp, from: Option<(usize, usize)>, which: Which, style: Option<LineStyle>, shade: bool) {
    TableEdit::with_table(wp, |e, t, r, c| {
        let els = &e.doc.els;
        let slots = t.slots(els);
        let (r0, r1, c0, c1) = area(&t, &slots, from.unwrap_or((r, c)), (r, c));
        let (nrows, ncols) = (t.rows.len(), t.cols.len());
        let owner = |r: usize, c: usize| match slots.get(r).and_then(|row| row.get(c)) {
            Some(Slot::Covered { r, c }) => Some((*r, *c)),
            Some(_) => Some((r, c)),
            None => None,
        };
        let mut fmts: std::collections::BTreeMap<(usize, usize), CellFmt> = Default::default();
        let old = |rc: (usize, usize)| t.fmt_of(els, rc.0, rc.1).map(|x| x.1).unwrap_or_default();
        match style {
            None => {
                for rr in r0..=r1 {
                    for cc in c0..=c1 {
                        if let Some(o) = owner(rr, cc) {
                            fmts.entry(o).or_insert_with(|| old(o)).shade = shade;
                        }
                    }
                }
            }
            Some(style) => {
                // the line at column boundary k beside row rr
                let mut set_v = |rr: usize, k: usize| {
                    let left = if k > 0 { owner(rr, k - 1) } else { None };
                    let right = if k < ncols { owner(rr, k) } else { None };
                    if left == right {
                        return; // inside a joined cell
                    }
                    let default = if k == 0 || k == ncols { LineStyle::Double } else { LineStyle::Single };
                    let val = (style != default).then_some(style);
                    match (left, right) {
                        (l, Some(o)) => {
                            fmts.entry(o).or_insert_with(|| old(o)).lines[0] = val;
                            if let Some(l) = l {
                                fmts.entry(l).or_insert_with(|| old(l)).lines[1] = None;
                            }
                        }
                        (Some(l), None) => fmts.entry(l).or_insert_with(|| old(l)).lines[1] = val,
                        (None, None) => {}
                    }
                };
                let (l, rt, tp, bt, inside) = match which {
                    Which::Left => (true, false, false, false, false),
                    Which::Right => (false, true, false, false, false),
                    Which::Top => (false, false, true, false, false),
                    Which::Bottom => (false, false, false, true, false),
                    Which::Inside => (false, false, false, false, true),
                    Which::Outside => (true, true, true, true, false),
                    Which::All => (true, true, true, true, true),
                };
                for rr in r0..=r1 {
                    for k in c0..=c1 + 1 {
                        if (k == c0 && l) || (k == c1 + 1 && rt) || (k > c0 && k <= c1 && inside) {
                            set_v(rr, k);
                        }
                    }
                }
                // the line in front of row b (0 = top of the table) at column cc
                let mut set_h = |b: usize, cc: usize| {
                    let up = if b > 0 { owner(b - 1, cc) } else { None };
                    let down = if b < nrows { owner(b, cc) } else { None };
                    if up == down {
                        return;
                    }
                    let default = if b == 0 || b == nrows { LineStyle::Double } else { LineStyle::Single };
                    let val = (style != default).then_some(style);
                    match (up, down) {
                        (u, Some(o)) => {
                            fmts.entry(o).or_insert_with(|| old(o)).lines[2] = val;
                            if let Some(u) = u {
                                fmts.entry(u).or_insert_with(|| old(u)).lines[3] = None;
                            }
                        }
                        (Some(u), None) => fmts.entry(u).or_insert_with(|| old(u)).lines[3] = val,
                        (None, None) => {}
                    }
                };
                for b in r0..=r1 + 1 {
                    for cc in c0..=c1 {
                        if (b == r0 && tp) || (b == r1 + 1 && bt) || (b > r0 && b <= r1 && inside) {
                            set_h(b, cc);
                        }
                    }
                }
            }
        }
        fmts.retain(|rc, f| *f != old(*rc));
        if fmts.is_empty() {
            return;
        }
        e.snapshot();
        // from the last cell backwards: the cells before keep their place
        for (&(rr, cc), f) in fmts.iter().rev() {
            write_fmt(&mut e.doc.els, &t, rr, cc, *f);
        }
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, t.def) {
            goto_cell(e, &t, r, c);
        }
    });
}

/// Table Edit, 4 Header and 6 Options: change the table-wide settings of
/// the table the cursor is in (one undo step).
fn change_opt(wp: &mut Wp, f: impl FnOnce(&mut TblOpt)) {
    TableEdit::with_table(wp, |e, t, r, c| {
        let old = opt_at(&e.doc.els, t.def);
        let mut new = old;
        f(&mut new);
        if new == old {
            return;
        }
        e.snapshot();
        write_opt(&mut e.doc.els, t.def, new);
        if new.neg_paren != old.neg_paren {
            // results are written again with the other sign
            crate::math::calc_table(&mut e.doc.els, t.def);
        }
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, t.def) {
            goto_cell(e, &t, r, c);
        }
    });
}

fn cur_opt(wp: &Wp) -> TblOpt {
    let e = wp.ed_ref();
    table_at(&e.doc.els, e.cursor).map(|t| opt_at(&e.doc.els, t.def)).unwrap_or_default()
}

/// Table Edit, 6 Options: the Table Options screen.  Exit returns to
/// Table Edit.
struct TableOptions {
    form: Box<crate::menus::Form>,
}

impl Screen for TableOptions {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match self.form.input(wp, i) {
            Outcome::CloseAll => Outcome::Close,
            out => out,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        self.form.draw(wp, g)
    }
}

fn table_options() -> Box<dyn Screen> {
    use crate::menus::{Edit, Form, Item};
    use std::rc::Rc;
    let items = vec![
        Item {
            key: '1',
            labels: vec!["~Spacing Between Text and Lines", "         Left", "         Right", "         Top", "         Bottom"],
            value: Rc::new(|wp| {
                let mut v = vec![String::new()];
                v.extend(cur_opt(wp).pad.iter().map(|p| fmt_inches(*p)));
                v
            }),
            edit: Edit::Fields(Rc::new(|wp, vals| {
                change_opt(wp, |o| {
                    for k in 0..4 {
                        if let Some(m) = vals.get(k + 1).and_then(|s| parse_measure(s)) {
                            o.pad[k] = m.clamp(0, 1200);
                        }
                    }
                })
            })),
        },
        Item {
            key: '2',
            labels: vec!["~Display Negative Results", "         1 = with minus signs", "         2 = with parentheses"],
            value: Rc::new(|wp| vec![if cur_opt(wp).neg_paren { "2".into() } else { "1".into() }]),
            edit: Edit::Choice(vec!["Minus Signs", "Parentheses"], Rc::new(|wp, k| change_opt(wp, |o| o.neg_paren = k == 1))),
        },
        Item {
            key: '3',
            labels: vec!["~Position of Table"],
            value: Rc::new(|wp| {
                vec![match cur_opt(wp).pos {
                    TblPos::Left => "Left".into(),
                    TblPos::Right => "Right".into(),
                    TblPos::Center => "Center".into(),
                    TblPos::Full => "Full".into(),
                    TblPos::Set(x) => format!("{} from left edge", fmt_inches(x)),
                }]
            }),
            edit: Edit::Choice(
                vec!["Left", "Right", "Center", "Full", "Set Position"],
                Rc::new(|wp, k| match k {
                    4 => {
                        // where the table stands now
                        let e = wp.ed_ref();
                        let now = match cur_opt(wp).pos {
                            TblPos::Set(x) => x,
                            _ => e.format_state_at(e.cursor).left,
                        };
                        wp.push(Prompt::new("Offset from left edge:", &fmt_inches(now), |wp, s| {
                            if let Some(x) = s.and_then(|s| parse_measure(&s)) {
                                change_opt(wp, |o| o.pos = TblPos::Set(x.max(0)));
                                wp.after_edit();
                            }
                            None
                        }));
                    }
                    _ => change_opt(wp, |o| o.pos = [TblPos::Left, TblPos::Right, TblPos::Center, TblPos::Full][k]),
                }),
            ),
        },
        Item {
            key: '4',
            labels: vec!["~Gray Shading (% of black)"],
            value: Rc::new(|wp| vec![format!("{}%", cur_opt(wp).gray)]),
            edit: Edit::Fields(Rc::new(|wp, vals| {
                if let Some(g) = vals.first().and_then(|s| s.trim().trim_end_matches('%').trim().parse::<u32>().ok()) {
                    change_opt(wp, |o| o.gray = g.min(100) as u8);
                }
            })),
        },
    ];
    let mut form = Form::new("Table Options", items);
    form.headings = true;
    Box::new(TableOptions { form })
}

/// Table Edit, 7 Join: the cells from (r0, c0) to (r1, c1) become one.
/// Their text is collected in the first cell.
fn join_cells(wp: &mut Wp, a: (usize, usize), b: (usize, usize)) {
    TableEdit::with_table(wp, |e, t, _, _| {
        let slots = t.slots(&e.doc.els);
        let (r0, r1, c0, c1) = area(&t, &slots, a, b);
        if (r0, c0) == (r1, c1) {
            return;
        }
        e.snapshot();
        let mut cells = contents(&e.doc.els, &t);
        let mut joined: Vec<El> = Vec::new();
        for r in r0..=r1 {
            for c in c0..=c1.min(cells[r].len().saturating_sub(1)) {
                let mut cell = std::mem::take(&mut cells[r][c]);
                // (the joined cell keeps the lines and shading of its first cell)
                cell.retain(|el| !matches!(el, El::Code(Code::Join { .. })) && ((r, c) == (r0, c0) || !matches!(el, El::Code(Code::CellFmt(_)))));
                let has_text = cell.iter().any(|el| !crate::editor::is_invisible(el));
                if has_text || (r, c) == (r0, c0) {
                    if joined.iter().any(|el| !crate::editor::is_invisible(el)) && has_text {
                        joined.push(El::Code(Code::HRt));
                    }
                    joined.extend(cell);
                }
            }
        }
        joined.insert(0, El::Code(Code::Join { cols: (c1 - c0 + 1) as u8, rows: (r1 - r0 + 1) as u8 }));
        cells[r0][c0] = joined;
        rebuild(e, &t, cells);
        if let Some(t) = parse(&e.doc.els, t.def) {
            goto_cell(e, &t, r0, c0);
        }
    });
}

/// Table Edit, 8 Split: a joined cell becomes separate cells again.
fn split_cell(wp: &mut Wp) -> bool {
    TableEdit::with_table(wp, |e, t, r, c| {
        let Some((i, _, _)) = t.join_of(&e.doc.els, r, c) else { return false };
        e.snapshot();
        e.doc.els.remove(i);
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, t.def) {
            goto_cell(e, &t, r, c);
        }
        true
    })
    .unwrap_or(false)
}

impl TableEdit {
    fn with_table<R>(wp: &mut Wp, f: impl FnOnce(&mut Editor, Table, usize, usize) -> R) -> Option<R> {
        let e = wp.ed();
        let t = table_at(&e.doc.els, e.cursor)?;
        let (r, c) = t.locate(e.cursor)?;
        Some(f(e, t, r, c))
    }
}

fn set_cols(e: &mut Editor, t: &Table, cols: Vec<i32>) {
    e.snapshot();
    e.doc.els[t.def] = El::Code(Code::TblDef { cols });
    e.modified = true;
    e.relayout();
}

fn insert_rows(wp: &mut Wp, n: usize) {
    TableEdit::with_table(wp, |e, t, r, _| {
        e.snapshot();
        let ncols = t.cols.len();
        let at = t.row_codes[r];
        let mut v = vec![];
        for _ in 0..n {
            v.push(El::Code(Code::Row));
            for _ in 0..ncols {
                v.push(El::Code(Code::Cell));
            }
        }
        let tail = e.doc.els.split_off(at);
        e.doc.els.extend(v);
        e.doc.els.extend(tail);
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, t.def) {
            goto_cell(e, &t, r, 0);
        }
    });
}

fn insert_cols(wp: &mut Wp, n: usize) {
    TableEdit::with_table(wp, |e, t, r, c| {
        e.snapshot();
        let mut cols = t.cols.clone();
        // split the current column's width
        let w = (cols[c] / (n as i32 + 1)).max(240);
        for _ in 0..n {
            cols.insert(c, w);
        }
        cols[c + n] = w;
        // insert cells before column c in every row (from the end backwards)
        let mut points: Vec<usize> = t.rows.iter().filter_map(|row| row.get(c).copied()).collect();
        points.sort_unstable_by(|a, b| b.cmp(a));
        for p in points {
            for _ in 0..n {
                e.doc.els.insert(p, El::Code(Code::Cell));
            }
        }
        let def = t.def;
        e.doc.els[def] = El::Code(Code::TblDef { cols });
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, def) {
            goto_cell(e, &t, r, c);
        }
    });
}

/// Add `n` columns at the right edge (Table Edit, Size).
fn append_cols(wp: &mut Wp, n: usize) {
    TableEdit::with_table(wp, |e, t, r, c| {
        e.snapshot();
        let mut cols = t.cols.clone();
        // share the last column's width with the new ones
        let last = cols.len() - 1;
        let w = (cols[last] / (n as i32 + 1)).max(240);
        cols[last] = w;
        cols.extend(std::iter::repeat_n(w, n));
        // new cells at the end of every row (from the last row backwards)
        let mut ends: Vec<usize> = (0..t.rows.len()).map(|ri| t.row_codes.get(ri + 1).copied().unwrap_or(t.off)).collect();
        ends.sort_unstable_by(|a, b| b.cmp(a));
        for p in ends {
            for _ in 0..n {
                e.doc.els.insert(p, El::Code(Code::Cell));
            }
        }
        let def = t.def;
        e.doc.els[def] = El::Code(Code::TblDef { cols });
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, def) {
            goto_cell(e, &t, r, c);
        }
    });
}

const KEEP_ONE: &str = "ERROR: A table needs one row and one column — to remove the table, delete its [Tbl Def] code";

fn delete_rows(wp: &mut Wp, n: usize) {
    let refused = TableEdit::with_table(wp, |e, t, r, _| {
        // as many as there are from this row down, but not the whole table
        let n = n.min(t.rows.len() - r);
        if t.rows.len() <= n {
            return true;
        }
        e.snapshot();
        let a = t.row_codes[r];
        let b = t.row_codes.get(r + n).copied().unwrap_or(t.off);
        let removed: Vec<El> = e.doc.els.drain(a..b).collect();
        e.undelete.insert(0, removed);
        e.undelete.truncate(3);
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, t.def) {
            goto_cell(e, &t, r.min(t.rows.len() - 1), 0);
        }
        false
    });
    if refused == Some(true) {
        wp.flash(KEEP_ONE);
    }
}

fn delete_cols(wp: &mut Wp, n: usize) {
    let refused = TableEdit::with_table(wp, |e, t, r, c| {
        let n = n.min(t.cols.len() - c);
        if t.cols.len() <= n {
            return true;
        }
        e.snapshot();
        let def = t.def;
        // remove cells c..c+n in every row, from the end backwards
        let mut ranges: Vec<(usize, usize)> = vec![];
        for (ri, row) in t.rows.iter().enumerate() {
            if c < row.len() {
                let a = row[c];
                let last = (c + n - 1).min(row.len() - 1);
                let (_, b) = t.cell_range(&e.doc.els, ri, last);
                ranges.push((a, b));
            }
        }
        ranges.sort_unstable_by(|x, y| y.0.cmp(&x.0));
        for (a, b) in ranges {
            e.doc.els.drain(a..b);
        }
        let mut cols = t.cols.clone();
        let freed: i32 = cols.drain(c..c + n).sum();
        if let Some(last) = cols.last_mut() {
            *last += freed;
        }
        e.doc.els[def] = El::Code(Code::TblDef { cols });
        e.modified = true;
        e.relayout();
        if let Some(t) = parse(&e.doc.els, def) {
            goto_cell(e, &t, r, c.min(t.cols.len() - 1));
        }
        false
    });
    if refused == Some(true) {
        wp.flash(KEEP_ONE);
    }
}

impl Screen for TableEdit {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::Menu(298)
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let moved = |wp: &mut Wp, dr: isize, dc: isize| {
            TableEdit::with_table(wp, |e, t, r, c| {
                // step over the cells that belong to the same joined cell
                let slots = t.slots(&e.doc.els);
                let owner = |r: usize, c: usize| match slots[r][c.min(slots[r].len() - 1)] {
                    Slot::Covered { r, c } => (r, c),
                    _ => (r, c.min(slots[r].len() - 1)),
                };
                let here = owner(r, c);
                let (mut nr, mut nc) = (r as isize, c as isize);
                loop {
                    let (tr, tc) = (nr + dr, nc + dc);
                    if tr < 0 || tc < 0 || tr >= t.rows.len() as isize || tc >= t.cols.len() as isize {
                        break;
                    }
                    (nr, nc) = (tr, tc);
                    if owner(nr as usize, nc as usize) != here {
                        break;
                    }
                }
                let (nr, nc) = owner(nr as usize, nc as usize);
                goto_cell(e, &t, nr, nc);
            });
            wp.after_edit();
        };
        match i {
            Input::Func(Func::Cancel) if self.block.is_some() => {
                self.block = None;
                wp.ed().block_anchor = None;
            }
            Input::Func(Func::Exit) | Input::Func(Func::ColumnsTable) | Input::Func(Func::Cancel) => {
                wp.ed().block_anchor = None;
                return Outcome::Close;
            }
            // Block on/off: mark cells for Join
            Input::Func(Func::Block) => {
                if self.block.take().is_some() {
                    wp.ed().block_anchor = None;
                } else {
                    self.block = TableEdit::with_table(wp, |e, t, r, c| {
                        e.block_anchor = Some(t.cell_range(&e.doc.els, r, c).0);
                        (r, c)
                    });
                }
            }
            Input::Char('7') | Input::Char('j') | Input::Char('J') => {
                let Some(from) = self.block.take() else {
                    wp.flash("Block the cells first: Alt-F4, move to the last cell, then 7 Join");
                    return Outcome::Keep;
                };
                wp.ed().block_anchor = None;
                let Some(to) = TableEdit::with_table(wp, |_, _, r, c| (r, c)) else { return Outcome::Keep };
                return Outcome::Push(YesNo::new("Join cells?", false, move |wp, yes| {
                    if yes {
                        join_cells(wp, from, to);
                        wp.after_edit();
                    }
                    None
                }));
            }
            Input::Char('8') | Input::Char('p') | Input::Char('P') => {
                if !split_cell(wp) {
                    wp.flash("This cell is not joined — Split separates cells that were joined with 7 Join");
                }
                wp.after_edit();
            }
            Input::Func(Func::Up) => moved(wp, -1, 0),
            Input::Func(Func::Down) => moved(wp, 1, 0),
            Input::Func(Func::Left) | Input::Func(Func::MarRel) => moved(wp, 0, -1),
            Input::Func(Func::Right) | Input::Func(Func::Tab) => moved(wp, 0, 1),
            Input::Func(Func::WordLeft) | Input::Func(Func::WordRight) => {
                let grow = matches!(i, Input::Func(Func::WordRight));
                TableEdit::with_table(wp, |e, t, _, c| {
                    let mut cols = t.cols.clone();
                    cols[c] = (cols[c] + if grow { 120 } else { -120 }).max(240);
                    set_cols(e, &t, cols);
                });
                wp.after_edit();
            }
            Input::Func(Func::Typeover) => {
                return Outcome::Push(MenuLine::new(&["~Rows", "~Columns"], |_wp, k| {
                    let label = if k == 0 { "Number of Rows:" } else { "Number of Columns:" };
                    Some(Prompt::new(label, "1", move |wp, s| {
                        let n: usize = s?.trim().parse().ok().filter(|n| *n >= 1)?;
                        if k == 0 { insert_rows(wp, n) } else { insert_cols(wp, n) }
                        wp.after_edit();
                        None
                    }) as Box<dyn Screen>)
                }).prefix("Insert: "));
            }
            Input::Func(Func::Del) => {
                return Outcome::Push(MenuLine::new(&["~Rows", "~Columns"], |_wp, k| {
                    let label = if k == 0 { "Number of Rows:" } else { "Number of Columns:" };
                    Some(Prompt::new(label, "1", move |wp, s| {
                        let n: usize = s?.trim().parse().ok().filter(|n| *n >= 1)?;
                        if k == 0 { delete_rows(wp, n) } else { delete_cols(wp, n) }
                        wp.after_edit();
                        None
                    }) as Box<dyn Screen>)
                }).prefix("Delete: "));
            }
            Input::Char('1') | Input::Char('s') | Input::Char('S') => {
                return Outcome::Push(MenuLine::new(&["~Rows", "~Columns"], |wp, k| {
                    let cur = TableEdit::with_table(wp, |_, t, _, _| (t.rows.len(), t.cols.len())).unwrap_or((1, 1));
                    let (label, v) = if k == 0 { ("Number of Rows:", cur.0) } else { ("Number of Columns:", cur.1) };
                    Some(Prompt::new(label, &v.to_string(), move |wp, s| {
                        let n: usize = s?.trim().parse().ok().filter(|n| *n >= 1)?;
                        let (rows, cols) = TableEdit::with_table(wp, |_, t, _, _| (t.rows.len(), t.cols.len()))?;
                        // add / remove at the end of the table
                        if k == 0 {
                            TableEdit::with_table(wp, |e, t, _, _| goto_cell(e, &t, t.rows.len() - 1, 0));
                            if n > rows {
                                // append rows after the last one
                                TableEdit::with_table(wp, |e, t, _, _| {
                                    e.snapshot();
                                    let mut v = vec![];
                                    for _ in 0..n - rows {
                                        v.push(El::Code(Code::Row));
                                        for _ in 0..t.cols.len() {
                                            v.push(El::Code(Code::Cell));
                                        }
                                    }
                                    let tail = e.doc.els.split_off(t.off);
                                    e.doc.els.extend(v);
                                    e.doc.els.extend(tail);
                                    e.modified = true;
                                    e.relayout();
                                });
                            } else if n < rows {
                                TableEdit::with_table(wp, |e, t, _, _| goto_cell(e, &t, n, 0));
                                delete_rows(wp, rows - n);
                            }
                        } else if n > cols {
                            append_cols(wp, n - cols);
                        } else if n < cols {
                            TableEdit::with_table(wp, |e, t, r, _| goto_cell(e, &t, r, n));
                            delete_cols(wp, cols - n);
                        }
                        wp.after_edit();
                        None
                    }) as Box<dyn Screen>)
                }));
            }
            Input::Char('2') | Input::Char('f') | Input::Char('F') => {
                return Outcome::Push(MenuLine::new(&["~Cell", "C~olumn"], |_wp, scope| {
                    Some(MenuLine::new(&["~Left", "~Center", "~Right", "~Full"], move |wp, k| {
                        let j = [Justify::Left, Justify::Center, Justify::Right, Justify::Full][k];
                        TableEdit::with_table(wp, |e, t, r, c| {
                            e.snapshot();
                            let targets: Vec<(usize, usize)> = if scope == 0 {
                                vec![(r, c)]
                            } else {
                                (0..t.rows.len()).filter(|rr| c < t.rows[*rr].len()).map(|rr| (rr, c)).collect()
                            };
                            // [Just:x] at the start of each cell, and the
                            // justification that was in effect again where
                            // the next cell (or the text after the table)
                            // starts, so it stays in this cell (back to front)
                            let mut spans: Vec<(usize, usize)> = targets
                                .iter()
                                .map(|&(rr, cc)| {
                                    let (s, end) = t.cell_range(&e.doc.els, rr, cc);
                                    // first place after the codes that end the cell
                                    // ([Cell], [Row][Cell] or [Tbl Off])
                                    let mut after = (end + 1).min(e.doc.els.len());
                                    if matches!(e.doc.els.get(end), Some(El::Code(Code::Row))) && matches!(e.doc.els.get(after), Some(El::Code(Code::Cell))) {
                                        after += 1;
                                    }
                                    (s, after)
                                })
                                .collect();
                            spans.sort_unstable_by(|a, b| b.0.cmp(&a.0));
                            for (s, after) in spans {
                                let before = e.format_state_at(s).just;
                                if !matches!(e.doc.els.get(after), Some(El::Code(Code::Just(_)))) {
                                    e.doc.els.insert(after, El::Code(Code::Just(before)));
                                }
                                if let Some(El::Code(Code::Just(_))) = e.doc.els.get(s) {
                                    e.doc.els[s] = El::Code(Code::Just(j));
                                } else {
                                    e.doc.els.insert(s, El::Code(Code::Just(j)));
                                }
                            }
                            e.modified = true;
                            e.relayout();
                            if let Some(t) = parse(&e.doc.els, t.def) {
                                goto_cell(e, &t, r, c);
                            }
                        });
                        wp.after_edit();
                        None
                    }) as Box<dyn Screen>)
                }).prefix("Format: "));
            }
            Input::Char('3') | Input::Char('l') | Input::Char('L') => {
                // (the block ends with the menu, as with Join)
                let from = self.block.take();
                wp.ed().block_anchor = None;
                return Outcome::Push(MenuLine::new(&["~Left", "~Right", "~Top", "~Bottom", "~Inside", "~Outside", "~All", "~Shade"], move |_wp, k| {
                    if k == 7 {
                        return Some(MenuLine::new(&["~On", "O~ff"], move |wp, on| {
                            set_lines(wp, from, Which::All, None, on == 0);
                            wp.after_edit();
                            None
                        }).prefix("Shade: ") as Box<dyn Screen>);
                    }
                    Some(MenuLine::new(&["~None", "~Single", "~Double", "D~ashed", "D~otted", "~Thick", "~Extra Thick"], move |wp, s| {
                        set_lines(wp, from, WHICH[k], LineStyle::from_index(s), false);
                        wp.after_edit();
                        None
                    }) as Box<dyn Screen>)
                }).prefix("Lines: "));
            }
            Input::Char('4') | Input::Char('h') | Input::Char('H') => {
                let cur = cur_opt(wp).header_rows;
                return Outcome::Push(Prompt::new("Number of header rows:", &cur.to_string(), |wp, s| {
                    let n: u16 = s?.trim().parse().ok()?;
                    let rows = TableEdit::with_table(wp, |_, t, _, _| t.rows.len())?;
                    change_opt(wp, |o| o.header_rows = n.min(rows as u16));
                    wp.after_edit();
                    None
                }));
            }
            Input::Char('6') | Input::Char('o') | Input::Char('O') => return Outcome::Push(table_options()),
            Input::Char('5') | Input::Char('m') | Input::Char('M') => return Outcome::Push(crate::math::table_math_menu()),
            Input::Char(_) => {}
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let e = wp.ed_ref();
        let status_row = wp.status_row();
        let rows = g.rows;
        let addr = table_at(&e.doc.els, e.cursor)
            .and_then(|t| t.locate(e.cursor))
            .map(|(r, c)| format!("Cell {}{}", col_letter(c), r + 1))
            .unwrap_or_default();
        // WP shows the table edit status at the status line position
        g.fill_row(status_row, 0, g.cols / 2, ' ', st);
        g.text(0, status_row, "Table Edit: Press Exit when done", st);
        let c = g.cols.saturating_sub(addr.len() + 32);
        g.text(c, status_row, &addr, st);
        if rows >= 3 {
            g.fill_row(rows - 2, 0, g.cols, ' ', st);
            g.fill_row(rows - 1, 0, g.cols, ' ', st);
            // (on a screen where the menu covers the status line, the
            // cell is shown at the end of the line above the menu)
            if self.block.is_some() {
                // the cells from where Block started to the cursor are
                // what Lines and Join work on
                let c = g.text(0, rows - 2, "Block on", wp.theme.bold);
                g.text(c, rows - 2, "; move to the last cell, then 3 Lines or 7 Join", st);
            } else {
                g.text(0, rows - 2, "Ctrl-Arrows Column Widths; Ins Insert; Del Delete; Alt-F4 Block;", st);
            }
            if status_row + 2 >= rows {
                g.text(g.cols.saturating_sub(addr.len() + 1), rows - 2, &addr, st);
            }
            let c = g.text_mn(0, rows - 1, "1 ~Size; 2 ~Format; 3 ~Lines; 4 ~Header; 5 ~Math; 6 ~Options; 7 ~Join; 8 S~plit: ", st, wp.theme.bold);
            g.text(c, rows - 1, "0", st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letters() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
    }
    #[test]
    fn structure() {
        let els = new_table(3, 2, 7200);
        let t = parse(&els, 0).unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0].len(), 3);
        assert_eq!(t.off, els.len() - 1);
        let mut e = Editor::with_doc(Document::new(), None);
        e.doc.els = els;
        e.relayout();
        goto_cell(&mut e, &t, 1, 2);
        assert_eq!(t.locate(e.cursor), Some((1, 2)));
        // layout: top border, row 1, mid border, row 2, bottom border
        use crate::layout::VRow;
        let borders = e.layout.vrows.iter().filter(|v| matches!(v, VRow::Border { .. })).count();
        assert_eq!(borders, 3, "{:?}", e.layout.vrows);
        let text_rows = e.layout.vrows.iter().filter(|v| matches!(v, VRow::Lines(ls) if ls.len() == 3)).count();
        assert_eq!(text_rows, 2, "{:?}", e.layout.vrows);
    }

    // ---- regression tests from the table test pass (issue #1) ----

    fn codes(wp: &Wp) -> String {
        wp.ed_ref().doc.els.iter().map(|e| match e {
            El::Ch(c) => c.to_string(),
            El::Code(c) => crate::reveal::code_text(c),
        }).collect()
    }

    fn keys(wp: &mut Wp, s: &str) {
        for c in s.chars() {
            wp.input(Input::Char(c));
        }
    }

    /// A 3×3 table a1…c3, cursor at the end of c3.
    fn table33() -> Wp {
        let mut wp = Wp::new(80, 25);
        wp.input(Input::Func(Func::ColumnsTable));
        keys(&mut wp, "213");
        wp.input(Input::Func(Func::Enter));
        keys(&mut wp, "3");
        wp.input(Input::Func(Func::Enter));
        wp.input(Input::Func(Func::Exit));
        for (k, cell) in ["a1", "b1", "c1", "a2", "b2", "c2", "a3", "b3", "c3"].iter().enumerate() {
            if k > 0 {
                wp.input(Input::Func(Func::Tab));
            }
            keys(&mut wp, cell);
        }
        wp
    }

    fn f(wp: &mut Wp, func: Func) {
        wp.input(Input::Func(func));
    }

    #[test]
    fn left_and_right_stop_at_every_cell_edge() {
        let mut wp = table33();
        for _ in 0..3 {
            f(&mut wp, Func::Left);
        }
        keys(&mut wp, "L"); // end of b3, not inside it
        assert!(codes(&wp).contains("[Cell]b3L[Cell]c3"), "{}", codes(&wp));
        // from the end of a row, Right goes to the first cell of the next
        let mut wp = table33();
        f(&mut wp, Func::Up);
        f(&mut wp, Func::Right);
        keys(&mut wp, "R");
        assert!(codes(&wp).contains("[Row][Cell]Ra3"), "{}", codes(&wp));
    }

    #[test]
    fn undo_takes_back_one_cell_at_a_time() {
        let mut wp = table33();
        wp.ed().undo();
        let c = codes(&wp);
        assert!(c.contains("[Cell]b3[Cell][Tbl Off]"), "{c}");
        wp.ed().undo();
        assert!(codes(&wp).contains("[Cell]a3[Cell][Cell][Tbl Off]"), "{}", codes(&wp));
    }

    #[test]
    fn deleting_part_of_a_table_keeps_its_structure() {
        let mut wp = table33();
        let t = table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).unwrap();
        // from the middle of b2 to the middle of b3
        let from = t.rows[1][1] + 2;
        let to = t.rows[2][1] + 2;
        wp.ed().delete_range(from, to, true);
        let c = codes(&wp);
        assert!(c.ends_with("[Row][Cell]a2[Cell]b[Cell][Row][Cell][Cell]3[Cell]c3[Tbl Off]"), "{c}");
        // … and copying it gives text with tabs and returns
        let wp = table33();
        let got: String = copy_range(&wp.ed_ref().doc.els, from, to).iter().map(|e| match e {
            El::Ch(c) => c.to_string(),
            El::Code(c) => crate::reveal::code_text(c),
        }).collect();
        assert_eq!(got, "2[Tab]c2[HRt]a3[Tab]b");
        // a whole table in the range goes as a whole
        let mut wp = table33();
        let n = wp.ed_ref().doc.els.len();
        wp.ed().delete_range(0, n, true);
        assert!(wp.ed_ref().doc.els.is_empty());
    }

    #[test]
    fn cell_justification_stays_in_its_cell() {
        let mut wp = table33();
        f(&mut wp, Func::Up);
        wp.input(Input::Func(Func::MarRel)); // Shift-Tab: b2
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "2cr");
        f(&mut wp, Func::Exit);
        let c = codes(&wp);
        assert!(c.contains("[Cell][Just:Right]b2[Cell][Just:"), "{c}");
        assert!(!c.contains("[Just:Right]c2") && c.matches("[Just:Right]").count() == 1, "{c}");
    }

    #[test]
    fn size_adds_columns_at_the_right_and_delete_is_clamped() {
        let mut wp = table33();
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "1c5");
        f(&mut wp, Func::Enter);
        let c = codes(&wp);
        assert!(c.contains("[Cell]a1[Cell]b1[Cell]c1[Cell][Cell][Row]"), "{c}");
        // deleting more rows than there are below: down to the last row,
        // and one Ctrl-Z brings them back
        let mut wp = table33();
        f(&mut wp, Func::Up);
        f(&mut wp, Func::ColumnsTable);
        f(&mut wp, Func::Del);
        keys(&mut wp, "r9");
        f(&mut wp, Func::Enter);
        f(&mut wp, Func::Exit);
        assert!(codes(&wp).ends_with("[Cell]c1[Tbl Off]"), "{}", codes(&wp));
        wp.ed().undo();
        assert!(codes(&wp).ends_with("[Cell]c3[Tbl Off]"), "{}", codes(&wp));
        // the whole table can't be deleted this way
        let mut wp = table33();
        f(&mut wp, Func::ColumnsTable);
        wp.ed().cursor = 3; // cell a1
        f(&mut wp, Func::Del);
        keys(&mut wp, "r3");
        f(&mut wp, Func::Enter);
        assert!(wp.message.as_deref().unwrap_or("").starts_with("ERROR"), "{:?}", wp.message);
    }

    #[test]
    fn plain_text_separates_cells_and_rows() {
        let wp = table33();
        let d = &wp.ed_ref().doc;
        assert_eq!(d.plain_text(0, d.len()), "a1\tb1\tc1\na2\tb2\tc2\na3\tb3\tc3\n");
    }

    #[test]
    fn join_and_split() {
        let mut wp = table33();
        // A1:B1 → one cell holding both texts
        f(&mut wp, Func::ColumnsTable);
        wp.ed().cursor = 3;
        f(&mut wp, Func::Block);
        f(&mut wp, Func::Right);
        keys(&mut wp, "7y");
        let c = codes(&wp);
        assert!(c.contains("[Row][Cell][Join:2x1]a1[HRt]b1[Cell][Cell]c1[Row]"), "{c}");
        let t = table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).unwrap();
        let s = t.slots(&wp.ed_ref().doc.els);
        assert_eq!(s[0], vec![Slot::Anchor { cols: 2, rows: 1 }, Slot::Covered { r: 0, c: 0 }, Slot::Normal]);
        // Right in Table Edit goes to C1, not into the hidden cell
        f(&mut wp, Func::Right);
        assert_eq!(t.locate(wp.ed_ref().cursor), Some((0, 2)));
        f(&mut wp, Func::Left);
        assert_eq!(t.locate(wp.ed_ref().cursor), Some((0, 0)));
        f(&mut wp, Func::Exit);
        // Tab and the arrow keys skip the hidden cell too
        f(&mut wp, Func::Tab);
        assert_eq!(t.locate(wp.ed_ref().cursor), Some((0, 2)));
        f(&mut wp, Func::Left);
        keys(&mut wp, "!");
        assert!(codes(&wp).contains("a1[HRt]b1![Cell][Cell]c1"), "{}", codes(&wp));
        // the layout: the joined cell is as wide as two columns, no rule inside
        let lay = &wp.ed_ref().layout;
        assert!(!lay.tables[0].rule_v(0, 1) && lay.tables[0].rule_v(0, 2) && lay.tables[0].rule_v(1, 1));
        // 8 Split takes the join away again
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "8");
        assert!(!codes(&wp).contains("[Join"), "{}", codes(&wp));
        // joining downwards
        let mut wp = table33();
        f(&mut wp, Func::ColumnsTable);
        wp.ed().cursor = 3;
        f(&mut wp, Func::Block);
        f(&mut wp, Func::Down);
        keys(&mut wp, "7y");
        f(&mut wp, Func::Exit);
        assert!(codes(&wp).contains("[Cell][Join:1x2]a1[HRt]a2[Cell]b1"), "{}", codes(&wp));
        let lay = &wp.ed_ref().layout;
        assert!(!lay.tables[0].rule_h(0, 0) && lay.tables[0].rule_h(0, 1));
    }

    // ---- Table Edit: 3 Lines, 4 Header, 6 Options ----

    use crate::layout::MonoMetrics;
    use LineStyle::{Dashed, Dotted, ExtraThick, Thick};

    /// Table Edit with the cursor in cell (r, c) of a 3×3 table.
    fn edit33(r: usize, c: usize) -> Wp {
        let mut wp = table33();
        go(&mut wp, r, c);
        f(&mut wp, Func::ColumnsTable);
        wp
    }

    fn go(wp: &mut Wp, r: usize, c: usize) {
        let e = wp.ed();
        let t = table_at(&e.doc.els, e.cursor).unwrap();
        goto_cell(e, &t, r, c);
    }

    fn geom(wp: &Wp) -> &crate::layout::TableGeom {
        &wp.ed_ref().layout.tables[0]
    }

    /// The lines of the whole table as text: per row its vertical lines,
    /// then per border its horizontal ones (first letters; `-` = none).
    fn lines(wp: &Wp) -> String {
        let g = geom(wp);
        let ch = |s: LineStyle| ['-', 'S', 'D', 'a', 'o', 'T', 'E'][s as usize];
        let v: Vec<String> = (0..3).map(|r| (0..4).map(|k| ch(g.style_v(r, k))).collect()).collect();
        let h: Vec<String> = (0..4).map(|b| (0..3).map(|c| ch(g.line_above(b, c))).collect()).collect();
        format!("{} / {}", v.join(" "), h.join(" "))
    }

    const PLAIN: &str = "DSSD DSSD DSSD / DDD SSS SSS DDD";

    #[test]
    fn lines_of_a_single_cell() {
        // every choice on B2: (keys, the table's lines, the code it leaves)
        let cases = [
            ("313", "DSSD DDSD DSSD / DDD SSS SSS DDD", "[Cell][Cell Lines:Left Double]b2"),
            ("324", "DSSD DSaD DSSD / DDD SSS SSS DDD", "[Cell][Cell Lines:Left Dashed]c2"),
            ("331", "DSSD DSSD DSSD / DDD S-S SSS DDD", "[Cell][Cell Lines:Top None]b2"),
            ("346", "DSSD DSSD DSSD / DDD SSS STS DDD", "[Cell][Cell Lines:Top Thick]b3"),
            ("367", "DSSD DEED DSSD / DDD SES SES DDD", "[Cell][Cell Lines:Left Extra Thick;Top Extra Thick]b2[Cell][Cell Lines:Left Extra Thick]c2"),
            ("375", "DSSD DooD DSSD / DDD SoS SoS DDD", "[Cell][Cell Lines:Left Dotted;Top Dotted]b2"),
            // (a single cell has no lines inside)
            ("353", PLAIN, "[Cell]b2[Cell]c2"),
        ];
        for (k, want, code) in cases {
            let mut wp = edit33(1, 1);
            let before = codes(&wp);
            keys(&mut wp, k);
            assert_eq!(lines(&wp), want, "{k}");
            assert!(codes(&wp).contains(code), "{k}: {}", codes(&wp));
            // still in Table Edit, in the same cell; one undo step
            let t = table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).unwrap();
            assert_eq!(t.locate(wp.ed_ref().cursor), Some((1, 1)), "{k}");
            if codes(&wp) != before {
                wp.ed().undo();
            }
            assert_eq!(codes(&wp), before, "{k}");
        }
        // letters work as well: Lines, Bottom, Double
        let mut wp = edit33(1, 1);
        keys(&mut wp, "lbd");
        assert_eq!(lines(&wp), "DSSD DSSD DSSD / DDD SSS SDS DDD");
        // the edges of the table: Right and Bottom of the last cell are
        // its own; None there removes the frame
        let mut wp = edit33(2, 2);
        keys(&mut wp, "321");
        keys(&mut wp, "341");
        assert_eq!(lines(&wp), "DSSD DSSD DSS- / DDD SSS SSS DD-");
        assert!(codes(&wp).contains("[Cell][Cell Lines:Right None;Bottom None]c3"), "{}", codes(&wp));
        // the default again: nothing is left behind
        keys(&mut wp, "323");
        keys(&mut wp, "343");
        assert_eq!(lines(&wp), PLAIN);
        assert!(!codes(&wp).contains("[Cell Lines"), "{}", codes(&wp));
    }

    #[test]
    fn a_line_set_last_counts() {
        // B2's left line from B2, then the same line as A2's right line
        let mut wp = edit33(1, 1);
        keys(&mut wp, "313");
        go(&mut wp, 1, 0);
        keys(&mut wp, "326");
        assert_eq!(lines(&wp), "DSSD DTSD DSSD / DDD SSS SSS DDD");
        assert_eq!(codes(&wp).matches("[Cell Lines").count(), 1, "{}", codes(&wp));
        // from a file both sides may be set: the left side of the cell on
        // the right counts, and the top of the cell below
        let mut wp = table33();
        let t = table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).unwrap();
        let (a2, b2, b1) = (t.rows[1][0], t.rows[1][1], t.rows[0][1]);
        let e = wp.ed();
        e.doc.els.insert(b2 + 1, El::Code(Code::CellFmt(CellFmt { lines: [Some(Dotted), None, Some(Dashed), None], shade: false })));
        e.doc.els.insert(a2 + 1, El::Code(Code::CellFmt(CellFmt { lines: [None, Some(Thick), None, None], shade: false })));
        e.doc.els.insert(b1 + 1, El::Code(Code::CellFmt(CellFmt { lines: [None, None, None, Some(ExtraThick)], shade: false })));
        e.relayout();
        assert_eq!(lines(&wp), "DSSD DoSD DSSD / DDD SaS SSS DDD");
    }

    #[test]
    fn lines_of_a_block() {
        // Block A1..B2, then: (keys, the table's lines)
        let cases = [
            ("316", "TSSD TSSD DSSD / DDD SSS SSS DDD"),
            ("326", "DSTD DSTD DSSD / DDD SSS SSS DDD"),
            ("331", "DSSD DSSD DSSD / --D SSS SSS DDD"),
            ("344", "DSSD DSSD DSSD / DDD SSS aaS DDD"),
            ("351", "D-SD D-SD DSSD / DDD --S SSS DDD"),
            ("366", "TSTD TSTD DSSD / TTD SSS TTS DDD"),
            ("373", "DDDD DDDD DSSD / DDD DDS DDS DDD"),
        ];
        for (k, want) in cases {
            let mut wp = edit33(0, 0);
            let before = codes(&wp);
            f(&mut wp, Func::Block);
            f(&mut wp, Func::Right);
            f(&mut wp, Func::Down);
            keys(&mut wp, k);
            assert_eq!(lines(&wp), want, "{k}");
            // Block is off again, and it was one undo step
            assert!(wp.ed_ref().block_anchor.is_none());
            wp.ed().undo();
            assert_eq!(codes(&wp), before, "{k}");
        }
        // a joined cell is one cell: no lines inside it, its outside set
        let mut wp = edit33(0, 0);
        f(&mut wp, Func::Block);
        f(&mut wp, Func::Right);
        keys(&mut wp, "7y");
        keys(&mut wp, "376");
        assert_eq!(lines(&wp), "T-TD DSSD DSSD / TTD TTS SSS DDD");
        assert!(codes(&wp).contains("[Cell][Join:2x1][Cell Lines:Left Thick;Top Thick]a1"), "{}", codes(&wp));
    }

    #[test]
    fn shading() {
        let mut wp = edit33(1, 1);
        let before = codes(&wp);
        keys(&mut wp, "381");
        assert!(codes(&wp).contains("[Cell][Cell Lines:Shade On]b2"), "{}", codes(&wp));
        assert!(geom(&wp).shaded(1, 1) && !geom(&wp).shaded(1, 0) && !geom(&wp).shaded(0, 1));
        // … in print: one gray area, 10 % of black, between the cell's lines
        let e = wp.ed_ref();
        let pages = crate::page::build_pages(&e.doc, &e.layout, &MonoMetrics);
        let fills: Vec<_> = pages[0].images.iter().filter(|p| p.img.is_none()).collect();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].color >> 24, 25);
        let x = &e.layout.tables[0].x;
        assert_eq!((fills[0].dst.0, fills[0].dst.2), (x[1], x[2] - x[1]));
        // … on the edit screen: another background
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let row = (0..25).find(|&r| (0..80).any(|c| g.get(c, r).unwrap().ch == 'b') && (0..80).any(|c| g.get(c, r).unwrap().ch == '2')).unwrap();
        let col = (0..80).find(|&c| g.get(c, row).unwrap().ch == 'b').unwrap();
        assert_ne!(g.get(col, row).unwrap().bg, wp.theme.normal.bg);
        assert_eq!(g.get(col - 3, row).unwrap().bg, wp.theme.normal.bg);
        // Off: the code goes; one undo step each
        keys(&mut wp, "382");
        assert_eq!(codes(&wp), before);
        wp.ed().undo();
        assert!(codes(&wp).contains("Shade On"));
        wp.ed().undo();
        assert_eq!(codes(&wp), before);
        // a block; shading stays beside lines
        f(&mut wp, Func::Block);
        f(&mut wp, Func::Up);
        f(&mut wp, Func::Left);
        keys(&mut wp, "38o");
        assert_eq!(codes(&wp).matches("[Cell Lines:Shade On]").count(), 4, "{}", codes(&wp));
        keys(&mut wp, "316");
        assert!(codes(&wp).contains("[Cell][Cell Lines:Left Thick;Shade On]a1"), "{}", codes(&wp));
        go(&mut wp, 0, 0);
        keys(&mut wp, "38f");
        assert!(codes(&wp).contains("[Cell][Cell Lines:Left Thick]a1"), "{}", codes(&wp));
    }

    #[test]
    fn line_characters_on_the_edit_screen() {
        let mut wp = edit33(0, 0);
        f(&mut wp, Func::Block);
        f(&mut wp, Func::Right);
        f(&mut wp, Func::Down);
        keys(&mut wp, "366"); // Outside, Thick
        go(&mut wp, 2, 2);
        keys(&mut wp, "374"); // All, Dashed
        f(&mut wp, Func::Block);
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let text: Vec<String> = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>().trim().to_string()).collect();
        let row = |k: usize| text.iter().filter(|l| l.starts_with(['┌', '┃', '├', '╟', '║', '╚', '╔'])).nth(k).cloned().unwrap_or_default();
        assert!(row(0).starts_with("┌━━") && row(0).ends_with("═╗"), "{text:#?}");
        assert!(row(1).starts_with("┃a1") && row(1).contains("│b1") && row(1).contains("┃c1") && row(1).ends_with('║'), "{text:#?}");
        assert!(row(2).starts_with("├──") && row(2).ends_with("─╢"), "{text:#?}");
        assert!(row(4).contains("━┼╌╌") , "{text:#?}");
        assert!(row(5).starts_with("║a3") && row(5).contains("╎c3") && row(5).ends_with('╎'), "{text:#?}");
        assert!(row(6).starts_with("╚═") && row(6).contains("╧╌╌"), "{text:#?}");
        // the footer says that Block is on, and which cell the cursor is in
        assert!(text[23].starts_with("Block on") && text[23].ends_with("Cell C3"), "{}", text[23]);
        assert!(text[24].starts_with("1 Size; 2 Format; 3 Lines; 4 Header; 5 Math; 6 Options; 7 Join; 8 Split:"), "{}", text[24]);
    }

    #[test]
    fn lines_in_print() {
        let rules = |k: &str| {
            let mut wp = edit33(1, 1);
            keys(&mut wp, k);
            let e = wp.ed_ref();
            let x = e.layout.tables[0].x.clone();
            let pages = crate::page::build_pages(&e.doc, &e.layout, &MonoMetrics);
            // what lies on B2's left line, beside its text
            let y = e.layout.lines[e.layout.line_of(e.cursor)].y + 100;
            let on: Vec<_> = pages[0].rules.iter().filter(|r| r.0 > x[1] - 60 && r.0 + r.2 < x[1] + 60 && r.1 <= y && r.1 + r.3 > y).cloned().collect();
            let all: Vec<_> = pages[0].rules.iter().filter(|r| r.0 > x[1] - 60 && r.0 + r.2 < x[1] + 60).cloned().collect();
            (on, all.len())
        };
        let (single, n_single) = rules("");
        assert_eq!(single.iter().map(|r| r.2).collect::<Vec<_>>(), vec![6]);
        assert_eq!(n_single, 3);
        assert!(rules("311").0.is_empty() && rules("311").1 == 2);
        // double: two thin rules side by side
        let (double, _) = rules("313");
        assert_eq!(double.len(), 2);
        assert!(double.iter().all(|r| r.2 == 6) && (double[0].0 - double[1].0).abs() == 12);
        // dashed and dotted: pieces
        assert!(rules("314").1 >= 4 && rules("315").1 > rules("314").1);
        assert_eq!(rules("316").0.iter().map(|r| r.2).collect::<Vec<_>>(), vec![30]);
        assert_eq!(rules("317").0.iter().map(|r| r.2).collect::<Vec<_>>(), vec![54]);
    }

    /// A table of two columns and `rows` one-line rows, "Head" in A1, the
    /// cursor in A1.
    fn long_table(rows: usize) -> Wp {
        let mut wp = Wp::new(80, 25);
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "212");
        f(&mut wp, Func::Enter);
        keys(&mut wp, &rows.to_string());
        f(&mut wp, Func::Enter);
        f(&mut wp, Func::Exit);
        keys(&mut wp, "Head");
        wp
    }

    #[test]
    fn header_rows() {
        let mut wp = edit33(1, 1);
        let before = codes(&wp);
        keys(&mut wp, "42");
        f(&mut wp, Func::Enter);
        assert!(codes(&wp).contains("\"][Tbl Opt:Hdr 2][Row][Cell]a1"), "{}", codes(&wp));
        assert_eq!(geom(&wp).header_rows, 2);
        let t = table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).unwrap();
        assert_eq!(t.locate(wp.ed_ref().cursor), Some((1, 1)));
        // the prompt shows the number; 0 takes the code away; undo
        keys(&mut wp, "4");
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let last: String = (0..80).map(|c| g.get(c, 24).unwrap().ch).collect();
        assert!(last.starts_with("Number of header rows: 2"), "{last}");
        keys(&mut wp, "0");
        f(&mut wp, Func::Enter);
        assert_eq!(codes(&wp), before);
        wp.ed().undo();
        assert!(codes(&wp).contains("[Tbl Opt:Hdr 2]"));
        wp.ed().undo();
        assert_eq!(codes(&wp), before);
        // not more than there are rows
        keys(&mut wp, "h9");
        f(&mut wp, Func::Enter);
        assert!(codes(&wp).contains("[Tbl Opt:Hdr 3]"), "{}", codes(&wp));
    }

    #[test]
    fn header_rows_take_room_on_the_following_pages() {
        let mut wp = long_table(120);
        let plain = wp.ed_ref().layout.clone();
        assert_eq!(plain.pages, 3);
        assert!(plain.tbl_repeats.is_empty());
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "41");
        f(&mut wp, Func::Enter);
        f(&mut wp, Func::Exit);
        let e = wp.ed_ref();
        let lay = &e.layout;
        // one header row: top line, text, the line below it
        let h = 60 + plain.lines[0].height + 60;
        let reps: Vec<_> = lay.tbl_repeats.iter().map(|r| (r.table, r.page, r.y)).collect();
        assert_eq!(reps, vec![(0, 2, e.doc.init.top), (0, 3, e.doc.init.top)]);
        // the first line of page 2 stands lower by that; the edit screen
        // has the same rows as before
        let first2 = |l: &crate::layout::Layout| l.lines.iter().find(|x| x.page == 2).map(|x| x.y).unwrap();
        assert_eq!(first2(lay), first2(&plain) + h);
        assert_eq!(lay.drows.len(), plain.drows.len());
        // fewer rows fit on page 2: the last row stands lower on page 3
        let (a, b) = (plain.lines.last().unwrap(), lay.lines.last().unwrap());
        assert!(b.page == 3 && a.page == 3 && b.y > a.y + h, "{} {}", a.y, b.y);
        // printed: the header text on every page, with its lines
        let pages = crate::page::build_pages(&e.doc, lay, &MonoMetrics);
        assert_eq!(pages.len(), 3);
        let head = |p: &crate::page::Page| p.runs.iter().filter(|r| r.text.iter().collect::<String>() == "Head").map(|r| r.y).collect::<Vec<_>>();
        let y1 = head(&pages[0]);
        assert_eq!(y1.len(), 1);
        assert_eq!(head(&pages[1]), y1);
        assert_eq!(head(&pages[2]), y1);
        // (the table's double top line at the same place on each page)
        let top = |p: &crate::page::Page| p.rules.iter().filter(|r| r.2 > 1000).map(|r| r.1).min().unwrap();
        assert_eq!(top(&pages[1]), top(&pages[0]));
        let plain_pages = crate::page::build_pages(&e.doc, &plain, &MonoMetrics);
        assert!(pages[1].rules.len() > plain_pages[1].rules.len());
        // without header rows page 2 has no "Head"
        assert!(head(&plain_pages[1]).is_empty());
    }

    /// Keys; a line feed is Enter.
    fn typed(wp: &mut Wp, s: &str) {
        for c in s.chars() {
            match c {
                '\n' => f(wp, Func::Enter),
                c => wp.input(Input::Char(c)),
            }
        }
    }

    #[test]
    fn options_spacing() {
        let mut wp = edit33(1, 1);
        let before = codes(&wp);
        let plain = wp.ed_ref().layout.clone();
        // the screen
        keys(&mut wp, "6");
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let text: Vec<String> = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>().trim_end().to_string()).collect();
        assert_eq!(text[0], "Table Options");
        assert!(text[2].contains("1 - Spacing Between Text and Lines"), "{text:#?}");
        assert!(text[3].contains("Left") && text[3].ends_with("0.1\""), "{text:#?}");
        assert!(text[6].contains("Bottom") && text[6].ends_with("0\""), "{text:#?}");
        assert!(text[8].contains("2 - Display Negative Results") && text[8].ends_with('1'), "{text:#?}");
        assert!(text[12].contains("3 - Position of Table") && text[12].ends_with("Left"), "{text:#?}");
        assert!(text[14].contains("4 - Gray Shading (% of black)") && text[14].ends_with("10%"), "{text:#?}");
        // left 0.3", right 0", top 0.1", bottom 0.2"
        keys(&mut wp, "1.3");
        f(&mut wp, Func::Enter);
        keys(&mut wp, "0");
        f(&mut wp, Func::Enter);
        keys(&mut wp, ".1");
        f(&mut wp, Func::Enter);
        keys(&mut wp, ".2");
        f(&mut wp, Func::Enter);
        // Exit leaves the screen for Table Edit, not for the document
        f(&mut wp, Func::Exit);
        assert_eq!(wp.screens.len(), 1);
        assert!(codes(&wp).contains("[Tbl Opt:Spacing 0.3\",0\",0.1\",0.2\"][Row]"), "{}", codes(&wp));
        let lay = &wp.ed_ref().layout;
        let x = lay.tables[0].x.clone();
        assert_eq!(x, plain.tables[0].x);
        // text starts 0.3" right of the line, and may reach the line on the right
        let li = lay.line_of(wp.ed_ref().cursor);
        assert_eq!(lay.lines[li].caret[0].1, x[1] + 360);
        assert_eq!(lay.lines[li].right, x[2]);
        assert_eq!(plain.lines[li].caret[0].1, x[1] + 120);
        assert_eq!(plain.lines[li].right, x[2] - 120);
        // rows stand 0.1" + 0.2" further apart; the first one 0.1" lower
        assert_eq!(lay.lines[0].y, plain.lines[0].y + 120);
        assert_eq!(lay.lines[li].y, plain.lines[li].y + 120 + 360);
        // the defaults again: no code; undo
        typed(&mut wp, "61.1\n.1\n0\n0\n");
        f(&mut wp, Func::Exit);
        assert_eq!(codes(&wp), before);
        wp.ed().undo();
        assert!(codes(&wp).contains("[Tbl Opt:Spacing"));
        wp.ed().undo();
        assert_eq!(codes(&wp), before);
    }

    #[test]
    fn options_position() {
        let mut wp = edit33(1, 1);
        // a table of 3 × 1"
        wp.ed().cursor = 0;
        wp.ed().doc.els[0] = El::Code(Code::TblDef { cols: vec![1200; 3] });
        wp.ed().relayout();
        go_from_start(&mut wp);
        let before = codes(&wp);
        let init = wp.ed_ref().doc.init.clone();
        let (l, r) = (init.left, init.page_width - init.right);
        assert_eq!(geom(&wp).x, vec![l, l + 1200, l + 2400, l + 3600]);
        keys(&mut wp, "632");
        assert!(codes(&wp).contains("[Tbl Opt:Pos Right][Row]"), "{}", codes(&wp));
        assert_eq!(geom(&wp).x, vec![r - 3600, r - 2400, r - 1200, r]);
        keys(&mut wp, "3c");
        let c = l + (r - l - 3600) / 2;
        assert_eq!(geom(&wp).x, vec![c, c + 1200, c + 2400, c + 3600]);
        // the text moves with its cell
        let lay = &wp.ed_ref().layout;
        assert_eq!(lay.lines[lay.line_of(wp.ed_ref().cursor)].caret[0].1, c + 1200 + 120);
        // Full: the columns share the room between the margins
        keys(&mut wp, "34");
        let w = (r - l) / 3;
        assert_eq!(geom(&wp).x, vec![l, l + w, l + 2 * w, r]);
        assert!(codes(&wp).contains("[Tbl Def:I;3,1\",1\",1\"][Tbl Opt:Pos Full]"), "{}", codes(&wp));
        // Set Position: from the left edge of the paper
        keys(&mut wp, "35");
        keys(&mut wp, "2.5");
        f(&mut wp, Func::Enter);
        assert_eq!(geom(&wp).x[0], 3000);
        assert!(codes(&wp).contains("[Tbl Opt:Pos 2.5\"]"), "{}", codes(&wp));
        // Left: the default, no code; every change was one undo step
        keys(&mut wp, "31");
        assert_eq!(codes(&wp), before);
        for want in ["Pos 2.5\"", "Pos Full", "Pos Center", "Pos Right"] {
            wp.ed().undo();
            assert!(codes(&wp).contains(want), "{want}: {}", codes(&wp));
        }
        wp.ed().undo();
        assert_eq!(codes(&wp), before);
    }

    fn go_from_start(wp: &mut Wp) {
        let e = wp.ed();
        let t = parse(&e.doc.els, 0).unwrap();
        goto_cell(e, &t, 1, 1);
    }

    #[test]
    fn options_negative_results_and_gray() {
        // A1 = 2, B1 = 5, C1 = A1-B1
        let mut wp = Wp::new(80, 25);
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "213");
        f(&mut wp, Func::Enter);
        keys(&mut wp, "1");
        f(&mut wp, Func::Enter);
        f(&mut wp, Func::Exit);
        keys(&mut wp, "2");
        f(&mut wp, Func::Tab);
        keys(&mut wp, "5");
        f(&mut wp, Func::Tab);
        f(&mut wp, Func::ColumnsTable);
        keys(&mut wp, "52A1-B1");
        f(&mut wp, Func::Enter);
        assert!(codes(&wp).ends_with("-3.00[Tbl Off]"), "{}", codes(&wp));
        let before = codes(&wp);
        keys(&mut wp, "622");
        assert!(codes(&wp).contains("[Tbl Opt:Neg ()]") && codes(&wp).ends_with("(3.00)[Tbl Off]"), "{}", codes(&wp));
        // … and a result in parentheses still counts as negative
        keys(&mut wp, "0");
        keys(&mut wp, "51");
        assert!(codes(&wp).ends_with("(3.00)[Tbl Off]"), "{}", codes(&wp));
        keys(&mut wp, "621");
        assert_eq!(codes(&wp), before);
        wp.ed().undo();
        assert!(codes(&wp).ends_with("(3.00)[Tbl Off]"), "{}", codes(&wp));
        // gray shading: the darkness of shaded cells
        typed(&mut wp, "440\n");
        f(&mut wp, Func::Exit);
        assert!(codes(&wp).contains("[Tbl Opt:Neg ();Gray 40%]"), "{}", codes(&wp));
        keys(&mut wp, "381");
        let e = wp.ed_ref();
        assert_eq!(e.layout.tables[0].gray, 40);
        let pages = crate::page::build_pages(&e.doc, &e.layout, &MonoMetrics);
        assert_eq!(pages[0].images.iter().map(|p| p.color >> 24).collect::<Vec<_>>(), vec![102]);
        wp.ed().undo();
        wp.ed().undo();
        assert!(codes(&wp).contains("[Tbl Opt:Neg ()][Row]"), "{}", codes(&wp));
    }

    #[test]
    fn table_codes_stay_in_place() {
        // header rows, B2 with lines and shading
        let setup = || {
            let mut wp = edit33(1, 1);
            keys(&mut wp, "41");
            f(&mut wp, Func::Enter);
            keys(&mut wp, "375");
            keys(&mut wp, "381");
            wp
        };
        let b2 = "[Cell][Cell Lines:Left Dotted;Top Dotted;Shade On]b2[Cell][Cell Lines:Left Dotted]c2";
        let wp = setup();
        assert!(codes(&wp).contains(b2) && codes(&wp).contains("[Tbl Opt:Hdr 1][Row][Cell]a1"), "{}", codes(&wp));
        // insert a row and a column in front: the codes move with their cells
        let mut wp = setup();
        f(&mut wp, Func::Typeover);
        keys(&mut wp, "r1");
        f(&mut wp, Func::Enter);
        go(&mut wp, 2, 1);
        f(&mut wp, Func::Typeover);
        keys(&mut wp, "c1");
        f(&mut wp, Func::Enter);
        let c = codes(&wp);
        assert!(c.contains("[Tbl Opt:Hdr 1][Row][Cell]a1[Cell][Cell]b1"), "{c}");
        assert!(c.contains("[Row][Cell]a2[Cell]") && c.contains(b2), "{c}");
        assert!(geom(&wp).shaded(2, 2) && !geom(&wp).shaded(1, 1));
        // delete the column and the row of B2: its codes go with it
        let mut wp = setup();
        f(&mut wp, Func::Del);
        keys(&mut wp, "c1");
        f(&mut wp, Func::Enter);
        let c = codes(&wp);
        assert!(c.contains("[Tbl Opt:Hdr 1][Row]") && c.contains("[Cell]a2[Cell][Cell Lines:Left Dotted]c2") && !c.contains("Shade"), "{c}");
        let mut wp = setup();
        f(&mut wp, Func::Del);
        keys(&mut wp, "r1");
        f(&mut wp, Func::Enter);
        let c = codes(&wp);
        assert!(c.contains("[Tbl Opt:Hdr 1][Row]") && c.contains("[Cell Lines:Top Dotted]b3") && !c.contains("Shade"), "{c}");
        // Join keeps the settings of the table and of the first cell
        let mut wp = setup();
        f(&mut wp, Func::Block);
        f(&mut wp, Func::Right);
        keys(&mut wp, "7y");
        let c = codes(&wp);
        assert!(c.contains("[Tbl Opt:Hdr 1][Row]"), "{c}");
        assert!(c.contains("[Cell][Join:2x1][Cell Lines:Left Dotted;Top Dotted;Shade On]b2[HRt]c2[Cell][Row]"), "{c}");
        assert!(geom(&wp).shaded(1, 1) && geom(&wp).shaded(1, 2));
        // … lines of the joined cell go behind [Join], and Split keeps them
        keys(&mut wp, "313");
        keys(&mut wp, "8");
        assert!(codes(&wp).contains("[Cell][Cell Lines:Left Double;Top Dotted;Shade On]b2"), "{}", codes(&wp));
        // deleting a part of the table leaves them, a copy doesn't take them
        let mut wp = setup();
        f(&mut wp, Func::Exit);
        let t = table_at(&wp.ed_ref().doc.els, wp.ed_ref().cursor).unwrap();
        let (from, to) = (t.rows[0][2] + 1, t.rows[2][0] + 2);
        let got: String = copy_range(&wp.ed_ref().doc.els, from, to).iter().map(|e| match e {
            El::Ch(c) => c.to_string(),
            El::Code(c) => crate::reveal::code_text(c),
        }).collect();
        assert_eq!(got, "c1[HRt]a2[Tab]b2[Tab]c2[HRt]a");
        wp.ed().delete_range(from, to, true);
        let c = codes(&wp);
        assert!(c.contains("[Cell][Cell Lines:Left Dotted;Top Dotted;Shade On][Cell][Cell Lines:Left Dotted][Row][Cell]3"), "{c}");
        // (from before the table into it: its settings stay, too)
        let mut wp = setup();
        f(&mut wp, Func::Exit);
        wp.ed().delete_range(0, 5, true);
        assert!(codes(&wp).contains("\"][Tbl Opt:Hdr 1][Row][Cell]1[Cell]b1"), "{}", codes(&wp));
        // plain text and the cursor don't notice the codes
        let wp = setup();
        let d = &wp.ed_ref().doc;
        assert_eq!(d.plain_text(0, d.len()), "a1\tb1\tc1\na2\tb2\tc2\na3\tb3\tc3\n");
        let mut wp = setup();
        f(&mut wp, Func::Exit);
        wp.ed().cursor = 0;
        f(&mut wp, Func::Right);
        keys(&mut wp, "X");
        assert!(codes(&wp).contains("[Tbl Opt:Hdr 1][Row][Cell]Xa1"), "{}", codes(&wp));
        // without its [Tbl Def] the table is text; what belongs to a table goes
        let mut wp = setup();
        f(&mut wp, Func::Exit);
        remove_structure(wp.ed(), 0);
        assert_eq!(codes(&wp), "a1[Tab]b1[Tab]c1[HRt]a2[Tab]b2[Tab]c2[HRt]a3[Tab]b3[Tab]c3[HRt]");
    }

    #[test]
    fn stream_round_trip() {
        let mut wp = edit33(1, 1);
        keys(&mut wp, "42");
        f(&mut wp, Func::Enter);
        typed(&mut wp, "61.2\n.05\n.1\n0\n223c435\n");
        f(&mut wp, Func::Exit);
        keys(&mut wp, "374");
        keys(&mut wp, "381");
        go(&mut wp, 2, 2);
        keys(&mut wp, "361");
        f(&mut wp, Func::Exit);
        let doc = &wp.ed_ref().doc;
        let c = codes(&wp);
        assert!(c.contains("[Tbl Opt:Hdr 2;Spacing 0.2\",0.05\",0.1\",0\";Neg ();Pos Center;Gray 35%]"), "{c}");
        assert!(c.contains("[Cell][Cell Lines:Left None;Right None;Top None;Bottom None]c3"), "{c}");
        let back = crate::stream::read(&crate::stream::write(doc)).unwrap();
        assert_eq!(back.els, doc.els);
        // Set Position, too
        let mut d = Document::new();
        d.els = new_table(2, 1, 4800);
        d.els.insert(1, El::Code(Code::TblOpt(TblOpt { pos: TblPos::Set(2040), ..Default::default() })));
        let back = crate::stream::read(&crate::stream::write(&d)).unwrap();
        assert_eq!(back.els, d.els);
    }

    #[test]
    fn autocorrect_does_not_read_across_cells() {
        let mut wp = Wp::new(80, 25);
        wp.input(Input::Func(Func::ColumnsTable));
        keys(&mut wp, "213");
        wp.input(Input::Func(Func::Enter));
        keys(&mut wp, "1");
        wp.input(Input::Func(Func::Enter));
        wp.input(Input::Func(Func::Exit));
        keys(&mut wp, "Header A");
        wp.input(Input::Func(Func::Tab));
        keys(&mut wp, "Header B");
        wp.input(Input::Func(Func::Tab));
        keys(&mut wp, "Header C ");
        let c = codes(&wp);
        assert!(c.contains("[Cell]Header A[Cell]Header B[Cell]Header C"), "{c}");
    }
}
