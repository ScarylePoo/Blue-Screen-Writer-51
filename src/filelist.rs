//! F5 List Files — WP 5.1 layout, but with long file names, spaces,
//! mapped drives and UNC paths (\\server\share\folder) working natively.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// Clean up `a\b\..\c` without touching the file system (canonicalize
/// would turn UNC paths into \\?\UNC\… which users should never see).
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                let has_normal = out.components().next_back().is_some_and(|c| matches!(c, Component::Normal(_)));
                if has_normal {
                    out.pop();
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Split "C:\docs\*.txt" into (dir, pattern).
pub fn split_pattern(p: &Path) -> (PathBuf, String) {
    let s = p.to_string_lossy();
    if s.contains('*') || s.contains('?') {
        let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let pat = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| "*.*".into());
        (dir, pat)
    } else {
        (p.to_path_buf(), "*.*".into())
    }
}

/// DOS-style wildcard match, case-insensitive; "*.*" matches everything.
pub fn wild_match(pat: &str, name: &str) -> bool {
    if pat == "*.*" || pat == "*" {
        return true;
    }
    fn m(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a.to_lowercase().eq(b.to_lowercase()) && m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pat.chars().collect();
    let n: Vec<char> = name.chars().collect();
    // "*." means "no extension"
    if pat.ends_with(".*") && !name.contains('.') {
        let p2: Vec<char> = pat[..pat.len() - 2].chars().collect();
        return m(&p2, &n);
    }
    m(&p, &n)
}

#[derive(Clone)]
struct Entry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
    special: bool, // "." / ".."
}

pub fn fmt_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut o = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            o.push(',');
        }
        o.push(c);
    }
    o
}

/// (date "mm-dd-yy", time "hh:mma") in local time.
pub fn fmt_time(t: SystemTime) -> (String, String) {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let secs = secs + crate::sys::utc_offset_at(secs);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let (h12, ap) = if h == 0 { (12, 'a') } else if h < 12 { (h, 'a') } else if h == 12 { (12, 'p') } else { (h - 12, 'p') };
    (format!("{:02}-{:02}-{:02}", m, d, y % 100), format!("{:02}:{:02}{}", h12, mi, ap))
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub struct FileList {
    dir: PathBuf,
    pattern: String,
    entries: Vec<Entry>,
    sel: usize,
    top: usize,
    marked: Vec<bool>,
    long: bool,
    name_search: Option<String>,
    error: Option<String>,
}

pub fn open_dir_prompt(wp: &mut Wp) {
    wp.push(Box::new(DirPrompt { text: format!("{}", wp.cwd.join("*.*").display()).chars().collect(), first: true, change_default: false }));
}

/// "Dir C:\WP51\*.*" — typing '=' first switches to "New directory =".
struct DirPrompt {
    text: Vec<char>,
    first: bool,
    change_default: bool,
}

impl Screen for DirPrompt {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let first = std::mem::replace(&mut self.first, false);
        match i {
            Input::Char('=') if first => {
                self.change_default = true;
                self.text = format!("{}", wp.cwd.display()).chars().collect();
                self.first = true;
            }
            Input::Char(c) => {
                if first {
                    self.text.clear();
                }
                self.text.push(*c);
            }
            Input::Func(Func::Backspace) => {
                self.text.pop();
            }
            Input::Func(Func::DelEol) => self.text.clear(),
            Input::Func(Func::Cancel) | Input::Func(Func::Esc) | Input::Func(Func::Exit) => return Outcome::Close,
            Input::Func(Func::Enter) | Input::Func(Func::List) => {
                let s: String = self.text.iter().collect();
                let p = normalize(&wp.resolve_path(&s));
                if self.change_default {
                    if p.is_dir() {
                        wp.cwd = p.clone();
                        // WP then shows "Dir <newdir>\*.*"
                        self.change_default = false;
                        self.text = format!("{}", p.join("*.*").display()).chars().collect();
                        self.first = true;
                        return Outcome::Keep;
                    } else {
                        wp.flash(&format!("ERROR: Invalid drive/path specification"));
                        return Outcome::Close;
                    }
                }
                let (dir, pat) = split_pattern(&p);
                return Outcome::Replace(Box::new(FileList::open(dir, pat)));
            }
            _ => {}
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        let st = wp.theme.normal;
        g.fill_row(row, 0, g.cols, ' ', st);
        let label = if self.change_default { "New directory = " } else { "Dir " };
        let c = g.text(0, row, label, st);
        let s: String = self.text.iter().collect();
        let avail = g.cols - c - 1;
        let shown: String = if s.chars().count() > avail { s.chars().skip(s.chars().count() - avail).collect() } else { s.clone() };
        let end = g.text(c, row, &shown, st);
        if self.first && !self.change_default {
            let hint = "(Type = to change default Dir)";
            if end + 3 + hint.len() < g.cols {
                g.text(g.cols - hint.len() - 1, row, hint, st);
            }
        }
        g.cursor = Some((end.min(g.cols - 1), row));
    }
}

impl FileList {
    pub fn open(dir: PathBuf, pattern: String) -> FileList {
        let mut f = FileList {
            dir,
            pattern,
            entries: vec![],
            sel: 0,
            top: 0,
            marked: vec![],
            long: true,
            name_search: None,
            error: None,
        };
        f.reload();
        f
    }

    fn reload(&mut self) {
        let mut entries = vec![
            Entry { name: ".".into(), is_dir: true, size: 0, modified: None, special: true },
            Entry { name: "..".into(), is_dir: true, size: 0, modified: None, special: true },
        ];
        let mut dirs = vec![];
        let mut files = vec![];
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for de in rd.flatten() {
                    let name = de.file_name().to_string_lossy().into_owned();
                    let md = de.metadata().ok();
                    let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    if crate::sys::is_hidden(&de) {
                        continue;
                    }
                    let e = Entry {
                        size: md.as_ref().map(|m| m.len()).unwrap_or(0),
                        modified: md.as_ref().and_then(|m| m.modified().ok()),
                        is_dir,
                        name,
                        special: false,
                    };
                    if is_dir {
                        dirs.push(e);
                    } else if wild_match(&self.pattern, &e.name) {
                        files.push(e);
                    }
                }
                self.error = None;
            }
            Err(e) => self.error = Some(format!("ERROR: {e}")),
        }
        let key = |e: &Entry| e.name.to_lowercase();
        dirs.sort_by_key(key);
        files.sort_by_key(key);
        entries.extend(dirs);
        entries.extend(files);
        self.marked = vec![false; entries.len()];
        self.entries = entries;
        if self.sel >= self.entries.len() {
            self.sel = self.entries.len().saturating_sub(1);
        }
    }

    fn cur(&self) -> &Entry {
        &self.entries[self.sel]
    }

    fn cur_path(&self) -> PathBuf {
        let e = self.cur();
        match e.name.as_str() {
            "." => self.dir.clone(),
            ".." => normalize(&self.dir.join("..")),
            n => self.dir.join(n),
        }
    }

    fn list_rows(&self, g_rows: usize) -> usize {
        g_rows.saturating_sub(5)
    }

    fn cols(&self) -> usize {
        if self.long { 1 } else { 2 }
    }

    fn marked_paths(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .zip(&self.marked)
            .filter(|(e, m)| **m && !e.special)
            .map(|(e, _)| self.dir.join(&e.name))
            .collect()
    }
}

