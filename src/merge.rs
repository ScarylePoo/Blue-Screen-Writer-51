//! Merge (WP 5.1 merge language): Shift-F9 Merge Codes, F9 End Field,
//! Ctrl-F9 Merge/Sort, and the merge engine.
//!
//! Merge commands are `Code::Merge(name)` in the stream; their parameters
//! follow as ordinary text, each ending with `~` — exactly like WP 5.1,
//! which shows `{FIELD}name~` on screen.  Secondary files are WP records
//! (`…{END FIELD}[HRt]…{END RECORD}[HPg]`, optional `{FIELD NAMES}a~b~~`)
//! or CSV/TSV/.xlsx tables whose first row holds the field names.

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{HitAction, Outcome, Screen, Wp};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Text shown on the editing screen for a text-like code.
pub fn screen_text(c: &Code) -> String {
    match c {
        Code::Merge(n) => format!("{{{n}}}"),
        Code::MathOp(c) => c.to_string(),
        _ => String::new(),
    }
}

/// (name, fixed parameters, ends with a `~~` list, parameter prompts)
type CmdSpec = (&'static str, u8, bool, &'static [&'static str]);

/// The WP 5.1 merge commands (Shift-F9, 6 More).
pub const COMMANDS: &[CmdSpec] = &[
    ("ASSIGN", 2, false, &["Variable:", "Expression:"]),
    ("BELL", 0, false, &[]),
    ("BREAK", 0, false, &[]),
    ("CALL", 1, false, &["Label:"]),
    ("CANCEL OFF", 0, false, &[]),
    ("CANCEL ON", 0, false, &[]),
    ("CASE", 1, true, &["Expression:", "Case:", "Label:"]),
    ("CASE CALL", 1, true, &["Expression:", "Case:", "Label:"]),
    ("CHAIN MACRO", 1, false, &["Macro name:"]),
    ("CHAIN PRIMARY", 1, false, &["Primary file:"]),
    ("CHAIN SECONDARY", 1, false, &["Secondary file:"]),
    ("CHAR", 2, false, &["Variable:", "Message:"]),
    ("COMMENT", 1, false, &["Comment:"]),
    ("CTON", 1, false, &["Character:"]),
    ("DATE", 0, false, &[]),
    ("DOCUMENT", 1, false, &["Filename:"]),
    ("ELSE", 0, false, &[]),
    ("END FIELD", 0, false, &[]),
    ("END FOR", 0, false, &[]),
    ("END IF", 0, false, &[]),
    ("END RECORD", 0, false, &[]),
    ("END WHILE", 0, false, &[]),
    ("FIELD", 1, false, &["Field:"]),
    ("FIELD NAMES", 0, true, &["Field name:"]),
    ("FOR", 4, false, &["Variable:", "Start:", "Stop:", "Step:"]),
    ("FOR EACH", 1, true, &["Variable:", "Expression:"]),
    ("GO", 1, false, &["Label:"]),
    ("IF", 1, false, &["Expression:"]),
    ("IF BLANK", 1, false, &["Field:"]),
    ("IF EXISTS", 1, false, &["Variable:"]),
    ("IF NOT BLANK", 1, false, &["Field:"]),
    ("INPUT", 1, false, &["Message:"]),
    ("KEYBOARD", 0, false, &[]),
    ("LABEL", 1, false, &["Label:"]),
    ("LEN", 1, false, &["Expression:"]),
    ("LOCAL", 2, false, &["Variable:", "Expression:"]),
    ("LOOK", 1, false, &["Variable:"]),
    ("MID", 3, false, &["Expression:", "Offset:", "Count:"]),
    ("MRG CMND", 0, false, &[]),
    ("NEST MACRO", 1, false, &["Macro name:"]),
    ("NEST PRIMARY", 1, false, &["Primary file:"]),
    ("NEST SECONDARY", 1, false, &["Secondary file:"]),
    ("NEXT", 0, false, &[]),
    ("NEXT RECORD", 0, false, &[]),
    ("NTOC", 1, false, &["Number:"]),
    ("ON CANCEL", 1, false, &["Action:"]),
    ("ON ERROR", 1, false, &["Action:"]),
    ("PAGE OFF", 0, false, &[]),
    ("PAGE ON", 0, false, &[]),
    ("PRINT", 0, false, &[]),
    ("PROCESS", 0, false, &[]),
    ("PROMPT", 1, false, &["Message:"]),
    ("QUIT", 0, false, &[]),
    ("RETURN", 0, false, &[]),
    ("RETURN CANCEL", 0, false, &[]),
    ("RETURN ERROR", 0, false, &[]),
    ("REWRITE", 0, false, &[]),
    ("STATUS PROMPT", 1, false, &["Message:"]),
    ("STEP OFF", 0, false, &[]),
    ("STEP ON", 0, false, &[]),
    ("SUBST PRIMARY", 1, false, &["Primary file:"]),
    ("SUBST SECONDARY", 1, false, &["Secondary file:"]),
    ("SYSTEM", 1, false, &["System variable:"]),
    ("TEXT", 2, false, &["Variable:", "Message:"]),
    ("VARIABLE", 1, false, &["Variable:"]),
    ("WAIT", 1, false, &["10ths second:"]),
    ("WHILE", 1, false, &["Expression:"]),
];

fn spec(name: &str) -> (u8, bool) {
    COMMANDS.iter().find(|c| c.0 == name).map(|c| (c.1, c.2)).unwrap_or((0, false))
}

/// The template shown in the command list, e.g. `{ASSIGN}var~expr~`.
fn template(c: &CmdSpec) -> String {
    let mut s = format!("{{{}}}", c.0);
    for p in c.3.iter().take(c.1 as usize) {
        s.push_str(&p.trim_end_matches(':').to_lowercase().replace(' ', ""));
        s.push('~');
    }
    if c.2 {
        s.push_str("…~~");
    }
    s
}

// ================================================================ keys

/// Code + parameters as elements: `{NAME}p1~p2~`.
pub fn command_els(name: &str, params: &[String], list: bool) -> Vec<El> {
    let mut v = vec![El::Code(Code::Merge(name.into()))];
    for p in params {
        v.extend(p.chars().map(El::Ch));
        v.push(El::Ch('~'));
    }
    if list {
        v.push(El::Ch('~'));
    }
    match name {
        "END RECORD" => v.push(El::Code(Code::HPg)),
        "END FIELD" => v.push(El::Code(Code::HRt)),
        _ => {}
    }
    v
}

fn insert(wp: &mut Wp, els: Vec<El>) {
    wp.ed().insert_els(&els);
    wp.after_edit();
}

/// F9 End Field: `{END FIELD}[HRt]`.
pub fn end_field(wp: &mut Wp) {
    insert(wp, command_els("END FIELD", &[], false));
}

/// Ask for a command's parameters one by one, then insert it.
fn ask_params(name: &'static str, got: Vec<String>) -> Box<dyn Screen> {
    let (fixed, list) = spec(name);
    let prompts = COMMANDS.iter().find(|c| c.0 == name).map(|c| c.3).unwrap_or(&[]);
    let k = got.len();
    let label = if k < fixed as usize {
        prompts.get(k).copied().unwrap_or("Parameter:").to_string()
    } else {
        // list entries (empty entry ends the list)
        let extra = &prompts[(fixed as usize).min(prompts.len())..];
        let base = if extra.is_empty() { "Item" } else { extra[(k - fixed as usize) % extra.len()].trim_end_matches(':') };
        format!("{base} (Enter when done):")
    };
    Prompt::new(&label, "", move |wp, s| {
        let s = s?;
        let mut got = got;
        if k >= fixed as usize && s.is_empty() {
            insert(wp, command_els(name, &got, list));
            return None;
        }
        got.push(s);
        if got.len() >= fixed as usize && !list {
            insert(wp, command_els(name, &got, list));
            return None;
        }
        Some(ask_params(name, got))
    })
}

fn pick_command(wp: &mut Wp, name: &'static str) -> Next {
    let (fixed, list) = spec(name);
    if fixed == 0 && !list {
        insert(wp, command_els(name, &[], false));
        return None;
    }
    Some(ask_params(name, vec![]))
}

/// Shift-F9 Merge Codes.
pub fn merge_codes_menu(wp: &mut Wp) {
    wp.push(MenuLine::new(&["~Field", "~End Record", "~Input", "~Page Off", "~Next Record", "~More"], |wp, k| match k {
        0 => pick_command(wp, "FIELD"),
        1 => pick_command(wp, "END RECORD"),
        2 => pick_command(wp, "INPUT"),
        3 => pick_command(wp, "PAGE OFF"),
        4 => pick_command(wp, "NEXT RECORD"),
        _ => Some(Box::new(CommandList { sel: 0, top: 0, search: String::new() }) as Box<dyn Screen>),
    }));
}

/// Shift-F9, 6 More: the list of all merge commands (name search by typing).
struct CommandList {
    sel: usize,
    top: usize,
    search: String,
}

impl CommandList {
    const HEIGHT: usize = 16;
    fn fix_top(&mut self) {
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + Self::HEIGHT {
            self.top = self.sel + 1 - Self::HEIGHT;
        }
    }
}

impl Screen for CommandList {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = COMMANDS.len();
        match i {
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
            Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n - 1),
            Input::Func(Func::PgUp) | Input::Func(Func::ScreenUp) => self.sel = self.sel.saturating_sub(Self::HEIGHT),
            Input::Func(Func::PgDn) | Input::Func(Func::ScreenDown) => self.sel = (self.sel + Self::HEIGHT).min(n - 1),
            Input::Func(Func::Home) => self.sel = 0,
            Input::Func(Func::End) => self.sel = n - 1,
            Input::Func(Func::Backspace) => {
                self.search.pop();
            }
            Input::Char(c) => {
                self.search.push(c.to_ascii_uppercase());
                if let Some(k) = COMMANDS.iter().position(|x| x.0.starts_with(&self.search)) {
                    self.sel = k;
                }
            }
            Input::Func(Func::Enter) | Input::Func(Func::SearchFwd) => {
                let name = COMMANDS[self.sel].0;
                return match pick_command(wp, name) {
                    Some(s) => Outcome::Replace(s),
                    None => Outcome::Close,
                };
            }
            Input::Func(Func::Cancel) | Input::Func(Func::Exit) | Input::Func(Func::Esc) => return Outcome::Close,
            _ => {}
        }
        self.fix_top();
        Outcome::Keep
    }

    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n.min(COMMANDS.len() - 1);
        if double {
            return self.input(wp, &Input::Func(Func::Enter));
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let (c0, w) = (g.cols.saturating_sub(42), 40usize);
        let r0 = 1usize;
        let h = Self::HEIGHT;
        let line = |g: &mut Grid, r: usize, l: char, m: char, rr: char| {
            g.put(c0, r, l, st);
            for c in 1..w - 1 {
                g.put(c0 + c, r, m, st);
            }
            g.put(c0 + w - 1, r, rr, st);
        };
        line(g, r0, '╔', '═', '╗');
        for k in 0..h {
            let r = r0 + 1 + k;
            line(g, r, '║', ' ', '║');
            if let Some(cmd) = COMMANDS.get(self.top + k) {
                let t: String = template(cmd).chars().take(w - 4).collect();
                let s = if self.top + k == self.sel { wp.theme.block } else { st };
                g.fill_row(r, c0 + 1, c0 + w - 1, ' ', s);
                g.text(c0 + 2, r, &t, s);
                wp.hit(r, c0 + 1, c0 + w - 1, HitAction::Select(self.top + k));
            }
        }
        line(g, r0 + h + 1, '╟', '─', '╢');
        line(g, r0 + h + 2, '║', ' ', '║');
        g.text(c0 + w - 16, r0 + h + 2, "(Name Search)", st);
        line(g, r0 + h + 3, '╚', '═', '╝');
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', st);
        let c = g.text(0, row, "Select a merge command; Enter to insert: ", st);
        g.text(c, row, &self.search, st);
        g.cursor = Some((c + self.search.chars().count(), row));
    }
}

