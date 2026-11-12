//! Track changes (Word-compatible w:ins / w:del) and Document Compare.
//!
//! In the code stream a tracked insertion is `[INS:author date]text[ins]`
//! and a deletion `[DEL:author date]text[del]`.  Deleted text stays in the
//! document (shown struck out) until the change is accepted.

use crate::doc::*;
use crate::editor::Editor;
use crate::screens::*;
use crate::ui::Wp;

pub fn author() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Author".into())
}

/// The local calendar date (YYYY-MM-DD) of a stored change date.  Dates are
/// kept in UTC like Word ("2026-11-12T01:06:00Z"); without a time part the
/// date is shown as it is.
pub fn local_date(iso: &str) -> String {
    let n = |a: usize, b: usize| iso.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(y), Some(mo), Some(d)) = (n(0, 4), n(5, 7), n(8, 10)) else { return iso.to_string() };
    let (Some(h), Some(mi)) = (n(11, 13), n(14, 16)) else { return iso.get(..10).unwrap_or(iso).to_string() };
    if !iso.ends_with('Z') {
        return iso[..10].to_string();
    }
    // days from civil (Howard Hinnant)
    let (yy, mm) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    let secs = days * 86400 + h * 3600 + mi * 60;
    let local = secs + crate::sys::utc_offset_at(secs);
    let (y, m, d) = crate::filelist::civil_from_days(local.div_euclid(86400));
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, m, d) = crate::filelist::civil_from_days(secs.div_euclid(86400));
    let r = secs.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", r / 3600, (r % 3600) / 60, r % 60)
}

fn on(del: bool) -> El {
    El::Code(Code::RevOn { del, author: author(), date: now_iso() })
}
fn off(del: bool) -> El {
    El::Code(Code::RevOff { del })
}

/// Tracked change state at position `i`: None, Some(false)=inserted, Some(true)=deleted.
pub fn rev_at(els: &[El], i: usize) -> Option<bool> {
    let mut cur = None;
    for e in &els[..i.min(els.len())] {
        match e {
            El::Code(Code::RevOn { del, .. }) => cur = Some(*del),
            El::Code(Code::RevOff { .. }) => cur = None,
            _ => {}
        }
    }
    cur
}

/// Typing with track changes on: the text goes into an insertion run.
pub fn insert_tracked(e: &mut Editor, el: El) {
    match rev_at(&e.doc.els, e.cursor) {
        Some(false) => e.insert_el(el),
        Some(true) => {
            // inside deleted text: close the deletion, insert, reopen
            let at = e.cursor;
            e.doc.els.insert(at, off(true));
            e.doc.els.insert(at + 1, on(false));
            e.doc.els.insert(at + 2, el);
            e.doc.els.insert(at + 3, off(false));
            e.doc.els.insert(at + 4, on(true));
            e.cursor = at + 3;
            e.modified = true;
            e.relayout();
        }
        None => {
            // right after our own insertion run?  extend it
            if e.cursor > 0 && matches!(e.doc.els[e.cursor - 1], El::Code(Code::RevOff { del: false })) {
                e.cursor -= 1;
                e.insert_el(el);
                // keep the cursor after the run's end code? no: inside, so
                // further typing continues the run
                return;
            }
            let at = e.cursor;
            e.doc.els.insert(at, on(false));
            e.doc.els.insert(at + 1, el);
            e.doc.els.insert(at + 2, off(false));
            e.cursor = at + 2;
            e.modified = true;
            e.relayout();
        }
    }
}

