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
    let flat: Vec<(usize, usize)> = t.rows.iter().enumerate().flat_map(|(r, row)| (0..row.len()).map(move |c| (r, c))).collect();
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
pub struct TableEdit {}

impl TableEdit {
    fn with_table<R>(wp: &mut Wp, f: impl FnOnce(&mut Editor, Table, usize, usize) -> R) -> Option<R> {
        let e = wp.ed();
        let t = table_at(&e.doc.els, e.cursor)?;
        let (r, c) = t.locate(e.cursor)?;
        Some(f(e, t, r, c))
    }
}

fn set_cols(e: &mut Editor, t: &Table, cols: Vec<i32>) {
    e.doc.els[t.def] = El::Code(Code::TblDef { cols });
    e.modified = true;
    e.relayout();
}

fn insert_rows(wp: &mut Wp, n: usize) {
    TableEdit::with_table(wp, |e, t, r, _| {
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

fn delete_rows(wp: &mut Wp, n: usize) {
    TableEdit::with_table(wp, |e, t, r, _| {
        if t.rows.len() <= n {
            return;
        }
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
    });
}

fn delete_cols(wp: &mut Wp, n: usize) {
    TableEdit::with_table(wp, |e, t, r, c| {
        if t.cols.len() <= n || c + n > t.cols.len() {
            return;
        }
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
    });
}

impl Screen for TableEdit {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::Menu(298)
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let moved = |wp: &mut Wp, dr: isize, dc: isize| {
            TableEdit::with_table(wp, |e, t, r, c| {
                let nr = (r as isize + dr).clamp(0, t.rows.len() as isize - 1) as usize;
                let nc = (c as isize + dc).clamp(0, t.cols.len() as isize - 1) as usize;
                goto_cell(e, &t, nr, nc);
            });
            wp.after_edit();
        };
        match i {
            Input::Func(Func::Exit) | Input::Func(Func::ColumnsTable) | Input::Func(Func::Cancel) => return Outcome::Close,
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
                            TableEdit::with_table(wp, |e, t, r, _| goto_cell(e, &t, r, t.cols.len() - 1));
                            insert_cols(wp, n - cols);
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
                            let targets: Vec<(usize, usize)> = if scope == 0 {
                                vec![(r, c)]
                            } else {
                                (0..t.rows.len()).filter(|rr| c < t.rows[*rr].len()).map(|rr| (rr, c)).collect()
                            };
                            // insert [Just:x] at the start of each cell (back to front)
                            let mut starts: Vec<usize> = targets.iter().map(|&(rr, cc)| t.rows[rr][cc] + 1).collect();
                            starts.sort_unstable_by(|a, b| b.cmp(a));
                            for s in starts {
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
}
