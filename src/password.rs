//! Ctrl-F5, 2 Password: `1 Add/Change; 2 Remove: 0`.  A document with a
//! password is saved as an encrypted .docx (Office "Agile Encryption",
//! AES-256/SHA-512) that Word and LibreOffice open with the same password.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

/// A status-line prompt whose input is not shown (one `*` per character).
pub struct PasswordPrompt {
    pub label: String,
    text: String,
    on_done: Option<Box<dyn FnOnce(&mut Wp, Option<String>) -> Next>>,
}

impl PasswordPrompt {
    pub fn new(label: &str, f: impl FnOnce(&mut Wp, Option<String>) -> Next + 'static) -> Box<PasswordPrompt> {
        Box::new(PasswordPrompt { label: label.into(), text: String::new(), on_done: Some(Box::new(f)) })
    }
}

impl Screen for PasswordPrompt {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let done = match i {
            Input::Char(c) => {
                self.text.push(*c);
                return Outcome::Keep;
            }
            Input::Func(Func::Backspace) => {
                self.text.pop();
                return Outcome::Keep;
            }
            Input::Func(Func::Enter) => Some(std::mem::take(&mut self.text)),
            Input::Func(Func::Cancel | Func::Esc | Func::Exit) => None,
            _ => return Outcome::Keep,
        };
        let cb = self.on_done.take().unwrap();
        match cb(wp, done) {
            Some(n) => Outcome::Replace(n),
            None => Outcome::Close,
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        let c = g.text(0, row, &self.label, st);
        let c = g.text(c, row, " ", st);
        let stars: String = self.text.chars().map(|_| '*').collect();
        let c = g.text(c, row, &stars, st);
        g.cursor = Some((c.min(g.cols - 1), row));
    }
}

/// Ask twice; `f` gets the confirmed password.
pub fn ask_new(f: impl FnOnce(&mut Wp, String) + 'static) -> Box<dyn Screen> {
    PasswordPrompt::new("Enter Password:", move |_wp, s| {
        let first = s.filter(|s| !s.is_empty())?;
        Some(PasswordPrompt::new("Re-Enter Password:", move |wp, s| {
            match s {
                Some(again) if again == first => f(wp, first),
                Some(_) => wp.flash("ERROR: Incorrect password"),
                None => {}
            }
            None
        }) as Box<dyn Screen>)
    })
}

pub fn menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Add/Change", "~Remove"], |wp, k| {
        if k == 0 {
            Some(ask_new(|wp, pw| {
                let e = wp.ed();
                e.doc.password = Some(pw);
                e.modified = true;
                let ext_ok = e.path.as_ref().is_none_or(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("docx")));
                wp.flash(if ext_ok { "Password set — saved encrypted (.docx)" } else { "Password set — only .docx files are saved encrypted" });
            }))
        } else {
            let e = wp.ed();
            if e.doc.password.take().is_some() {
                e.modified = true;
                wp.flash("Password removed");
            }
            None
        }
    })
}

/// The document on disk is encrypted: ask `Enter Password:` and load it.
pub fn open_encrypted(wp: &mut Wp, p: std::path::PathBuf) {
    wp.push(PasswordPrompt::new("Enter Password:", move |wp, s| {
        let pw = s?;
        match crate::fileio::load_with_password(&p, &pw) {
            Ok(doc) => wp.retrieve_loaded(doc, &p),
            Err(e) => wp.flash(&format!("ERROR: {e}")),
        }
        None
    }));
}

#[cfg(test)]
mod tests {
    use crate::doc::*;

    #[test]
    fn encrypted_docx_roundtrip() {
        let dir = std::env::temp_dir().join(format!("bsw51-pw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("geheim.docx");
        let mut d = Document::from_text("Vertraulich: Übersetzung");
        d.password = Some("Kennwort".into());
        crate::fileio::save(&p, &d).unwrap();
        assert!(crate::fileio::is_encrypted(&p));
        assert!(!std::fs::read(&p).unwrap().starts_with(b"PK"));
        assert!(crate::fileio::load(&p).is_err());
        assert!(crate::fileio::load_with_password(&p, "falsch").is_err());
        let d2 = crate::fileio::load_with_password(&p, "Kennwort").unwrap();
        assert_eq!(d2.els, d.els);
        assert_eq!(d2.password.as_deref(), Some("Kennwort"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
