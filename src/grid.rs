//! A text-mode character grid (the "screen"), like the 80×25 VGA text
//! screen WordPerfect drew on.  UI code draws into this; the renderer turns
//! it into pixels.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u32);

/// The 16 classic PC text-mode colours.
pub mod pal {
    use super::Rgb;
    pub const BLACK: Rgb = Rgb(0x000000);
    pub const BLUE: Rgb = Rgb(0x0000AA);
    pub const GREEN: Rgb = Rgb(0x00AA00);
    pub const CYAN: Rgb = Rgb(0x00AAAA);
    pub const RED: Rgb = Rgb(0xAA0000);
    pub const MAGENTA: Rgb = Rgb(0xAA00AA);
    pub const BROWN: Rgb = Rgb(0xAA5500);
    pub const LGRAY: Rgb = Rgb(0xAAAAAA);
    pub const DGRAY: Rgb = Rgb(0x555555);
    pub const LBLUE: Rgb = Rgb(0x5555FF);
    pub const LGREEN: Rgb = Rgb(0x55FF55);
    pub const LCYAN: Rgb = Rgb(0x55FFFF);
    pub const LRED: Rgb = Rgb(0xFF5555);
    pub const LMAGENTA: Rgb = Rgb(0xFF55FF);
    pub const YELLOW: Rgb = Rgb(0xFFFF55);
    pub const WHITE: Rgb = Rgb(0xFFFFFF);
    pub const ALL: [Rgb; 16] = [
        BLACK, BLUE, GREEN, CYAN, RED, MAGENTA, BROWN, LGRAY, DGRAY, LBLUE, LGREEN, LCYAN, LRED, LMAGENTA, YELLOW, WHITE,
    ];
}

/// The box-drawing characters where lines meet.  Per entry: which arms
/// there are (left, right, up, down) and the character for horizontal
/// single / vertical single, horizontal double / vertical single,
/// horizontal single / vertical double, both double.
pub const BOX_SETS: [([bool; 4], [char; 4]); 9] = [
    ([false, true, false, true], ['┌', '╒', '╓', '╔']),
    ([true, false, false, true], ['┐', '╕', '╖', '╗']),
    ([false, true, true, false], ['└', '╘', '╙', '╚']),
    ([true, false, true, false], ['┘', '╛', '╜', '╝']),
    ([false, true, true, true], ['├', '╞', '╟', '╠']),
    ([true, false, true, true], ['┤', '╡', '╢', '╣']),
    ([true, true, false, true], ['┬', '╤', '╥', '╦']),
    ([true, true, true, false], ['┴', '╧', '╨', '╩']),
    ([true, true, true, true], ['┼', '╪', '╫', '╬']),
];

/// Where a line ends at a place where lines meet, so that double lines
/// form clean corners and crossings.  For one of the two strokes of a
/// double line (`double`): `side_a` = the line that leaves the meeting
/// point at a right angle on this stroke's side is double, `side_b` = the
/// one on the other side is.  For any other line: the two lines at right
/// angles.  `opp`: the line goes on straight ahead.  Returns 1 = stop at
/// the near stroke of the double line that crosses, 0 = go to the middle,
/// -1 = go on to the far stroke (around the outside of a corner).
pub fn rule_end(double: bool, opp: bool, side_a: bool, side_b: bool) -> i32 {
    if double {
        match (side_a, opp, side_b) {
            (true, _, _) => 1,
            (false, true, _) => 0,
            (false, false, true) => -1,
            (false, false, false) => 0,
        }
    } else if opp {
        0
    } else {
        match (side_a, side_b) {
            (true, true) => 1,
            (false, false) => 0,
            _ => -1,
        }
    }
}

pub const F_BOLD: u8 = 1;
pub const F_ITALIC: u8 = 2;
pub const F_UNDER: u8 = 4;
pub const F_DUNDER: u8 = 8;
pub const F_STRIKE: u8 = 16;
pub const F_BLINK: u8 = 32;
/// Misspelled (spell check as you type): red wavy underline.
pub const F_SPELL: u8 = 64;
/// The cell belongs to a Japanese paragraph (CJK font choice).
pub const F_JA: u8 = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: u8,
}

