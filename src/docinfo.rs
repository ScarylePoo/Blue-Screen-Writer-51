//! Ctrl-F5: Document Information (word/character/line counts for invoices,
//! incl. standard lines "Normzeilen" of 55 characters), Save As in every
//! export format, and inserting hyperlinks / bookmarks.

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub words: usize,
    pub chars: usize,
    pub chars_no_spaces: usize,
    pub paragraphs: usize,
}

impl Stats {
    fn add_text(&mut self, s: &str) {
        for para in s.split(['\n', '\u{c}']) {
            if para.chars().any(|c| !c.is_whitespace()) {
                self.paragraphs += 1;
            }
            self.words += para.split_whitespace().filter(|w| w.chars().any(|c| c.is_alphanumeric())).count();
            for c in para.chars() {
                if c == '\r' || c == '\u{ad}' {
                    continue;
                }
                self.chars += 1;
                if !c.is_whitespace() {
                    self.chars_no_spaces += 1;
                }
            }
        }
    }
    pub fn std_lines(&self, per_line: u32) -> f64 {
        self.chars as f64 / per_line.max(1) as f64
    }
    pub fn std_pages(&self) -> f64 {
        self.chars as f64 / 1800.0
    }
}

/// Counts for elements; `nested` also counts footnotes/endnotes, headers,
/// footers and comments.
pub fn stats(els: &[El], nested: bool) -> Stats {
    let mut st = Stats::default();
    let d = Document { els: els.to_vec(), ..Default::default() };
    // table cells are separate "paragraphs": treat [Cell]/[Row] as breaks
    let mut text = String::new();
    for e in &d.els {
        match e {
            El::Code(Code::Cell | Code::Row | Code::TblOff) => text.push('\n'),
            El::Code(Code::Note { els, .. } | Code::HeadFoot { els, .. }) if nested => {
                let s = stats(els, true);
                st.words += s.words;
                st.chars += s.chars;
                st.chars_no_spaces += s.chars_no_spaces;
                st.paragraphs += s.paragraphs;
            }
            El::Code(Code::Comment(c)) if nested => {
                let mut s = Stats::default();
                s.add_text(c);
                st.words += s.words;
                st.chars += s.chars;
                st.chars_no_spaces += s.chars_no_spaces;
            }
            e => text.push_str(&Document { els: vec![e.clone()], ..Default::default() }.plain_text(0, 1)),
        }
    }
    st.add_text(&text);
    st
}

/// Parse a money amount typed German or English style ("1,20" / "1.20").
pub fn parse_rate(s: &str) -> Option<f64> {
    let t: String = s.trim().chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    if t.is_empty() {
        return None;
    }
    let t = match (t.rfind(','), t.rfind('.')) {
        (Some(c), Some(p)) if c > p => t.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        (Some(_), None) => t.replace(',', "."),
        _ => t,
    };
    t.parse().ok()
}

fn fmt_num(v: f64, places: usize, german: bool) -> String {
    let s = format!("{v:.places$}");
    let (int, frac) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s.clone(), None),
    };
    let mut g = String::new();
    for (k, c) in int.chars().rev().enumerate() {
        if k > 0 && k % 3 == 0 {
            g.push(if german { '.' } else { ',' });
        }
        g.push(c);
    }
    let mut out: String = g.chars().rev().collect();
    if let Some(f) = frac {
        out.push(if german { ',' } else { '.' });
        out.push_str(&f);
    }
    out
}

pub struct DocInfo {
    main: Stats,
    all: Stats,
    block: Option<Stats>,
    lines: usize,
    pages: u32,
    footnotes: usize,
}

pub fn open(wp: &mut Wp) {
    wp.ed().relayout();
    let e = wp.ed_ref();
    let els = &e.doc.els;
    let block = e.block_range().map(|(a, b)| stats(&els[a..b], true));
    let lines = e.layout.lines.iter().filter(|l| l.start < l.end).count();
    let footnotes = els.iter().filter(|x| matches!(x, El::Code(Code::Note { .. }))).count();
    let info = DocInfo { main: stats(els, false), all: stats(els, true), block, lines, pages: e.layout.pages.max(1), footnotes };
    wp.push(Box::new(info));
}

