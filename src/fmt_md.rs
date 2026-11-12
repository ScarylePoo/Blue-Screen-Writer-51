//! Markdown (.md) reader and writer.
//!
//! Reading (CommonMark + GitHub tables/strikethrough/footnotes, the parts
//! that matter for a document): ATX and Setext headings (bold + Large /
//! Very Large / Extra Large), paragraphs (lines joined; two trailing
//! spaces or a backslash = line break), `*em*` `_em_` `**strong**`
//! `~~strike~~` `` `code` `` (Courier New), escapes, entities, links (text
//! kept), images (alt text), bullet and numbered lists (as text with
//! "• " / "1. ", nesting by Tab), block quotes (`[->Indent]`), fenced and
//! indented code blocks, pipe tables (`[Tbl Def]…`), horizontal rules,
//! footnotes `[^1]` → `[Footnote]`, `<u>`/`<sup>`/`<sub>`/`<br>` tags.
//!
//! Writing: paragraphs separated by blank lines, `**bold**`, `*italic*`,
//! `~~strike~~`, `<u>`, `<sup>`, `<sub>`, all-bold large paragraphs as
//! headings, tables as pipe tables, footnotes/endnotes as `[^n]`.

use crate::doc::*;
use crate::fmt_build::Builder;
use std::collections::HashMap;

// ===========================================================================
// reading

fn base_font() -> FontSpec {
    FontSpec { name: "Times New Roman".into(), size_cpt: 1200 }
}

struct Md<'a> {
    b: Builder,
    defs: &'a HashMap<String, String>,
    depth: u32,
    quote: usize,
    /// indentation of the open list levels
    list_indent: Vec<usize>,
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

/// `- x`, `* x`, `+ x`, `12. x`, `3) x` → (indent, marker, text)
fn list_item(l: &str) -> Option<(usize, String, &str)> {
    let ind = indent_of(l);
    let t = l.trim_start();
    let mut chars = t.char_indices();
    let (_, c) = chars.next()?;
    let (marker, rest) = if matches!(c, '-' | '*' | '+') {
        ("• ".to_string(), &t[1..])
    } else if c.is_ascii_digit() {
        let nd = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if nd > 9 {
            return None;
        }
        let d = t[nd..].chars().next()?;
        if d != '.' && d != ')' {
            return None;
        }
        (format!("{}{} ", &t[..nd], d), &t[nd + 1..])
    } else {
        return None;
    };
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let mut rest = rest.trim_start();
    let mut marker = marker;
    if marker == "• " {
        if let Some(r) = rest.strip_prefix("[ ] ") {
            marker = "☐ ".into();
            rest = r;
        } else if let Some(r) = rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
            marker = "☒ ".into();
            rest = r;
        }
    }
    Some((ind, marker, rest))
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

fn split_row(l: &str) -> Vec<String> {
    let t = l.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = if t.ends_with('|') && !t.ends_with("\\|") { &t[..t.len() - 1] } else { t };
    let mut cells = vec![];
    let mut cur = String::new();
    let mut code = false;
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' if it.peek() == Some(&'|') => {
                cur.push('|');
                it.next();
            }
            '`' => {
                code = !code;
                cur.push(c);
            }
            '|' if !code => cells.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

