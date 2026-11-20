//! Physical page model: positioned text runs per page, computed from the
//! layout.  Used by View Document, printing and PDF.

use crate::doc::*;
use crate::layout::{Layout, LineEnd};

#[derive(Clone, Debug)]
pub struct Run {
    /// x of the first glyph, WPU from the left paper edge
    pub x: i32,
    /// baseline, WPU from the top paper edge
    pub y: i32,
    pub text: Vec<char>,
    /// advance of each char in WPU (incl. justification spacing)
    pub adv: Vec<i32>,
    pub font: String,
    /// size in centi-points after attribute scaling
    pub size_cpt: u32,
    pub attrs: AttrSet,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub number: u32,
    pub width: i32,
    pub height: i32,
    pub runs: Vec<Run>,
    /// Filled rectangles (x, y, w, h) in WPU — rules, footnote separator.
    pub rules: Vec<(i32, i32, i32, i32)>,
    /// Pictures and gray fills (graphics boxes), drawn below rules and text.
    pub images: Vec<crate::raster::Placed>,
}

/// Lay out nested text (header, footer, note) and return its runs placed
/// with their top at `y_top`, plus the height used.
pub fn nested_runs(doc: &Document, els: &[El], m: &dyn crate::layout::Metrics, y_top: i32) -> (Vec<Run>, i32) {
    let mut d = Document::new();
    d.init = doc.init.clone();
    d.init.top = 0;
    d.init.page_height = i32::MAX / 4;
    d.els = els.iter().filter(|e| !matches!(e, El::Code(Code::Note { .. } | Code::HeadFoot { .. }))).cloned().collect();
    let l = crate::layout::layout(&d, m);
    let mut h: i32 = l.lines.iter().map(|x| x.height).sum();
    if l.lines.len() > 1 && l.lines.last().is_some_and(|x| x.start == x.end) {
        h -= l.lines.last().unwrap().height;
    }
    let pages = build_pages(&d, &l, m);
    let mut runs = vec![];
    for p in pages {
        for mut r in p.runs {
            r.y += y_top;
            runs.push(r);
        }
    }
    (runs, h)
}

/// The document as printed: endnotes appended at the end.
pub fn print_document(doc: &Document, layout: &Layout) -> Option<Document> {
    let ends: Vec<(usize, &Vec<El>)> = doc
        .els
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            El::Code(Code::Note { kind: NoteKind::End, els }) => Some((i, els)),
            _ => None,
        })
        .collect();
    if ends.is_empty() {
        return None;
    }
    let mut d = doc.clone();
    if !matches!(d.els.last(), Some(El::Code(Code::HRt))) {
        d.els.push(El::Code(Code::HRt));
    }
    d.els.push(El::Code(Code::HRt));
    for (i, els) in ends {
        let n = layout.note_nums.get(&i).copied().unwrap_or(0);
        d.els.extend(format!("{n}. ").chars().map(El::Ch));
        d.els.extend(els.iter().cloned());
        d.els.push(El::Code(Code::HRt));
    }
    Some(d)
}