impl DocInfo {
    fn report(&self, wp: &Wp) -> Vec<(String, Vec<String>)> {
        let ext = &wp.settings.ext;
        let de = !matches!(crate::spell::current_lang(wp).as_str(), "US" | "UK");
        let n = |v: usize| fmt_num(v as f64, 0, de);
        let f2 = |v: f64| fmt_num(v, 2, de);
        let mut cols: Vec<&Stats> = vec![&self.main, &self.all];
        if let Some(b) = &self.block {
            cols.push(b);
        }
        let row = |label: &str, f: &dyn Fn(&Stats) -> String| (label.to_string(), cols.iter().map(|s| f(s)).collect::<Vec<_>>());
        let mut v = vec![
            row("Words", &|s| n(s.words)),
            row("Characters (with spaces)", &|s| n(s.chars)),
            row("Characters (without spaces)", &|s| n(s.chars_no_spaces)),
            row("Paragraphs", &|s| n(s.paragraphs)),
            row(&format!("Standard lines ({} characters)", ext.line_chars), &|s| f2(s.std_lines(ext.line_chars))),
            row("Standard pages (1800 characters)", &|s| f2(s.std_pages())),
        ];
        if let Some(rate) = parse_rate(&ext.line_rate) {
            v.push(row(&format!("Amount at {} per line", ext.line_rate), &|s| f2((s.std_lines(ext.line_chars) * 100.0).ceil() / 100.0 * rate)));
        }
        v.push(("Lines (as printed)".into(), vec![n(self.lines)]));
        v.push(("Pages".into(), vec![n(self.pages as usize)]));
        v.push(("Footnotes/endnotes".into(), vec![n(self.footnotes)]));
        v
    }

    fn as_text(&self, wp: &Wp) -> String {
        let mut s = String::new();
        let name = wp.ed_ref().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        s.push_str(&format!("Document Information {name}\n"));
        s.push_str(&format!("{:<38}\t{:>14}\t{:>14}\n", "", "Text", "+ Notes/Headers"));
        for (l, vals) in self.report(wp) {
            s.push_str(&format!("{l:<38}"));
            for v in vals {
                s.push_str(&format!("\t{v:>14}"));
            }
            s.push('\n');
        }
        s
    }
}

