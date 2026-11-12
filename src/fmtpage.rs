//! Printing effects of the Format codes (fmt.rs) on the physical pages:
//! Center Page (top to bottom), line numbering in the margin, redline
//! method bars, page numbers with style/suppression, and labels arranged
//! onto sheets.

use crate::doc::*;
use crate::fmt::{Fmt, PageInfo, SUP_PG_BC, SUP_PGNUM};
use crate::layout::{Layout, Metrics};
use crate::page::{Page, Run};

fn margins_per_page(doc: &Document, layout: &Layout) -> Vec<(i32, i32, i32, i32)> {
    // (left, right, top, bottom) in effect at the start of each page
    let init = &doc.init;
    let (mut l, mut r, mut t, mut b) = (init.left, init.right, init.top, init.bottom);
    let apply = |c: &Code, l: &mut i32, r: &mut i32, t: &mut i32, b: &mut i32| match c {
        Code::LRMargin { left, right } => {
            *l = *left;
            *r = *right
        }
        Code::TBMargin { top, bottom } => {
            *t = *top;
            *b = *bottom
        }
        Code::Fmt(Fmt::Paper(p)) => {
            if let Some(lb) = &p.labels {
                *l = lb.ml;
                *r = lb.mr;
                *t = lb.mt;
                *b = lb.mb;
            }
        }
        _ => {}
    };
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            apply(c, &mut l, &mut r, &mut t, &mut b);
        }
    }
    let n = layout.pages.max(1) as usize;
    let mut out = vec![(l, r, t, b); n];
    let mut cur_page = 0usize;
    for line in &layout.lines {
        let p = (line.page.max(1) as usize - 1).min(n - 1);
        if p != cur_page {
            for slot in out.iter_mut().take(p + 1).skip(cur_page + 1) {
                *slot = (l, r, t, b);
            }
            cur_page = p;
        }
        // codes on the first line of a page count for that page
        for k in line.start..line.end.min(doc.els.len()) {
            if let El::Code(c) = &doc.els[k] {
                apply(c, &mut l, &mut r, &mut t, &mut b);
            }
        }
        if line.start == layout.lines.iter().find(|x| x.page == line.page).map(|x| x.start).unwrap_or(usize::MAX) {
            out[p] = (l, r, t, b);
        }
    }
    out
}

/// Center Page, line numbering and redline bars — applied to the body
/// text before headers, footers and notes are added.
pub fn body_effects(doc: &Document, layout: &Layout, infos: &[PageInfo], pages: &mut [Page], m: &dyn Metrics) {
    let init = &doc.init;
    let margins = margins_per_page(doc, layout);
    // ---- Center Page (top to bottom)
    for p in pages.iter_mut() {
        let pi = p.number as usize - 1;
        if !infos.get(pi).is_some_and(|i| i.center) {
            continue;
        }
        let lines: Vec<_> = layout.lines.iter().filter(|l| l.page == p.number && !(l.start == l.end && l.cells.is_empty() && l.term == crate::layout::LineEnd::Eof)).collect();
        let Some(first) = lines.first() else { continue };
        // trailing empty lines do not count
        let last = lines.iter().rev().find(|l| !l.cells.is_empty()).unwrap_or(first);
        let content_bottom = last.y + last.height;
        let body_bottom = p.height - margins.get(pi).map(|m| m.3).unwrap_or(init.bottom);
        let shift = (body_bottom - content_bottom) / 2;
        let _ = first;
        if shift > 0 {
            for r in p.runs.iter_mut() {
                r.y += shift;
            }
            for r in p.rules.iter_mut() {
                r.1 += shift;
            }
        }
    }
    // ---- Line numbering
    let base_h = (init.font.size_cpt as i64 * 12 / 72) as i32;
    let mut cfg: Option<crate::fmt::LineNum> = None;
    for e in &doc.initial_codes {
        if let El::Code(Code::Fmt(Fmt::LnNum(n))) = e {
            cfg = n.clone();
        }
    }
    let mut count: u32 = cfg.as_ref().map(|c| c.start.saturating_sub(1)).unwrap_or(0);
    let mut last_page = 0u32;
    let mut shifts: Vec<i32> = vec![0; pages.len()];
    for p in pages.iter() {
        let pi = p.number as usize - 1;
        if infos.get(pi).is_some_and(|i| i.center) {
            // already shifted: find it from the first run vs layout (cheap: recompute)
            let lines: Vec<_> = layout.lines.iter().filter(|l| l.page == p.number).collect();
            if let Some(last) = lines.iter().rev().find(|l| !l.cells.is_empty()) {
                let body_bottom = p.height - margins.get(pi).map(|m| m.3).unwrap_or(init.bottom);
                shifts[pi] = ((body_bottom - (last.y + last.height)) / 2).max(0);
            }
        }
    }
    for line in &layout.lines {
        for k in line.start..line.end.min(doc.els.len()) {
            if let El::Code(Code::Fmt(Fmt::LnNum(n))) = &doc.els[k] {
                cfg = n.clone();
                count = n.as_ref().map(|c| c.start.saturating_sub(1)).unwrap_or(0);
            }
        }
        let Some(c) = &cfg else {
            last_page = line.page;
            continue;
        };
        if line.page != last_page {
            if c.restart && last_page != 0 {
                count = c.start.saturating_sub(1);
            }
            last_page = line.page;
        }
        let blank = line.cells.iter().all(|x| x.ch == ' ');
        if blank && (!c.count_blank || line.start == line.end && line.term == crate::layout::LineEnd::Eof) {
            continue;
        }
        count += 1;
        if count % c.every.max(1) != 0 {
            continue;
        }
        let Some(p) = pages.get_mut(line.page as usize - 1) else { continue };
        let text: Vec<char> = count.to_string().chars().collect();
        let adv: Vec<i32> = text.iter().map(|ch| m.width(&init.font, init.font.size_cpt, *ch)).collect();
        let y = line.y + base_h * 4 / 5 + shifts.get(line.page as usize - 1).copied().unwrap_or(0);
        p.runs.push(Run { x: c.pos, y, text, adv, font: init.font.name.clone(), size_cpt: init.font.size_cpt, attrs: AttrSet::default() });
    }
    // ---- Redline method: a bar in the margin
    let method = doc.opts.redline;
    if method != 0 {
        for line in &layout.lines {
            if !line.cells.iter().any(|c| c.attrs.has(Attr::Redline)) {
                continue;
            }
            let Some(p) = pages.get_mut(line.page as usize - 1) else { continue };
            let pi = p.number as usize - 1;
            let (lm, rm, _, _) = margins.get(pi).copied().unwrap_or((init.left, init.right, 0, 0));
            let right_side = method == 2 && p.number % 2 == 1;
            let x = if right_side { p.width - rm + 120 } else { (lm - 180).max(0) };
            let dy = shifts.get(pi).copied().unwrap_or(0);
            p.rules.push((x, line.y + dy, 12, line.height));
        }
    }
}

