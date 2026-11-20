//! Home, Shift-F3: the list of documents (Doc 1 … Doc 9).
//!
//! 5.1 for DOS had two documents and Shift-F3 switched between them; that
//! still works (Switch goes to the document used before).  As in 5.1 for
//! Unix there are nine: Home, 1-9 goes to one directly, and this list shows
//! them all.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::ui::{Outcome, Screen, Wp};

pub struct DocList {
    sel: usize,
}

pub fn open(wp: &mut Wp) {
    wp.push(Box::new(DocList { sel: wp.cur }));
}

fn describe(wp: &Wp, k: usize) -> String {
    let e = &wp.docs[k];
    match (&e.path, e.is_empty()) {
        (Some(p), _) => p.display().to_string(),
        (None, true) => "(empty)".into(),
        (None, false) => {
            // a document without a name: its first words
            let text = e.doc.plain_text(0, e.doc.len().min(400));
            let words: Vec<&str> = text.split_whitespace().take(8).collect();
            format!("(no name)  {}", words.join(" "))
        }
    }
}

impl Screen for DocList {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::List
    }
    fn full_screen(&self) -> bool {
        true
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n.min(wp.docs.len() - 1);
        if double {
            wp.switch_to(self.sel);
            return Outcome::Close;
        }
        Outcome::Keep
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = wp.docs.len();
        match i {
            Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n - 1),
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
            Input::Func(Func::Enter) => {
                wp.switch_to(self.sel);
                return Outcome::Close;
            }
            Input::Char(c @ '1'..='9') => {
                wp.switch_to(*c as usize - '1' as usize);
                return Outcome::Close;
            }
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Switch) | Input::Char('0') | Input::Char(' ') => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, "Documents", st);
        g.text(1, 2, "Doc   Name", wp.theme.bold);
        for k in 0..wp.docs.len() {
            let e = &wp.docs[k];
            let mark = if k == wp.cur { '>' } else { ' ' };
            let changed = if e.modified && !e.is_empty() { "  (not saved)" } else { "" };
            let mut line = format!("{mark}{}    {}{changed}", k + 1, describe(wp, k));
            let max = g.cols.saturating_sub(2);
            if line.chars().count() > max {
                line = line.chars().take(max).collect();
            }
            let row = 3 + k;
            g.text(1, row, &line, if k == self.sel { wp.theme.block } else { st });
            wp.hit(row, 0, g.cols, crate::ui::HitAction::Select(k));
        }
        let row = g.rows - 1;
        let c = g.text(0, row, "Type the number of a document, or move the cursor and press Enter: ", st);
        let c = g.text(c, row, &(wp.cur + 1).to_string(), st);
        g.cursor = Some((c - 1, row));
    }
}

#[cfg(test)]
mod tests {
    use crate::keys::{Func, Input};
    use crate::ui::Wp;

    fn typed(wp: &mut Wp, s: &str) {
        for c in s.chars() {
            wp.input(Input::Char(c));
        }
    }

    #[test]
    fn nine_documents() {
        let mut wp = Wp::new(80, 25);
        assert_eq!(wp.docs.len(), 9);
        typed(&mut wp, "one");
        // Shift-F3 still toggles between Doc 1 and Doc 2
        wp.input(Input::Func(Func::Switch));
        assert_eq!(wp.cur, 1);
        typed(&mut wp, "two");
        wp.input(Input::Func(Func::Switch));
        assert_eq!(wp.cur, 0);
        // Home, 5 goes to Doc 5; Switch then goes back to where we were
        wp.input(Input::Func(Func::Home));
        typed(&mut wp, "5");
        assert_eq!(wp.cur, 4);
        typed(&mut wp, "five");
        assert_eq!(wp.docs[4].doc.plain_text(0, 99), "five");
        assert_eq!(wp.docs[0].doc.plain_text(0, 99), "one");
        wp.input(Input::Func(Func::Switch));
        assert_eq!(wp.cur, 0);
        // a digit without Home is just typed
        typed(&mut wp, "7");
        assert_eq!(wp.docs[0].doc.plain_text(0, 99), "one7");
        // the list: Home, Shift-F3, then a number
        wp.input(Input::Func(Func::Home));
        wp.input(Input::Func(Func::Switch));
        assert!(wp.has_screens());
        typed(&mut wp, "2");
        assert!(!wp.has_screens());
        assert_eq!(wp.cur, 1);
    }

    #[test]
    fn exit_moves_on_to_a_document_in_use() {
        let mut wp = Wp::new(80, 25);
        typed(&mut wp, "one");
        wp.input(Input::Func(Func::Home));
        typed(&mut wp, "3");
        typed(&mut wp, "three");
        // F7, don't save, "Exit doc 3?" Yes → back in Doc 1, not out
        wp.input(Input::Func(Func::Exit));
        typed(&mut wp, "ny");
        assert!(!wp.quit);
        assert_eq!(wp.cur, 0);
        assert!(wp.docs[2].is_empty());
        // the last document in use: now the program ends
        wp.input(Input::Func(Func::Exit));
        typed(&mut wp, "ny");
        assert!(wp.quit);
    }
}