fn scaled(attrs: AttrSet, base: u32) -> u32 {
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

/// ^B in headers/footers prints the page number.
fn substitute_page_number(els: &[El], n: &str) -> Vec<El> {
    let mut v = vec![];
    for e in els {
        match e {
            El::Ch('\u{2}') => v.extend(n.chars().map(El::Ch)),
            other => v.push(other.clone()),
        }
    }
    v
}


pub fn build_pages(doc: &Document, layout: &Layout, m: &dyn crate::layout::Metrics) -> Vec<Page> {
    // styles and paragraph numbers print expanded
    if let Some(flat) = crate::styles::flat_document(doc) {
        let l = crate::layout::layout_flat(&flat, m);
        return build_pages_flat(&flat, &l, m);
    }
    build_pages_flat(doc, layout, m)
}

fn build_pages_flat(doc: &Document, layout: &Layout, m: &dyn crate::layout::Metrics) -> Vec<Page> {
    let init = &doc.init;
    let mut pages: Vec<Page> = Vec::new();
    // walk the document state alongside the lines
    let mut font = init.font.clone();
    let mut just = init.justify;
    let mut right = init.right;
    let mut left = init.left;
    let mut bottom = init.bottom;
    let mut pgnum_pos: u8 = 0;
    let mut page_offset: i64 = 0; // [Pg Num:n] adjustments
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            match c {
                Code::Font(f) => font = f.clone(),
                Code::Just(j) => just = *j,
                Code::LRMargin { left: l, right: r } => {
                    left = *l;
                    right = *r
                }
                Code::PageNumberPos(p) => pgnum_pos = *p,
                _ => {}
            }
        }
    }
    let mut attrs = AttrSet::default();
    let mut scan = 0usize; // next element whose code effects are applied
    // Format codes of fmt.rs (underline mode, justification limits, …)
    let infos = crate::fmt::page_infos(doc, layout);
    let mut fxs = crate::fmt::FmtState::default();
    for e in &doc.initial_codes {
        if let El::Code(Code::Fmt(f)) = e {
            fxs.apply(f);
        }
    }
    // Advance up/down/to line: vertical offset for the rest of the page
    let mut adv_dy = 0i32;
    let mut adv_page = 0u32;
    let page_size = |n: u32| crate::fmtlayout::paper_size(init, layout.page_paper.get(n as usize - 1).unwrap_or(&None));
    // per line: its page and the runs it gave (for repeated table headers)
    let mut line_runs: Vec<(usize, usize, usize)> = Vec::with_capacity(layout.lines.len());
    for line in &layout.lines {
        while pages.len() < line.page as usize {
            let n = pages.len() as u32 + 1;
            let (w, h) = page_size(n);
            pages.push(Page { number: n, width: w, height: h, runs: vec![], rules: vec![], images: vec![] });
        }
        if line.page != adv_page {
            adv_page = line.page;
            adv_dy = 0;
        }
        // apply codes up to the line start
        while scan < line.start {
            if let El::Code(c) = &doc.els[scan] {
                match c {
                    Code::Font(f) => font = f.clone(),
                    Code::Just(j) => just = *j,
                    Code::LRMargin { left: l, right: r } => {
                        left = *l;
                        right = *r
                    }
                    Code::TBMargin { bottom: b, .. } => bottom = *b,
                    Code::AttrOn(a) => attrs.set(*a, true),
                    Code::AttrOff(a) => attrs.set(*a, false),
                    Code::PageNumberPos(p) => pgnum_pos = *p,
                    Code::PageNumber(n) => page_offset = *n as i64 - line.page as i64,
                    Code::Fmt(f) => fxs.apply(f),
                    _ => {}
                }
            }
            scan += 1;
        }
        let page = &mut pages[line.page as usize - 1];
        let first_run = page.runs.len();
        let _ = (left, bottom);
        // baseline ≈ top + 0.8 × line height (single spacing component)
        let base_h = (font.size_cpt as i64 * 12 / 72) as i32;
        let mut baseline = (if crate::gfx::line_has_char_box(doc, line) { line.y + line.height - base_h / 5 } else { line.y + base_h * 4 / 5 }) + adv_dy;
        // full justification: distribute slack over spaces on soft lines
        let rm = line.right;
        let _ = right;
        let mut extra_per_space = 0f64;
        // lines in visual (right-to-left) order: glyphs are placed one by one
        let rtl_line = crate::bidi::line_has_rtl(&line.cells);
        // Japanese paragraph? (decided on the first CJK character)
        let mut line_jp: Option<bool> = None;
        if just == Justify::Full && line.term == LineEnd::Soft && !rtl_line {
            let last_nonspace = line.cells.iter().rposition(|c| c.ch != ' ');
            if let Some(ln) = last_nonspace {
                let last = &line.cells[ln];
                let (_, xpos) = line.caret_of(last.el);
                let end = xpos + m.width(&font, scaled(last.attrs, font.size_cpt), last.ch);
                let spaces = line.cells[..ln].iter().filter(|c| c.ch == ' ').count();
                if spaces > 0 && end < rm {
                    extra_per_space = (rm - end) as f64 / spaces as f64;
                    // [Just Lim]: spaces stretch at most to expand %
                    let (_, expand) = fxs.just_lim;
                    if expand > 100 {
                        let sw = m.width(&font, font.size_cpt, ' ') as f64;
                        extra_per_space = extra_per_space.min(sw * (expand as f64 - 100.0) / 100.0);
                    }
                }
            }
        }
        // cells → runs (attribute changes inside the line come from codes)
        let mut cur_attrs = attrs;
        let mut cur_font = font.clone();
        let mut run: Option<Run> = None;
        let mut acc = 0f64;
        let mut k = line.start;
        let mut ci = 0usize;
        while k < line.end {
            match &doc.els[k] {
                El::Code(c) => {
                    match c {
                        Code::AttrOn(a) => cur_attrs.set(*a, true),
                        Code::AttrOff(a) => cur_attrs.set(*a, false),
                        Code::Font(f) => cur_font = f.clone(),
                        Code::Fmt(crate::fmt::Fmt::Adv(a, v)) => {
                            let d = match a {
                                crate::fmt::Adv::Up => -v,
                                crate::fmt::Adv::Down => *v,
                                crate::fmt::Adv::ToLine => *v - line.y,
                                _ => 0,
                            };
                            if d != 0 {
                                if let Some(r) = run.take() {
                                    page.runs.push(r);
                                }
                                adv_dy += d;
                                baseline += d;
                            }
                        }
                        Code::Fmt(f) => fxs.apply(f),
                        Code::Tab | Code::Indent | Code::DblIndent if fxs.undrln.1 && (cur_attrs.has(Attr::Underline) || cur_attrs.has(Attr::DblUnderline)) => {
                            // Format: Other, Underline Tabs
                            let (_, x0) = line.caret_of(k);
                            let (_, x1) = line.caret_of(k + 1);
                            if x1 > x0 {
                                let th = (cur_font.size_cpt as i32 / 120).max(6);
                                page.rules.push((x0 + acc.round() as i32, baseline + base_h / 8, x1 - x0, th));
                            }
                        }
                        _ => {}
                    }
                    if let Code::Fmt(crate::fmt::Fmt::InsertPgNum(style)) = c {
                        // the page number as it prints on this page
                        if let Some(r) = run.take() {
                            page.runs.push(r);
                        }
                        let num = infos.get(line.page as usize - 1).map(|i| i.num_text()).unwrap_or_default();
                        let text: Vec<char> = crate::fmt::fill_style(style, &num).chars().collect();
                        let size = scaled(cur_attrs, cur_font.size_cpt);
                        let adv: Vec<i32> = text.iter().map(|ch| m.width(&cur_font, size, *ch)).collect();
                        let (_, xpos) = line.caret_of(k);
                        page.runs.push(Run { x: xpos + acc.round() as i32, y: baseline, text, adv, font: cur_font.name.clone(), size_cpt: size, attrs: cur_attrs });
                        k += 1;
                        continue;
                    }
                    if let Code::Note { .. } = c {
                        // reference number: superscript digits right here
                        if let Some(r) = run.take() {
                            page.runs.push(r);
                        }
                        let cells: Vec<_> = line.cells.iter().filter(|x| x.el == k).collect();
                        if let Some(first) = cells.first() {
                            let mut a = cur_attrs;
                            a.set(Attr::Superscript, true);
                            let size = scaled(a, cur_font.size_cpt);
                            let (_, xpos) = line.caret_of(k);
                            let text: Vec<char> = cells.iter().map(|x| x.ch).collect();
                            let adv: Vec<i32> = text.iter().map(|ch| m.width(&cur_font, size, *ch)).collect();
                            let _ = first;
                            page.runs.push(Run { x: xpos + acc.round() as i32, y: baseline - base_h / 3, text, adv, font: cur_font.name.clone(), size_cpt: size, attrs: a });
                        }
                        k += 1;
                        continue;
                    }
                    if matches!(c, Code::Mark(Mark::Ref { .. }) | Code::FlushRightDot | Code::Tab) {
                        // cross-reference values and dot leaders
                        if let Some(r) = run.take() {
                            page.runs.push(r);
                        }
                        let cells: Vec<_> = line.cells.iter().filter(|x| x.el == k).collect();
                        if let Some(first) = cells.first() {
                            let a = first.attrs;
                            let size = scaled(a, cur_font.size_cpt);
                            let (_, x0) = line.caret_of(k);
                            if matches!(c, Code::Mark(_)) {
                                let text: Vec<char> = cells.iter().map(|x| x.ch).collect();
                                let adv: Vec<i32> = text.iter().map(|ch| m.width(&cur_font, size, *ch)).collect();
                                page.runs.push(Run { x: x0 + acc.round() as i32, y: baseline, text, adv, font: cur_font.name.clone(), size_cpt: size, attrs: a });
                            } else {
                                let (_, x1) = line.caret_of(k + 1);
                                let w = m.width(&cur_font, size, '.').max(1);
                                let gap = m.width(&cur_font, size, ' ');
                                let n = ((x1 - x0 - 2 * gap) / w).max(0) as usize;
                                if n > 0 {
                                    let x = x1 - gap - n as i32 * w + acc.round() as i32;
                                    page.runs.push(Run { x, y: baseline, text: vec!['.'; n], adv: vec![w; n], font: cur_font.name.clone(), size_cpt: size, attrs: a });
                                }
                            }
                        }
                        k += 1;
                        continue;
                    }
                    if !matches!(c, Code::HardSpace | Code::HardHyphen | Code::Fmt(crate::fmt::Fmt::Ovrstk(_))) {
                        k += 1;
                        if let Some(r) = run.take() {
                            page.runs.push(r);
                        }
                        continue;
                    }
                }
                El::Ch(_) => {}
            }
            // a visible element: find its cell
            while ci < line.cells.len() && line.cells[ci].el < k {
                ci += 1;
            }
            if ci >= line.cells.len() || line.cells[ci].el != k {
                k += 1;
                continue;
            }
            let cell = &line.cells[ci];
            // displayed attributes (include tracked-change marks)
            let mut cur_attrs = cell.attrs;
            if cell.ch == ' ' && !fxs.undrln.0 {
                // Format: Other, Underline Spaces = No
                cur_attrs.set(Attr::Underline, false);
                cur_attrs.set(Attr::DblUnderline, false);
            }
            let (_, xpos) = line.caret_of(k);
            let size = scaled(cur_attrs, cur_font.size_cpt);
            let w = m.width(&cur_font, size, cell.ch);
            // CJK characters the font lacks: a Windows CJK font
            let run_font = if crate::cjk::is_cjk(cell.ch) {
                let jp = *line_jp.get_or_insert_with(|| crate::cjk::para_japanese(&doc.els, line.start));
                crate::fonts::font_for_char(&cur_font.name, cell.ch, jp)
            } else {
                cur_font.name.clone()
            };
            let mut x = xpos + acc.round() as i32;
            if cell.ch == ' ' && extra_per_space > 0.0 {
                acc += extra_per_space;
            }
            let dy = if cur_attrs.has(Attr::Superscript) {
                -(base_h / 3)
            } else if cur_attrs.has(Attr::Subscript) {
                base_h / 6
            } else {
                0
            };
            let same = run.as_ref().is_some_and(|r| {
                r.attrs == cur_attrs
                    && r.font == run_font
                    && r.size_cpt == size
                    && r.y == baseline + dy
                    && (!rtl_line || (r.x + r.adv.iter().sum::<i32>() - x).abs() <= 2)
            });
            if !same {
                if let Some(r) = run.take() {
                    page.runs.push(r);
                }
                run = Some(Run { x, y: baseline + dy, text: vec![], adv: vec![], font: run_font, size_cpt: size, attrs: cur_attrs });
            } else if let Some(r) = run.as_mut() {
                // keep x continuity: the previous advance absorbs rounding
                let expected = r.x + r.adv.iter().sum::<i32>();
                if let Some(last) = r.adv.last_mut() {
                    *last += x - expected;
                }
                x = expected;
                let _ = x;
            }
            let r = run.as_mut().unwrap();
            let extra = if cell.ch == ' ' && extra_per_space > 0.0 { extra_per_space.round() as i32 } else { 0 };
            r.text.push(cell.ch);
            r.adv.push(w + extra);
            k += 1;
        }
        if line.hyph {
            // hyphenated word: print the hyphen after the last character
            let size = scaled(cur_attrs, cur_font.size_cpt);
            let hw = m.width(&cur_font, size, '-');
            match run.as_mut() {
                Some(r) => {
                    r.text.push('-');
                    r.adv.push(hw);
                }
                None => {
                    let x = line.caret_of(line.end).1 + acc.round() as i32;
                    page.runs.push(Run { x, y: baseline, text: vec!['-'], adv: vec![hw], font: cur_font.name.clone(), size_cpt: size, attrs: cur_attrs });
                }
            }
        }
        if let Some(r) = run.take() {
            page.runs.push(r);
        }
        line_runs.push((line.page as usize - 1, first_run, page.runs.len()));
        attrs = cur_attrs;
        font = cur_font;
        scan = scan.max(line.end);
    }
    table_headers(layout, &mut pages, &line_runs);
    if pages.is_empty() {
        let (w, h) = page_size(1);
        pages.push(Page { number: 1, width: w, height: h, runs: vec![], rules: vec![], images: vec![] });
    }
    // Center Page (top to bottom), line numbering, redline method
    crate::fmtpage::body_effects(doc, layout, &infos, &mut pages, m);
    table_lines(layout, &mut pages);
    crate::gfx::draw(doc, layout, &mut pages, m);
    // headers, footers and footnotes
    for p in pages.iter_mut() {
        let pi = p.number as usize - 1;
        if let Some(hf) = layout.page_hf.get(pi) {
            for (w, idx) in hf.iter().enumerate() {
                let Some(idx) = idx else { continue };
                if let Some(El::Code(Code::HeadFoot { occ, els, .. })) = doc.els.get(*idx) {
                    if !occ.applies(p.number) {
                        continue;
                    }
                    let info = infos.get(pi).cloned().unwrap_or_default();
                    if info.suppress & (4 << w) != 0 {
                        continue;
                    }
                    let els = substitute_page_number(els, &info.num_text());
                    if w < 2 {
                        let (runs, _) = nested_runs(doc, &els, m, init.top);
                        p.runs.extend(runs);
                    } else {
                        let h = crate::layout::block_height(doc, &els, m);
                        let (runs, _) = nested_runs(doc, &els, m, p.height - init.bottom - h);
                        p.runs.extend(runs);
                    }
                }
            }
        }
        if let Some(notes) = layout.page_notes.get(pi) {
            if !notes.is_empty() {
                // footer space
                let footer_h = layout
                    .page_hf
                    .get(pi)
                    .map(|hf| {
                        hf[2..]
                            .iter()
                            .flatten()
                            .filter_map(|i| match doc.els.get(*i) {
                                Some(El::Code(Code::HeadFoot { occ, els, .. })) if occ.applies(p.number) => {
                                    Some(crate::layout::block_height(doc, els, m) + crate::layout::HF_GAP)
                                }
                                _ => None,
                            })
                            .max()
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);
                let mut blocks = vec![];
                let mut total = 0;
                for idx in notes {
                    if let Some(El::Code(Code::Note { els, .. })) = doc.els.get(*idx) {
                        let n = layout.note_nums.get(idx).copied().unwrap_or(0);
                        let mut v = vec![El::Code(Code::Tab), El::Code(Code::AttrOn(Attr::Superscript))];
                        v.extend(n.to_string().chars().map(El::Ch));
                        v.push(El::Code(Code::AttrOff(Attr::Superscript)));
                        v.push(El::Ch(' '));
                        v.extend(els.iter().cloned());
                        let h = crate::layout::block_height(doc, &v, m) + 60;
                        total += h;
                        blocks.push((v, h));
                    }
                }
                let mut y = p.height - init.bottom - footer_h - total;
                p.rules.push((init.left, y - crate::layout::NOTE_SEP / 2, 2400, 8));
                for (v, h) in blocks {
                    let (runs, _) = nested_runs(doc, &v, m, y);
                    p.runs.extend(runs);
                    y += h;
                }
            }
        }
    }
    // page numbers ([Pg Numbering], [Pg Num Style], [Suppress])
    let _ = (pgnum_pos, page_offset);
    crate::fmtpage::page_numbers(doc, layout, &infos, &mut pages, m, &font);
    pages
}

