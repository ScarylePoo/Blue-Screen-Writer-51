//! Ctrl-F5, 6 Revisions — every saved version of the current document.

use crate::grid::*;
use crate::history::{self, Revision};
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

pub struct Revisions {
    list: Vec<Revision>,
    sel: usize,
    top: usize,
}

pub fn open(wp: &mut Wp) {
    let id = wp.ed_ref().doc.id.clone();
    let mut list = history::list(&id);
    list.reverse(); // newest first
    if list.is_empty() {
        wp.flash("No revisions yet — they are kept every time the document is saved.");
        return;
    }
    wp.push(Box::new(Revisions { list, sel: 0, top: 0 }));
}

fn kind_name(k: &str) -> &'static str {
    match k {
        "save" => "Saved",
        "autosave" => "Autosaved",
        "outside" => "Changed outside (e.g. in Word)",
        "recovered" => "Recovered after crash",
        "discarded" => "Left without saving",
        _ => "Saved",
    }
}

impl Screen for Revisions {
    fn full_screen(&self) -> bool {
        true
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n.min(self.list.len().saturating_sub(1));
        if double { self.input(wp, &Input::Char('1')) } else { Outcome::Keep }
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = self.list.len();
        let vis = wp.rows.saturating_sub(5);
        match i {
            Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n - 1),
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
            Input::Func(Func::PgDn) | Input::Func(Func::ScreenDown) => self.sel = (self.sel + vis).min(n - 1),
            Input::Func(Func::PgUp) | Input::Func(Func::ScreenUp) => self.sel = self.sel.saturating_sub(vis),
            Input::Char('1') | Input::Char('l') | Input::Char('L') | Input::Func(Func::Enter) => {
                if let Some(d) = history::load_revision(&self.list[self.sel].file) {
                    let text = d.plain_text(0, d.len());
                    let tmp = std::env::temp_dir().join("wp51-revision-look.txt");
                    let _ = std::fs::write(&tmp, text);
                    return Outcome::Push(Box::new(crate::filelist::Look::open(&tmp)));
                }
            }
            Input::Char('2') | Input::Char('r') | Input::Char('R') => {
                // open the old version in the other document (Doc 2 / Doc 1)
                if let Some(mut d) = history::load_revision(&self.list[self.sel].file) {
                    let other = 1 - wp.cur;
                    if !wp.docs[other].is_empty() && wp.docs[other].modified {
                        wp.flash("ERROR: The other document has unsaved changes");
                        return Outcome::Keep;
                    }
                    d.id = crate::doc::new_doc_id(); // a copy, with its own history
                    let mut ed = crate::editor::Editor::with_doc(d, None);
                    ed.settle();
                    wp.docs[other] = ed;
                    wp.cur = other;
                    wp.apply_doc_theme();
                    wp.flash("Old version opened as a new document — Shift-F3 switches back");
                    return Outcome::CloseAll;
                }
            }
            Input::Char('3') | Input::Char('c') | Input::Char('C') => {
                if let Some(old) = history::load_revision(&self.list[self.sel].file) {
                    crate::track::compare_into_current(wp, &old);
                    return Outcome::CloseAll;
                }
            }
            Input::Char('4') | Input::Char('s') | Input::Char('S') => {
                let file = self.list[self.sel].file.clone();
                return Outcome::Push(YesNo::new("Replace the current text with this version?", false, move |wp, yes| {
                    if yes {
                        if let Some(d) = history::load_revision(&file) {
                            let e = wp.ed();
                            let id = e.doc.id.clone();
                            e.set_els(d.els); // one undo step: Ctrl-Z brings the text back
                            e.doc.initial_codes = d.initial_codes;
                            e.doc.init = d.init;
                            e.doc.id = id;
                            e.cursor = 0;
                            e.modified = true;
                            e.relayout();
                            e.settle();
                        }
                    }
                    None
                }));
            }
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Char('0') | Input::Char(' ') => return Outcome::Close,
            _ => {}
        }
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + vis {
            self.top = self.sel + 1 - vis;
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, &format!("Revisions of {}", if wp.doc_label().is_empty() { "this document".into() } else { wp.doc_label() }), st);
        g.text(1, 2, "Date      Time    Words   Kind", wp.theme.bold);
        let vis = g.rows.saturating_sub(5);
        for (r, rev) in self.list.iter().enumerate().skip(self.top).take(vis) {
            let (d, t) = crate::filelist::fmt_time(rev.when);
            let s = format!("{d}  {t}  {:>6}   {}", rev.words, kind_name(&rev.kind));
            g.text(1, 3 + r - self.top, &s, if r == self.sel { wp.theme.block } else { st });
            wp.hit(3 + r - self.top, 0, g.cols, crate::ui::HitAction::Select(r));
        }
        let row = g.rows - 1;
        let c = g.text_mn(0, row, "1 ~Look; 2 ~Retrieve as new document; 3 ~Compare with current; 4 Re~store: ", st, wp.theme.bold);
        g.text(c, row, "1", st);
        g.cursor = Some((c, row));
    }
}