/// Ctrl-F9 Merge/Sort (with Block on: sort the block).
pub fn merge_sort_menu(wp: &mut Wp) {
    if wp.ed_ref().block_anchor.is_some() {
        crate::sort::open_block(wp);
        return;
    }
    wp.push(MenuLine::new(&["~Merge", "~Sort", "~Convert Old Merge Codes"], |wp, k| match k {
        0 => Some(Prompt::new("Primary file:", "", |_wp, s| {
            let prim = s?;
            Some(Prompt::new("Secondary file:", "", move |wp, s| {
                let sec = s?;
                start_merge(wp, prim.trim(), sec.trim())
            }) as Box<dyn Screen>)
        }) as Box<dyn Screen>),
        1 => crate::sort::open_prompts(wp),
        _ => {
            let e = wp.ed();
            let els = convert_codes(&e.doc.els);
            e.set_els(els);
            wp.after_edit();
            None
        }
    }));
}

// ============================================================ secondary data

/// Field names + records (each a list of fields).
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub names: Vec<String>,
    pub records: Vec<Vec<Vec<El>>>,
}

impl Data {
    /// Field by name (case-insensitive) or 1-based number.  `Some(empty)`
    /// for an existing but empty field.
    pub fn field(&self, rec: usize, name: &str) -> Option<&[El]> {
        let r = self.records.get(rec)?;
        let n = name.trim();
        let k = match self.names.iter().position(|x| x.trim().eq_ignore_ascii_case(n)) {
            Some(k) => k,
            None => n.parse::<usize>().ok().filter(|k| *k >= 1)? - 1,
        };
        Some(r.get(k).map(|v| v.as_slice()).unwrap_or(&[]))
    }

