//! Lossless text serialisation of a document's code stream.
//!
//! Stored inside our .docx files (as a private part) so that reopening a
//! file we saved gives back *exactly* the same codes, while Word and other
//! programs read the normal document.xml.  Also used for backups.

use crate::doc::*;

pub(crate) fn esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '|' => o.push_str("\\p"),
            c => o.push(c),
        }
    }
    o
}

pub(crate) fn unesc(s: &str) -> String {
    let mut o = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => o.push('\n'),
                Some('p') => o.push('|'),
                Some(x) => o.push(x),
                None => {}
            }
        } else {
            o.push(c);
        }
    }
    o
}

fn attr_idx(a: Attr) -> usize {
    a as usize
}

fn kind_code(k: TabKind) -> char {
    match k {
        TabKind::Left => 'L',
        TabKind::Center => 'C',
        TabKind::Right => 'R',
        TabKind::Decimal => 'D',
    }
}

fn just_code(j: Justify) -> &'static str {
    match j {
        Justify::Left => "L",
        Justify::Center => "C",
        Justify::Right => "R",
        Justify::Full => "F",
    }
}

fn parse_just(s: &str) -> Justify {
    match s {
        "L" => Justify::Left,
        "C" => Justify::Center,
        "R" => Justify::Right,
        _ => Justify::Full,
    }
}

fn tabs_str(t: &TabSet) -> String {
    let mut s = String::from(if t.relative { "r" } else { "a" });
    for st in &t.stops {
        s.push_str(&format!(",{}{}{}", st.pos, kind_code(st.kind), if st.dot_leader { "." } else { "" }));
    }
    s
}

fn parse_tabs(s: &str) -> TabSet {
    let mut parts = s.split(',');
    let relative = parts.next() == Some("r");
    let mut stops = Vec::new();
    for p in parts {
        let dot = p.ends_with('.');
        let p = p.trim_end_matches('.');
        if p.is_empty() {
            continue;
        }
        let (num, k) = p.split_at(p.len() - 1);
        let kind = match k {
            "C" => TabKind::Center,
            "R" => TabKind::Right,
            "D" => TabKind::Decimal,
            _ => TabKind::Left,
        };
        if let Ok(pos) = num.parse() {
            stops.push(TabStop { pos, kind, dot_leader: dot });
        }
    }
    TabSet { stops, relative }
}

