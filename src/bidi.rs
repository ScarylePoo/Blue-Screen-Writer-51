//! Right-to-left text (Arabic, Hebrew) on screen, in View Document and in
//! print.
//!
//! The document always stays in *logical* order (the order it was typed and
//! the order Reveal Codes shows).  When a line is laid out and contains RTL
//! characters, this module
//!
//! 1. gives every Arabic letter its contextual form (isolated, initial,
//!    medial, final — Unicode Arabic Presentation Forms) and joins lam + alef
//!    into the mandatory ligature, so a character grid and per-glyph printing
//!    show connected script;
//! 2. runs the Unicode Bidirectional Algorithm (UAX #9) on the line and puts
//!    the cells into *visual* order (screen columns and WPU positions), with
//!    brackets mirrored inside RTL runs;
//! 3. mirrors whole lines of a paragraph whose first strong letter is RTL,
//!    so they start at the right margin.
//!
//! Lines without RTL characters are not touched at all.

use crate::doc::*;
use crate::layout::LCell;
use unicode_bidi::{BidiInfo, Level};

/// Hebrew, Arabic (incl. supplements, extended) and their presentation forms.
pub fn is_rtl_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

pub fn has_rtl(s: &str) -> bool {
    s.chars().any(is_rtl_char)
}

/// Strong direction of a letter: Some(true) = RTL, Some(false) = LTR,
/// None = neutral (digits, punctuation, spaces, marks).
fn strong(c: char) -> Option<bool> {
    if is_rtl_char(c) {
        // Arabic-Indic digits and marks are not strong
        let u = c as u32;
        if (0x0660..=0x0669).contains(&u) || (0x06F0..=0x06F9).contains(&u) || is_transparent(c) {
            return None;
        }
        return Some(true);
    }
    if c.is_alphabetic() { Some(false) } else { None }
}

/// Direction of the paragraph containing element `at`: its first strong
/// letter decides (UAX #9 rules P2/P3), like Notepad and text editors.
pub fn para_is_rtl(els: &[El], at: usize) -> bool {
    let at = at.min(els.len());
    let mut s = at;
    while s > 0 && !is_para_break(&els[s - 1]) {
        s -= 1;
    }
    for e in &els[s..] {
        match e {
            El::Ch(c) => {
                if let Some(d) = strong(*c) {
                    return d;
                }
            }
            e if is_para_break(e) => return false,
            _ => {}
        }
    }
    false
}

fn is_para_break(e: &El) -> bool {
    matches!(e, El::Code(Code::HRt | Code::HPg | Code::Cell | Code::Row | Code::TblDef { .. } | Code::TblOff))
}

// ------------------------------------------------------------- shaping

#[derive(Clone, Copy, PartialEq)]
enum Join {
    /// joins on both sides
    Dual,
    /// joins only to the previous (right-hand) letter
    Right,
    /// tatweel: joins on both sides, has no forms
    Causing,
}

/// (joining type, isolated, final, initial, medial) — initial/medial are 0
/// for right-joining letters.
fn forms(c: char) -> Option<(Join, u32, u32, u32, u32)> {
    use Join::*;
    let u = c as u32;
    let r = |iso: u32| Some((Right, iso, iso + 1, 0, 0));
    let d = |iso: u32| Some((Dual, iso, iso + 1, iso + 2, iso + 3));
    match u {
        0x0622 => r(0xFE81),
        0x0623 => r(0xFE83),
        0x0624 => r(0xFE85),
        0x0625 => r(0xFE87),
        0x0626 => d(0xFE89),
        0x0627 => r(0xFE8D),
        0x0628 => d(0xFE8F),
        0x0629 => r(0xFE93),
        0x062A => d(0xFE95),
        0x062B => d(0xFE99),
        0x062C => d(0xFE9D),
        0x062D => d(0xFEA1),
        0x062E => d(0xFEA5),
        0x062F => r(0xFEA9),
        0x0630 => r(0xFEAB),
        0x0631 => r(0xFEAD),
        0x0632 => r(0xFEAF),
        0x0633 => d(0xFEB1),
        0x0634 => d(0xFEB5),
        0x0635 => d(0xFEB9),
        0x0636 => d(0xFEBD),
        0x0637 => d(0xFEC1),
        0x0638 => d(0xFEC5),
        0x0639 => d(0xFEC9),
        0x063A => d(0xFECD),
        0x0640 => Some((Causing, 0x0640, 0x0640, 0x0640, 0x0640)),
        0x0641 => d(0xFED1),
        0x0642 => d(0xFED5),
        0x0643 => d(0xFED9),
        0x0644 => d(0xFEDD),
        0x0645 => d(0xFEE1),
        0x0646 => d(0xFEE5),
        0x0647 => d(0xFEE9),
        0x0648 => r(0xFEED),
        0x0649 => r(0xFEEF),
        0x064A => d(0xFEF1),
        // Persian / Urdu letters (Presentation Forms-A)
        0x067E => d(0xFB56),
        0x0686 => d(0xFB7A),
        0x0698 => r(0xFB8A),
        0x06A9 => d(0xFB8E),
        0x06AF => d(0xFB92),
        0x06CC => d(0xFBFC),
        _ => None,
    }
}

