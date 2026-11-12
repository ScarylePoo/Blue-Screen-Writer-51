//! Shared helper for the import filters (`fmt_*.rs`): builds a WP code
//! stream with the same conventions as the .docx/.rtf/.doc readers.
//!
//! * character formatting is *wanted* state (`attrs`, `font`); it is
//!   written as paired `[BOLD]…[bold]` codes and `[Font]` only when text
//!   actually follows, so empty runs never produce codes;
//! * paragraph ends are *pending* `[HRt]`s, written when more content
//!   follows — trailing ones at the end of a table cell / note / document
//!   are dropped;
//! * paragraph justification (`just`) becomes a `[Just:…]` code at the
//!   first content of a paragraph when it differs from the current one;
//! * tables: `table_start(cols)`, `row()`, `cell()`, `table_end()` produce
//!   `[Tbl Def][Row][Cell]…[Tbl Off]` (missing widths are filled in);
//! * notes / headers: build the inner text with another `Builder` and pass
//!   `finish()`ed elements to `note()` / `head_foot()`.

use crate::doc::*;

pub struct Builder {
    pub els: Vec<El>,
    /// wanted character attributes for the next text
    pub attrs: AttrSet,
    /// wanted font for the next text
    pub font: FontSpec,
    /// wanted justification of the current paragraph
    pub just: Justify,
    cur_attrs: AttrSet,
    cur_font: FontSpec,
    cur_just: Justify,
    pending_hrt: usize,
    pending_hpg: bool,
    /// table nesting: index of the `[Tbl Def]` of the open table
    tbl_def: Option<usize>,
    /// nested tables (inside a cell) are flattened: depth > 1
    tbl_depth: usize,
    nested_cell: usize,
    /// usable width for default column widths
    pub text_width: i32,
    para_has_content: bool,
}

impl Builder {
    pub fn new(base_font: FontSpec) -> Builder {
        Builder {
            els: vec![],
            attrs: AttrSet::default(),
            font: base_font.clone(),
            just: Justify::Left,
            cur_attrs: AttrSet::default(),
            cur_font: base_font,
            cur_just: Justify::Left,
            pending_hrt: 0,
            pending_hpg: false,
            tbl_def: None,
            tbl_depth: 0,
            nested_cell: 0,
            text_width: 6 * WPU_PER_INCH + WPU_PER_INCH / 2,
            para_has_content: false,
        }
    }

    pub fn set(&mut self, a: Attr, on: bool) {
        self.attrs.set(a, on);
    }

    /// Write pending paragraph/page breaks and the paragraph's justification.
    fn flush_breaks(&mut self) {
        if self.pending_hpg {
            self.pending_hpg = false;
            // a page break ends the paragraph too: one [HPg] replaces one [HRt]
            self.pending_hrt = self.pending_hrt.saturating_sub(1);
            for _ in 0..self.pending_hrt {
                self.els.push(El::Code(Code::HRt));
            }
            self.els.push(El::Code(Code::HPg));
        } else {
            for _ in 0..self.pending_hrt {
                self.els.push(El::Code(Code::HRt));
            }
        }
        self.pending_hrt = 0;
        if !self.para_has_content {
            self.para_has_content = true;
            if self.just != self.cur_just {
                self.els.push(El::Code(Code::Just(self.just)));
                self.cur_just = self.just;
            }
        }
    }

    fn sync_attrs(&mut self) {
        crate::rtf::sync_attrs(&mut self.els, &mut self.cur_attrs, &mut self.cur_font, self.attrs, self.font.clone());
    }

    /// Append one character with the wanted formatting.  `\n` = paragraph
    /// end, `\t` = Tab, `\u{c}` = page break, U+00A0 = hard space,
    /// U+00AD = soft hyphen, other control characters are dropped.
    pub fn ch(&mut self, c: char) {
        match c {
            '\n' => self.para(),
            '\r' => {}
            '\t' => self.code_fmt(Code::Tab),
            '\u{c}' => self.page(),
            '\u{a0}' => self.code_fmt(Code::HardSpace),
            '\u{ad}' => self.code(Code::SoftHyphen),
            '\u{2011}' => self.code(Code::HardHyphen),
            c if (c as u32) < 0x20 || ('\u{7f}'..='\u{9f}').contains(&c) => {}
            '\u{feff}' | '\u{200b}' => {}
            c => {
                self.flush_breaks();
                self.sync_attrs();
                self.els.push(El::Ch(c));
            }
        }
    }

    pub fn text(&mut self, s: &str) {
        for c in s.chars() {
            self.ch(c);
        }
    }

    /// A code that carries the current character formatting (Tab, hard
    /// space, a note reference…).
    pub fn code_fmt(&mut self, c: Code) {
        self.flush_breaks();
        self.sync_attrs();
        self.els.push(El::Code(c));
    }

    /// A code written as is (formatting codes such as margins, `[Center]`,
    /// `[Font]`…) — counts as paragraph content.
    pub fn code(&mut self, c: Code) {
        self.flush_breaks();
        self.els.push(El::Code(c));
    }

