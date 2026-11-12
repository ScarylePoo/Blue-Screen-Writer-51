//! Reveal Codes text for every code — what WP 5.1 shows between brackets.

use crate::doc::*;

pub fn tabs_text(t: &TabSet) -> String {
    let mut s = String::from(if t.relative { "Rel; " } else { "Abs; " });
    if t.stops.is_empty() {
        return format!("{s}No Tabs");
    }
    // evenly spaced left tabs → "Rel; -1", every 0.5""
    if t.stops.len() >= 3 && t.stops.iter().all(|x| x.kind == TabKind::Left && !x.dot_leader) {
        let step = t.stops[1].pos - t.stops[0].pos;
        if step > 0 && t.stops.windows(2).all(|w| w[1].pos - w[0].pos == step) {
            return format!("{s}{}, every {}", fmt_inches(t.stops[0].pos), fmt_inches(step));
        }
    }
    let parts: Vec<String> = t
        .stops
        .iter()
        .take(8)
        .map(|st| {
            let mut p = fmt_inches(st.pos).trim_end_matches('"').to_string();
            match st.kind {
                TabKind::Left => {}
                TabKind::Center => p.push('C'),
                TabKind::Right => p.push('R'),
                TabKind::Decimal => p.push('D'),
            }
            if st.dot_leader {
                p.push('.');
            }
            p
        })
        .collect();
    s.push_str(&parts.join(","));
    if t.stops.len() > 8 {
        s.push_str("...");
    }
    s
}

/// The bracketed Reveal Codes token for a code, e.g. `[BOLD]`.
pub fn code_text(c: &Code) -> String {
    match c {
        Code::HRt => "[HRt]".into(),
        Code::HPg => "[HPg]".into(),
        Code::Tab => "[Tab]".into(),
        // (the Reveal Codes window shows [Cntr Tab]/[Rgt Tab]/[Dec Tab]
        // according to the stop the tab lands on — see tab_text)
        Code::Indent => "[\u{2192}Indent]".into(),
        Code::DblIndent => "[\u{2192}Indent\u{2190}]".into(),
        Code::MarRel => "[\u{2190}Mar Rel]".into(),
        Code::Center => "[Center]".into(),
        Code::FlushRight => "[Flsh Rgt]".into(),
        Code::HardSpace => "[ ]".into(),
        Code::HardHyphen => "-".into(),
        Code::SoftHyphen => "-".into(),
        Code::AttrOn(a) => format!("[{}]", a.mnemonic()),
        Code::AttrOff(a) => format!("[{}]", a.mnemonic().to_lowercase()),
        Code::Font(f) => {
            let pts = f.size_cpt as f64 / 100.0;
            let pts = if (pts - pts.round()).abs() < 0.001 { format!("{}", pts as i64) } else { format!("{pts}") };
            format!("[Font:{} {}pt]", f.name, pts)
        }
        Code::LRMargin { left, right } => format!("[L/R Mar:{},{}]", fmt_inches(*left), fmt_inches(*right)),
        Code::TBMargin { top, bottom } => format!("[T/B Mar:{},{}]", fmt_inches(*top), fmt_inches(*bottom)),
        Code::LineSpacing(s) => {
            let v = *s as f64 / 100.0;
            if s % 100 == 0 { format!("[Ln Spacing:{}]", s / 100) } else { format!("[Ln Spacing:{v}]") }
        }
        Code::Just(j) => format!("[Just:{}]", j.name()),
        Code::TabSet(t) => format!("[Tab Set:{}]", tabs_text(t)),
        Code::CenterPage => "[Center Pg]".into(),
        Code::Comment(_) => "[Comment]".into(),
        Code::DateCode(f) => format!("[Date:{f}]"),
        Code::Hyphenation(on) => if *on { "[Hyph On]".into() } else { "[Hyph Off]".into() },
        Code::WidowOrphan(on) => if *on { "[W/O On]".into() } else { "[W/O Off]".into() },
        Code::PageNumber(n) => format!("[Pg Num:{n}]"),
        Code::PageNumberPos(p) => format!("[Pg Numbering:{}]", page_num_pos_name(*p)),
        Code::Suppress(b) => crate::fmt::suppress_text(*b),
        Code::Note { kind, els } => {
            let label = if *kind == NoteKind::Foot { "Footnote" } else { "Endnote" };
            format!("[{label}:#;[Note Num]{}]", preview(els))
        }
        Code::HeadFoot { which, occ, els } => {
            let name = ["Header A", "Header B", "Footer A", "Footer B"][*which as usize & 3];
            if *occ == Occurrence::Discontinue {
                format!("[{name}:Discontinue]")
            } else {
                format!("[{name}:{};{}]", occ.name(), preview(els))
            }
        }
        Code::TblDef { cols } => {
            let ws: Vec<String> = cols.iter().map(|w| fmt_inches(*w)).collect();
            format!("[Tbl Def:I;{},{}]", cols.len(), ws.join(","))
        }
        Code::Row => "[Row]".into(),
        Code::Cell => "[Cell]".into(),
        Code::TblOff => "[Tbl Off]".into(),
        Code::ColDef { kind, cols } => crate::columns::reveal_text(*kind, cols),
        Code::ColOn => "[Col On]".into(),
        Code::ColOff => "[Col Off]".into(),
        Code::RevOn { del, author, date } => {
            let d = crate::track::local_date(date);
            if *del { format!("[DEL:{author} {d}]") } else { format!("[INS:{author} {d}]") }
        }
        Code::RevOff { del } => if *del { "[del]".into() } else { "[ins]".into() },
        Code::Merge(n) => format!("[Mrg:{n}]"),
        Code::MathDef(_) => "[Math Def]".into(),
        Code::Math(on) => if *on { "[Math On]".into() } else { "[Math Off]".into() },
        Code::MathOp(c) => c.to_string(),
        Code::Formula(f) => format!("[Formula:{f}]"),
        Code::SsLink { file, range, .. } => {
            let name = std::path::Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.clone());
            if range.is_empty() { format!("[Link:{name}]") } else { format!("[Link:{name};{range}]") }
        }
        Code::SsLinkEnd => "[Link End]".into(),
        Code::Link(url) => format!("[Link:{url}]"),
        Code::LinkEnd => "[Link End]".into(),
        Code::Bookmark(name) => format!("[Bookmark:{name}]"),
        Code::StyleOn(n) => format!("[Style On:{n}]"),
        Code::StyleOff(n) => format!("[Style Off:{n}]"),
        Code::OpenStyle(n) => format!("[Open Style:{n}]"),
        Code::ParNum(None) => "[Par Num:Auto]".into(),
        Code::ParNum(Some(l)) => format!("[Par Num:{l}]"),
        Code::ParNumDef(_) => "[Par Num Def]".into(),
        Code::Outline(on) => if *on { "[Outline On]".into() } else { "[Outline Off]".into() },
        Code::Fmt(f) => f.reveal(),
        Code::GBox(_) | Code::BoxOpt(_) | Code::BoxNewNum(..) | Code::BoxNum | Code::GLine(_) => crate::gfx::reveal(c).unwrap_or_default(),
        Code::Unknown { name, .. } => format!("[{name}]"),
        Code::Mark(m) => mark_text(m),
        Code::FlushRightDot => "[Dot Flsh Rgt]".into(),
    }
}