pub fn code_to_str(c: &Code) -> String {
    match c {
        Code::HRt => "HRt".into(),
        Code::HPg => "HPg".into(),
        Code::Tab => "Tab".into(),
        Code::Indent => "Ind".into(),
        Code::DblIndent => "DInd".into(),
        Code::MarRel => "MRel".into(),
        Code::Center => "Ctr".into(),
        Code::FlushRight => "FlR".into(),
        Code::HardSpace => "HSp".into(),
        Code::HardHyphen => "HHy".into(),
        Code::SoftHyphen => "SHy".into(),
        Code::AttrOn(a) => format!("A+{}", attr_idx(*a)),
        Code::AttrOff(a) => format!("A-{}", attr_idx(*a)),
        Code::Font(f) => format!("Font|{}|{}", esc(&f.name), f.size_cpt),
        Code::LRMargin { left, right } => format!("LR|{left}|{right}"),
        Code::TBMargin { top, bottom } => format!("TB|{top}|{bottom}"),
        Code::LineSpacing(s) => format!("LS|{s}"),
        Code::Just(j) => format!("J|{}", just_code(*j)),
        Code::TabSet(t) => format!("TS|{}", tabs_str(t)),
        Code::CenterPage => "CPg".into(),
        Code::Comment(s) => format!("Cmt|{}", esc(s)),
        Code::DateCode(s) => format!("Date|{}", esc(s)),
        Code::Hyphenation(b) => format!("Hyph|{}", *b as u8),
        Code::WidowOrphan(b) => format!("WO|{}", *b as u8),
        Code::PageNumber(n) => format!("PgN|{n}"),
        Code::PageNumberPos(p) => format!("PgP|{p}"),
        Code::Suppress(p) => format!("Sup|{p}"),
        Code::Note { kind, els } => {
            format!("Note|{}|{}", if *kind == NoteKind::Foot { "F" } else { "E" }, esc(&els_to_string(els)))
        }
        Code::HeadFoot { which, occ, els } => {
            let o = match occ {
                Occurrence::Discontinue => 0,
                Occurrence::Every => 1,
                Occurrence::Odd => 2,
                Occurrence::Even => 3,
            };
            format!("HF|{which}|{o}|{}", esc(&els_to_string(els)))
        }
        Code::TblDef { cols } => format!("Tbl|{}", cols.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(",")),
        Code::Row => "Row".into(),
        Code::RevOn { del, author, date } => format!("Rev|{}|{}|{}", *del as u8, esc(author), esc(date)),
        Code::RevOff { del } => format!("RevOff|{}", *del as u8),
        Code::Cell => "Cell".into(),
        Code::TblOff => "TblOff".into(),
        Code::ColDef { kind, cols } => {
            let k = match kind {
                ColKind::Newspaper => "N",
                ColKind::Parallel => "P",
                ColKind::ParallelProtect => "B",
            };
            format!("ColDef|{k}|{}", cols.iter().map(|(l, r)| format!("{l},{r}")).collect::<Vec<_>>().join(";"))
        }
        Code::ColOn => "ColOn".into(),
        Code::ColOff => "ColOff".into(),
        Code::Merge(n) => format!("Mrg|{}", esc(n)),
        Code::MathDef(cols) => {
            let v: Vec<String> = cols.iter().map(|c| format!("{}:{}:{}:{}", c.kind, c.paren as u8, c.digits, c.formula.replace('\\', "").replace(';', ""))).collect();
            format!("MDef|{}", esc(&v.join(";")))
        }
        Code::Math(on) => format!("Math|{}", *on as u8),
        Code::MathOp(c) => format!("MOp|{}", esc(&c.to_string())),
        Code::Formula(f) => format!("Fml|{}", esc(f)),
        Code::SsLink { file, range, table } => format!("Link|{}|{}|{}", esc(file), esc(range), *table as u8),
        Code::SsLinkEnd => "LinkEnd".into(),
        Code::Link(u) => format!("Lnk|{}", esc(u)),
        Code::LinkEnd => "LnkE".into(),
        Code::Bookmark(n) => format!("Bmk|{}", esc(n)),
        Code::StyleOn(n) => format!("StOn|{}", esc(n)),
        Code::StyleOff(n) => format!("StOff|{}", esc(n)),
        Code::OpenStyle(n) => format!("StOpen|{}", esc(n)),
        Code::ParNum(l) => format!("PN|{}", l.unwrap_or(0)),
        Code::ParNumDef(d) => format!("PND|{}", esc(&crate::styles::def_to_str(d))),
        Code::Outline(on) => format!("Outl|{}", *on as u8),
        Code::Fmt(f) => format!("F|{}", f.to_stream()),
        Code::GBox(_) | Code::BoxOpt(_) | Code::BoxNewNum(..) | Code::BoxNum | Code::GLine(_) => crate::gfx::to_stream(c).unwrap_or_default(),
        Code::Unknown { name, data } => {
            let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
            format!("Unk|{}|{}", esc(name), hex)
        }
        Code::FlushRightDot => "FlRD".into(),
        Code::Mark(m) => mark_to_str(m),
    }
}

fn mark_to_str(m: &Mark) -> String {
    let b = |x: bool| x as u8;
    match m {
        Mark::Begin { list, n } => format!("MkB|{}|{n}", b(*list)),
        Mark::End { list, n } => format!("MkE|{}|{n}", b(*list)),
        Mark::Index { heading, sub } => format!("MkIx|{}|{}", esc(heading), esc(sub)),
        Mark::ToA { section, short, full } => format!("MkToA|{section}|{}|{}", esc(short), esc(full)),
        Mark::DefToc { levels, wrap_last, styles } => {
            format!("MkDT|{levels}|{}|{}", b(*wrap_last), styles.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))
        }
        Mark::DefList { n, style } => format!("MkDL|{n}|{style}"),
        Mark::DefIndex { style, concordance } => format!("MkDI|{style}|{}", esc(concordance)),
        Mark::DefToA { section, dots, underline, blank } => format!("MkDA|{section}|{}|{}|{}", b(*dots), b(*underline), b(*blank)),
        Mark::EndDef => "MkED".into(),
        Mark::Ref { name, kind, text } => format!("MkRef|{kind}|{}|{}", esc(name), esc(text)),
        Mark::Target { name } => format!("MkTgt|{}", esc(name)),
        Mark::Subdoc(f) => format!("MkSD|{}", esc(f)),
        Mark::SubdocStart(f) => format!("MkSDS|{}", esc(f)),
        Mark::SubdocEnd(f) => format!("MkSDE|{}", esc(f)),
    }
}