impl Screen for DocInfo {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char('1') => Outcome::Push(Prompt::new("Price per standard line:", &wp.settings.ext.line_rate.clone(), |wp, s| {
                if let Some(s) = s {
                    let mut st = wp.settings.clone();
                    st.ext.line_rate = s.trim().to_string();
                    let _ = st.save();
                    wp.settings = st;
                }
                None
            })),
            Input::Char('2') => Outcome::Push(Prompt::new("Characters per standard line:", &wp.settings.ext.line_chars.to_string(), |wp, s| {
                if let Some(n) = s.and_then(|s| s.trim().parse::<u32>().ok()) {
                    let mut st = wp.settings.clone();
                    st.ext.line_chars = n.clamp(20, 200);
                    let _ = st.save();
                    wp.settings = st;
                }
                None
            })),
            Input::Char('3') => {
                let t = self.as_text(wp);
                match crate::clipboard::set_text(&t) {
                    Ok(()) => wp.flash("Figures copied to the clipboard"),
                    Err(e) => wp.flash(&format!("ERROR: {e}")),
                }
                Outcome::Keep
            }
            Input::Func(Func::Exit) => Outcome::CloseAll,
            Input::Func(Func::Cancel | Func::Esc | Func::Enter) | Input::Char('0' | ' ') => Outcome::Close,
            _ => Outcome::Keep,
        }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Document Information", hi);
        if let Some(p) = &wp.ed_ref().path {
            g.text(22, 0, &p.display().to_string(), st);
        }
        let (c1, c2, c3) = (40usize, 54usize, 70usize);
        let right = |g: &mut Grid, col_end: usize, row: usize, s: &str, sty| {
            let w = s.chars().count();
            g.text(col_end.saturating_sub(w), row, s, sty);
        };
        right(g, c1 + 12, 2, "Text", hi);
        right(g, c2 + 14, 2, "+Notes/Headers", hi);
        if self.block.is_some() {
            right(g, c3 + 9, 2, "Block", hi);
        }
        let mut row = 4;
        for (label, vals) in self.report(wp) {
            if row + 5 >= g.rows {
                break;
            }
            g.text(5, row, &label, st);
            for (k, v) in vals.iter().enumerate() {
                let end = [c1 + 12, c2 + 14, c3 + 9][k.min(2)];
                right(g, end, row, v, if k == 0 { hi } else { st });
            }
            row += 1;
            if label.starts_with("Paragraphs") || label.starts_with("Amount") || label.starts_with("Standard pages") && parse_rate(&wp.settings.ext.line_rate).is_none() {
                row += 1;
            }
        }
        let fr = g.rows - 1;
        let items = ["1 Price per Line", "2 Characters per Line", "3 Copy to Clipboard"];
        let mut c = 0;
        for (k, it) in items.iter().enumerate() {
            let c0 = c;
            c = g.text(c, fr, it, st);
            wp.hit(fr, c0, c, crate::ui::HitAction::Input(Input::Char(char::from(b'1' + k as u8))));
            c = g.text(c, fr, "; ", st);
        }
        let c = g.text(c.saturating_sub(2), fr, ": 0", st);
        g.cursor = Some((c.saturating_sub(1), fr));
    }
}

// ============================================================== Save As

const FORMATS: [(&str, &str); 9] = [
    ("~Word (.docx)", "docx"),
    ("~PDF", "pdf"),
    ("~OpenDocument (.odt)", "odt"),
    ("~RTF", "rtf"),
    ("~HTML (e-mail)", "html"),
    ("~Text", "txt"),
    ("Blue Screen Writer ~stream (.wps)", "wps"),
    ("~Flat ODF (.fodt)", "fodt"),
    ("ODF Te~mplate (.ott)", "ott"),
];

/// Ctrl-F5, 3 Save As.
pub fn save_as_menu() -> Box<dyn Screen> {
    let items: Vec<&str> = FORMATS.iter().map(|f| f.0).collect();
    MenuLine::new(&items, |wp, k| {
        let ext = FORMATS[k].1;
        if ext == "txt" {
            let names: Vec<&str> = vec!["~UTF-8", "~Windows-1252 (Western)", "~DOS 850", "Windows-1251 (~Cyrillic)"];
            return Some(MenuLine::new(&names, |wp, k| Some(save_prompt(wp, "txt", Some(crate::export::TextEnc::ALL[k])))) as Box<dyn Screen>);
        }
        Some(save_prompt(wp, ext, None))
    })
}

fn save_prompt(wp: &mut Wp, ext: &'static str, enc: Option<crate::export::TextEnc>) -> Box<dyn Screen> {
    let label = wp.doc_label();
    let label = if label.is_empty() { String::new() } else { std::path::Path::new(&label).with_extension(ext).display().to_string() };
    Prompt::new("Document to be saved:", &label, move |wp, s| {
        let s = s?;
        if s.trim().is_empty() {
            return None;
        }
        let mut p = wp.resolve_path(&s);
        if p.extension().is_none() {
            p.set_extension(ext);
        }
        save_as(wp, &p, ext, enc);
        None
    })
}

/// Save a copy in another format (the document keeps its own name).
pub fn save_as(wp: &mut Wp, p: &std::path::Path, ext: &str, enc: Option<crate::export::TextEnc>) {
    let doc = wp.ed_ref().doc.clone();
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let res = match (ext, enc) {
        ("pdf", _) => {
            crate::print::save_pdf(wp, p);
            return;
        }
        (_, Some(enc)) => crate::fileio::save_text(p, &doc, enc).map(|lost| {
            if lost > 0 {
                wp.flash(&format!("Saved — {lost} characters not available in {} (written as ?)", enc.name()));
            } else {
                wp.flash(&format!("Saved {name} ({})", enc.name()));
            }
        }),
        _ => crate::fileio::save(p, &doc).map(|()| wp.flash(&format!("Saved {name}"))),
    };
    if let Err(e) = res {
        wp.flash(&format!("ERROR: Can't save file — {e}"));
    }
}