/// Delete element `i` with track changes on.  Returns the new cursor.
pub fn delete_tracked(e: &mut Editor, i: usize, backwards: bool) {
    if i >= e.doc.els.len() {
        return;
    }
    match rev_at(&e.doc.els, i) {
        Some(false) => {
            // removing our own insertion: really delete
            e.delete_at(i);
            cleanup(e);
        }
        Some(true) => {
            // already deleted: just move over it
            e.cursor = if backwards { i } else { i + 1 };
        }
        None => {
            if !e.doc.els[i].is_char() && !matches!(e.doc.els[i], El::Code(Code::HRt | Code::Tab | Code::HardSpace)) {
                e.delete_at(i);
                return;
            }
            // extend an adjacent deletion run if there is one
            let ci;
            if i + 1 < e.doc.els.len() && matches!(e.doc.els[i + 1], El::Code(Code::RevOn { del: true, .. })) {
                let start = e.doc.els.remove(i + 1);
                e.doc.els.insert(i, start);
                ci = i + 1;
            } else if i > 0 && matches!(e.doc.els[i - 1], El::Code(Code::RevOff { del: true })) {
                let end = e.doc.els.remove(i - 1);
                e.doc.els.insert(i, end);
                ci = i - 1;
                // merge with a following deletion run: [del][DEL] → nothing
                if i + 2 < e.doc.els.len()
                    && matches!(e.doc.els[i + 1], El::Code(Code::RevOn { del: true, .. }))
                    && matches!(e.doc.els[i], El::Code(Code::RevOff { del: true }))
                {
                    e.doc.els.drain(i..i + 2);
                }
            } else {
                e.doc.els.insert(i + 1, off(true));
                e.doc.els.insert(i, on(true));
                ci = i + 1;
            }
            // bounds of the run now containing the character
            let start = (0..=ci).rev().find(|&k| matches!(e.doc.els[k], El::Code(Code::RevOn { del: true, .. }))).unwrap_or(ci);
            let end = (ci..e.doc.els.len()).find(|&k| matches!(e.doc.els[k], El::Code(Code::RevOff { del: true }))).unwrap_or(ci);
            e.cursor = if backwards { start } else { (end + 1).min(e.doc.els.len()) };
            e.modified = true;
            e.relayout();
        }
    }
}

/// Delete a block with track changes on.
pub fn delete_block_tracked(e: &mut Editor) {
    let Some((s, t)) = e.block_range() else { return };
    e.block_anchor = None;
    let mut out: Vec<El> = Vec::new();
    let els = std::mem::take(&mut e.doc.els);
    let mut state = rev_at(&els, s);
    let mut opened = false;
    for (k, el) in els.into_iter().enumerate() {
        if k == s {
            if state.is_none() {
                out.push(on(true));
                opened = true;
            }
        }
        if k >= s && k < t {
            match &el {
                El::Code(Code::RevOn { del, .. }) => state = Some(*del),
                El::Code(Code::RevOff { .. }) => state = None,
                _ => {}
            }
            // drop our own insertions entirely
            if state == Some(false) || matches!(el, El::Code(Code::RevOn { del: false, .. }) | El::Code(Code::RevOff { del: false })) {
                if matches!(el, El::Code(Code::RevOff { del: false })) && opened {
                    // continue deletion run
                }
                continue;
            }
            out.push(el);
            continue;
        }
        if k == t && opened {
            out.push(off(true));
            opened = false;
        }
        out.push(el);
    }
    if opened {
        out.push(off(true));
    }
    e.doc.els = out;
    e.cursor = s.min(e.doc.els.len());
    cleanup(e);
}

/// Remove empty runs like [INS][ins].
fn cleanup(e: &mut Editor) {
    let mut i = 0;
    while i + 1 < e.doc.els.len() {
        if let (El::Code(Code::RevOn { del: a, .. }), El::Code(Code::RevOff { del: b })) = (&e.doc.els[i], &e.doc.els[i + 1]) {
            if a == b {
                e.doc.els.drain(i..i + 2);
                if e.cursor > i {
                    e.cursor = e.cursor.saturating_sub(2).max(i);
                }
                continue;
            }
        }
        i += 1;
    }
    e.modified = true;
    e.relayout();
}

