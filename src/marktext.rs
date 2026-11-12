//! Mark Text (Alt-F5): tables of contents, lists, indexes, tables of
//! authorities, cross-references and master documents — marking, defining
//! and generating, as in WordPerfect 5.1.
//!
//! Generation fills the text between each `[Def Mark]` and its `[End Def]`
//! with entries and real page numbers taken from the layout, and updates
//! every `[Ref]` code.  Because the inserted text can move pages, it is
//! repeated until the result no longer changes (WP's "passes").

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::Layout;
use crate::screens::*;
use crate::ui::{HitAction, Outcome, Screen, Wp};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

// ================================================================ engine

/// Page-number style names (Mark Text definitions).
pub const STYLE_NAMES: [&str; 5] = ["No page number", "Page # follows entry", "(Page #) follows entry", "Flush right", "Flush right with leader"];

#[derive(Debug, Default, Clone, PartialEq)]
pub struct GenStats {
    /// Definitions ([Def Mark]) filled.
    pub regions: usize,
    /// Cross-references updated.
    pub refs: usize,
    /// Reference names without a target.
    pub unresolved: Vec<String>,
    pub passes: usize,
}

fn is_def(e: &El) -> bool {
    matches!(e, El::Code(Code::Mark(Mark::DefToc { .. } | Mark::DefList { .. } | Mark::DefIndex { .. } | Mark::DefToA { .. })))
}

fn is_end_def(e: &El) -> bool {
    matches!(e, El::Code(Code::Mark(Mark::EndDef)))
}

/// Printed page number of every element (honouring `[Pg Num:n]`).
pub fn printed_pages(doc: &Document, lay: &Layout) -> Vec<i64> {
    let n = doc.els.len();
    let mut out = vec![1i64; n + 1];
    let mut off: i64 = 0;
    for line in &lay.lines {
        for k in line.start..line.end.min(n) {
            if let El::Code(Code::PageNumber(p)) = &doc.els[k] {
                off = *p as i64 - line.page as i64;
            }
            out[k] = line.page as i64 + off;
        }
    }
    if n > 0 {
        out[n] = out[n - 1];
    }
    out
}

/// Text of marked elements: characters, spaces for tabs/returns.
fn plain_of(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(Code::HardSpace | Code::HRt | Code::Tab | Code::Indent | Code::DblIndent | Code::HPg) => s.push(' '),
            El::Code(Code::HardHyphen) => s.push('-'),
            _ => {}
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_els(s: &str) -> Vec<El> {
    Document::from_text(s).els
}

fn pages_str(p: &BTreeSet<i64>) -> String {
    p.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
}

/// The page-number part after an entry, by style (0..4).
fn page_part(style: u8, pages: &str) -> Vec<El> {
    if pages.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    match style {
        0 => {}
        1 => {
            out.push(El::Ch(' '));
            out.extend(text_els(pages));
        }
        2 => {
            out.extend(text_els(&format!(" ({pages})")));
        }
        3 => {
            out.push(El::Code(Code::FlushRight));
            out.extend(text_els(pages));
        }
        _ => {
            out.push(El::Code(Code::FlushRightDot));
            out.extend(text_els(pages));
        }
    }
    out
}

#[derive(Default)]
struct Collected {
    /// (level, text, page) — ToC
    toc: Vec<(u8, String, i64)>,
    /// list number → (text, page)
    lists: BTreeMap<u8, Vec<(String, i64)>>,
    /// (heading, subheading, page)
    index: Vec<(String, String, i64)>,
    /// key (lower-case short form) → (section, full form)
    authorities: HashMap<String, (u8, String)>,
    /// (key, page)
    toa_refs: Vec<(String, i64)>,
    /// target name → element index (a note index if the target is inside a note)
    targets: HashMap<String, usize>,
}

fn find_targets(els: &[El], at: Option<usize>, out: &mut HashMap<String, usize>) {
    for (i, e) in els.iter().enumerate() {
        match e {
            El::Code(Code::Mark(Mark::Target { name })) => {
                out.entry(name.clone()).or_insert(at.unwrap_or(i));
            }
            El::Code(Code::Note { els: nels, .. }) if at.is_none() => find_targets(nels, Some(i), out),
            _ => {}
        }
    }
}

fn collect(doc: &Document, pages: &[i64], in_region: &[bool], conc: &[String]) -> Collected {
    let els = &doc.els;
    let n = els.len();
    let mut c = Collected::default();
    for i in 0..n {
        if in_region[i] {
            continue;
        }
        let El::Code(Code::Mark(m)) = &els[i] else { continue };
        match m {
            Mark::Begin { list, n: lv } => {
                let end = (i + 1..n).find(|&j| matches!(&els[j], El::Code(Code::Mark(Mark::End { list: l2, n: n2 })) if l2 == list && n2 == lv));
                let Some(j) = end else { continue };
                let text = plain_of(&els[i + 1..j]);
                if *list {
                    c.lists.entry(*lv).or_default().push((text, pages[i]));
                } else {
                    c.toc.push((*lv, text, pages[i]));
                }
            }
            Mark::Index { heading, sub } => c.index.push((heading.clone(), sub.clone(), pages[i])),
            Mark::ToA { section, short, full } => {
                let key = if short.trim().is_empty() { full.trim().to_lowercase() } else { short.trim().to_lowercase() };
                if *section > 0 {
                    c.authorities.entry(key.clone()).or_insert((*section, full.clone()));
                }
                c.toa_refs.push((key, pages[i]));
            }
            _ => {}
        }
    }
    find_targets(els, None, &mut c.targets);
    // concordance phrases: every occurrence in the text is an index entry
    if !conc.is_empty() {
        let mut chars: Vec<(char, usize)> = vec![];
        for (i, e) in els.iter().enumerate() {
            if in_region[i] {
                chars.push(('\n', i));
                continue;
            }
            match e {
                El::Ch(ch) => chars.push((ch.to_lowercase().next().unwrap_or(*ch), i)),
                El::Code(Code::HardSpace | Code::HRt | Code::Tab | Code::Indent | Code::DblIndent) => chars.push((' ', i)),
                _ => {}
            }
        }
        for phrase in conc {
            let p: Vec<char> = phrase.trim().to_lowercase().chars().collect();
            if p.is_empty() {
                continue;
            }
            let mut k = 0;
            while k + p.len() <= chars.len() {
                let hit = (0..p.len()).all(|q| chars[k + q].0 == p[q])
                    && (k == 0 || !chars[k - 1].0.is_alphanumeric())
                    && chars.get(k + p.len()).is_none_or(|x| !x.0.is_alphanumeric());
                if hit {
                    c.index.push((phrase.trim().to_string(), String::new(), pages[chars[k].1]));
                    k += p.len();
                } else {
                    k += 1;
                }
            }
        }
    }
    c
}

fn toc_els(levels: u8, wrap_last: bool, styles: &[u8; 5], entries: &[(u8, String, i64)]) -> Vec<El> {
    let mut out = vec![];
    let mut k = 0;
    while k < entries.len() {
        let (lv, text, pg) = &entries[k];
        if *lv == 0 || *lv > levels {
            k += 1;
            continue;
        }
        let style = styles[(*lv as usize - 1).min(4)];
        for _ in 1..*lv {
            out.push(El::Code(Code::Indent));
        }
        if wrap_last && levels > 1 && *lv == levels {
            let mut first = true;
            while k < entries.len() && entries[k].0 == levels {
                if !first {
                    out.extend(text_els("; "));
                }
                out.extend(text_els(&entries[k].1));
                out.extend(page_part(style.min(2), &entries[k].2.to_string()));
                first = false;
                k += 1;
            }
            out.push(El::Code(Code::HRt));
            continue;
        }
        out.extend(text_els(text));
        out.extend(page_part(style, &pg.to_string()));
        out.push(El::Code(Code::HRt));
        k += 1;
    }
    out
}

fn index_els(style: u8, entries: &[(String, String, i64)]) -> Vec<El> {
    // heading (lower) → (shown heading, pages, sub (lower) → (shown, pages))
    type Subs = BTreeMap<String, (String, BTreeSet<i64>)>;
    let mut h: BTreeMap<String, (String, BTreeSet<i64>, Subs)> = BTreeMap::new();
    for (head, sub, pg) in entries {
        if head.trim().is_empty() {
            continue;
        }
        let e = h.entry(head.trim().to_lowercase()).or_insert_with(|| (head.trim().to_string(), BTreeSet::new(), BTreeMap::new()));
        if sub.trim().is_empty() {
            e.1.insert(*pg);
        } else {
            e.2.entry(sub.trim().to_lowercase()).or_insert_with(|| (sub.trim().to_string(), BTreeSet::new())).1.insert(*pg);
        }
    }
    let mut out = vec![];
    for (_, (shown, pages, subs)) in h {
        out.extend(text_els(&shown));
        out.extend(page_part(style, &pages_str(&pages)));
        out.push(El::Code(Code::HRt));
        for (_, (s, p)) in subs {
            out.push(El::Code(Code::Indent));
            out.extend(text_els(&s));
            out.extend(page_part(style, &pages_str(&p)));
            out.push(El::Code(Code::HRt));
        }
    }
    out
}

fn toa_els(section: u8, dots: bool, blank: bool, c: &Collected) -> Vec<El> {
    let mut list: Vec<(&String, &String)> = c.authorities.iter().filter(|(_, (s, _))| *s == section).map(|(k, (_, f))| (k, f)).collect();
    list.sort_by_key(|(_, f)| f.to_lowercase());
    let mut out = vec![];
    for (key, full) in list {
        let pages: BTreeSet<i64> = c.toa_refs.iter().filter(|(k, _)| k == key).map(|(_, p)| *p).collect();
        out.extend(text_els(full.trim_end()));
        out.extend(page_part(if dots { 4 } else { 3 }, &pages_str(&pages)));
        out.push(El::Code(Code::HRt));
        if blank {
            out.push(El::Code(Code::HRt));
        }
    }
    out
}

fn ref_value(doc: &Document, lay: &Layout, pages: &[i64], c: &Collected, name: &str, kind: u8) -> Option<String> {
    let t = *c.targets.get(name)?;
    let els = &doc.els;
    match kind {
        0 => Some(pages[t].to_string()),
        1 => {
            let n = els[..t].iter().filter(|e| matches!(e, El::Code(Code::Unknown { name, .. }) if name.starts_with("Par Num"))).count();
            (n > 0).then(|| n.to_string())
        }
        2 | 3 => {
            let nk = if kind == 2 { NoteKind::Foot } else { NoteKind::End };
            let best = els
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e, El::Code(Code::Note { kind: k, .. }) if *k == nk))
                .min_by_key(|(i, _)| (*i as isize - t as isize).unsigned_abs())
                .map(|(i, _)| i)?;
            lay.note_nums.get(&best).map(|n| n.to_string())
        }
        _ => {
            let prefix = ["Figure", "Tbl Box", "Text Box", "Usr Box", "Equation"][(kind as usize - 4).min(4)];
            let n = els[..=t.min(els.len().saturating_sub(1))]
                .iter()
                .filter(|e| matches!(e, El::Code(Code::Unknown { name, .. }) if name.starts_with(prefix)))
                .count();
            (n > 0).then(|| n.to_string())
        }
    }
}