fn table_sep(l: &str) -> Option<Vec<Justify>> {
    if !l.contains('-') {
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

impl<'a> Md<'a> {
    fn indent_prefix(&mut self) {
        if self.b.at_para_start() && self.quote > 0 && !self.b.in_table() {
            for _ in 0..self.quote.min(8) {
                self.b.code(Code::Indent);
            }
        }
    }

    fn put(&mut self, c: char) {
        self.indent_prefix();
        self.b.ch(c);
    }

    fn mono(&mut self, on: bool) {
        self.b.font = if on { FontSpec { name: "Courier New".into(), size_cpt: 1200 } } else { base_font() };
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
                self.flush_para(&mut para, 0);
                // Markdown separates paragraphs with a blank line: keep it
                self.b.blank_line();
                i += 1;
                continue;
            }
            // fenced code
            if ind < 4 {
                if let Some((fc, fl)) = fence(t) {
                    self.flush_para(&mut para, 0);
                    self.list_indent.clear();
                    i += 1;
                    self.mono(true);
                    while i < n {
                        let l = lines[i].trim_end();
                        if fence(l).is_some_and(|(c, len)| c == fc && len >= fl) && l.trim().chars().all(|x| x == fc) {
                            i += 1;
                            break;
                        }
                        let l = l.strip_prefix(&" ".repeat(ind.min(3))).unwrap_or(l);
                        for c in l.chars() {
                            self.put(c);
                        }
                        self.b.para();
                        i += 1;
                    }
                    self.mono(false);
                    continue;
                }
            }
            // indented code (not inside a paragraph or list)
            if ind >= 4 && para.is_empty() && self.list_indent.is_empty() {
                self.mono(true);
                while i < n {
                    if lines[i].trim().is_empty() {
                        // blank lines belong to the block only if more code follows
                        let next = lines[i..].iter().position(|l| !l.trim().is_empty()).map(|k| i + k);
                        match next {
                            Some(k) if indent_of(&lines[k]) >= 4 => {
                                for _ in i..k {
                                    self.b.para();
                                }
                                i = k;
                            }
                            _ => break,
                        }
                    }
                    if indent_of(&lines[i]) < 4 {
                        break;
                    }
                    let l = lines[i].trim_end();
                    let mut skip = 4;
                    let s: String = l.chars().skip_while(|&c| {
                        if skip > 0 && c == ' ' {
                            skip -= 1;
                            true
                        } else if skip > 0 && c == '\t' {
                            skip = 0;
                            true
                        } else {
                            false
                        }
                    }).collect();
                    for c in s.chars() {
                        self.put(c);
                    }
                    self.b.para();
                    i += 1;
                }
                self.mono(false);
                continue;
            }
            // ATX heading
            if ind < 4 {
                if let Some((level, text)) = atx(t) {
                    self.flush_para(&mut para, 0);
                    self.list_indent.clear();
                    self.heading(level, text);
                    i += 1;
                    continue;
                }
            }
            // Setext heading underline
            if !para.is_empty() && ind < 4 {
                let tt = t.trim();
                if !tt.is_empty() && (tt.chars().all(|c| c == '=') || (tt.chars().all(|c| c == '-') && self.list_indent.is_empty())) {
                    let level = if tt.starts_with('=') { 1 } else { 2 };
                    let text = para.join(" ");
                    para.clear();
                    self.heading(level, &text);
                    i += 1;
                    continue;
                }
            }
            if ind < 4 && is_hr(t) {
                self.flush_para(&mut para, 0);
                self.list_indent.clear();
                self.b.text(&"─".repeat(40));
                self.b.para();
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
                self.flush_para(&mut para, 0);
                self.list_indent.clear();
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
                self.flush_para(&mut para, 0);
                self.quote -= 1;
                continue;
            }
            // list item
            if let Some((ind, marker, text)) = list_item(line) {
                if para.is_empty() || !self.list_indent.is_empty() || marker == "• " || marker.starts_with("1") {
                    self.flush_para(&mut para, 0);
                    while self.list_indent.last().is_some_and(|&top| top > ind) {
                        self.list_indent.pop();
                    }
                    if self.list_indent.last() != Some(&ind) {
                        self.list_indent.push(ind);
                    }
                    let depth = self.list_indent.len().saturating_sub(1).min(8);
                    i += 1;
                    let mut item = vec![text.to_string()];
                    // continuation lines
                    while i < n {
                        let l = &lines[i];
                        if l.trim().is_empty() || list_item(l).is_some() || atx(l).is_some() || fence(l).is_some() || l.trim_start().starts_with('>') || is_hr(l) {
                            break;
                        }
                        item.push(l.trim().to_string());
                        i += 1;
                    }
                    for _ in 0..depth {
                        self.b.code_fmt(Code::Tab);
                    }
                    for c in marker.chars() {
                        self.put(c);
                    }
                    let mut it = item;
                    self.flush_para(&mut it, 0);
                    continue;
                }
            }
            if ind == 0 && para.is_empty() {
                self.list_indent.clear();
            }
            para.push(t.to_string());
            // keep hard-break markers (two trailing spaces)
            if line.ends_with("  ") {
                if let Some(last) = para.last_mut() {
                    last.push_str("  ");
                }
            }
            i += 1;
        }
        self.flush_para(&mut para, 0);
    }

    fn heading(&mut self, level: usize, text: &str) {
        let mut base = AttrSet::default();
        base.set(Attr::Bold, true);
        match level {
            1 => base.set(Attr::ExtraLarge, true),
            2 => base.set(Attr::VeryLarge, true),
            3 => base.set(Attr::Large, true),
            _ => {}
        }
        let chars: Vec<char> = text.chars().collect();
        self.inline(&chars, base);
        self.b.attrs = AttrSet::default();
        self.b.para();
    }

    fn flush_para(&mut self, para: &mut Vec<String>, _x: u8) {
        if para.is_empty() {
            return;
        }
        let mut s = String::new();
        let k = para.len();
        for (j, l) in para.drain(..).enumerate() {
            let hard = l.ends_with("  ") || (l.ends_with('\\') && !l.ends_with("\\\\"));
            let l = l.trim_end();
            let l = if hard && l.ends_with('\\') { &l[..l.len() - 1] } else { l };
            s.push_str(l.trim_start());
            if j + 1 < k {
                s.push(if hard { '\n' } else { ' ' });
            }
        }
        let chars: Vec<char> = s.chars().collect();
        self.inline(&chars, AttrSet::default());
        self.b.attrs = AttrSet::default();
        self.b.para();
    }

    fn table(&mut self, rows: &[Vec<String>], aligns: &[Justify]) {
        let ncols = aligns.len();
        self.b.table_start(vec![]);
        for (r, row) in rows.iter().enumerate() {
            self.b.row();
            for c in 0..ncols {
                self.b.cell();
                self.b.just = aligns[c];
                let text = row.get(c).cloned().unwrap_or_default();
                let mut base = AttrSet::default();
                if r == 0 {
                    base.set(Attr::Bold, true);
                }
                let chars: Vec<char> = text.chars().collect();
                self.inline(&chars, base);
                self.b.attrs = AttrSet::default();
            }
        }
        self.b.just = Justify::Left;
        self.b.table_end();
    }

    /// Footnote text as a separate element list.
    fn note_els(&self, text: &str) -> Vec<El> {
        let mut sub = Md { b: Builder::new(base_font()), defs: self.defs, depth: self.depth + 1, quote: 0, list_indent: vec![] };
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if self.depth < 4 {
            sub.blocks(&lines);
        }
        sub.b.finish()
    }

    fn inline(&mut self, s: &[char], base: AttrSet) {
        let mut st = AttrSet::default(); // toggled by * _ ~~ and tags
        let mut i = 0;
        let n = s.len();
        let apply = |b: &mut Builder, st: AttrSet| b.attrs = AttrSet(base.0 | st.0);
        apply(&mut self.b, st);
        while i < n {
            let c = s[i];
            match c {
                '\\' if i + 1 < n && s[i + 1].is_ascii_punctuation() => {
                    self.put(s[i + 1]);
                    i += 2;
                }
                '\n' => {
                    self.b.para();
                    i += 1;
                }
                '`' => {
                    let run = s[i..].iter().take_while(|&&x| x == '`').count();
                    // find closing run of exactly `run`
                    let mut j = i + run;
                    let mut found = None;
                    while j < n {
                        if s[j] == '`' {
                            let r2 = s[j..].iter().take_while(|&&x| x == '`').count();
                            if r2 == run {
                                found = Some(j);
                                break;
                            }
                            j += r2;
                        } else {
                            j += 1;
                        }
                    }
                    match found {
                        Some(j) => {
                            let mut inner: &[char] = &s[i + run..j];
                            if inner.len() >= 2 && inner[0] == ' ' && inner[inner.len() - 1] == ' ' && inner.iter().any(|&x| x != ' ') {
                                inner = &inner[1..inner.len() - 1];
                            }
                            self.mono(true);
                            for &x in inner {
                                self.put(x);
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
                    if let Some((text, end)) = link_at(s, i + 1) {
                        let alt: Vec<char> = text.to_vec();
                        if !alt.is_empty() {
                            self.inline_nested(&alt, base, st);
                        }
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
                                self.indent_prefix();
                                self.b.note(NoteKind::Foot, els);
                                i += close + 1;
                                continue;
                            }
                        }
                    }
                    if let Some((text, end)) = link_at(s, i) {
                        let t = text.to_vec();
                        self.inline_nested(&t, base, st);
                        apply(&mut self.b, st);
                        i = end;
                    } else {
                        self.put('[');
                        i += 1;
                    }
                }
                '<' => {
                    let close = s[i..].iter().take(300).position(|&x| x == '>');
                    let Some(close) = close else {
                        self.put('<');
                        i += 1;
                        continue;
                    };
                    let inner: String = s[i + 1..i + close].iter().collect();
                    let lower = inner.to_ascii_lowercase();
                    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") || (inner.contains('@') && !inner.contains(' ')) {
                        for x in inner.trim_start_matches("mailto:").chars() {
                            self.put(x);
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
                        st.set(a, !closing);
                        apply(&mut self.b, st);
                    } else if name == "br" {
                        self.b.para();
                    }
                    i += close + 1;
                }
                '&' => {
                    let (e, k) = crate::fmt_html::entity_at(s, i);
                    match (e, k) {
                        (Some(e), k) if k > 0 => {
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
                    let closer_later = |len: usize| rest.windows(len.min(rest.len().max(1))).any(|w| w.len() == len && w.iter().all(|&x| x == c));
                    let mut handled = false;
                    if !intraword {
                        let (em, strong) = (st.has(Attr::Italic), st.has(Attr::Bold));
                        if right && (em || strong) {
                            // closing
                            if run >= 3 && em && strong {
                                st.set(Attr::Italic, false);
                                st.set(Attr::Bold, false);
                                handled = true;
                            } else if run >= 2 && strong {
                                st.set(Attr::Bold, false);
                                handled = true;
                                if run >= 3 && em {
                                    st.set(Attr::Italic, false);
                                }
                            } else if em {
                                st.set(Attr::Italic, false);
                                handled = true;
                                if run >= 2 && strong {
                                    st.set(Attr::Bold, false);
                                }
                            }
                        }
                        if !handled && left {
                            let want = run.min(3);
                            if closer_later(want) || (want == 3 && closer_later(1) && closer_later(2)) {
                                if want >= 2 {
                                    st.set(Attr::Bold, true);
                                }
                                if want != 2 {
                                    st.set(Attr::Italic, true);
                                }
                                handled = true;
                            }
                        }
                    }
                    if handled {
                        apply(&mut self.b, st);
                    } else {
                        for _ in 0..run {
                            self.put(c);
                        }
                    }
                    i += run;
                }
                '~' if s.get(i + 1) == Some(&'~') => {
                    let rest = &s[i + 2..];
                    if st.has(Attr::Strikeout) {
                        st.set(Attr::Strikeout, false);
                        apply(&mut self.b, st);
                    } else if rest.windows(2).any(|w| w == ['~', '~']) && rest.first().is_some_and(|x| !x.is_whitespace()) {
                        st.set(Attr::Strikeout, true);
                        apply(&mut self.b, st);
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

/// `[text](url)` / `[text][ref]` / `[text]` starting at `s[i] == '['`:
/// the text and the index after the link.  Only when followed by `(` or
/// `[` (a bare `[x]` stays literal).
fn link_at(s: &[char], i: usize) -> Option<(&[char], usize)> {
    let mut depth = 0;
    let mut j = i;
    let mut close = None;
    while j < s.len() && j - i < 2000 {
        match s[j] {
            '\\' => j += 1,
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
            while k < s.len() {
                match s[k] {
                    '(' => d += 1,
                    ')' => {
                        d -= 1;
                        if d == 0 {
                            return Some((text, k + 1));
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            None
        }
        Some('[') => {
            let k = s[close + 1..].iter().position(|&x| x == ']')? + close + 1;
            Some((text, k + 1))
        }
        _ => None,
    }
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    let text = crate::fmt_text::decode_auto(bytes).replace("\r\n", "\n").replace('\r', "\n");
    let all: Vec<&str> = text.split('\n').collect();
    // footnote definitions `[^label]: text` (+ indented continuation
    // lines) and link reference definitions are taken out first
    let mut defs: HashMap<String, String> = HashMap::new();
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
            if let Some(k) = t.find("]:") {
                let label = t[2..k].to_lowercase();
                let mut body = t[k + 2..].trim().to_string();
                i += 1;
                while i < all.len() && (indent_of(all[i]) >= 2 || (all[i].trim().is_empty() && all.get(i + 1).is_some_and(|n| indent_of(n) >= 4))) {
                    body.push('\n');
                    body.push_str(all[i].trim());
                    i += 1;
                }
                defs.insert(label, body);
                continue;
            }
        }
        if indent_of(l) < 4 && t.starts_with('[') && !t.starts_with("[^") {
            if let Some(k) = t.find("]:") {
                if !t[1..k].contains(']') && t[k + 2..].trim().split_whitespace().count() <= 3 && !t[k + 2..].trim().is_empty() {
                    i += 1;
                    continue;
                }
            }
        }
        lines.push(l.to_string());
        i += 1;
    }
    let mut md = Md { b: Builder::new(base_font()), defs: &defs, depth: 0, quote: 0, list_indent: vec![] };
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

fn escape(s: &str, line_start: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for (k, c) in s.chars().enumerate() {
        match c {
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '~' | '|' => {
                o.push('\\');
                o.push(c);
            }
            '#' | '>' | '+' | '-' | '=' if line_start && k == 0 => {
                o.push('\\');
                o.push(c);
            }
            c => o.push(c),
        }
    }
    o
}

/// One piece of a paragraph: attributes, text, raw (already Markdown).
type Run = (AttrSet, String, bool);

/// Render runs with nested emphasis markers; markers never touch spaces.
fn render(runs: &[Run], mask: AttrSet) -> String {
    let mut out = String::new();
    let mut open: Vec<Attr> = vec![];
    let close_to = |out: &mut String, open: &mut Vec<Attr>, keep: &dyn Fn(Attr) -> bool| {
        if let Some(pos) = open.iter().position(|a| !keep(*a)) {
            let trimmed_len = out.trim_end().len();
            let ws = out[trimmed_len..].to_string();
            out.truncate(trimmed_len);
            while open.len() > pos {
                let a = open.pop().unwrap_or(Attr::Bold);
                if let Some((_, _, c)) = MARK.iter().find(|m| m.0 == a) {
                    out.push_str(c);
                }
            }
            out.push_str(&ws);
        }
    };
    for (a, text, raw) in runs {
        let want = AttrSet(md_attrs(*a).0 & !mask.0);
        close_to(&mut out, &mut open, &|x| want.has(x));
        let t = if *raw { text.clone() } else { escape(text, out.is_empty()) };
        if t.trim().is_empty() {
            out.push_str(&t);
            continue;
        }
        let lead = t.len() - t.trim_start().len();
        out.push_str(&t[..lead]);
        for (at, o, _) in MARK {
            if want.has(at) && !open.contains(&at) {
                out.push_str(o);
                open.push(at);
            }
        }
        out.push_str(&t[lead..]);
    }
    close_to(&mut out, &mut open, &|_| false);
    out
}

struct Writer {
    notes: Vec<String>,
    attrs: AttrSet,
}

impl Writer {
    /// Paragraph of runs → Markdown line (heading detection).
    fn para(&self, runs: &[Run]) -> String {
        let text_runs: Vec<&Run> = runs.iter().filter(|r| !r.2 && !r.1.trim().is_empty()).collect();
        if !text_runs.is_empty() && text_runs.iter().all(|r| r.0.has(Attr::Bold)) {
            let level = if text_runs.iter().all(|r| r.0.has(Attr::ExtraLarge)) {
                Some(1)
            } else if text_runs.iter().all(|r| r.0.has(Attr::VeryLarge)) {
                Some(2)
            } else if text_runs.iter().all(|r| r.0.has(Attr::Large)) {
                Some(3)
            } else {
                None
            };
            if let Some(l) = level {
                let mut mask = AttrSet::default();
                mask.set(Attr::Bold, true);
                let body = render(runs, mask);
                return format!("{} {}", "#".repeat(l), body.trim_start_matches('\\').trim());
            }
        }
        render(runs, AttrSet::default()).trim_end().to_string()
    }

    /// Convert an element list into Markdown blocks.
    fn blocks(&mut self, els: &[El]) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let mut runs: Vec<Run> = vec![];
        let mut i = 0;
        while i < els.len() {
            match &els[i] {
                El::Ch(c) => self.push(&mut runs, &c.to_string()),
                El::Code(c) => match c {
                    Code::AttrOn(a) => self.attrs.set(*a, true),
                    Code::AttrOff(a) => self.attrs.set(*a, false),
                    Code::HRt | Code::HPg => {
                        let p = self.para(&runs);
                        runs.clear();
                        if !p.trim().is_empty() {
                            out.push(p);
                        }
                    }
                    Code::Tab | Code::Indent | Code::DblIndent => {
                        if runs.iter().any(|r| !r.1.trim().is_empty()) {
                            self.push(&mut runs, "\t");
                        }
                    }
                    Code::HardSpace => self.push(&mut runs, "\u{a0}"),
                    Code::HardHyphen => self.push(&mut runs, "-"),
                    Code::Note { els: n, .. } => {
                        let mut w = Writer { notes: vec![], attrs: AttrSet::default() };
                        let body = w.blocks(n).join("\n\n    ");
                        self.notes.push(body);
                        self.notes.extend(w.notes);
                        let num = self.notes.len();
                        runs.push((self.attrs, format!("[^{num}]"), true));
                    }
                    Code::TblDef { .. } => {
                        let p = self.para(&runs);
                        runs.clear();
                        if !p.trim().is_empty() {
                            out.push(p);
                        }
                        let end = els[i..].iter().position(|e| matches!(e, El::Code(Code::TblOff))).map(|k| i + k).unwrap_or(els.len());
                        out.push(self.table(&els[i + 1..end]));
                        i = end + 1;
                        continue;
                    }
                    _ => {}
                },
            }
            i += 1;
        }
        let p = self.para(&runs);
        if !p.trim().is_empty() {
            out.push(p);
        }
        out
    }

    fn push(&self, runs: &mut Vec<Run>, s: &str) {
        match runs.last_mut() {
            Some(r) if r.0 == self.attrs && !r.2 => r.1.push_str(s),
            _ => runs.push((self.attrs, s.to_string(), false)),
        }
    }

    fn table(&mut self, els: &[El]) -> String {
        let mut rows: Vec<Vec<String>> = vec![];
        let mut cell: Vec<El> = vec![];
        let mut have_cell = false;
        let flush = |w: &mut Writer, rows: &mut Vec<Vec<String>>, cell: &mut Vec<El>, have: &mut bool| {
            if *have {
                let saved = w.attrs;
                let blocks = w.blocks(cell);
                w.attrs = saved;
                if rows.is_empty() {
                    rows.push(vec![]);
                }
                if let Some(r) = rows.last_mut() {
                    r.push(blocks.join("<br>").replace('\n', " "));
                }
            }
            cell.clear();
            *have = false;
        };
        for e in els {
            match e {
                El::Code(Code::Row) => {
                    flush(self, &mut rows, &mut cell, &mut have_cell);
                    rows.push(vec![]);
                }
                El::Code(Code::Cell) => {
                    flush(self, &mut rows, &mut cell, &mut have_cell);
                    have_cell = true;
                }
                e => cell.push(e.clone()),
            }
        }
        flush(self, &mut rows, &mut cell, &mut have_cell);
        rows.retain(|r| !r.is_empty());
        let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        let mut s = String::new();
        for (k, r) in rows.iter().enumerate() {
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
                for _ in 0..ncols {
                    s.push_str(" --- |");
                }
                s.push('\n');
            }
        }
        s.trim_end().to_string()
    }
}

/// Write a document as Markdown.
pub fn write(doc: &Document) -> String {
    let mut w = Writer { notes: vec![], attrs: AttrSet::default() };
    let blocks = w.blocks(&doc.els);
    let mut s = blocks.join("\n\n");
    if !w.notes.is_empty() {
        s.push_str("\n\n");
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
        assert_eq!(d.plain_text(0, d.len()), "• eins\n• zwei\n\t1. drei\n\t2. vier\n• ☒ fertig\n\n\tzitat weiter\n\nlet a  = 1;\n\ncode\n\n────────────────────────────────────────\nEnde & € *nicht* Link Bild".replace("• ☒", "☒"));
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
        assert_eq!(md, "## Titel\n\nGrüße **fett *beides*** a\\*b\\_c <sup>2</sup>[^1]\n\n| Z00\\| | Z01\\| |\n| --- | --- |\n| Z10\\| | Z11\\| |\n\nПривет\n\n[^1]: Fußnote\n");
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
    }

    #[test]
    fn never_panics() {
        let samples = ["*", "**", "***a", "[", "[^", "[^1]:", "`", "``a`", "|", "| a |\n|", "| a |\n|-|", "#", "> ", ">>>>>>>>", "- ", "1.", "~~", "<", "<a", "&", "&#", "![", "[a](", "[a][", "    ", "```", "\\", "a  \n", "_a_b_", "***a**b*"];
        for s in samples {
            let _ = read(s.as_bytes());
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
    }
}