/// Combining marks (harakat etc.): invisible to joining.
fn is_transparent(c: char) -> bool {
    matches!(c as u32, 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7 | 0x06E8 | 0x06EA..=0x06ED)
}

/// lam + (alef variant) → (isolated ligature, final ligature)
fn lam_alef(next: char) -> Option<(u32, u32)> {
    match next as u32 {
        0x0622 => Some((0xFEF5, 0xFEF6)),
        0x0623 => Some((0xFEF7, 0xFEF8)),
        0x0625 => Some((0xFEF9, 0xFEFA)),
        0x0627 => Some((0xFEFB, 0xFEFC)),
        _ => None,
    }
}

/// Contextual forms for a run of characters in logical order.  Returns the
/// shaped characters; `None` marks a character absorbed into the previous
/// one (the alef of a lam-alef ligature).
pub fn shape(chars: &[char]) -> Vec<Option<char>> {
    let n = chars.len();
    let mut out: Vec<Option<char>> = chars.iter().map(|&c| Some(c)).collect();
    let prev_of = |i: usize| (0..i).rev().find(|&k| !is_transparent(chars[k]));
    let next_of = |i: usize| (i + 1..n).find(|&k| !is_transparent(chars[k]));
    let joins_left = |k: usize| matches!(forms(chars[k]), Some((Join::Dual | Join::Causing, ..)));
    let joins_right = |k: usize| forms(chars[k]).is_some();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        let Some((kind, iso, fin, ini, med)) = forms(c) else {
            i += 1;
            continue;
        };
        let jp = prev_of(i).is_some_and(joins_left);
        let next = next_of(i);
        if c == '\u{0644}' {
            if let Some((li, lf)) = next.filter(|&k| k == i + 1).and_then(|k| lam_alef(chars[k])) {
                out[i] = char::from_u32(if jp { lf } else { li });
                out[i + 1] = None;
                i += 2;
                continue;
            }
        }
        let jn = kind != Join::Right && next.is_some_and(joins_right);
        let f = match (jp, jn) {
            (true, true) => med,
            (true, false) => fin,
            (false, true) => ini,
            (false, false) => iso,
        };
        out[i] = char::from_u32(f).or(Some(c));
        i += 1;
    }
    out
}

/// Bracket mirroring inside RTL runs (UAX #9 rule L4).
fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

// -------------------------------------------------------- line reorder

/// Margins of the line being laid out (screen columns and WPU).
pub struct Bounds {
    pub lm_col: i32,
    pub rm_col: i32,
    pub lm: i32,
    pub rm: i32,
}