/// One generation pass: the new element stream.
fn generate_once(doc: &Document, lay: &Layout, conc: &HashMap<String, Vec<String>>, stats: &mut GenStats) -> Vec<El> {
    let els = &doc.els;
    let n = els.len();
    let pages = printed_pages(doc, lay);
    let mut regions: Vec<(usize, Option<usize>)> = vec![];
    let mut in_region = vec![false; n];
    for i in 0..n {
        if !is_def(&els[i]) {
            continue;
        }
        let mut end = None;
        for j in i + 1..n {
            if is_def(&els[j]) {
                break;
            }
            if is_end_def(&els[j]) {
                end = Some(j);
                break;
            }
        }
        if let Some(e) = end {
            for r in in_region.iter_mut().take(e).skip(i + 1) {
                *r = true;
            }
        }
        regions.push((i, end));
    }
    let phrases: Vec<String> = els
        .iter()
        .filter_map(|e| match e {
            El::Code(Code::Mark(Mark::DefIndex { concordance, .. })) if !concordance.is_empty() => conc.get(concordance).cloned(),
            _ => None,
        })
        .flatten()
        .collect();
    let c = collect(doc, &pages, &in_region, &phrases);
    *stats = GenStats { passes: stats.passes, ..Default::default() };
    let mut out = Vec::with_capacity(n + 64);
    let mut i = 0;
    let mut ri = 0;
    while i < n {
        if let Some(&(d, e)) = regions.get(ri).filter(|r| r.0 == i) {
            ri += 1;
            stats.regions += 1;
            out.push(els[d].clone());
            let generated = match &els[d] {
                El::Code(Code::Mark(Mark::DefToc { levels, wrap_last, styles })) => toc_els(*levels, *wrap_last, styles, &c.toc),
                El::Code(Code::Mark(Mark::DefList { n: ln, style })) => {
                    let entries: Vec<(u8, String, i64)> = c.lists.get(ln).map(|v| v.iter().map(|(t, p)| (1, t.clone(), *p)).collect()).unwrap_or_default();
                    toc_els(1, false, &[*style; 5], &entries)
                }
                El::Code(Code::Mark(Mark::DefIndex { style, .. })) => index_els(*style, &c.index),
                El::Code(Code::Mark(Mark::DefToA { section, dots, blank, .. })) => toa_els(*section, *dots, *blank, &c),
                _ => vec![],
            };
            out.extend(generated);
            match e {
                Some(e) => i = e,
                None => {
                    out.push(El::Code(Code::Mark(Mark::EndDef)));
                    i = d + 1;
                }
            }
            continue;
        }
        match &els[i] {
            El::Code(Code::Mark(Mark::Ref { name, kind, text })) if !in_region[i] => {
                stats.refs += 1;
                let v = ref_value(doc, lay, &pages, &c, name, *kind);
                if v.is_none() && !stats.unresolved.contains(name) {
                    stats.unresolved.push(name.clone());
                }
                let v = v.unwrap_or_else(|| "?".into());
                let _ = text;
                out.push(El::Code(Code::Mark(Mark::Ref { name: name.clone(), kind: *kind, text: v })));
            }
            e => out.push(e.clone()),
        }
        i += 1;
    }
    out
}