/// Reveal Codes text of a Mark Text code.
pub fn mark_text(m: &Mark) -> String {
    let lk = |list: bool| if list { "List" } else { "ToC" };
    match m {
        Mark::Begin { list, n } => format!("[Mark:{},{n}]", lk(*list)),
        Mark::End { list, n } => format!("[End Mark:{},{n}]", lk(*list)),
        Mark::Index { heading, sub } => {
            if sub.is_empty() { format!("[Index:{heading}]") } else { format!("[Index:{heading};{sub}]") }
        }
        Mark::ToA { section, short, full } => {
            if *section == 0 { format!("[ToA:;{short};]") } else { format!("[ToA:{section};{short};{}]", preview_str(full)) }
        }
        Mark::DefToc { levels, styles, .. } => {
            let s: Vec<String> = styles.iter().take(*levels as usize).map(|x| (x + 1).to_string()).collect();
            format!("[Def Mark:ToC,{levels}:{}]", s.join(","))
        }
        Mark::DefList { n, style } => format!("[Def Mark:List,{n}:{}]", style + 1),
        Mark::DefIndex { style, concordance } => {
            if concordance.is_empty() { format!("[Def Mark:Index,{}]", style + 1) } else { format!("[Def Mark:Index,{};{concordance}]", style + 1) }
        }
        Mark::DefToA { section, .. } => format!("[Def Mark:ToA,{section}]"),
        Mark::EndDef => "[End Def]".into(),
        Mark::Ref { name, kind, text } => format!("[Ref({name}):{} {}]", Mark::ref_kind_name(*kind), if text.is_empty() { "?" } else { text }),
        Mark::Target { name } => format!("[Target({name})]"),
        Mark::Subdoc(f) => format!("[Subdoc:{f}]"),
        Mark::SubdocStart(f) => format!("[Subdoc Start:{f}]"),
        Mark::SubdocEnd(f) => format!("[Subdoc End:{f}]"),
    }
}

fn preview_str(s: &str) -> String {
    let one: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if one.chars().count() > 30 { one.chars().take(30).collect::<String>() + "…" } else { one }
}

pub fn page_num_pos_name(p: u8) -> &'static str {
    match p {
        1 => "Top Left",
        2 => "Top Center",
        3 => "Top Right",
        4 => "Alternating Top",
        5 => "Bottom Left",
        6 => "Bottom Center",
        7 => "Bottom Right",
        8 => "Alternating Bottom",
        _ => "None",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(code_text(&Code::AttrOn(Attr::Bold)), "[BOLD]");
        assert_eq!(code_text(&Code::AttrOff(Attr::Bold)), "[bold]");
        assert_eq!(code_text(&Code::LRMargin { left: 1200, right: 1800 }), "[L/R Mar:1\",1.5\"]");
        assert_eq!(code_text(&Code::Font(FontSpec { name: "Times New Roman".into(), size_cpt: 1200 })), "[Font:Times New Roman 12pt]");
    }
}

/// Abbreviated text of nested elements, as Reveal Codes shows it.
pub fn preview(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(Code::HRt) => s.push_str("[HRt]"),
            El::Code(Code::Tab) => s.push_str("[Tab]"),
            _ => {}
        }
        if s.chars().count() > 50 {
            let t: String = s.chars().take(50).collect();
            return t + "…";
        }
    }
    s
}

/// `[Tab]`, `[Cntr Tab]`, `[Rgt Tab]`, `[Dec Tab]`.
pub fn tab_text(k: TabKind) -> &'static str {
    match k {
        TabKind::Left => "[Tab]",
        TabKind::Center => "[Cntr Tab]",
        TabKind::Right => "[Rgt Tab]",
        TabKind::Decimal => "[Dec Tab]",
    }
}
