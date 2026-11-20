//! Markdown (.md) reader and writer.
//!
//! A Markdown file is opened with its structure as ordinary codes, and
//! saving writes that structure back as Markdown, in one consistent style
//! (the author's own markers and line wrapping are not kept).  Reading what
//! was written gives the same elements again.  The representation, which
//! the reader produces and the writer recognises:
//!
//! * **Headings** `#`…`######` (ATX and Setext): the line is marked
//!   `[Mark:ToC,n]…[End Mark:ToC,n]` (n = level 1–6, so a table of contents
//!   can be generated) and shown bold with a size: 1 Extra Large, 2 Very
//!   Large, 3 Large, 4 normal size, 5 Small, 6 Fine (as HTML sizes h1–h6).
//!   When writing, a line without marks that is all bold and Extra Large /
//!   Very Large / Large is a heading 1 / 2 / 3 too.
//! * **Paragraphs**: a blank line is `[HRt][HRt]`; a single `[HRt]` inside
//!   running text is a hard line break (written as `\` at the line end;
//!   two trailing spaces and `<br>` are read as well).  There is never more
//!   than one blank line, several spaces are one, and headings, rules,
//!   code blocks, tables and quotes always have a blank line around them.
//! * **Emphasis**: `**bold**`, `*italic*`, `~~strikeout~~`, `<u>`, `<sup>`,
//!   `<sub>` ↔ the attribute codes (`<strong>` / `<em>` where stars would
//!   be ambiguous).
//! * **Links**: `[Link:url]text[Link End]` ↔ `[text](url)`; autolinks
//!   `<https://…>` are links whose text is the address; reference links are
//!   resolved and written inline.  Link titles are not kept.
//! * **Lists**: text, as the HTML reader does it — the item starts with one
//!   `[Tab]` per nesting level and then `• ` (bullet), `☐ ` / `☒ ` (task
//!   list item, open / done) or the number as written, `3. `.  A line that
//!   follows an item with a single `[HRt]` belongs to that item; after a
//!   blank line, text that is not an item ends the list.
//! * **Block quotes**: one `[->Indent]` per level at the start of each line
//!   (before the Tabs of a list in the quote).
//! * **Code**: Courier New.  Part of a line is `` `inline code` ``; whole
//!   lines standing by themselves are a fenced code block, whose language
//!   is kept in a `[Comment]` with the text `` ```language `` at the start
//!   of its first line.  (Only when the document's own font is not a
//!   typewriter font.  Code blocks with nothing but blank lines between
//!   them become one, unless the second has a language.)
//! * **Horizontal rule**: a line of 40 `─` standing by itself (when
//!   writing, a horizontal graphics line too).
//! * **Tables**: `[Tbl Def]…[Tbl Off]`; the column alignment of the
//!   separator row is a `[Just:…]` at the start of the cells (and
//!   `[Just:Left]` again where the next cell or the text after the table
//!   starts); the header row is bold.  `|` in a cell is `\|`, a line break
//!   `<br>`.
//! * **Images**: there is no picture to show, so the Markdown text
//!   `![alt](source "title")` stays in the document as it is.
//! * **Footnotes** `[^1]` ↔ `[Footnote]`; `<!-- … -->` ↔ `[Comment]`.
//!
//! Not in Markdown, and dropped when writing: fonts and sizes, colours,
//! justification outside tables, margins, tabs, headers/footers, columns,
//! joined cells, graphics (of a figure only `![](FILE)`).  Endnotes become
//! footnotes.

use crate::doc::*;
use crate::fmt_build::Builder;
use std::collections::HashMap;

const MONO: &str = "Courier New";
const RULE: char = '─';
/// A horizontal rule is a line of this many `─`.
const RULE_LEN: usize = 40;
const BULLET: char = '•';
const BOX_OPEN: char = '☐';
const BOX_DONE: char = '☒';

/// White space that Markdown may trim or collapse (not the hard space).
fn is_blank(c: char) -> bool {
    c.is_whitespace() && c != '\u{a0}'
}

// ===========================================================================
// reading

fn base_font() -> FontSpec {
    FontSpec { name: "Times New Roman".into(), size_cpt: 1200 }
}

struct Md<'a> {
    b: Builder,
    /// footnote definitions
    defs: &'a HashMap<String, String>,
    /// link reference definitions: label → destination (and title)
    refs: &'a HashMap<String, String>,
    depth: u32,
    quote: usize,
    /// indentation of the open list levels
    list_indent: Vec<usize>,
    /// paragraph ends since the last content (at most two outside code)
    hrts: usize,
    /// spaces not written yet (dropped at the end of a line)
    sp: usize,
    /// attributes of the last character written in this line
    last_attrs: AttrSet,
    /// inside code (spaces are kept as they are)
    code_on: bool,
    /// address of the link being read, and the attributes around it
    link: Option<String>,
    link_attrs: AttrSet,
    /// heading level whose `[Mark:ToC]` is written with the first content
    pending_mark: Option<u8>,
    /// inside a heading: a line break is a space
    no_break: bool,
    /// nothing of the line's text has been written yet (start of a line,
    /// after a list marker): spaces and line breaks there are dropped
    lead: bool,
}

fn indent_of(l: &str) -> usize {
    let mut n = 0;
    for c in l.chars() {
        match c {
            ' ' => n += 1,
            '\t' => n = (n / 4 + 1) * 4,
            _ => break,
        }
    }
    n
}

/// The line without (up to) `n` columns of indentation.
fn strip_indent(l: &str, n: usize) -> &str {
    let mut col = 0;
    for (k, c) in l.char_indices() {
        if col >= n {
            return &l[k..];
        }
        match c {
            ' ' => col += 1,
            '\t' => col = (col / 4 + 1) * 4,
            _ => return &l[k..],
        }
    }
    ""
}

fn is_hr(t: &str) -> bool {
    let t = t.trim();
    let Some(c) = t.chars().next() else { return false };
    matches!(c, '*' | '-' | '_') && t.chars().filter(|&x| x == c).count() >= 3 && t.chars().all(|x| x == c || x == ' ' || x == '\t')
}

fn atx(t: &str) -> Option<(usize, &str)> {
    let t = t.trim_start();
    let n = t.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&n) {
        return None;
    }
    let rest = &t[n..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let mut rest = rest.trim();
    // closing sequence of #s
    let stripped = rest.trim_end_matches('#');
    if stripped.is_empty() || stripped.ends_with([' ', '\t']) {
        rest = stripped.trim_end();
    }
    Some((n, rest))
}

struct ListItem<'s> {
    indent: usize,
    /// the marker as it is shown: `• `, `☐ `, `3. `, `3. ☒ `
    marker: String,
    text: &'s str,
    /// may start directly under a paragraph line (a bullet, or number 1)
    interrupts: bool,
    numbered: bool,
}

/// `- x`, `* x`, `+ x`, `12. x`, `3) x`, `- [ ] x`
fn list_item(l: &str) -> Option<ListItem<'_>> {
    let indent = indent_of(l);
    let t = l.trim_start();
    let c = t.chars().next()?;
    let (mut marker, rest, interrupts, numbered) = if matches!(c, '-' | '*' | '+') {
        (format!("{BULLET} "), &t[1..], true, false)
    } else if c.is_ascii_digit() {
        let nd = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if nd > 9 {
            return None;
        }
        let d = t[nd..].chars().next()?;
        if d != '.' && d != ')' {
            return None;
        }
        (format!("{}{} ", &t[..nd], d), &t[nd + 1..], t[..nd].parse::<u32>() == Ok(1), true)
    } else {
        return None;
    };
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let mut rest = rest.trim_start();
    let boxed = if let Some(r) = rest.strip_prefix("[ ]") {
        Some((BOX_OPEN, r))
    } else {
        rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]")).map(|r| (BOX_DONE, r))
    };
    if let Some((b, r)) = boxed {
        if r.is_empty() || r.starts_with([' ', '\t']) {
            if numbered {
                marker.push(b);
                marker.push(' ');
            } else {
                marker = format!("{b} ");
            }
            rest = r.trim_start();
        }
    }
    // (an item without text does not interrupt a paragraph)
    Some(ListItem { indent, marker, text: rest, interrupts: interrupts && !rest.is_empty(), numbered })
}

fn fence(t: &str) -> Option<(char, usize)> {
    let t = t.trim_start();
    let c = t.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = t.chars().take_while(|&x| x == c).count();
    (n >= 3 && !(c == '`' && t[n..].contains('`'))).then_some((c, n))
}

/// A source line of a paragraph that is not its last: the text, and
/// whether it ends with a hard break (`\` or two spaces).
fn split_hard(l: &str) -> (&str, bool) {
    let t = l.trim_end();
    let bs = t.chars().rev().take_while(|&c| c == '\\').count();
    if bs % 2 == 1 {
        (t[..t.len() - 1].trim(), true)
    } else {
        (t.trim(), l.ends_with("  "))
    }
}

fn split_row(l: &str) -> Vec<String> {
    let t = l.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = if t.ends_with('|') && !t.ends_with("\\|") { &t[..t.len() - 1] } else { t };
    // (as on GitHub: every `|` that is not `\|` ends the cell, in code too)
    let mut cells = vec![];
    let mut cur = String::new();
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' if it.peek() == Some(&'|') => {
                cur.push('|');
                it.next();
            }
            '|' => cells.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

fn table_sep(l: &str) -> Option<Vec<Justify>> {
    // (with a `|`: dashes alone are a rule, a heading or a list item)
    if !l.contains('-') || !l.contains('|') {
        return None;
    }
    let cells = split_row(l);
    let mut out = vec![];
    for c in cells {
        let c = c.trim();
        let core = c.trim_matches(':');
        if core.is_empty() || !core.chars().all(|x| x == '-') {
            return None;
        }
        out.push(match (c.starts_with(':'), c.ends_with(':')) {
            (true, true) => Justify::Center,
            (false, true) => Justify::Right,
            _ => Justify::Left,
        });
    }
    Some(out)
}