/// Put a laid-out line into visual order (see the module comment).
/// `start` is the element index of `caret[0]`.  Does nothing for lines
/// without RTL characters in LTR paragraphs.
pub fn reorder_line(cells: &mut Vec<LCell>, caret: &mut [(i32, i32)], start: usize, rtl_para: bool, b: &Bounds) {
    if !rtl_para && !cells.iter().any(|c| is_rtl_char(c.ch)) {
        return;
    }
    if cells.is_empty() {
        if rtl_para {
            for k in caret.iter_mut() {
                *k = (b.rm_col - 1, b.rm);
            }
        }
        return;
    }
    // one cell per element only (tab leaders etc. would need more care)
    for w in cells.windows(2) {
        if w[0].el == w[1].el {
            return;
        }
    }
    let cidx = |el: usize| el.checked_sub(start).filter(|&k| k < caret.len());
    // widths in WPU from the caret positions
    let widths: Vec<i32> = cells
        .iter()
        .map(|c| match (cidx(c.el), cidx(c.el + 1)) {
            (Some(a), Some(z)) => (caret[z].1 - caret[a].1).max(0),
            _ => 0,
        })
        .collect();
    let old_caret: Vec<(i32, i32)> = caret.to_vec();

    // 1. shaping (logical order), lam-alef absorbs the alef cell
    let shaped = shape(&cells.iter().map(|c| c.ch).collect::<Vec<_>>());
    let mut keep: Vec<(LCell, i32)> = Vec::with_capacity(cells.len());
    for (k, c) in cells.iter().enumerate() {
        match shaped[k] {
            Some(ch) => keep.push((LCell { ch, ..*c }, widths[k])),
            None => {
                if let Some(last) = keep.last_mut() {
                    last.1 += widths[k];
                }
            }
        }
    }

    // 2. segments = runs of adjacent columns (tabs/indents separate them,
    //    like segment separators in UAX #9)
    let base = if rtl_para { Level::rtl() } else { Level::ltr() };
    let mut segs: Vec<(usize, usize)> = vec![];
    let mut s = 0;
    for k in 1..=keep.len() {
        let orig_col = |k: usize| keep[k].0.col;
        if k == keep.len() || orig_col(k) > orig_col(k - 1) + cw(keep[k - 1].0.ch) + absorbed_between(cells, keep[k - 1].0.el, keep[k].0.el) {
            segs.push((s, k));
            s = k;
        }
    }
    let mut placed: Vec<(i32, i32)> = vec![(0, 0); keep.len()]; // (col, x)
    let mut levels_odd = vec![false; keep.len()];
    for &(a, z) in &segs {
        let text: String = keep[a..z].iter().map(|c| c.0.ch).collect();
        let info = BidiInfo::new(&text, Some(base));
        let para = &info.paragraphs[0];
        let lv = info.reordered_levels_per_char(para, para.range.clone());
        let order = BidiInfo::reorder_visual(&lv);
        // original extent of the segment
        let col0 = keep[a].0.col;
        let x0 = cidx(keep[a].0.el).map(|k| old_caret[k].1).unwrap_or(b.lm);
        let ncols: i32 = keep[a..z].iter().map(|c| cw(c.0.ch)).sum();
        let wsum: i32 = keep[a..z].iter().map(|c| c.1).sum();
        let (mut col, mut x) = if rtl_para {
            // mirror the segment's place across the line
            (b.lm_col + (b.rm_col - 1) - (col0 + ncols - 1), b.lm + b.rm - (x0 + wsum))
        } else {
            (col0, x0)
        };
        for &v in &order {
            let k = a + v;
            placed[k] = (col, x);
            levels_odd[k] = lv[v].is_rtl();
            col += cw(keep[k].0.ch);
            x += keep[k].1;
        }
    }
    let mut out: Vec<LCell> = Vec::with_capacity(keep.len());
    for (k, (c, _)) in keep.iter().enumerate() {
        let ch = if levels_odd[k] { mirror(c.ch) } else { c.ch };
        out.push(LCell { col: placed[k].0, ch, ..*c });
    }

    // 3. caret: on its character's cell; elements without a cell go to
    //    the next character of their segment, or after the last one
    let mut cell_of: Vec<Option<usize>> = vec![None; caret.len()];
    for (k, c) in keep.iter().enumerate() {
        if let Some(ci) = cidx(c.0.el) {
            cell_of[ci] = Some(k);
        }
    }
    let first_col = keep[0].0.col;
    let last_col = keep[keep.len() - 1].0.col;
    for ci in 0..caret.len() {
        if let Some(k) = cell_of[ci] {
            caret[ci] = placed[k];
            continue;
        }
        let (oc, ox) = old_caret[ci];
        let inside = oc >= first_col && oc <= last_col + 1;
        if inside {
            // next character with a cell
            if let Some(k) = (ci + 1..caret.len()).find_map(|j| cell_of[j]) {
                caret[ci] = placed[k];
            } else {
                let k = keep.len() - 1;
                caret[ci] = if levels_odd[k] { (placed[k].0 - 1, placed[k].1) } else { (placed[k].0 + 1, placed[k].1 + keep[k].1) };
            }
        } else if rtl_para {
            caret[ci] = (b.lm_col + (b.rm_col - 1) - oc, b.lm + b.rm - ox);
        }
    }
    *cells = out;
}