/// Generate tables, lists, indexes and cross-references.  `lay` lays out a
/// document; `conc` loads a concordance file (one phrase per line).
pub fn generate_with(doc: &mut Document, lay: &mut dyn FnMut(&Document) -> Layout, conc: &dyn Fn(&str) -> Option<Vec<String>>) -> GenStats {
    let mut conc_cache: HashMap<String, Vec<String>> = HashMap::new();
    for e in &doc.els {
        if let El::Code(Code::Mark(Mark::DefIndex { concordance, .. })) = e {
            if !concordance.is_empty() && !conc_cache.contains_key(concordance) {
                conc_cache.insert(concordance.clone(), conc(concordance).unwrap_or_default());
            }
        }
    }
    let mut stats = GenStats::default();
    for pass in 1..=6 {
        stats.passes = pass;
        let l = lay(doc);
        let new = generate_once(doc, &l, &conc_cache, &mut stats);
        if new == doc.els {
            break;
        }
        doc.els = new;
    }
    stats
}

/// Generate → 1: remove redline markings, delete strikeout text, and accept
/// tracked changes (insertions stay, deletions go).
pub fn remove_redline(els: &[El]) -> Vec<El> {
    let mut out = Vec::with_capacity(els.len());
    let mut strike = false;
    let mut deleted = false;
    for e in els {
        match e {
            El::Code(Code::AttrOn(Attr::Redline) | Code::AttrOff(Attr::Redline)) => {}
            El::Code(Code::AttrOn(Attr::Strikeout)) => strike = true,
            El::Code(Code::AttrOff(Attr::Strikeout)) => strike = false,
            El::Code(Code::RevOn { del, .. }) => deleted = *del,
            El::Code(Code::RevOff { .. }) => deleted = false,
            _ if strike || deleted => {}
            _ => out.push(e.clone()),
        }
    }
    out
}

/// Generate → 3: replace each `[Subdoc:FILE]` by the file's text between
/// `[Subdoc Start]` / `[Subdoc End]`.  Returns (expanded, errors).
pub fn expand_master(els: &[El], load: &dyn Fn(&str) -> Option<Document>) -> (Vec<El>, usize, Vec<String>) {
    let mut out = Vec::with_capacity(els.len());
    let mut n = 0;
    let mut errs = vec![];
    for e in els {
        if let El::Code(Code::Mark(Mark::Subdoc(f))) = e {
            match load(f) {
                Some(d) => {
                    out.push(El::Code(Code::Mark(Mark::SubdocStart(f.clone()))));
                    out.extend(d.initial_codes.iter().cloned());
                    out.extend(d.els);
                    out.push(El::Code(Code::Mark(Mark::SubdocEnd(f.clone()))));
                    n += 1;
                }
                None => {
                    errs.push(f.clone());
                    out.push(e.clone());
                }
            }
            continue;
        }
        out.push(e.clone());
    }
    (out, n, errs)
}

/// Generate → 4: collapse expanded subdocuments back into `[Subdoc]`
/// codes; `save` is called with each subdocument's text first.
pub fn condense_master(els: &[El], save: &mut dyn FnMut(&str, &[El])) -> (Vec<El>, usize) {
    let mut out = Vec::with_capacity(els.len());
    let mut n = 0;
    let mut i = 0;
    while i < els.len() {
        if let El::Code(Code::Mark(Mark::SubdocStart(f))) = &els[i] {
            let end = (i + 1..els.len()).find(|&j| matches!(&els[j], El::Code(Code::Mark(Mark::SubdocEnd(g))) if g == f));
            if let Some(j) = end {
                save(f, &els[i + 1..j]);
                out.push(El::Code(Code::Mark(Mark::Subdoc(f.clone()))));
                n += 1;
                i = j + 1;
                continue;
            }
        }
        out.push(els[i].clone());
        i += 1;
    }
    (out, n)
}

// ==================================================================== UI

fn path_for(wp: &Wp, name: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(name.trim());
    if p.is_relative() {
        if let Some(dir) = wp.ed_ref().path.as_ref().and_then(|p| p.parent()) {
            let cand = dir.join(p);
            if cand.exists() || !wp.resolve_path(name).exists() {
                return cand;
            }
        }
    }
    wp.resolve_path(name)
}

fn mk(m: Mark) -> Code {
    Code::Mark(m)
}

fn ins(wp: &mut Wp, m: Mark) {
    wp.ed().insert_code(mk(m));
    wp.after_edit();
}

/// The word at (or just before) the cursor — default index heading.
fn word_at_cursor(wp: &Wp) -> String {
    let e = wp.ed_ref();
    let els = &e.doc.els;
    let isw = |k: usize| matches!(els.get(k), Some(El::Ch(c)) if c.is_alphanumeric() || *c == '-' || *c == '\'');
    let mut a = e.cursor.min(els.len());
    if !isw(a) && a > 0 && isw(a - 1) {
        a -= 1;
    }
    if !isw(a) {
        return String::new();
    }
    let mut s = a;
    while s > 0 && isw(s - 1) {
        s -= 1;
    }
    let mut t = a;
    while isw(t) {
        t += 1;
    }
    plain_of(&els[s..t])
}

/// Insert a paired mark around the block.
fn mark_block(wp: &mut Wp, list: bool, n: u8) {
    let e = wp.ed();
    let Some((s, t)) = e.block_range() else { return };
    e.block_anchor = None;
    e.cursor = t;
    e.insert_code(mk(Mark::End { list, n }));
    e.cursor = s;
    e.insert_code(mk(Mark::Begin { list, n }));
    e.cursor = t + 2;
    wp.after_edit();
}

fn index_prompts(default: String, at: Option<usize>) -> Next {
    let shown = default.clone();
    Some(Prompt::new("Index heading:", &shown, move |_wp, h| {
        let h = h?;
        let h = if h.trim().is_empty() { default.clone() } else { h };
        if h.trim().is_empty() {
            return None;
        }
        Some(Prompt::new("Subheading:", "", move |wp, s| {
            let s = s?;
            if let Some(at) = at {
                wp.ed().cursor = at;
            }
            ins(wp, Mark::Index { heading: h.trim().to_string(), sub: s.trim().to_string() });
            None
        }) as Box<dyn Screen>)
    }) as Box<dyn Screen>)
}

