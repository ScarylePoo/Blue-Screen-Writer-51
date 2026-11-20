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

    /// The cell just started is joined over `cols` × `rows` cells (the
    /// caller adds the empty cells it covers).
    pub fn join(&mut self, cols: usize, rows: usize) {
        if self.tbl_depth == 1 && cols * rows > 1 && matches!(self.els.last(), Some(El::Code(Code::Cell))) {
            self.els.push(El::Code(Code::Join { cols: cols.min(255) as u8, rows: rows.min(255) as u8 }));
        }
    }

    pub fn table_end(&mut self) {
        self.table_end_at();
    }

    /// Like `table_end`, and store what the file said about the lines,
    /// shading and options of the table (see `apply_table_in`).  Nothing
    /// is stored for a table inside a table.
    pub fn table_end_with(&mut self, info: &TableIn) {
        if let Some(at) = self.table_end_at() {
            apply_table_in(&mut self.els, at, info);
        }
    }

    /// Ends the table; the position of its `[Tbl Def]` if one was written.
    fn table_end_at(&mut self) -> Option<usize> {
        if self.tbl_depth == 0 {
            return None;
        }
        self.tbl_depth -= 1;
        if self.tbl_depth > 0 {
            self.ch(' ');
            return None;
        }
        let at = self.tbl_def.take()?;
        self.close_attrs();
        self.pending_hrt = 0;
        self.pending_hpg = false;
        // a table without any cell is dropped
        if !self.els[at..].iter().any(|e| matches!(e, El::Code(Code::Cell))) {
            self.els.truncate(at);
            return None;
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
        Some(at)
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

// ------------------------------------------------------------------
// Table lines, shading and options: what the writers (docx, odt, rtf,
// html) put into a file and what the readers make of one.

/// Lines (left, right, top, bottom) and shading one cell really shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellLook {
    pub lines: [LineStyle; 4],
    pub shade: bool,
}

/// The effective look of every cell of a table.
pub struct TableLook {
    /// rows × columns: the four lines around each grid cell (a line
    /// inside a joined cell is `LineStyle::None`) and its shading
    pub cells: Vec<Vec<CellLook>>,
    /// the same for a table without any `[Cell Lines]` code
    pub plain: Vec<Vec<CellLook>>,
}

impl TableLook {
    fn of(grid: &[Vec<CellLook>], r: usize, c: usize, cols: usize, rows: usize) -> CellLook {
        let at = |r: usize, c: usize| grid.get(r).and_then(|row| row.get(c.min(row.len().saturating_sub(1)))).copied();
        let none = CellLook { lines: [LineStyle::None; 4], shade: false };
        let a = at(r, c).unwrap_or(none);
        let right = at(r, c + cols.max(1) - 1).unwrap_or(a);
        let bottom = at((r + rows.max(1) - 1).min(grid.len().saturating_sub(1)), c).unwrap_or(a);
        CellLook { lines: [a.lines[0], right.lines[1], a.lines[2], bottom.lines[3]], shade: a.shade }
    }
    /// The look of the cell at (r, c) that covers `cols` × `rows` cells.
    pub fn area(&self, r: usize, c: usize, cols: usize, rows: usize) -> CellLook {
        Self::of(&self.cells, r, c, cols, rows)
    }
    /// What `area` would be without any `[Cell Lines]` code.
    pub fn plain_area(&self, r: usize, c: usize, cols: usize, rows: usize) -> CellLook {
        Self::of(&self.plain, r, c, cols, rows)
    }
    /// Has the table no line or shading that differs from the defaults?
    pub fn is_plain(&self) -> bool {
        self.cells == self.plain
    }
}

fn grid_cols(t: &crate::tables::Table) -> usize {
    t.rows.iter().map(|r| r.len()).max().unwrap_or(0).max(t.cols.len())
}

fn owners(t: &crate::tables::Table, els: &[El]) -> impl Fn(usize, usize) -> (usize, usize) + use<> {
    let slots = t.slots(els);
    move |r: usize, c: usize| match slots.get(r).and_then(|row| row.get(c)) {
        Some(crate::tables::Slot::Covered { r, c }) => (*r, *c),
        _ => (r, c),
    }
}

/// The line every cell edge of the table shows, by the rules of
/// `layout::table_rules` (mirrored here: that function is private to the
/// layout and returns its result in the layout's own shape).  `fmt` gives
/// the `[Cell Lines]` setting of a cell.
fn look_with(nrows: usize, ncols: usize, owner: &dyn Fn(usize, usize) -> (usize, usize), fmt: &dyn Fn((usize, usize)) -> CellFmt) -> Vec<Vec<CellLook>> {
    (0..nrows)
        .map(|r| {
            // the vertical line in front of column k
            let v = |k: usize| {
                let left = (k > 0).then(|| owner(r, k - 1));
                let right = (k < ncols).then(|| owner(r, k));
                if left.is_some() && left == right {
                    return LineStyle::None;
                }
                let outer = if k == 0 || k == ncols { LineStyle::Double } else { LineStyle::Single };
                right.and_then(|o| fmt(o).lines[0]).or(left.and_then(|o| fmt(o).lines[1])).unwrap_or(outer)
            };
            // the horizontal line above row j at column c
            let h = |j: usize, c: usize| {
                let up = (j > 0).then(|| owner(j - 1, c));
                let down = (j < nrows).then(|| owner(j, c));
                if up.is_some() && up == down {
                    return LineStyle::None;
                }
                let outer = if j == 0 || j == nrows { LineStyle::Double } else { LineStyle::Single };
                down.and_then(|o| fmt(o).lines[2]).or(up.and_then(|o| fmt(o).lines[3])).unwrap_or(outer)
            };
            (0..ncols).map(|c| CellLook { lines: [v(c), v(c + 1), h(r, c), h(r + 1, c)], shade: fmt(owner(r, c)).shade }).collect()
        })
        .collect()
}

/// The effective lines and shading of the table `t`.
pub fn table_look(els: &[El], t: &crate::tables::Table) -> TableLook {
    let (nrows, ncols) = (t.rows.len(), grid_cols(t));
    let owner = owners(t, els);
    let fmt = |(r, c): (usize, usize)| if r < t.rows.len() && c < t.rows[r].len() { t.fmt_of(els, r, c).map(|x| x.1).unwrap_or_default() } else { CellFmt::default() };
    TableLook { cells: look_with(nrows, ncols, &owner, &fmt), plain: look_with(nrows, ncols, &owner, &|_| CellFmt::default()) }
}

/// Column widths of a table with Position `Full`: stretched (or shrunk)
/// in proportion to fill `avail`, like the layout does.
pub fn full_widths(cols: &[i32], avail: i32) -> Vec<i32> {
    let total: i32 = cols.iter().sum();
    if total <= 0 || avail <= 0 || total == avail {
        return cols.to_vec();
    }
    let n = cols.len();
    let mut used = 0;
    cols.iter()
        .enumerate()
        .map(|(k, w)| {
            let w = if k + 1 == n { avail - used } else { (*w as i64 * avail as i64 / total as i64) as i32 }.max(60);
            used += w;
            w
        })
        .collect()
}

/// Gray fill of shaded cells as RGB hex (`gray` = % of black).
pub fn gray_hex(gray: u8) -> String {
    let v = 255 - (gray.min(100) as u32 * 255 + 50) / 100;
    format!("{v:02X}{v:02X}{v:02X}")
}

/// Darkness of a fill colour in % of black; `None` for white.
pub fn gray_of_rgb(r: u8, g: u8, b: u8) -> Option<u8> {
    let lum = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114 + 500) / 1000;
    let pct = ((255 - lum.min(255)) * 100 + 127) / 255;
    (pct > 0).then_some(pct as u8)
}

