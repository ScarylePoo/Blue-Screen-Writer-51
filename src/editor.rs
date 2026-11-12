//! One open document plus everything needed to edit it: cursor, block,
//! typeover, undelete buffer, layout cache.

use crate::doc::*;
use crate::layout::{self, Layout, LineEnd, Metrics, MonoMetrics};
use std::path::PathBuf;

/// Codes that occupy no space on screen.  In the normal (non-Reveal)
/// editing screen the cursor glides past them, like WP 5.1.
pub fn is_invisible(e: &El) -> bool {
    match e {
        El::Ch(_) => false,
        El::Code(Code::Fmt(f)) => !f.occupies_space(),
        El::Code(c) => !matches!(
            c,
            Code::HRt
                | Code::HPg
                | Code::Tab
                | Code::Indent
                | Code::DblIndent
                | Code::MarRel
                | Code::HardSpace
                | Code::HardHyphen
                | Code::Note { .. }
                | Code::Mark(Mark::Ref { .. })
                | Code::Merge(_)
                | Code::MathOp(_)
                | Code::ParNum(_)
        ) && !matches!(c, Code::GBox(b) if b.anchor == crate::gfx::Anchor::Char),
    }
}

pub struct Editor {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub cursor: usize,
    pub block_anchor: Option<usize>,
    pub typeover: bool,
    pub modified: bool,
    pub layout: Layout,
    pub top_row: usize,
    pub left_col: i32,
    goal_col: Option<i32>,
    /// Up to three most recent deletions (F1 Undelete), newest first.
    pub undelete: Vec<Vec<El>>,
    pub reveal: bool,
    metrics: Box<dyn Metrics>,
    layout_dirty: bool,
    /// Text of the last block operation (Move/Copy) awaiting "retrieve".
    pub clipboard: Option<Vec<El>>,
    /// Undo (WP 5.1 had none; this is a modest modern addition).
    undo: UndoLog,
}

impl Editor {
    pub fn new() -> Editor {
        let doc = Document::new();
        let metrics: Box<dyn Metrics> = Box::new(crate::fonts::TtfMetrics::new());
        let _ = MonoMetrics;
        let layout = layout::layout(&doc, metrics.as_ref());
        Editor {
            doc,
            path: None,
            cursor: 0,
            block_anchor: None,
            typeover: false,
            modified: false,
            layout,
            top_row: 0,
            left_col: 0,
            goal_col: None,
            undelete: Vec::new(),
            reveal: false,
            metrics,
            layout_dirty: false,
            clipboard: None,
            undo: UndoLog::default(),
        }
    }

    pub fn with_doc(doc: Document, path: Option<PathBuf>) -> Editor {
        let mut e = Editor::new();
        e.doc = doc;
        e.path = path;
        e.relayout();
        e
    }

    pub fn set_metrics(&mut self, m: Box<dyn Metrics>) {
        self.metrics = m;
        self.relayout();
    }

    pub fn is_empty(&self) -> bool {
        self.doc.els.is_empty()
    }

    /// The font metrics this editor lays out with (Mark Text: Generate).
    pub fn metrics(&self) -> &dyn Metrics {
        self.metrics.as_ref()
    }

    pub fn relayout(&mut self) {
        self.layout = layout::layout(&self.doc, self.metrics.as_ref());
        self.layout_dirty = false;
        if self.cursor > self.doc.els.len() {
            self.cursor = self.doc.els.len();
        }
    }

    fn dirty(&mut self) {
        self.modified = true;
        self.layout_dirty = true;
    }

    pub fn ensure_layout(&mut self) {
        if self.layout_dirty {
            self.relayout();
        }
    }

    pub(crate) fn snapshot(&mut self) {
        self.undo.snapshot(&self.doc.els, self.cursor);
    }