impl Style {
    pub const fn new(fg: Rgb, bg: Rgb) -> Style {
        Style { fg, bg, flags: 0 }
    }
    pub fn flags(mut self, f: u8) -> Style {
        self.flags |= f;
        self
    }
    pub fn reverse(self) -> Style {
        Style { fg: self.bg, bg: self.fg, flags: self.flags }
    }
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    /// Text cursor (col,row) if visible.
    pub cursor: Option<(usize, usize)>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize, base: Style) -> Grid {
        Grid {
            cols,
            rows,
            cells: vec![Cell { ch: ' ', fg: base.fg, bg: base.bg, flags: 0 }; cols * rows],
            cursor: None,
        }
    }

    pub fn clear(&mut self, s: Style) {
        for c in &mut self.cells {
            *c = Cell { ch: ' ', fg: s.fg, bg: s.bg, flags: 0 };
        }
        self.cursor = None;
    }

    pub fn put(&mut self, col: usize, row: usize, ch: char, s: Style) {
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col] = Cell { ch, fg: s.fg, bg: s.bg, flags: s.flags };
        }
    }

    pub fn get(&self, col: usize, row: usize) -> Option<&Cell> {
        if col < self.cols && row < self.rows { Some(&self.cells[row * self.cols + col]) } else { None }
    }

    pub fn get_mut(&mut self, col: usize, row: usize) -> Option<&mut Cell> {
        if col < self.cols && row < self.rows { Some(&mut self.cells[row * self.cols + col]) } else { None }
    }

    /// Write a string; returns the column after the last char.
    pub fn text(&mut self, col: usize, row: usize, s: &str, st: Style) -> usize {
        let mut c = col;
        for ch in s.chars() {
            self.put(c, row, ch, st);
            c += 1;
        }
        c
    }

    /// Write text where `~x` marks a mnemonic letter shown in `hi` style
    /// (WP shows the mnemonic letter of menu items bold).
    pub fn text_mn(&mut self, col: usize, row: usize, s: &str, st: Style, hi: Style) -> usize {
        let mut c = col;
        let mut hot = false;
        for ch in s.chars() {
            if ch == '~' && !hot {
                hot = true;
                continue;
            }
            self.put(c, row, ch, if hot { hi } else { st });
            hot = false;
            c += 1;
        }
        c
    }

    pub fn fill_row(&mut self, row: usize, from: usize, to: usize, ch: char, st: Style) {
        for c in from..to.min(self.cols) {
            self.put(c, row, ch, st);
        }
    }

    pub fn style_row(&mut self, row: usize, st: Style) {
        for c in 0..self.cols {
            if let Some(cell) = self.get_mut(c, row) {
                cell.fg = st.fg;
                cell.bg = st.bg;
            }
        }
    }

    /// Draw a single-line box.
    pub fn frame(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, st: Style, double: bool) {
        let (h, v, tl, tr, bl, br) = if double {
            ('═', '║', '╔', '╗', '╚', '╝')
        } else {
            ('─', '│', '┌', '┐', '└', '┘')
        };
        for x in x0 + 1..x1 {
            self.put(x, y0, h, st);
            self.put(x, y1, h, st);
        }
        for y in y0 + 1..y1 {
            self.put(x0, y, v, st);
            self.put(x1, y, v, st);
        }
        self.put(x0, y0, tl, st);
        self.put(x1, y0, tr, st);
        self.put(x0, y1, bl, st);
        self.put(x1, y1, br, st);
    }
}

/// Number of display columns a string takes (one per char in text mode).
pub fn width(s: &str) -> usize {
    s.chars().filter(|&c| c != '~').count()
}

#[cfg(test)]
mod box_tests {
    use super::*;
    #[test]
    fn line_ends() {
        // a stroke of a double line: stops at a double line on its side,
        // goes around a corner on the outside, else to the middle
        assert_eq!(rule_end(true, false, true, false), 1);
        assert_eq!(rule_end(true, false, false, true), -1);
        assert_eq!(rule_end(true, true, false, true), 0);
        assert_eq!(rule_end(true, false, false, false), 0);
        // a single line ends at a double line across its way, and reaches
        // the far stroke at a corner with one
        assert_eq!(rule_end(false, false, true, true), 1);
        assert_eq!(rule_end(false, false, true, false), -1);
        assert_eq!(rule_end(false, true, true, true), 0);
        // every junction character is there once
        let mut all: Vec<char> = BOX_SETS.iter().flat_map(|(_, set)| set.iter().copied()).collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 36);
    }
}
