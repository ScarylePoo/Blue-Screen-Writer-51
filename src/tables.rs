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

fn is_table_code(e: &El) -> bool {
    matches!(e, El::Code(Code::Row | Code::Cell | Code::TblOff | Code::TblDef { .. }))
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
        let first_in_row = k > 0 && matches!(els[k - 1], El::Code(Code::Row | Code::TblDef { .. }));
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

/// Table Edit, 7 Join: the cells from (r0, c0) to (r1, c1) become one.
/// Their text is collected in the first cell.
fn join_cells(wp: &mut Wp, a: (usize, usize), b: (usize, usize)) {
    TableEdit::with_table(wp, |e, t, _, _| {
        let slots = t.slots(&e.doc.els);
        let (mut r0, mut r1) = (a.0.min(b.0), a.0.max(b.0).min(t.rows.len() - 1));
        let (mut c0, mut c1) = (a.1.min(b.1), a.1.max(b.1).min(t.cols.len() - 1));
        // joined cells that reach into the area belong to it as a whole
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
                break;
            }
        }
        if (r0, c0) == (r1, c1) {
            return;
        }
        e.snapshot();
        let mut cells = contents(&e.doc.els, &t);
        let mut joined: Vec<El> = Vec::new();
        for r in r0..=r1 {
            for c in c0..=c1.min(cells[r].len().saturating_sub(1)) {
                let mut cell = std::mem::take(&mut cells[r][c]);
                cell.retain(|el| !matches!(el, El::Code(Code::Join { .. })));
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
            g.text(0, rows - 2, "Ctrl-Arrows Column Widths; Ins Insert; Del Delete; Move Copy;", st);
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
}