// ---------------------------------------------------------------- tables

/// Is this repeat of header rows still what the layout holds?  (Text
/// columns regroup the visual rows after pagination.)
fn repeat_ok(layout: &Layout, rp: &crate::layout::TblRepeat) -> bool {
    use crate::layout::{BorderKind, VRow};
    rp.last_v < layout.vrows.len()
        && rp.last_v < layout.vrow_y.len()
        && matches!(layout.vrows.get(rp.first_v), Some(VRow::Border { table, kind: BorderKind::Top, .. }) if *table == rp.table)
}

/// The text of table header rows, printed again on the pages their table
/// continues on (Table Edit, 4 Header).
fn table_headers(layout: &Layout, pages: &mut [Page], line_runs: &[(usize, usize, usize)]) {
    use crate::layout::VRow;
    for rp in layout.tbl_repeats.iter().filter(|rp| repeat_ok(layout, rp)) {
        let dy = rp.y - layout.vrow_y[rp.first_v];
        let mut copies: Vec<Run> = vec![];
        for v in rp.first_v..=rp.last_v {
            let VRow::Lines(ls) = &layout.vrows[v] else { continue };
            for &li in ls {
                let Some(&(pg, a, b)) = line_runs.get(li) else { continue };
                let Some(p) = pages.get(pg) else { continue };
                copies.extend(p.runs[a.min(p.runs.len())..b.min(p.runs.len())].iter().cloned().map(|mut r| {
                    r.y += dy;
                    r
                }));
            }
        }
        if let Some(p) = pages.get_mut(rp.page as usize - 1) {
            p.runs.extend(copies);
        }
    }
}

