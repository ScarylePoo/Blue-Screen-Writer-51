//! Text-mode layout, WordPerfect 5.1 style.
//!
//! Line breaking uses real (printer) font metrics in WPU, while the screen
//! shows one character per cell — exactly like WP 5.1 did.  The screen
//! column of the left margin is `margin / 0.1"`.

use crate::doc::*;

/// Supplies character advance widths in WPU.
pub trait Metrics {
    fn width(&self, font: &FontSpec, size_cpt: u32, ch: char) -> i32;
}

/// Fixed-pitch fallback: 0.6 em (Courier-like).  12pt → 0.1" (10 cpi).
pub struct MonoMetrics;

impl Metrics for MonoMetrics {
    fn width(&self, _font: &FontSpec, size_cpt: u32, _ch: char) -> i32 {
        // pt * 1200/72 * 0.6  ==  cpt * 12 * 0.6 / 72 == cpt / 10
        (size_cpt as i32) / 10
    }
}

pub const COL_WPU: i32 = 120; // one screen column = 0.1"

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnd {
    /// Word-wrapped: `[SRt]`.
    Soft,
    /// `[HRt]`.
    Hard,
    /// `[HPg]`.
    HardPage,
    /// End of document.
    Eof,
    /// Ends because a table code ([Cell], [Row], [Tbl Off]) follows.
    Cell,
}

