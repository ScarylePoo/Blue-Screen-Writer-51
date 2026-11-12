//! Ctrl-F5, 5 Spreadsheet: Import / Create Link / Edit Link / Link Options.
//! Reads CSV/TSV and Excel .xlsx (first sheet, a named sheet, a cell
//! range `A1:D10`, `Sheet2!B2:C9`, or a defined name) into a table or
//! tabbed text.  Links are `[Link:file;range]…[Link End]` around the data.

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{HitAction, Outcome, Screen, Wp};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

// ================================================================ ranges

/// "B12" → (col 1, row 11); `$` allowed.
fn cell_ref(s: &str) -> Option<(usize, usize)> {
    let s = s.replace('$', "");
    let (r, c) = crate::math::parse_ref(&s)?;
    Some((c, r))
}

/// "A1:C5" → ((c0, r0), (c1, r1)); a single cell is a 1×1 range.
fn parse_range(s: &str) -> Option<((usize, usize), (usize, usize))> {
    let s = s.trim();
    let (a, b) = s.split_once(':').or_else(|| s.split_once("..")).unwrap_or((s, s));
    let a = cell_ref(a)?;
    let b = cell_ref(b)?;
    Some(((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1))))
}

fn apply_range(grid: Vec<Vec<String>>, r: Option<((usize, usize), (usize, usize))>) -> Vec<Vec<String>> {
    let Some(((c0, r0), (c1, r1))) = r else { return trim_grid(grid) };
    (r0..=r1).map(|r| (c0..=c1).map(|c| grid.get(r).and_then(|row| row.get(c)).cloned().unwrap_or_default()).collect()).collect()
}

/// Drop empty trailing rows/columns and pad rows to equal length.
fn trim_grid(mut g: Vec<Vec<String>>) -> Vec<Vec<String>> {
    while g.last().is_some_and(|r| r.iter().all(|c| c.trim().is_empty())) {
        g.pop();
    }
    let w = g.iter().map(|r| r.iter().rposition(|c| !c.trim().is_empty()).map(|k| k + 1).unwrap_or(0)).max().unwrap_or(0);
    for r in &mut g {
        r.resize(w, String::new());
    }
    g
}

/// Read a spreadsheet range.  `range` may be empty (whole first sheet), a
/// sheet name, `A1:C5`, `Sheet!A1:C5` or a defined name.
pub fn read_range(p: &Path, range: &str) -> Result<Vec<Vec<String>>, String> {
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if ext == "xlsx" || ext == "xlsm" {
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        return read_xlsx(&bytes, range);
    }
    let b = std::fs::read(p).map_err(|e| e.to_string())?;
    let rows = crate::merge::parse_csv(&crate::fileio::decode_text(&b));
    let r = if range.trim().is_empty() { None } else { Some(parse_range(range).ok_or_else(|| format!("invalid range {range}"))?) };
    Ok(apply_range(rows, r))
}

// ================================================================ xlsx

fn zip_text(z: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        let k = a.key.as_ref();
        let local = k.iter().position(|&c| c == b':').map(|p| &k[p + 1..]).unwrap_or(k);
        if local == key || k == key {
            return a.unescape_value().ok().map(|v| v.to_string());
        }
    }
    None
}

fn local_name(n: &[u8]) -> &[u8] {
    n.iter().position(|&c| c == b':').map(|p| &n[p + 1..]).unwrap_or(n)
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut r = Reader::from_str(xml);
    let mut out = vec![];
    let mut cur = String::new();
    let mut in_t = false;
    let mut in_rph = false;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                b"si" => cur.clear(),
                b"t" => in_t = true,
                b"rPh" => in_rph = true,
                _ => {}
            },
            Ok(Event::Empty(e)) if local_name(e.name().as_ref()) == b"si" => out.push(String::new()),
            Ok(Event::Text(t)) if in_t && !in_rph => {
                if let Ok(s) = t.unescape() {
                    cur.push_str(&s);
                }
            }
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                b"si" => out.push(std::mem::take(&mut cur)),
                b"t" => in_t = false,
                b"rPh" => in_rph = false,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Which cell styles (index into cellXfs) are dates.