/// Accept (true) or reject (false) the changes in `from..to` (element range,
/// whole runs that overlap it).
fn resolve(e: &mut Editor, from: usize, to: usize, accept: bool) -> usize {
    let els = std::mem::take(&mut e.doc.els);
    // find runs
    let mut runs: Vec<(usize, usize, bool)> = vec![];
    let mut start: Option<(usize, bool)> = None;
    for (k, el) in els.iter().enumerate() {
        match el {
            El::Code(Code::RevOn { del, .. }) => start = Some((k, *del)),
            El::Code(Code::RevOff { .. }) => {
                if let Some((s, d)) = start.take() {
                    runs.push((s, k, d));
                }
            }
            _ => {}
        }
    }
    let chosen: Vec<(usize, usize, bool)> = runs.into_iter().filter(|&(s, t, _)| t >= from && s <= to).collect();
    let mut drop = vec![false; els.len()];
    for &(s, t, del) in &chosen {
        drop[s] = true;
        drop[t] = true;
        // accepting a deletion or rejecting an insertion removes the text
        if del == accept {
            for d in drop.iter_mut().take(t).skip(s + 1) {
                *d = true;
            }
        }
    }
    let n = chosen.len();
    let mut new_cursor = e.cursor;
    let mut out = Vec::with_capacity(els.len());
    for (k, el) in els.into_iter().enumerate() {
        if k == e.cursor {
            new_cursor = out.len();
        }
        if !drop[k] {
            out.push(el);
        }
    }
    if e.cursor >= drop.len() {
        new_cursor = out.len();
    }
    e.doc.els = out;
    e.cursor = new_cursor.min(e.doc.els.len());
    e.doc.normalize_attrs_keep(&mut e.cursor);
    e.modified = true;
    e.relayout();
    n
}

/// Position of the next change start after `from` (wrapping).
fn next_change(e: &Editor, from: usize) -> Option<usize> {
    let starts: Vec<usize> = e.doc.els.iter().enumerate().filter(|(_, x)| matches!(x, El::Code(Code::RevOn { .. }))).map(|(k, _)| k).collect();
    starts.iter().copied().find(|&k| k >= from).or(starts.first().copied())
}

pub fn count_changes(doc: &Document) -> usize {
    doc.els.iter().filter(|x| matches!(x, El::Code(Code::RevOn { .. }))).count()
}

/// The Track Changes menu (Alt-F5, 7).
pub fn menu(wp: &mut Wp) {
    let state = if wp.ed_ref().doc.track { "ON" } else { "off" };
    let n = count_changes(&wp.ed_ref().doc);
    let m = MenuLine::new(
        &["~Track On/Off", "~Next Change", "~Accept", "~Reject", "Accept A~ll", "Reject All (~Undo)", "~Compare with Disk Document"],
        |wp, k| {
            match k {
                0 => {
                    let e = wp.ed();
                    e.doc.track = !e.doc.track;
                    e.modified = true;
                    let s = if e.doc.track { "Track changes ON — insertions and deletions are marked" } else { "Track changes off" };
                    wp.flash(s);
                }
                1 => {
                    let e = wp.ed();
                    match next_change(e, e.cursor + 1) {
                        Some(k) => {
                            e.cursor = k + 1;
                        }
                        None => wp.flash("* No changes *"),
                    }
                }
                2 | 3 => {
                    let e = wp.ed();
                    let (a, b) = e.block_range().unwrap_or((e.cursor.saturating_sub(1), e.cursor));
                    e.block_anchor = None;
                    let n = resolve(e, a, b, k == 2);
                    if n == 0 {
                        wp.flash("* No change at the cursor *");
                    } else if let Some(nx) = next_change(wp.ed_ref(), wp.ed_ref().cursor) {
                        wp.ed().cursor = nx + 1;
                    }
                }
                4 | 5 => {
                    let accept = k == 4;
                    return Some(YesNo::new(if accept { "Accept all changes?" } else { "Reject all changes?" }, false, move |wp, yes| {
                        if yes {
                            let e = wp.ed();
                            let n = resolve(e, 0, usize::MAX, accept);
                            wp.flash(&format!("{n} change(s) {}", if accept { "accepted" } else { "rejected" }));
                        }
                        None
                    }) as Box<dyn crate::ui::Screen>);
                }
                _ => {
                    return Some(Prompt::new("Other Document:", "", |wp, s| {
                        let s = s?;
                        let p = wp.resolve_path(&s);
                        match crate::fileio::load(&p) {
                            Ok(old) => compare_into_current(wp, &old),
                            Err(e) => wp.flash(&format!("ERROR: {e}")),
                        }
                        None
                    }) as Box<dyn crate::ui::Screen>);
                }
            }
            wp.after_edit();
            None
        },
    )
    .prefix(&format!("Track ({state}, {n} changes): "));
    wp.push(m);
}