fn last_short_form(wp: &Wp) -> String {
    let e = wp.ed_ref();
    e.doc.els[..e.cursor.min(e.doc.els.len())]
        .iter()
        .rev()
        .find_map(|x| match x {
            El::Code(Code::Mark(Mark::ToA { short, .. })) => Some(short.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn short_form_prompt(default: String, at: usize, section: u8, full: String) -> Box<dyn Screen> {
    Prompt::new("Short Form:", &default, move |wp, s| {
        let s = s?;
        if s.trim().is_empty() && full.trim().is_empty() {
            return None;
        }
        let e = wp.ed();
        e.cursor = at.min(e.doc.els.len());
        ins(wp, Mark::ToA { section, short: s.trim().to_string(), full });
        None
    })
}

/// Alt-F5 with Block on: `Mark for: 1 ToC; 2 List; 3 Index; 4 ToA`.
fn block_menu(wp: &mut Wp) {
    wp.push(
        MenuLine::new(&["To~C", "~List", "~Index", "To~A"], |wp, k| match k {
            0 => Some(Prompt::new("ToC Level:", "", |wp, s| {
                let n: u8 = s?.trim().parse().ok().filter(|n| (1..=5).contains(n))?;
                mark_block(wp, false, n);
                None
            }) as Box<dyn Screen>),
            1 => Some(Prompt::new("List Number:", "", |wp, s| {
                let n: u8 = s?.trim().parse().ok().filter(|n| (1..=10).contains(n))?;
                mark_block(wp, true, n);
                None
            }) as Box<dyn Screen>),
            2 => {
                let e = wp.ed();
                let (s, _) = e.block_range()?;
                let text = plain_of(&e.block_els().unwrap_or_default());
                e.block_anchor = None;
                index_prompts(text, Some(s))
            }
            _ => {
                let e = wp.ed();
                let (s, _) = e.block_range()?;
                let block = e.block_els().unwrap_or_default();
                e.block_anchor = None;
                Some(Prompt::new("ToA Section Number (Press Enter for Short Form only):", "", move |wp, sec| {
                    let sec = sec?;
                    let text = plain_of(&block);
                    let short_default: String = text.chars().take(40).collect();
                    match sec.trim().parse::<u8>().ok().filter(|n| (1..=16).contains(n)) {
                        None if sec.trim().is_empty() => Some(short_form_prompt(short_default, s, 0, String::new())),
                        None => None,
                        Some(n) => {
                            // edit the full form (WP: "ToA Full Form" editing screen)
                            let full_els: Vec<El> = block.iter().filter(|e| e.is_char() || matches!(e, El::Code(Code::HRt | Code::HardSpace | Code::Tab))).cloned().collect();
                            crate::subedit::open(wp, "ToA Full Form", full_els, false, move |wp, els| {
                                let full = crate::comments::els_to_text(&els);
                                wp.push(short_form_prompt(short_default, s, n, full));
                            });
                            None
                        }
                    }
                }) as Box<dyn Screen>)
            }
        })
        .prefix("Mark for: "),
    );
}

/// Alt-F5.
pub fn menu(wp: &mut Wp) {
    if wp.ed_ref().block_anchor.is_some() {
        block_menu(wp);
        return;
    }
    wp.push(MenuLine::new(
        &["Cross-~Ref", "~Subdoc", "~Index", "To~A Short Form", "~Define", "~Generate", "~Track Changes"],
        |wp, k| match k {
            0 => Some(cross_ref_screen()),
            1 => Some(Prompt::new("Subdoc Filename:", "", |wp, s| {
                let s = s?;
                if !s.trim().is_empty() {
                    ins(wp, Mark::Subdoc(s.trim().to_string()));
                }
                None
            }) as Box<dyn Screen>),
            2 => index_prompts(word_at_cursor(wp), None),
            3 => {
                let d = last_short_form(wp);
                let at = wp.ed_ref().cursor;
                Some(short_form_prompt(d, at, 0, String::new()))
            }
            4 => Some(define_screen()),
            5 => Some(generate_screen()),
            _ => {
                crate::track::menu(wp);
                None
            }
        },
    ));
}

// ------------------------------------------------- full-screen choices

/// A WP full-screen numbered menu ("Mark Text: Define" etc.).
pub struct ChoiceScreen {
    title: String,
    items: Vec<&'static str>,
    /// Rows between items (WP leaves a blank line).
    spacing: usize,
    on_pick: Option<Box<dyn FnOnce(&mut Wp, usize) -> Next>>,
}

impl ChoiceScreen {
    pub fn new(title: &str, items: &[&'static str], f: impl FnOnce(&mut Wp, usize) -> Next + 'static) -> Box<ChoiceScreen> {
        Box::new(ChoiceScreen { title: title.into(), items: items.to_vec(), spacing: 2, on_pick: Some(Box::new(f)) })
    }
}

fn mnemonic(item: &str) -> Option<char> {
    let k = item.find('~')?;
    item[k + 1..].chars().next().map(|c| c.to_ascii_lowercase())
}

impl Screen for ChoiceScreen {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let pick = match i {
            Input::Char(c) if c.is_ascii_digit() => {
                let d = c.to_digit(10).unwrap() as usize;
                if d == 0 {
                    return Outcome::Close;
                }
                (d <= self.items.len()).then(|| d - 1)
            }
            Input::Char(c) => {
                let lc = c.to_ascii_lowercase();
                self.items.iter().position(|it| mnemonic(it) == Some(lc))
            }
            Input::Func(Func::Enter | Func::Cancel | Func::Esc | Func::Exit) => return Outcome::Close,
            _ => None,
        };
        match pick {
            Some(k) => match (self.on_pick.take().unwrap())(wp, k) {
                Some(n) => Outcome::Replace(n),
                None => Outcome::Close,
            },
            None => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, &self.title, st);
        for (k, it) in self.items.iter().enumerate() {
            let row = 2 + k * self.spacing;
            g.text(5, row, &format!("{} - ", k + 1), st);
            let end = g.text_mn(9, row, it, st, wp.theme.bold);
            wp.hit(row, 5, end, HitAction::Input(Input::Char(char::from_digit((k + 1) as u32 % 10, 10).unwrap_or('0'))));
        }
        let fr = g.rows - 1;
        let c = g.text(0, fr, "Selection: ", st);
        g.text(c, fr, "0", st);
        g.cursor = Some((c, fr));
    }
}

const STYLE_ITEMS: [&str; 5] =
    ["~No Page Numbers", "~Page Numbers Follow Entries", "~(Page Numbers) Follow Entries", "~Flush Right Page Numbers", "Flush Right Page Numbers with ~Leaders"];

/// Mark Text → Define.
fn define_screen() -> Box<dyn Screen> {
    ChoiceScreen::new(
        "Mark Text: Define",
        &["Define Table of ~Contents", "Define ~List", "Define ~Index", "Define Table of ~Authorities", "~Edit Table of Authorities Full Form"],
        |wp, k| match k {
            0 => Some(Box::new(TocDef { st: Rc::new(RefCell::new((1, false, [4; 5]))) }) as Box<dyn Screen>),
            1 => Some(Prompt::new("List Number (1-10):", "", |_wp, s| {
                let n: u8 = s?.trim().parse().ok().filter(|n| (1..=10).contains(n))?;
                Some(ChoiceScreen::new(&format!("List {n} Definition"), &STYLE_ITEMS, move |wp, k| {
                    ins(wp, Mark::DefList { n, style: k as u8 });
                    None
                }) as Box<dyn Screen>)
            }) as Box<dyn Screen>),
            2 => Some(Prompt::new("Concordance Filename (Enter=none):", "", |wp, s| {
                let s = s?.trim().to_string();
                if !s.is_empty() && !path_for(wp, &s).exists() {
                    wp.flash("Concordance file not found");
                }
                Some(ChoiceScreen::new("Index Definition", &STYLE_ITEMS, move |wp, k| {
                    ins(wp, Mark::DefIndex { style: k as u8, concordance: s });
                    None
                }) as Box<dyn Screen>)
            }) as Box<dyn Screen>),
            3 => Some(Prompt::new("Section Number (1-16):", "1", |_wp, s| {
                let n: u8 = s?.trim().parse().ok().filter(|n| (1..=16).contains(n))?;
                Some(Box::new(ToADef { section: n, st: Rc::new(RefCell::new((true, false, true))) }) as Box<dyn Screen>)
            }) as Box<dyn Screen>),
            _ => {
                edit_full_form(wp);
                None
            }
        },
    )
}

/// Define → 5: edit the full form of the nearest preceding ToA mark.
fn edit_full_form(wp: &mut Wp) {
    let e = wp.ed_ref();
    let cur = e.cursor.min(e.doc.els.len());
    let found = e.doc.els[..cur].iter().rposition(|x| matches!(x, El::Code(Code::Mark(Mark::ToA { section, .. })) if *section > 0));
    let Some(idx) = found else {
        wp.flash("* Not found *");
        return;
    };
    let Some(El::Code(Code::Mark(Mark::ToA { full, .. }))) = e.doc.els.get(idx).cloned() else { return };
    crate::subedit::open(wp, "ToA Full Form", crate::comments::text_to_els(&full), false, move |wp, els| {
        let e = wp.ed();
        if let Some(El::Code(Code::Mark(Mark::ToA { full, .. }))) = e.doc.els.get_mut(idx) {
            *full = crate::comments::els_to_text(&els);
            e.modified = true;
            e.relayout();
        }
    });
}

/// "Table of Contents Definition".
struct TocDef {
    st: Rc<RefCell<(u8, bool, [u8; 5])>>,
}

fn level_style_menu(st: Rc<RefCell<(u8, bool, [u8; 5])>>, level: u8) -> Box<dyn Screen> {
    let (levels, wrap, _) = *st.borrow();
    let wrapped = wrap && levels > 1 && level == levels;
    let items: &[&str] = if wrapped { &["~None", "~Pg # Follows", "~(Pg #) Follows"] } else { &["~None", "~Pg # Follows", "~(Pg #) Follows", "~Flush Rt", "Flush Rt with ~Leader"] };
    MenuLine::new(items, move |_wp, k| {
        st.borrow_mut().2[level as usize - 1] = k as u8;
        if level < levels { Some(level_style_menu(st, level + 1)) } else { None }
    })
    .prefix(&format!("Level {level}: "))
}

impl Screen for TocDef {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char('1') => {
                let st = self.st.clone();
                let cur = st.borrow().0.to_string();
                Outcome::Push(Prompt::new("Number of Levels (1-5):", &cur, move |_wp, s| {
                    if let Some(n) = s.and_then(|s| s.trim().parse::<u8>().ok()).filter(|n| (1..=5).contains(n)) {
                        let mut b = st.borrow_mut();
                        b.0 = n;
                        if n == 1 {
                            b.1 = false;
                        }
                    }
                    None
                }))
            }
            Input::Char('2') => {
                if self.st.borrow().0 < 2 {
                    return Outcome::Keep;
                }
                let st = self.st.clone();
                let cur = st.borrow().1;
                Outcome::Push(YesNo::new("Display Last Level in Wrapped Format?", cur, move |_wp, y| {
                    let mut b = st.borrow_mut();
                    b.1 = y;
                    let l = b.0 as usize - 1;
                    if y && b.2[l] > 2 {
                        b.2[l] = 1;
                    }
                    None
                }))
            }
            Input::Char('3') => Outcome::Push(level_style_menu(self.st.clone(), 1)),
            Input::Func(Func::Exit) | Input::Func(Func::Enter) | Input::Char('0') => {
                let (levels, wrap_last, styles) = *self.st.borrow();
                ins(wp, Mark::DefToc { levels, wrap_last, styles });
                Outcome::Close
            }
            Input::Func(Func::Cancel | Func::Esc) => Outcome::Close,
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let (levels, wrap, styles) = *self.st.borrow();
        g.clear(st);
        g.text(0, 0, "Table of Contents Definition", st);
        let rows: [(usize, &str, char); 3] = [(2, "Number of Levels", '1'), (4, "Display Last Level in", '2'), (7, "Page Numbering - Level 1", '3')];
        for (row, label, key) in rows {
            g.text(5, row, &format!("{key} - "), st);
            let end = g.text_mn(9, row, label, st, wp.theme.bold);
            wp.hit(row, 5, end, HitAction::Input(Input::Char(key)));
        }
        g.text(11, 5, "Wrapped Format", st);
        g.text(44, 2, &levels.to_string(), st);
        g.text(44, 4, if wrap { "Yes" } else { "No" }, st);
        for l in 0..5usize {
            if l > 0 {
                g.text(26, 7 + l, &format!("Level {}", l + 1), st);
            }
            if l < levels as usize {
                g.text(44, 7 + l, STYLE_NAMES[styles[l] as usize % 5], st);
            }
        }
        let fr = g.rows - 1;
        let c = g.text(0, fr, "Selection: ", st);
        g.text(c, fr, "0", st);
        g.cursor = Some((c, fr));
    }
}

/// "Definition for Table of Authorities".
struct ToADef {
    section: u8,
    st: Rc<RefCell<(bool, bool, bool)>>,
}

impl Screen for ToADef {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let labels = ["Dot Leaders?", "Underlining Allowed?", "Blank Line Between Authorities?"];
        match i {
            Input::Char(c @ '1'..='3') => {
                let k = *c as usize - '1' as usize;
                let st = self.st.clone();
                let cur = { let b = st.borrow(); [b.0, b.1, b.2][k] };
                Outcome::Push(YesNo::new(labels[k], cur, move |_wp, y| {
                    let mut b = st.borrow_mut();
                    match k {
                        0 => b.0 = y,
                        1 => b.1 = y,
                        _ => b.2 = y,
                    }
                    None
                }))
            }
            Input::Func(Func::Exit) | Input::Func(Func::Enter) | Input::Char('0') => {
                let (dots, underline, blank) = *self.st.borrow();
                ins(wp, Mark::DefToA { section: self.section, dots, underline, blank });
                Outcome::Close
            }
            Input::Func(Func::Cancel | Func::Esc) => Outcome::Close,
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let (a, b, c) = *self.st.borrow();
        g.clear(st);
        g.text(0, 0, &format!("Definition for Table of Authorities {}", self.section), st);
        for (k, (label, v)) in [("~Dot Leaders", a), ("~Underlining Allowed", b), ("~Blank Line Between Authorities", c)].iter().enumerate() {
            let row = 2 + k * 2;
            g.text(5, row, &format!("{} - ", k + 1), st);
            let end = g.text_mn(9, row, label, st, wp.theme.bold);
            wp.hit(row, 5, end, HitAction::Input(Input::Char((b'1' + k as u8) as char)));
            g.text(44, row, if *v { "Yes" } else { "No" }, st);
        }
        let fr = g.rows - 1;
        let c = g.text(0, fr, "Selection: ", st);
        g.text(c, fr, "0", st);
        g.cursor = Some((c, fr));
    }
}

// ---------------------------------------------------------- cross-ref

const TIE_ITEMS: [&str; 5] = ["~Page Number", "Paragraph/~Outline Number", "~Footnote Number", "~Endnote Number", "~Graphics Box Number"];

/// Choose the reference type, then `then(kind)`.
fn tie_screen(then: impl FnOnce(&mut Wp, u8) -> Next + 'static) -> Box<dyn Screen> {
    ChoiceScreen::new("Tie Reference to:", &TIE_ITEMS, move |_wp, k| {
        if k < 4 {
            return then(_wp, k as u8);
        }
        Some(MenuLine::new(&["~Figure", "~Table Box", "Text ~Box", "~User Box", "~Equation"], move |wp, b| then(wp, 4 + b as u8)) as Box<dyn Screen>)
    })
}

fn kind_target_name(kind: u8) -> &'static str {
    ["target", "paragraph number", "footnote", "endnote", "figure", "table box", "text box", "user box", "equation"].get(kind as usize).copied().unwrap_or("target")
}

fn cross_ref_screen() -> Box<dyn Screen> {
    ChoiceScreen::new(
        "Mark Text: Cross-Reference",
        &["Mark ~Reference", "Mark ~Target", "Mark ~Both Reference and Target"],
        |wp, k| match k {
            0 => Some(tie_screen(|_wp, kind| {
                Some(Prompt::new("Target Name:", "", move |wp, s| {
                    let s = s?.trim().to_string();
                    if !s.is_empty() {
                        ins(wp, Mark::Ref { name: s, kind, text: "?".into() });
                    }
                    None
                }) as Box<dyn Screen>)
            })),
            1 => {
                let d = last_target_name(wp);
                Some(Prompt::new("Target Name:", &d, |wp, s| {
                    let s = s?.trim().to_string();
                    if !s.is_empty() {
                        ins(wp, Mark::Target { name: s });
                    }
                    None
                }) as Box<dyn Screen>)
            }
            _ => Some(tie_screen(|wp, kind| {
                let orig = wp.ed_ref().cursor;
                Some(Box::new(MoveToTarget { orig, kind }) as Box<dyn Screen>)
            })),
        },
    )
}

fn last_target_name(wp: &Wp) -> String {
    wp.ed_ref()
        .doc
        .els
        .iter()
        .rev()
        .find_map(|x| match x {
            El::Code(Code::Mark(Mark::Ref { name, .. })) => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Mark Both: the user moves the cursor to the target and presses Enter.
struct MoveToTarget {
    orig: usize,
    kind: u8,
}

impl Screen for MoveToTarget {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Enter) => {
                let (orig, kind) = (self.orig, self.kind);
                Outcome::Replace(Prompt::new("Target Name:", "", move |wp, s| {
                    let name = s?.trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    let e = wp.ed();
                    let tpos = e.cursor.min(e.doc.els.len());
                    let orig = orig.min(e.doc.els.len());
                    let r = El::Code(mk(Mark::Ref { name: name.clone(), kind, text: "?".into() }));
                    let t = El::Code(mk(Mark::Target { name }));
                    if tpos >= orig {
                        e.doc.els.insert(tpos, t);
                        e.doc.els.insert(orig, r);
                        e.cursor = tpos + 2;
                    } else {
                        e.doc.els.insert(orig, r);
                        e.doc.els.insert(tpos, t);
                        e.cursor = tpos + 1;
                    }
                    e.modified = true;
                    e.relayout();
                    // show the value right away
                    generate_refs_only(wp);
                    wp.after_edit();
                    None
                }))
            }
            Input::Func(Func::Exit | Func::Cancel | Func::Esc) => {
                wp.ed().cursor = self.orig;
                wp.after_edit();
                Outcome::Close
            }
            other => {
                wp.edit_input(other.clone());
                Outcome::Keep
            }
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        g.text(0, row, &format!("Cross-Ref:  Move to {}; Press Enter.", kind_target_name(self.kind)), st);
    }
}

/// Update just the [Ref] values (no tables) — after Mark Both.
fn generate_refs_only(wp: &mut Wp) {
    let e = wp.ed();
    e.relayout();
    let pages = printed_pages(&e.doc, &e.layout);
    let n = e.doc.els.len();
    let c = collect(&e.doc, &pages, &vec![false; n], &[]);
    let mut vals = vec![];
    for (i, el) in e.doc.els.iter().enumerate() {
        if let El::Code(Code::Mark(Mark::Ref { name, kind, .. })) = el {
            vals.push((i, ref_value(&e.doc, &e.layout, &pages, &c, name, *kind).unwrap_or_else(|| "?".into())));
        }
    }
    for (i, v) in vals {
        if let El::Code(Code::Mark(Mark::Ref { text, .. })) = &mut e.doc.els[i] {
            *text = v;
        }
    }
    e.relayout();
}

// ------------------------------------------------------------ generate

fn generate_screen() -> Box<dyn Screen> {
    ChoiceScreen::new(
        "Mark Text: Generate",
        &[
            "~Remove Redline Markings and Strikeout Text from Document",
            "~Compare Screen and Disk Documents and Add Redline and Strikeout",
            "~Expand Master Document",
            "C~ondense Master Document",
            "~Generate Tables, Indexes, Automatic References, etc.",
        ],
        |wp, k| match k {
            0 => Some(YesNo::new("Delete redline markings and strikeout text?", false, |wp, y| {
                if y {
                    let e = wp.ed();
                    e.doc.els = remove_redline(&e.doc.els);
                    e.doc.normalize_attrs();
                    e.modified = true;
                    e.relayout();
                    wp.after_edit();
                }
                None
            }) as Box<dyn Screen>),
            1 => {
                let label = wp.doc_label();
                Some(Prompt::new("Other Document:", &label, |wp, s| {
                    let s = s?;
                    let p = wp.resolve_path(&s);
                    match crate::fileio::load(&p) {
                        Ok(old) => crate::track::compare_into_current(wp, &old),
                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                    }
                    wp.after_edit();
                    None
                }) as Box<dyn Screen>)
            }
            2 => {
                run_expand(wp);
                None
            }
            3 => Some(YesNo::new("Save Subdocs?", true, |wp, y| {
                run_condense(wp, y);
                None
            }) as Box<dyn Screen>),
            _ => Some(YesNo::new("Existing tables, lists, and indexes will be replaced.  Continue?", true, |wp, y| {
                if y {
                    run_generate(wp);
                }
                None
            }) as Box<dyn Screen>),
        },
    )
}

fn load_sub(wp: &Wp, name: &str) -> Option<Document> {
    crate::fileio::load(&path_for(wp, name)).ok()
}

fn run_expand(wp: &mut Wp) {
    let els = wp.ed_ref().doc.els.clone();
    let names: Vec<String> = els
        .iter()
        .filter_map(|e| match e {
            El::Code(Code::Mark(Mark::Subdoc(f))) => Some(f.clone()),
            _ => None,
        })
        .collect();
    let loaded: HashMap<String, Document> = names.iter().filter_map(|n| load_sub(wp, n).map(|d| (n.clone(), d))).collect();
    let (out, n, errs) = expand_master(&els, &|f| loaded.get(f).cloned());
    let e = wp.ed();
    e.doc.els = out;
    e.modified = true;
    e.relayout();
    wp.after_edit();
    if errs.is_empty() {
        wp.flash(&format!("{n} subdocument(s) expanded"));
    } else {
        wp.flash(&format!("ERROR: Subdocument not found: {}", errs.join(", ")));
    }
}

fn run_condense(wp: &mut Wp, save: bool) {
    let els = wp.ed_ref().doc.els.clone();
    let mut to_save: Vec<(String, Vec<El>)> = vec![];
    let (out, n) = condense_master(&els, &mut |f, sub| to_save.push((f.to_string(), sub.to_vec())));
    let mut errs = vec![];
    if save {
        for (f, sub) in to_save {
            let p = path_for(wp, &f);
            // keep the subdocument's own page setup when it exists
            let mut d = crate::fileio::load(&p).unwrap_or_else(|_| {
                let mut d = Document::new();
                d.init = wp.ed_ref().doc.init.clone();
                d
            });
            let ic = d.initial_codes.len();
            let body = if ic > 0 && sub.len() >= ic && sub[..ic] == d.initial_codes[..] { sub[ic..].to_vec() } else { sub };
            d.els = body;
            if let Err(e) = crate::fileio::save(&p, &d) {
                errs.push(format!("{f}: {e}"));
            }
        }
    }
    let e = wp.ed();
    e.doc.els = out;
    e.modified = true;
    e.relayout();
    wp.after_edit();
    if errs.is_empty() {
        wp.flash(&format!("{n} subdocument(s) condensed{}", if save { " and saved" } else { "" }));
    } else {
        wp.flash(&format!("ERROR: {}", errs.join("; ")));
    }
}

/// Generate (6, 5) on the current document.
pub fn run_generate(wp: &mut Wp) {
    let base: Option<std::path::PathBuf> = wp.ed_ref().path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let cwd = wp.cwd.clone();
    let conc = move |name: &str| -> Option<Vec<String>> {
        let p = std::path::Path::new(name);
        let p = if p.is_absolute() { p.to_path_buf() } else { base.as_ref().map(|b| b.join(p)).filter(|q| q.exists()).unwrap_or_else(|| cwd.join(p)) };
        let d = crate::fileio::load(&p).ok()?;
        Some(d.plain_text(0, d.len()).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    };
    let e = wp.ed();
    let cursor = e.cursor;
    let mut doc = std::mem::take(&mut e.doc);
    let stats = {
        let m = e.metrics();
        generate_with(&mut doc, &mut |d| crate::layout::layout(d, m), &conc)
    };
    e.doc = doc;
    e.cursor = cursor.min(e.doc.els.len());
    e.modified = true;
    e.relayout();
    wp.after_edit();
    if stats.unresolved.is_empty() {
        wp.flash("* Generation complete *");
    } else {
        wp.flash(&format!("Target(s) not found: {}", stats.unresolved.join(", ")));
    }
}

// ================================================================= tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{MonoMetrics, layout};

    fn m(x: Mark) -> El {
        El::Code(Code::Mark(x))
    }
    fn txt(s: &str) -> Vec<El> {
        Document::from_text(s).els
    }
    fn regen(d: &mut Document) -> GenStats {
        generate_with(d, &mut |d| layout(d, &MonoMetrics), &|_| None)
    }

    /// A 3-page document: ToC definition on page 1, chapters on pages 2, 3.
    fn three_pages() -> Document {
        let mut d = Document::new();
        d.els.push(m(Mark::DefToc { levels: 2, wrap_last: false, styles: [4; 5] }));
        d.els.push(El::Code(Code::HPg));
        for (k, title) in ["Einleitung", "Hauptteil"].iter().enumerate() {
            d.els.push(m(Mark::Begin { list: false, n: 1 }));
            d.els.extend(txt(title));
            d.els.push(m(Mark::End { list: false, n: 1 }));
            d.els.extend(txt("\nText.\n"));
            d.els.push(m(Mark::Begin { list: false, n: 2 }));
            d.els.extend(txt(&format!("Abschnitt {}", k + 1)));
            d.els.push(m(Mark::End { list: false, n: 2 }));
            d.els.extend(txt("\n"));
            if k == 0 {
                d.els.push(El::Code(Code::HPg));
            }
        }
        d
    }

    #[test]
    fn toc_flush_right_dot_leaders() {
        let mut d = three_pages();
        let st = regen(&mut d);
        assert_eq!(st.regions, 1);
        let end = d.els.iter().position(is_end_def).expect("End Def inserted");
        let region = &d.els[1..end];
        let t = d.plain_text(1, end);
        assert_eq!(t, "Einleitung2\n\tAbschnitt 12\nHauptteil3\n\tAbschnitt 23\n", "{region:?}");
        assert_eq!(region.iter().filter(|e| matches!(e, El::Code(Code::FlushRightDot))).count(), 4);
        // generating again changes nothing
        let before = d.els.clone();
        regen(&mut d);
        assert_eq!(d.els, before);
        // the leader dots appear on screen
        let l = layout(&d, &MonoMetrics);
        let line = &l.lines[0];
        let s: String = line.cells.iter().map(|c| c.ch).collect();
        assert!(s.starts_with("Einleitung....") && s.ends_with(".2"), "{s}");
        let dots: Vec<i32> = line.cells.iter().filter(|c| c.ch == '.').map(|c| c.col).collect();
        let g_col = line.cells[9].col;
        assert_eq!(dots[0], g_col + 2, "one blank before the leader");
        assert_eq!(*dots.last().unwrap(), line.cells.last().unwrap().col - 2, "one blank after the leader");
        let last = line.cells.last().unwrap();
        assert_eq!(last.col, (d.init.page_width - d.init.right) / crate::layout::COL_WPU - 1);
    }

    #[test]
    fn toc_pages_follow_new_page_number() {
        let mut d = three_pages();
        d.els.insert(2, El::Code(Code::PageNumber(1)));
        if let El::Code(Code::Mark(Mark::DefToc { styles, .. })) = &mut d.els[0] {
            *styles = [2; 5];
        }
        regen(&mut d);
        let end = d.els.iter().position(is_end_def).unwrap();
        assert_eq!(d.plain_text(1, end), "Einleitung (1)\n\tAbschnitt 1 (1)\nHauptteil (2)\n\tAbschnitt 2 (2)\n");
    }

    #[test]
    fn regenerate_replaces_old_text_and_grows_pages() {
        // a long ToC pushes the text down; the pass loop settles page numbers
        let mut d = Document::new();
        d.els.push(m(Mark::DefToc { levels: 1, wrap_last: false, styles: [1; 5] }));
        for k in 0..60 {
            d.els.push(m(Mark::Begin { list: false, n: 1 }));
            d.els.extend(txt(&format!("K{k}")));
            d.els.push(m(Mark::End { list: false, n: 1 }));
            d.els.extend(txt("\n"));
        }
        let st = regen(&mut d);
        assert!(st.passes >= 2);
        let end = d.els.iter().position(is_end_def).unwrap();
        let t = d.plain_text(1, end);
        assert_eq!(t.lines().count(), 60);
        // page of the last entry is where it really is
        let lay = layout(&d, &MonoMetrics);
        let pages = printed_pages(&d, &lay);
        let last_mark = d.els.iter().rposition(|e| matches!(e, El::Code(Code::Mark(Mark::Begin { .. })))).unwrap();
        assert!(t.lines().last().unwrap().ends_with(&format!(" {}", pages[last_mark])), "{t}");
    }

    #[test]
    fn index_with_concordance_and_subheadings() {
        let mut d = Document::new();
        d.els.extend(txt("Der Vertrag und die Haftung.\n"));
        d.els.push(m(Mark::Index { heading: "Vertrag".into(), sub: "Kündigung".into() }));
        d.els.push(El::Code(Code::HPg));
        d.els.extend(txt("Noch einmal Haftung.\n"));
        d.els.push(m(Mark::DefIndex { style: 1, concordance: "conc".into() }));
        let st = generate_with(&mut d, &mut |d| layout(d, &MonoMetrics), &|n| (n == "conc").then(|| vec!["Haftung".to_string()]));
        assert_eq!(st.regions, 1);
        let def = d.els.iter().position(is_def).unwrap();
        let end = d.els.iter().position(is_end_def).unwrap();
        assert_eq!(d.plain_text(def + 1, end), "Haftung 1, 2\nVertrag\n\tKündigung 1\n");
    }

    #[test]
    fn lists_and_toa() {
        let mut d = Document::new();
        d.els.push(m(Mark::DefList { n: 1, style: 1 }));
        d.els.push(m(Mark::DefToA { section: 1, dots: false, underline: false, blank: false }));
        d.els.extend(txt("x\n"));
        d.els.push(m(Mark::Begin { list: true, n: 1 }));
        d.els.extend(txt("Bild A"));
        d.els.push(m(Mark::End { list: true, n: 1 }));
        d.els.push(m(Mark::ToA { section: 1, short: "BGB".into(), full: "Bürgerliches Gesetzbuch".into() }));
        d.els.push(El::Code(Code::HPg));
        d.els.push(m(Mark::ToA { section: 0, short: "BGB".into(), full: String::new() }));
        regen(&mut d);
        let t = d.plain_text(0, d.len());
        assert!(t.starts_with("Bild A 1\nBürgerliches Gesetzbuch1, 2\n"), "{t:?}");
    }

    #[test]
    fn cross_references() {
        let mut d = Document::new();
        d.els.extend(txt("Siehe Seite "));
        d.els.push(m(Mark::Ref { name: "ziel".into(), kind: 0, text: "?".into() }));
        d.els.extend(txt(" und Fußnote "));
        d.els.push(m(Mark::Ref { name: "fn".into(), kind: 2, text: "?".into() }));
        d.els.push(m(Mark::Ref { name: "weg".into(), kind: 0, text: "?".into() }));
        d.els.push(El::Code(Code::Note { kind: NoteKind::Foot, els: txt("eins") }));
        d.els.push(El::Code(Code::Note { kind: NoteKind::Foot, els: vec![m(Mark::Target { name: "fn".into() })] }));
        d.els.push(El::Code(Code::HPg));
        d.els.push(El::Code(Code::HPg));
        d.els.push(m(Mark::Target { name: "ziel".into() }));
        let st = regen(&mut d);
        assert_eq!(st.refs, 3);
        assert_eq!(st.unresolved, vec!["weg".to_string()]);
        let vals: Vec<String> = d.els.iter().filter_map(|e| match e {
            El::Code(Code::Mark(Mark::Ref { text, .. })) => Some(text.clone()),
            _ => None,
        }).collect();
        assert_eq!(vals, ["3", "2", "?"]);
        // the value is part of the text on screen
        let l = layout(&d, &MonoMetrics);
        let s: String = l.lines[0].cells.iter().map(|c| c.ch).collect();
        assert!(s.starts_with("Siehe Seite 3 und Fußnote 2?"), "{s}");
    }

    #[test]
    fn redline_and_master() {
        let mut els = txt("ab");
        els.insert(1, El::Code(Code::AttrOn(Attr::Strikeout)));
        els.insert(2, El::Ch('X'));
        els.insert(3, El::Code(Code::AttrOff(Attr::Strikeout)));
        els.insert(0, El::Code(Code::AttrOn(Attr::Redline)));
        els.push(El::Code(Code::AttrOff(Attr::Redline)));
        assert_eq!(remove_redline(&els), txt("ab"));

        let mut master = txt("A\n");
        master.push(m(Mark::Subdoc("kap1.docx".into())));
        master.extend(txt("Z"));
        let (exp, n, errs) = expand_master(&master, &|f| (f == "kap1.docx").then(|| Document::from_text("Kapitel eins\n")));
        assert_eq!((n, errs.len()), (1, 0));
        assert!(matches!(exp[2], El::Code(Code::Mark(Mark::SubdocStart(_)))));
        let mut saved = vec![];
        let (cond, n) = condense_master(&exp, &mut |f, els| saved.push((f.to_string(), els.to_vec())));
        assert_eq!(n, 1);
        assert_eq!(cond, master);
        assert_eq!(saved[0].1, txt("Kapitel eins\n"));
    }

    #[test]
    fn stream_roundtrip_of_marks() {
        let mut d = three_pages();
        d.els.push(m(Mark::Index { heading: "a|b".into(), sub: "c\nd".into() }));
        d.els.push(m(Mark::ToA { section: 3, short: "s".into(), full: "full\nform".into() }));
        d.els.push(m(Mark::DefIndex { style: 2, concordance: "K.WP".into() }));
        d.els.push(m(Mark::DefToA { section: 2, dots: true, underline: false, blank: true }));
        d.els.push(m(Mark::DefList { n: 7, style: 3 }));
        d.els.push(m(Mark::Ref { name: "n".into(), kind: 5, text: "4".into() }));
        d.els.push(m(Mark::Target { name: "n".into() }));
        d.els.push(m(Mark::Subdoc("x.wp".into())));
        d.els.push(m(Mark::SubdocStart("x.wp".into())));
        d.els.push(m(Mark::SubdocEnd("x.wp".into())));
        d.els.push(m(Mark::EndDef));
        d.els.push(El::Code(Code::FlushRightDot));
        let d2 = crate::stream::read(&crate::stream::write(&d)).unwrap();
        assert_eq!(d.els, d2.els);
        assert_eq!(crate::reveal::code_text(&Code::Mark(Mark::Begin { list: false, n: 1 })), "[Mark:ToC,1]");
        assert_eq!(crate::reveal::code_text(&Code::Mark(Mark::Ref { name: "a".into(), kind: 0, text: "2".into() })), "[Ref(a):Pg 2]");
        assert_eq!(crate::reveal::code_text(&Code::Mark(Mark::DefToc { levels: 2, wrap_last: false, styles: [4; 5] })), "[Def Mark:ToC,2:5,5]");
    }
}