/// Screen columns of a character.
fn cw(c: char) -> i32 {
    if crate::cjk::is_wide(c) { 2 } else { 1 }
}

/// Number of cells absorbed (ligature alefs) between two kept elements, so
/// the column gap check does not see a ligature as a segment break.
fn absorbed_between(cells: &[LCell], a: usize, z: usize) -> i32 {
    cells.iter().filter(|c| c.el > a && c.el < z).count() as i32
}

/// Does a laid-out line contain RTL text (needs per-glyph placement)?
pub fn line_has_rtl(cells: &[LCell]) -> bool {
    cells.iter().any(|c| is_rtl_char(c.ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> String {
        shape(&t.chars().collect::<Vec<_>>()).into_iter().flatten().collect()
    }

    #[test]
    fn contextual_forms() {
        // سلام = seen lam alef meem: seen initial, lam-alef final ligature, meem isolated
        assert_eq!(s("سلام"), "\u{FEB3}\u{FEFC}\u{FEE1}");
        // كتاب: kaf initial, teh medial, alef final, beh isolated
        assert_eq!(s("كتاب"), "\u{FEDB}\u{FE98}\u{FE8E}\u{FE8F}");
        // non-joining after alef: ا ب → alef isolated, beh isolated
        assert_eq!(s("اب"), "\u{FE8D}\u{FE8F}");
        // marks do not break joining: بَب → beh initial, fatha, beh final
        assert_eq!(s("بَب"), "\u{FE91}\u{064E}\u{FE90}");
        // Latin untouched
        assert_eq!(s("abc"), "abc");
    }

    #[test]
    fn paragraph_direction() {
        let d = Document::from_text("Hello مرحبا\nمرحبا Hello");
        assert!(!para_is_rtl(&d.els, 0));
        let second = d.els.iter().position(|e| matches!(e, El::Code(Code::HRt))).unwrap() + 1;
        assert!(para_is_rtl(&d.els, second));
        assert!(para_is_rtl(&d.els, d.els.len()));
    }
}

#[cfg(test)]
mod layout_tests {
    use crate::doc::*;
    use crate::layout::{MonoMetrics, layout};

    fn screen(l: &crate::layout::Line) -> String {
        let mut cells = l.cells.clone();
        cells.sort_by_key(|c| c.col);
        cells.iter().map(|c| c.ch).collect()
    }

    #[test]
    fn arabic_phrase_in_english_line() {
        let d = Document::from_text("in اب x");
        let l = layout(&d, &MonoMetrics);
        // "اب" (alef, beh) shows right-to-left: beh then alef, both isolated
        assert_eq!(screen(&l.lines[0]), "in \u{FE8F}\u{FE8D} x");
        // the cursor before alef (element 3) is on the alef's cell
        let alef_cell = l.lines[0].cells.iter().find(|c| c.el == 3).unwrap();
        assert_eq!(l.lines[0].caret_of(3).0, alef_cell.col);
    }

    #[test]
    fn rtl_paragraph_is_right_aligned() {
        let d = Document::from_text("بب 12");
        let l = layout(&d, &MonoMetrics);
        let line = &l.lines[0];
        let max = line.cells.iter().map(|c| c.col).max().unwrap();
        // ends at the right margin (65 columns from column 10)
        assert_eq!(max, 10 + 65 - 1);
        // numbers keep left-to-right order, Arabic is at the right
        assert_eq!(screen(line), "12 \u{FE90}\u{FE91}");
    }

    #[test]
    fn latin_lines_untouched() {
        let d = Document::from_text("plain text (no change)");
        let l = layout(&d, &MonoMetrics);
        assert_eq!(screen(&l.lines[0]), "plain text (no change)");
        assert_eq!(l.lines[0].cells[0].col, 10);
    }

    #[test]
    fn docx_marks_rtl_paragraphs_and_runs() {
        let d = Document::from_text("مرحبا Word\nHello");
        let bytes = crate::docx::write_docx(&d).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut x = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut x).unwrap();
        let paras: Vec<&str> = x.split("<w:p>").skip(1).collect();
        assert!(paras[0].contains("<w:bidi/>") && paras[0].contains("<w:rtl/>"));
        assert!(!paras[1].contains("<w:bidi/>") && !paras[1].contains("<w:rtl/>"));
    }
}