    /// A table (first row = field names) as merge data.
    pub fn from_rows(rows: Vec<Vec<String>>) -> Data {
        let mut it = rows.into_iter();
        let names = it.next().unwrap_or_default().into_iter().map(|s| s.trim().to_string()).collect();
        let records = it
            .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
            .map(|r| r.into_iter().map(|c| Document::from_text(&c).els).collect())
            .collect();
        Data { names, records }
    }
}

fn is_cmd(e: &El, name: &str) -> bool {
    matches!(e, El::Code(Code::Merge(n)) if n == name)
}

/// Parse a WP secondary merge file.
pub fn parse_secondary(els: &[El]) -> Data {
    let mut d = Data::default();
    let n = els.len();
    let mut i = 0;
    // {FIELD NAMES}a~b~~{END RECORD}
    while i < n && matches!(els[i], El::Code(Code::HRt) | El::Ch(' ')) {
        i += 1;
    }
    if i < n && is_cmd(&els[i], "FIELD NAMES") {
        let (ps, next) = scan_params(els, i + 1, 0, true);
        d.names = ps.iter().map(|p| plain(p)).collect();
        i = next;
        while i < n && !is_cmd(&els[i], "END RECORD") && !matches!(els[i], El::Ch(c) if !c.is_whitespace()) {
            i += 1;
        }
        if i < n && is_cmd(&els[i], "END RECORD") {
            i += 1;
        }
        while i < n && matches!(els[i], El::Code(Code::HPg | Code::HRt)) {
            i += 1;
        }
    }
    let mut rec: Vec<Vec<El>> = vec![];
    let mut field: Vec<El> = vec![];
    while i < n {
        if is_cmd(&els[i], "END FIELD") {
            rec.push(std::mem::take(&mut field));
            i += 1;
            if matches!(els.get(i), Some(El::Code(Code::HRt))) {
                i += 1;
            }
            continue;
        }
        if is_cmd(&els[i], "END RECORD") {
            if !field.iter().all(|e| matches!(e, El::Code(Code::HRt))) {
                rec.push(std::mem::take(&mut field));
            }
            field.clear();
            d.records.push(std::mem::take(&mut rec));
            i += 1;
            while i < n && matches!(els[i], El::Code(Code::HPg | Code::HRt)) {
                i += 1;
            }
            continue;
        }
        field.push(els[i].clone());
        i += 1;
    }
    if !rec.is_empty() || field.iter().any(|e| !matches!(e, El::Code(Code::HRt) | El::Ch(' '))) {
        if !field.is_empty() {
            rec.push(field);
        }
        d.records.push(rec);
    }
    d
}

/// CSV / TSV text → rows.  Delimiter: tab if the first line has one, else
/// `;` (German Excel) if it has more `;` than `,`, else `,`.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let delim = if first.contains('\t') {
        '\t'
    } else if first.matches(';').count() > first.matches(',').count() {
        ';'
    } else {
        ','
    };
    let mut rows = vec![];
    let mut row = vec![];
    let mut cell = String::new();
    let mut q = false;
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if q {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    cell.push('"');
                    it.next();
                } else {
                    q = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => q = true,
            c if c == delim => row.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// Load merge data: CSV/TSV, .xlsx, or any document (WP records).
pub fn load_data(p: &Path) -> Result<Data, String> {
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "csv" | "tsv" | "tab" => {
            let b = std::fs::read(p).map_err(|e| e.to_string())?;
            Ok(Data::from_rows(parse_csv(&crate::fileio::decode_text(&b))))
        }
        "xlsx" | "xlsm" => Ok(Data::from_rows(crate::sheet::read_range(p, "")?)),
        _ => {
            let d = crate::fileio::load(p).map_err(|e| e.to_string())?;
            let els = convert_codes(&d.els);
            if !els.iter().any(|e| is_cmd(e, "END FIELD") || is_cmd(e, "END RECORD")) && ext == "txt" {
                return Ok(Data::from_rows(parse_csv(&d.plain_text(0, d.len()))));
            }
            Ok(parse_secondary(&els))
        }
    }
}

// ============================================================ conversion

/// Turn text placeholders `{FIELD}` (e.g. from a document saved to Word)
/// and WP 5.0 merge codes (`^F1^`, `^R`, `^E` …) into merge codes.
pub fn convert_codes(els: &[El]) -> Vec<El> {
    let mut out = Vec::with_capacity(els.len());
    let mut i = 0;
    while i < els.len() {
        if let El::Ch('{') = els[i] {
            let mut name = String::new();
            let mut j = i + 1;
            while j < els.len() && j < i + 20 {
                match els[j] {
                    El::Ch('}') => break,
                    El::Ch(c) => name.push(c),
                    _ => break,
                }
                j += 1;
            }
            if matches!(els.get(j), Some(El::Ch('}'))) && COMMANDS.iter().any(|c| c.0 == name) {
                out.push(El::Code(Code::Merge(name)));
                i = j + 1;
                continue;
            }
        }
        if let El::Code(Code::Unknown { name, .. }) = &els[i] {
            if let Some(x) = name.strip_prefix("Mrg:^").and_then(|x| x.chars().next()) {
                let (cmd, close) = match x {
                    'C' => ("KEYBOARD", None),
                    'D' => ("DATE", None),
                    'E' => ("END RECORD", None),
                    'F' => ("FIELD", Some('^')),
                    'G' => ("CHAIN MACRO", Some('G')),
                    'N' => ("NEXT RECORD", None),
                    'O' => ("STATUS PROMPT", Some('O')),
                    'P' => ("SUBST PRIMARY", Some('P')),
                    'Q' => ("QUIT", None),
                    'R' => ("END FIELD", None),
                    'S' => ("SUBST SECONDARY", Some('S')),
                    'T' => ("PRINT", None),
                    'U' => ("REWRITE", None),
                    'V' => ("MRG CMND", None),
                    _ => ("", None),
                };
                if !cmd.is_empty() {
                    out.push(El::Code(Code::Merge(cmd.into())));
                    i += 1;
                    if let Some(cl) = close {
                        // parameter up to the closing ^ / same code → `~`
                        while i < els.len() {
                            let end = match &els[i] {
                                El::Ch('^') if cl == '^' => true,
                                El::Code(Code::Unknown { name, .. }) => name == &format!("Mrg:^{cl}"),
                                _ => false,
                            };
                            if end {
                                i += 1;
                                break;
                            }
                            out.push(els[i].clone());
                            i += 1;
                        }
                        out.push(El::Ch('~'));
                    }
                    continue;
                }
            }
        }
        out.push(els[i].clone());
        i += 1;
    }
    out
}

