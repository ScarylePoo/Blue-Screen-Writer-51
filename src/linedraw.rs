//! Ctrl-F3, 2 — Line Draw: draw boxes and lines with the arrow keys.
//! Menu: `1 │; 2 ║; 3 *; 4 Change; 5 Erase; 6 Move: 1`

use crate::doc::*;
use crate::editor::Editor;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::LineEnd;
use crate::ui::{Outcome, Screen, Wp};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Single,
    Double,
    Char(char),
    Erase,
    Move,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

pub struct LineDraw {
    mode: Mode,
    last: Option<Dir>,
    changing: bool,
}

impl LineDraw {
    pub fn new() -> LineDraw {
        LineDraw { mode: Mode::Single, last: None, changing: false }
    }
}

/// Connection bits: up=1, down=2, left=4, right=8.
fn box_char(bits: u8, double: bool) -> char {
    let s = ['·', '│', '│', '│', '─', '┘', '┐', '┤', '─', '└', '┌', '├', '─', '┴', '┬', '┼'];
    let d = ['·', '║', '║', '║', '═', '╝', '╗', '╣', '═', '╚', '╔', '╠', '═', '╩', '╦', '╬'];
    if double { d[bits as usize] } else { s[bits as usize] }
}

fn bits_of(c: char) -> u8 {
    for (i, (a, b)) in ['·', '│', '│', '│', '─', '┘', '┐', '┤', '─', '└', '┌', '├', '─', '┴', '┬', '┼']
        .iter()
        .zip(['·', '║', '║', '║', '═', '╝', '╗', '╣', '═', '╚', '╔', '╠', '═', '╩', '╦', '╬'].iter())
        .enumerate()
    {
        if c == *a || c == *b {
            return i as u8;
        }
    }
    0
}

/// Make sure line `li` reaches screen column `col`; return the element
/// index for that column (padding with spaces, adding lines as needed).
fn index_at(e: &mut Editor, li: usize, col: i32) -> usize {
    e.ensure_layout();
    // add lines at the end of the document if needed
    while li >= e.layout.lines.len() {
        let n = e.doc.els.len();
        e.doc.els.insert(n, El::Code(Code::HRt));
        e.relayout();
    }
    let line = e.layout.lines[li].clone();
    for c in &line.cells {
        if c.col == col {
            return c.el;
        }
    }
    // pad with spaces at the end of the line (before [HRt])
    let end_col = line.caret.last().map(|x| x.0).unwrap_or(0);
    let insert_at = match line.term {
        LineEnd::Hard | LineEnd::HardPage => line.end - 1,
        _ => line.end,
    };
    let pad = (col - end_col).max(0) as usize;
    for k in 0..=pad {
        e.doc.els.insert(insert_at + k, El::Ch(' '));
    }
    e.relayout();
    insert_at + pad
}

fn put(e: &mut Editor, li: usize, col: i32, ch: char) -> usize {
    let i = index_at(e, li, col);
    if let Some(El::Ch(_)) = e.doc.els.get(i) {
        e.doc.els[i] = El::Ch(ch);
    } else {
        e.doc.els.insert(i, El::Ch(ch));
    }
    e.modified = true;
    e.relayout();
    i
}

fn char_at(e: &Editor, li: usize, col: i32) -> char {
    e.layout.lines.get(li).and_then(|l| l.cells.iter().find(|c| c.col == col)).map(|c| c.ch).unwrap_or(' ')
}

impl Screen for LineDraw {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if self.changing {
            self.changing = false;
            if let Input::Char(c) = i {
                self.mode = Mode::Char(*c);
            }
            return Outcome::Keep;
        }
        let dir = match i {
            Input::Char('1') => {
                self.mode = Mode::Single;
                return Outcome::Keep;
            }
            Input::Char('2') => {
                self.mode = Mode::Double;
                return Outcome::Keep;
            }
            Input::Char('3') => {
                self.mode = Mode::Char('*');
                return Outcome::Keep;
            }
            Input::Char('4') | Input::Char('c') | Input::Char('C') => {
                self.changing = true;
                return Outcome::Keep;
            }
            Input::Char('5') | Input::Char('e') | Input::Char('E') => {
                self.mode = Mode::Erase;
                return Outcome::Keep;
            }
            Input::Char('6') | Input::Char('m') | Input::Char('M') => {
                self.mode = Mode::Move;
                return Outcome::Keep;
            }
            Input::Func(Func::Up) => Dir::Up,
            Input::Func(Func::Down) => Dir::Down,
            Input::Func(Func::Left) => Dir::Left,
            Input::Func(Func::Right) => Dir::Right,
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Enter) => return Outcome::Close,
            _ => return Outcome::Keep,
        };
        let e = wp.ed();
        e.ensure_layout();
        let li = e.layout.line_of(e.cursor);
        let col = e.layout.lines[li].caret_of(e.cursor).0;
        let (nli, ncol) = match dir {
            Dir::Up => (li.saturating_sub(1), col),
            Dir::Down => (li + 1, col),
            Dir::Left => (li, (col - 1).max(0)),
            Dir::Right => (li, col + 1),
        };
        let out_bit = match dir {
            Dir::Up => 1,
            Dir::Down => 2,
            Dir::Left => 4,
            Dir::Right => 8,
        };
        let in_bit = match dir {
            Dir::Up => 2,
            Dir::Down => 1,
            Dir::Left => 8,
            Dir::Right => 4,
        };
        match self.mode {
            Mode::Single | Mode::Double => {
                let dbl = self.mode == Mode::Double;
                let here = bits_of(char_at(e, li, col)) | out_bit;
                put(e, li, col, box_char(here, dbl));
                let there = bits_of(char_at(e, nli, ncol)) | in_bit;
                let idx = put(e, nli, ncol, box_char(there, dbl));
                e.cursor = idx;
            }
            Mode::Char(c) => {
                put(e, li, col, c);
                let idx = put(e, nli, ncol, c);
                e.cursor = idx;
            }
            Mode::Erase => {
                put(e, li, col, ' ');
                let idx = index_at(e, nli, ncol);
                e.cursor = idx;
            }
            Mode::Move => {
                let idx = index_at(e, nli, ncol);
                e.cursor = idx;
            }
        }
        self.last = Some(dir);
        wp.after_edit();
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        if self.changing {
            g.text(0, row, "Solid character: ", st);
            return;
        }
        let star = match self.mode {
            Mode::Char(c) => c,
            _ => '*',
        };
        let c = g.text(0, row, &format!("1 │; 2 ║; 3 {star}; 4 Change; 5 Erase; 6 Move: "), st);
        let cur = match self.mode {
            Mode::Single => "1",
            Mode::Double => "2",
            Mode::Char(_) => "3",
            Mode::Erase => "5",
            Mode::Move => "6",
        };
        g.text(c, row, cur, st);
    }
}