    /// A code that does not start the paragraph's content (e.g. a margin
    /// change before the first character): pending breaks are written but
    /// justification is still decided by the first text.
    pub fn code_before_text(&mut self, c: Code) {
        let had = self.para_has_content;
        self.para_has_content = true;
        self.flush_breaks();
        self.para_has_content = had;
        self.els.push(El::Code(c));
    }

    /// End of paragraph.
    /// An empty line between paragraphs (the WP way: [HRt][HRt]), unless
    /// there is one already or nothing has been written yet.
    pub fn blank_line(&mut self) {
        if self.tbl_depth == 0 && (self.pending_hrt > 0 || !self.els.is_empty()) {
            self.pending_hrt = self.pending_hrt.max(2);
            self.para_has_content = false;
        }
    }

    pub fn para(&mut self) {
        if self.tbl_depth > 1 {
            // inside a flattened nested table: keep text on one line
            self.ch(' ');
            return;
        }
        self.pending_hrt += 1;
        self.para_has_content = false;
    }

    /// Hard page break (also ends the paragraph).
    pub fn page(&mut self) {
        if self.tbl_def.is_some() {
            return;
        }
        if self.els.is_empty() && self.pending_hrt == 0 {
            return;
        }
        if self.pending_hrt == 0 {
            self.pending_hrt = 1;
        }
        self.pending_hpg = true;
        self.para_has_content = false;
    }

    /// Is the builder at the start of a line (nothing written in the
    /// current paragraph yet)?
    pub fn at_para_start(&self) -> bool {
        !self.para_has_content
    }

    pub fn in_table(&self) -> bool {
        self.tbl_def.is_some()
    }

    // ---------------------------------------------------------- tables

    /// Start a table; `cols` = column widths in WPU (may be empty: filled
    /// in from the cell count at `table_end`).  A table inside a table is
    /// flattened into its cell (cells separated by Tab, rows by a space).
    pub fn table_start(&mut self, cols: Vec<i32>) {
        self.tbl_depth += 1;
        if self.tbl_depth > 1 {
            self.nested_cell = 0;
            return;
        }
        self.close_attrs();
        // a table starts on its own line
        if self.pending_hrt > 0 || self.pending_hpg {
            if self.pending_hpg {
                self.flush_breaks();
            } else {
                self.flush_breaks_raw();
            }
        } else if !matches!(self.els.last(), None | Some(El::Code(Code::HRt | Code::HPg | Code::TblOff))) {
            self.els.push(El::Code(Code::HRt));
        }
        self.para_has_content = false;
        self.tbl_def = Some(self.els.len());
        self.els.push(El::Code(Code::TblDef { cols }));
    }

    fn flush_breaks_raw(&mut self) {
        for _ in 0..self.pending_hrt {
            self.els.push(El::Code(Code::HRt));
        }
        self.pending_hrt = 0;
    }

    /// Close every open attribute right now (before structural codes).
    fn close_attrs(&mut self) {
        let want = std::mem::take(&mut self.attrs);
        let font = self.cur_font.clone();
        crate::rtf::sync_attrs(&mut self.els, &mut self.cur_attrs, &mut self.cur_font, AttrSet::default(), font);
        self.attrs = want;
    }

    pub fn row(&mut self) {
        if self.tbl_depth > 1 {
            self.nested_cell = 0;
            self.ch(' ');
            return;
        }
        if self.tbl_def.is_none() {
            self.table_start(vec![]);
        }
        self.close_attrs();
        self.pending_hrt = 0;
        self.pending_hpg = false;
        self.els.push(El::Code(Code::Row));
        self.para_has_content = false;
    }

    pub fn cell(&mut self) {
        if self.tbl_depth > 1 {
            if self.nested_cell > 0 {
                self.code_fmt(Code::Tab);
            }
            self.nested_cell += 1;
            return;
        }
        if self.tbl_def.is_none() {
            return;
        }
        if !self.els.iter().skip(self.tbl_def.unwrap_or(0)).any(|e| matches!(e, El::Code(Code::Row))) {
            self.row();
        }
        self.close_attrs();
        self.pending_hrt = 0;
        self.pending_hpg = false;
        self.els.push(El::Code(Code::Cell));
        self.para_has_content = false;
    }

    pub fn table_end(&mut self) {
        if self.tbl_depth == 0 {
            return;
        }
        self.tbl_depth -= 1;
        if self.tbl_depth > 0 {
            self.ch(' ');
            return;
        }
        let Some(at) = self.tbl_def.take() else { return };
        self.close_attrs();
        self.pending_hrt = 0;
        self.pending_hpg = false;
        // a table without any cell is dropped
        if !self.els[at..].iter().any(|e| matches!(e, El::Code(Code::Cell))) {
            self.els.truncate(at);
            return;
        }
        self.els.push(El::Code(Code::TblOff));
        let ncols = crate::tables::parse(&self.els, at).map(|t| t.rows.iter().map(|r| r.len()).max().unwrap_or(1)).unwrap_or(1).max(1);
        let tw = self.text_width.max(1200);
        if let Some(El::Code(Code::TblDef { cols })) = self.els.get_mut(at) {
            cols.retain(|w| *w > 0);
            cols.truncate(ncols.max(cols.len().min(64)));
            if cols.len() < ncols {
                let used: i32 = cols.iter().sum();
                let rest = ncols - cols.len();
                let w = ((tw - used) / rest as i32).max(600);
                cols.resize(ncols, w);
            }
        }
        self.para_has_content = false;
    }