// ============================================================ engine

fn plain(els: &[El]) -> String {
    let mut s = String::new();
    for e in els {
        match e {
            El::Ch(c) => s.push(*c),
            El::Code(Code::HardSpace) => s.push(' '),
            El::Code(Code::HRt) => s.push('\n'),
            El::Code(Code::Tab) => s.push('\t'),
            _ => {}
        }
    }
    s
}

/// Parameters starting at `pos`: `fixed` of them, then (if `list`) more
/// until an empty one (`~~`).  Nested commands' own parameters are part of
/// the enclosing parameter.  Returns (parameters, position after).
pub fn scan_params(els: &[El], mut pos: usize, fixed: u8, list: bool) -> (Vec<Vec<El>>, usize) {
    let mut ps = vec![];
    loop {
        if ps.len() >= fixed as usize && !list {
            break;
        }
        if pos >= els.len() {
            break;
        }
        let start = pos;
        while pos < els.len() {
            match &els[pos] {
                El::Ch('~') => break,
                El::Code(Code::Merge(n)) => {
                    let (f, l) = spec(n);
                    let (_, next) = scan_params(els, pos + 1, f, l);
                    pos = next;
                }
                _ => pos += 1,
            }
        }
        let p = els[start..pos.min(els.len())].to_vec();
        pos += 1; // the ~
        if ps.len() >= fixed as usize && p.is_empty() {
            break; // ~~ ends a list
        }
        ps.push(p);
    }
    (ps, pos.min(els.len()))
}

/// A value in a merge expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    N(i64),
    S(String),
}