// ----------------------------------------------------------- compare

/// Tokens: words, single whitespace/punctuation chars, or single codes.
fn tokens(els: &[El]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while i < els.len() {
        let start = i;
        match &els[i] {
            El::Ch(c) if c.is_alphanumeric() => {
                while i < els.len() && matches!(&els[i], El::Ch(c) if c.is_alphanumeric()) {
                    i += 1;
                }
            }
            _ => i += 1,
        }
        out.push((start, i));
    }
    out
}

/// Myers diff on token sequences; returns edit script as (op, a_range or b_range):
/// op 0 = equal (a token idx), 1 = delete (a idx), 2 = insert (b idx).
fn diff(a: &[&[El]], b: &[&[El]]) -> Vec<(u8, usize)> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let max = (n + m) as usize;
    let off = max as isize;
    let mut v = vec![0isize; 2 * max + 2];
    let mut trace: Vec<Vec<isize>> = vec![];
    'outer: for d in 0..=max as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[(k - 1 + off) as usize] < v[(k + 1 + off) as usize]) {
                v[(k + 1 + off) as usize]
            } else {
                v[(k - 1 + off) as usize] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(k + off) as usize] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
        if trace.len() > 20000 {
            break;
        }
    }
    // backtrack
    let mut ops = vec![];
    let (mut x, mut y) = (n, m);
    for (d, vv) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let k = x - y;
        let prev_k = if k == -d || (k != d && vv[(k - 1 + off) as usize] < vv[(k + 1 + off) as usize]) { k + 1 } else { k - 1 };
        let prev_x = vv[(prev_k + off) as usize];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push((0, x as usize));
        }
        if d > 0 {
            if x == prev_x {
                y -= 1;
                ops.push((2, y as usize));
            } else {
                x -= 1;
                ops.push((1, x as usize));
            }
        }
        x = prev_x;
        y = prev_y;
        if x <= 0 && y <= 0 {
            break;
        }
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        ops.push((0, x as usize));
    }
    ops.reverse();
    ops
}