impl Screen for FileList {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::List
    }
    fn full_screen(&self) -> bool {
        true
    }

    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        if n >= self.entries.len() {
            return Outcome::Keep;
        }
        self.sel = n;
        if !double {
            return Outcome::Keep;
        }
        // double-click: open a folder, retrieve a file
        let e = self.cur().clone();
        let p = self.cur_path();
        if e.is_dir {
            let pat = self.pattern.clone();
            return Outcome::Replace(Box::new(FileList::open(p, pat)));
        }
        wp.retrieve(&p);
        Outcome::Close
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let rows = wp.rows;
        let page = self.list_rows(rows) * self.cols();
        let n = self.entries.len();
        if let Some(ns) = &mut self.name_search {
            match i {
                Input::Char(c) => {
                    ns.push(*c);
                    let pref = ns.to_lowercase();
                    if let Some(k) = self.entries.iter().position(|e| !e.special && e.name.to_lowercase().starts_with(&pref)) {
                        self.sel = k;
                    }
                    return Outcome::Keep;
                }
                Input::Func(Func::Backspace) => {
                    ns.pop();
                    return Outcome::Keep;
                }
                _ => {
                    self.name_search = None;
                    if matches!(i, Input::Func(Func::Enter) | Input::Func(Func::Cancel) | Input::Func(Func::Exit)) {
                        return Outcome::Keep;
                    }
                }
            }
        }
        match i {
            Input::Func(Func::Down) => self.sel = (self.sel + self.cols()).min(n - 1),
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(self.cols()),
            Input::Func(Func::Right) => self.sel = (self.sel + 1).min(n - 1),
            Input::Func(Func::Left) => self.sel = self.sel.saturating_sub(1),
            Input::Func(Func::PgDn) | Input::Func(Func::ScreenDown) => self.sel = (self.sel + page).min(n - 1),
            Input::Func(Func::PgUp) | Input::Func(Func::ScreenUp) => self.sel = self.sel.saturating_sub(page),
            Input::Func(Func::Home) => {}
            Input::Func(Func::End) => self.sel = n - 1,
            Input::Func(Func::GoTo) => self.sel = 0,
            Input::Func(Func::Cancel) | Input::Func(Func::Exit) | Input::Char(' ') | Input::Char('0') => return Outcome::Close,
            Input::Func(Func::SearchFwd) | Input::Char('n') | Input::Char('N') => {
                self.name_search = Some(String::new());
            }
            Input::Char('*') => {
                if !self.cur().special {
                    self.marked[self.sel] = !self.marked[self.sel];
                }
                self.sel = (self.sel + 1).min(n - 1);
            }
            Input::Func(Func::DelEol) => {
                // Alt-F5 / Home,* in WP: (un)mark all — we use Ctrl-End? keep simple
            }
            Input::Func(Func::MarkText) => {
                let any = self.marked.iter().any(|m| *m);
                for (k, e) in self.entries.iter().enumerate() {
                    self.marked[k] = !any && !e.special && !e.is_dir;
                }
            }
            Input::Char('5') | Input::Char('s') | Input::Char('S') => {
                self.long = !self.long;
            }
            Input::Char('1') | Input::Char('r') | Input::Char('R') => {
                let e = self.cur().clone();
                if e.is_dir {
                    return Outcome::Keep;
                }
                let p = self.cur_path();
                wp.retrieve(&p);
                return Outcome::Close;
            }
            Input::Func(Func::Enter) | Input::Char('6') | Input::Char('l') | Input::Char('L') => {
                let e = self.cur().clone();
                let p = self.cur_path();
                if e.is_dir {
                    let pat = self.pattern.clone();
                    return Outcome::Replace(Box::new(DirPrompt {
                        text: format!("{}", p.join(&pat).display()).chars().collect(),
                        first: true,
                        change_default: false,
                    }));
                }
                return Outcome::Push(Box::new(Look::open(&p)));
            }
            Input::Char('7') | Input::Char('o') | Input::Char('O') => {
                let start = self.cur_path();
                let start = if self.cur().is_dir { start } else { self.dir.clone() };
                return Outcome::Push(Prompt::new("New directory =", &format!("{}", start.display()), |wp, s| {
                    let s = s?;
                    let p = normalize(&wp.resolve_path(&s));
                    if p.is_dir() {
                        wp.cwd = p;
                    } else {
                        wp.flash("ERROR: Invalid drive/path specification");
                    }
                    None
                }).keep_text());
            }
            Input::Char('2') | Input::Char('d') | Input::Char('D') => {
                let marked = self.marked_paths();
                let targets = if marked.is_empty() { vec![self.cur_path()] } else { marked };
                if self.cur().special && targets.len() == 1 && self.marked_paths().is_empty() {
                    return Outcome::Keep;
                }
                let q = if targets.len() == 1 {
                    format!("Delete {}?", targets[0].display())
                } else {
                    "Delete marked files?".to_string()
                };
                let dir = self.dir.clone();
                let pat = self.pattern.clone();
                let sel = self.sel;
                return Outcome::Replace(YesNo::new(&q, false, move |wp, yes| {
                    if yes {
                        for t in &targets {
                            let r = if t.is_dir() { std::fs::remove_dir(t) } else { std::fs::remove_file(t) };
                            if let Err(e) = r {
                                wp.flash(&format!("ERROR: {e}"));
                            }
                        }
                    }
                    let mut f = FileList::open(dir, pat);
                    f.sel = sel.min(f.entries.len().saturating_sub(1));
                    Some(Box::new(f))
                }));
            }
            Input::Char('3') | Input::Char('m') | Input::Char('M') => {
                if self.cur().special {
                    return Outcome::Keep;
                }
                let src = self.cur_path();
                let dir = self.dir.clone();
                let pat = self.pattern.clone();
                return Outcome::Replace(
                    Prompt::new("New name:", &format!("{}", src.display()), move |wp, s| {
                        if let Some(s) = s {
                            let dst = wp.resolve_path(&s);
                            let dst = if dst.is_dir() { dst.join(src.file_name().unwrap()) } else { dst };
                            if let Err(e) = std::fs::rename(&src, &dst).or_else(|_| {
                                std::fs::copy(&src, &dst).and_then(|_| std::fs::remove_file(&src))
                            }) {
                                wp.flash(&format!("ERROR: {e}"));
                            }
                        }
                        Some(Box::new(FileList::open(dir, pat)))
                    })
                    .keep_text(),
                );
            }
            Input::Char('8') | Input::Char('c') | Input::Char('C') => {
                let marked = self.marked_paths();
                let srcs = if marked.is_empty() {
                    if self.cur().special || self.cur().is_dir {
                        return Outcome::Keep;
                    }
                    vec![self.cur_path()]
                } else {
                    marked
                };
                let label = if srcs.len() == 1 { "Copy this file to:" } else { "Copy marked files to:" };
                let dir = self.dir.clone();
                let pat = self.pattern.clone();
                return Outcome::Replace(Prompt::new(label, "", move |wp, s| {
                    if let Some(s) = s {
                        let dst = wp.resolve_path(&s);
                        for src in &srcs {
                            let d = if dst.is_dir() { dst.join(src.file_name().unwrap()) } else { dst.clone() };
                            if let Err(e) = std::fs::copy(src, &d) {
                                wp.flash(&format!("ERROR: {e}"));
                            }
                        }
                    }
                    Some(Box::new(FileList::open(dir, pat)))
                }));
            }
            Input::Char('9') | Input::Char('f') | Input::Char('F') => {
                let dir = self.dir.clone();
                let pat = self.pattern.clone();
                return Outcome::Replace(Prompt::new("Word pattern:", "", move |_wp, s| {
                    let mut f = FileList::open(dir, pat);
                    if let Some(word) = s.filter(|s| !s.trim().is_empty()) {
                        let w = word.to_lowercase();
                        let keep: Vec<Entry> = f
                            .entries
                            .iter()
                            .filter(|e| {
                                e.special || e.is_dir || {
                                    let p = f.dir.join(&e.name);
                                    crate::fileio::load(&p)
                                        .map(|d| d.plain_text(0, d.len()).to_lowercase().contains(&w))
                                        .unwrap_or(false)
                                }
                            })
                            .cloned()
                            .collect();
                        f.marked = vec![false; keep.len()];
                        f.entries = keep;
                        f.sel = 0;
                    }
                    Some(Box::new(f))
                }));
            }
            Input::Char('4') | Input::Char('p') | Input::Char('P') => {
                let p = self.cur_path();
                if !self.cur().is_dir {
                    crate::menus::print_file(wp, &p);
                }
            }
            _ => {}
        }
        // keep selection visible
        let vis = self.list_rows(rows);
        let row = self.sel / self.cols();
        if row < self.top {
            self.top = row;
        } else if row >= self.top + vis {
            self.top = row + 1 - vis;
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let th = &wp.theme;
        let st = th.normal;
        g.clear(st);
        let now = fmt_time(SystemTime::now());
        let head = format!("{}", self.dir.join(&self.pattern).display());
        g.text(1, 0, &format!("{} {}", now.0, now.1), st);
        let dtitle = format!("Directory {head}");
        let x = if dtitle.chars().count() + 20 < g.cols { 20 } else { 0 };
        let dshow: String = dtitle.chars().take(g.cols - x).collect();
        g.text(x, 0, &dshow, st);
        let docsize = wp.ed_ref().doc.plain_text(0, wp.ed_ref().doc.len()).len();
        let (free, _total) = crate::sys::disk_space(&self.dir).unwrap_or((0, 0));
        let used: u64 = self.entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
        let files = self.entries.iter().filter(|e| !e.is_dir).count();
        g.text(
            0,
            1,
            &format!(
                "Document size:{:>8}  Free:{:>16}  Used:{:>12}  Files:{:>5}",
                fmt_thousands(docsize as u64),
                fmt_thousands(free),
                fmt_thousands(used),
                files
            ),
            st,
        );
        g.fill_row(2, 0, g.cols, '─', st);
        let vis = self.list_rows(g.rows);
        if let Some(err) = &self.error {
            g.text(1, 3, err, st);
        }
        let colw = g.cols / self.cols();
        for r in 0..vis {
            for c in 0..self.cols() {
                let k = (self.top + r) * self.cols() + c;
                let Some(e) = self.entries.get(k) else { continue };
                let x = c * colw;
                let y = 3 + r;
                let mark = if self.marked[k] { '*' } else { ' ' };
                let text = if self.long {
                    let size = if e.is_dir { "<Dir>".to_string() } else { fmt_thousands(e.size) };
                    let (d, t) = e.modified.map(fmt_time).unwrap_or_default();
                    let label = match e.name.as_str() {
                        "." => ".  Current".to_string(),
                        ".." => ".. Parent".to_string(),
                        n => n.to_string(),
                    };
                    let namew = g.cols.saturating_sub(36);
                    let mut name: String = label.chars().take(namew).collect();
                    if label.chars().count() > namew {
                        name.pop();
                        name.push('…');
                    }
                    format!("{mark}{name:<namew$} {size:>13}  {d} {t}")
                } else {
                    let size = if e.is_dir { "<Dir>".to_string() } else { fmt_thousands(e.size) };
                    let (d, t) = e.modified.map(fmt_time).unwrap_or_default();
                    let label = match e.name.as_str() {
                        "." => ".  Current".to_string(),
                        ".." => ".. Parent".to_string(),
                        n => n.to_string(),
                    };
                    let mut name: String = label.chars().take(12).collect();
                    if label.chars().count() > 12 {
                        name.pop();
                        name.push('…');
                    }
                    format!("{mark}{name:<12}{size:>10} {d} {}", &t[..t.len().min(6)])
                };
                let sty = if k == self.sel { th.block } else { st };
                let shown: String = text.chars().take(colw.saturating_sub(1)).collect();
                g.text(x, y, &shown, sty);
                wp.hit(y, x, x + colw.saturating_sub(1), crate::ui::HitAction::Select(k));
                if self.cols() == 2 && c == 0 {
                    g.put(colw - 1, y, '│', st);
                }
            }
        }
        // bottom menu
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', st);
        if let Some(ns) = &self.name_search {
            g.text(0, row, &format!("(Name Search; Enter or arrows to Exit) {ns}"), st);
        } else {
            let items = "1 ~Retrieve; 2 ~Delete; 3 ~Move/Rename; 4 ~Print; 5 ~Short/Long Display;";
            g.text_mn(0, row - 1, items, st, th.bold);
            let c = g.text_mn(0, row, "6 ~Look; 7 ~Other Directory; 8 ~Copy; 9 ~Find; ~N Name Search: ", st, th.bold);
            // clickable menu items
            for (r, line) in [(row - 1, items), (row, "6 ~Look; 7 ~Other Directory; 8 ~Copy; 9 ~Find; ~N Name Search")] {
                let plain: String = line.chars().filter(|&c| c != '~').collect();
                let mut x = 0;
                for part in plain.split("; ") {
                    let key = part.chars().next().unwrap_or(' ');
                    let w = part.chars().count();
                    wp.hit(r, x, x + w, crate::ui::HitAction::Input(Input::Char(key)));
                    x += w + 2;
                }
            }
            g.text(c, row, "6", st);
            g.cursor = Some((c, row));
        }
    }
}