    /// Replace the whole text stream (sort, merge, math…) as one undo step.
    pub fn set_els(&mut self, els: Vec<El>) {
        self.snapshot();
        self.doc.els = els;
        self.cursor = self.cursor.min(self.doc.els.len());
        self.dirty();
        self.relayout();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.undo(&self.doc.els) {
            Some((els, cur)) => {
                self.doc.els = els;
                self.cursor = cur.min(self.doc.els.len());
                self.dirty();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.undo.redo(&self.doc.els, self.cursor) {
            Some((els, cur)) => {
                self.doc.els = els;
                self.cursor = cur.min(self.doc.els.len());
                self.dirty();
                true
            }
            None => false,
        }
    }

    // ---------------------------------------------------------------- cursor

    /// In normal view, move the cursor forward past invisible codes so it
    /// rests right before the next visible element.
    pub fn settle(&mut self) {
        if self.reveal {
            return;
        }
        while self.cursor < self.doc.els.len()
            && is_invisible(&self.doc.els[self.cursor])
            && !matches!(self.doc.els[self.cursor], El::Code(Code::Cell | Code::Row | Code::TblOff | Code::TblDef { .. }))
        {
            self.cursor += 1;
        }
    }

    pub fn cur_line(&mut self) -> usize {
        self.ensure_layout();
        self.layout.line_of(self.cursor)
    }

    /// (page, Ln WPU, Pos WPU) for the status line.
    pub fn status_pos(&mut self) -> (u32, i32, i32) {
        let li = self.cur_line();
        let l = &self.layout.lines[li];
        let (_, pos) = l.caret_of(self.cursor);
        (l.page, l.y, pos)
    }

    pub fn caret_col(&mut self) -> i32 {
        let li = self.cur_line();
        self.layout.lines[li].caret_of(self.cursor).0
    }

    pub fn left(&mut self) {
        self.goal_col = None;
        if self.reveal {
            self.cursor = self.cursor.saturating_sub(1);
            return;
        }
        // step back over invisible codes then one visible element
        let mut c = self.cursor;
        while c > 0 && is_invisible(&self.doc.els[c - 1]) {
            c -= 1;
        }
        if c > 0 {
            c -= 1;
        }
        // rest after the codes that precede that element
        self.cursor = c;
        self.settle();
    }

    pub fn right(&mut self) {
        self.goal_col = None;
        if self.reveal {
            self.cursor = (self.cursor + 1).min(self.doc.els.len());
            return;
        }
        self.settle();
        if self.cursor < self.doc.els.len() {
            self.cursor += 1;
        }
        self.settle();
    }

    pub fn vertical(&mut self, delta: isize) {
        use crate::layout::VRow;
        self.ensure_layout();
        let li = self.layout.line_of(self.cursor);
        let goal = match self.goal_col {
            Some(g) => g,
            None => {
                let g = self.layout.lines[li].caret_of(self.cursor).0;
                self.goal_col = Some(g);
                g
            }
        };
        let lay = &self.layout;
        let v0 = lay.line_vrow.get(li).copied().unwrap_or(0) as isize;
        let mut tv = v0;
        let mut steps = 0;
        let want = delta.unsigned_abs();
        let mut found = None;
        loop {
            tv += delta.signum();
            if tv < 0 || tv as usize >= lay.vrows.len() {
                break;
            }
            if let VRow::Lines(ls) = &lay.vrows[tv as usize] {
                if !ls.is_empty() {
                    steps += 1;
                    found = Some(tv as usize);
                    if steps >= want {
                        break;
                    }
                }
            }
        }
        let Some(v) = found else { return };
        let VRow::Lines(ls) = &lay.vrows[v] else { return };
        // the line (table cell) horizontally closest to the goal column
        let target = *ls
            .iter()
            .min_by_key(|&&l| {
                let line = &lay.lines[l];
                let a = line.caret.first().map(|c| c.0).unwrap_or(0);
                let b = line.caret.last().map(|c| c.0).unwrap_or(a);
                if goal < a { a - goal } else if goal > b + 2 { goal - b } else { 0 }
            })
            .unwrap();
        self.cursor = self.layout.idx_at_col(target, goal);
        let g = self.goal_col;
        self.settle_on_line(target);
        self.goal_col = g;
    }

    fn settle_on_line(&mut self, li: usize) {
        // like settle, but never leave the line
        if self.reveal {
            return;
        }
        let line = &self.layout.lines[li];
        let limit = line.end;
        while self.cursor < limit && is_invisible(&self.doc.els[self.cursor]) {
            self.cursor += 1;
        }
    }

    pub fn up(&mut self) {
        self.vertical(-1);
    }
    pub fn down(&mut self) {
        self.vertical(1);
    }

    pub fn line_start(&mut self) {
        let li = self.cur_line();
        self.cursor = self.layout.lines[li].start;
        self.goal_col = None;
        self.settle_on_line(li);
    }

    /// Before every code at the beginning of the line (Home,Home,Home,←).
    pub fn line_start_before_codes(&mut self) {
        let li = self.cur_line();
        self.cursor = self.layout.lines[li].start;
        self.goal_col = None;
    }

    pub fn line_end(&mut self) {
        let li = self.cur_line();
        let l = &self.layout.lines[li];
        self.cursor = match l.term {
            LineEnd::Soft => {
                // after the trailing spaces but before next line's start
                // – WP puts the cursor on the last space
                l.end.saturating_sub(1).max(l.start)
            }
            LineEnd::Hard | LineEnd::HardPage => l.end - 1,
            LineEnd::Eof | LineEnd::Cell => l.end,
        };
        self.goal_col = None;
    }

    pub fn doc_start(&mut self) {
        self.cursor = 0;
        self.goal_col = None;
        self.settle();
    }

    pub fn doc_start_before_codes(&mut self) {
        self.cursor = 0;
        self.goal_col = None;
    }

    pub fn doc_end(&mut self) {
        self.cursor = self.doc.els.len();
        self.goal_col = None;
    }

    fn is_word_char(e: &El) -> bool {
        matches!(e, El::Ch(c) if !c.is_whitespace())
    }

    pub fn word_right(&mut self) {
        self.goal_col = None;
        let els = &self.doc.els;
        let mut c = self.cursor;
        while c < els.len() && (Self::is_word_char(&els[c]) || (is_invisible(&els[c]) && !self.reveal)) {
            c += 1;
        }
        while c < els.len() && !Self::is_word_char(&els[c]) {
            c += 1;
        }
        // back up over invisible codes directly in front of the word? No —
        // WP rests after them.
        self.cursor = c;
        self.settle();
    }

    pub fn word_left(&mut self) {
        self.goal_col = None;
        let els = &self.doc.els;
        let mut c = self.cursor;
        while c > 0 && !Self::is_word_char(&els[c - 1]) {
            c -= 1;
        }
        while c > 0 && (Self::is_word_char(&els[c - 1]) || (is_invisible(&els[c - 1]) && !self.reveal)) {
            c -= 1;
        }
        self.cursor = c;
        self.settle();
    }

    pub fn para_down(&mut self) {
        let els = &self.doc.els;
        let mut c = self.cursor;
        while c < els.len() && !matches!(els[c], El::Code(Code::HRt) | El::Code(Code::HPg)) {
            c += 1;
        }
        self.cursor = (c + 1).min(els.len());
        self.goal_col = None;
        self.settle();
    }

    pub fn para_up(&mut self) {
        let els = &self.doc.els;
        let mut c = self.cursor;
        // skip the HRt immediately before
        if c > 0 && matches!(els[c - 1], El::Code(Code::HRt) | El::Code(Code::HPg)) {
            c -= 1;
        }
        while c > 0 && !matches!(els[c - 1], El::Code(Code::HRt) | El::Code(Code::HPg)) {
            c -= 1;
        }
        self.cursor = c;
        self.goal_col = None;
        self.settle();
    }

    pub fn page_move(&mut self, delta: i32) {
        self.ensure_layout();
        let li = self.layout.line_of(self.cursor);
        let page = self.layout.lines[li].page as i32;
        let target = (page + delta).clamp(1, self.layout.pages as i32) as u32;
        if let Some(l) = self.layout.lines.iter().find(|l| l.page == target) {
            self.cursor = l.start;
        }
        self.goal_col = None;
        self.settle();
    }

    pub fn goto_page(&mut self, page: u32) {
        self.ensure_layout();
        let target = page.clamp(1, self.layout.pages);
        if let Some(l) = self.layout.lines.iter().find(|l| l.page == target) {
            self.cursor = l.start;
        }
        self.goal_col = None;
        self.settle();
    }

    pub fn page_top(&mut self) {
        let li = self.cur_line();
        let page = self.layout.lines[li].page;
        if let Some(l) = self.layout.lines.iter().find(|l| l.page == page) {
            self.cursor = l.start;
        }
        self.settle();
    }

    pub fn page_bottom(&mut self) {
        let li = self.cur_line();
        let page = self.layout.lines[li].page;
        if let Some(l) = self.layout.lines.iter().rev().find(|l| l.page == page) {
            self.cursor = l.start;
        }
        self.line_end();
    }

    // --------------------------------------------------------------- editing

    pub fn insert_el(&mut self, e: El) {
        self.snapshot_coalesce(&e);
        if self.typeover {
            if let El::Ch(_) = e {
                if let Some(El::Ch(_)) = self.doc.els.get(self.cursor) {
                    self.doc.els[self.cursor] = e;
                    self.cursor += 1;
                    self.dirty();
                    return;
                }
            }
        }
        self.doc.els.insert(self.cursor, e);
        self.cursor += 1;
        self.dirty();
    }

    fn snapshot_coalesce(&mut self, e: &El) {
        // one undo step per word for typing
        let boundary = !matches!(e, El::Ch(c) if !c.is_whitespace());
        if boundary || self.undo.is_empty() {
            self.snapshot();
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.insert_el(El::Ch(c));
        self.goal_col = None;
    }

    pub fn insert_code(&mut self, c: Code) {
        self.snapshot();
        self.doc.els.insert(self.cursor, El::Code(c));
        self.cursor += 1;
        self.dirty();
        self.goal_col = None;
    }

    pub fn insert_els(&mut self, els: &[El]) {
        self.snapshot();
        let tail = self.doc.els.split_off(self.cursor);
        self.doc.els.extend_from_slice(els);
        self.doc.els.extend(tail);
        self.cursor += els.len();
        self.doc.normalize_attrs_keep(&mut self.cursor);
        self.dirty();
    }

    fn push_undelete(&mut self, els: Vec<El>) {
        if els.is_empty() {
            return;
        }
        self.undelete.insert(0, els);
        self.undelete.truncate(3);
    }

    /// Remove `from..to`; paired attribute codes whose partner remains are
    /// fixed up by normalisation.  Returns removed elements.
    pub fn delete_range(&mut self, from: usize, to: usize, remember: bool) -> Vec<El> {
        let to = to.min(self.doc.els.len());
        if from >= to {
            return vec![];
        }
        self.snapshot();
        let removed: Vec<El> = self.doc.els.drain(from..to).collect();
        if self.cursor > to {
            self.cursor -= to - from;
        } else if self.cursor > from {
            self.cursor = from;
        }
        // If an attribute was turned on/off inside the removed part, the
        // partner code must go too — WP deletes code pairs together.
        self.drop_orphan_partners(&removed, from);
        if remember {
            self.push_undelete(removed.clone());
        }
        self.dirty();
        removed
    }

    fn drop_orphan_partners(&mut self, removed: &[El], at: usize) {
        let _ = at;
        let mut bal: std::collections::HashMap<Attr, i32> = Default::default();
        for e in removed {
            if let Some((a, on)) = e.code().and_then(|c| c.attr()) {
                *bal.entry(a).or_default() += if on { 1 } else { -1 };
            }
        }
        for (a, b) in bal {
            if b > 0 {
                // removed an ON without its OFF: remove next OFF after cursor
                if let Some(i) = (self.cursor..self.doc.els.len())
                    .find(|&i| self.doc.els[i] == El::Code(Code::AttrOff(a)))
                {
                    self.doc.els.remove(i);
                }
            } else if b < 0 {
                if let Some(i) = (0..self.cursor).rev().find(|&i| self.doc.els[i] == El::Code(Code::AttrOn(a))) {
                    self.doc.els.remove(i);
                    self.cursor -= 1;
                }
            }
        }
    }

    /// Delete one element at index `i`, deleting a paired partner too.
    pub fn delete_at(&mut self, i: usize) -> bool {
        if i >= self.doc.els.len() {
            return false;
        }
        self.snapshot();
        let partner = self.doc.partner_of(i);
        let removed = self.doc.els.remove(i);
        if self.cursor > i {
            self.cursor -= 1;
        }
        if let Some(p) = partner {
            let p = if p > i { p - 1 } else { p };
            self.doc.els.remove(p);
            if self.cursor > p {
                self.cursor -= 1;
            }
        }
        if removed.is_char() || matches!(removed, El::Code(Code::HRt | Code::Tab | Code::HPg)) {
            // single characters join the last undelete entry if adjacent
            if let Some(last) = self.undelete.first_mut() {
                if last.len() < 512 && last.iter().all(|e| e.is_char()) && removed.is_char() {
                    last.push(removed);
                } else {
                    self.push_undelete(vec![removed]);
                }
            } else {
                self.push_undelete(vec![removed]);
            }
        }
        self.dirty();
        true
    }

    /// Index of the element Backspace would delete (normal view: skip
    /// invisible codes).  Returns None at document start.
    pub fn backspace_target(&self) -> Option<usize> {
        let mut c = self.cursor;
        if self.reveal {
            return c.checked_sub(1);
        }
        while c > 0 && is_invisible(&self.doc.els[c - 1]) {
            c -= 1;
        }
        c.checked_sub(1)
    }

    pub fn del_target(&self) -> Option<usize> {
        let mut c = self.cursor;
        if !self.reveal {
            while c < self.doc.els.len() && is_invisible(&self.doc.els[c]) {
                c += 1;
            }
        }
        if c < self.doc.els.len() { Some(c) } else { None }
    }

    pub fn delete_word(&mut self) {
        // Ctrl-Backspace: delete the word at the cursor plus trailing space
        let els = &self.doc.els;
        let mut a = self.cursor;
        let mut b = self.cursor;
        if b < els.len() && !Self::is_word_char(&els[b]) && a > 0 && Self::is_word_char(&els[a - 1]) {
            // on a space after a word → delete previous word
            b = a;
        }
        while a > 0 && Self::is_word_char(&els[a - 1]) {
            a -= 1;
        }
        while b < els.len() && Self::is_word_char(&els[b]) {
            b += 1;
        }
        while b < els.len() && matches!(els[b], El::Ch(' ')) {
            b += 1;
        }
        self.cursor = a;
        self.delete_range(a, b, true);
        self.settle();
    }

    pub fn delete_word_left(&mut self) {
        let els = &self.doc.els;
        let mut a = self.cursor;
        while a > 0 && Self::is_word_char(&els[a - 1]) {
            a -= 1;
        }
        let b = self.cursor;
        self.delete_range(a, b, true);
    }

    pub fn delete_word_right(&mut self) {
        let els = &self.doc.els;
        let mut b = self.cursor;
        while b < els.len() && Self::is_word_char(&els[b]) {
            b += 1;
        }
        while b < els.len() && matches!(els[b], El::Ch(' ')) {
            b += 1;
        }
        let a = self.cursor;
        self.delete_range(a, b, true);
    }

    pub fn delete_eol(&mut self) {
        let li = self.cur_line();
        let l = &self.layout.lines[li];
        let end = match l.term {
            LineEnd::Hard | LineEnd::HardPage => l.end - 1,
            _ => l.end,
        };
        let a = self.cursor;
        // keep codes (WP 5.1 only deletes text to EOL, codes stay)
        let removed: Vec<usize> = (a..end).filter(|&i| !matches!(self.doc.els[i], El::Code(_)) || matches!(self.doc.els[i], El::Code(Code::Tab | Code::HardSpace))).collect();
        if removed.is_empty() {
            return;
        }
        self.snapshot();
        let mut saved = Vec::new();
        for &i in removed.iter().rev() {
            saved.insert(0, self.doc.els.remove(i));
        }
        self.push_undelete(saved);
        self.dirty();
    }

    pub fn delete_eop(&mut self) {
        let li = self.cur_line();
        let page = self.layout.lines[li].page;
        let end = self
            .layout
            .lines
            .iter()
            .rev()
            .find(|l| l.page == page)
            .map(|l| l.end)
            .unwrap_or(self.doc.els.len());
        let a = self.cursor;
        self.delete_range(a, end, true);
    }

    /// Bold/Underline/etc. key without block: WP 5.1 inserts the pair and
    /// puts the cursor between; pressing again steps over the off-code.
    pub fn toggle_attr(&mut self, a: Attr) {
        // already sitting right before the matching off code?
        let mut c = self.cursor;
        while c < self.doc.els.len() {
            match &self.doc.els[c] {
                El::Code(Code::AttrOff(x)) if *x == a => {
                    self.cursor = c + 1;
                    return;
                }
                El::Code(Code::AttrOff(_)) => c += 1,
                _ => break,
            }
        }
        if self.doc.attrs_at(self.cursor).has(a) {
            // inside attributed text: insert an off/on pair
            self.snapshot();
            self.doc.els.insert(self.cursor, El::Code(Code::AttrOff(a)));
            self.doc.els.insert(self.cursor + 1, El::Code(Code::AttrOn(a)));
            self.cursor += 1;
            self.dirty();
            return;
        }
        self.snapshot();
        self.doc.els.insert(self.cursor, El::Code(Code::AttrOn(a)));
        self.doc.els.insert(self.cursor + 1, El::Code(Code::AttrOff(a)));
        self.cursor += 1;
        self.dirty();
    }

    /// Apply an attribute to the block range.
    pub fn attr_block(&mut self, a: Attr) {
        if let Some((s, e)) = self.block_range() {
            self.snapshot();
            self.doc.els.insert(e, El::Code(Code::AttrOff(a)));
            self.doc.els.insert(s, El::Code(Code::AttrOn(a)));
            let mut cur = e + 2;
            self.doc.normalize_attrs_keep(&mut cur);
            self.cursor = cur.min(self.doc.els.len());
            self.block_anchor = None;
            self.dirty();
        }
    }

    pub fn block_range(&self) -> Option<(usize, usize)> {
        let a = self.block_anchor?;
        let (s, e) = if a <= self.cursor { (a, self.cursor) } else { (self.cursor, a) };
        Some((s, e))
    }

    pub fn block_els(&self) -> Option<Vec<El>> {
        let (s, e) = self.block_range()?;
        Some(self.doc.els[s..e].to_vec())
    }

    pub fn delete_block(&mut self) -> Option<Vec<El>> {
        let (s, e) = self.block_range()?;
        self.block_anchor = None;
        self.cursor = s;
        Some(self.delete_range(s, e, true))
    }

    pub fn undelete_restore(&mut self, which: usize) {
        if let Some(els) = self.undelete.get(which).cloned() {
            self.insert_els(&els);
        }
    }

    /// Case conversion of block (Shift-F3 with block on).
    pub fn block_case(&mut self, upper: bool) {
        if let Some((s, e)) = self.block_range() {
            self.snapshot();
            for i in s..e {
                if let El::Ch(c) = self.doc.els[i] {
                    let n: String = if upper { c.to_uppercase().collect() } else { c.to_lowercase().collect() };
                    if n.chars().count() == 1 {
                        self.doc.els[i] = El::Ch(n.chars().next().unwrap());
                    }
                }
            }
            self.block_anchor = None;
            self.dirty();
        }
    }

    /// Insert a format code, replacing an identical-kind code immediately
    /// before the cursor (WP replaces rather than stacking).
    pub fn insert_format(&mut self, c: Code) {
        let same_kind = |a: &Code, b: &Code| match (a, b) {
            (Code::Fmt(x), Code::Fmt(y)) => x.same_kind(y),
            _ => std::mem::discriminant(a) == std::mem::discriminant(b),
        };
        if self.cursor > 0 {
            if let El::Code(prev) = &self.doc.els[self.cursor - 1] {
                if same_kind(prev, &c) {
                    self.snapshot();
                    self.doc.els[self.cursor - 1] = El::Code(c);
                    self.dirty();
                    return;
                }
            }
        }
        self.insert_code(c);
    }

    /// Format codes like margins take effect from the start of the
    /// paragraph in WP 5.1 when inserted at its beginning; we insert at
    /// the start of the current line if the cursor is at line start.
    pub fn format_state_at(&self, idx: usize) -> FormatState {
        let mut f = FormatState::from_init(&self.doc.init);
        for e in self.doc.initial_codes.iter().chain(self.doc.els[..idx.min(self.doc.els.len())].iter()) {
            if let El::Code(c) = e {
                f.apply(c);
            }
        }
        f
    }

    // -------------------------------------------------------------- search

    pub fn find(&self, needle: &[El], from: usize, forward: bool, whole_word: bool) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        let els = &self.doc.els;
        let matches_at = |i: usize| -> bool {
            if i + needle.len() > els.len() {
                return false;
            }
            for (k, n) in needle.iter().enumerate() {
                let e = &els[i + k];
                let ok = match (n, e) {
                    // lower-case in the search string matches both cases
                    (El::Ch(a), El::Ch(b)) => {
                        if a.is_lowercase() {
                            a.to_lowercase().eq(b.to_lowercase())
                        } else {
                            a == b
                        }
                    }
                    (El::Code(a), El::Code(b)) => code_matches(a, b),
                    _ => false,
                };
                if !ok {
                    return false;
                }
            }
            if whole_word {
                let before = i.checked_sub(1).and_then(|j| els.get(j));
                let after = els.get(i + needle.len());
                let wc = |e: Option<&El>| matches!(e, Some(El::Ch(c)) if c.is_alphanumeric());
                if wc(before) || wc(after) {
                    return false;
                }
            }
            true
        };
        if forward {
            (from..els.len()).find(|&i| matches_at(i))
        } else {
            (0..from.min(els.len())).rev().find(|&i| matches_at(i))
        }
    }
}

/// One step of the undo chain: how to turn a state back into the older one
/// (replace `[prefix .. len-suffix]` with `middle`).
#[derive(Clone)]
struct Delta {
    prefix: usize,
    suffix: usize,
    middle: Vec<El>,
    cursor: usize,
}

fn delta(from: &[El], to: &[El], cursor: usize) -> Delta {
    let mut p = 0;
    while p < from.len() && p < to.len() && from[p] == to[p] {
        p += 1;
    }
    let mut s = 0;
    while s < from.len() - p && s < to.len() - p && from[from.len() - 1 - s] == to[to.len() - 1 - s] {
        s += 1;
    }
    Delta { prefix: p, suffix: s, middle: to[p..to.len() - s].to_vec(), cursor }
}

fn apply(state: &[El], d: &Delta) -> Vec<El> {
    let mut v = Vec::with_capacity(state.len() + d.middle.len());
    v.extend_from_slice(&state[..d.prefix.min(state.len())]);
    v.extend_from_slice(&d.middle);
    v.extend_from_slice(&state[state.len().saturating_sub(d.suffix)..]);
    v
}

/// Memory-friendly undo/redo: one full copy of the last snapshot plus small
/// deltas, so long documents don't multiply in memory.
#[derive(Default)]
pub struct UndoLog {
    last: Option<(Vec<El>, usize)>,
    back: Vec<Delta>,
    redo: Vec<Delta>,
}

impl UndoLog {
    pub fn is_empty(&self) -> bool {
        self.last.is_none()
    }

    /// Record the state *before* an edit.
    pub fn snapshot(&mut self, els: &[El], cursor: usize) {
        if let Some((prev, pc)) = self.last.take() {
            if prev.as_slice() != els {
                self.back.push(delta(els, &prev, pc));
                if self.back.len() > 1000 {
                    self.back.remove(0);
                }
            } else {
                self.last = Some((prev, pc));
                self.redo.clear();
                return;
            }
        }
        self.last = Some((els.to_vec(), cursor));
        self.redo.clear();
    }

    /// Returns the state to go back to.
    pub fn undo(&mut self, current: &[El]) -> Option<(Vec<El>, usize)> {
        let (target, tc) = self.last.take()?;
        if target.as_slice() == current {
            // nothing changed since the snapshot: step further back
            let d = self.back.pop()?;
            let older = apply(&target, &d);
            self.redo.push(delta(&older, &target, tc));
            self.last = Some((older.clone(), d.cursor));
            return Some((older, d.cursor));
        }
        self.redo.push(delta(&target, current, tc));
        // the snapshot before this one becomes the new "last"
        if let Some(d) = self.back.pop() {
            self.last = Some((apply(&target, &d), d.cursor));
        }
        Some((target, tc))
    }

    pub fn redo(&mut self, current: &[El], cursor: usize) -> Option<(Vec<El>, usize)> {
        let d = self.redo.pop()?;
        let next = apply(current, &d);
        if let Some((prev, pc)) = self.last.take() {
            if prev.as_slice() != current {
                self.back.push(delta(current, &prev, pc));
            }
        }
        self.last = Some((current.to_vec(), cursor));
        Some((next, d.cursor))
    }
}

/// Search matching for codes: e.g. searching for [BOLD] finds any [BOLD];
/// searching for [Font] finds any font code.
pub fn code_matches(pattern: &Code, actual: &Code) -> bool {
    match (pattern, actual) {
        (Code::AttrOn(a), Code::AttrOn(b)) | (Code::AttrOff(a), Code::AttrOff(b)) => a == b,
        _ => std::mem::discriminant(pattern) == std::mem::discriminant(actual),
    }
}

#[derive(Clone, Debug)]
pub struct FormatState {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    pub spacing: u32,
    pub just: Justify,
    pub font: FontSpec,
    pub tabs: TabSet,
    pub hyph: bool,
    pub widow: bool,
}

impl FormatState {
    pub fn from_init(i: &InitialFormat) -> FormatState {
        FormatState {
            left: i.left,
            right: i.right,
            top: i.top,
            bottom: i.bottom,
            spacing: i.line_spacing,
            just: i.justify,
            font: i.font.clone(),
            tabs: i.tabs.clone(),
            hyph: false,
            widow: false,
        }
    }
    pub fn apply(&mut self, c: &Code) {
        match c {
            Code::LRMargin { left, right } => {
                self.left = *left;
                self.right = *right
            }
            Code::TBMargin { top, bottom } => {
                self.top = *top;
                self.bottom = *bottom
            }
            Code::LineSpacing(s) => self.spacing = *s,
            Code::Just(j) => self.just = *j,
            Code::Font(f) => self.font = f.clone(),
            Code::TabSet(t) => self.tabs = t.clone(),
            Code::Hyphenation(h) => self.hyph = *h,
            Code::WidowOrphan(w) => self.widow = *w,
            _ => {}
        }
    }
}

impl Document {
    /// normalize_attrs while keeping a cursor index valid.
    pub fn normalize_attrs_keep(&mut self, cursor: &mut usize) {
        // mark cursor with a sentinel comment code
        let sentinel = El::Code(Code::Unknown { name: "\u{0}cursor".into(), data: vec![] });
        let c = (*cursor).min(self.els.len());
        self.els.insert(c, sentinel.clone());
        // normalisation must not treat the sentinel as separating codes
        // incorrectly; it is a code with no attr so pairs around it survive.
        self.normalize_attrs();
        if let Some(p) = self.els.iter().position(|e| *e == sentinel) {
            self.els.remove(p);
            *cursor = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(s: &str) -> Editor {
        Editor::with_doc(Document::from_text(s), None)
    }

    #[test]
    fn bold_toggle_types_bold() {
        let mut e = ed("");
        e.toggle_attr(Attr::Bold);
        e.insert_char('a');
        e.toggle_attr(Attr::Bold);
        e.insert_char('b');
        assert_eq!(
            e.doc.els,
            vec![El::Code(Code::AttrOn(Attr::Bold)), El::Ch('a'), El::Code(Code::AttrOff(Attr::Bold)), El::Ch('b')]
        );
    }

    #[test]
    fn deleting_code_deletes_partner() {
        let mut e = ed("");
        e.toggle_attr(Attr::Bold);
        e.insert_char('a');
        e.delete_at(0);
        assert_eq!(e.doc.els, vec![El::Ch('a')]);
    }

    #[test]
    fn cursor_skips_codes() {
        let mut e = ed("");
        e.doc.els = vec![El::Ch('a'), El::Code(Code::AttrOn(Attr::Bold)), El::Ch('b'), El::Code(Code::AttrOff(Attr::Bold))];
        e.relayout();
        e.cursor = 0;
        e.right();
        assert_eq!(e.cursor, 2); // after [BOLD], before 'b'
        e.right();
        assert_eq!(e.cursor, 4);
        e.left();
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn block_bold() {
        let mut e = ed("hello world");
        e.cursor = 0;
        e.block_anchor = Some(0);
        e.cursor = 5;
        e.attr_block(Attr::Bold);
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "hello world");
        assert_eq!(e.doc.els[0], El::Code(Code::AttrOn(Attr::Bold)));
        assert_eq!(e.doc.els[6], El::Code(Code::AttrOff(Attr::Bold)));
    }

    #[test]
    fn undo_redo() {
        let mut e = ed("");
        for c in "eins zwei drei".chars() {
            e.insert_char(c);
        }
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "eins zwei drei");
        e.undo();
        let a = e.doc.plain_text(0, e.doc.len());
        assert!(a.len() < 14, "{a}");
        e.undo();
        let b = e.doc.plain_text(0, e.doc.len());
        assert!(b.len() < a.len(), "{b}");
        e.redo();
        assert_eq!(e.doc.plain_text(0, e.doc.len()), a);
        e.redo();
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "eins zwei drei");
        while e.undo() {}
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "");
    }

    #[test]
    fn search_case() {
        let e = ed("Hello hello");
        let n: Vec<El> = "hello".chars().map(El::Ch).collect();
        assert_eq!(e.find(&n, 0, true, false), Some(0));
        assert_eq!(e.find(&n, 1, true, false), Some(6));
        let n: Vec<El> = "Hello".chars().map(El::Ch).collect();
        assert_eq!(e.find(&n, 1, true, false), None);
    }
}