/// `RRGGBB` / `#RRGGBB` / `#RGB` → darkness in % of black (`None`: white
/// or not a colour).
pub fn gray_of_hex(s: &str) -> Option<u8> {
    let s = s.trim().trim_start_matches('#');
    if !s.is_ascii() {
        return None;
    }
    let p = |x: &str| u8::from_str_radix(x, 16).ok();
    match s.len() {
        6 => gray_of_rgb(p(&s[0..2])?, p(&s[2..4])?, p(&s[4..6])?),
        3 => gray_of_rgb(p(&s[0..1])? * 17, p(&s[1..2])? * 17, p(&s[2..3])? * 17),
        _ => None,
    }
}

/// A CSS colour (hex, `rgb()`, the common gray names) → darkness in % of
/// black; `None` for white, transparent or an unknown name.
pub fn css_gray(v: &str) -> Option<u8> {
    let v = v.trim().to_ascii_lowercase();
    let v = v.split_whitespace().next().unwrap_or("").to_string() + v.split_once(' ').map(|x| x.1).filter(|_| v.starts_with("rgb")).unwrap_or("");
    if let Some(args) = v.strip_prefix("rgba(").or(v.strip_prefix("rgb(")) {
        let n: Vec<u8> = args.trim_end_matches(')').split([',', ' ', '/']).filter(|x| !x.is_empty()).filter_map(|x| x.trim().parse::<f64>().ok()).map(|x| x.clamp(0.0, 255.0) as u8).collect();
        return if n.len() >= 3 { gray_of_rgb(n[0], n[1], n[2]) } else { None };
    }
    let hex = match v.as_str() {
        "black" => "000000",
        "dimgray" | "dimgrey" => "696969",
        "gray" | "grey" => "808080",
        "darkgray" | "darkgrey" => "a9a9a9",
        "silver" => "c0c0c0",
        "lightgray" | "lightgrey" => "d3d3d3",
        "gainsboro" => "dcdcdc",
        "whitesmoke" => "f5f5f5",
        other => other,
    };
    gray_of_hex(hex)
}

