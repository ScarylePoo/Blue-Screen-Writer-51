//! Format: Line → Tab Set — the WP 5.1 tab ruler editor.

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::COL_WPU;
use crate::ui::{Outcome, Screen, Wp};

pub struct TabRuler {
    tabs: TabSet,
    left_margin: i32,
    /// cursor position in WPU, relative to the left margin when relative
    pos: i32,
    typed: String,
    changed: bool,
}

impl TabRuler {
    pub fn new(wp: &Wp) -> TabRuler {
        let e = wp.ed_ref();
        let f = e.format_state_at(e.cursor);
        TabRuler { tabs: f.tabs, left_margin: f.left, pos: 0, typed: String::new(), changed: false }
    }

    fn base(&self) -> i32 {
        if self.tabs.relative { self.left_margin } else { 0 }
    }

    fn set_at(&mut self, pos: i32, kind: TabKind, dot: bool) {
        self.tabs.stops.retain(|t| t.pos != pos);
        self.tabs.stops.push(TabStop { pos, kind, dot_leader: dot });
        self.tabs.stops.sort_by_key(|t| t.pos);
        self.changed = true;
    }
}

impl Screen for TabRuler {
    fn full_screen(&self) -> bool {
        false
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Left) => self.pos -= COL_WPU,
            Input::Func(Func::Right) => self.pos += COL_WPU,
            Input::Func(Func::Up) => {
                if let Some(t) = self.tabs.stops.iter().rev().find(|t| t.pos < self.pos) {
                    self.pos = t.pos;
                }
            }
            Input::Func(Func::Down) => {
                if let Some(t) = self.tabs.stops.iter().find(|t| t.pos > self.pos) {
                    self.pos = t.pos;
                }
            }
            Input::Func(Func::Home) => {}
            Input::Func(Func::Del) | Input::Func(Func::Backspace) => {
                let p = self.pos;
                self.tabs.stops.retain(|t| t.pos != p);
                self.changed = true;
            }
            Input::Func(Func::DelEol) => {
                let p = self.pos;
                self.tabs.stops.retain(|t| t.pos < p);
                self.changed = true;
            }
            Input::Char(c) if c.is_ascii_digit() || *c == '.' || *c == ',' || *c == '-' || *c == '"' => self.typed.push(*c),
            Input::Char(c) => {
                let pos = self.pos;
                match c.to_ascii_lowercase() {
                    'l' => self.set_at(pos, TabKind::Left, false),
                    'c' => self.set_at(pos, TabKind::Center, false),
                    'r' => self.set_at(pos, TabKind::Right, false),
                    'd' => self.set_at(pos, TabKind::Decimal, false),
                    't' => {
                        // Type: toggle Absolute / Relative to margin
                        let base_old = self.base();
                        self.tabs.relative = !self.tabs.relative;
                        let base_new = self.base();
                        for t in &mut self.tabs.stops {
                            t.pos += base_old - base_new;
                        }
                        self.changed = true;
                    }
                    '.' => {
                        if let Some(t) = self.tabs.stops.iter_mut().find(|t| t.pos == pos) {
                            t.dot_leader = !t.dot_leader;
                            self.changed = true;
                        }
                    }
                    _ => {}
                }
            }
            Input::Func(Func::Enter) => {
                // "n" sets a tab; "start,step" sets evenly spaced tabs
                let s = std::mem::take(&mut self.typed);
                if let Some((a, b)) = s.split_once(',') {
                    if let (Some(start), Some(step)) = (parse_measure(a), parse_measure(b)) {
                        if step > 0 {
                            self.tabs.stops.retain(|t| t.pos < start);
                            let mut p = start;
                            while p < 8 * 1200 * 2 {
                                self.tabs.stops.push(TabStop { pos: p, kind: TabKind::Left, dot_leader: false });
                                p += step;
                            }
                            self.changed = true;
                        }
                    }
                } else if let Some(p) = parse_measure(&s) {
                    self.set_at(p, TabKind::Left, false);
                    self.pos = p;
                }
            }
            Input::Func(Func::Exit) => {
                if self.changed {
                    let t = self.tabs.clone();
                    wp.ed().insert_format(Code::TabSet(t));
                    wp.after_edit();
                }
                return Outcome::Close;
            }
            Input::Func(Func::Cancel) => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let rows = g.rows;
        // clear the lower part, show ruler and help
        for r in rows - 6..rows {
            g.fill_row(r, 0, g.cols, ' ', st);
        }
        let ruler_row = rows - 5;
        let base = self.base();
        // measures every inch
        for c in 0..g.cols {
            let wpu = c as i32 * COL_WPU - base;
            if wpu % 1200 == 0 {
                g.text(c, ruler_row - 1, &format!("|{}\"", wpu / 1200), st);
            }
        }
        g.fill_row(ruler_row, 0, g.cols, '▀', st);
        let lcol = (self.left_margin / COL_WPU) as usize;
        g.put(lcol, ruler_row, '[', st);
        for t in &self.tabs.stops {
            let c = (t.pos + base) / COL_WPU;
            if c >= 0 && (c as usize) < g.cols {
                let ch = match t.kind {
                    TabKind::Left => 'L',
                    TabKind::Center => 'C',
                    TabKind::Right => 'R',
                    TabKind::Decimal => 'D',
                };
                g.put(c as usize, ruler_row, ch, if t.dot_leader { st.reverse() } else { wp.theme.bold });
            }
        }
        let cc = ((self.pos + base) / COL_WPU).clamp(0, g.cols as i32 - 1) as usize;
        g.cursor = Some((cc, ruler_row));
        g.text(0, rows - 3, "Delete EOL (clear tabs); Enter Number (set tab); Del (clear tab);", st);
        g.text(0, rows - 2, "Left; Center; Right; Decimal; .= Dot Leader; Press Exit when done.", st);
        let ty = if self.tabs.relative { "Relative to Margin" } else { "Absolute" };
        g.text(0, rows - 1, &format!("Type: {ty}   {}", self.typed), st);
        let _ = fmt_inches(self.pos);
    }
}