fn date_styles(xml: &str) -> Vec<bool> {
    let mut r = Reader::from_str(xml);
    let mut custom: HashMap<u32, bool> = HashMap::new();
    let mut out = vec![];
    let mut in_xfs = false;
    loop {
        let ev = r.read_event();
        let (e, start) = match &ev {
            Ok(Event::Start(e)) => (e.clone(), true),
            Ok(Event::Empty(e)) => (e.clone(), false),
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == b"cellXfs" {
                    in_xfs = false;
                }
                continue;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => continue,
        };
        match local_name(e.name().as_ref()) {
            b"numFmt" => {
                let id: u32 = attr(&e, b"numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0);
                let code = attr(&e, b"formatCode").unwrap_or_default().to_lowercase();
                // strip quoted text / [colour] parts before looking for d, m, y
                let mut plain = String::new();
                let mut q = false;
                let mut br = false;
                for c in code.chars() {
                    match c {
                        '"' => q = !q,
                        '[' => br = true,
                        ']' => br = false,
                        c if !q && !br => plain.push(c),
                        _ => {}
                    }
                }
                custom.insert(id, plain.contains('d') || plain.contains('y') || (plain.contains('m') && !plain.contains('h') && !plain.contains('0')));
            }
            b"cellXfs" => in_xfs = start,
            b"xf" if in_xfs => {
                let id: u32 = attr(&e, b"numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0);
                out.push((14..=22).contains(&id) || (45..=47).contains(&id) || custom.get(&id).copied().unwrap_or(false));
            }
            _ => {}
        }
    }
    out
}

/// Excel serial date → `dd.mm.yyyy` (German).
fn excel_date(v: f64) -> String {
    let days = v.floor() as i64 - 25569; // since 1970-01-01
    // civil from days (H. Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{d:02}.{m:02}.{y}")
}

/// A number as text: integers without decimals, others with up to 10.
pub fn fmt_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.10}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn read_xlsx(bytes: &[u8], range: &str) -> Result<Vec<Vec<String>>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let wb = zip_text(&mut z, "xl/workbook.xml").ok_or("not an Excel workbook")?;
    let rels = zip_text(&mut z, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    // sheets (name, rel id) and defined names
    let mut sheets: Vec<(String, String)> = vec![];
    let mut names: HashMap<String, String> = HashMap::new();
    {
        let mut r = Reader::from_str(&wb);
        let mut def_name: Option<String> = None;
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match local_name(e.name().as_ref()) {
                    b"sheet" => sheets.push((attr(&e, b"name").unwrap_or_default(), attr(&e, b"id").unwrap_or_default())),
                    b"definedName" => def_name = attr(&e, b"name"),
                    _ => {}
                },
                Ok(Event::Text(t)) => {
                    if let (Some(n), Ok(v)) = (def_name.take(), t.unescape()) {
                        names.insert(n.to_lowercase(), v.to_string());
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    let mut targets: HashMap<String, String> = HashMap::new();
    {
        let mut r = Reader::from_str(&rels);
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) if local_name(e.name().as_ref()) == b"Relationship" => {
                    targets.insert(attr(&e, b"Id").unwrap_or_default(), attr(&e, b"Target").unwrap_or_default());
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    // resolve the range
    let mut range = range.trim().to_string();
    if let Some(v) = names.get(&range.to_lowercase()) {
        range = v.clone();
    }
    let (sheet_name, cells) = match range.rsplit_once('!') {
        Some((s, c)) => (Some(s.trim_matches('\'').to_string()), c.to_string()),
        None if sheets.iter().any(|s| s.0.eq_ignore_ascii_case(&range)) => (Some(range.clone()), String::new()),
        None => (None, range.clone()),
    };
    let rng = if cells.trim().is_empty() { None } else { Some(parse_range(&cells).ok_or_else(|| format!("invalid range {cells}"))?) };
    let sheet = match &sheet_name {
        Some(n) => sheets.iter().find(|s| s.0.eq_ignore_ascii_case(n)).ok_or_else(|| format!("no sheet {n}"))?,
        None => sheets.first().ok_or("no sheets")?,
    };
    let target = targets.get(&sheet.1).cloned().unwrap_or_else(|| "worksheets/sheet1.xml".into());
    let path = if let Some(t) = target.strip_prefix('/') { t.to_string() } else { format!("xl/{target}") };
    let xml = zip_text(&mut z, &path).ok_or_else(|| format!("missing {path}"))?;
    let sst = zip_text(&mut z, "xl/sharedStrings.xml").map(|s| shared_strings(&s)).unwrap_or_default();
    let dates = zip_text(&mut z, "xl/styles.xml").map(|s| date_styles(&s)).unwrap_or_default();
    // cells
    let mut grid: Vec<Vec<String>> = vec![];
    let mut r = Reader::from_str(&xml);
    let mut cur: Option<((usize, usize), String, usize)> = None; // (pos, type, style)
    let mut val = String::new();
    let mut in_v = false;
    let mut next_col = 0usize;
    let mut row_no = 0usize;
    let put = |grid: &mut Vec<Vec<String>>, (c, rr): (usize, usize), s: String| {
        if grid.len() <= rr {
            grid.resize(rr + 1, vec![]);
        }
        if grid[rr].len() <= c {
            grid[rr].resize(c + 1, String::new());
        }
        grid[rr][c] = s;
    };
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                b"row" => {
                    row_no = attr(&e, b"r").and_then(|v| v.parse::<usize>().ok()).map(|v| v - 1).unwrap_or(row_no);
                    next_col = 0;
                }
                b"c" => {
                    let pos = attr(&e, b"r").and_then(|v| cell_ref(&v)).unwrap_or((next_col, row_no));
                    next_col = pos.0 + 1;
                    let t = attr(&e, b"t").unwrap_or_else(|| "n".into());
                    let s = attr(&e, b"s").and_then(|v| v.parse().ok()).unwrap_or(0);
                    cur = Some((pos, t, s));
                    val.clear();
                }
                b"v" | b"t" => in_v = true,
                _ => {}
            },
            Ok(Event::Empty(e)) if local_name(e.name().as_ref()) == b"c" => {
                next_col = attr(&e, b"r").and_then(|v| cell_ref(&v)).map(|p| p.0 + 1).unwrap_or(next_col + 1);
            }
            Ok(Event::Empty(e)) if local_name(e.name().as_ref()) == b"row" => {
                row_no = attr(&e, b"r").and_then(|v| v.parse::<usize>().ok()).unwrap_or(row_no + 1);
            }
            Ok(Event::Text(t)) if in_v => {
                if let Ok(s) = t.unescape() {
                    val.push_str(&s);
                }
            }
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                b"v" | b"t" => in_v = false,
                b"c" => {
                    if let Some((pos, t, s)) = cur.take() {
                        let text = match t.as_str() {
                            "s" => val.trim().parse::<usize>().ok().and_then(|k| sst.get(k).cloned()).unwrap_or_default(),
                            "b" => if val.trim() == "1" { "TRUE".into() } else { "FALSE".into() },
                            "str" | "inlineStr" | "e" => val.clone(),
                            _ => match val.trim().parse::<f64>() {
                                Ok(v) if dates.get(s).copied().unwrap_or(false) => excel_date(v),
                                Ok(v) => fmt_number(v),
                                Err(_) => val.clone(),
                            },
                        };
                        put(&mut grid, pos, text);
                    }
                }
                b"row" => row_no += 1,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    Ok(apply_range(grid, rng))
}

// ================================================================ output

/// Rows → a WP table (or tab-separated lines).
pub fn rows_to_els(rows: &[Vec<String>], table: bool, width: i32) -> Vec<El> {
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let mut v = vec![];
    let text = |v: &mut Vec<El>, s: &str| {
        for (k, line) in s.split('\n').enumerate() {
            if k > 0 {
                v.push(El::Code(Code::HRt));
            }
            v.extend(line.trim_end_matches('\r').chars().map(El::Ch));
        }
    };
    if table {
        let w = (width / ncols as i32).max(600);
        v.push(El::Code(Code::TblDef { cols: vec![w; ncols] }));
        for r in rows {
            v.push(El::Code(Code::Row));
            for c in 0..ncols {
                v.push(El::Code(Code::Cell));
                text(&mut v, r.get(c).map(|s| s.as_str()).unwrap_or(""));
            }
        }
        v.push(El::Code(Code::TblOff));
    } else {
        for r in rows {
            for (c, s) in r.iter().enumerate() {
                if c > 0 {
                    v.push(El::Code(Code::Tab));
                }
                text(&mut v, &s.replace('\n', " "));
            }
            v.push(El::Code(Code::HRt));
        }
    }
    v
}

fn text_width(wp: &Wp) -> i32 {
    let e = wp.ed_ref();
    let f = e.format_state_at(e.cursor);
    e.doc.init.page_width - f.left - f.right
}

fn import_els(wp: &Wp, file: &str, range: &str, table: bool) -> Result<Vec<El>, String> {
    let p = wp.resolve_path(file);
    let rows = read_range(&p, range)?;
    if rows.is_empty() {
        return Err("the range is empty".into());
    }
    Ok(rows_to_els(&rows, table, text_width(wp)))
}

/// Re-read every spreadsheet link of a document.
pub fn update_links(wp: &mut Wp) {
    let els = wp.ed_ref().doc.els.clone();
    let mut out = els.clone();
    let mut errors = vec![];
    // pairs, from the end
    let starts: Vec<usize> = els.iter().enumerate().filter(|(_, e)| matches!(e, El::Code(Code::SsLink { .. }))).map(|(i, _)| i).collect();
    for &s in starts.iter().rev() {
        let El::Code(Code::SsLink { file, range, table }) = &els[s] else { continue };
        let Some(end) = (s + 1..els.len()).find(|&k| matches!(els[k], El::Code(Code::SsLinkEnd | Code::SsLink { .. }))) else { continue };
        if !matches!(els[end], El::Code(Code::SsLinkEnd)) {
            continue;
        }
        match import_els(wp, file, range, *table) {
            Ok(v) => {
                out.splice(s + 1..end, v);
            }
            Err(e) => errors.push(format!("{file}: {e}")),
        }
    }
    let e = wp.ed();
    let cur = e.cursor;
    e.set_els(out);
    e.cursor = cur.min(e.doc.els.len());
    if let Some(m) = errors.first() {
        wp.flash(&format!("ERROR: {m}"));
    }
    wp.after_edit();
}

/// The link the cursor is in: (start, end) indices of the link codes.
fn link_at(els: &[El], pos: usize) -> Option<(usize, usize)> {
    let s = (0..pos.min(els.len())).rev().find(|&k| matches!(els[k], El::Code(Code::SsLink { .. } | Code::SsLinkEnd)))?;
    if !matches!(els[s], El::Code(Code::SsLink { .. })) {
        return None;
    }
    let e = (s + 1..els.len()).find(|&k| matches!(els[k], El::Code(Code::SsLinkEnd)))?;
    Some((s, e))
}

// ================================================================ UI

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Import,
    Link,
    EditLink(usize, usize),
}

struct SsForm {
    action: Action,
    file: String,
    range: String,
    table: bool,
    editing: Option<(usize, Vec<char>)>,
    choosing: bool,
}

/// Ctrl-F5, 5.
pub fn spreadsheet_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Import", "~Create Link", "~Edit Link", "~Link Options"], |wp, k| match k {
        0 | 1 => Some(Box::new(SsForm {
            action: if k == 0 { Action::Import } else { Action::Link },
            file: String::new(),
            range: String::new(),
            table: true,
            editing: Some((0, vec![])),
            choosing: false,
        }) as Box<dyn Screen>),
        2 => {
            let e = wp.ed_ref();
            let Some((s, end)) = link_at(&e.doc.els, e.cursor) else {
                wp.flash("ERROR: Not in a spreadsheet link");
                return None;
            };
            let El::Code(Code::SsLink { file, range, table }) = e.doc.els[s].clone() else { return None };
            Some(Box::new(SsForm { action: Action::EditLink(s, end), file, range, table, editing: None, choosing: false }) as Box<dyn Screen>)
        }
        _ => Some(MenuLine::new(&["~Update All Links"], |wp, _| {
            update_links(wp);
            None
        })
        .prefix("Link Options: ") as Box<dyn Screen>),
    })
    .prefix("Spreadsheet: ")
}