pub fn str_to_code(s: &str) -> Option<Code> {
    if let Some(rest) = s.strip_prefix("F|") {
        return crate::fmt::Fmt::from_stream(rest).map(Code::Fmt);
    }
    let parts: Vec<&str> = s.split('|').collect();
    let p = |i: usize| parts.get(i).copied().unwrap_or("");
    let num = |i: usize| p(i).parse::<i32>().unwrap_or(0);
    Some(match p(0) {
        "HRt" => Code::HRt,
        "HPg" => Code::HPg,
        "Tab" => Code::Tab,
        "Ind" => Code::Indent,
        "DInd" => Code::DblIndent,
        "MRel" => Code::MarRel,
        "Ctr" => Code::Center,
        "FlR" => Code::FlushRight,
        "HSp" => Code::HardSpace,
        "HHy" => Code::HardHyphen,
        "SHy" => Code::SoftHyphen,
        "Font" => Code::Font(FontSpec { name: unesc(p(1)), size_cpt: p(2).parse().unwrap_or(1200) }),
        "LR" => Code::LRMargin { left: num(1), right: num(2) },
        "TB" => Code::TBMargin { top: num(1), bottom: num(2) },
        "LS" => Code::LineSpacing(p(1).parse().unwrap_or(100)),
        "J" => Code::Just(parse_just(p(1))),
        "TS" => Code::TabSet(parse_tabs(p(1))),
        "CPg" => Code::CenterPage,
        "Cmt" => Code::Comment(unesc(p(1))),
        "Date" => Code::DateCode(unesc(p(1))),
        "Hyph" => Code::Hyphenation(p(1) == "1"),
        "WO" => Code::WidowOrphan(p(1) == "1"),
        "PgN" => Code::PageNumber(p(1).parse().unwrap_or(1)),
        "PgP" => Code::PageNumberPos(p(1).parse().unwrap_or(0)),
        "Sup" => Code::Suppress(p(1).parse().unwrap_or(0)),
        "Note" => Code::Note {
            kind: if p(1) == "E" { NoteKind::End } else { NoteKind::Foot },
            els: string_to_els(&unesc(p(2))),
        },
        "HF" => Code::HeadFoot {
            which: p(1).parse().unwrap_or(0),
            occ: match p(2) {
                "0" => Occurrence::Discontinue,
                "2" => Occurrence::Odd,
                "3" => Occurrence::Even,
                _ => Occurrence::Every,
            },
            els: string_to_els(&unesc(p(3))),
        },
        "Tbl" => Code::TblDef { cols: p(1).split(',').filter_map(|x| x.parse().ok()).collect() },
        "Row" => Code::Row,
        "Rev" => Code::RevOn { del: p(1) == "1", author: unesc(p(2)), date: unesc(p(3)) },
        "RevOff" => Code::RevOff { del: p(1) == "1" },
        "Cell" => Code::Cell,
        "TblOff" => Code::TblOff,
        "FlRD" => Code::FlushRightDot,
        "MkB" | "MkE" => {
            let (list, n) = (p(1) == "1", p(2).parse().unwrap_or(1));
            Code::Mark(if p(0) == "MkB" { Mark::Begin { list, n } } else { Mark::End { list, n } })
        }
        "MkIx" => Code::Mark(Mark::Index { heading: unesc(p(1)), sub: unesc(p(2)) }),
        "MkToA" => Code::Mark(Mark::ToA { section: p(1).parse().unwrap_or(0), short: unesc(p(2)), full: unesc(p(3)) }),
        "MkDT" => {
            let mut styles = [4u8; 5];
            for (k, v) in p(3).split(',').enumerate().take(5) {
                styles[k] = v.parse().unwrap_or(4);
            }
            Code::Mark(Mark::DefToc { levels: p(1).parse().unwrap_or(1), wrap_last: p(2) == "1", styles })
        }
        "MkDL" => Code::Mark(Mark::DefList { n: p(1).parse().unwrap_or(1), style: p(2).parse().unwrap_or(4) }),
        "MkDI" => Code::Mark(Mark::DefIndex { style: p(1).parse().unwrap_or(1), concordance: unesc(p(2)) }),
        "MkDA" => Code::Mark(Mark::DefToA { section: p(1).parse().unwrap_or(1), dots: p(2) == "1", underline: p(3) == "1", blank: p(4) == "1" }),
        "MkED" => Code::Mark(Mark::EndDef),
        "MkRef" => Code::Mark(Mark::Ref { kind: p(1).parse().unwrap_or(0), name: unesc(p(2)), text: unesc(p(3)) }),
        "MkTgt" => Code::Mark(Mark::Target { name: unesc(p(1)) }),
        "MkSD" => Code::Mark(Mark::Subdoc(unesc(p(1)))),
        "MkSDS" => Code::Mark(Mark::SubdocStart(unesc(p(1)))),
        "MkSDE" => Code::Mark(Mark::SubdocEnd(unesc(p(1)))),
        "ColDef" => Code::ColDef {
            kind: match p(1) {
                "P" => ColKind::Parallel,
                "B" => ColKind::ParallelProtect,
                _ => ColKind::Newspaper,
            },
            cols: p(2)
                .split(';')
                .filter_map(|x| {
                    let (a, b) = x.split_once(',')?;
                    Some((a.parse().ok()?, b.parse().ok()?))
                })
                .collect(),
        },
        "ColOn" => Code::ColOn,
        "ColOff" => Code::ColOff,
        "Mrg" => Code::Merge(unesc(p(1))),
        "MDef" => Code::MathDef(
            unesc(p(1))
                .split(';')
                .filter(|x| !x.is_empty())
                .map(|c| {
                    let f: Vec<&str> = c.splitn(4, ':').collect();
                    MathCol {
                        kind: f.first().and_then(|x| x.parse().ok()).unwrap_or(2),
                        paren: f.get(1) == Some(&"1"),
                        digits: f.get(2).and_then(|x| x.parse().ok()).unwrap_or(2),
                        formula: f.get(3).unwrap_or(&"").to_string(),
                    }
                })
                .collect(),
        ),
        "Math" => Code::Math(p(1) == "1"),
        "MOp" => Code::MathOp(unesc(p(1)).chars().next().unwrap_or('+')),
        "Fml" => Code::Formula(unesc(p(1))),
        "Link" => Code::SsLink { file: unesc(p(1)), range: unesc(p(2)), table: p(3) == "1" },
        "LinkEnd" => Code::SsLinkEnd,
        "Lnk" => Code::Link(unesc(p(1))),
        "LnkE" => Code::LinkEnd,
        "Bmk" => Code::Bookmark(unesc(p(1))),
        "StOn" => Code::StyleOn(unesc(p(1))),
        "StOff" => Code::StyleOff(unesc(p(1))),
        "StOpen" => Code::OpenStyle(unesc(p(1))),
        "PN" => Code::ParNum(p(1).parse::<u8>().ok().filter(|l| *l > 0)),
        "PND" => Code::ParNumDef(crate::styles::str_to_def(&unesc(p(1)))),
        "Outl" => Code::Outline(p(1) == "1"),
        "Unk" => {
            let hex = p(2);
            let data = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect();
            Code::Unknown { name: unesc(p(1)), data }
        }
        "GBox" | "BOpt" | "BNum" | "BoxNum" | "GLine" => crate::gfx::from_stream(&parts)?,
        x if x.starts_with("A+") || x.starts_with("A-") => {
            let i: usize = x[2..].parse().ok()?;
            let a = *Attr::ALL.get(i)?;
            if x.starts_with("A+") { Code::AttrOn(a) } else { Code::AttrOff(a) }
        }
        _ => return None,
    })
}

