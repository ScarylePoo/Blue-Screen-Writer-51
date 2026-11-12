//! Recently used documents (the last 9), kept in settings.txt.
//! Shift-F10 Retrieve: ↑ / ↓ at `Document to be retrieved:` cycle through them.

use crate::keys::{Func, Input};
use crate::screens::Prompt;
use crate::ui::{Outcome, Screen, Wp};
use std::path::{Path, PathBuf};

pub const MAX: usize = 9;

/// The recent documents, most recent first.
pub fn recent_files(wp: &Wp) -> Vec<PathBuf> {
    wp.settings.ext.recent.clone()
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Add a document to the front of the list (and save the settings).
pub fn remember(wp: &mut Wp, p: &Path) {
    let p = if p.is_absolute() { p.to_path_buf() } else { wp.cwd.join(p) };
    let list = &mut wp.settings.ext.recent;
    if list.first().is_some_and(|f| same(f, &p)) {
        return;
    }
    list.retain(|x| !same(x, &p));
    list.insert(0, p);
    list.truncate(MAX);
    if !cfg!(test) {
        let _ = wp.settings.save();
    }
}

/// `Document to be retrieved:` with ↑/↓ cycling through recent files.
pub struct RecentPrompt {
    inner: Box<Prompt>,
    list: Vec<PathBuf>,
    k: Option<usize>,
}

impl RecentPrompt {
    pub fn new(wp: &Wp, inner: Box<Prompt>) -> Box<RecentPrompt> {
        Box::new(RecentPrompt { inner, list: recent_files(wp), k: None })
    }
    fn show(&mut self) {
        let s = match self.k {
            Some(k) => self.list[k].display().to_string(),
            None => String::new(),
        };
        self.inner.text = s.chars().collect();
        self.inner.pos = self.inner.text.len();
        self.inner.first_key = true;
        self.inner.replace_on_type = true;
    }
}

impl Screen for RecentPrompt {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Up) if !self.list.is_empty() => {
                self.k = Some(match self.k {
                    None => 0,
                    Some(k) => (k + 1) % self.list.len(),
                });
                self.show();
                Outcome::Keep
            }
            Input::Func(Func::Down) if !self.list.is_empty() => {
                self.k = match self.k {
                    None | Some(0) => None,
                    Some(k) => Some(k - 1),
                };
                self.show();
                Outcome::Keep
            }
            _ => self.inner.input(wp, i),
        }
    }

    fn draw(&self, wp: &Wp, g: &mut crate::grid::Grid) {
        self.inner.draw(wp, g);
        if self.k.is_none() && self.inner.text.is_empty() && !self.list.is_empty() {
            let row = g.rows - 1;
            let hint = "(↑ = recent documents)";
            let c = g.cols.saturating_sub(hint.chars().count() + 1);
            g.text(c, row, hint, wp.theme.normal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)] // C:\ paths, case-insensitive like Windows
    fn keeps_nine_most_recent_first() {
        let mut wp = Wp::new(80, 25);
        for k in 0..12 {
            remember(&mut wp, Path::new(&format!("C:\\docs\\d{k}.docx")));
        }
        remember(&mut wp, Path::new("C:\\DOCS\\D5.docx"));
        let r = recent_files(&wp);
        assert_eq!(r.len(), 9);
        assert_eq!(r[0], PathBuf::from("C:\\DOCS\\D5.docx"));
        assert_eq!(r[1], PathBuf::from("C:\\docs\\d11.docx"));
        assert_eq!(r.iter().filter(|p| p.to_string_lossy().to_lowercase().ends_with("d5.docx")).count(), 1);
    }

    #[test]
    #[cfg(windows)] // C:\ paths
    fn up_cycles_in_prompt() {
        let mut wp = Wp::new(80, 25);
        remember(&mut wp, Path::new("C:\\a.docx"));
        remember(&mut wp, Path::new("C:\\b.docx"));
        wp.retrieve_prompt();
        wp.input(Input::Func(Func::Up));
        wp.input(Input::Func(Func::Up));
        let mut g = crate::grid::Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let last: String = (0..80).map(|c| g.get(c, 24).unwrap().ch).collect();
        assert!(last.starts_with("Document to be retrieved: C:\\a.docx"), "{last}");
    }
}