impl SsForm {
    fn perform(&self, wp: &mut Wp) -> Result<(), String> {
        if self.file.trim().is_empty() {
            return Err("no filename".into());
        }
        let data = import_els(wp, &self.file, &self.range, self.table)?;
        let link = Code::SsLink { file: self.file.trim().to_string(), range: self.range.trim().to_string(), table: self.table };
        let e = wp.ed();
        match self.action {
            Action::Import => {
                if e.cursor > 0 && self.table && !matches!(e.doc.els.get(e.cursor - 1), Some(El::Code(Code::HRt))) {
                    e.insert_code(Code::HRt);
                }
                e.insert_els(&data);
            }
            Action::Link => {
                let mut v = vec![El::Code(link)];
                v.extend(data);
                v.push(El::Code(Code::SsLinkEnd));
                e.insert_els(&v);
            }
            Action::EditLink(s, end) => {
                let mut els = e.doc.els.clone();
                let mut v = vec![El::Code(link)];
                v.extend(data);
                v.push(El::Code(Code::SsLinkEnd));
                els.splice(s..=end, v);
                e.set_els(els);
            }
        }
        Ok(())
    }
}

impl Screen for SsForm {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if self.choosing {
            match i {
                Input::Char('1') | Input::Char('t') | Input::Char('T') => self.table = true,
                Input::Char('2') | Input::Char('x') | Input::Char('X') => self.table = false,
                _ => {}
            }
            self.choosing = false;
            return Outcome::Keep;
        }
        if let Some((item, buf)) = &mut self.editing {
            match i {
                Input::Char(c) => buf.push(*c),
                Input::Func(Func::Backspace) => {
                    buf.pop();
                }
                Input::Func(Func::DelEol) => buf.clear(),
                Input::Func(Func::Enter) | Input::Func(Func::Exit) | Input::Func(Func::Tab) => {
                    let s: String = buf.iter().collect();
                    if *item == 0 {
                        self.file = s;
                    } else {
                        self.range = s;
                    }
                    self.editing = None;
                }
                Input::Func(Func::Cancel) => self.editing = None,
                _ => {}
            }
            return Outcome::Keep;
        }
        match i {
            Input::Char('1') | Input::Char('f') | Input::Char('F') => self.editing = Some((0, self.file.chars().collect())),
            Input::Char('2') | Input::Char('r') | Input::Char('R') => self.editing = Some((1, self.range.chars().collect())),
            Input::Char('3') | Input::Char('y') | Input::Char('Y') => self.choosing = true,
            Input::Char('4') | Input::Char('p') | Input::Char('P') => {
                return match self.perform(wp) {
                    Ok(()) => {
                        wp.after_edit();
                        Outcome::Close
                    }
                    Err(e) => {
                        wp.flash(&format!("ERROR: {e}"));
                        Outcome::Keep
                    }
                };
            }
            Input::Func(Func::Exit) => return Outcome::CloseAll,
            Input::Func(Func::Cancel) | Input::Func(Func::Enter) | Input::Char('0') => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        let (title, perform) = match self.action {
            Action::Import => ("Spreadsheet: Import", "~Perform Import"),
            Action::Link => ("Spreadsheet: Create Link", "~Perform Link"),
            Action::EditLink(..) => ("Spreadsheet: Edit Link", "~Perform Link"),
        };
        g.text(0, 0, title, st);
        let range_shown = if self.range.is_empty() && !self.file.is_empty() { "(All)".to_string() } else { self.range.clone() };
        let items: [(&str, String); 4] = [
            ("~Filename", self.file.clone()),
            ("~Range", range_shown),
            ("T~ype", if self.table { "Table".into() } else { "Text".into() }),
            (perform, String::new()),
        ];
        let mut cursor = None;
        for (k, (label, val)) in items.iter().enumerate() {
            let row = 2 + 2 * k;
            g.text(5, row, &format!("{} - ", k + 1), st);
            let end = g.text_mn(9, row, label, st, hi);
            wp.hit(row, 5, end.max(60), HitAction::Input(Input::Char((b'1' + k as u8) as char)));
            let v = match &self.editing {
                Some((it, buf)) if *it == k => {
                    let s: String = buf.iter().collect();
                    cursor = Some((40 + buf.len(), row));
                    s
                }
                _ => val.clone(),
            };
            g.text(40, row, &v, st);
        }
        let fr = g.rows - 1;
        if self.choosing {
            let c = g.text_mn(0, fr, "Type: 1 ~Table; 2 Te~xt: ", st, hi);
            g.text(c, fr, "0", st);
            cursor = Some((c, fr));
        } else if cursor.is_none() {
            let c = g.text(0, fr, "Selection: ", st);
            g.text(c, fr, "0", st);
            cursor = Some((c, fr));
        }
        g.cursor = cursor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal .xlsx written with the zip crate.
    pub fn sample_xlsx() -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o: zip::write::SimpleFileOptions = Default::default();
            let files = [
                ("xl/workbook.xml", r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Kunden" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="Preise">Kunden!$B$1:$C$3</definedName></definedNames></workbook>"#),
                ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="x" Target="worksheets/sheet1.xml"/></Relationships>"#),
                ("xl/sharedStrings.xml", r#"<?xml version="1.0"?><sst><si><t>Name</t></si><si><t>Preis</t></si><si><r><t>Mül</t></r><r><t>ler &amp; Co</t></r></si><si><t>Datum</t></si></sst>"#),
                ("xl/styles.xml", r#"<?xml version="1.0"?><styleSheet><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14"/></cellXfs></styleSheet>"#),
                ("xl/worksheets/sheet1.xml", r#"<?xml version="1.0"?><worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>3</v></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>12.5</v></c><c r="C2" s="1"><v>46337</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Weiß</t></is></c><c r="B3"><v>3</v></c></row></sheetData></worksheet>"#),
            ];
            for (n, x) in files {
                z.start_file(n, o).unwrap();
                z.write_all(x.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn xlsx_read() {
        let b = sample_xlsx();
        let g = read_xlsx(&b, "").unwrap();
        assert_eq!(g[0], vec!["Name", "Preis", "Datum"]);
        assert_eq!(g[1], vec!["Müller & Co", "12.5", "11.11.2026"]);
        assert_eq!(g[2], vec!["Weiß", "3", ""]);
        assert_eq!(read_xlsx(&b, "Preise").unwrap()[2], vec!["3", ""]);
        assert_eq!(read_xlsx(&b, "A2:B2").unwrap(), vec![vec!["Müller & Co".to_string(), "12.5".to_string()]]);
        assert_eq!(read_xlsx(&b, "Kunden!A3").unwrap(), vec![vec!["Weiß".to_string()]]);
        // as a table, then merge data
        let els = rows_to_els(&g, true, 7200);
        assert!(matches!(&els[0], El::Code(Code::TblDef { cols }) if cols.len() == 3));
        let d = crate::merge::Data::from_rows(g);
        assert_eq!(d.records.len(), 2);
        assert_eq!(d.field(1, "name").map(|v| v.len()), Some(4));
        // text mode
        let t = rows_to_els(&[vec!["a".into(), "b".into()]], false, 7200);
        assert_eq!(t, vec![El::Ch('a'), El::Code(Code::Tab), El::Ch('b'), El::Code(Code::HRt)]);
    }

    #[test]
    fn csv_range() {
        let dir = std::env::temp_dir().join(format!("bsw51_sheet_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.csv");
        std::fs::write(&p, "a;b;c\n1;2;3\n4;5;6\n").unwrap();
        assert_eq!(read_range(&p, "B2:C3").unwrap(), vec![vec!["2", "3"], vec!["5", "6"]]);
        assert_eq!(read_range(&p, "").unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
