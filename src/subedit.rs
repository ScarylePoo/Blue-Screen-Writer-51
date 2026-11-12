//! Editing nested text (footnotes, endnotes, headers, footers, comments,
//! initial codes) on its own screen, exactly like WP 5.1: the normal editor
//! with a title such as "Footnote: Press Exit when done" on the status line.

use crate::doc::*;
use crate::editor::Editor;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

pub struct SubEdit {
    saved: Option<Editor>,
    title: String,
    on_done: Option<Box<dyn FnOnce(&mut Wp, Vec<El>)>>,
}

/// Open the sub-editor with `els`; `on_done` receives the edited elements
/// when the user presses Exit.
pub fn open(wp: &mut Wp, title: &str, els: Vec<El>, reveal: bool, on_done: impl FnOnce(&mut Wp, Vec<El>) + 'static) {
    let mut d = Document::new();
    d.init = wp.ed_ref().doc.init.clone();
    d.els = els;
    let mut ed = Editor::with_doc(d, None);
    ed.reveal = reveal;
    ed.cursor = ed.doc.els.len();
    let saved = std::mem::replace(&mut wp.docs[wp.cur], ed);
    wp.push(Box::new(SubEdit { saved: Some(saved), title: title.to_string(), on_done: Some(Box::new(on_done)) }));
    wp.after_edit();
}

impl Screen for SubEdit {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::SubEdit
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Exit) => {
                let mut sub = std::mem::replace(&mut wp.docs[wp.cur], self.saved.take().unwrap());
                sub.doc.normalize_attrs();
                let els = std::mem::take(&mut sub.doc.els);
                if let Some(cb) = self.on_done.take() {
                    cb(wp, els);
                }
                wp.after_edit();
                Outcome::Close
            }
            // Nested notes/headers inside a note are not allowed in WP.
            Input::Func(Func::Footnote) | Input::Func(Func::Switch) => Outcome::Keep,
            other => {
                wp.edit_input(other.clone());
                Outcome::Keep
            }
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        // WP shows the title on the status line, replacing the file name.
        let row = wp.status_row();
        let st = wp.theme.normal;
        let label = format!("{}: Press Exit when done", self.title);
        for c in 0..label.chars().count().min(g.cols / 2 + 10) {
            g.put(c, row, ' ', st);
        }
        g.text(0, row, &label, st);
    }
}

// ----------------------------------------------------------- Ctrl-F7

fn note_indices(wp: &Wp, kind: NoteKind) -> Vec<usize> {
    wp.ed_ref()
        .doc
        .els
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, El::Code(Code::Note { kind: k, .. }) if *k == kind))
        .map(|(i, _)| i)
        .collect()
}

fn note_menu(kind: NoteKind) -> Box<dyn Screen> {
    let label = if kind == NoteKind::Foot { "Footnote" } else { "Endnote" };
    MenuLine::new(&["~Create", "~Edit", "~New Number", "~Options"], move |wp, k| match k {
        0 => {
            open(wp, label, vec![], false, move |wp, els| {
                wp.ed().insert_code(Code::Note { kind, els });
            });
            None
        }
        1 => {
            // default: the next note after the cursor
            let notes = note_indices(wp, kind);
            let cur = wp.ed_ref().cursor;
            let pos = notes.iter().position(|&i| i >= cur).unwrap_or(0);
            let dflt = (pos + 1).min(notes.len().max(1));
            Some(Prompt::new(&format!("{label} number?"), &dflt.to_string(), move |wp, s| {
                let n: usize = s?.trim().parse().ok()?;
                let notes = note_indices(wp, kind);
                let idx = *notes.get(n.checked_sub(1)?)?;
                let els = match &wp.ed_ref().doc.els[idx] {
                    El::Code(Code::Note { els, .. }) => els.clone(),
                    _ => return None,
                };
                wp.ed().cursor = idx + 1;
                open(wp, label, els, false, move |wp, els| {
                    let e = wp.ed();
                    if let Some(El::Code(Code::Note { els: old, .. })) = e.doc.els.get_mut(idx) {
                        *old = els;
                        e.modified = true;
                        e.relayout();
                    }
                });
                None
            }) as Box<dyn Screen>)
        }
        _ => {
            crate::menus::not_yet(wp, "Note numbering/options");
            None
        }
    })
}

pub fn footnote_menu(wp: &mut Wp) {
    wp.push(MenuLine::new(&["~Footnote", "~Endnote", "Endnote ~Placement"], |wp, k| match k {
        0 => Some(note_menu(NoteKind::Foot)),
        1 => Some(note_menu(NoteKind::End)),
        _ => {
            crate::menus::not_yet(wp, "Endnote Placement");
            None
        }
    }));
}

// ------------------------------------------------- headers / footers

/// Format → Page → Headers (which_base 0) or Footers (2).
pub fn headfoot_menu(which_base: u8) -> Box<dyn Screen> {
    let (a, b) = if which_base == 0 { ("Header ~A", "Header ~B") } else { ("Footer ~A", "Footer ~B") };
    MenuLine::new(&[a, b], move |_wp, k| {
        let which = which_base + k as u8;
        let name = ["Header A", "Header B", "Footer A", "Footer B"][which as usize];
        Some(MenuLine::new(&["~Discontinue", "Every ~Page", "~Odd Pages", "~Even Pages", "Edi~t"], move |wp, k| {
            let occ = match k {
                0 => {
                    wp.ed().insert_code(Code::HeadFoot { which, occ: Occurrence::Discontinue, els: vec![] });
                    return None;
                }
                1 => Occurrence::Every,
                2 => Occurrence::Odd,
                3 => Occurrence::Even,
                _ => {
                    // edit the last such code before the cursor
                    let cur = wp.ed_ref().cursor;
                    let found = wp.ed_ref().doc.els[..cur].iter().rposition(
                        |e| matches!(e, El::Code(Code::HeadFoot { which: w, occ, .. }) if *w == which && *occ != Occurrence::Discontinue),
                    );
                    let Some(idx) = found else {
                        wp.flash(&format!("ERROR: {name} not found"));
                        return None;
                    };
                    let els = match &wp.ed_ref().doc.els[idx] {
                        El::Code(Code::HeadFoot { els, .. }) => els.clone(),
                        _ => vec![],
                    };
                    open(wp, name, els, false, move |wp, els| {
                        let e = wp.ed();
                        if let Some(El::Code(Code::HeadFoot { els: old, .. })) = e.doc.els.get_mut(idx) {
                            *old = els;
                            e.modified = true;
                            e.relayout();
                        }
                    });
                    return None;
                }
            };
            open(wp, name, vec![], false, move |wp, els| {
                wp.ed().insert_code(Code::HeadFoot { which, occ, els });
            });
            None
        }) as Box<dyn Screen>)
    })
}

/// Format → Document → Initial Codes.
pub fn edit_initial_codes(wp: &mut Wp) {
    let els = wp.ed_ref().doc.initial_codes.clone();
    open(wp, "Initial Codes", els, true, |wp, els| {
        let e = wp.ed();
        // only codes belong in initial codes
        e.doc.initial_codes = els.into_iter().filter(|e| !e.is_char()).collect();
        e.modified = true;
        e.relayout();
    });
}
