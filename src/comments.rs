//! Document comments (Ctrl-F5, 4): create / edit / convert to text, and the
//! double-line comment box WP 5.1 shows in the edit screen (never printed).

use crate::doc::*;
use crate::grid::*;
use crate::layout::COL_WPU;
use crate::screens::*;
use crate::ui::{Screen, Wp};

/// Screen columns (first, last) of the comment box for a document.
pub fn box_cols(doc: &Document) -> (i32, i32) {
    let c0 = (doc.init.left / COL_WPU - 1).max(0);
    let c1 = ((doc.init.page_width - doc.init.right) / COL_WPU).max(c0 + 12);
    (c0, c1)
}

/// The comment text word-wrapped to the inside of the box.
pub fn box_lines(doc: &Document, text: &str) -> Vec<String> {
    let (c0, c1) = box_cols(doc);
    let width = (c1 - c0 - 3).max(8) as usize;
    let mut out = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let wl = word.chars().count();
            let ll = line.chars().count();
            if ll > 0 && ll + 1 + wl > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            // hard-break very long words
            let mut w: String = word.to_string();
            while w.chars().count() > width {
                let head: String = w.chars().take(width).collect();
                out.push(head);
                w = w.chars().skip(width).collect();
            }
            line.push_str(&w);
        }
        out.push(line);
    }
    while out.len() > 1 && out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// Number of screen rows the box takes (text lines + the two borders).
pub fn box_rows(doc: &Document, text: &str) -> usize {
    box_lines(doc, text).len() + 2
}

/// Draw row `r` of the box of the comment at element `el` on screen row `row`.
pub fn draw_row(g: &mut Grid, doc: &Document, el: usize, r: usize, row: usize, left: i32, st: Style) {
    let Some(El::Code(Code::Comment(text))) = doc.els.get(el) else { return };
    let lines = box_lines(doc, text);
    let (c0, c1) = box_cols(doc);
    let put = |g: &mut Grid, c: i32, ch: char| {
        let x = c - left;
        if x >= 0 && (x as usize) < g.cols {
            g.put(x as usize, row, ch, st);
        }
    };
    if r == 0 || r == lines.len() + 1 {
        let (a, b) = if r == 0 { ('╔', '╗') } else { ('╚', '╝') };
        put(g, c0, a);
        for c in c0 + 1..c1 {
            put(g, c, '═');
        }
        put(g, c1, b);
    } else {
        put(g, c0, '║');
        for c in c0 + 1..c1 {
            put(g, c, ' ');
        }
        for (k, ch) in lines[r - 1].chars().enumerate() {
            put(g, c0 + 2 + k as i32, ch);
        }
        put(g, c1, '║');
    }
}

pub fn text_to_els(s: &str) -> Vec<El> {
    Document::from_text(s).els
}

pub fn els_to_text(els: &[El]) -> String {
    let mut d = Document::new();
    d.els = els.to_vec();
    d.plain_text(0, d.len())
}

/// The comment WP would edit: the first one before the cursor (searching
/// backwards), else the first one after it.
fn find_comment(wp: &Wp) -> Option<usize> {
    let e = wp.ed_ref();
    let is_c = |x: &El| matches!(x, El::Code(Code::Comment(_)));
    let cur = e.cursor.min(e.doc.els.len());
    e.doc.els[..cur].iter().rposition(is_c).or_else(|| e.doc.els[cur..].iter().position(is_c).map(|k| k + cur))
}

pub fn create(wp: &mut Wp) {
    crate::subedit::open(wp, "Document Comment", vec![], false, |wp, els| {
        let t = els_to_text(&els);
        if !t.trim().is_empty() {
            wp.ed().insert_code(Code::Comment(t));
        }
    });
}

pub fn edit(wp: &mut Wp) {
    let Some(idx) = find_comment(wp) else {
        wp.flash("* Not found *");
        return;
    };
    let Some(El::Code(Code::Comment(t))) = wp.ed_ref().doc.els.get(idx).cloned() else { return };
    crate::subedit::open(wp, "Document Comment", text_to_els(&t), false, move |wp, els| {
        let e = wp.ed();
        if let Some(El::Code(Code::Comment(old))) = e.doc.els.get_mut(idx) {
            *old = els_to_text(&els);
            e.modified = true;
            e.relayout();
        }
    });
}

pub fn convert_to_text(wp: &mut Wp) {
    let Some(idx) = find_comment(wp) else {
        wp.flash("* Not found *");
        return;
    };
    let e = wp.ed();
    let Some(El::Code(Code::Comment(t))) = e.doc.els.get(idx).cloned() else { return };
    let els = text_to_els(&t);
    let n = els.len();
    e.doc.els.splice(idx..idx + 1, els);
    e.cursor = idx + n;
    e.modified = true;
    e.relayout();
}

/// Ctrl-F5, 4: `Comment: 1 Create; 2 Edit; 3 Convert to Text: 0`.
pub fn menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Create", "~Edit", "Con~vert to Text"], |wp, k| {
        match k {
            0 => create(wp),
            1 => edit(wp),
            _ => {
                convert_to_text(wp);
                wp.after_edit();
            }
        }
        None
    })
    .prefix("Comment: ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrap_and_rows() {
        let d = Document::from_text("x");
        let long = "wort ".repeat(40);
        let lines = box_lines(&d, &long);
        assert!(lines.len() >= 3, "{lines:?}");
        let (c0, c1) = box_cols(&d);
        assert!(lines.iter().all(|l| l.chars().count() as i32 <= c1 - c0 - 3));
        assert_eq!(box_rows(&d, "a\nb"), 4);
    }
    #[test]
    fn comment_box_rows_in_layout() {
        let mut d = Document::from_text("Hallo\nWelt");
        d.els.insert(2, El::Code(Code::Comment("Notiz".into())));
        let l = crate::layout::layout(&d, &crate::layout::MonoMetrics);
        let n = l.drows.iter().filter(|r| matches!(r, crate::layout::DRow::Comment { .. })).count();
        assert_eq!(n, 3);
        // comments take no space on paper
        let l2 = crate::layout::layout(&Document::from_text("Hallo\nWelt"), &crate::layout::MonoMetrics);
        assert_eq!(l.lines[1].y, l2.lines[1].y);
    }
}