/// "6 Look" — read-only view of a file.
pub struct Look {
    title: String,
    lines: Vec<String>,
    top: usize,
}

impl Look {
    pub fn open(p: &Path) -> Look {
        let text = match crate::fileio::load(p) {
            Ok(d) => d.plain_text(0, d.len()),
            Err(e) => format!("ERROR: {e}"),
        };
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let chars: Vec<char> = para.chars().collect();
            if chars.is_empty() {
                lines.push(String::new());
            }
            for ch in chars.chunks(78) {
                lines.push(ch.iter().collect());
            }
        }
        Look { title: format!("File: {}", p.display()), lines, top: 0 }
    }
}

impl Screen for Look {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let h = wp.rows.saturating_sub(3);
        match i {
            Input::Func(Func::Down) => self.top = (self.top + 1).min(self.lines.len().saturating_sub(1)),
            Input::Func(Func::Up) => self.top = self.top.saturating_sub(1),
            Input::Func(Func::PgDn) | Input::Func(Func::ScreenDown) => {
                self.top = (self.top + h).min(self.lines.len().saturating_sub(1))
            }
            Input::Func(Func::PgUp) | Input::Func(Func::ScreenUp) => self.top = self.top.saturating_sub(h),
            Input::Func(Func::Home) => {}
            _ => return Outcome::Close,
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, &self.title, st);
        g.fill_row(1, 0, g.cols, '─', st);
        for (r, l) in self.lines.iter().skip(self.top).take(g.rows - 3).enumerate() {
            g.text(1, 2 + r, l, st);
        }
        g.text(0, g.rows - 1, "Look: 1 Next; 2 Previous: 0", st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wild() {
        assert!(wild_match("*.*", "Mein Brief.docx"));
        assert!(wild_match("*.docx", "Mein Brief.DOCX"));
        assert!(!wild_match("*.txt", "a.docx"));
        assert!(wild_match("br?ef*", "Brief 2.txt"));
    }
    #[test]
    fn norm_unc() {
        let p = normalize(Path::new(r"\\server\share\a\..\b"));
        assert_eq!(p.to_string_lossy().replace('/', "\\"), r"\\server\share\b");
    }
    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(fmt_thousands(1234567), "1,234,567");
    }
}