/// A solid line of `pt` points: Single, Thick (from 1.25 pt) or Extra
/// Thick (from 2.25 pt).  The writers use 0.5, 1.5 and 3 pt.
pub fn solid_by_width(pt: f64) -> LineStyle {
    if pt >= 2.25 {
        LineStyle::ExtraThick
    } else if pt >= 1.25 {
        LineStyle::Thick
    } else {
        LineStyle::Single
    }
}

/// A CSS / XSL-FO length in points (`1px` = 0.75 pt; a bare number is px).
pub fn css_len_pt(v: &str) -> Option<f64> {
    let v = v.trim();
    let idx = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(v.len());
    let num: f64 = v[..idx].parse().ok()?;
    let f = match v[idx..].trim().to_ascii_lowercase().as_str() {
        "pt" => 1.0,
        "px" | "" => 0.75,
        "in" | "inch" => 72.0,
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "pc" | "em" | "rem" => 12.0,
        _ => return None,
    };
    (num.is_finite() && num.abs() < 100000.0).then_some(num * f)
}

/// A CSS / XSL-FO border ("0.5pt solid #000000", "none", "thin dashed
/// red") as one of our lines; `None` for an empty value.
pub fn css_border(v: &str) -> Option<LineStyle> {
    let v = v.trim().to_ascii_lowercase();
    if v.is_empty() || v == "inherit" || v == "initial" {
        return None;
    }
    // CSS: without a style there is no line; "medium" is the default width
    let (mut style, mut pt) = ("none", 2.25);
    for tok in v.split_whitespace() {
        match tok {
            "none" | "hidden" | "solid" | "double" | "dashed" | "dotted" | "groove" | "ridge" | "inset" | "outset" => style = tok,
            "thin" => pt = 0.75,
            "medium" => pt = 2.25,
            "thick" => pt = 3.75,
            _ => {
                if let Some(w) = css_len_pt(tok) {
                    pt = w;
                }
            }
        }
    }
    Some(match style {
        "none" | "hidden" => LineStyle::None,
        _ if pt <= 0.0 => LineStyle::None,
        "double" => LineStyle::Double,
        "dashed" => LineStyle::Dashed,
        "dotted" => LineStyle::Dotted,
        _ => solid_by_width(pt),
    })
}

/// What a file says about one cell: lines left, right, top, bottom
/// (`None` = the file says nothing) and the darkness of its fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellIn {
    pub lines: [Option<LineStyle>; 4],
    pub fill: Option<u8>,
}

