//! Pagination helpers for the Format codes (fmt.rs): keep-together rules
//! (widow/orphan protection, block protect, conditional end of page) and
//! paper sizes per page.

use crate::doc::*;
use crate::fmt::{Fmt, Paper};
use crate::layout::{Layout, LineEnd, VRow};

/// Height of a page with this paper (labels: one label).
pub fn paper_height(init: &InitialFormat, paper: &Option<Paper>) -> i32 {
    match paper {
        Some(p) => match &p.labels {
            Some(l) => l.h,
            None => p.h,
        },
        None => init.page_height,
    }
}

/// Width/height of a logical page.
pub fn paper_size(init: &InitialFormat, paper: &Option<Paper>) -> (i32, i32) {
    match paper {
        Some(p) => match &p.labels {
            Some(l) => (l.w, l.h),
            None => (p.w, p.h),
        },
        None => (init.page_width, init.page_height),
    }
}

/// For every visual row: must it stay on the same page as the row before?
pub fn keep_with_prev(doc: &Document, l: &Layout) -> Vec<bool> {
    let nv = l.vrows.len();
    let mut keep = vec![false; nv];
    let mut wo = false;
    for e in &doc.initial_codes {
        if let El::Code(Code::WidowOrphan(w)) = e {
            wo = *w;
        }
    }
    let mut in_block = false;
    let mut in_table = false;
    let mut cndl_left = 0u32;
    // rows of the current paragraph, and W/O at its start
    let mut para: Vec<usize> = vec![];
    let mut para_wo = wo;
    for v in 0..nv {
        let ls = match &l.vrows[v] {
            VRow::Lines(ls) => ls.clone(),
            VRow::Border { .. } | VRow::Gap(_) => {
                keep[v] = in_block;
                continue;
            }
        };
        let mut start = usize::MAX;
        let mut end = 0;
        for &li in &ls {
            start = start.min(l.lines[li].start);
            end = end.max(l.lines[li].end);
        }
        if start == usize::MAX {
            continue;
        }
        if para.is_empty() {
            para_wo = wo;
        }
        // state at the row's first visible element decides block protect
        let mut decided = false;
        let mut new_cndl = None;
        for k in start..end.min(doc.els.len()) {
            match &doc.els[k] {
                El::Ch(_) => {
                    if !decided {
                        if in_block && v > 0 {
                            keep[v] = true;
                        }
                        decided = true;
                    }
                }
                El::Code(c) => match c {
                    Code::Fmt(Fmt::BlockPro(on)) => {
                        if !decided && !*on && in_block && v > 0 && k > start {
                            // Off after text on this row: the row belongs to the block
                            keep[v] = true;
                            decided = true;
                        }
                        in_block = *on;
                    }
                    Code::Fmt(Fmt::CndlEop(n)) => new_cndl = Some(n.saturating_sub(1)),
                    Code::WidowOrphan(w) => {
                        wo = *w;
                        if para.is_empty() {
                            para_wo = wo;
                        }
                    }
                    Code::TblDef { .. } => in_table = true,
                    Code::TblOff => in_table = false,
                    _ => {}
                },
            }
        }
        if !decided && in_block && v > 0 && start < end {
            // a row of codes only (e.g. an empty line) inside the block
            keep[v] = true;
        }
        if cndl_left > 0 {
            keep[v] = true;
            cndl_left -= 1;
        }
        if let Some(n) = new_cndl {
            cndl_left = n;
        }
        // paragraphs (outside tables) for widow/orphan protection
        let simple = ls.len() == 1 && !in_table && l.lines[ls[0]].term != LineEnd::Cell;
        if simple {
            para.push(v);
            let t = l.lines[ls[0]].term;
            if t != LineEnd::Soft {
                if para_wo && para.len() >= 2 {
                    keep[para[1]] = true;
                    keep[*para.last().unwrap()] = true;
                }
                para.clear();
            }
        } else {
            para.clear();
        }
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{MonoMetrics, layout};

    fn para_lines(words: usize) -> Vec<El> {
        let mut v: Vec<El> = "wort ".repeat(words).chars().map(El::Ch).collect();
        v.push(El::Code(Code::HRt));
        v
    }

    #[test]
    fn widow_orphan_moves_lines() {
        // 50 single lines, then a 3-line paragraph straddling the break
        let mut d = Document::from_text(&"x\n".repeat(52));
        d.els.extend(para_lines(39)); // 39*5 = 195 chars = 3 lines
        let l = layout(&d, &MonoMetrics);
        let pages_of = |l: &Layout| l.lines.iter().map(|x| x.page).collect::<Vec<_>>();
        let p0 = pages_of(&l);
        let n = l.lines.len();
        // without W/O the paragraph is split
        let split = p0[n - 4] != p0[n - 2];
        assert!(split, "{:?}", &p0[n - 6..]);
        d.els.insert(0, El::Code(Code::WidowOrphan(true)));
        let l = layout(&d, &MonoMetrics);
        let p = pages_of(&l);
        let n = l.lines.len();
        // the paragraph's first two or last two lines stay together
        assert!(p[n - 4] == p[n - 3] && p[n - 3] == p[n - 2] || p[n - 3] == p[n - 2] && p[n - 4] != p[n - 5] || p[n - 4] == p[n - 3], "{:?}", &p[n - 6..]);
        assert!(!(p[n - 4] != p[n - 3]), "orphan left behind: {:?}", &p[n - 6..]);
    }

    #[test]
    fn block_protect_and_cndl_eop() {
        let mut d = Document::from_text(&"x\n".repeat(50));
        d.els.push(El::Code(Code::Fmt(Fmt::BlockPro(true))));
        d.els.extend(Document::from_text(&"y\n".repeat(6)).els);
        d.els.push(El::Code(Code::Fmt(Fmt::BlockPro(false))));
        d.els.extend(Document::from_text("z\n").els);
        let l = layout(&d, &MonoMetrics);
        let ys: Vec<u32> = l.lines.iter().filter(|x| x.cells.first().is_some_and(|c| c.ch == 'y')).map(|x| x.page).collect();
        assert_eq!(ys.len(), 6);
        assert!(ys.iter().all(|p| *p == ys[0]), "{ys:?}");
        assert_eq!(ys[0], 2);

        let mut d = Document::from_text(&"x\n".repeat(52));
        d.els.push(El::Code(Code::Fmt(Fmt::CndlEop(4))));
        d.els.extend(Document::from_text(&"y\n".repeat(4)).els);
        let l = layout(&d, &MonoMetrics);
        let ys: Vec<u32> = l.lines.iter().filter(|x| x.cells.first().is_some_and(|c| c.ch == 'y')).map(|x| x.page).collect();
        assert_eq!(ys, vec![2, 2, 2, 2]);
    }

    #[test]
    fn auto_hyphenation_breaks_words() {
        let mut d = Document::from_text("Das ist ein kurzer Satz mit vielen Wörtern und dann Übersetzungsbüros und mehr.");
        d.els.insert(0, El::Code(Code::Hyphenation(true)));
        let l = layout(&d, &MonoMetrics);
        assert!(l.lines[0].hyph, "{:?}", l.lines[0].cells.iter().map(|c| c.ch).collect::<String>());
        let t: String = l.lines[0].cells.iter().map(|c| c.ch).collect();
        assert!(t.ends_with("Übersetzungs-"), "{t}");
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        assert!(pages[0].runs.iter().any(|r| r.text.iter().collect::<String>().ends_with("Übersetzungs-")));
        // [/] before the word: no hyphenation
        let k = d.els.iter().position(|e| *e == El::Ch('Ü')).unwrap();
        d.els.insert(k, El::Code(Code::Fmt(Fmt::CancelHyph)));
        let l = layout(&d, &MonoMetrics);
        assert!(!l.lines[0].hyph);
    }

    #[test]
    fn soft_hyphen_shows_at_line_end() {
        let mut d = Document::from_text(&("x".repeat(50) + " Übersetzungsbüro"));
        let k = d.els.iter().rposition(|e| *e == El::Ch('b')).unwrap();
        d.els.insert(k, El::Code(Code::SoftHyphen));
        let l = layout(&d, &MonoMetrics);
        let t: String = l.lines[0].cells.iter().map(|c| c.ch).collect();
        assert!(t.ends_with("Übersetzungs-"), "{t}");
        assert!(l.lines[0].hyph);
    }

    #[test]
    fn hard_tabs_isrt_line_height() {
        let mut d = Document::from_text("a");
        d.els.insert(0, El::Code(Code::Fmt(Fmt::HardTab(TabKind::Right))));
        let l = layout(&d, &MonoMetrics);
        assert_eq!(l.lines[0].tab_kinds[0].1, TabKind::Right);
        // [ISRt] is a break point without a hyphen
        let mut d = Document::from_text(&"y".repeat(60));
        d.els.push(El::Code(Code::Fmt(Fmt::ISRt)));
        d.els.extend("zzzzzzzzzzzz".chars().map(El::Ch));
        let l = layout(&d, &MonoMetrics);
        assert_eq!(l.lines.len(), 2);
        assert!(!l.lines[0].hyph);
        // fixed line height
        let mut d = Document::from_text("a\nb\n");
        d.els.insert(0, El::Code(Code::Fmt(Fmt::LnHeight(Some(400)))));
        let l = layout(&d, &MonoMetrics);
        assert_eq!(l.lines[1].y - l.lines[0].y, 400);
    }

    #[test]
    fn paper_code_changes_page_size() {
        let mut d = Document::from_text(&"x\n".repeat(80));
        d.els.insert(0, El::Code(Code::Fmt(Fmt::Paper(Paper { w: 13200, h: 10200, ty: "Standard".into(), labels: None }))));
        let l = layout(&d, &MonoMetrics);
        // landscape letter: 8.5" - 2" = 6.5" of body = 39 lines per page at 6 lpi
        assert_eq!(l.lines[39].page, 2, "{}", l.lines[38].page);
        assert!(l.page_paper[0].as_ref().unwrap().landscape());
    }
}