    // ---------------------------------------------------------- notes etc.

    pub fn note(&mut self, kind: NoteKind, els: Vec<El>) {
        self.code_fmt(Code::Note { kind, els });
    }

    pub fn head_foot(&mut self, which: u8, occ: Occurrence, els: Vec<El>) {
        self.code(Code::HeadFoot { which, occ, els });
    }

    /// Finish: close everything; returns the element stream (trailing
    /// paragraph ends dropped).
    pub fn finish(mut self) -> Vec<El> {
        while self.tbl_depth > 0 {
            self.table_end();
        }
        let attrs = self.cur_attrs;
        let mut els = crate::rtf::finish_els(self.els, attrs);
        while matches!(els.last(), Some(El::Code(Code::HRt))) {
            els.pop();
        }
        els
    }

    /// Finish into a document: `init.font` = `base_font`, the first
    /// paragraph's `[Just]` becomes the initial justification.
    pub fn into_document(self, base_font: FontSpec) -> Document {
        let mut d = Document::new();
        d.init.font = base_font;
        d.init.justify = Justify::Left;
        let mut els = self.finish();
        if let Some(El::Code(Code::Just(j))) = els.first().cloned() {
            d.init.justify = j;
            els.remove(0);
        }
        d.els = els;
        d
    }
}

/// Plain text of an element list (for tests and diagnostics).
pub fn plain(els: &[El]) -> String {
    let d = Document { els: els.to_vec(), ..Default::default() };
    d.plain_text(0, d.len())
}

/// Text including nested notes/headers, tables as `|` cells — handy for
/// asserting import results in tests.
#[cfg(test)]
pub fn debug_text(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(Code::HRt) => s.push('\n'),
            El::Code(Code::HPg) => s.push_str("\n[HPg]\n"),
            El::Code(Code::Tab) => s.push('\t'),
            El::Code(Code::HardSpace) => s.push(' '),
            El::Code(Code::Cell) => s.push('|'),
            El::Code(Code::Row) => s.push_str("\n[Row]"),
            El::Code(Code::TblDef { .. }) => s.push_str("[Tbl]"),
            El::Code(Code::TblOff) => s.push_str("[TblOff]"),
            El::Code(Code::Note { kind, els }) => {
                s.push_str(if *kind == NoteKind::Foot { "[Footnote:" } else { "[Endnote:" });
                s.push_str(&debug_text(els));
                s.push(']');
            }
            El::Code(Code::HeadFoot { which, els, .. }) => {
                s.push_str(&format!("[{}:", if *which < 2 { "Header" } else { "Footer" }));
                s.push_str(&debug_text(els));
                s.push(']');
            }
            _ => {}
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> FontSpec {
        FontSpec { name: "Times New Roman".into(), size_cpt: 1200 }
    }

    #[test]
    fn paragraphs_and_attrs() {
        let mut b = Builder::new(f());
        b.text("Ä ");
        b.set(Attr::Bold, true);
        b.text("fett");
        b.set(Attr::Bold, false);
        b.para();
        b.set(Attr::Italic, true); // no text follows → no codes
        b.para();
        b.set(Attr::Italic, false);
        b.just = Justify::Center;
        b.text("Mitte");
        b.para();
        b.para();
        let d = b.into_document(f());
        assert_eq!(d.plain_text(0, d.len()), "Ä fett\n\nMitte");
        assert_eq!(d.els[2], El::Code(Code::AttrOn(Attr::Bold)));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Center))));
        assert!(!d.els.iter().any(|e| matches!(e, El::Code(Code::AttrOn(Attr::Italic)))));
    }

    #[test]
    fn table() {
        let mut b = Builder::new(f());
        b.text("vor");
        b.para();
        b.table_start(vec![]);
        for r in 0..2 {
            b.row();
            for c in 0..3 {
                b.cell();
                b.text(&format!("{r}{c}"));
                b.para();
            }
        }
        b.table_end();
        b.text("nach");
        let els = b.finish();
        assert_eq!(debug_text(&els), "vor\n[Tbl]\n[Row]|00|01|02\n[Row]|10|11|12[TblOff]nach");
        let at = els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        let t = crate::tables::parse(&els, at).unwrap();
        assert_eq!(t.cols.len(), 3);
    }

    #[test]
    fn page_break() {
        let mut b = Builder::new(f());
        b.text("a");
        b.para();
        b.page();
        b.text("b");
        let els = b.finish();
        assert_eq!(debug_text(&els), "a\n[HPg]\nb");
    }
}