/// `[Pg Numbering]` per page, with `[Pg Num Style]`, new numbers,
/// `[Force]` and `[Suppress]`.
pub fn page_numbers(doc: &Document, layout: &Layout, infos: &[PageInfo], pages: &mut [Page], m: &dyn Metrics, font: &FontSpec) {
    let margins = margins_per_page(doc, layout);
    let n = layout.pages.max(1) as usize;
    let mut pos: u8 = 0;
    for e in &doc.initial_codes {
        if let El::Code(Code::PageNumberPos(p)) = e {
            pos = *p;
        }
    }
    let mut pos_per_page = vec![0u8; n];
    let mut li = 0;
    for (pi, slot) in pos_per_page.iter_mut().enumerate() {
        while li < layout.lines.len() && layout.lines[li].page as usize <= pi + 1 {
            let l = &layout.lines[li];
            for k in l.start..l.end.min(doc.els.len()) {
                if let El::Code(Code::PageNumberPos(p)) = &doc.els[k] {
                    pos = *p;
                }
            }
            li += 1;
        }
        *slot = pos;
    }
    for p in pages.iter_mut() {
        let pi = p.number as usize - 1;
        let info = infos.get(pi).cloned().unwrap_or_default();
        let mut ppos = pos_per_page.get(pi).copied().unwrap_or(0);
        if info.suppress & SUP_PG_BC != 0 {
            ppos = 6;
        } else if info.suppress & SUP_PGNUM != 0 {
            continue;
        }
        if ppos == 0 {
            continue;
        }
        let text: Vec<char> = info.styled().chars().collect();
        let adv: Vec<i32> = text.iter().map(|c| m.width(font, font.size_cpt, *c)).collect();
        let w: i32 = adv.iter().sum();
        let even = info.num % 2 == 0;
        let (top, hpos) = match ppos {
            1 => (true, 0),
            2 => (true, 1),
            3 => (true, 2),
            4 => (true, if even { 0 } else { 2 }),
            5 => (false, 0),
            6 => (false, 1),
            7 => (false, 2),
            _ => (false, if even { 0 } else { 2 }),
        };
        let (lm, rm, tm, bm) = margins.get(pi).copied().unwrap_or((doc.init.left, doc.init.right, doc.init.top, doc.init.bottom));
        let x = match hpos {
            0 => lm,
            1 => (p.width - w) / 2,
            _ => p.width - rm - w,
        };
        let y = if top { tm - 200 } else { p.height - bm + 400 };
        p.runs.push(Run { x, y, text, adv, font: font.name.clone(), size_cpt: font.size_cpt, attrs: AttrSet::default() });
    }
}