/// A visual row on the editing screen / page: one or more lines side by
/// side (table cells) or a table border.
#[derive(Clone, Debug)]
pub enum VRow {
    Lines(Vec<usize>),
    Border { table: usize, kind: BorderKind },
    /// An empty row of this height (WPU): text continues below a
    /// graphics box that leaves no room beside it.
    Gap(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderKind {
    Top,
    Mid,
    Bottom,
}

/// Display rows of the editing screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DRow {
    V(usize),
    Break(PageBreak),
    /// One screen row of the double-line box showing the comment at
    /// element `el` (edit screen only; comments are not printed).
    Comment { el: usize, row: u16 },
}

/// Setup: Display → Comments Display (show comment boxes on the edit screen).
pub static COMMENTS_DISPLAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[derive(Clone, Debug)]
pub struct TableGeom {
    /// Column boundaries (WPU, absolute), ncols + 1 entries.
    pub x: Vec<i32>,
}

/// Inner padding of a table cell.
pub const CELL_PAD: i32 = 120;
/// Height a table border takes on paper.
pub const BORDER_H: i32 = 60;

struct TblState {
    id: usize,
    x: Vec<i32>,
    col: Option<usize>,
    row_open: bool,
    row_base: usize,
    row_max: usize,
    cell_lines: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageBreak {
    Soft,
    Hard,
}

#[derive(Clone, Debug)]
pub struct LCell {
    pub col: i32,
    pub ch: char,
    pub attrs: AttrSet,
    pub el: usize,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub term: LineEnd,
    pub cells: Vec<LCell>,
    /// For every element index in `start..=end`: (screen col, pos WPU)
    /// where the cursor is shown when placed *before* that element.
    pub caret: Vec<(i32, i32)>,
    pub page: u32,
    /// Vertical position of the line (top) from top of page, WPU.
    pub y: i32,
    pub height: i32,
    /// Page break drawn *after* this line.
    pub break_after: Option<PageBreak>,
    /// Which kind of tab each [Tab] element on this line landed on.
    pub tab_kinds: Vec<(usize, TabKind)>,
    /// Right edge available to this line (WPU) — margin or table cell.
    pub right: i32,
    /// The line ends in a hyphenated word (soft hyphen or automatic
    /// hyphenation): a '-' is shown/printed at its end.
    pub hyph: bool,
}

impl Line {
    pub fn tab_kind(&self, el: usize) -> TabKind {
        self.tab_kinds.iter().find(|(i, _)| *i == el).map(|x| x.1).unwrap_or(TabKind::Left)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SegKind {
    Center,
    Right,
    Decimal,
}

/// Text after a centre/right/decimal tab or [Center]/[Flsh Rgt] that is
/// positioned once its extent is known.
struct Seg {
    fc: usize,
    fk: usize,
    start_pos: i32,
    start_col: i32,
    kind: SegKind,
    target: i32,
    tcol: i32,
    /// (pos, cells index) of the alignment character for decimal tabs
    dec: Option<(i32, usize)>,
    /// Fill the gap with a dot leader (`[Dot Flsh Rgt]`, dot-leader tabs).
    leader: bool,
    /// Element index of the tab / flush-right code (owner of leader cells).
    el: usize,
}

fn finish_seg(sg: &Seg, cells: &mut Vec<LCell>, caret: &mut [(i32, i32)], pos: &mut i32, col: &mut i32) {
    let seg_w = *pos - sg.start_pos;
    let seg_cols = cells.len().saturating_sub(sg.fc) as i32;
    let (w_before, c_before) = match sg.kind {
        SegKind::Center => (seg_w / 2, seg_cols / 2),
        SegKind::Right => (seg_w, seg_cols),
        SegKind::Decimal => match sg.dec {
            Some((p, ci)) => (p - sg.start_pos, (ci - sg.fc) as i32),
            None => (seg_w, seg_cols),
        },
    };
    let dpos = (sg.target - w_before - sg.start_pos).max(0);
    let dcol = (sg.tcol - c_before - sg.start_col).max(0);
    for c in cells.iter_mut().skip(sg.fc) {
        c.col += dcol;
    }
    for k in caret.iter_mut().skip(sg.fk + 1) {
        k.0 += dcol;
        k.1 += dpos;
    }
    *pos += dpos;
    *col += dcol;
    if sg.leader && dcol >= 3 {
        let dots: Vec<LCell> =
            (sg.start_col + 1..sg.start_col + dcol - 1).map(|c| LCell { col: c, ch: '.', attrs: AttrSet::default(), el: sg.el }).collect();
        cells.splice(sg.fc..sg.fc, dots);
    }
}

impl Line {
    pub fn caret_of(&self, idx: usize) -> (i32, i32) {
        let k = idx.saturating_sub(self.start).min(self.caret.len().saturating_sub(1));
        self.caret.get(k).copied().unwrap_or((0, 0))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub pages: u32,
    /// Per page (index = page-1): element index of the header/footer code
    /// in effect for Header A, Header B, Footer A, Footer B.
    pub page_hf: Vec<[Option<usize>; 4]>,
    /// Per page: element indices of the footnotes printed at its bottom.
    pub page_notes: Vec<Vec<usize>>,
    /// Footnote/endnote number for a Note element index.
    pub note_nums: std::collections::HashMap<usize, u32>,
    pub vrows: Vec<VRow>,
    pub tables: Vec<TableGeom>,
    pub line_vrow: Vec<usize>,
    pub vrow_y: Vec<i32>,
    pub vrow_page: Vec<u32>,
    pub drows: Vec<DRow>,
    pub vrow_drow: Vec<usize>,
    /// Text columns: per line, where it sits (None outside columns).
    pub line_col: Vec<Option<crate::columns::LineCol>>,
    /// Text column regions ([Col On] … [Col Off]).
    pub col_blocks: Vec<crate::columns::ColBlock>,
    /// Words the user should be asked to hyphenate (Setup: Prompt for
    /// Hyphenation): (first letter, end of word, suggested break), element indices.
    pub hyph_ask: Vec<(usize, usize, usize)>,
    /// Per page: the `[Paper Sz/Typ]` in effect (None = document page size).
    pub page_paper: Vec<Option<crate::fmt::Paper>>,
    /// Graphics boxes and where they landed (gfx.rs).
    pub boxes: Vec<crate::gfx::BoxGeom>,
}

impl Layout {
    /// Display row of a line.
    pub fn drow_of_line(&self, li: usize) -> usize {
        self.line_vrow.get(li).and_then(|v| self.vrow_drow.get(*v)).copied().unwrap_or(li)
    }
}

impl Layout {
    /// Index of the line containing cursor position `idx`.
    pub fn line_of(&self, idx: usize) -> usize {
        // binary search: last line with start <= idx
        let mut lo = 0usize;
        let mut hi = self.lines.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.lines[mid].start <= idx {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        // A cursor at line.end of a soft-wrapped line belongs to next line
        // (handled because next line starts at that index) — but at the end
        // of a table cell it stays in that cell.
        if lo > 0 && self.lines[lo].start == idx && self.lines[lo - 1].term == LineEnd::Cell && self.lines[lo - 1].end == idx {
            return lo - 1;
        }
        lo
    }

    /// Find the element index closest to screen column `col` on line `li`.
    pub fn idx_at_col(&self, li: usize, col: i32) -> usize {
        let line = &self.lines[li];
        let last = if matches!(line.term, LineEnd::Soft | LineEnd::Eof | LineEnd::Cell) {
            line.end
        } else {
            line.end.saturating_sub(1).max(line.start)
        };
        let mut best = line.start;
        for idx in line.start..=last {
            let (c, _) = line.caret_of(idx);
            if c <= col {
                best = idx;
            } else {
                break;
            }
        }
        // For soft lines, the end belongs to the next line; step back.
        if line.term == LineEnd::Soft && best == line.end && line.end > line.start {
            best = line.end - 1;
        }
        best
    }
}

#[derive(Clone)]
struct State {
    /// inside a hyperlink (shown underlined)
    link: bool,
    /// tracked change in effect: 0 none, 1 inserted, 2 deleted
    rev: u8,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    page_w: i32,
    page_h: i32,
    font: std::rc::Rc<FontSpec>,
    tabs: std::rc::Rc<TabSet>,
    just: Justify,
    spacing: u32,
    attrs: AttrSet,
    /// `[Hyph On]`
    hyph: bool,
    /// Further format settings (fmt.rs): zone, line height, language …
    fx: crate::fmt::FmtState,
}

/// Line height for the state, for text of `size_cpt`.
fn st_line_height(st: &State, size_cpt: u32) -> i32 {
    match st.fx.ln_height {
        Some(h) => (h as i64 * st.spacing as i64 / 100) as i32,
        None => line_height(&FontSpec { name: String::new(), size_cpt }, st.spacing),
    }
}

fn size_for(attrs: AttrSet, base: u32) -> u32 {
    let pct = if attrs.has(Attr::ExtraLarge) {
        200
    } else if attrs.has(Attr::VeryLarge) {
        150
    } else if attrs.has(Attr::Large) {
        120
    } else if attrs.has(Attr::Small) {
        80
    } else if attrs.has(Attr::Fine) {
        60
    } else {
        100
    };
    let pct = if attrs.has(Attr::Superscript) || attrs.has(Attr::Subscript) { pct * 60 / 100 } else { pct };
    base * pct / 100
}

/// Attributes as displayed/printed: tracked insertions show as redline
/// + underline, deletions as strikeout.
fn shown_attrs(st: &State) -> AttrSet {
    let mut a = st.attrs;
    if st.link {
        a.set(Attr::Underline, true);
    }
    match st.rev {
        1 => {
            a.set(Attr::Redline, true);
            a.set(Attr::Underline, true);
        }
        2 => {
            a.set(Attr::Redline, true);
            a.set(Attr::Strikeout, true);
        }
        _ => {}
    }
    a
}

fn line_height(font: &FontSpec, spacing: u32) -> i32 {
    // pt → WPU is *1200/72; cpt = pt*100.
    let h = font.size_cpt as i64 * 12 / 72; // WPU for 100 % spacing
    (h * spacing as i64 / 100) as i32
}

fn apply_code(st: &mut State, c: &Code) {
    match c {
        Code::AttrOn(a) => st.attrs.set(*a, true),
        Code::AttrOff(a) => st.attrs.set(*a, false),
        Code::RevOn { del, .. } => st.rev = if *del { 2 } else { 1 },
        Code::RevOff { .. } => st.rev = 0,
        Code::Link(_) => st.link = true,
        Code::LinkEnd => st.link = false,
        Code::Font(f) => st.font = std::rc::Rc::new(f.clone()),
        Code::LRMargin { left, right } => {
            st.left = *left;
            st.right = *right;
        }
        Code::TBMargin { top, bottom } => {
            st.top = *top;
            st.bottom = *bottom;
        }
        Code::LineSpacing(s) => st.spacing = *s,
        Code::Just(j) => st.just = *j,
        Code::TabSet(t) => st.tabs = std::rc::Rc::new(t.clone()),
        Code::Hyphenation(h) => st.hyph = *h,
        Code::Fmt(f) => {
            st.fx.apply(f);
            if let crate::fmt::Fmt::Paper(p) = f {
                match &p.labels {
                    Some(l) => {
                        st.page_w = l.w;
                        st.page_h = l.h;
                        st.left = l.ml;
                        st.right = l.mr;
                        st.top = l.mt;
                        st.bottom = l.mb;
                    }
                    None => {
                        st.page_w = p.w;
                        st.page_h = p.h;
                    }
                }
            }
        }
        _ => {}
    }
}

/// Lay out the whole document.
pub fn layout(doc: &Document, m: &dyn Metrics) -> Layout {
    // Styles and paragraph numbers: lay out the expanded copy and map it
    // back onto the document (see styles.rs).
    if let Some(v) = crate::styles::virtual_stream(doc) {
        let hints = std::collections::HashMap::new();
        let mut l = layout_lines(&v.doc, m, &hints);
        crate::styles::remap_layout(&mut l, &v);
        paginate(doc, &mut l, m);
        crate::gfx::finish(&mut l);
        return l;
    }
    layout_flat(doc, m)
}

/// Layout without style expansion (the document has none, or is already
/// expanded).  Newspaper columns of different widths: lines are wrapped
/// for the column pagination puts them in, which may take another pass.
pub fn layout_flat(doc: &Document, m: &dyn Metrics) -> Layout {
    let mut hints = std::collections::HashMap::new();
    let mut l = layout_pass(doc, m, &hints);
    for _ in 0..3 {
        match crate::columns::rewrap_hints(&l) {
            Some(h) if h != hints => {
                hints = h;
                l = layout_pass(doc, m, &hints);
            }
            _ => break,
        }
    }
    l
}

fn layout_pass(doc: &Document, m: &dyn Metrics, col_hints: &std::collections::HashMap<usize, usize>) -> Layout {
    let mut l = layout_lines(doc, m, col_hints);
    paginate(doc, &mut l, m);
    crate::gfx::finish(&mut l);
    l
}

/// Pass 1: lines and visual rows.  `col_hints` maps the first element of a
/// line in newspaper columns to the column it is wrapped for.
fn layout_lines(doc: &Document, m: &dyn Metrics, col_hints: &std::collections::HashMap<usize, usize>) -> Layout {
    use crate::columns::{ColBlock, ColRun, LineCol};
    let init = &doc.init;
    let mut st = State {
        left: init.left,
        right: init.right,
        top: init.top,
        bottom: init.bottom,
        page_w: init.page_width,
        page_h: init.page_height,
        font: std::rc::Rc::new(init.font.clone()),
        tabs: std::rc::Rc::new(init.tabs.clone()),
        just: init.justify,
        spacing: init.line_spacing,
        attrs: AttrSet::default(),
        rev: 0,
        link: false,
        hyph: false,
        fx: Default::default(),
    };
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            apply_code(&mut st, c);
        }
    }
    let els = &doc.els;
    let n = els.len();
    let mut lines: Vec<Line> = Vec::new();
    let mut page: u32 = 1;
    let mut y = st.top;
    let mut i = 0usize;
    // Temporary indent state (F4 / Shift-F4) lasts until HRt.
    let mut indent_left: Option<i32> = None;
    let mut indent_right: Option<i32> = None;
    let mut note_nums: std::collections::HashMap<usize, u32> = Default::default();
    let (mut foot_no, mut end_no) = (0u32, 0u32);
    let mut vrows: Vec<VRow> = Vec::new();
    let mut tables: Vec<TableGeom> = Vec::new();
    let mut line_vrow: Vec<usize> = Vec::new();
    let mut next_vrow = 0usize;
    let mut tbl: Option<TblState> = None;
    // text columns
    let mut coldef: Option<(ColKind, Vec<(i32, i32)>)> = None;
    let mut colrun: Option<ColRun> = None;
    let mut col_blocks: Vec<ColBlock> = Vec::new();
    let mut line_col: Vec<Option<LineCol>> = Vec::new();
    let mut math_on = false;
    let mut hyph_ask: Vec<(usize, usize, usize)> = Vec::new();
    let mut gx = crate::gfx::LayoutState::new();
    fn ensure(vrows: &mut Vec<VRow>, v: usize) {
        while vrows.len() <= v {
            vrows.push(VRow::Lines(vec![]));
        }
    }
    let finish_row = |t: &mut TblState, next_vrow: &mut usize, vrows: &mut Vec<VRow>, kind: BorderKind| {
        if t.row_open {
            *next_vrow = t.row_base + t.row_max.max(1);
            ensure(vrows, *next_vrow);
            vrows[*next_vrow] = VRow::Border { table: t.id, kind };
            *next_vrow += 1;
            t.row_open = false;
        }
    };

    loop {
        let line_start = i;
        // Table codes at the start of a line set up the cell first.
        let mut pre_end = i;
        while pre_end < n {
            match &els[pre_end] {
                El::Code(Code::TblDef { cols }) => {
                    if let Some(mut t) = tbl.take() {
                        finish_row(&mut t, &mut next_vrow, &mut vrows, BorderKind::Bottom);
                    }
                    let mut x = vec![st.left];
                    for w in cols {
                        x.push(x.last().unwrap() + (*w).max(COL_WPU * 2));
                    }
                    tables.push(TableGeom { x: x.clone() });
                    ensure(&mut vrows, next_vrow);
                    vrows[next_vrow] = VRow::Border { table: tables.len() - 1, kind: BorderKind::Top };
                    next_vrow += 1;
                    tbl = Some(TblState { id: tables.len() - 1, x, col: None, row_open: false, row_base: next_vrow, row_max: 0, cell_lines: 0 });
                }
                El::Code(Code::Row) => {
                    if let Some(t) = tbl.as_mut() {
                        finish_row(t, &mut next_vrow, &mut vrows, BorderKind::Mid);
                        t.row_open = true;
                        t.row_base = next_vrow;
                        t.row_max = 0;
                        t.col = None;
                    }
                }
                El::Code(Code::Cell) => {
                    if let Some(t) = tbl.as_mut() {
                        if !t.row_open {
                            t.row_open = true;
                            t.row_base = next_vrow;
                            t.row_max = 0;
                        }
                        let c = t.col.map(|c| c + 1).unwrap_or(0).min(t.x.len().saturating_sub(2));
                        t.col = Some(c);
                        t.cell_lines = 0;
                    }
                }
                El::Code(Code::TblOff) => {
                    if let Some(mut t) = tbl.take() {
                        finish_row(&mut t, &mut next_vrow, &mut vrows, BorderKind::Bottom);
                    }
                }
                El::Code(Code::ColDef { kind, cols }) => coldef = Some((*kind, cols.clone())),
                El::Code(Code::ColOn) => {
                    if let Some(r) = colrun.take() {
                        r.finish(&mut next_vrow);
                    }
                    if tbl.is_none() {
                        colrun = ColRun::start(coldef.as_ref(), &mut col_blocks, next_vrow);
                    }
                }
                El::Code(Code::ColOff) => {
                    if let Some(r) = colrun.take() {
                        r.finish(&mut next_vrow);
                    }
                }
                // format codes right before [Col On]/[Col Off] belong to its line
                El::Code(c) if crate::columns::col_code_ahead(els, pre_end) => apply_code(&mut st, c),
                _ => break,
            }
            pre_end += 1;
            // one cell per line: stop after [Cell] / [Tbl Off]
            if matches!(els[pre_end - 1], El::Code(Code::Cell | Code::TblOff)) {
                break;
            }
        }
        // a paper size code at the very start of a line (e.g. labels)
        // already sets this line's margins
        {
            let mut k = pre_end.max(i);
            while k < n && crate::editor::is_invisible(&els[k]) {
                if let El::Code(c @ Code::Fmt(crate::fmt::Fmt::Paper(_))) = &els[k] {
                    apply_code(&mut st, c);
                }
                k += 1;
            }
        }
        let in_cell = tbl.as_ref().and_then(|t| t.col.map(|c| (t.x[c], t.x[c + 1])));
        if in_cell.is_some() {
            indent_left = None;
            indent_right = None;
        }
        let in_col = colrun.as_mut().map(|r| r.edges(line_start, col_hints));
        let mut lm = match (in_cell, in_col) {
            (Some((a, _)), _) => a + CELL_PAD,
            (None, Some((a, _))) => indent_left.unwrap_or(a),
            (None, None) => indent_left.unwrap_or(st.left),
        };
        let mut rm_limit = match (in_cell, in_col) {
            (Some((_, b)), _) => b - CELL_PAD,
            (None, Some((_, b))) => b - indent_right.unwrap_or(0),
            (None, None) => st.page_w - st.right - indent_right.unwrap_or(0),
        };
        // graphics boxes: text wraps around them (or continues below)
        if tbl.is_none() && pre_end == i && i < n {
            let cx = crate::gfx::Ctx { left: st.left, right: st.right, top: st.top, bottom: st.bottom, page_w: st.page_w, page_h: st.page_h, page, y, vrow: Some(next_vrow), line: lines.len() };
            gx.activate_leading(doc, i, &cx, m);
            if gx.has_zones() {
                let lh = line_height(&st.font, st.spacing);
                match gx.narrow(lm, rm_limit, page, y, lh) {
                    Some((a, b)) => {
                        lm = a;
                        rm_limit = b;
                    }
                    None => {
                        if y + lh > st.page_h - st.bottom && y > st.top {
                            page += 1;
                            y = st.top;
                        }
                        ensure(&mut vrows, next_vrow);
                        vrows[next_vrow] = VRow::Gap(lh);
                        next_vrow += 1;
                        y += lh;
                        gx.consume(lh, page);
                        continue;
                    }
                }
            }
        }
        let mut pos = lm; // current x in WPU
        let mut col = lm / COL_WPU;
        let mut cells: Vec<LCell> = Vec::new();
        let mut caret: Vec<(i32, i32)> = Vec::new();
        let mut max_h = st_line_height(&st, st.font.size_cpt);
        let mut hyph_end = false;
        // last break opportunity: (element index to start next line, cells len, caret len, state snapshot)
        let mut last_break: Option<(usize, usize, usize, State)> = None;
        let mut term = LineEnd::Eof;
        // pending aligned segment (centre/right/decimal tab, [Center], [Flsh Rgt])
        let mut seg: Option<Seg> = None;
        let mut tab_kinds: Vec<(usize, TabKind)> = Vec::new();
        // justification in effect at the first character of the line (a [Just]
        // code later on the line applies from the next line)
        let mut first_just: Option<Justify> = None;
        let lm_col = lm / COL_WPU;
        let rm_col = rm_limit / COL_WPU;

        while i < n {
            match &els[i] {
                El::Ch(ch) => {
                    let ch = *ch;
                    let sz = size_for(st.attrs, st.font.size_cpt);
                    let w = m.width(&st.font, sz, ch);
                    let is_space = ch == ' ';
                    // Format: Printer Functions — word/letter spacing
                    let w = w * if is_space { st.fx.wrd_ltr.0.pct() } else { st.fx.wrd_ltr.1.pct() } / 100;
                    if !is_space && pos + w > rm_limit && !cells.is_empty() {
                        if st.hyph && in_cell.is_none() {
                            let ws = last_break.as_ref().map(|b| b.0).unwrap_or(line_start);
                            let hw = m.width(&st.font, sz, '-');
                            let hz = crate::hyph::Zone { lm, rm: rm_limit, left_pct: st.fx.hzone.0 };
                            match crate::hyph::decide(els, line_start, ws, i, &caret, pos, hw, &hz, &st.fx.lang) {
                                crate::hyph::Decision::Break(e) => {
                                    let hcol = if e < i { caret[e - line_start].0 } else { col };
                                    let cut = cells.iter().position(|c| c.el >= e).unwrap_or(cells.len());
                                    cells.truncate(cut);
                                    caret.truncate(e - line_start);
                                    cells.push(LCell { col: hcol, ch: '-', attrs: shown_attrs(&st), el: e.saturating_sub(1) });
                                    col = hcol;
                                    pos = caret.last().map(|k| k.1).unwrap_or(pos);
                                    i = e;
                                    term = LineEnd::Soft;
                                    hyph_end = true;
                                    break;
                                }
                                crate::hyph::Decision::Ask(a, b, sug) => {
                                    if !hyph_ask.iter().any(|x| x.0 == a) {
                                        hyph_ask.push((a, b, sug));
                                    }
                                }
                                crate::hyph::Decision::Wrap => {}
                            }
                        }
                        if let Some((bi, cl, kl, snap)) = last_break.take() {
                            if bi > 0 && matches!(els[bi - 1], El::Code(Code::SoftHyphen)) {
                                let hcol = caret.get(bi - 1 - line_start).map(|k| k.0).unwrap_or(col);
                                cells.truncate(cl);
                                cells.push(LCell { col: hcol, ch: '-', attrs: shown_attrs(&snap), el: bi - 1 });
                                caret.truncate(kl);
                                st = snap;
                                i = bi;
                                term = LineEnd::Soft;
                                hyph_end = true;
                                break;
                            }
                            // wrap at last opportunity
                            cells.truncate(cl);
                            caret.truncate(kl);
                            st = snap;
                            i = bi;
                            term = LineEnd::Soft;
                            break;
                        } else {
                            // no break opportunity: break mid-word
                            term = LineEnd::Soft;
                            break;
                        }
                    }
                    if let Some(sg) = seg.as_mut() {
                        let dc = st.fx.dec;
                        if sg.kind == SegKind::Decimal && sg.dec.is_none() && (ch == dc || dc == '.' && ch == ',') {
                            sg.dec = Some((pos, cells.len()));
                        }
                    }
                    caret.push((col, pos));
                    first_just.get_or_insert(st.just);
                    cells.push(LCell { col, ch, attrs: shown_attrs(&st), el: i });
                    pos += w;
                    col += 1;
                    let lh = st_line_height(&st, sz);
                    if lh > max_h {
                        max_h = lh;
                    }
                    i += 1;
                    if is_space || ch == '-' || ch == '\u{2014}' || ch == '\u{2013}' || ch == '/' && false {
                        last_break = Some((i, cells.len(), caret.len(), st.clone()));
                    }
                }
                El::Code(c) => {
                    caret.push((col, pos));
                    match c {
                        Code::TblDef { .. } | Code::Row | Code::Cell | Code::TblOff if i >= pre_end => {
                            // a new cell/row starts: end this line before it
                            caret.pop();
                            term = LineEnd::Cell;
                            break;
                        }
                        Code::TblDef { .. } | Code::Row | Code::Cell | Code::TblOff => {}
                        Code::ColOn | Code::ColOff if i >= pre_end => {
                            // columns switch on/off: end this line before it
                            caret.pop();
                            term = LineEnd::Cell;
                            break;
                        }
                        Code::ColOn | Code::ColOff => {}
                        Code::ColDef { kind, cols } => coldef = Some((*kind, cols.clone())),
                        Code::HRt => {
                            i += 1;
                            term = LineEnd::Hard;
                            break;
                        }
                        Code::HPg => {
                            i += 1;
                            term = LineEnd::HardPage;
                            break;
                        }
                        Code::HardSpace => {
                            let sz = size_for(st.attrs, st.font.size_cpt);
                            let w = m.width(&st.font, sz, ' ');
                            cells.push(LCell { col, ch: ' ', attrs: st.attrs, el: i });
                            pos += w;
                            col += 1;
                        }
                        Code::HardHyphen => {
                            let sz = size_for(st.attrs, st.font.size_cpt);
                            let w = m.width(&st.font, sz, '-');
                            cells.push(LCell { col, ch: '-', attrs: st.attrs, el: i });
                            pos += w;
                            col += 1;
                        }
                        Code::SoftHyphen | Code::Fmt(crate::fmt::Fmt::ISRt) => {
                            // break opportunity, invisible unless at line end
                            last_break = Some((i + 1, cells.len(), caret.len(), st.clone()));
                        }
                        Code::Fmt(crate::fmt::Fmt::Adv(a, v)) if matches!(a, crate::fmt::Adv::Left | crate::fmt::Adv::Right | crate::fmt::Adv::ToPos) => {
                            let np = match a {
                                crate::fmt::Adv::Left => pos - v,
                                crate::fmt::Adv::Right => pos + v,
                                _ => *v,
                            }
                            .max(0);
                            pos = np;
                            col = (np + COL_WPU / 2) / COL_WPU;
                        }
                        Code::Fmt(crate::fmt::Fmt::Ovrstk(s)) => {
                            let sz = size_for(st.attrs, st.font.size_cpt);
                            let ch = crate::fmtmenu::overstrike_char(s);
                            let w = s.chars().map(|c| m.width(&st.font, sz, c)).max().unwrap_or(0);
                            cells.push(LCell { col, ch, attrs: shown_attrs(&st), el: i });
                            pos += w;
                            col += 1;
                        }
                        Code::Fmt(crate::fmt::Fmt::InsertPgNum(style)) => {
                            let sz = size_for(st.attrs, st.font.size_cpt);
                            for d in crate::fmt::fill_style(style, &page.to_string()).chars() {
                                cells.push(LCell { col, ch: d, attrs: shown_attrs(&st), el: i });
                                pos += m.width(&st.font, sz, d);
                                col += 1;
                            }
                        }
                        Code::Note { kind, .. } => {
                            let num = match note_nums.get(&i) {
                                Some(n) => *n,
                                None => {
                                    let n = if *kind == NoteKind::Foot {
                                        foot_no += 1;
                                        foot_no
                                    } else {
                                        end_no += 1;
                                        end_no
                                    };
                                    note_nums.insert(i, n);
                                    n
                                }
                            };
                            let mut a = st.attrs;
                            a.set(Attr::Superscript, true);
                            let sz = size_for(a, st.font.size_cpt);
                            for d in num.to_string().chars() {
                                let w = m.width(&st.font, sz, d);
                                cells.push(LCell { col, ch: d, attrs: a, el: i });
                                pos += w;
                                col += 1;
                            }
                        }
                        Code::Tab | Code::Indent | Code::DblIndent | Code::Fmt(crate::fmt::Fmt::HardTab(_) | crate::fmt::Fmt::DecTab) => {
                            if let Some(sg) = seg.take() {
                                finish_seg(&sg, &mut cells, &mut caret, &mut pos, &mut col);
                            }
                            let base = if st.tabs.relative { st.left } else { 0 };
                            let stop = st.tabs.stops.iter().map(|t| (t.pos + base, t.kind, t.dot_leader)).find(|&(p, _, _)| p > pos);
                            let (next, kind, leader) = stop.unwrap_or((pos + 600, TabKind::Left, false));
                            let next = next.min(rm_limit.max(pos));
                            let tcol = (next + COL_WPU / 2) / COL_WPU;
                            let kind = match c {
                                Code::Tab => kind,
                                Code::Fmt(f) => f.tab_kind().unwrap_or(kind),
                                _ => TabKind::Left,
                            };
                            // Math On: tab stops align numbers on the decimal point
                            let kind = if math_on && matches!(c, Code::Tab) { TabKind::Decimal } else { kind };
                            tab_kinds.push((i, kind));
                            if kind == TabKind::Left {
                                pos = next;
                                col = col.max(tcol);
                            } else {
                                let k = match kind {
                                    TabKind::Center => SegKind::Center,
                                    TabKind::Right => SegKind::Right,
                                    _ => SegKind::Decimal,
                                };
                                seg = Some(Seg {
                                    fc: cells.len(),
                                    fk: caret.len() - 1,
                                    start_pos: pos,
                                    start_col: col,
                                    kind: k,
                                    target: next,
                                    tcol,
                                    dec: None,
                                    leader: leader && matches!(c, Code::Tab),
                                    el: i,
                                });
                            }
                            if matches!(c, Code::Indent | Code::DblIndent) && indent_left.is_none_or(|l| l < next) {
                                indent_left = Some(next);
                                if matches!(c, Code::DblIndent) {
                                    indent_right = Some(next - lm.min(next));
                                }
                            }
                            last_break = Some((i + 1, cells.len(), caret.len() + 0, st.clone()));
                        }
                        Code::MarRel => {
                            let base = if st.tabs.relative { st.left } else { 0 };
                            let prev = st
                                .tabs
                                .stops
                                .iter()
                                .rev()
                                .map(|t| t.pos + base)
                                .find(|&p| p < pos)
                                .unwrap_or(0)
                                .max(0);
                            pos = prev;
                            col = prev / COL_WPU;
                        }
                        Code::GBox(b) if b.anchor == crate::gfx::Anchor::Char => {
                            gx.visit(i, c);
                            let (w, h, ncols, label) = gx.char_box(doc, i, b, m);
                            if pos + w > rm_limit && !cells.is_empty() {
                                if let Some((bi, cl, kl, snap)) = last_break.take() {
                                    cells.truncate(cl);
                                    caret.truncate(kl);
                                    st = snap;
                                    i = bi;
                                    term = LineEnd::Soft;
                                    break;
                                }
                            }
                            let lab: Vec<char> = format!("[{label}]").chars().collect();
                            for k in 0..ncols as usize {
                                let ch = if k + 1 == ncols as usize && ncols > 1 { ']' } else { lab.get(k).copied().unwrap_or(' ') };
                                cells.push(LCell { col: col + k as i32, ch, attrs: st.attrs, el: i });
                            }
                            pos += w;
                            col += ncols;
                            let desc = (st.font.size_cpt as i32 * 12 / 72) / 5;
                            max_h = max_h.max(h + desc);
                        }
                        Code::GBox(_) | Code::BoxOpt(_) | Code::BoxNewNum(..) => gx.visit(i, c),
                        Code::Center | Code::FlushRight | Code::FlushRightDot => {
                            if let Some(sg) = seg.take() {
                                finish_seg(&sg, &mut cells, &mut caret, &mut pos, &mut col);
                            }
                            let (k, target, tcol) = if matches!(c, Code::Center) {
                                (SegKind::Center, (lm + rm_limit) / 2, (lm_col + rm_col) / 2)
                            } else {
                                (SegKind::Right, rm_limit, rm_col)
                            };
                            let leader = matches!(c, Code::FlushRightDot);
                            seg = Some(Seg { fc: cells.len(), fk: caret.len() - 1, start_pos: pos, start_col: col, kind: k, target, tcol, dec: None, leader, el: i });
                        }
                        Code::Mark(Mark::Ref { text, .. }) => {
                            // the referenced number is shown in the text
                            let t = if text.is_empty() { "?" } else { text.as_str() };
                            let sz = size_for(st.attrs, st.font.size_cpt);
                            for d in t.chars() {
                                let w = m.width(&st.font, sz, d);
                                cells.push(LCell { col, ch: d, attrs: shown_attrs(&st), el: i });
                                pos += w;
                                col += 1;
                            }
                        }
                        Code::Merge(_) | Code::MathOp(_) => {
                            // merge codes / math operators show as text-like codes
                            let sz = size_for(st.attrs, st.font.size_cpt);
                            for ch in crate::merge::screen_text(c).chars() {
                                let w = m.width(&st.font, sz, ch);
                                cells.push(LCell { col, ch, attrs: st.attrs, el: i });
                                pos += w;
                                col += 1;
                            }
                            if matches!(c, Code::Merge(_)) {
                                last_break = Some((i + 1, cells.len(), caret.len(), st.clone()));
                            }
                        }
                        Code::Math(on) => math_on = *on,
                        other => apply_code(&mut st, other),
                    }
                    i += 1;
                }
            }
        }
        if i >= n && term == LineEnd::Eof {
            // cursor can sit at the very end
        }
        // caret entry for the end position
        caret.push((col, pos));
        let had_seg = seg.is_some();
        let line_just = first_just.unwrap_or(st.just);
        if let Some(sg) = seg.take() {
            // the end caret belongs to the segment too
            finish_seg(&sg, &mut cells, &mut caret, &mut pos, &mut col);
        }
        let shift_seg = |cells: &mut Vec<LCell>, caret: &mut Vec<(i32, i32)>, from_c: usize, from_k: usize, dpos: i32, dcol: i32| {
            for c in cells.iter_mut().skip(from_c) {
                c.col += dcol;
            }
            for k in caret.iter_mut().skip(from_k + 1) {
                k.0 += dcol;
                k.1 += dpos;
            }
        };
        if had_seg {
        } else if matches!(line_just, Justify::Center | Justify::Right) && !cells.is_empty() {
            // trailing spaces don't count
            let mut used_cols = 0;
            for c in cells.iter().rev() {
                if c.ch != ' ' {
                    used_cols = c.col - lm_col + 1;
                    break;
                }
            }
            let free = (rm_col - lm_col) - used_cols;
            if free > 0 {
                let d = if line_just == Justify::Center { free / 2 } else { free };
                shift_seg(&mut cells, &mut caret, 0, 0, d * COL_WPU, d);
                // shift caret[0] too
                if let Some(k) = caret.first_mut() {
                    k.0 += d;
                    k.1 += d * COL_WPU;
                }
            }
        }

        // Pagination
        // Format: Printer Functions — leading adjustment ([SRt] / [HRt] lines)
        let height = max_h
            + match term {
                LineEnd::Soft => st.fx.leading.0,
                LineEnd::Hard | LineEnd::HardPage => st.fx.leading.1,
                _ => 0,
            };
        if y + height > st.page_h - st.bottom && !lines.is_empty() && y > st.top {
            if let Some(prev) = lines.last_mut() {
                if prev.break_after.is_none() {
                    prev.break_after = Some(PageBreak::Soft);
                }
            }
            page += 1;
            y = st.top;
        }
        let line = Line {
            start: line_start,
            end: i,
            term,
            cells,
            caret,
            page,
            y,
            height,
            break_after: if term == LineEnd::HardPage { Some(PageBreak::Hard) } else { None },
            tab_kinds,
            right: rm_limit,
            hyph: hyph_end,
        };
        lines.push(line);
        {
            let li = lines.len() - 1;
            line_col.push(colrun.as_ref().map(|r| r.line_col()));
            let par = if tbl.is_none() { colrun.as_mut().filter(|r| r.parallel()) } else { None };
            let desc = (st.font.size_cpt as i32 * 12 / 72) / 5;
            gx.record_char_boxes(doc, &lines[li], li, desc, m);
            let v = match tbl.as_mut() {
                _ if par.is_some() => par.unwrap().next_vrow(),
                Some(t) if t.col.is_some() => {
                    let v = t.row_base + t.cell_lines;
                    t.cell_lines += 1;
                    t.row_max = t.row_max.max(t.cell_lines);
                    v
                }
                _ => {
                    let v = next_vrow;
                    next_vrow += 1;
                    v
                }
            };
            ensure(&mut vrows, v);
            if let VRow::Lines(ls) = &mut vrows[v] {
                ls.push(li);
            }
            line_vrow.push(v);
        }
        y += height;
        gx.consume(height, page);
        if tbl.is_none() {
            let cx = crate::gfx::Ctx { left: st.left, right: st.right, top: st.top, bottom: st.bottom, page_w: st.page_w, page_h: st.page_h, page, y, vrow: Some(next_vrow), line: lines.len() };
            gx.activate_range(doc, line_start..i, &cx, m);
        }
        if term == LineEnd::HardPage {
            page += 1;
            y = st.top;
            if let Some(r) = colrun.as_mut() {
                r.column_break();
            }
        }
        if matches!(term, LineEnd::Hard | LineEnd::HardPage) {
            indent_left = None;
            indent_right = None;
        }
        if i >= n && term != LineEnd::Hard && term != LineEnd::HardPage {
            if let Some(mut t) = tbl.take() {
                finish_row(&mut t, &mut next_vrow, &mut vrows, BorderKind::Bottom);
            }
            break;
        }
        if i >= n && matches!(term, LineEnd::Hard | LineEnd::HardPage) {
            // empty last line after a final HRt
            let (lm, rm) = colrun.as_mut().map(|r| r.edges(n, col_hints)).unwrap_or((st.left, st.page_w - st.right));
            let pos = lm;
            let col = lm / COL_WPU;
            if y + st_line_height(&st, st.font.size_cpt) > st.page_h - st.bottom && y > st.top {
                if let Some(prev) = lines.last_mut() {
                    if prev.break_after.is_none() {
                        prev.break_after = Some(PageBreak::Soft);
                    }
                }
                page += 1;
                y = st.top;
            }
            lines.push(Line {
                start: n,
                end: n,
                term: LineEnd::Eof,
                cells: vec![],
                caret: vec![(col, pos)],
                page,
                y,
                height: st_line_height(&st, st.font.size_cpt),
                break_after: None,
                tab_kinds: vec![],
                right: rm,
                hyph: false,
            });
            if let Some(mut t) = tbl.take() {
                finish_row(&mut t, &mut next_vrow, &mut vrows, BorderKind::Bottom);
            }
            line_col.push(colrun.as_ref().map(|r| r.line_col()));
            let v = match colrun.as_mut() {
                Some(r) if r.parallel() => r.next_vrow(),
                _ => next_vrow,
            };
            ensure(&mut vrows, v);
            if let VRow::Lines(ls) = &mut vrows[v] {
                ls.push(lines.len() - 1);
            }
            line_vrow.push(v);
            break;
        }
        // Safety: a line must consume at least one element.
        if i == line_start {
            i += 1;
        }
    }
    if lines.is_empty() {
        let col = st.left / COL_WPU;
        lines.push(Line {
            start: 0,
            end: 0,
            term: LineEnd::Eof,
            cells: vec![],
            caret: vec![(col, st.left)],
            page: 1,
            y: st.top,
            height: st_line_height(&st, st.font.size_cpt),
            break_after: None,
            tab_kinds: vec![],
            right: st.page_w - st.right,
            hyph: false,
        });
        vrows = vec![VRow::Lines(vec![0])];
        line_vrow = vec![0];
        line_col = vec![None];
    }
    let _ = page;
    vrows.retain(|v| !matches!(v, VRow::Lines(ls) if ls.is_empty()) || true);
    let l = Layout {
        pages: 1,
        lines,
        page_hf: vec![],
        page_notes: vec![],
        note_nums,
        vrows,
        tables,
        line_vrow,
        vrow_y: vec![],
        vrow_page: vec![],
        drows: vec![],
        vrow_drow: vec![],
        line_col,
        col_blocks,
        hyph_ask,
        page_paper: vec![],
        boxes: std::mem::take(&mut gx.boxes),
    };
    l
}

/// Total height of a block of nested text (note, header) laid out with the
/// document's page setup.
pub fn block_height(doc: &Document, els: &[El], m: &dyn Metrics) -> i32 {
    let mut d = Document::new();
    d.init = doc.init.clone();
    d.init.page_height = i32::MAX / 4;
    d.els = els.to_vec();
    let mut sub = layout_lines_only(&d, m);
    // the empty line after a trailing HRt does not count
    if sub.len() > 1 && sub.last().is_some_and(|l| l.start == l.end) {
        sub.pop();
    }
    sub.iter().map(|l| l.height).sum()
}

fn layout_lines_only(doc: &Document, m: &dyn Metrics) -> Vec<Line> {
    // Nested blocks never contain notes/headers, so a plain layout is fine;
    // avoid recursion through paginate by laying out a copy without them.
    let mut d = doc.clone();
    d.els.retain(|e| !matches!(e, El::Code(Code::Note { .. } | Code::HeadFoot { .. })));
    layout(&d, m).lines
}

/// Space between header/footer and body text (WP: 0.16").
pub const HF_GAP: i32 = 192;
/// Footnote separator: blank line + 2" rule.
pub const NOTE_SEP: i32 = 300;

/// Pass 2: assign pages and vertical positions (per visual row),
/// honouring T/B margins, headers, footers and footnotes.
fn paginate(doc: &Document, l: &mut Layout, m: &dyn Metrics) {
    // Widow/orphan protection, block protect and conditional end of page:
    // rows that must stay on the page of the row before them.  A pass that
    // would separate them reports where the page should break instead, and
    // pagination is redone with that break forced.
    let keep = crate::fmtlayout::keep_with_prev(doc, l);
    let mut forced: std::collections::HashSet<usize> = Default::default();
    for _ in 0..l.vrows.len() + 1 {
        match paginate_pass(doc, l, m, &keep, &forced) {
            Some(b) if forced.insert(b) => {}
            _ => break,
        }
    }
}

fn paginate_pass(doc: &Document, l: &mut Layout, m: &dyn Metrics, keep: &[bool], forced: &std::collections::HashSet<usize>) -> Option<usize> {
    let init = &doc.init;
    let mut paper: Option<crate::fmt::Paper> = None;
    let mut top = init.top;
    let mut bottom = init.bottom;
    for e in &doc.initial_codes {
        match e {
            El::Code(Code::TBMargin { top: t, bottom: b }) => {
                top = *t;
                bottom = *b;
            }
            El::Code(Code::Fmt(crate::fmt::Fmt::Paper(p))) => {
                if let Some(lb) = &p.labels {
                    top = lb.mt;
                    bottom = lb.mb;
                }
                paper = Some(p.clone())
            }
            _ => {}
        }
    }
    let mut page_h;
    let mut page_paper: Vec<Option<crate::fmt::Paper>> = vec![];
    type Hf = [Option<(usize, Occurrence, i32)>; 4];
    let mut hf: Hf = [None; 4];
    let mut hf_cache: std::collections::HashMap<usize, i32> = Default::default();
    let mut note_cache: std::collections::HashMap<usize, i32> = Default::default();
    let mut page: u32 = 1;
    let mut notes_h = 0;
    let mut page_hf: Vec<[Option<usize>; 4]> = vec![];
    let mut page_notes: Vec<Vec<usize>> = vec![vec![]];
    let body_top = |hf: &Hf, page: u32, top: i32| {
        let h = hf[..2].iter().flatten().filter(|x| x.1.applies(page)).map(|x| x.2 + HF_GAP).max().unwrap_or(0);
        top + h
    };
    let body_bottom = |hf: &Hf, page: u32, bottom: i32, page_h: i32| {
        let h = hf[2..].iter().flatten().filter(|x| x.1.applies(page)).map(|x| x.2 + HF_GAP).max().unwrap_or(0);
        page_h - bottom - h
    };
    let snap = |hf: &Hf| [hf[0].map(|x| x.0), hf[1].map(|x| x.0), hf[2].map(|x| x.0), hf[3].map(|x| x.0)];
    let scan_codes = |range: std::ops::Range<usize>,
                      hf: &mut Hf,
                      hf_cache: &mut std::collections::HashMap<usize, i32>,
                      top: &mut i32,
                      bottom: &mut i32,
                      paper: &mut Option<crate::fmt::Paper>| {
        for k in range {
            if let El::Code(c) = &doc.els[k] {
                match c {
                    Code::HeadFoot { which, occ, els } => {
                        let w = (*which & 3) as usize;
                        if *occ == Occurrence::Discontinue {
                            hf[w] = None;
                        } else {
                            let h = *hf_cache.entry(k).or_insert_with(|| block_height(doc, els, m));
                            hf[w] = Some((k, *occ, h));
                        }
                    }
                    Code::TBMargin { top: t, bottom: b } => {
                        *top = *t;
                        *bottom = *b;
                    }
                    Code::Fmt(crate::fmt::Fmt::Paper(p)) => {
                        if let Some(lb) = &p.labels {
                            *top = lb.mt;
                            *bottom = lb.mb;
                        }
                        *paper = Some(p.clone());
                    }
                    _ => {}
                }
            }
        }
    };
    let nv = l.vrows.len();
    let mut vrow_y = vec![0; nv];
    let mut vrow_page = vec![1; nv];
    let mut drows: Vec<DRow> = Vec::with_capacity(nv + 8);
    let mut vrow_drow = vec![0; nv];
    // codes on the first visual row apply to page 1
    if let Some(VRow::Lines(ls)) = l.vrows.first() {
        for &li in ls {
            let (s, e) = (l.lines[li].start, l.lines[li].end);
            scan_codes(s..e, &mut hf, &mut hf_cache, &mut top, &mut bottom, &mut paper);
        }
    }
    page_h = crate::fmtlayout::paper_height(init, &paper);
    let mut y = body_top(&hf, page, top);
    page_hf.push(snap(&hf));
    page_paper.push(paper.clone());
    let mut first_on_page = true;
    // newspaper columns being filled
    let mut flow: Option<crate::columns::Flow> = None;
    let mut page_first_v = 0usize;
    for v in 0..nv {
        let (height, hard_after, line_ids): (i32, bool, Vec<usize>) = match &l.vrows[v] {
            VRow::Lines(ls) => (
                ls.iter().map(|&li| l.lines[li].height).max().unwrap_or(0),
                // inside text columns [HPg] is a column break
                ls.iter().any(|&li| l.lines[li].term == LineEnd::HardPage && l.line_col.get(li).is_none_or(|c| c.is_none())),
                ls.clone(),
            ),
            VRow::Border { .. } => (BORDER_H, false, vec![]),
            VRow::Gap(h) => (*h, false, vec![]),
        };
        if v > 0 {
            for &li in &line_ids {
                let (s, e) = (l.lines[li].start, l.lines[li].end);
                scan_codes(s..e, &mut hf, &mut hf_cache, &mut top, &mut bottom, &mut paper);
            }
            if first_on_page {
                page_h = crate::fmtlayout::paper_height(init, &paper);
                y = body_top(&hf, page, top);
                page_hf[page as usize - 1] = snap(&hf);
                page_paper[page as usize - 1] = paper.clone();
            }
        }
        let mut row_notes = vec![];
        let mut row_notes_h = 0;
        for &li in &line_ids {
            for k in l.lines[li].start..l.lines[li].end {
                if let El::Code(Code::Note { kind: NoteKind::Foot, els }) = &doc.els[k] {
                    let h = *note_cache.entry(k).or_insert_with(|| block_height(doc, els, m) + 60);
                    row_notes.push(k);
                    row_notes_h += h;
                }
            }
        }
        let extra = if row_notes.is_empty() { 0 } else { row_notes_h + if notes_h == 0 { NOTE_SEP } else { 0 } };
        let limit = body_bottom(&hf, page, bottom, page_h) - notes_h - extra;
        let overflow = y + height > limit;
        if overflow && !first_on_page && flow.is_none() {
            // keep-together rows: break before the first of them instead
            let mut b = v;
            while b > page_first_v && keep.get(b).copied().unwrap_or(false) {
                b -= 1;
            }
            if b > page_first_v && b < v && !forced.contains(&b) {
                return Some(b);
            }
        }
        let mut brk = (overflow || forced.contains(&v)) && !first_on_page;
        crate::columns::paginate_row(l, &line_ids, &mut flow, &mut y, height, limit, first_on_page, &mut brk);
        if brk {
            drows.push(DRow::Break(PageBreak::Soft));
            // mark on the last line before this row (for Reveal Codes [SPg])
            if let Some(prev) = (0..v).rev().find_map(|pv| match &l.vrows[pv] {
                VRow::Lines(ls) => ls.iter().max().copied(),
                _ => None,
            }) {
                if l.lines[prev].break_after.is_none() {
                    l.lines[prev].break_after = Some(PageBreak::Soft);
                }
            }
            page += 1;
            page_notes.push(vec![]);
            notes_h = 0;
            page_h = crate::fmtlayout::paper_height(init, &paper);
            y = body_top(&hf, page, top);
            page_hf.push(snap(&hf));
            if let Some(f) = flow.as_mut() {
                f.new_page(y);
            }
            page_paper.push(paper.clone());
        }
        if first_on_page || vrow_page.get(v.wrapping_sub(1)).is_some_and(|p| *p != page) {
            page_first_v = v;
        }
        if !row_notes.is_empty() {
            if notes_h == 0 {
                notes_h += NOTE_SEP;
            }
            notes_h += row_notes_h;
            page_notes[page as usize - 1].extend(row_notes);
        }
        vrow_y[v] = y;
        vrow_page[v] = page;
        vrow_drow[v] = drows.len();
        drows.push(DRow::V(v));
        if COMMENTS_DISPLAY.load(std::sync::atomic::Ordering::Relaxed) {
            for &li in &line_ids {
                for k in l.lines[li].start..l.lines[li].end.min(doc.els.len()) {
                    if let El::Code(Code::Comment(t)) = &doc.els[k] {
                        let n = crate::comments::box_rows(doc, t);
                        drows.extend((0..n).map(|r| DRow::Comment { el: k, row: r as u16 }));
                    }
                }
            }
        }
        for &li in &line_ids {
            let line = &mut l.lines[li];
            line.page = page;
            line.y = y;
            line.break_after = if line.term == LineEnd::HardPage && !hard_after_col(&l.line_col, li) { Some(PageBreak::Hard) } else { None };
        }
        y += height;
        first_on_page = false;
        let col_page_break = crate::columns::placed(l, &line_ids, &mut flow, &mut y) && v + 1 < nv;
        if col_page_break {
            // [HPg] in the last newspaper column: next page
            drows.push(DRow::Break(PageBreak::Hard));
            page += 1;
            page_notes.push(vec![]);
            notes_h = 0;
            y = body_top(&hf, page, top);
            page_hf.push(snap(&hf));
            first_on_page = true;
            if let Some(f) = flow.as_mut() {
                f.new_page(y);
            }
        }
        if hard_after && v + 1 < nv {
            drows.push(DRow::Break(PageBreak::Hard));
            page += 1;
            page_notes.push(vec![]);
            notes_h = 0;
            page_h = crate::fmtlayout::paper_height(init, &paper);
            y = body_top(&hf, page, top);
            page_hf.push(snap(&hf));
            page_paper.push(paper.clone());
            first_on_page = true;
        }
    }
    l.pages = page;
    l.page_hf = page_hf;
    l.page_notes = page_notes;
    l.vrow_y = vrow_y;
    l.vrow_page = vrow_page;
    l.drows = drows;
    l.vrow_drow = vrow_drow;
    l.page_paper = page_paper;
    crate::columns::regroup(l);
    None
}


fn hard_after_col(line_col: &[Option<crate::columns::LineCol>], li: usize) -> bool {
    line_col.get(li).is_some_and(|c| c.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        let text = "word ".repeat(40);
        let d = Document::from_text(&text);
        let l = layout(&d, &MonoMetrics);
        // 6.5" at 10cpi = 65 chars per line; "word " = 5 → 13 words/line
        assert!(l.lines.len() >= 3);
        assert_eq!(l.lines[0].term, LineEnd::Soft);
        assert_eq!(l.lines[0].end - l.lines[0].start, 65);
        assert_eq!(l.lines[0].cells[0].col, 10);
    }

    #[test]
    fn hard_returns() {
        let d = Document::from_text("a\nb\n");
        let l = layout(&d, &MonoMetrics);
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0].term, LineEnd::Hard);
        assert_eq!(l.lines[2].term, LineEnd::Eof);
        assert_eq!(l.line_of(2), 1);
        assert_eq!(l.line_of(4), 2);
    }

    #[test]
    fn pages() {
        let d = Document::from_text(&"x\n".repeat(120));
        let l = layout(&d, &MonoMetrics);
        assert!(l.pages >= 3, "pages={}", l.pages);
        assert_eq!(l.lines[54].page, 2);
    }
}