/// Stroke width and, for a double line, how far its two strokes are from
/// the middle (WPU).
fn rule_strokes(style: LineStyle) -> (i32, &'static [i32]) {
    match style {
        LineStyle::None => (0, &[]),
        LineStyle::Single | LineStyle::Dashed => (6, &[0]),
        LineStyle::Dotted => (8, &[0]),
        LineStyle::Double => (6, &[-6, 6]),
        LineStyle::Thick => (30, &[0]),
        LineStyle::ExtraThick => (54, &[0]),
    }
}

/// Half of the room a line takes across.
fn rule_half(style: LineStyle) -> i32 {
    let (w, offs) = rule_strokes(style);
    offs.iter().map(|o| o.abs()).max().unwrap_or(0) + w / 2
}

/// The lines that meet one end of a table line: the one straight ahead
/// and the two at right angles (above / below a horizontal line, left /
/// right of a vertical one).
#[derive(Clone, Copy)]
struct Meet {
    opp: LineStyle,
    before: LineStyle,
    after: LineStyle,
}

const NO_MEET: Meet = Meet { opp: LineStyle::None, before: LineStyle::None, after: LineStyle::None };

/// One table line as filled rectangles: from `from` to `to` along its
/// direction, centred on `mid` across.  The ends are fitted to the lines
/// they meet (corners close; lines stop at a double line they run into).
fn table_rule(p: &mut Page, horizontal: bool, style: LineStyle, mid: i32, from: i32, to: i32, m0: Meet, m1: Meet) {
    let (w, offs) = rule_strokes(style);
    let double = style == LineStyle::Double;
    // how far a stroke on side `off` is pulled back from the meeting point
    let trim = |m: &Meet, off: i32| -> i32 {
        let (a, b) = if off < 0 { (m.before, m.after) } else { (m.after, m.before) };
        match crate::grid::rule_end(double, m.opp != LineStyle::None, a == LineStyle::Double, b == LineStyle::Double) {
            1 => 3,
            -1 => -rule_half(LineStyle::Double),
            // an end without a line straight ahead reaches under the
            // lines it meets
            _ if m.opp == LineStyle::None => -[m.before, m.after].iter().filter(|s| **s != LineStyle::Double).map(|s| rule_half(*s)).max().unwrap_or(0),
            _ => 0,
        }
    };
    for &off in offs {
        let (a, b) = (from + trim(&m0, off), to - trim(&m1, off));
        let across = mid + off - w / 2;
        let mut piece = |a: i32, b: i32| {
            if b > a {
                p.rules.push(if horizontal { (a, across, b - a, w) } else { (across, a, w, b - a) });
            }
        };
        // (on, period) of dashes and dots
        let pattern = match style {
            LineStyle::Dashed => Some((60, 100)),
            LineStyle::Dotted => Some((8, 24)),
            _ => None,
        };
        match pattern {
            None => piece(a, b),
            Some((on, period)) => {
                let mut x = a;
                while x < b {
                    piece(x, (x + on).min(b));
                    x += period;
                }
            }
        }
    }
}