/// What a file says about a table, collected by a reader and turned into
/// `[Tbl Opt]` / `[Cell Lines]` codes by `apply_table_in`.
#[derive(Clone, Debug, Default)]
pub struct TableIn {
    /// by (row, column) in the grid of the table as read
    pub cells: std::collections::BTreeMap<(usize, usize), CellIn>,
    /// table-wide lines for cells that say nothing themselves: left,
    /// right, top, bottom, inside horizontal, inside vertical
    pub frame: [Option<LineStyle>; 6],
    pub header_rows: usize,
    pub pos: Option<TblPos>,
    /// cell spacing left, right, top, bottom (WPU)
    pub pad: [Option<i32>; 4],
    /// The format draws only the lines its cells state (ODT, RTF, HTML):
    /// as soon as one cell of the table states a line, a side nothing is
    /// said about has no line.  (A table that states no line at all is
    /// still taken as an ordinary table: its lines may come from a style
    /// sheet we do not read.)
    pub unstated_none: bool,
}

impl TableIn {
    pub fn cell(&mut self, r: usize, c: usize) -> &mut CellIn {
        self.cells.entry((r, c)).or_default()
    }
}

/// Cell spacing other programs (and our own writers, for the default)
/// put into ordinary tables: read as our default, so such a table gets no
/// `[Tbl Opt]` code.  Left, right, top, bottom in WPU: Word (0.075"
/// left/right), our .docx (115 twips), LibreOffice (0.097 cm all round),
/// our .odt (0.04" all round), LibreOffice's HTML (cellpadding 4), our
/// .html (2px 6px).
const ORDINARY_PAD: [[i32; 4]; 6] = [[90, 90, 0, 0], [96, 96, 0, 0], [46, 46, 46, 46], [48, 48, 48, 48], [50, 50, 50, 50], [75, 75, 25, 25]];