impl Val {
    fn text(&self) -> String {
        match self {
            Val::N(n) => n.to_string(),
            Val::S(s) => s.clone(),
        }
    }
    fn num(&self) -> i64 {
        match self {
            Val::N(n) => *n,
            Val::S(s) => s.trim().parse().unwrap_or(0),
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Val::N(n) => *n != 0,
            Val::S(s) => s.trim().parse::<i64>().map(|n| n != 0).unwrap_or(!s.trim().is_empty()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    V(Val),
    Op(&'static str),
}

fn lex(s: &str) -> Vec<Tok> {
    const OPS: [&str; 17] = ["!=", "<>", "<=", ">=", "=", "<", ">", "+", "-", "*", "/", "%", "&", "|", "!", "(", ")"];
    let cs: Vec<char> = s.chars().collect();
    let mut t = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' {
            let mut v = String::new();
            i += 1;
            while i < cs.len() && cs[i] != '"' {
                v.push(cs[i]);
                i += 1;
            }
            i += 1;
            t.push(Tok::V(Val::S(v)));
        } else if c.is_ascii_digit() {
            let mut v = 0i64;
            while i < cs.len() && cs[i].is_ascii_digit() {
                v = v.saturating_mul(10).saturating_add(cs[i] as i64 - 48);
                i += 1;
            }
            // a decimal part (integer arithmetic, as in WP) is dropped
            if i + 1 < cs.len() && (cs[i] == ',' || cs[i] == '.') && cs[i + 1].is_ascii_digit() {
                i += 1;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
            }
            t.push(Tok::V(Val::N(v)));
        } else if let Some(op) = OPS.iter().find(|o| cs[i..].iter().take(o.len()).copied().eq(o.chars())) {
            t.push(Tok::Op(op));
            i += op.len();
        } else {
            let mut v = String::new();
            while i < cs.len() && !cs[i].is_whitespace() && !"=<>+-*/%&|!()\"".contains(cs[i]) {
                v.push(cs[i]);
                i += 1;
            }
            t.push(Tok::V(Val::S(v)));
        }
    }
    t
}

struct Ex {
    t: Vec<Tok>,
    i: usize,
}

impl Ex {
    fn take(&mut self, ops: &[&'static str]) -> Option<&'static str> {
        if let Some(Tok::Op(o)) = self.t.get(self.i) {
            if ops.contains(o) {
                self.i += 1;
                return Some(o);
            }
        }
        None
    }
    fn or(&mut self) -> Val {
        let mut a = self.and();
        while self.take(&["|"]).is_some() {
            let b = self.and();
            a = Val::N(a.num() | b.num());
        }
        a
    }
    fn and(&mut self) -> Val {
        let mut a = self.cmp();
        while self.take(&["&"]).is_some() {
            let b = self.cmp();
            a = Val::N(a.num() & b.num());
        }
        a
    }
    fn cmp(&mut self) -> Val {
        let mut a = self.add();
        while let Some(op) = self.take(&["=", "!=", "<>", "<", ">", "<=", ">="]) {
            let b = self.add();
            let ord = match (&a, &b) {
                (Val::N(x), Val::N(y)) => x.cmp(y),
                _ => a.text().to_lowercase().cmp(&b.text().to_lowercase()),
            };
            use std::cmp::Ordering::*;
            let r = match op {
                "=" => ord == Equal,
                "!=" | "<>" => ord != Equal,
                "<" => ord == Less,
                ">" => ord == Greater,
                "<=" => ord != Greater,
                _ => ord != Less,
            };
            a = Val::N(if r { -1 } else { 0 });
        }
        a
    }
    fn add(&mut self) -> Val {
        let mut a = self.mul();
        while let Some(op) = self.take(&["+", "-"]) {
            let b = self.mul();
            a = Val::N(if op == "+" { a.num().wrapping_add(b.num()) } else { a.num().wrapping_sub(b.num()) });
        }
        a
    }
    fn mul(&mut self) -> Val {
        let mut a = self.unary();
        while let Some(op) = self.take(&["*", "/", "%"]) {
            let b = self.unary().num();
            let x = a.num();
            a = Val::N(match op {
                "*" => x.wrapping_mul(b),
                "/" => x.checked_div(b).unwrap_or(0),
                _ => x.checked_rem(b).unwrap_or(0),
            });
        }
        a
    }
    fn unary(&mut self) -> Val {
        match self.take(&["-", "!", "+"]) {
            Some("-") => Val::N(self.unary().num().wrapping_neg()),
            Some("!") => Val::N(!self.unary().num()),
            Some(_) => Val::N(self.unary().num()),
            None => self.primary(),
        }
    }
    fn primary(&mut self) -> Val {
        match self.t.get(self.i).cloned() {
            Some(Tok::V(v)) => {
                self.i += 1;
                v
            }
            Some(Tok::Op("(")) => {
                self.i += 1;
                let v = self.or();
                self.take(&[")"]);
                v
            }
            _ => Val::S(String::new()),
        }
    }
}

/// Evaluate a merge expression (integers, `+ - * / %`, `& | !`,
/// comparisons give -1/0; text compares case-insensitively).
pub fn eval(s: &str) -> Val {
    let t = lex(s);
    if t.len() == 1 {
        if let Tok::V(v) = &t[0] {
            return v.clone();
        }
    }
    let mut e = Ex { t, i: 0 };
    let v = e.or();
    if e.i < e.t.len() { Val::S(s.trim().to_string()) } else { v }
}

/// What the engine needs from the user.
#[derive(Debug, PartialEq)]
pub enum Step {
    Done,
    /// Keyboard input: message, variable to set (None = typed into the document).
    Input { prompt: String, var: Option<String> },
}

enum Loop {
    While { pc: usize },
    For { var: String, stop: i64, step: i64, body: usize },
    Each { var: String, rest: VecDeque<String>, body: usize },
}

pub struct Engine {
    primary: Rc<Vec<El>>,
    code: Rc<Vec<El>>,
    pc: usize,
    calls: Vec<(Rc<Vec<El>>, usize)>,
    chain: Option<Rc<Vec<El>>>,
    data: Option<Data>,
    rec: usize,
    pub out: Vec<El>,
    vars: HashMap<String, String>,
    page_off: bool,
    loops: Vec<Loop>,
    base: PathBuf,
    date: String,
    pub messages: Vec<String>,
    steps: u64,
    quit: bool,
}

impl Engine {
    pub fn new(primary: Vec<El>, data: Option<Data>, date: &str, base: &Path) -> Engine {
        let p = Rc::new(primary);
        Engine {
            primary: p.clone(),
            code: p,
            pc: 0,
            calls: vec![],
            chain: None,
            data,
            rec: 0,
            out: vec![],
            vars: HashMap::new(),
            page_off: false,
            loops: vec![],
            base: base.to_path_buf(),
            date: date.into(),
            messages: vec![],
            steps: 0,
            quit: false,
        }
    }

    fn path(&self, s: &str) -> PathBuf {
        let p = Path::new(s.trim());
        if p.is_absolute() { p.to_path_buf() } else { self.base.join(p) }
    }

    fn load_els(&mut self, s: &str) -> Option<Vec<El>> {
        match crate::fileio::load(&self.path(s)) {
            Ok(d) => Some(d.els),
            Err(e) => {
                self.messages.push(format!("ERROR: {} — {e}", s.trim()));
                None
            }
        }
    }

    fn field(&self, name: &str) -> Option<Vec<El>> {
        self.data.as_ref().and_then(|d| d.field(self.rec, name)).map(|v| v.to_vec())
    }

    /// The value of a value-producing command, as elements.
    fn value(&self, cmd: &str, ps: &[Vec<El>]) -> Option<Vec<El>> {
        let arg = |k: usize| ps.get(k).map(|p| self.expand(p)).unwrap_or_default();
        let txt = |s: String| Some(s.chars().map(El::Ch).collect());
        match cmd {
            "FIELD" => {
                let n = arg(0);
                Some(self.field(n.trim_end_matches('?')).unwrap_or_default())
            }
            "VARIABLE" => txt(self.vars.get(arg(0).trim()).cloned().unwrap_or_default()),
            "DATE" => txt(self.date.clone()),
            "LEN" => txt(arg(0).chars().count().to_string()),
            "MID" => {
                let s: Vec<char> = arg(0).chars().collect();
                let off = eval(&arg(1)).num().max(0) as usize;
                let cnt = eval(&arg(2)).num().max(0) as usize;
                txt(s.iter().skip(off).take(cnt).collect())
            }
            "NTOC" => txt(char::from_u32(eval(&arg(0)).num() as u32).map(|c| c.to_string()).unwrap_or_default()),
            "CTON" => txt(arg(0).chars().next().map(|c| (c as u32).to_string()).unwrap_or_default()),
            "SYSTEM" => txt(match arg(0).trim().to_lowercase().as_str() {
                "record" => (self.rec + 1).to_string(),
                "date" => self.date.clone(),
                _ => String::new(),
            }),
            _ => None,
        }
    }

    /// A parameter as text (nested value commands expanded).
    fn expand(&self, els: &[El]) -> String {
        let mut s = String::new();
        let mut i = 0;
        while i < els.len() {
            if let El::Code(Code::Merge(n)) = &els[i] {
                let (f, l) = spec(n);
                let (ps, next) = scan_params(els, i + 1, f, l);
                if let Some(v) = self.value(n, &ps) {
                    s.push_str(&plain(&v));
                }
                i = next.max(i + 1);
                continue;
            }
            s.push_str(&plain(&els[i..i + 1]));
            i += 1;
        }
        s
    }

    /// Skip forward from pc past the matching `close` (or `also` at the
    /// same depth); returns the command that ended the skip.
    fn skip(&mut self, opens: &[&str], close: &str, also: Option<&str>) -> Option<String> {
        let mut depth = 0;
        while self.pc < self.code.len() {
            let el = &self.code[self.pc];
            self.pc += 1;
            if let El::Code(Code::Merge(n)) = el {
                if opens.contains(&n.as_str()) {
                    depth += 1;
                } else if n == close {
                    if depth == 0 {
                        return Some(n.clone());
                    }
                    depth -= 1;
                } else if depth == 0 && Some(n.as_str()) == also {
                    return Some(n.clone());
                }
            }
        }
        None
    }

    fn goto_label(&mut self, label: &str) -> bool {
        let code = self.code.clone();
        for (i, e) in code.iter().enumerate() {
            if is_cmd(e, "LABEL") {
                let (ps, next) = scan_params(&code, i + 1, 1, false);
                if ps.first().map(|p| self.expand(p)).is_some_and(|l| l.trim().eq_ignore_ascii_case(label.trim())) {
                    self.pc = next;
                    return true;
                }
            }
        }
        self.messages.push(format!("ERROR: label not found: {label}"));
        false
    }

    /// Text typed at an {INPUT}/{KEYBOARD}/{CHAR}/{TEXT} pause.
    pub fn resume(&mut self, var: Option<&str>, text: &str) {
        match var {
            Some(v) => {
                self.vars.insert(v.to_string(), text.to_string());
            }
            None => self.out.extend(Document::from_text(text).els),
        }
    }

    pub fn stop(&mut self) {
        self.quit = true;
    }

    /// Run until the merge ends or needs keyboard input.
    pub fn run(&mut self) -> Step {
        const IFS: [&str; 4] = ["IF", "IF BLANK", "IF NOT BLANK", "IF EXISTS"];
        const FORS: [&str; 2] = ["FOR", "FOR EACH"];
        loop {
            if self.quit {
                return Step::Done;
            }
            self.steps += 1;
            if self.steps > 20_000_000 {
                self.messages.push("ERROR: merge stopped (endless loop?)".into());
                return Step::Done;
            }
            if self.pc >= self.code.len() {
                if let Some((c, p)) = self.calls.pop() {
                    self.code = c;
                    self.pc = p;
                    continue;
                }
                if let Some(c) = self.chain.take() {
                    self.primary = c;
                }
                let more = self.data.as_ref().is_some_and(|d| self.rec + 1 < d.records.len());
                if !more {
                    return Step::Done;
                }
                self.rec += 1;
                if !self.page_off {
                    self.out.push(El::Code(Code::HPg));
                }
                self.page_off = false;
                self.code = self.primary.clone();
                self.pc = 0;
                self.loops.clear();
                continue;
            }
            let el = self.code[self.pc].clone();
            let cmd = match el {
                El::Code(Code::Merge(n)) => n,
                El::Ch('«') => {
                    // Word-style «Field» placeholder
                    let mut name = String::new();
                    let mut j = self.pc + 1;
                    while j < self.code.len() && j < self.pc + 64 {
                        match self.code[j] {
                            El::Ch('»') => break,
                            El::Ch(c) => name.push(c),
                            _ => break,
                        }
                        j += 1;
                    }
                    if matches!(self.code.get(j), Some(El::Ch('»'))) {
                        if let Some(v) = self.field(&name) {
                            self.out.extend(v);
                            self.pc = j + 1;
                            continue;
                        }
                    }
                    self.out.push(El::Ch('«'));
                    self.pc += 1;
                    continue;
                }
                other => {
                    self.out.push(other);
                    self.pc += 1;
                    continue;
                }
            };
            let at = self.pc;
            let (f, l) = spec(&cmd);
            let code = self.code.clone();
            let (ps, next) = scan_params(&code, at + 1, f, l);
            self.pc = next.max(at + 1);
            let arg = |s: &Self, k: usize| ps.get(k).map(|p| s.expand(p)).unwrap_or_default();
            match cmd.as_str() {
                "FIELD" => {
                    let name = arg(self, 0);
                    let v = self.field(name.trim_end_matches('?')).unwrap_or_default();
                    let blank = v.iter().all(|e| matches!(e, El::Ch(' ') | El::Code(Code::HRt)));
                    if name.ends_with('?') && blank {
                        // empty field with `?`: no blank line
                        let line_start = self.out.iter().rposition(|e| matches!(e, El::Code(Code::HRt | Code::HPg))).map(|k| k + 1).unwrap_or(0);
                        let rest_blank = self.out[line_start..].iter().all(|e| matches!(e, El::Ch(' ') | El::Code(Code::Tab)));
                        if rest_blank && matches!(self.code.get(self.pc), Some(El::Code(Code::HRt))) {
                            self.out.truncate(line_start);
                            self.pc += 1;
                        }
                    } else {
                        self.out.extend(v);
                    }
                }
                "VARIABLE" | "DATE" | "LEN" | "MID" | "NTOC" | "CTON" | "SYSTEM" => {
                    if let Some(v) = self.value(&cmd, &ps) {
                        self.out.extend(v);
                    }
                }
                "ASSIGN" | "LOCAL" => {
                    let var = arg(self, 0).trim().to_string();
                    let e = arg(self, 1);
                    let has_op = lex(&e).iter().any(|t| matches!(t, Tok::Op(_)));
                    let v = if has_op { eval(&e).text() } else { e };
                    self.vars.insert(var, v);
                }
                "IF" | "IF BLANK" | "IF NOT BLANK" | "IF EXISTS" => {
                    let a = arg(self, 0);
                    let t = match cmd.as_str() {
                        "IF" => eval(&a).truthy(),
                        "IF EXISTS" => self.vars.contains_key(a.trim()),
                        _ => {
                            let blank = self.field(&a).is_none_or(|v| plain(&v).trim().is_empty());
                            (cmd == "IF BLANK") == blank
                        }
                    };
                    if !t {
                        self.skip(&IFS, "END IF", Some("ELSE"));
                    }
                }
                "ELSE" => {
                    self.skip(&IFS, "END IF", None);
                }
                "WHILE" => {
                    let t = eval(&arg(self, 0)).truthy();
                    let top_is_this = matches!(self.loops.last(), Some(Loop::While { pc }) if *pc == at);
                    if t {
                        if !top_is_this {
                            self.loops.push(Loop::While { pc: at });
                        }
                    } else {
                        if top_is_this {
                            self.loops.pop();
                        }
                        self.skip(&["WHILE"], "END WHILE", None);
                    }
                }
                "END WHILE" => {
                    if let Some(Loop::While { pc }) = self.loops.last() {
                        self.pc = *pc;
                    }
                }
                "FOR" => {
                    let var = arg(self, 0).trim().to_string();
                    let start = eval(&arg(self, 1)).num();
                    let stop = eval(&arg(self, 2)).num();
                    let step = match eval(&arg(self, 3)).num() {
                        0 => 1,
                        s => s,
                    };
                    self.vars.insert(var.clone(), start.to_string());
                    if (step > 0 && start <= stop) || (step < 0 && start >= stop) {
                        self.loops.push(Loop::For { var, stop, step, body: self.pc });
                    } else {
                        self.skip(&FORS, "END FOR", None);
                    }
                }
                "FOR EACH" => {
                    let var = arg(self, 0).trim().to_string();
                    let mut rest: VecDeque<String> = (1..ps.len()).map(|k| arg(self, k)).collect();
                    match rest.pop_front() {
                        Some(v) => {
                            self.vars.insert(var.clone(), v);
                            self.loops.push(Loop::Each { var, rest, body: self.pc });
                        }
                        None => {
                            self.skip(&FORS, "END FOR", None);
                        }
                    }
                }
                "END FOR" => match self.loops.last_mut() {
                    Some(Loop::For { var, stop, step, body }) => {
                        let v = self.vars.get(var.as_str()).map(|s| eval(s).num()).unwrap_or(0) + *step;
                        let (stop, step, body) = (*stop, *step, *body);
                        self.vars.insert(var.clone(), v.to_string());
                        if (step > 0 && v <= stop) || (step < 0 && v >= stop) {
                            self.pc = body;
                        } else {
                            self.loops.pop();
                        }
                    }
                    Some(Loop::Each { var, rest, body }) => match rest.pop_front() {
                        Some(v) => {
                            let (var, body) = (var.clone(), *body);
                            self.vars.insert(var, v);
                            self.pc = body;
                        }
                        None => {
                            self.loops.pop();
                        }
                    },
                    _ => {}
                },
                "BREAK" => match self.loops.pop() {
                    Some(Loop::While { .. }) => {
                        self.skip(&["WHILE"], "END WHILE", None);
                    }
                    Some(_) => {
                        self.skip(&FORS, "END FOR", None);
                    }
                    None => {}
                },
                "GO" => {
                    let l = arg(self, 0);
                    self.goto_label(&l);
                }
                "CALL" => {
                    let l = arg(self, 0);
                    self.calls.push((self.code.clone(), self.pc));
                    if !self.goto_label(&l) {
                        self.calls.pop();
                    }
                }
                "CASE" | "CASE CALL" => {
                    let v = eval(&arg(self, 0));
                    let mut k = 1;
                    while k + 1 < ps.len() {
                        let c = eval(&arg(self, k));
                        let hit = match (&v, &c) {
                            (Val::N(a), Val::N(b)) => a == b,
                            _ => v.text().trim().eq_ignore_ascii_case(c.text().trim()),
                        };
                        if hit {
                            let l = arg(self, k + 1);
                            if cmd == "CASE CALL" {
                                self.calls.push((self.code.clone(), self.pc));
                            }
                            self.goto_label(&l);
                            break;
                        }
                        k += 2;
                    }
                }
                "RETURN" | "RETURN CANCEL" | "RETURN ERROR" => {
                    if let Some((c, p)) = self.calls.pop() {
                        self.code = c;
                        self.pc = p;
                    }
                }
                "QUIT" => {
                    self.quit = true;
                }
                "NEXT RECORD" => {
                    let n = self.data.as_ref().map(|d| d.records.len()).unwrap_or(0);
                    if self.rec + 1 >= n {
                        self.quit = true;
                    } else {
                        self.rec += 1;
                    }
                }
                "PAGE OFF" => self.page_off = true,
                "PAGE ON" => self.page_off = false,
                "PROMPT" | "STATUS PROMPT" => {
                    let m = arg(self, 0);
                    self.messages.push(m);
                }
                "INPUT" => return Step::Input { prompt: arg(self, 0), var: None },
                "KEYBOARD" => return Step::Input { prompt: String::new(), var: None },
                "CHAR" | "TEXT" => return Step::Input { prompt: arg(self, 1), var: Some(arg(self, 0).trim().to_string()) },
                "LOOK" => {
                    let v = arg(self, 0).trim().to_string();
                    self.vars.insert(v, String::new());
                }
                "DOCUMENT" => {
                    if let Some(els) = self.load_els(&arg(self, 0)) {
                        self.out.extend(els);
                    }
                }
                "NEST PRIMARY" | "SUBST PRIMARY" | "CHAIN PRIMARY" => {
                    if let Some(els) = self.load_els(&arg(self, 0)) {
                        let r = Rc::new(convert_codes(&els));
                        match cmd.as_str() {
                            "NEST PRIMARY" => {
                                self.calls.push((self.code.clone(), self.pc));
                                self.code = r;
                                self.pc = 0;
                            }
                            "SUBST PRIMARY" => {
                                self.primary = r.clone();
                                self.code = r;
                                self.pc = 0;
                            }
                            _ => self.chain = Some(r),
                        }
                    }
                }
                "NEST SECONDARY" | "SUBST SECONDARY" | "CHAIN SECONDARY" => {
                    let a = arg(self, 0);
                    match load_data(&self.path(&a)) {
                        Ok(d) => {
                            self.data = Some(d);
                            self.rec = 0;
                        }
                        Err(e) => self.messages.push(format!("ERROR: {a} — {e}")),
                    }
                }
                // framing only / not meaningful here: END FIELD, END RECORD,
                // END IF, LABEL, COMMENT, BELL, PRINT, REWRITE, STEP…, CANCEL…,
                // WAIT, ON…, PROCESS, MRG CMND, macros, FIELD NAMES …
                _ => {}
            }
        }
    }
}

/// Merge without user interaction.
#[cfg(test)]
pub fn merge_all(primary: &[El], data: Option<Data>) -> Vec<El> {
    let mut e = Engine::new(convert_codes(primary), data, "", Path::new("."));
    while let Step::Input { var, .. } = e.run() {
        e.resume(var.as_deref(), "");
    }
    e.out
}

// ================================================================== UI

/// Ctrl-F9, 1 Merge: primary (empty = this document) + secondary (may be empty).
fn start_merge(wp: &mut Wp, prim: &str, sec: &str) -> Next {
    let (els, replace) = if prim.is_empty() {
        (wp.ed_ref().doc.els.clone(), true)
    } else {
        match crate::fileio::load(&wp.resolve_path(prim)) {
            Ok(d) => (d.els, false),
            Err(e) => {
                wp.flash(&format!("ERROR: File not found — {prim} ({e})"));
                return None;
            }
        }
    };
    let data = if sec.is_empty() {
        None
    } else {
        match load_data(&wp.resolve_path(sec)) {
            Ok(d) => Some(d),
            Err(e) => {
                wp.flash(&format!("ERROR: File not found — {sec} ({e})"));
                return None;
            }
        }
    };
    let date = crate::menus::format_date(&wp.date_format(), wp.german_dates());
    let base = wp.cwd.clone();
    let eng = Engine::new(convert_codes(&els), data, &date, &base);
    wp.flash("* Merging *");
    continue_merge(wp, eng, replace)
}

fn continue_merge(wp: &mut Wp, mut eng: Engine, replace: bool) -> Next {
    match eng.run() {
        Step::Done => {
            let mut out = std::mem::take(&mut eng.out);
            let mut d = Document::new();
            d.els = std::mem::take(&mut out);
            d.normalize_attrs();
            if replace {
                let e = wp.ed();
                e.set_els(d.els);
                e.cursor = 0;
            } else {
                wp.ed().insert_els(&d.els);
            }
            wp.message = eng.messages.last().cloned();
            wp.after_edit();
            None
        }
        Step::Input { prompt, var } => {
            let label = if prompt.is_empty() { "Merge input (Enter to continue):".to_string() } else { prompt };
            Some(Prompt::new(&label, "", move |wp, s| {
                match s {
                    Some(s) => eng.resume(var.as_deref(), &s),
                    None => eng.stop(),
                }
                continue_merge(wp, eng, replace)
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn els(parts: &[&str]) -> Vec<El> {
        // "{X}" = merge code X, "\n" = HRt, "\x0c" = HPg
        let mut v = vec![];
        for p in parts {
            if let Some(n) = p.strip_prefix('{').and_then(|x| x.strip_suffix('}')) {
                v.push(El::Code(Code::Merge(n.into())));
            } else {
                v.extend(Document::from_text(p).els);
            }
        }
        v
    }

    fn text(v: &[El]) -> String {
        let mut d = Document::new();
        d.els = v.to_vec();
        d.plain_text(0, d.len())
    }

    #[test]
    fn three_records() {
        let sec = els(&[
            "{FIELD NAMES}", "Name~Ort~~", "{END RECORD}", "\x0c",
            "Müller", "{END FIELD}", "\n", "Berlin", "{END FIELD}", "\n", "{END RECORD}", "\x0c",
            "Schmidt", "{END FIELD}", "\n", "{END FIELD}", "\n", "{END RECORD}", "\x0c",
            "Özdemir", "{END FIELD}", "\n", "Köln", "{END FIELD}", "\n", "{END RECORD}", "\x0c",
        ]);
        let d = parse_secondary(&sec);
        assert_eq!(d.names, vec!["Name", "Ort"]);
        assert_eq!(d.records.len(), 3);
        let prim = els(&["Liebe/r ", "{FIELD}", "Name~,\n", "{FIELD}", "Ort?~", "\n", "Gruß"]);
        let out = merge_all(&prim, Some(d));
        assert_eq!(text(&out), "Liebe/r Müller,\nBerlin\nGruß\u{c}Liebe/r Schmidt,\nGruß\u{c}Liebe/r Özdemir,\nKöln\nGruß");
    }

    #[test]
    fn page_off_if_vars() {
        let d = Data::from_rows(parse_csv("Name;Betrag\nA;5\nB;50\n"));
        let prim = els(&[
            "{PAGE OFF}", "{FIELD}", "Name~: ", "{IF}", "{FIELD}", "Betrag~>10~", "groß", "{ELSE}", "klein", "{END IF}",
            "{ASSIGN}", "n~", "{VARIABLE}", "n~+1~", "\n",
        ]);
        let out = merge_all(&prim, Some(d));
        assert_eq!(text(&out), "A: klein\nB: groß\n");
        // loops
        let prim = els(&["{FOR}", "i~1~3~1~", "{VARIABLE}", "i~", "{END FOR}", "{FOR EACH}", "x~a~b~~", "{VARIABLE}", "x~", "{END FOR}"]);
        assert_eq!(text(&merge_all(&prim, None)), "123ab");
        // Word-style «Field» and csv quoting
        let d = Data::from_rows(parse_csv("Vorname,Stadt\n\"Anna, Maria\",Wien\n"));
        let prim = els(&["«Vorname» aus «Stadt»"]);
        assert_eq!(text(&merge_all(&prim, Some(d))), "Anna, Maria aus Wien");
    }

    #[test]
    fn codes_roundtrip_and_docx() {
        let mut d = Document::new();
        let mut def = MathCol::defaults();
        def[2] = MathCol { kind: 0, paren: true, digits: 1, formula: "A+B".into() };
        d.els = els(&["Hallo ", "{FIELD}", "Name~", "\n"]);
        d.els.extend([
            El::Code(Code::MathDef(def)),
            El::Code(Code::Math(true)),
            El::Code(Code::MathOp('+')),
            El::Code(Code::Math(false)),
            El::Code(Code::Formula("A1+B1".into())),
            El::Code(Code::SsLink { file: "C:\\x|y.xlsx".into(), range: "A1:B2".into(), table: true }),
            El::Code(Code::SsLinkEnd),
        ]);
        let s = crate::stream::write(&d);
        assert_eq!(crate::stream::read(&s).unwrap().els, d.els);
        // Word sees plain-text placeholders, which convert back
        let x = crate::docx::document_xml(&d, &mut Default::default());
        assert!(x.contains("{FIELD}"), "{x}");
        let back = crate::docx::parse_document_xml(&x, None);
        let conv = convert_codes(&back.els);
        assert_eq!(conv[..7], d.els[..7]);
    }

    #[test]
    fn convert_placeholders() {
        let v = convert_codes(&Document::from_text("{FIELD}Name~ {END FIELD}{foo}").els);
        assert_eq!(v[0], El::Code(Code::Merge("FIELD".into())));
        assert!(v.contains(&El::Code(Code::Merge("END FIELD".into()))));
        assert!(text(&v).contains("{foo}"));
        assert_eq!(eval("3+4*2"), Val::N(11));
        assert!(eval("\"Herr\"=\"herr\"").truthy());
    }
}