/// Table lines and shading, and the lines of repeated header rows.
fn table_lines(layout: &Layout, pages: &mut [Page]) {
    use crate::layout::{BorderKind, TableGeom, VRow};
    let none = LineStyle::None;
    // the border in front of row `b` (0 = top of the table) at height `y`
    let border = |pages: &mut [Page], geom: &TableGeom, pg: usize, b: usize, y: i32| {
        let Some(p) = pages.get_mut(pg - 1) else { return };
        let ncols = geom.x.len() - 1;
        let nrows = geom.rows.len();
        let h = |c: usize| geom.line_above(b, c);
        // the lines above and below the border at boundary k
        let up = |k: usize| if b > 0 { geom.style_v(b - 1, k) } else { none };
        let down = |k: usize| if b < nrows { geom.style_v(b, k) } else { none };
        for c in 0..ncols {
            let m0 = Meet { opp: if c > 0 { h(c - 1) } else { none }, before: up(c), after: down(c) };
            let m1 = Meet { opp: if c + 1 < ncols { h(c + 1) } else { none }, before: up(c + 1), after: down(c + 1) };
            table_rule(p, true, h(c), y, geom.x[c], geom.x[c + 1], m0, m1);
        }
    };
    // row `r` between `y0` and `y1`: its shading and the lines beside its
    // cells; `b0` / `b1`: the borders it reaches from and to (None: the
    // row is cut off by the page)
    let span = |pages: &mut [Page], geom: &TableGeom, pg: usize, r: usize, y0: i32, y1: i32, b0: Option<usize>, b1: Option<usize>| {
        let Some(p) = pages.get_mut(pg - 1) else { return };
        if y1 <= y0 {
            return;
        }
        let ncols = geom.x.len() - 1;
        let nrows = geom.rows.len();
        if geom.gray > 0 {
            let color = (geom.gray.min(100) as u32 * 255 / 100) << 24;
            let mut c = 0;
            while c < ncols {
                // (cells side by side as one area)
                let c0 = c;
                while c < ncols && geom.shaded(r, c) {
                    c += 1;
                }
                if c > c0 {
                    p.images.push(crate::raster::Placed::solid(geom.x[c0], y0, geom.x[c] - geom.x[c0], y1 - y0, color));
                } else {
                    c += 1;
                }
            }
        }
        let meet = |b: Option<usize>, k: usize, top: bool| match b {
            None => NO_MEET,
            Some(b) => Meet {
                opp: if top {
                    if b > 0 { geom.style_v(b - 1, k) } else { none }
                } else if b < nrows {
                    geom.style_v(b, k)
                } else {
                    none
                },
                before: if k > 0 { geom.line_above(b, k - 1) } else { none },
                after: if k < ncols { geom.line_above(b, k) } else { none },
            },
        };
        for k in 0..=ncols {
            table_rule(p, false, geom.style_v(r, k), geom.x[k], y0, y1, meet(b0, k, true), meet(b1, k, false));
        }
    };
    /// The table row being drawn.
    struct Open {
        table: usize,
        row: usize,
        page: usize,
        y0: i32,
        /// the border at `y0` (None: top of a page)
        b0: Option<usize>,
        /// text of the row stands on this page
        lines: bool,
    }
    let mut cur: Option<Open> = None;
    let mut prev_bottom = 0;
    for (v, vr) in layout.vrows.iter().enumerate() {
        let pg = layout.vrow_page.get(v).copied().unwrap_or(1) as usize;
        let y = layout.vrow_y.get(v).copied().unwrap_or(0);
        if matches!(vr, VRow::Gap(_)) {
            continue;
        }
        let at_border = matches!(vr, VRow::Border { .. });
        // the table goes on on another page
        if let Some(o) = cur.take_if(|o| o.page != pg) {
            let geom = &layout.tables[o.table];
            if o.lines {
                span(pages, geom, o.page, o.row, o.y0, prev_bottom, o.b0, None);
            }
            // the header rows again
            let mut next = (!at_border).then_some(Open { table: o.table, row: o.row, page: pg, y0: y, b0: None, lines: false });
            if let Some(rp) = layout.tbl_repeats.iter().find(|rp| rp.table == o.table && rp.page as usize == pg && repeat_ok(layout, rp)) {
                let dy = rp.y - layout.vrow_y[rp.first_v];
                let mut head: Option<Open> = None;
                for hv in rp.first_v..=rp.last_v {
                    if let VRow::Border { kind, row: b, .. } = &layout.vrows[hv] {
                        let hy = layout.vrow_y[hv] + geom.rule_offset(*kind) + dy;
                        if let Some(h) = head.take() {
                            span(pages, geom, pg, h.row, h.y0, hy, h.b0, Some(*b));
                        }
                        border(pages, geom, pg, *b, hy);
                        head = Some(Open { table: o.table, row: *b, page: pg, y0: hy, b0: Some(*b), lines: false });
                    }
                }
                if let Some(h) = head {
                    // text follows: the row that was cut goes on below the
                    // last header line; a line follows: it closes the
                    // last header row
                    next = Some(if at_border { h } else { Open { row: o.row, ..h } });
                }
            }
            cur = next;
        }
        match vr {
            VRow::Border { table, kind, row } => {
                let geom = &layout.tables[*table];
                let yy = y + geom.rule_offset(*kind);
                if let Some(o) = cur.take() {
                    if o.table == *table {
                        span(pages, geom, pg, o.row, o.y0, yy, o.b0, Some(*row));
                    }
                }
                border(pages, geom, pg, *row, yy);
                if *kind != BorderKind::Bottom {
                    cur = Some(Open { table: *table, row: *row, page: pg, y0: yy, b0: Some(*row), lines: false });
                }
                prev_bottom = y + geom.border_height(*kind);
            }
            VRow::Lines(ls) => {
                if let Some(o) = cur.as_mut() {
                    o.lines = true;
                }
                prev_bottom = y + ls.iter().filter_map(|&li| layout.lines.get(li)).map(|l| l.height).max().unwrap_or(0);
            }
            VRow::Gap(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meet(opp: LineStyle, before: LineStyle, after: LineStyle) -> Meet {
        Meet { opp, before, after }
    }

    #[test]
    fn table_rules() {
        use LineStyle::{Dashed, Dotted, Double, ExtraThick, Single, Thick};
        let none = LineStyle::None;
        let draw = |horizontal, style, m0, m1| {
            let mut p = Page::default();
            table_rule(&mut p, horizontal, style, 1000, 2000, 3000, m0, m1);
            p.rules
        };
        // no line, a thin one, the thick ones (centred on 1000)
        assert!(draw(true, none, NO_MEET, NO_MEET).is_empty());
        assert_eq!(draw(true, Single, NO_MEET, NO_MEET), vec![(2000, 997, 1000, 6)]);
        assert_eq!(draw(false, Thick, NO_MEET, NO_MEET), vec![(985, 2000, 30, 1000)]);
        assert_eq!(draw(true, ExtraThick, NO_MEET, NO_MEET), vec![(2000, 973, 1000, 54)]);
        // double: two thin rules
        assert_eq!(draw(true, Double, NO_MEET, NO_MEET), vec![(2000, 991, 1000, 6), (2000, 1003, 1000, 6)]);
        // dashes and dots end with the line
        let dashes = draw(true, Dashed, NO_MEET, NO_MEET);
        assert_eq!(dashes.len(), 10);
        assert_eq!((dashes[0], dashes[9]), ((2000, 997, 60, 6), (2900, 997, 60, 6)));
        let dots = draw(false, Dotted, NO_MEET, NO_MEET);
        assert_eq!(dots.len(), 42);
        assert!(dots.iter().all(|r| r.1 + r.3 <= 3000 && r.2 == 8));
        // the top left corner of a double frame: the outer stroke goes
        // round the outside, the inner one starts at the inner stroke of
        // the line going down
        let corner = meet(none, none, Double);
        assert_eq!(draw(true, Double, corner, NO_MEET), vec![(1991, 991, 1009, 6), (2003, 1003, 997, 6)]);
        // a single line running into a double one stops at its near stroke
        // (both ends), one that goes on straight is not cut
        let into = meet(none, Double, Double);
        assert_eq!(draw(true, Single, into, into), vec![(2003, 997, 994, 6)]);
        let through = meet(Single, Double, Double);
        assert_eq!(draw(true, Single, through, NO_MEET), vec![(2000, 997, 1000, 6)]);
        // a corner of single lines closes; so does one with a thick line
        assert_eq!(draw(true, Single, meet(none, none, Single), NO_MEET), vec![(1997, 997, 1003, 6)]);
        assert_eq!(draw(true, Single, meet(none, Thick, none), NO_MEET), vec![(1985, 997, 1015, 6)]);
    }
}