/// A style definition as one `#style|…` header line (without the prefix).
pub fn style_to_str(s: &StyleDef) -> String {
    let kind = match s.kind {
        StyleKind::Paired => "P",
        StyleKind::Open => "O",
        StyleKind::Outline => "L",
    };
    let enter = match s.enter {
        EnterAction::HRt => "H",
        EnterAction::Off => "F",
        EnterAction::OffOn => "B",
    };
    format!("{}|{kind}|{enter}|{}|{}|{}", esc(&s.name), esc(&s.description), esc(&els_to_string(&s.on)), esc(&els_to_string(&s.off)))
}

pub fn str_to_style(line: &str) -> Option<StyleDef> {
    let p: Vec<&str> = line.split('|').collect();
    if p.len() < 6 {
        return None;
    }
    Some(StyleDef {
        name: unesc(p[0]),
        kind: match p[1] {
            "O" => StyleKind::Open,
            "L" => StyleKind::Outline,
            _ => StyleKind::Paired,
        },
        enter: match p[2] {
            "F" => EnterAction::Off,
            "B" => EnterAction::OffOn,
            _ => EnterAction::HRt,
        },
        description: unesc(p[3]),
        on: string_to_els(&unesc(p[4])),
        off: string_to_els(&unesc(p[5])),
    })
}

/// Nested element lists (note/header text) as one escaped string:
/// the same `t…`/`k…` records joined with newlines.
pub(crate) fn els_to_string(els: &[El]) -> String {
    let mut d = Document::new();
    d.els = els.to_vec();
    let s = write(&d);
    s.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n")
}

pub(crate) fn string_to_els(s: &str) -> Vec<El> {
    let full = format!("#WPSTREAM 1\n{s}");
    read(&full).map(|d| d.els).unwrap_or_default()
}