/// Labels: every logical page that is a label is placed onto sheets of
/// its paper (rows × columns, top-left corner, gaps).
pub fn arrange_labels(pages: Vec<Page>, layout: &Layout) -> Vec<Page> {
    if !layout.page_paper.iter().any(|p| p.as_ref().is_some_and(|p| p.labels.is_some())) {
        return pages;
    }
    let mut out: Vec<Page> = vec![];
    let mut slot = 0u32;
    let mut sheet: Option<Page> = None;
    let mut last_def: Option<crate::fmt::Paper> = None;
    for pg in pages {
        let paper = layout.page_paper.get(pg.number as usize - 1).cloned().flatten();
        match paper.as_ref().and_then(|p| p.labels.clone().map(|l| (p.clone(), l))) {
            Some((paper, l)) => {
                if last_def.as_ref() != Some(&paper) {
                    if let Some(s) = sheet.take() {
                        out.push(s);
                    }
                    slot = 0;
                    last_def = Some(paper.clone());
                }
                let per = (l.cols * l.rows).max(1);
                if slot % per == 0 {
                    if let Some(s) = sheet.take() {
                        out.push(s);
                    }
                    sheet = Some(Page { number: 0, width: paper.w, height: paper.h, runs: vec![], rules: vec![], images: vec![] });
                }
                let k = slot % per;
                let (c, r) = ((k % l.cols) as i32, (k / l.cols) as i32);
                let dx = l.left + c * (l.w + l.gap_col);
                let dy = l.top + r * (l.h + l.gap_row);
                let s = sheet.as_mut().unwrap();
                for mut run in pg.runs {
                    run.x += dx;
                    run.y += dy;
                    s.runs.push(run);
                }
                for (x, y, w, h) in pg.rules {
                    s.rules.push((x + dx, y + dy, w, h));
                }
                slot += 1;
            }
            None => {
                if let Some(s) = sheet.take() {
                    out.push(s);
                }
                last_def = None;
                out.push(pg);
            }
        }
    }
    if let Some(s) = sheet.take() {
        out.push(s);
    }
    for (k, p) in out.iter_mut().enumerate() {
        p.number = k as u32 + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt::{Labels, LineNum, Paper};
    use crate::layout::{MonoMetrics, layout};

    fn texts(p: &Page) -> Vec<String> {
        p.runs.iter().map(|r| r.text.iter().collect()).collect()
    }

    #[test]
    fn line_numbers_printed() {
        let mut d = Document::from_text("eins\nzwei\n\ndrei\n");
        d.els.insert(0, El::Code(Code::Fmt(Fmt::LnNum(Some(LineNum { count_blank: false, ..Default::default() })))));
        let l = layout(&d, &MonoMetrics);
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        let nums: Vec<String> = pages[0].runs.iter().filter(|r| r.x == 720).map(|r| r.text.iter().collect()).collect();
        assert_eq!(nums, vec!["1", "2", "3"]);
    }

    #[test]
    fn center_page_and_numbers() {
        let mut d = Document::from_text("Titel\n");
        d.els.insert(0, El::Code(Code::CenterPage));
        d.els.insert(0, El::Code(Code::PageNumberPos(6)));
        d.els.insert(0, El::Code(Code::Fmt(Fmt::PgNumStyle("Seite \u{2}".into()))));
        d.els.insert(0, El::Code(Code::Fmt(Fmt::PgNumRoman(4))));
        let l = layout(&d, &MonoMetrics);
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        let t = pages[0].runs.iter().find(|r| r.text.iter().collect::<String>() == "Titel").unwrap();
        assert!(t.y > d.init.page_height / 2 - 600, "y={}", t.y);
        assert!(texts(&pages[0]).contains(&"Seite iv".to_string()), "{:?}", texts(&pages[0]));
    }

    #[test]
    fn force_odd_and_suppress() {
        let mut d = Document::from_text("a\u{c}b\u{c}c");
        d.els.insert(0, El::Code(Code::PageNumberPos(7)));
        // page 2: force odd → 3; page 3 → 4 but suppressed
        let p2 = d.els.iter().position(|e| *e == El::Ch('b')).unwrap();
        d.els.insert(p2, El::Code(Code::Fmt(Fmt::Force { odd: true })));
        let p3 = d.els.iter().position(|e| *e == El::Ch('c')).unwrap();
        d.els.insert(p3, El::Code(Code::Suppress(SUP_PGNUM)));
        let l = layout(&d, &MonoMetrics);
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        assert!(texts(&pages[0]).contains(&"1".to_string()));
        assert!(texts(&pages[1]).contains(&"3".to_string()), "{:?}", texts(&pages[1]));
        assert!(!texts(&pages[2]).iter().any(|t| t == "4"));
    }

    #[test]
    fn labels_on_sheets() {
        let mut d = Document::from_text("Anna\u{c}Bert\u{c}Carl\u{c}Dora");
        let lab = Labels { w: 3600, h: 1200, cols: 2, rows: 1, top: 600, left: 600, gap_col: 0, gap_row: 0, ml: 120, mr: 120, mt: 120, mb: 0 };
        d.els.insert(0, El::Code(Code::Fmt(Fmt::Paper(Paper { w: 10200, h: 13200, ty: "Labels".into(), labels: Some(lab) }))));
        let l = layout(&d, &MonoMetrics);
        assert_eq!(l.pages, 4);
        let pages = crate::page::build_pages(&d, &l, &MonoMetrics);
        let sheets = arrange_labels(pages, &l);
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].width, 10200);
        let b = sheets[0].runs.iter().find(|r| r.text.iter().collect::<String>() == "Bert").unwrap();
        assert!(b.x >= 600 + 3600, "x={}", b.x);
    }
}