/// Compare `old` with the current text and mark the differences as tracked
/// changes in the current document (WP's Document Compare, Word-style).
pub fn compare_into_current(wp: &mut Wp, old: &Document) {
    let cur = wp.ed_ref().doc.els.clone();
    let strip = |els: &[El]| -> Vec<El> { els.iter().filter(|e| !matches!(e, El::Code(Code::RevOn { .. } | Code::RevOff { .. }))).cloned().collect() };
    let a_els = strip(&old.els);
    let b_els = strip(&cur);
    let ta = tokens(&a_els);
    let tb = tokens(&b_els);
    let a: Vec<&[El]> = ta.iter().map(|&(s, t)| &a_els[s..t]).collect();
    let b: Vec<&[El]> = tb.iter().map(|&(s, t)| &b_els[s..t]).collect();
    if a.len() + b.len() > 400_000 {
        wp.flash("ERROR: Documents too large to compare");
        return;
    }
    let ops = diff(&a, &b);
    let mut out: Vec<El> = vec![];
    let mut state: u8 = 0; // 0 none, 1 del, 2 ins
    let mut changes = 0;
    let set = |out: &mut Vec<El>, state: &mut u8, want: u8, changes: &mut usize| {
        if *state == want {
            return;
        }
        match *state {
            1 => out.push(off(true)),
            2 => out.push(off(false)),
            _ => {}
        }
        match want {
            1 => {
                out.push(on(true));
                *changes += 1;
            }
            2 => {
                out.push(on(false));
                *changes += 1;
            }
            _ => {}
        }
        *state = want;
    };
    for (op, idx) in ops {
        match op {
            0 => {
                set(&mut out, &mut state, 0, &mut changes);
                out.extend_from_slice(a[idx]);
            }
            1 => {
                set(&mut out, &mut state, 1, &mut changes);
                out.extend_from_slice(a[idx]);
            }
            _ => {
                set(&mut out, &mut state, 2, &mut changes);
                out.extend_from_slice(b[idx]);
            }
        }
    }
    set(&mut out, &mut state, 0, &mut changes);
    let e = wp.ed();
    e.doc.els = out;
    e.cursor = 0;
    e.modified = true;
    e.relayout();
    e.settle();
    wp.flash(&format!("Compare: {changes} change(s) marked — Alt-F5, 7 to accept or reject"));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ed(s: &str) -> Editor {
        Editor::with_doc(Document::from_text(s), None)
    }
    #[test]
    fn typing_and_deleting_tracked() {
        let mut e = ed("abc");
        e.cursor = 3;
        insert_tracked(&mut e, El::Ch('d'));
        insert_tracked(&mut e, El::Ch('e'));
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "abcde");
        assert_eq!(count_changes(&e.doc), 1);
        // delete 'b' tracked
        delete_tracked(&mut e, 1, true);
        assert_eq!(count_changes(&e.doc), 2);
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "abcde"); // still there, struck out
        // accept all: b disappears, de stays
        let n = resolve(&mut e, 0, usize::MAX, true);
        assert_eq!(n, 2);
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "acde");
        assert_eq!(count_changes(&e.doc), 0);
    }
    #[test]
    fn backspace_builds_one_run() {
        let mut e = ed("abcdef");
        e.cursor = 6;
        for _ in 0..3 {
            let t = e.backspace_target().unwrap();
            delete_tracked(&mut e, t, true);
        }
        assert_eq!(count_changes(&e.doc), 1, "{:?}", e.doc.els);
        let n = resolve(&mut e, 0, usize::MAX, true);
        assert_eq!(n, 1);
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "abc");
    }

    #[test]
    fn reject_restores() {
        let mut e = ed("abc");
        e.cursor = 1;
        insert_tracked(&mut e, El::Ch('X'));
        let last = e.doc.els.len() - 1;
        delete_tracked(&mut e, last, true);
        resolve(&mut e, 0, usize::MAX, false);
        assert_eq!(e.doc.plain_text(0, e.doc.len()), "abc");
    }
    #[test]
    fn compare_marks() {
        let old = Document::from_text("Der Vertrag ist gültig.");
        let new = Document::from_text("Der neue Vertrag ist ungültig.");
        let a_els = old.els.clone();
        let b_els = new.els.clone();
        let ta = tokens(&a_els);
        let tb = tokens(&b_els);
        let a: Vec<&[El]> = ta.iter().map(|&(s, t)| &a_els[s..t]).collect();
        let b: Vec<&[El]> = tb.iter().map(|&(s, t)| &b_els[s..t]).collect();
        let ops = diff(&a, &b);
        let ins = ops.iter().filter(|o| o.0 == 2).count();
        let del = ops.iter().filter(|o| o.0 == 1).count();
        assert!(ins >= 2 && del >= 1, "{ops:?}");
    }
}

#[cfg(test)]
mod local_date_tests {
    #[test]
    fn plain_dates_pass_through() {
        assert_eq!(super::local_date("2026-11-12"), "2026-11-12");
        assert_eq!(super::local_date("2026-11-12T00:00:00"), "2026-11-12");
        assert_eq!(super::local_date("junk"), "junk");
        // UTC noon is the same calendar day in every inhabited time zone
        assert_eq!(super::local_date("2026-07-01T12:00:00Z"), "2026-07-01");
    }
}