// ======================================================= links/bookmarks

/// Ctrl-F5, 8 Link/Bookmark.
pub fn link_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Hyperlink", "~Bookmark", "~Go to Bookmark", "~Open Link at Cursor"], |wp, k| match k {
        0 => {
            let init = wp.ed_ref().block_els().map(|els| Document { els, ..Default::default() }.plain_text(0, usize::MAX)).filter(|t| t.contains('.') && !t.contains(' ')).unwrap_or_default();
            Some(Prompt::new("Address (URL, e-mail or #bookmark):", &init, |wp, s| {
                if let Some(s) = s {
                    let u = if s.trim().starts_with('#') { s.trim().to_string() } else { crate::links::normalize_url(&s) };
                    crate::links::insert_link(wp, &u);
                }
                None
            }) as Box<dyn Screen>)
        }
        1 => Some(Prompt::new("Bookmark name:", "", |wp, s| {
            if let Some(s) = s {
                crate::links::insert_bookmark(wp, &s);
            }
            None
        }) as Box<dyn Screen>),
        2 => {
            let names: Vec<String> = wp.ed_ref().doc.els.iter().filter_map(|e| if let El::Code(Code::Bookmark(n)) = e { Some(n.clone()) } else { None }).collect();
            let first = names.first().cloned().unwrap_or_default();
            Some(Prompt::new("Go to bookmark:", &first, |wp, s| {
                if let Some(s) = s.filter(|s| !s.trim().is_empty()) {
                    crate::links::follow(wp, &format!("#{}", s.trim()));
                }
                None
            }) as Box<dyn Screen>)
        }
        _ => {
            let e = wp.ed_ref();
            match crate::links::link_at(&e.doc.els, e.cursor) {
                Some(u) => crate::links::follow(wp, &u),
                None => wp.flash("No link at the cursor"),
            }
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        let mut d = Document::from_text("Das ist ein Test.\n\nZweiter  Absatz – mit Strich.");
        d.els.push(El::Code(Code::Note { kind: NoteKind::Foot, els: Document::from_text("Eine Fußnote").els }));
        let s = stats(&d.els, false);
        assert_eq!(s.words, 8);
        assert_eq!(s.paragraphs, 2);
        assert_eq!(s.chars, 17 + 29);
        assert_eq!(s.chars_no_spaces, 14 + 24);
        let a = stats(&d.els, true);
        assert_eq!(a.words, 10);
        assert_eq!(a.chars, 46 + 12);
        assert!((s.std_lines(55) - 46.0 / 55.0).abs() < 1e-9);
    }

    #[test]
    fn rates_and_numbers() {
        assert_eq!(parse_rate("1,20"), Some(1.2));
        assert_eq!(parse_rate("1.234,50 €"), Some(1234.5));
        assert_eq!(parse_rate("1,234.50"), Some(1234.5));
        assert_eq!(parse_rate(""), None);
        assert_eq!(fmt_num(12345.678, 2, true), "12.345,68");
        assert_eq!(fmt_num(1234.0, 0, false), "1,234");
    }

    #[test]
    fn screen_draws() {
        let mut wp = Wp::new(80, 25);
        for c in "Hallo Welt".chars() {
            wp.ed().insert_char(c);
        }
        wp.settings.ext.line_rate = "1,10".into();
        open(&mut wp);
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let text: String = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>() + "\n").collect();
        assert!(text.contains("Words"), "{text}");
        assert!(text.contains("Standard lines (55 characters)"), "{text}");
        assert!(text.contains("Amount at 1,10 per line"), "{text}");
    }
}