/// Serialise: one record per line. `t<text>` for runs of characters,
/// `k<code>` for codes. Header lines start with `#`.
pub fn write(doc: &Document) -> String {
    let mut out = String::from("#WPSTREAM 1\n");
    let i = &doc.init;
    out.push_str(&format!(
        "#init|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
        i.page_width,
        i.page_height,
        i.left,
        i.right,
        i.top,
        i.bottom,
        esc(&i.font.name),
        i.font.size_cpt,
        tabs_str(&i.tabs),
        just_code(i.justify),
        i.line_spacing
    ));
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            out.push_str(&format!("#ic{}\n", code_to_str(c)));
        }
    }
    if !doc.id.is_empty() {
        out.push_str(&format!("#id|{}\n", doc.id));
    }
    if doc.track {
        out.push_str("#track\n");
    }
    for st in &doc.styles {
        out.push_str(&format!("#style|{}\n", style_to_str(st)));
    }
    if !doc.opts.is_default() {
        out.push_str(&doc.opts.to_line());
        out.push('\n');
    }
    let s = &doc.summary;
    out.push_str(&format!(
        "#sum|{}|{}|{}|{}|{}\n",
        esc(&s.descriptive_name),
        esc(&s.subject),
        esc(&s.author),
        esc(&s.typist),
        esc(&s.comments)
    ));
    let mut run = String::new();
    for e in &doc.els {
        match e {
            El::Ch(c) => run.push(*c),
            El::Code(c) => {
                if !run.is_empty() {
                    out.push('t');
                    out.push_str(&esc(&run));
                    out.push('\n');
                    run.clear();
                }
                out.push('k');
                out.push_str(&code_to_str(c));
                out.push('\n');
            }
        }
    }
    if !run.is_empty() {
        out.push('t');
        out.push_str(&esc(&run));
        out.push('\n');
    }
    out
}

pub fn read(s: &str) -> Option<Document> {
    let mut lines = s.lines();
    if lines.next()? != "#WPSTREAM 1" {
        return None;
    }
    let mut d = Document::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("#init|") {
            let p: Vec<&str> = rest.split('|').collect();
            if p.len() >= 11 {
                let n = |i: usize| p[i].parse::<i32>().unwrap_or(0);
                d.init = InitialFormat {
                    page_width: n(0),
                    page_height: n(1),
                    left: n(2),
                    right: n(3),
                    top: n(4),
                    bottom: n(5),
                    font: FontSpec { name: unesc(p[6]), size_cpt: p[7].parse().unwrap_or(1200) },
                    tabs: parse_tabs(p[8]),
                    justify: parse_just(p[9]),
                    line_spacing: p[10].parse().unwrap_or(100),
                };
            }
        } else if let Some(rest) = line.strip_prefix("#style|") {
            if let Some(st) = str_to_style(rest) {
                d.styles.push(st);
            }
        } else if let Some(rest) = line.strip_prefix("#opts|") {
            d.opts = crate::fmt::DocOpts::from_line(rest);
        } else if line == "#track" {
            d.track = true;
        } else if let Some(rest) = line.strip_prefix("#id|") {
            d.id = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("#ic") {
            if let Some(c) = str_to_code(rest) {
                d.initial_codes.push(El::Code(c));
            }
        } else if let Some(rest) = line.strip_prefix("#sum|") {
            let p: Vec<String> = rest.split('|').map(unesc).collect();
            let g = |i: usize| p.get(i).cloned().unwrap_or_default();
            d.summary = Summary { descriptive_name: g(0), subject: g(1), author: g(2), typist: g(3), comments: g(4) };
        } else if let Some(t) = line.strip_prefix('t') {
            d.els.extend(unesc(t).chars().map(El::Ch));
        } else if let Some(k) = line.strip_prefix('k') {
            if let Some(c) = str_to_code(k) {
                d.els.push(El::Code(c));
            }
        }
    }
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let mut d = Document::from_text("Hallo Welt | \\ ok\nzweite Zeile\tTab");
        d.els.insert(0, El::Code(Code::AttrOn(Attr::Bold)));
        d.els.insert(3, El::Code(Code::AttrOff(Attr::Bold)));
        d.els.push(El::Code(Code::Font(FontSpec { name: "Times|New".into(), size_cpt: 1150 })));
        d.els.push(El::Code(Code::TabSet(TabSet::default_tabs())));
        d.els.push(El::Code(Code::Comment("a\nb|c".into())));
        let s = write(&d);
        let d2 = read(&s).unwrap();
        assert_eq!(d.els, d2.els);
        assert_eq!(d.init, d2.init);
    }
}