/// Label of a link reference: case and white space do not matter.
fn norm_label(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The address in a link destination `url "title"` / `<url> "title"`.
fn dest_url(raw: &str) -> String {
    let r = raw.trim();
    if let Some(rest) = r.strip_prefix('<') {
        if let Some(k) = rest.find('>') {
            return clean_url(&rest[..k]);
        }
    }
    clean_url(r.split_whitespace().next().unwrap_or(""))
}

/// Can the address be written between `(` and `)` (in `<…>` if need be)?
fn url_ok(u: &str) -> bool {
    let mut depth = 0i32;
    for c in u.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '<' | '>' | '`' => return false,
            c if c.is_control() => return false,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// An address that can be written: what cannot is percent-encoded.
fn clean_url(u: &str) -> String {
    if url_ok(u) {
        return u.to_string();
    }
    let mut o = String::new();
    for c in u.chars() {
        match c {
            '(' => o.push_str("%28"),
            ')' => o.push_str("%29"),
            '<' => o.push_str("%3C"),
            '>' => o.push_str("%3E"),
            '`' => o.push_str("%60"),
            c if c.is_whitespace() || c.is_control() => o.push_str("%20"),
            c => o.push(c),
        }
    }
    o
}

/// Can `<url>` be written (and read back) as an autolink?
fn autolink_url(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    (l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:")) && u.chars().count() < 290 && !u.chars().any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>')
}

impl<'a> Md<'a> {
    fn new(defs: &'a HashMap<String, String>, refs: &'a HashMap<String, String>, depth: u32) -> Md<'a> {
        Md {
            b: Builder::new(base_font()),
            defs,
            refs,
            depth,
            quote: 0,
            list_indent: vec![],
            hrts: 0,
            sp: 0,
            last_attrs: AttrSet::default(),
            code_on: false,
            link: None,
            link_attrs: AttrSet::default(),
            pending_mark: None,
            no_break: false,
            lead: true,
        }
    }

    /// What every line starts with, written with its first content: the
    /// quote indentation and the heading mark.
    fn indent_prefix(&mut self) {
        if self.b.at_para_start() && self.quote > 0 && !self.b.in_table() {
            for _ in 0..self.quote.min(8) {
                self.b.code(Code::Indent);
            }
        }
        if let Some(n) = self.pending_mark.take() {
            self.b.code(Code::Mark(Mark::Begin { list: false, n }));
        }
    }

    /// Write the space that was held back, with the attributes both
    /// neighbours share (so it never hangs at the edge of an attribute);
    /// next to a link or comment: those in effect there.
    fn flush_sp(&mut self) {
        if self.sp > 0 {
            let now = self.b.attrs;
            self.b.attrs = AttrSet(now.0 & self.last_attrs.0);
            for _ in 0..self.sp {
                self.b.ch(' ');
            }
            self.b.attrs = now;
            self.sp = 0;
        }
    }

    fn put(&mut self, c: char) {
        let c = if c == '\t' && !self.code_on { ' ' } else { c };
        if ((c as u32) < 0x20 && c != '\t') || ('\u{7f}'..='\u{9f}').contains(&c) || c == '\u{feff}' || c == '\u{200b}' {
            return;
        }
        if c == ' ' && !self.code_on {
            // spaces at the start of a line are dropped, those at its end too
            // (and several are one, as Markdown shows them)
            if !self.lead {
                self.sp = 1;
            }
            return;
        }
        self.flush_sp();
        self.indent_prefix();
        self.b.ch(c);
        self.last_attrs = self.b.attrs;
        self.hrts = 0;
        self.lead = false;
    }

    /// A code that is content of the line (link, comment…).
    fn code(&mut self, c: Code) {
        self.flush_sp();
        self.indent_prefix();
        self.b.code(c);
        self.last_attrs = self.b.attrs;
        self.hrts = 0;
        self.lead = false;
    }

    fn link_end(&mut self) {
        self.flush_sp();
        self.b.code(Code::LinkEnd);
        self.last_attrs = self.b.attrs;
        self.hrts = 0;
        self.lead = false;
    }

    /// End of paragraph / line; never more than one empty line.
    fn para(&mut self) {
        self.sp = 0;
        self.lead = true;
        self.last_attrs = AttrSet::default();
        // (nothing before the first line)
        if self.hrts < 2 && !(self.hrts == 0 && self.b.els.is_empty()) {
            self.b.para();
            self.hrts += 1;
        }
    }

    /// An empty line before what follows (blocks stand by themselves).
    fn blank(&mut self) {
        self.sp = 0;
        self.lead = true;
        self.last_attrs = AttrSet::default();
        if self.hrts > 0 || !self.b.els.is_empty() {
            self.b.blank_line();
            self.hrts = self.hrts.max(2);
        }
    }

    /// Line break inside a paragraph; a link continues on the next line.
    fn line_break(&mut self) {
        if self.no_break {
            self.put(' ');
            return;
        }
        if self.lead {
            return;
        }
        match self.link.clone() {
            Some(u) => {
                self.sp = 0;
                self.link_end();
                self.para();
                self.code(Code::Link(u));
                self.last_attrs = self.link_attrs;
            }
            None => self.para(),
        }
    }

    fn mono(&mut self, on: bool) {
        self.flush_sp();
        self.code_on = on;
        self.b.font = if on { FontSpec { name: MONO.into(), size_cpt: 1200 } } else { base_font() };
    }

    /// A code block: its lines in Courier New, the language in a comment.
    fn code_block(&mut self, lines: &[String], info: &str) {
        // (a block without text still separates what is around it)
        self.blank();
        let Some(first) = lines.iter().position(|l| !l.trim().is_empty()) else { return };
        let last = lines.iter().rposition(|l| !l.trim().is_empty()).unwrap_or(first);
        self.mono(true);
        for (k, l) in lines[first..=last].iter().enumerate() {
            if !l.trim().is_empty() {
                if k == 0 && !info.is_empty() {
                    self.code(Code::Comment(format!("```{info}")));
                }
                for c in l.trim_end().chars() {
                    self.put(c);
                }
            }
            self.b.para();
        }
        self.mono(false);
        self.hrts = 1;
        self.blank();
    }

    fn blocks(&mut self, lines: &[String]) {
        let mut para: Vec<String> = vec![];
        let mut i = 0;
        let n = lines.len();
        while i < n {
            let line = lines[i].as_str();
            let t = line.trim_end();
            let ind = indent_of(line);
            if t.trim().is_empty() {
                self.flush_para(&mut para);
                // Markdown separates paragraphs with a blank line: keep it
                self.blank();
                i += 1;
                continue;
            }
            // fenced code
            if ind < 4 {
                if let Some((fc, fl)) = fence(t) {
                    self.flush_para(&mut para);
                    self.list_indent.clear();
                    let info: String = t.trim_start()[fl..].chars().filter(|&c| c != '`' && !c.is_control()).collect();
                    let info = info.trim().to_string();
                    i += 1;
                    let mut body = vec![];
                    while i < n {
                        let l = lines[i].trim_end();
                        if fence(l).is_some_and(|(c, len)| c == fc && len >= fl) && l.trim().chars().all(|x| x == fc) {
                            i += 1;
                            break;
                        }
                        let l = l.strip_prefix(&" ".repeat(ind.min(3))).unwrap_or(l);
                        body.push(l.to_string());
                        i += 1;
                    }
                    self.code_block(&body, &info);
                    continue;
                }
            }
            // indented code (not inside a paragraph or list)
            if ind >= 4 && para.is_empty() && self.list_indent.is_empty() {
                let mut body = vec![];
                while i < n {
                    if lines[i].trim().is_empty() {
                        body.push(String::new());
                    } else if indent_of(&lines[i]) < 4 {
                        break;
                    } else {
                        body.push(strip_indent(lines[i].trim_end(), 4).to_string());
                    }
                    i += 1;
                }
                self.code_block(&body, "");
                continue;
            }
            // ATX heading
            if ind < 4 {
                if let Some((level, text)) = atx(t) {
                    self.flush_para(&mut para);
                    self.list_indent.clear();
                    self.heading(level, text);
                    i += 1;
                    continue;
                }
            }
            // Setext heading underline
            if !para.is_empty() && ind < 4 {
                let tt = t.trim();
                if tt.chars().all(|c| c == '=') || tt.chars().all(|c| c == '-') {
                    let level = if tt.starts_with('=') { 1 } else { 2 };
                    let text = para.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
                    para.clear();
                    self.heading(level, &text);
                    i += 1;
                    continue;
                }
            }
            if ind < 4 && is_hr(t) {
                self.flush_para(&mut para);
                self.list_indent.clear();
                self.blank();
                for _ in 0..RULE_LEN {
                    self.put(RULE);
                }
                self.para();
                self.blank();
                i += 1;
                continue;
            }
            // table
            if t.contains('|') && para.is_empty() {
                if let Some(aligns) = lines.get(i + 1).and_then(|l| table_sep(l)) {
                    let header = split_row(t);
                    if header.len() == aligns.len() {
                        self.list_indent.clear();
                        let mut rows = vec![header];
                        i += 2;
                        while i < n && lines[i].contains('|') && !lines[i].trim().is_empty() {
                            rows.push(split_row(&lines[i]));
                            i += 1;
                        }
                        self.table(&rows, &aligns);
                        continue;
                    }
                }
            }
            // block quote
            if ind < 4 && t.trim_start().starts_with('>') {
                self.flush_para(&mut para);
                self.list_indent.clear();
                self.blank();
                let mut inner = vec![];
                while i < n && indent_of(&lines[i]) < 4 && lines[i].trim_start().starts_with('>') {
                    let l = lines[i].trim_start();
                    let l = &l[1..];
                    inner.push(l.strip_prefix(' ').unwrap_or(l).to_string());
                    i += 1;
                }
                self.quote += 1;
                if self.depth < 20 {
                    self.depth += 1;
                    self.blocks(&inner);
                    self.depth -= 1;
                }
                self.quote -= 1;
                self.list_indent.clear();
                self.blank();
                continue;
            }
            // list item
            if let Some(item) = list_item(line) {
                if para.is_empty() || item.interrupts {
                    self.flush_para(&mut para);
                    let ind = item.indent;
                    while self.list_indent.last().is_some_and(|&top| top > ind) {
                        self.list_indent.pop();
                    }
                    if self.list_indent.last() != Some(&ind) {
                        self.list_indent.push(ind);
                    }
                    let depth = self.list_indent.len().saturating_sub(1).min(8);
                    i += 1;
                    let mut text = vec![item.text.to_string()];
                    // continuation lines
                    while i < n {
                        let l = &lines[i];
                        // (up to what starts another block)
                        if l.trim().is_empty() || list_item(l).is_some() || (indent_of(l) < 4 && (atx(l).is_some() || fence(l).is_some() || l.trim_start().starts_with('>') || is_hr(l))) {
                            break;
                        }
                        text.push(l.to_string());
                        i += 1;
                    }
                    self.indent_prefix();
                    for _ in 0..depth {
                        self.b.code_fmt(Code::Tab);
                    }
                    for c in item.marker.chars() {
                        self.put(c);
                    }
                    // (the space after the marker stays, also before nothing)
                    self.flush_sp();
                    self.lead = true;
                    self.flush_para(&mut text);
                    continue;
                }
            }
            if para.is_empty() {
                // a paragraph that is not part of an item ends the list
                self.list_indent.clear();
            }
            para.push(line.to_string());
            i += 1;
        }
        self.flush_para(&mut para);
    }

    fn heading(&mut self, level: usize, text: &str) {
        self.blank();
        let mut base = AttrSet::default();
        base.set(Attr::Bold, true);
        match level {
            1 => base.set(Attr::ExtraLarge, true),
            2 => base.set(Attr::VeryLarge, true),
            3 => base.set(Attr::Large, true),
            5 => base.set(Attr::Small, true),
            6 => base.set(Attr::Fine, true),
            _ => {}
        }
        let n = level.clamp(1, 6) as u8;
        let chars: Vec<char> = text.trim().chars().collect();
        self.pending_mark = Some(n);
        self.no_break = true;
        self.inline(&chars, base);
        self.no_break = false;
        self.sp = 0;
        self.b.attrs = AttrSet::default();
        if self.pending_mark.take().is_none() {
            // the heading has text: close its mark
            self.b.code_fmt(Code::Mark(Mark::End { list: false, n }));
        }
        self.para();
        self.blank();
    }

    fn flush_para(&mut self, para: &mut Vec<String>) {
        if para.is_empty() {
            return;
        }
        let mut s = String::new();
        let k = para.len();
        for (j, l) in para.drain(..).enumerate() {
            if j + 1 == k {
                s.push_str(l.trim());
            } else {
                let (t, hard) = split_hard(&l);
                s.push_str(t);
                s.push(if hard { '\n' } else { ' ' });
            }
        }
        let chars: Vec<char> = s.trim().chars().collect();
        self.inline(&chars, AttrSet::default());
        self.b.attrs = AttrSet::default();
        self.para();
    }

    fn table(&mut self, rows: &[Vec<String>], aligns: &[Justify]) {
        let ncols = aligns.len();
        self.blank();
        self.b.table_start(vec![]);
        // the alignment of a column: [Just] at the start of its cells
        let mut cur = Justify::Left;
        for (r, row) in rows.iter().enumerate() {
            self.b.row();
            for (c, &align) in aligns.iter().enumerate().take(ncols) {
                self.b.cell();
                self.hrts = 0;
                self.sp = 0;
                self.lead = true;
                self.last_attrs = AttrSet::default();
                if align != cur {
                    self.b.els.push(El::Code(Code::Just(align)));
                    cur = align;
                }
                let text = row.get(c).cloned().unwrap_or_default();
                let mut base = AttrSet::default();
                if r == 0 {
                    base.set(Attr::Bold, true);
                }
                let chars: Vec<char> = text.chars().collect();
                self.inline(&chars, base);
                self.sp = 0;
                self.b.attrs = AttrSet::default();
            }
        }
        self.b.table_end();
        if cur != Justify::Left {
            self.b.els.push(El::Code(Code::Just(Justify::Left)));
        }
        self.hrts = 0;
        self.last_attrs = AttrSet::default();
        self.blank();
    }

    /// Footnote text as a separate element list.
    fn note_els(&self, text: &str) -> Vec<El> {
        let mut sub = Md::new(self.defs, self.refs, self.depth + 1);
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if self.depth < 4 {
            sub.blocks(&lines);
        }
        sub.b.finish()
    }

    /// The destination a link points to (`None`: not a link).
    fn resolve(&self, text: &[char], target: &Target) -> Option<String> {
        match target {
            Target::Inline(d) => Some(d.clone()),
            Target::Ref(l) => self.refs.get(&norm_label(l)).cloned(),
            Target::Shortcut => self.refs.get(&norm_label(&text.iter().collect::<String>())).cloned(),
        }
    }

    fn inline(&mut self, s: &[char], base: AttrSet) {
        // attributes toggled by * _ ~~, and those switched by tags (the
        // two do not close each other)
        let mut st = AttrSet::default();
        let mut tag = AttrSet::default();
        // the runs that opened emphasis / strong: length, and whether they
        // could have closed as well
        let (mut em_run, mut strong_run) = ((0usize, false), (0usize, false));
        let mut i = 0;
        let n = s.len();
        let apply = |b: &mut Builder, st: AttrSet, tag: AttrSet| b.attrs = AttrSet(base.0 | st.0 | tag.0);
        apply(&mut self.b, st, tag);
        while i < n {
            let c = s[i];
            match c {
                '\\' if i + 1 < n && s[i + 1].is_ascii_punctuation() => {
                    self.put(s[i + 1]);
                    i += 2;
                }
                '\n' => {
                    self.line_break();
                    i += 1;
                }
                '`' => {
                    let run = s[i..].iter().take_while(|&&x| x == '`').count();
                    // find closing run of exactly `run`
                    match code_span_end(s, i).map(|e| e - run) {
                        Some(j) => {
                            let mut inner: &[char] = &s[i + run..j];
                            if inner.len() >= 2 && inner[0] == ' ' && inner[inner.len() - 1] == ' ' && inner.iter().any(|&x| x != ' ') {
                                inner = &inner[1..inner.len() - 1];
                            }
                            self.mono(true);
                            for &x in inner {
                                self.put(if x == '\n' { ' ' } else { x });
                            }
                            self.mono(false);
                            i = j + run;
                        }
                        None => {
                            for _ in 0..run {
                                self.put('`');
                            }
                            i += run;
                        }
                    }
                }
                '!' if s.get(i + 1) == Some(&'[') => {
                    // an image stays as its Markdown text
                    let found = link_at(s, i + 1).and_then(|(text, target, end)| self.resolve(text, &target).map(|d| (text, target, end, d)));
                    if let Some((text, target, end, dest)) = found {
                        let raw: String = match target {
                            Target::Inline(_) => s[i..end].iter().collect(),
                            _ => format!("![{}]({})", text.iter().collect::<String>(), dest),
                        };
                        let was = self.code_on;
                        self.flush_sp();
                        self.code_on = true;
                        for x in raw.chars() {
                            self.put(if x == '\n' || x == '\t' { ' ' } else { x });
                        }
                        self.code_on = was;
                        i = end;
                    } else {
                        self.put('!');
                        i += 1;
                    }
                }
                '[' => {
                    // footnote reference
                    if s.get(i + 1) == Some(&'^') {
                        if let Some(close) = s[i..].iter().position(|&x| x == ']') {
                            let label: String = s[i + 2..i + close].iter().collect::<String>().to_lowercase();
                            if let Some(def) = self.defs.get(&label) {
                                let els = self.note_els(def);
                                self.flush_sp();
                                self.indent_prefix();
                                self.b.note(NoteKind::Foot, els);
                                self.last_attrs = self.b.attrs;
                                self.hrts = 0;
                                self.lead = false;
                                i += close + 1;
                                continue;
                            }
                        }
                    }
                    let found = link_at(s, i).and_then(|(text, target, end)| self.resolve(text, &target).map(|d| (text, end, d)));
                    if let Some((text, end, dest)) = found {
                        let t = text.to_vec();
                        if self.link.is_some() {
                            // no link inside a link
                            self.inline_nested(&t, base, AttrSet(st.0 | tag.0));
                        } else {
                            let url = dest_url(&dest);
                            self.code(Code::Link(url.clone()));
                            self.link = Some(url);
                            self.link_attrs = self.b.attrs;
                            self.inline_nested(&t, base, AttrSet(st.0 | tag.0));
                            self.link = None;
                            self.link_end();
                        }
                        apply(&mut self.b, st, tag);
                        i = end;
                    } else {
                        self.put('[');
                        i += 1;
                    }
                }
                '<' => {
                    // comment
                    if s[i..].starts_with(&['<', '!', '-', '-']) {
                        if let Some(e) = (i + 4..n.saturating_sub(2)).find(|&k| s[k] == '-' && s[k + 1] == '-' && s[k + 2] == '>') {
                            let text: String = s[i + 4..e].iter().map(|&x| if x == '\n' { ' ' } else { x }).collect();
                            self.code(Code::Comment(text.trim().to_string()));
                            i = e + 3;
                            continue;
                        }
                    }
                    let close = s[i..].iter().take(300).position(|&x| x == '>');
                    let Some(close) = close else {
                        self.put('<');
                        i += 1;
                        continue;
                    };
                    let inner: String = s[i + 1..i + close].iter().collect();
                    let lower = inner.to_ascii_lowercase();
                    let plain = !inner.is_empty() && !inner.chars().any(|x| x.is_whitespace() || x.is_control() || x == '<');
                    let scheme = lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:");
                    if plain && (scheme || inner.contains('@')) {
                        // autolink: the address is the text
                        let (url, text) = if lower.starts_with("mailto:") {
                            (inner.clone(), inner[7..].to_string())
                        } else if scheme {
                            (inner.clone(), inner.clone())
                        } else {
                            (format!("mailto:{inner}"), inner.clone())
                        };
                        let link = self.link.is_none();
                        if link {
                            self.code(Code::Link(url));
                        }
                        for x in text.chars() {
                            self.put(x);
                        }
                        if link {
                            self.link_end();
                        }
                        i += close + 1;
                        continue;
                    }
                    let (closing, name) = match lower.strip_prefix('/') {
                        Some(r) => (true, r),
                        None => (false, lower.as_str()),
                    };
                    let name: String = name.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                    if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
                        self.put('<');
                        i += 1;
                        continue;
                    }
                    let a = match name.as_str() {
                        "u" | "ins" => Some(Attr::Underline),
                        "sup" => Some(Attr::Superscript),
                        "sub" => Some(Attr::Subscript),
                        "b" | "strong" => Some(Attr::Bold),
                        "i" | "em" => Some(Attr::Italic),
                        "s" | "del" | "strike" => Some(Attr::Strikeout),
                        _ => None,
                    };
                    if let Some(a) = a {
                        tag.set(a, !closing);
                        apply(&mut self.b, st, tag);
                    } else if name == "br" {
                        self.line_break();
                    }
                    i += close + 1;
                }
                '&' => {
                    let (e, k) = crate::fmt_html::entity_at(s, i);
                    match (e, k) {
                        (Some(e), k) if k > 1 && s.get(i + k - 1) == Some(&';') => {
                            self.put(e);
                            i += k;
                        }
                        _ => {
                            self.put('&');
                            i += 1;
                        }
                    }
                }
                '*' | '_' => {
                    let run = s[i..].iter().take_while(|&&x| x == c).count();
                    let prev = if i > 0 { Some(s[i - 1]) } else { None };
                    let next = s.get(i + run).copied();
                    let left = next.is_some_and(|x| !x.is_whitespace());
                    let right = prev.is_some_and(|x| !x.is_whitespace());
                    let intraword = c == '_' && prev.is_some_and(|x| x.is_alphanumeric()) && next.is_some_and(|x| x.is_alphanumeric());
                    let rest = &s[i + run..];
                    let closer_later = |len: usize| rest.windows(len).any(|w| w.iter().all(|&x| x == c));
                    // the run closes what is open (strong with two, emphasis
                    // with one) and what is left of it may open again
                    let mut left_n = run;
                    let mut used = false;
                    if !intraword {
                        // (CommonMark: when one of the two runs could both
                        // open and close, their lengths must not add up to
                        // a multiple of 3 — `*a**b**c*` is strong in emphasis)
                        let both = left && right;
                        let fits = |o: (usize, bool)| !(both || o.1) || (o.0 + run) % 3 != 0 || (o.0 % 3 == 0 && run % 3 == 0);
                        if right {
                            loop {
                                if left_n >= 2 && st.has(Attr::Bold) && fits(strong_run) {
                                    st.set(Attr::Bold, false);
                                    left_n -= 2;
                                } else if left_n >= 1 && st.has(Attr::Italic) && fits(em_run) {
                                    st.set(Attr::Italic, false);
                                    left_n -= 1;
                                } else {
                                    break;
                                }
                                used = true;
                            }
                        }
                        if left_n > 0 && left {
                            let want = left_n.min(3);
                            if closer_later(want) || (want == 3 && closer_later(2)) {
                                if want >= 2 && !st.has(Attr::Bold) {
                                    st.set(Attr::Bold, true);
                                    strong_run = (run, both);
                                }
                                if want != 2 && !st.has(Attr::Italic) {
                                    st.set(Attr::Italic, true);
                                    em_run = (run, both);
                                }
                                left_n -= want;
                                used = true;
                            }
                        }
                    }
                    if used {
                        apply(&mut self.b, st, tag);
                    }
                    for _ in 0..left_n {
                        self.put(c);
                    }
                    i += run;
                }
                '~' if s.get(i + 1) == Some(&'~') => {
                    let rest = &s[i + 2..];
                    if st.has(Attr::Strikeout) {
                        st.set(Attr::Strikeout, false);
                        apply(&mut self.b, st, tag);
                    } else if rest.windows(2).any(|w| w == ['~', '~']) && rest.first().is_some_and(|x| !x.is_whitespace()) {
                        st.set(Attr::Strikeout, true);
                        apply(&mut self.b, st, tag);
                    } else {
                        self.put('~');
                        self.put('~');
                    }
                    i += 2;
                }
                c => {
                    self.put(c);
                    i += 1;
                }
            }
        }
    }

    fn inline_nested(&mut self, s: &[char], base: AttrSet, st: AttrSet) {
        if self.depth > 20 {
            for &c in s {
                self.put(c);
            }
            return;
        }
        self.depth += 1;
        self.inline(s, AttrSet(base.0 | st.0));
        self.depth -= 1;
        self.b.attrs = AttrSet(base.0 | st.0);
    }
}

/// What follows the `[text]` of a link.
enum Target {
    /// `(destination "title")`: what is between the parentheses
    Inline(String),
    /// `[label]`
    Ref(String),
    /// nothing: `[label]` alone
    Shortcut,
}

/// A code span starting at `s[i] == '`'`: the index after its closing
/// backticks (`None`: the backticks do not close).
fn code_span_end(s: &[char], i: usize) -> Option<usize> {
    let run = s[i..].iter().take_while(|&&x| x == '`').count();
    let mut j = i + run;
    while j < s.len() {
        if s[j] == '`' {
            let r2 = s[j..].iter().take_while(|&&x| x == '`').count();
            if r2 == run {
                return Some(j + run);
            }
            j += r2;
        } else {
            j += 1;
        }
    }
    None
}

/// `[text](url)` / `[text][ref]` / `[text]` starting at `s[i] == '['`:
/// the text, what it points to and the index after the link.
fn link_at(s: &[char], i: usize) -> Option<(&[char], Target, usize)> {
    let mut depth = 0;
    let mut j = i;
    let mut close = None;
    while j < s.len() && j - i < 2000 {
        match s[j] {
            '\\' => j += 1,
            // (brackets in code are not brackets)
            '`' => {
                j = code_span_end(s, j).unwrap_or(j + s[j..].iter().take_while(|&&x| x == '`').count());
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    let close = close?;
    let text = &s[i + 1..close];
    match s.get(close + 1) {
        Some('(') => {
            let mut k = close + 2;
            let mut d = 1;
            while k < s.len() && k - close < 4000 {
                match s[k] {
                    '`' => {
                        k = code_span_end(s, k).unwrap_or(k + s[k..].iter().take_while(|&&x| x == '`').count());
                        continue;
                    }
                    '(' => d += 1,
                    ')' => {
                        d -= 1;
                        if d == 0 {
                            return Some((text, Target::Inline(s[close + 2..k].iter().collect()), k + 1));
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            Some((text, Target::Shortcut, close + 1))
        }
        Some('[') => match s[close + 1..].iter().position(|&x| x == ']') {
            Some(p) => {
                let k = close + 1 + p;
                let label: String = s[close + 2..k].iter().collect();
                let label = if label.trim().is_empty() { text.iter().collect() } else { label };
                Some((text, Target::Ref(label), k + 1))
            }
            None => Some((text, Target::Shortcut, close + 1)),
        },
        _ => Some((text, Target::Shortcut, close + 1)),
    }
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let text = crate::fmt_text::decode_auto(bytes).replace("\r\n", "\n").replace('\r', "\n");
    let all: Vec<&str> = text.split('\n').collect();
    // footnote definitions `[^label]: text` (+ indented continuation
    // lines) and link reference definitions are taken out first
    let mut defs: HashMap<String, String> = HashMap::new();
    let mut refs: HashMap<String, String> = HashMap::new();
    let mut lines: Vec<String> = vec![];
    let mut i = 0;
    let mut in_fence: Option<(char, usize)> = None;
    while i < all.len() {
        let l = all[i];
        if let Some((fc, fl)) = in_fence {
            if fence(l).is_some_and(|(c, n)| c == fc && n >= fl) {
                in_fence = None;
            }
            lines.push(l.to_string());
            i += 1;
            continue;
        }
        if let Some(f) = fence(l) {
            in_fence = Some(f);
            lines.push(l.to_string());
            i += 1;
            continue;
        }
        let t = l.trim_start();
        if indent_of(l) < 4 && t.starts_with("[^") {
            if let Some(k) = t.find(']').filter(|&k| t[k + 1..].starts_with(':')) {
                let label = t[2..k].to_lowercase();
                let mut body = t[k + 2..].trim().to_string();
                i += 1;
                while i < all.len() && (indent_of(all[i]) >= 2 || (all[i].trim().is_empty() && all.get(i + 1).is_some_and(|n| indent_of(n) >= 4))) {
                    body.push('\n');
                    body.push_str(strip_indent(all[i], 4).trim_end());
                    i += 1;
                }
                defs.insert(label, body);
                continue;
            }
        }
        if indent_of(l) < 4 && t.starts_with('[') && !t.starts_with("[^") {
            if let Some(k) = t.find("]:") {
                let label = norm_label(&t[1..k]);
                let dest = t[k + 2..].trim();
                // after the address only a title may follow
                let after = if dest.starts_with('<') { dest.split_once('>').map(|x| x.1).unwrap_or("") } else { dest.split_once(char::is_whitespace).map(|x| x.1).unwrap_or("") };
                if !label.is_empty() && !t[1..k].contains(['[', ']']) && !dest.is_empty() && (after.trim().is_empty() || after.trim_start().starts_with(['"', '\'', '('])) {
                    refs.entry(label).or_insert_with(|| dest.to_string());
                    i += 1;
                    continue;
                }
            }
        }
        lines.push(l.to_string());
        i += 1;
    }
    let mut md = Md::new(&defs, &refs, 0);
    md.blocks(&lines);
    let b = md.b;
    Ok(b.into_document(base_font()))
}

// ===========================================================================
// writing

const MARK: [(Attr, &str, &str); 6] = [
    (Attr::Bold, "**", "**"),
    (Attr::Italic, "*", "*"),
    (Attr::Strikeout, "~~", "~~"),
    (Attr::Underline, "<u>", "</u>"),
    (Attr::Superscript, "<sup>", "</sup>"),
    (Attr::Subscript, "<sub>", "</sub>"),
];

/// The attributes Markdown can show.
fn md_attrs(a: AttrSet) -> AttrSet {
    let mut o = AttrSet::default();
    for (at, _, _) in MARK {
        o.set(at, a.has(at));
    }
    if a.has(Attr::DblUnderline) {
        o.set(Attr::Underline, true);
    }
    o
}

/// A typewriter font: its text is code.
fn is_mono(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["courier", "mono", "consolas", "console", "typewriter", "fixedsys", "terminal"].iter().any(|k| n.contains(k))
}

#[derive(Clone, Debug, PartialEq)]
enum Item {
    Ch(char),
    /// Markdown as it is (a footnote reference)
    Raw(String),
    Comment(String),
    LinkOn(String),
    LinkOff,
}

/// One piece of a line with the formatting in effect.
#[derive(Clone, Debug)]
struct Atom {
    attrs: AttrSet,
    mono: bool,
    item: Item,
}

impl Atom {
    fn ch(&self) -> Option<char> {
        match self.item {
            Item::Ch(c) => Some(c),
            _ => None,
        }
    }
    /// A space that Markdown may move or drop.
    fn is_ws(&self) -> bool {
        !self.mono && self.ch().is_some_and(is_blank)
    }
}

/// A line of the document (up to its `[HRt]`).
#[derive(Default)]
struct Line {
    /// `[->Indent]` codes at the start: block quote level
    quote: usize,
    /// `[Tab]` codes at the start: list level
    tabs: usize,
    /// heading level from `[Mark:ToC,n]`
    head: Option<u8>,
    atoms: Vec<Atom>,
}

impl Line {
    fn is_empty(&self) -> bool {
        self.atoms.iter().all(|a| a.is_ws())
    }
}

enum Blk {
    Line(Line),
    Table(String),
}

/// What a line is in Markdown.
enum Kind {
    Table(String),
    Heading(usize, Vec<Atom>),
    Rule,
    /// a line of a code block: the language (first line), the text
    Code(Option<String>, String),
    /// `atoms`: the text after the marker; `line`: the whole line
    Item { depth: usize, marker: String, width: usize, interrupts: bool, atoms: Vec<Atom>, line: Vec<Atom> },
    Para(Vec<Atom>),
}

struct Entry {
    /// `[HRt]`s between the line before and this one (0: first line)
    gap: usize,
    quote: usize,
    kind: Kind,
}

/// A stretch of a line with the same formatting.
enum Seg {
    Text(AttrSet, Vec<char>),
    Code(AttrSet, String),
    Raw(AttrSet, String),
    /// a comment, and the markers open around it (`None`: as they are)
    Plain(String, Option<AttrSet>),
    /// a link, and the markers open around its brackets
    LinkOn(String, Option<AttrSet>),
    LinkOff,
}

/// Close the open markers that are not kept (and all opened after them),
/// but none of the first `floor`; markers never follow a space.
fn close_marks(out: &mut String, open: &mut Vec<(Attr, bool)>, floor: usize, keep: &dyn Fn(Attr) -> bool) {
    let Some(pos) = open.iter().position(|a| !keep(a.0)) else { return };
    let pos = pos.max(floor);
    if pos >= open.len() {
        return;
    }
    let trimmed_len = out.trim_end_matches([' ', '\t']).len();
    let ws = out[trimmed_len..].to_string();
    out.truncate(trimmed_len);
    while open.len() > pos {
        match open.pop() {
            Some((Attr::Bold, true)) => out.push_str("</strong>"),
            Some((Attr::Italic, true)) => out.push_str("</em>"),
            Some((a, _)) => {
                if let Some((_, _, c)) = MARK.iter().find(|m| m.0 == a) {
                    out.push_str(c);
                }
            }
            None => {}
        }
    }
    out.push_str(&ws);
}

/// Open the markers wanted and not open yet, in the given order.  Directly
/// after a closing `*` the stars would run together, and inside a word
/// with `*` or `**` open they could be taken to close it: there `<strong>`
/// / `<em>` are written.
fn open_marks(out: &mut String, open: &mut Vec<(Attr, bool)>, want: AttrSet, order: &[Attr]) {
    let stars = open.iter().any(|o| matches!(o, (Attr::Bold | Attr::Italic, false)));
    let tags = out.ends_with('*') || (stars && out.chars().next_back().is_some_and(|c| !c.is_whitespace()));
    for &at in order {
        if want.has(at) && !open.iter().any(|o| o.0 == at) {
            let tag = tags && matches!(at, Attr::Bold | Attr::Italic);
            match (at, tag) {
                (Attr::Bold, true) => out.push_str("<strong>"),
                (_, true) => out.push_str("<em>"),
                _ => {
                    if let Some((_, o, _)) = MARK.iter().find(|m| m.0 == at) {
                        out.push_str(o);
                    }
                }
            }
            open.push((at, tag));
        }
    }
}

/// The markers in the order to open them at `segs[k]`: what stays on
/// longest first, so that it need not be closed and opened again.
fn mark_order(segs: &[Seg], k: usize) -> Vec<Attr> {
    let extent = |at: Attr| {
        segs[k..]
            .iter()
            .filter_map(|s| match s {
                Seg::Text(w, _) | Seg::Code(w, _) | Seg::Raw(w, _) => Some(*w),
                _ => None,
            })
            .take_while(|w| w.has(at))
            .count()
    };
    let mut order: Vec<(usize, Attr)> = MARK.iter().map(|m| (extent(m.0), m.0)).collect();
    order.sort_by(|a, b| b.0.cmp(&a.0));
    order.into_iter().map(|o| o.1).collect()
}

/// Text as Markdown: only what would be read as syntax is escaped.
/// `first` / `last`: the text starts / ends the line (nothing else there);
/// `marked`: it has markers around it (they go next to its first and last
/// character, not next to the spaces at its ends); `then`: what follows
/// it: 2 a link, 1 a footnote reference, 0 something else.
fn escape(chars: &[char], in_link: bool, then: u8, first: bool, last: bool, marked: bool) -> String {
    let n = chars.len();
    let edge = (chars.iter().position(|&c| !is_blank(c)), chars.iter().rposition(|&c| !is_blank(c)));
    let mut o = String::with_capacity(n + 8);
    let mut k = 0;
    while k < n {
        let c = chars[k];
        let prev = if k > 0 { Some(chars[k - 1]) } else { None };
        let next = chars.get(k + 1).copied();
        let pws = prev.map_or(first, is_blank) && !(marked && edge.0 == Some(k));
        let nws = next.map_or(last, is_blank) && !(marked && edge.1 == Some(k));
        // an image is kept as it is
        if c == '!' && next == Some('[') {
            if let Some((_, Target::Inline(_), end)) = link_at(chars, k + 1) {
                o.extend(&chars[k..end]);
                k = end;
                continue;
            }
        }
        let esc = match c {
            // (the hard space is written `&nbsp;`)
            '\\' => next.is_none_or(|x| x.is_ascii_punctuation() || x == '\u{a0}'),
            '`' => true,
            '*' => !(pws && nws),
            '_' => !((pws && nws) || (prev.is_some_and(|x| x.is_alphanumeric()) && next.is_some_and(|x| x.is_alphanumeric()))),
            '~' => prev == Some('~') || next == Some('~') || (prev.is_none() && !first) || (next.is_none() && !last),
            // brackets only where they could be taken for a link: `](`, `][`
            '[' => in_link || next == Some('^'),
            ']' => in_link || matches!(next, Some('(' | '[')) || (next.is_none() && then > 0),
            // (a link may follow: not an image)
            '!' => next.is_none() && then == 2,
            '<' => !nws,
            '&' => {
                let (e, len) = crate::fmt_html::entity_at(chars, k);
                e.is_some() && len > 1 && chars.get(k + len - 1) == Some(&';')
            }
            _ => false,
        };
        if c == '\u{a0}' {
            o.push_str("&nbsp;");
        } else {
            if esc {
                o.push('\\');
            }
            o.push(c);
        }
        k += 1;
    }
    o
}

/// `` `code` `` with as many backticks as it takes.
fn code_span(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let f = "`".repeat(longest + 1);
    let pad = text.starts_with('`') || text.ends_with('`') || (text.starts_with(' ') && text.ends_with(' ') && text.chars().any(|c| c != ' '));
    if pad { format!("{f} {text} {f}") } else { format!("{f}{text}{f}") }
}

/// A link destination between `(` and `)`.
fn link_dest(u: &str) -> String {
    let u = clean_url(u);
    if u.contains(char::is_whitespace) { format!("<{u}>") } else { u }
}

/// Escape the start of a line that would begin a block (heading, list,
/// quote, rule, table…) but is text.  `item`: the text of a list item.
fn guard(t: String, item: bool) -> String {
    if t.is_empty() {
        return t;
    }
    // `[label]: …` would be a definition
    if t.starts_with("[^") {
        // (after a footnote reference: the colon is escaped)
        if let Some(k) = t.find(']').filter(|&k| t[k + 1..].starts_with(':')) {
            return format!("{}\\{}", &t[..=k], &t[k + 1..]);
        }
    } else if t.starts_with('[') && t.find("]:").is_some_and(|k| !t[1..k].contains(['[', ']'])) {
        return format!("\\{t}");
    }
    if let Some(li) = list_item(&t) {
        if li.numbered {
            let nd = t.chars().take_while(|c| c.is_ascii_digit()).count();
            return format!("{}\\{}", &t[..nd], &t[nd..]);
        }
        return format!("\\{t}");
    }
    let block = atx(&t).is_some()
        || is_hr(&t)
        || fence(&t).is_some()
        || t.starts_with('>')
        || (t.starts_with(['|', '-', ':']) && table_sep(&t).is_some())
        || t.chars().all(|c| c == '=')
        || t.chars().all(|c| c == '-')
        || (item && ["[ ]", "[x]", "[X]"].iter().any(|b| t.strip_prefix(b).is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t']))));
    if block { format!("\\{t}") } else { t }
}

/// The list marker a line starts with: atoms used, the Markdown marker,
/// the width of its bullet / number, whether it may follow a paragraph line.
fn marker_of(atoms: &[Atom]) -> Option<(usize, String, usize, bool)> {
    // (the marker is plain text, as the reader writes it)
    let ch = |k: usize| atoms.get(k).filter(|a| !a.mono && md_attrs(a.attrs).is_empty()).and_then(|a| a.ch());
    let sp = |k: usize| ch(k).is_some_and(is_blank);
    let boxed = |k: usize| match ch(k) {
        Some(BOX_OPEN) if sp(k + 1) => Some("[ ] "),
        Some(BOX_DONE) if sp(k + 1) => Some("[x] "),
        _ => None,
    };
    if ch(0) == Some(BULLET) && sp(1) {
        return Some((2, "- ".into(), 2, true));
    }
    if let Some(b) = boxed(0) {
        return Some((2, format!("- {b}"), 2, true));
    }
    let nd = (0..9).take_while(|&k| ch(k).is_some_and(|c| c.is_ascii_digit())).count();
    if nd == 0 || !matches!(ch(nd), Some('.' | ')')) || !sp(nd + 1) {
        return None;
    }
    let num: String = (0..=nd).filter_map(ch).collect();
    let interrupts = num[..nd].parse::<u32>() == Ok(1);
    let mut used = nd + 2;
    let mut md = format!("{num} ");
    if let Some(b) = boxed(used) {
        md.push_str(b);
        used += 2;
    }
    Some((used, md, nd + 2, interrupts))
}

struct Writer {
    notes: Vec<String>,
    attrs: AttrSet,
    mono: bool,
    just: Justify,
    /// the document is in a typewriter font: no code
    base_mono: bool,
    /// address of the open link
    link: Option<String>,
}

impl Writer {
    fn atom(&self, item: Item) -> Atom {
        Atom { attrs: self.attrs, mono: self.mono, item }
    }

    /// The element stream as lines and tables.
    fn split(&mut self, els: &[El]) -> Vec<Blk> {
        let mut out: Vec<Blk> = vec![];
        let mut cur = Line::default();
        // the link is open in this line; links inside a link
        let mut link_here = false;
        let mut nested = 0;
        let mut i = 0;
        while i < els.len() {
            // content: a link from the line before goes on
            let mut content: Option<Item> = None;
            let mut end_line = 0;
            match &els[i] {
                El::Ch(c) => content = Some(Item::Ch(*c)),
                El::Code(c) => match c {
                    Code::AttrOn(a) => self.attrs.set(*a, true),
                    Code::AttrOff(a) => self.attrs.set(*a, false),
                    Code::Font(f) => self.mono = !self.base_mono && is_mono(&f.name),
                    Code::Just(j) => self.just = *j,
                    Code::HRt => end_line = 1,
                    Code::HPg => end_line = 2,
                    // (a Tab in code is text)
                    Code::Tab if cur.atoms.is_empty() && !self.mono => cur.tabs += 1,
                    Code::Indent | Code::DblIndent if cur.atoms.is_empty() => {
                        if cur.tabs == 0 {
                            cur.quote += 1;
                        } else {
                            cur.tabs += 1;
                        }
                    }
                    Code::Tab | Code::Indent | Code::DblIndent => content = Some(Item::Ch('\t')),
                    Code::HardSpace => content = Some(Item::Ch('\u{a0}')),
                    Code::HardHyphen => content = Some(Item::Ch('\u{2011}')),
                    Code::SoftHyphen => content = Some(Item::Ch('\u{ad}')),
                    Code::Mark(Mark::Begin { list: false, n }) if (1..=6).contains(n) && cur.atoms.is_empty() => cur.head = Some(*n),
                    Code::Comment(s) => content = Some(Item::Comment(s.clone())),
                    Code::Link(u) => {
                        if self.link.is_some() {
                            nested += 1;
                        } else {
                            self.link = Some(u.clone());
                            cur.atoms.push(self.atom(Item::LinkOn(u.clone())));
                            link_here = true;
                        }
                    }
                    Code::LinkEnd => {
                        if nested > 0 {
                            nested -= 1;
                        } else if self.link.take().is_some() && link_here {
                            cur.atoms.push(self.atom(Item::LinkOff));
                            link_here = false;
                        }
                    }
                    Code::Note { els: n, .. } => {
                        let at = self.notes.len();
                        self.notes.push(String::new());
                        let saved = (self.attrs, self.mono, self.just, self.link.take());
                        self.attrs = AttrSet::default();
                        self.mono = false;
                        let blks = self.split(n);
                        let lines = self.emit(blks);
                        (self.attrs, self.mono, self.just, self.link) = saved;
                        let mut body = String::new();
                        for (k, l) in lines.iter().enumerate() {
                            if k > 0 {
                                body.push('\n');
                                if !l.is_empty() {
                                    body.push_str("    ");
                                }
                            }
                            body.push_str(l);
                        }
                        self.notes[at] = body;
                        content = Some(Item::Raw(format!("[^{}]", at + 1)));
                    }
                    Code::GBox(g) if g.contents == crate::gfx::Contents::Image && !g.file.is_empty() => {
                        // a picture: at least its name
                        for ch in format!("![]({})", link_dest(&g.file)).chars() {
                            let a = self.atom(Item::Ch(ch));
                            cur.atoms.push(a);
                        }
                    }
                    Code::GLine(g) if !g.vertical && cur.atoms.is_empty() => {
                        for _ in 0..RULE_LEN {
                            cur.atoms.push(Atom { attrs: AttrSet::default(), mono: false, item: Item::Ch(RULE) });
                        }
                    }
                    Code::TblDef { .. } => {
                        if link_here {
                            cur.atoms.push(self.atom(Item::LinkOff));
                            link_here = false;
                        }
                        if !cur.atoms.is_empty() {
                            out.push(Blk::Line(std::mem::take(&mut cur)));
                        }
                        cur = Line::default();
                        let end = els[i..].iter().position(|e| matches!(e, El::Code(Code::TblOff))).map(|k| i + k).unwrap_or(els.len());
                        let saved = self.link.take();
                        let t = self.table(&els[i + 1..end]);
                        self.link = saved;
                        out.push(Blk::Table(t));
                        i = end + 1;
                        continue;
                    }
                    _ => {}
                },
            }
            if let Some(item) = content {
                if !link_here {
                    if let Some(u) = self.link.clone() {
                        cur.atoms.push(self.atom(Item::LinkOn(u)));
                        link_here = true;
                    }
                }
                cur.atoms.push(self.atom(item));
            }
            if end_line > 0 {
                if link_here {
                    cur.atoms.push(self.atom(Item::LinkOff));
                    link_here = false;
                }
                out.push(Blk::Line(std::mem::take(&mut cur)));
                if end_line > 1 {
                    // a page break separates like an empty line
                    out.push(Blk::Line(Line::default()));
                }
            }
            i += 1;
        }
        if link_here {
            cur.atoms.push(self.atom(Item::LinkOff));
        }
        out.push(Blk::Line(cur));
        out
    }

    /// Could the line be a line of a code block?
    fn code_line(&self, l: &Line) -> bool {
        l.head.is_none()
            && l.atoms.iter().enumerate().all(|(k, a)| match &a.item {
                Item::Ch(_) => a.mono && md_attrs(a.attrs).is_empty(),
                Item::Comment(s) => k == 0 && s.starts_with("```"),
                _ => false,
            })
            // (a block does not keep spaces at the end of a line)
            && l.atoms.last().is_some_and(|a| a.ch().is_some_and(|c| !c.is_whitespace()))
    }

    /// `code`: a line of a code block; `alone`: empty lines around it.
    fn classify(&self, l: Line, code: bool, alone: bool) -> Kind {
        if code {
            let lang = match l.atoms.first().map(|a| &a.item) {
                Some(Item::Comment(s)) => Some(s.trim_start_matches('`').trim().to_string()).filter(|s| !s.is_empty()),
                _ => None,
            };
            let mut t = "\t".repeat(l.tabs);
            t.extend(l.atoms.iter().filter_map(|a| a.ch()));
            return Kind::Code(lang, t.trim_end().to_string());
        }
        // spaces at both ends do not count
        let s = l.atoms.iter().position(|a| !a.is_ws()).unwrap_or(l.atoms.len());
        let e = l.atoms.iter().rposition(|a| !a.is_ws()).map(|k| k + 1).unwrap_or(s);
        let body = &l.atoms[s..e];
        let text: Vec<&Atom> = body.iter().filter(|a| a.ch().is_some_and(|c| !is_blank(c))).collect();
        let all = |at: Attr| !text.is_empty() && text.iter().all(|a| a.attrs.has(at));
        let level = match l.head {
            Some(n) => Some(n as usize),
            None if all(Attr::Bold) && all(Attr::ExtraLarge) => Some(1),
            None if all(Attr::Bold) && all(Attr::VeryLarge) => Some(2),
            None if all(Attr::Bold) && all(Attr::Large) => Some(3),
            None => None,
        };
        if let Some(n) = level {
            return Kind::Heading(n, body.to_vec());
        }
        if alone && body.len() == RULE_LEN && body.iter().all(|a| a.ch() == Some(RULE) && a.attrs.is_empty() && !a.mono) {
            return Kind::Rule;
        }
        if let Some((used, marker, width, interrupts)) = marker_of(&l.atoms) {
            return Kind::Item { depth: l.tabs.min(8), marker, width, interrupts, atoms: l.atoms[used.min(l.atoms.len())..].to_vec(), line: l.atoms };
        }
        Kind::Para(l.atoms)
    }

    /// Lines → what they are in Markdown, with the distance between them.
    fn entries(&self, blks: Vec<Blk>) -> Vec<Entry> {
        // (gap, block) without the empty lines
        let mut list: Vec<(usize, Blk)> = vec![];
        let mut empty = 0;
        for b in blks {
            if matches!(&b, Blk::Line(l) if l.is_empty()) {
                empty += 1;
                continue;
            }
            let gap = if list.is_empty() { 0 } else { empty + 1 };
            list.push((gap, b));
            empty = 0;
        }
        // code blocks: lines all in the typewriter font, standing by
        // themselves (an empty line or nothing before and after)
        let n = list.len();
        let cand: Vec<Option<usize>> = list.iter().map(|(_, b)| match b {
            Blk::Line(l) if self.code_line(l) => Some(l.quote),
            _ => None,
        }).collect();
        let mut code = vec![false; n];
        let mut i = 0;
        while i < n {
            let Some(q) = cand[i] else {
                i += 1;
                continue;
            };
            let mut j = i;
            while j + 1 < n && cand[j + 1] == Some(q) {
                j += 1;
            }
            if (i == 0 || list[i].0 >= 2) && (j + 1 == n || list[j + 1].0 >= 2) {
                for c in &mut code[i..=j] {
                    *c = true;
                }
            }
            i = j + 1;
        }
        let alone: Vec<bool> = (0..n).map(|i| (i == 0 || list[i].0 >= 2) && (i + 1 == n || list[i + 1].0 >= 2)).collect();
        list.into_iter()
            .zip(code.into_iter().zip(alone))
            .map(|((gap, b), (code, alone))| match b {
                Blk::Table(t) => Entry { gap, quote: 0, kind: Kind::Table(t) },
                Blk::Line(l) => Entry { gap, quote: l.quote.min(8), kind: self.classify(l, code, alone) },
            })
            .collect()
    }

    /// The Markdown lines of a document or a note.
    fn emit(&mut self, blks: Vec<Blk>) -> Vec<String> {
        let mut entries = self.entries(blks);
        let mut out: Vec<String> = vec![];
        // text columns of the open list levels; of the item being continued
        let mut stack: Vec<usize> = vec![];
        let mut cont: Option<usize> = None;
        // quote level and class of the line before: 0 block, 1 text, 2 item
        // (3: item without text)
        let mut prev: Option<(usize, u8)> = None;
        let n = entries.len();
        let mut i = 0;
        while i < n {
            // a line break in running text, and the next line happens to
            // look like a list item that cannot start there: it is text
            if let (1, Some((pq, 1)), None) = (entries[i].gap, prev, cont) {
                if pq == entries[i].quote && matches!(entries[i].kind, Kind::Item { interrupts: false, .. }) {
                    if let Kind::Item { line, .. } = std::mem::replace(&mut entries[i].kind, Kind::Rule) {
                        entries[i].kind = Kind::Para(line);
                    }
                }
            }
            // … or, in an item, when the next item is more than one level
            // below it (it would not have a parent then)
            if let (1, Some((pq, 1 | 2)), Some(_), Kind::Item { depth, .. }) = (entries[i].gap, prev, cont, &entries[i].kind) {
                let d = (*depth).min(stack.len());
                let q = entries[i].quote;
                let next = entries[i + 1..].iter().take_while(|e| e.quote == q).find(|e| !(e.gap == 1 && matches!(e.kind, Kind::Para(_))));
                if pq == q && matches!(next, Some(Entry { kind: Kind::Item { depth: nd, .. }, .. }) if *nd > d + 1) {
                    if let Kind::Item { line, .. } = std::mem::replace(&mut entries[i].kind, Kind::Rule) {
                        entries[i].kind = Kind::Para(line);
                    }
                }
            }
            let e = &entries[i];
            let q = e.quote;
            let pre = "> ".repeat(q);
            let mut tight = false;
            if let Some((pq, pc)) = prev {
                if pq != q {
                    stack.clear();
                    cont = None;
                }
                tight = e.gap == 1
                    && pq == q
                    && match &e.kind {
                        Kind::Para(_) => pc == 1 || pc == 2,
                        Kind::Item { .. } => pc >= 1,
                        _ => false,
                    };
                if !tight {
                    out.push("> ".repeat(q.min(pq)).trim_end().to_string());
                }
            }
            if !tight && !matches!(e.kind, Kind::Item { .. }) {
                stack.clear();
                cont = None;
            }
            let mut class = 0;
            match &e.kind {
                Kind::Table(t) => {
                    for l in t.lines() {
                        out.push(format!("{pre}{l}"));
                    }
                }
                Kind::Heading(level, atoms) => {
                    let mut mask = AttrSet::default();
                    mask.set(Attr::Bold, true);
                    let mut t = self.inline(atoms, mask);
                    if t.ends_with('#') && !t.ends_with("\\#") {
                        t.insert(t.len() - 1, '\\');
                    }
                    out.push(format!("{pre}{} {t}", "#".repeat(*level)).trim_end().to_string());
                }
                Kind::Rule => out.push(format!("{pre}---")),
                Kind::Code(lang, text) => {
                    let mut body = vec![text.clone()];
                    let mut j = i + 1;
                    while j < n && entries[j].quote == q {
                        let Kind::Code(None, t) = &entries[j].kind else { break };
                        for _ in 1..entries[j].gap {
                            body.push(String::new());
                        }
                        body.push(t.clone());
                        j += 1;
                    }
                    let mut longest = 2;
                    for l in &body {
                        let mut run = 0;
                        for c in l.chars() {
                            run = if c == '`' { run + 1 } else { 0 };
                            longest = longest.max(run);
                        }
                    }
                    let f = "`".repeat(longest + 1);
                    out.push(format!("{pre}{f}{}", lang.as_deref().unwrap_or("")));
                    for l in body {
                        out.push(format!("{pre}{l}").trim_end().to_string());
                    }
                    out.push(format!("{pre}{f}"));
                    prev = Some((q, 0));
                    i = j;
                    continue;
                }
                Kind::Item { depth, marker, width, atoms, .. } => {
                    let d = (*depth).min(stack.len());
                    stack.truncate(d);
                    let indent = if d == 0 { 0 } else { stack[d - 1] };
                    let mut t = guard(self.inline(atoms, AttrSet::default()), true);
                    class = if t.is_empty() { 3 } else { 2 };
                    if t.is_empty() && tight && cont.is_none() && matches!(prev, Some((_, 1))) {
                        // an item without text cannot follow a paragraph
                        // line directly; with a line break in it, it can
                        t = "<br>".into();
                    }
                    if table_sep(&format!("{marker}{t}")).is_some() {
                        // `- |` under a line with `|` would make a table
                        t = t.replace('|', "\\|");
                    }
                    out.push(format!("{pre}{}{marker}{t}", " ".repeat(indent)).trim_end().to_string());
                    stack.push(indent + width);
                    cont = Some(indent + width);
                }
                Kind::Para(atoms) => {
                    let t = guard(self.inline(atoms, AttrSet::default()), false);
                    let indent = if tight { cont.unwrap_or(0) } else { 0 };
                    if tight {
                        // a line break inside the paragraph / item
                        if let Some(l) = out.last_mut() {
                            l.push('\\');
                        }
                    }
                    class = 1;
                    out.push(format!("{pre}{}{t}", " ".repeat(indent)).trim_end().to_string());
                }
            }
            prev = Some((q, class));
            i += 1;
        }
        out
    }

    /// The text of a line as Markdown; `mask`: attributes not written.
    fn inline(&self, atoms: &[Atom], mask: AttrSet) -> String {
        // spaces at the ends of the line go
        let s = atoms.iter().position(|a| !a.is_ws()).unwrap_or(atoms.len());
        let e = atoms.iter().rposition(|a| !a.is_ws()).map(|k| k + 1).unwrap_or(s);
        let atoms = &atoms[s..e];
        let n = atoms.len();
        let mut wants: Vec<AttrSet> = atoms.iter().map(|a| AttrSet(md_attrs(a.attrs).0 & !mask.0)).collect();
        // A space has the attributes both its neighbours have.  Next to
        // a link bracket or a comment it keeps its own: there the reader
        // gives it those of its other neighbour that are open at that place
        // — so the markers open at these places are chosen to fit (`place`).
        let chr = |a: &Atom| !a.is_ws() && matches!(a.item, Item::Ch(_) | Item::Raw(_));
        let left = |k: usize| (0..k).rev().find(|&j| !atoms[j].is_ws());
        let right = |k: usize| (k + 1..n).find(|&j| !atoms[j].is_ws());
        for k in 0..n {
            if atoms[k].is_ws() {
                if let (Some(l), Some(r)) = (left(k), right(k)) {
                    if chr(&atoms[l]) && chr(&atoms[r]) {
                        wants[k] = AttrSet(wants[l].0 & wants[r].0);
                    }
                }
            }
        }
        let all = md_attrs(AttrSet(!0));
        let mut place: Vec<Option<AttrSet>> = vec![None; n];
        // attributes at the last thing before (as the reader keeps them)
        let mut last = AttrSet::default();
        let mut k = 0;
        while k < n {
            let ws_before = k > 0 && atoms[k - 1].is_ws();
            match &atoms[k].item {
                _ if atoms[k].is_ws() => {}
                Item::Ch(_) | Item::Raw(_) => last = wants[k],
                Item::LinkOn(_) => {
                    let end = (k + 1..n).find(|&j| atoms[j].item == Item::LinkOff).unwrap_or(n);
                    let mut common: Option<AttrSet> = None;
                    for j in k + 1..end {
                        if chr(&atoms[j]) {
                            common = Some(AttrSet(common.map_or(wants[j].0, |c| c.0 & wants[j].0)));
                        }
                    }
                    // open after the space before
                    let here = if ws_before { AttrSet(last.0 & wants[k - 1].0) } else { last };
                    let mut o = common.unwrap_or(all);
                    // (what the spaces next to the brackets have is open there)
                    let mut least = if ws_before { wants[k - 1] } else { AttrSet::default() };
                    let mut fixed = false;
                    for j in [k + 1, end.saturating_sub(1)] {
                        if j > k && j < end && atoms[j].is_ws() {
                            o = AttrSet(o.0 & wants[j].0);
                            least = AttrSet(least.0 | wants[j].0);
                            fixed = true;
                        }
                    }
                    // (a space next to a comment in the link: what it does not
                    // have of its other neighbour is not open there either)
                    for j in k + 1..end {
                        if atoms[j].is_ws() {
                            let (l, r) = (left(j).filter(|&l| l > k), right(j).filter(|&r| r < end));
                            if let (Some(a), Some(b)) = (l, r) {
                                if chr(&atoms[a]) != chr(&atoms[b]) {
                                    let c = if chr(&atoms[a]) { a } else { b };
                                    o = AttrSet(o.0 & !(wants[c].0 & !wants[j].0));
                                }
                            }
                        }
                    }
                    if ws_before {
                        o = AttrSet(o.0 & !(last.0 & !wants[k - 1].0));
                        fixed = true;
                    }
                    if end + 1 < n && atoms[end + 1].is_ws() {
                        if let Some(r) = right(end + 1).filter(|&r| chr(&atoms[r])) {
                            o = AttrSet(o.0 & !(wants[r].0 & !wants[end + 1].0));
                        }
                        least = AttrSet(least.0 | wants[end + 1].0);
                        fixed = true;
                    }
                    if common.is_none() {
                        // a link without text: nothing more opens for it
                        o = AttrSet(o.0 & (here.0 | least.0));
                    }
                    if common.is_some() || fixed {
                        place[k] = Some(o);
                        if end < n {
                            place[end] = Some(o);
                        }
                        last = o;
                    }
                }
                Item::LinkOff => last = place[k].unwrap_or(last),
                Item::Comment(_) => {
                    let mut x = if ws_before { AttrSet(last.0 & wants[k - 1].0) } else { last };
                    if k + 1 < n && atoms[k + 1].is_ws() {
                        if let Some(r) = right(k + 1).filter(|&r| chr(&atoms[r])) {
                            x = AttrSet(x.0 & !(wants[r].0 & !wants[k + 1].0));
                        }
                        x = AttrSet(x.0 | wants[k + 1].0);
                    }
                    place[k] = Some(x);
                    last = x;
                }
            }
            k += 1;
        }
        // stretches of the same kind
        let mut segs: Vec<Seg> = vec![];
        for ((a, w), x) in atoms.iter().zip(&wants).zip(&place) {
            match &a.item {
                Item::Ch(c) if a.mono => match segs.last_mut() {
                    Some(Seg::Code(sw, t)) if sw == w => t.push(*c),
                    _ => segs.push(Seg::Code(*w, c.to_string())),
                },
                Item::Ch(c) => match segs.last_mut() {
                    Some(Seg::Text(sw, t)) if sw == w => t.push(*c),
                    _ => segs.push(Seg::Text(*w, vec![*c])),
                },
                Item::Raw(r) => segs.push(Seg::Raw(*w, r.clone())),
                Item::Comment(c) => segs.push(Seg::Plain(format!("<!-- {} -->", c.replace("--", "- -").replace(['\n', '\r'], " ")), *x)),
                Item::LinkOn(u) => segs.push(Seg::LinkOn(u.clone(), *x)),
                Item::LinkOff => segs.push(Seg::LinkOff),
            }
        }
        let mut out = String::new();
        let mut open: Vec<(Attr, bool)> = vec![];
        let mut floor = 0;
        let mut link: Option<String> = None;
        let count = segs.len();
        let mut k = 0;
        while k < count {
            match &segs[k] {
                Seg::LinkOn(u, x) => {
                    let end = segs[k + 1..].iter().position(|s| matches!(s, Seg::LinkOff)).map(|p| k + 1 + p).unwrap_or(count);
                    let inner = &segs[k + 1..end];
                    if let [Seg::Text(w, t)] = inner {
                        let t: String = t.iter().collect();
                        if w.is_empty() && autolink_url(u) && (t == *u || (u.to_ascii_lowercase().starts_with("mailto:") && u[7..] == t && !t.is_empty())) {
                            close_marks(&mut out, &mut open, 0, &|_| false);
                            out.push_str(&format!("<{u}>"));
                            k = end + 1;
                            continue;
                        }
                    }
                    // what the whole link text has stays open around it
                    if let Some(x) = x {
                        close_marks(&mut out, &mut open, 0, &|a| x.has(a));
                        open_marks(&mut out, &mut open, *x, &mark_order(&segs, k + 1));
                    }
                    out.push('[');
                    floor = open.len();
                    link = Some(u.clone());
                }
                Seg::LinkOff => {
                    if let Some(u) = link.take() {
                        close_marks(&mut out, &mut open, floor, &|_| false);
                        out.push_str(&format!("]({})", link_dest(&u)));
                        floor = 0;
                    }
                }
                Seg::Text(w, t) => {
                    close_marks(&mut out, &mut open, floor, &|a| w.has(a));
                    let then = match segs.get(k + 1) {
                        Some(Seg::LinkOn(..)) => 2,
                        Some(Seg::Raw(..)) => 1,
                        _ => 0,
                    };
                    let esc = escape(t, link.is_some(), then, k == 0 && w.is_empty(), k + 1 == count && w.is_empty(), !w.is_empty());
                    if t.iter().all(|&c| is_blank(c)) {
                        out.push_str(&esc);
                    } else {
                        let lead = esc.len() - esc.trim_start_matches([' ', '\t']).len();
                        out.push_str(&esc[..lead]);
                        open_marks(&mut out, &mut open, *w, &mark_order(&segs, k));
                        out.push_str(&esc[lead..]);
                    }
                }
                Seg::Code(w, t) => {
                    close_marks(&mut out, &mut open, floor, &|a| w.has(a));
                    open_marks(&mut out, &mut open, *w, &mark_order(&segs, k));
                    out.push_str(&code_span(t));
                }
                Seg::Raw(w, t) => {
                    close_marks(&mut out, &mut open, floor, &|a| w.has(a));
                    open_marks(&mut out, &mut open, *w, &mark_order(&segs, k));
                    out.push_str(t);
                }
                Seg::Plain(t, x) => {
                    if let Some(x) = x {
                        close_marks(&mut out, &mut open, floor, &|a| x.has(a));
                        open_marks(&mut out, &mut open, *x, &mark_order(&segs, k + 1));
                    }
                    out.push_str(t);
                }
            }
            k += 1;
        }
        if let Some(u) = link.take() {
            close_marks(&mut out, &mut open, floor, &|_| false);
            out.push_str(&format!("]({})", link_dest(&u)));
        }
        close_marks(&mut out, &mut open, 0, &|_| false);
        out
    }

    /// A table as a pipe table; the first row is the header.
    fn table(&mut self, els: &[El]) -> String {
        // the cells of every row
        let mut rows: Vec<Vec<&[El]>> = vec![];
        let mut start: Option<usize> = None;
        for (k, e) in els.iter().enumerate() {
            if !matches!(e, El::Code(Code::Row | Code::Cell)) {
                continue;
            }
            if let (Some(s), Some(r)) = (start.take(), rows.last_mut()) {
                r.push(&els[s..k]);
            }
            if matches!(e, El::Code(Code::Row)) || rows.is_empty() {
                rows.push(vec![]);
            }
            if matches!(e, El::Code(Code::Cell)) {
                start = Some(k + 1);
            }
        }
        if let (Some(s), Some(r)) = (start, rows.last_mut()) {
            r.push(&els[s..]);
        }
        rows.retain(|r| !r.is_empty());
        let mut aligns: Vec<Justify> = vec![];
        let mut text: Vec<Vec<String>> = vec![];
        for (r, row) in rows.iter().enumerate() {
            let mut line = vec![];
            for cell in row {
                let blks = self.split(cell);
                self.link = None;
                if r == 0 {
                    // the justification at the end of a header cell is
                    // that of its column
                    aligns.push(self.just);
                }
                // the header row is bold by itself
                let mut mask = AttrSet::default();
                mask.set(Attr::Bold, r == 0);
                let mut parts: Vec<String> = vec![];
                for b in blks {
                    if let Blk::Line(l) = b {
                        let t = if l.is_empty() { String::new() } else { self.inline(&l.atoms, mask) };
                        // (never more than one empty line)
                        if !(t.is_empty() && parts.len() >= 2 && parts[parts.len() - 1].is_empty() && parts[parts.len() - 2].is_empty()) {
                            parts.push(t);
                        }
                    }
                }
                while parts.last().is_some_and(|p| p.is_empty()) {
                    parts.pop();
                }
                line.push(parts.join("<br>").replace('|', "\\|"));
            }
            text.push(line);
        }
        let ncols = text.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        let mut s = String::new();
        if text.is_empty() {
            text.push(vec![]);
        }
        for (k, r) in text.iter().enumerate() {
            s.push('|');
            for c in 0..ncols {
                let t = r.get(c).map(|x| x.as_str()).unwrap_or("");
                s.push(' ');
                s.push_str(if t.is_empty() { " " } else { t });
                s.push_str(" |");
            }
            s.push('\n');
            if k == 0 {
                s.push('|');
                for c in 0..ncols {
                    s.push_str(match aligns.get(c) {
                        Some(Justify::Center) => " :---: |",
                        Some(Justify::Right) => " ---: |",
                        _ => " --- |",
                    });
                }
                s.push('\n');
            }
        }
        s.trim_end().to_string()
    }
}

/// Write a document as Markdown.
pub fn write(doc: &Document) -> String {
    let mut w = Writer { notes: vec![], attrs: AttrSet::default(), mono: false, just: doc.init.justify, base_mono: is_mono(&doc.init.font.name), link: None };
    let blks = w.split(&doc.els);
    let mut s = w.emit(blks).join("\n");
    if !w.notes.is_empty() {
        if !s.is_empty() {
            s.push_str("\n\n");
        }
        for (k, n) in w.notes.iter().enumerate() {
            s.push_str(&format!("[^{}]: {}\n", k + 1, n));
        }
    }
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt_build::debug_text;

    fn rd(s: &str) -> Document {
        read(s.as_bytes()).unwrap()
    }
    fn has(d: &Document, c: Code) -> bool {
        d.els.contains(&El::Code(c))
    }
    /// Reading what was written gives the same elements, and writing
    /// those the same text.
    fn stable(s: &str) -> String {
        let d1 = rd(s);
        let m1 = write(&d1);
        let d2 = rd(&m1);
        assert_eq!(d1.els, d2.els, "\n--- source:\n{s}\n--- written:\n{m1}");
        let m2 = write(&d2);
        assert_eq!(m1, m2);
        m1
    }

    #[test]
    fn headings_and_emphasis() {
        let d = rd("# Überschrift\n\nGrüße **fett** und *kursiv* und _auch_ und ***beides*** ~~weg~~ `code`\nzweite Zeile  \nneue Zeile\n\nПривет\n=====\n\nsnake_case_name 2 * 3\n");
        assert_eq!(d.plain_text(0, d.len()), "Überschrift\n\nGrüße fett und kursiv und auch und beides weg code zweite Zeile\nneue Zeile\n\nПривет\n\nsnake_case_name 2 * 3");
        for a in [Attr::Bold, Attr::ExtraLarge, Attr::Italic, Attr::Strikeout] {
            assert!(has(&d, Code::AttrOn(a)), "{a:?}");
        }
        assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::Font(f)) if f.name == "Courier New")));
    }

    #[test]
    fn lists_quotes_code() {
        let d = rd("- eins\n- zwei\n  1. drei\n  2. vier\n* [x] fertig\n\n> zitat\n> weiter\n\n```\nlet a  = 1;\n```\n\n    code\n\n---\nEnde &amp; &euro; \\*nicht\\* [Link](http://x.de) ![Bild](b.png)\n");
        assert_eq!(d.plain_text(0, d.len()), "• eins\n• zwei\n\t1. drei\n\t2. vier\n☒ fertig\n\n\tzitat weiter\n\nlet a  = 1;\n\ncode\n\n────────────────────────────────────────\n\nEnde & € *nicht* Link ![Bild](b.png)");
        assert!(has(&d, Code::Link("http://x.de".into())) && has(&d, Code::LinkEnd));
    }

    #[test]
    fn table_and_footnote() {
        let d = rd("Text[^1] hier.\n\n| A | B |\n|:--|--:|\n| ä | **б** |\n| 1 \\| 2 |\n\n[^1]: Die Fußnote.\n");
        let t = debug_text(&d.els);
        assert_eq!(t, "Text[Footnote:Die Fußnote.] hier.\n\n[Tbl]\n[Row]|A|B\n[Row]|ä|б\n[Row]|1 | 2|[TblOff]");
        assert!(has(&d, Code::Just(Justify::Right)));
    }

    #[test]
    fn export() {
        let mut b = Builder::new(base_font());
        b.set(Attr::Bold, true);
        b.set(Attr::VeryLarge, true);
        b.text("Titel");
        b.set(Attr::Bold, false);
        b.set(Attr::VeryLarge, false);
        b.para();
        b.text("Grüße ");
        b.set(Attr::Bold, true);
        b.text("fett ");
        b.set(Attr::Italic, true);
        b.text("beides");
        b.set(Attr::Italic, false);
        b.set(Attr::Bold, false);
        b.text(" a*b_c ");
        b.set(Attr::Superscript, true);
        b.text("2");
        b.set(Attr::Superscript, false);
        b.note(NoteKind::Foot, "Fußnote".chars().map(El::Ch).collect());
        b.para();
        b.table_start(vec![]);
        for r in 0..2 {
            b.row();
            for c in 0..2 {
                b.cell();
                b.text(&format!("Z{r}{c}|"));
            }
        }
        b.table_end();
        b.text("Привет");
        let d = b.into_document(base_font());
        let md = write(&d);
        assert_eq!(md, "## Titel\n\nGrüße **fett *beides*** a\\*b_c <sup>2</sup>[^1]\n\n| Z00\\| | Z01\\| |\n| --- | --- |\n| Z10\\| | Z11\\| |\n\nПривет\n\n[^1]: Fußnote\n");
        // round trip
        let d2 = read(md.as_bytes()).unwrap();
        // reading keeps the blank line between paragraphs ([HRt][HRt]);
        // compare without those empty lines
        let mut single = d2.els.clone();
        single.dedup_by(|b, a| *a == El::Code(Code::HRt) && *b == El::Code(Code::HRt));
        // (and the paragraph break after the table, which Markdown needs)
        single.dedup_by(|b, a| *a == El::Code(Code::TblOff) && *b == El::Code(Code::HRt));
        assert_eq!(debug_text(&single), debug_text(&d.els));
        for a in [Attr::Bold, Attr::Italic, Attr::Superscript, Attr::VeryLarge] {
            assert!(has(&d2, Code::AttrOn(a)), "{a:?}");
        }
        let at = d2.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).unwrap();
        assert_eq!(crate::tables::parse(&d2.els, at).unwrap().rows.len(), 2);
        // and from here on it stays as it is
        assert_eq!(write(&d2), md);
    }

    #[test]
    fn never_panics() {
        let samples = ["*", "**", "***a", "[", "[^", "[^1]:", "`", "``a`", "|", "| a |\n|", "| a |\n|-|", "#", "> ", ">>>>>>>>", "- ", "1.", "~~", "<", "<a", "&", "&#", "![", "[a](", "[a][", "    ", "```", "\\", "a  \n", "_a_b_", "***a**b*", "<!--", "<!-- a", "<!---->", "[a]: b", "![a][b]", "[](", "<>", "<@>", "- [ ]", "1. [x]"];
        for s in samples {
            let d = read(s.as_bytes()).unwrap();
            let _ = write(&d);
        }
        let full = "# Ü\n\n**f** *k* | a | b |\n|---|---|\n| 1 | 2 |\n\n- x\n  - y\n\n> q\n\n[^1]: n\n";
        for n in 0..full.len() {
            if full.is_char_boundary(n) {
                let _ = read(&full.as_bytes()[..n]);
            }
        }
        let mut x: u32 = 7;
        let mut g = String::new();
        for _ in 0..4000 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            g.push("*_`[]()!<>&#|-~\\\n ab1.^:"[(x >> 16) as usize % 23..].chars().next().unwrap_or('a'));
        }
        let d = read(g.as_bytes()).unwrap();
        let _ = write(&d);
    }

    #[test]
    fn sample_file() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/sample.md");
        let Ok(bytes) = std::fs::read(p) else { return };
        let d = read(&bytes).unwrap();
        let t = debug_text(&d.els);
        assert!(t.contains("Grüße") && t.contains("Привет") && t.contains("[Tbl]") && t.contains("[Footnote:"), "{t}");
        stable(&String::from_utf8_lossy(&bytes));
    }

    const RICH: &str = r#"# Title *one*

Intro with **bold**, *italic*, ***both***, ~~gone~~, <u>under</u>, H<sub>2</sub>O, x<sup>2</sup> and `code *raw*`.
Second line after a hard break\
third line. Escapes: \*stars\*, snake_case, a_b_c, 2 * 3, \# no heading, [brackets], AT&T, &amp;amp;, 1 < 2, C:\dir, a\_.

Setext two
----------

A [link](https://example.org/a_b?x=1&y=2), an autolink <https://example.org>, mail <me@example.org>,
a [reference link][ref], a [shortcut] and **[bold link](http://x.de "title") in bold**, [*partly* italic](<my file.md>).

[ref]: https://example.com/ref
[shortcut]: https://example.com/short "Title"

### Lists
- one
- two
  - nested **bold**
    - deep
      continued\
      and broken
  - back
- [ ] todo
- [x] done

Text directly before a list:
* star item

3. three
4. four
   1. sub `code`
5) five
6. [ ] numbered task

#### Quote

> quoted *text*
> more
>
> - item in quote
>   - nested in quote
> - second
>
> > nested quote
>
> ```
> code in quote
> ```

##### Code

```rust
fn main() {
    let s = "*not bold* | `x`";

    println!("{s}");
}
```

Text between.

    indented code
    	with a tab

A fence in a fence:

````
```
fence in code
```
````

###### Table \#

| Left | Center | Right |
|:-----|:------:|------:|
| a \| b | **c** | 1 |
| line1<br>line2 | `x\|y` |  |
| [l](http://t.co) | *i* | ~~s~~ |

---

Image: ![alt *text*](pic.png "A title"), linked [![logo](l.png)](https://example.org) and a footnote[^n].

<!-- a comment -->

Last&nbsp;line with a hard space.

[^n]: The note with **bold**
    and a second line.

    - a list in the note
"#;

    #[test]
    fn rich_document_is_stable() {
        let m = stable(RICH);
        // every construct is still there as Markdown
        for part in [
            "# Title *one*",
            "## Setext two",
            "###### Table \\#",
            "hard break\\\nthird line",
            "\\*stars\\*, snake_case, a_b_c, 2 * 3, # no heading, [brackets], AT&T, \\&amp;, 1 < 2, C:\\dir, a\\_.",
            "[link](https://example.org/a_b?x=1&y=2)",
            "<https://example.org>",
            "<mailto:me@example.org>",
            "[reference link](https://example.com/ref)",
            "[shortcut](https://example.com/short)",
            "**[bold link](http://x.de) in bold**",
            "[*partly* italic](<my file.md>)",
            "- two\n  - nested **bold**\n    - deep continued\\\n      and broken\n  - back\n- [ ] todo\n- [x] done",
            "Text directly before a list:\n- star item",
            "3. three\n4. four\n   1. sub `code`\n5) five\n6. [ ] numbered task",
            "> quoted *text* more\n>\n> - item in quote\n>   - nested in quote\n> - second\n>\n> > nested quote\n>\n> ```\n> code in quote\n> ```",
            "```rust\nfn main() {\n    let s = \"*not bold* | `x`\";\n\n    println!(\"{s}\");\n}\n```",
            "```\nindented code\n\twith a tab\n```",
            "````\n```\nfence in code\n```\n````",
            "| Left | Center | Right |\n| --- | :---: | ---: |\n| a \\| b | **c** | 1 |\n| line1<br>line2 | `x\\|y` |   |\n| [l](http://t.co) | *i* | ~~s~~ |",
            "\n\n---\n\n",
            "![alt *text*](pic.png \"A title\")",
            "[![logo](l.png)](https://example.org)",
            "<!-- a comment -->",
            "Last&nbsp;line",
            "[^1]: The note with **bold** and a second line.\n\n    - a list in the note\n",
        ] {
            assert!(m.contains(part), "missing:\n{part}\n--- in:\n{m}");
        }
    }

    #[test]
    fn headings_six_levels() {
        let src = "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n";
        let d = rd(src);
        for n in 1..=6u8 {
            assert!(has(&d, Code::Mark(Mark::Begin { list: false, n })), "{n}");
            assert!(has(&d, Code::Mark(Mark::End { list: false, n })), "{n}");
        }
        for a in [Attr::ExtraLarge, Attr::VeryLarge, Attr::Large, Attr::Small, Attr::Fine] {
            assert!(has(&d, Code::AttrOn(a)), "{a:?}");
        }
        assert_eq!(stable(src), src);
        // Setext, closing #s, a heading directly under text
        assert_eq!(stable("Text\n# One #\nmore\n\nTwo\nlines\n---\n"), "Text\n\n# One\n\nmore\n\n## Two lines\n");
        // without marks: bold and large is a heading
        let mut b = Builder::new(base_font());
        b.set(Attr::Bold, true);
        b.set(Attr::Large, true);
        b.text("Drei");
        let d = b.into_document(base_font());
        assert_eq!(write(&d), "### Drei\n");
    }

    #[test]
    fn links() {
        let d = rd("[a *b*](http://x.de/?q=(1) \"T\") <https://y.de> [r][R] [R][] [R] [no]\n\n[r]: <http://z.de/a b>\n");
        let urls: Vec<&str> = d.els.iter().filter_map(|e| if let El::Code(Code::Link(u)) = e { Some(u.as_str()) } else { None }).collect();
        assert_eq!(urls, ["http://x.de/?q=(1)", "https://y.de", "http://z.de/a b", "http://z.de/a b", "http://z.de/a b"]);
        assert_eq!(d.plain_text(0, d.len()), "a b https://y.de r R R [no]");
        assert_eq!(stable("[a *b*](http://x.de/?q=(1) \"T\") <https://y.de> [r][R] [no]\n\n[r]: <http://z.de/a b>\n"), "[a *b*](http://x.de/?q=(1)) <https://y.de> [r](<http://z.de/a b>) [no]\n");
        // brackets that could be taken for a link are escaped
        assert_eq!(stable("\\[x\\](y) \\[^1\\] [z]\n"), "[x\\](y) \\[^1] [z]\n");
        // a link over a line break is one link per line
        assert_eq!(stable("[a\\\nb](u)\n"), "[a](u)\\\n[b](u)\n");
    }

    #[test]
    fn lists() {
        let src = "- a\n- b\n  - c\n    - d\n  - e\n\n7. x\n8. y\n   - z\n- [ ] open\n- [x] done\n";
        let d = rd(src);
        assert_eq!(d.plain_text(0, d.len()), "• a\n• b\n\t• c\n\t\t• d\n\t• e\n\n7. x\n8. y\n\t• z\n☐ open\n☒ done");
        assert_eq!(stable(src), src);
        // other markers, loose items, continuation lines
        assert_eq!(stable("* a\n\n+ b\n  more\n  text  \n  broken\n1) c\n"), "- a\n\n- b more text\\\n  broken\n1) c\n");
        // typed in the program: bullets and numbers at the start of lines
        let mut b = Builder::new(base_font());
        b.text("• eins\n");
        b.code_fmt(Code::Tab);
        b.text("1. zwei\n- kein Punkt\n+ auch nicht\n2. drei");
        let d = b.into_document(base_font());
        assert_eq!(write(&d), "- eins\n  1. zwei\\\n     \\- kein Punkt\\\n     \\+ auch nicht\n2. drei\n");
    }

    #[test]
    fn quotes() {
        let src = "> a\n> b\n>\n> > c\n>\n> - d\n>   - e\n>\n> # H\n\nf\n";
        let d = rd(src);
        assert_eq!(d.els.iter().filter(|e| **e == El::Code(Code::Indent)).count(), 1 + 2 + 1 + 1 + 1);
        assert_eq!(stable(src), "> a b\n>\n> > c\n>\n> - d\n>   - e\n>\n> # H\n\nf\n");
    }

    #[test]
    fn code() {
        let src = "Text `a*b` and `` ` `` and `  x  `.\n\n```python\nif a < b:\n    print(\"*x*\")\n\n# no heading\n```\n\nText\n\n~~~\n- no list\n~~~\n";
        let d = rd(src);
        assert_eq!(d.plain_text(0, d.len()), "Text a*b and ` and  x .\n\nif a < b:\n    print(\"*x*\")\n\n# no heading\n\nText\n\n- no list");
        assert!(has(&d, Code::Comment("```python".into())));
        assert_eq!(stable(src), "Text `a*b` and `` ` `` and `  x  `.\n\n```python\nif a < b:\n    print(\"*x*\")\n\n# no heading\n```\n\nText\n\n```\n- no list\n```\n");
        // (two code blocks with nothing between them become one)
        assert_eq!(stable("```\na\n```\n\n```\nb\n```\n"), "```\na\n\nb\n```\n");
        // a document in a typewriter font has no code
        let mut d = rd("a `b`\n");
        d.init.font = FontSpec::default_font();
        d.els.retain(|e| !matches!(e, El::Code(Code::Font(_))));
        assert_eq!(write(&d), "a b\n");
    }

    #[test]
    fn rules_and_breaks() {
        assert_eq!(stable("a\n***\nb\n\n- - -\n\n___\n"), "a\n\n---\n\nb\n\n---\n\n---\n");
        assert_eq!(stable("a  \nb\\\nc<br>d\n\n\n\ne\n"), "a\\\nb\\\nc\\\nd\n\ne\n");
        // text that looks like a block
        let mut b = Builder::new(base_font());
        b.text("# kein Titel\n> kein Zitat\n---\n===\n| a |\n|---|\n1986.\n    eingerückt\nEnde\\");
        let d = b.into_document(base_font());
        let md = write(&d);
        assert_eq!(md, "\\# kein Titel\\\n\\> kein Zitat\\\n\\---\\\n\\===\\\n| a |\\\n\\|---|\\\n1986\\.\\\neingerückt\\\nEnde\\\\\n");
        assert_eq!(rd(&md).plain_text(0, 999), "# kein Titel\n> kein Zitat\n---\n===\n| a |\n|---|\n1986.\neingerückt\nEnde\\");
    }

    #[test]
    fn escapes() {
        // only what would be syntax is escaped
        let mut b = Builder::new(base_font());
        b.text("a_b _c_ *d* 2 * 3 x*y `q` <tag> a < b ~~s~~ ~ [l](u) [m] !n &copy; R&D \\* \\n 1. 2.5 #1 a|b");
        let d = b.into_document(base_font());
        let md = write(&d);
        assert_eq!(md, "a_b \\_c\\_ \\*d\\* 2 * 3 x\\*y \\`q\\` \\<tag> a < b \\~\\~s\\~\\~ ~ [l\\](u) [m] !n \\&copy; R&D \\\\\\* \\n 1. 2.5 #1 a|b\n");
        assert_eq!(rd(&md).els, d.els);
        assert_eq!(stable(&md), md);
    }

    #[test]
    fn tables() {
        let src = "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n|   | d |   |\n";
        let d = rd(src);
        let justs: Vec<Justify> = d.els.iter().filter_map(|e| if let El::Code(Code::Just(j)) = e { Some(*j) } else { None }).collect();
        use Justify::*;
        assert_eq!(justs, [Center, Right, Left, Center, Right, Left, Center, Right, Left]);
        assert_eq!(stable(src), "| L | C | R |\n| --- | :---: | ---: |\n| a | b | c |\n|   | d |   |\n");
        // as Table Edit, Format, Justify does it: [Just] in the cells of
        // a column, the one before again in the next cell
        let mut b = Builder::new(base_font());
        b.table_start(vec![]);
        for r in 0..2 {
            b.row();
            b.cell();
            b.text(&format!("a{r}"));
            b.cell();
            b.els.push(El::Code(Code::Just(Right)));
            b.set(Attr::Bold, r == 0);
            b.text(&format!("b{r}"));
            b.set(Attr::Bold, false);
            b.para();
            b.text("x|y");
            b.cell();
            b.els.push(El::Code(Code::Just(Left)));
            b.text(&format!("c{r}"));
        }
        b.table_end();
        let d = b.into_document(base_font());
        // (header cells are not marked bold: the header row is bold)
        assert_eq!(write(&d), "| a0 | b0<br>x\\|y | c0 |\n| --- | ---: | --- |\n| a1 | b1<br>x\\|y | c1 |\n");
    }

    #[test]
    fn images_and_comments() {
        let src = "![a](b.png) ![c *d*](<e f.png> \"T\") ![r][x] <!-- note -->\n\n[x]: g.png 'G'\n";
        let d = rd(src);
        assert_eq!(d.plain_text(0, d.len()), "![a](b.png) ![c *d*](<e f.png> \"T\") ![r](g.png 'G') ");
        assert!(has(&d, Code::Comment("note".into())));
        assert_eq!(stable(src), "![a](b.png) ![c *d*](<e f.png> \"T\") ![r](g.png 'G') <!-- note -->\n");
    }

    #[test]
    fn footnotes() {
        let src = "A[^a] and B[^b].\n\n[^a]: First.\n[^b]: Second\n    line.\n\n    New paragraph.\n";
        let d = rd(src);
        assert_eq!(debug_text(&d.els), "A[Footnote:First.] and B[Footnote:Second line.\n\nNew paragraph.].");
        assert_eq!(stable(src), "A[^1] and B[^2].\n\n[^1]: First.\n[^2]: Second line.\n\n    New paragraph.\n");
    }

    #[test]
    fn random_text_is_stable() {
        // whatever is read: writing and reading it again changes nothing
        let alphabet: Vec<char> = "*_`[]()!<>&#|-~\\\n\n  ab1.^:>+=x".chars().collect();
        let mut x: u32 = 12345;
        // (and pieces of Markdown in any order)
        let pieces = [
            "# ", "### ", "- ", "  - ", "    - ", "1. ", "7) ", "> ", "> > ", "```\n", "```c\n", "~~~\n", "| a | b |\n|---|:-:|\n", "| ", " | ", "**", "*", "***", "`", "[x](u)", "[", "](v w)", "<br>", "\\\n", "\n", "\n\n", "text ", "  ", "[^1]", "![a](b)", "<u>", "</u>", "<b>", "</b>", "~~", "---\n", "    ", "\t", "\n[^1]: note\n", "<http://a.b>", "&amp;", "&nbsp;", "- [ ] ", "- [x] ", "===\n", "<!-- c -->", "_", "word", "a", " ", "\\", "☐ ", "• ", "──────\n", "────────────────────────────────────────\n", "[r]", "\n[r]: http://r\n", "<sup>", "</sup>", "2.", "#",
        ];
        for round in 0..6000 {
            let mut g = String::new();
            for _ in 0..(20 + round % 60) {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                if round % 2 == 0 {
                    g.push(alphabet[(x >> 16) as usize % alphabet.len()]);
                } else {
                    g.push_str(pieces[(x >> 16) as usize % pieces.len()]);
                }
            }
            let d1 = rd(&g);
            let m1 = write(&d1);
            let d2 = rd(&m1);
            if d1.els != d2.els {
                let k = d1.els.iter().zip(&d2.els).position(|(a, b)| a != b).unwrap_or(d1.els.len().min(d2.els.len()));
                let show = |els: &[El]| els[k.saturating_sub(12).min(els.len())..(k + 12).min(els.len())].iter().map(|e| match e {
                    El::Ch(c) => c.to_string(),
                    El::Code(c) => crate::reveal::code_text(c),
                }).collect::<String>();
                panic!("round {round}\n--- source:\n{g:?}\n--- written:\n{m1:?}\n--- first:  {}\n--- second: {}", show(&d1.els), show(&d2.els));
            }
            assert_eq!(write(&d2), m1, "round {round}");
        }
    }
}