/// Store what a reader found about the table at `def` (complete, with its
/// `[Tbl Off]`): `[Cell Lines]` codes for lines and shading that differ
/// from the defaults, a `[Tbl Opt]` code for options that do.  Returns
/// the number of codes inserted (all behind `def`).
///
/// Lines: a line between two cells takes what the cells say, otherwise
/// the table-wide line, otherwise our default (double outside, single
/// inside).  If both cells say something different, a visible line wins
/// over "none"; then a cell that only this line runs along wins over a
/// joined cell (which has one setting for a side several lines long);
/// then the heavier line wins, as in Word and in browsers.
///
/// In formats that draw only what the cells state (`unstated_none`), a
/// side without a statement has no line as soon as any cell of the table
/// states one.
///
/// An ordinary grid — every line of the table single, as Word, LibreOffice
/// and browsers draw a table by default — is taken as an ordinary table of
/// this program: no codes, so it shows with our default double frame.
/// Every other table is taken as it is (a single frame stays single).
/// The limit of this: a table of our own whose whole frame was set to
/// Single, with nothing else changed, comes back with the default frame.
///
/// Shading: the table has one darkness (`gray`); it is taken from the
/// first shaded cell.
pub fn apply_table_in(els: &mut Vec<El>, def: usize, info: &TableIn) -> usize {
    let Some(t) = crate::tables::parse(els, def) else { return 0 };
    let (nrows, ncols) = (t.rows.len(), grid_cols(&t));
    let owner = owners(&t, els);
    let bare = info.unstated_none && info.cells.values().any(|x| x.lines.iter().any(|l| l.is_some()));
    let stated = |r: usize, c: usize| {
        let mut x = info.cells.get(&(r, c)).or_else(|| info.cells.get(&owner(r, c))).copied().unwrap_or_default();
        if bare {
            for l in x.lines.iter_mut() {
                l.get_or_insert(LineStyle::None);
            }
        }
        x
    };
    // (columns, rows) a cell covers
    let slots = t.slots(els);
    let extent = |(r, c): (usize, usize)| match slots.get(r).and_then(|row| row.get(c)) {
        Some(crate::tables::Slot::Anchor { cols, rows }) => (*cols, *rows),
        _ => (1, 1),
    };
    // `behind`: the cell on the right / below; `long` = it is a joined
    // cell with more than this one line along its side
    // which of two lines shows when two cells disagree, as Word and
    // browsers decide it: the heavier one
    let weight = |l: LineStyle| match l {
        LineStyle::None => 0,
        LineStyle::Dotted => 1,
        LineStyle::Dashed => 2,
        LineStyle::Single => 3,
        LineStyle::Thick => 4,
        LineStyle::Double => 5,
        LineStyle::ExtraThick => 6,
    };
    let pick = |behind: Option<LineStyle>, long: bool, front: Option<LineStyle>, table: Option<LineStyle>, default: LineStyle| match (behind, front) {
        (Some(LineStyle::None), Some(b)) => b,
        (Some(a), Some(LineStyle::None)) => a,
        (Some(_), Some(b)) if long => b,
        (Some(a), Some(b)) => if weight(b) > weight(a) { b } else { a },
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => table.unwrap_or(default),
    };
    // wanted and default style of every edge that is a line: vertical
    // (row, k = in front of column k), horizontal (j = above row j, column)
    let mut vert = vec![];
    let mut horz = vec![];
    for r in 0..nrows {
        for k in 0..=ncols {
            let left = (k > 0).then(|| owner(r, k - 1));
            let right = (k < ncols).then(|| owner(r, k));
            if left.is_some() && left == right {
                continue;
            }
            let outer = k == 0 || k == ncols;
            let table = info.frame[if k == 0 { 0 } else if k == ncols { 1 } else { 5 }];
            let default = if outer { LineStyle::Double } else { LineStyle::Single };
            let long = right.is_some_and(|o| extent(o).1 > 1) && !left.is_some_and(|o| extent(o).1 > 1);
            let want = pick(right.and_then(|_| stated(r, k).lines[0]), long, left.and_then(|_| stated(r, k - 1).lines[1]), table, default);
            vert.push((r, k, want, default));
        }
    }
    for j in 0..=nrows {
        for c in 0..ncols {
            let up = (j > 0).then(|| owner(j - 1, c));
            let down = (j < nrows).then(|| owner(j, c));
            if up.is_some() && up == down {
                continue;
            }
            let outer = j == 0 || j == nrows;
            let table = info.frame[if j == 0 { 2 } else if j == nrows { 3 } else { 4 }];
            let default = if outer { LineStyle::Double } else { LineStyle::Single };
            let long = down.is_some_and(|o| extent(o).0 > 1) && !up.is_some_and(|o| extent(o).0 > 1);
            let want = pick(down.and_then(|_| stated(j, c).lines[2]), long, up.and_then(|_| stated(j - 1, c).lines[3]), table, default);
            horz.push((j, c, want, default));
        }
    }
    let ordinary = vert.iter().chain(horz.iter()).all(|e| e.2 == LineStyle::Single);
    let mut fmts: std::collections::BTreeMap<(usize, usize), CellFmt> = Default::default();
    if !ordinary {
        // stored where Table Edit stores a line: as Left / Top of the cell
        // behind it, at the right and bottom of the table as Right / Bottom.
        // A joined cell has one Left and one Top for all the lines along
        // that side: those are stored as Right / Bottom of the cells in
        // front of it, which can differ from each other.
        for &(r, k, want, default) in &vert {
            if want != default {
                let front = k == ncols || (k > 0 && extent(owner(r, k)).1 > 1);
                let (cell, side) = if front { (owner(r, k - 1), 1) } else { (owner(r, k), 0) };
                fmts.entry(cell).or_default().lines[side] = Some(want);
            }
        }
        for &(j, c, want, default) in &horz {
            if want != default {
                let front = j == nrows || (j > 0 && extent(owner(j, c)).0 > 1);
                let (cell, side) = if front { (owner(j - 1, c), 3) } else { (owner(j, c), 2) };
                fmts.entry(cell).or_default().lines[side] = Some(want);
            }
        }
    }
    let mut opt = TblOpt::default();
    let mut gray = None;
    for r in 0..nrows {
        for c in 0..t.rows[r].len() {
            if owner(r, c) != (r, c) {
                continue;
            }
            if let Some(g) = info.cells.get(&(r, c)).and_then(|x| x.fill) {
                fmts.entry((r, c)).or_default().shade = true;
                gray.get_or_insert(g);
            }
        }
    }
    if let Some(g) = gray {
        opt.gray = g.clamp(1, 100);
    }
    opt.header_rows = info.header_rows.min(nrows) as u16;
    if let Some(p) = info.pos {
        opt.pos = p;
    }
    let mut pad = opt.pad;
    for (k, p) in info.pad.iter().enumerate() {
        if let Some(p) = p {
            pad[k] = (*p).clamp(0, 2400);
        }
    }
    if info.pad.iter().any(|p| p.is_some()) && !ORDINARY_PAD.iter().any(|o| (0..4).all(|k| (o[k] - pad[k]).abs() <= 3)) {
        opt.pad = pad;
    }
    // insert from the back, so the positions in front stay valid
    let mut n = 0;
    for (&(r, c), f) in fmts.iter().rev() {
        if f.is_default() || r >= t.rows.len() || c >= t.rows[r].len() {
            continue;
        }
        let at = t.join_of(els, r, c).map(|j| j.0 + 1).unwrap_or(t.rows[r][c] + 1);
        els.insert(at, El::Code(Code::CellFmt(*f)));
        n += 1;
    }
    if !opt.is_default() {
        els.insert(def + 1, El::Code(Code::TblOpt(opt)));
        n += 1;
    }
    n
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

/// Tables for the round-trip tests of the file formats.
#[cfg(test)]
pub mod testkit {
    use super::*;
    use LineStyle::{Dashed, Dotted, Double, ExtraThick, Single, Thick};

    pub fn def_of(d: &Document) -> usize {
        d.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).expect("table")
    }

    /// "Vor", a 4 × 3 table, "Nach".  The table has every kind of line,
    /// two shaded cells, a joined cell (A4:B4) and the options `opt`.
    pub fn lines_doc(opt: TblOpt) -> Document {
        let mut d = Document::from_text("Vor\n");
        d.els.push(El::Code(Code::TblDef { cols: vec![1800, 1800, 1800] }));
        if !opt.is_default() {
            d.els.push(El::Code(Code::TblOpt(opt)));
        }
        let f = |lines: [Option<LineStyle>; 4], shade: bool| El::Code(Code::CellFmt(CellFmt { lines, shade }));
        for r in 0..4 {
            d.els.push(El::Code(Code::Row));
            for c in 0..3 {
                d.els.push(El::Code(Code::Cell));
                match (r, c) {
                    (0, 0) => d.els.push(f([Some(Single), None, Some(Thick), None], true)),
                    (0, 2) => d.els.push(f([Some(LineStyle::None), Some(ExtraThick), None, None], false)),
                    (1, 0) => d.els.push(f([None, None, Some(Double), None], false)),
                    (1, 1) => d.els.push(f([Some(Dashed), None, Some(Dotted), None], true)),
                    (1, 2) => d.els.push(f([None, Some(Single), Some(LineStyle::None), None], false)),
                    (2, 1) => d.els.push(f([Some(Thick), None, None, Some(ExtraThick)], false)),
                    (3, 0) => {
                        d.els.push(El::Code(Code::Join { cols: 2, rows: 1 }));
                        d.els.push(f([Some(Dotted), None, None, Some(Single)], false));
                    }
                    (3, 2) => d.els.push(f([None, None, None, Some(Dashed)], false)),
                    _ => {}
                }
                if (r, c) != (3, 1) {
                    d.els.extend(format!("z{r}{c}").chars().map(El::Ch));
                }
            }
        }
        d.els.push(El::Code(Code::TblOff));
        d.els.extend("Nach".chars().map(El::Ch));
        d
    }

    pub fn some_opt(pos: TblPos) -> TblOpt {
        TblOpt { header_rows: 2, pad: [60, 180, 30, 40], neg_paren: false, pos, gray: 25 }
    }

    /// The table of `b` shows what the table of `a` shows: the lines and
    /// shading of every cell, the options, the joined cells and the text.
    pub fn assert_same_look(a: &Document, b: &Document) {
        let (ta, tb) = (crate::tables::parse(&a.els, def_of(a)).unwrap(), crate::tables::parse(&b.els, def_of(b)).unwrap());
        assert_eq!(tb.slots(&b.els), ta.slots(&a.els), "{:?}", b.els);
        let (la, lb) = (table_look(&a.els, &ta), table_look(&b.els, &tb));
        for r in 0..la.cells.len() {
            for c in 0..la.cells[r].len() {
                assert_eq!(lb.cells[r][c], la.cells[r][c], "cell ({r}, {c}): {:?}", b.els);
            }
        }
        assert_eq!(lb.cells.len(), la.cells.len());
        assert_eq!(crate::tables::opt_at(&b.els, tb.def), crate::tables::opt_at(&a.els, ta.def));
        assert_eq!(plain(&b.els).replace('\n', "|"), plain(&a.els).replace('\n', "|"));
    }

    /// No `[Tbl Opt]` and no `[Cell Lines]` code in the document.
    pub fn assert_no_table_codes(d: &Document) {
        assert!(!d.els.iter().any(|e| matches!(e, El::Code(Code::TblOpt(_) | Code::CellFmt(_)))), "{:?}", d.els);
    }

    pub const POSITIONS: [TblPos; 5] = [TblPos::Left, TblPos::Right, TblPos::Center, TblPos::Full, TblPos::Set(2400)];
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
    #[test]
    fn gray_fill_round_trip() {
        for g in 1..=100u8 {
            assert_eq!(gray_of_hex(&gray_hex(g)), Some(g));
        }
        assert_eq!(gray_of_hex("FFFFFF"), None);
        assert_eq!(gray_of_hex("#fff"), None);
        assert_eq!(gray_of_hex("auto"), None);
        assert_eq!(gray_of_hex("000000"), Some(100));
    }

    #[test]
    fn table_in_stores_only_what_differs() {
        use LineStyle::{Double, Single, Thick};
        let table = || {
            let mut els = crate::tables::new_table(3, 3, 5400);
            els.insert(0, El::Ch('x'));
            els
        };
        // nothing said, or our own defaults said: no codes
        let mut els = table();
        assert_eq!(apply_table_in(&mut els, 1, &TableIn::default()), 0);
        let mut info = TableIn { frame: [Some(Double), Some(Double), Some(Double), Some(Double), Some(Single), Some(Single)], ..Default::default() };
        info.pad = [Some(96), Some(96), None, None];
        assert_eq!(apply_table_in(&mut els, 1, &info), 0);
        // the ordinary single grid of other programs: no codes either
        let mut info = TableIn::default();
        for r in 0..3 {
            for c in 0..3 {
                info.cell(r, c).lines = [Some(Single); 4];
            }
        }
        info.pad = [Some(90), Some(90), Some(0), Some(0)];
        assert_eq!(apply_table_in(&mut els, 1, &info), 0);
        assert_eq!(els, table());
        // … but with one thick line the table is taken as it is: the
        // single frame is stored on the cells at the outside
        info.cell(1, 1).lines[3] = Some(Thick);
        info.cell(2, 1).lines[2] = Some(Thick);
        info.cell(0, 2).fill = Some(40);
        info.header_rows = 1;
        assert_eq!(apply_table_in(&mut els, 1, &info), 9);
        let t = crate::tables::parse(&els, 1).unwrap();
        assert_eq!(crate::tables::opt_at(&els, 1), TblOpt { header_rows: 1, gray: 40, ..Default::default() });
        assert_eq!(t.fmt_of(&els, 1, 1), None);
        assert_eq!(t.fmt_of(&els, 2, 1).unwrap().1, CellFmt { lines: [None, None, Some(Thick), Some(Single)], shade: false });
        assert_eq!(t.fmt_of(&els, 0, 2).unwrap().1, CellFmt { lines: [None, Some(Single), Some(Single), None], shade: true });
        let look = table_look(&els, &t);
        assert_eq!(look.cells[1][1].lines, [Single, Single, Single, Thick]);
        assert_eq!(look.cells[0][0].lines, [Single; 4]);
        assert!(!look.is_plain());
    }
}
