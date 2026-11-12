//! Alt-F8 Styles (list and edit screens) and Shift-F5 Outline / Para Num /
//! Define (Paragraph Number Definition), plus the editing behaviour of
//! styles and outline mode (Enter, Tab, Shift-Tab).

use crate::doc::*;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::screens::*;
use crate::styles::*;
use crate::ui::{HitAction, Outcome, Screen, Wp};
use std::path::PathBuf;

// ============================================================ editing

/// 1 On: a paired style goes around the block (or `[Style On][Style Off]`
/// with the cursor between); an open style is one code.
pub fn style_on(wp: &mut Wp, name: &str) {
    let kind = find_style(&wp.ed_ref().doc, name).map(|s| s.kind).unwrap_or_default();
    let e = wp.ed();
    if kind == StyleKind::Open {
        e.block_anchor = None;
        e.insert_code(Code::OpenStyle(name.into()));
    } else if let Some((s, t)) = e.block_range() {
        e.block_anchor = None;
        e.cursor = t;
        e.insert_code(Code::StyleOff(name.into()));
        e.cursor = s;
        e.insert_code(Code::StyleOn(name.into()));
        e.cursor = t + 2;
    } else {
        e.insert_code(Code::StyleOn(name.into()));
        e.insert_code(Code::StyleOff(name.into()));
        e.cursor -= 1;
    }
    wp.after_edit();
}

/// 2 Off: move past the `[Style Off]` of the style the cursor is in.
pub fn style_off(wp: &mut Wp) {
    let e = wp.ed();
    if let Some((_, Some(off))) = open_style_at(&e.doc, e.cursor) {
        e.cursor = off + 1;
    }
    wp.after_edit();
}

fn para_start(els: &[El], i: usize) -> usize {
    els[..i.min(els.len())].iter().rposition(|e| matches!(e, El::Code(Code::HRt | Code::HPg))).map(|p| p + 1).unwrap_or(0)
}

/// Enter: the style's Enter action, outline mode, else not handled.
pub fn enter(wp: &mut Wp) -> bool {
    let e = wp.ed();
    if e.block_anchor.is_some() {
        return false;
    }
    if let Some((name, Some(off))) = open_style_at(&e.doc, e.cursor) {
        let action = find_style(&e.doc, &name).map(|s| s.enter).unwrap_or_default();
        match action {
            EnterAction::Off => {
                e.cursor = off + 1;
                return true;
            }
            EnterAction::OffOn => {
                e.cursor = off + 1;
                e.insert_code(Code::StyleOn(name.clone()));
                e.insert_code(Code::StyleOff(name));
                e.cursor -= 1;
                return true;
            }
            EnterAction::HRt => {}
        }
    }
    if outline_mode(&e.doc, e.cursor) && def_at(&e.doc, e.cursor).enter_inserts {
        let ps = para_start(&e.doc.els, e.cursor);
        let only_number = e.cursor > ps
            && e.doc.els[ps..e.cursor].iter().all(|x| matches!(x, El::Code(Code::Tab | Code::ParNum(_) | Code::Indent)) || !matches!(x, El::Ch(_)) && crate::editor::is_invisible(x))
            && e.doc.els[ps..e.cursor].iter().any(|x| matches!(x, El::Code(Code::ParNum(_))));
        if only_number {
            // an empty numbered line: the number moves down
            let c = e.cursor;
            e.cursor = ps;
            e.insert_code(Code::HRt);
            e.cursor = c + 1;
        } else {
            e.insert_code(Code::HRt);
            e.insert_code(Code::ParNum(None));
        }
        return true;
    }
    false
}

/// Tab / Shift-Tab right after a paragraph number in outline mode: change
/// its level.
pub fn tab(wp: &mut Wp, forward: bool) -> bool {
    let e = wp.ed();
    if e.block_anchor.is_some() || e.cursor == 0 || !outline_mode(&e.doc, e.cursor) {
        return false;
    }
    let mut k = e.cursor;
    // (an [Indent] after the number may already be there)
    while k > 0 && matches!(e.doc.els[k - 1], El::Code(Code::Indent)) {
        k -= 1;
    }
    if k == 0 || !matches!(e.doc.els[k - 1], El::Code(Code::ParNum(None))) {
        return false;
    }
    let num = k - 1;
    let ps = para_start(&e.doc.els, num);
    if forward {
        if crate::styles::auto_level(&e.doc.els, num) >= 8 {
            return true;
        }
        let c = e.cursor;
        e.cursor = num;
        e.insert_code(Code::Tab);
        e.cursor = c + 1;
    } else if let Some(t) = (ps..num).rev().find(|&i| matches!(e.doc.els[i], El::Code(Code::Tab))) {
        let c = e.cursor;
        e.delete_range(t, t + 1, false);
        e.cursor = c - 1;
    }
    true
}

// ======================================================= Alt-F8 Styles

/// Setup → Location of Files → Style Files: the default style library.
pub fn default_library(wp: &Wp) -> Option<PathBuf> {
    if let Some(l) = &wp.settings.style_library {
        if l.is_absolute() || wp.settings.style_dir.is_none() {
            return Some(l.clone());
        }
        return wp.settings.style_dir.as_ref().map(|d| d.join(l));
    }
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("STYLE").join("LIBRARY.STY");
    p.is_file().then_some(p)
}

pub fn open_styles(wp: &mut Wp) {
    // WP retrieves the default library into a document without styles
    if wp.ed_ref().doc.styles.is_empty() {
        if let Some(p) = default_library(wp) {
            if let Ok(lib) = load_library(&p) {
                merge_styles(&mut wp.ed().doc, lib, false);
            }
        }
    }
    wp.push(Box::new(StylesList { sel: 0, search: None }));
}

struct StylesList {
    sel: usize,
    search: Option<String>,
}

const MENU: [&str; 8] = ["~On", "O~ff", "~Create", "~Edit", "~Delete", "~Save", "~Retrieve", "~Update"];

fn styles_list(sel: usize) -> Box<dyn Screen> {
    Box::new(StylesList { sel, search: None })
}

fn library_prompt(wp: &Wp) -> String {
    default_library(wp).map(|p| p.display().to_string()).unwrap_or_default()
}

impl StylesList {
    fn count(wp: &Wp) -> usize {
        wp.ed_ref().doc.styles.len()
    }
    fn name(&self, wp: &Wp) -> Option<String> {
        wp.ed_ref().doc.styles.get(self.sel).map(|s| s.name.clone())
    }

    fn act(&mut self, wp: &mut Wp, k: usize) -> Outcome {
        let sel = self.sel;
        match k {
            0 => {
                if let Some(n) = self.name(wp) {
                    style_on(wp, &n);
                }
                Outcome::CloseAll
            }
            1 => {
                style_off(wp);
                Outcome::CloseAll
            }
            2 => {
                let e = wp.ed();
                e.doc.styles.push(StyleDef::default());
                e.modified = true;
                let idx = e.doc.styles.len() - 1;
                self.sel = idx;
                Outcome::Push(Box::new(StyleEdit { idx, editing: None }))
            }
            3 => {
                if self.sel < Self::count(wp) {
                    Outcome::Push(Box::new(StyleEdit { idx: self.sel, editing: None }))
                } else {
                    Outcome::Keep
                }
            }
            4 => {
                let Some(name) = self.name(wp) else { return Outcome::Keep };
                let m = MenuLine::new(&["~Leaving Codes", "~Including Codes", "~Definition Only"], move |wp, k| {
                    delete_style(wp, &name, k);
                    Some(styles_list(sel.saturating_sub(1)))
                })
                .prefix("Delete Styles: ");
                Outcome::Replace(m)
            }
            5 => {
                let dflt = library_prompt(wp);
                Outcome::Replace(Prompt::new("Filename:", &dflt, move |wp, s| {
                    if let Some(s) = s.filter(|s| !s.trim().is_empty()) {
                        let p = wp.resolve_path(s.trim());
                        if let Err(e) = save_library(&p, &wp.ed_ref().doc.styles) {
                            wp.flash(&format!("ERROR: {e}"));
                        }
                    }
                    Some(styles_list(sel))
                }))
            }
            6 => Outcome::Replace(Prompt::new("Filename:", "", move |wp, s| {
                if let Some(s) = s.filter(|s| !s.trim().is_empty()) {
                    let p = wp.resolve_path(s.trim());
                    return retrieve(wp, &p, sel);
                }
                Some(styles_list(sel))
            })),
            _ => {
                match default_library(wp) {
                    Some(p) => match load_library(&p) {
                        Ok(lib) => {
                            let e = wp.ed();
                            merge_styles(&mut e.doc, lib, true);
                            e.modified = true;
                            e.relayout();
                        }
                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                    },
                    None => wp.flash("ERROR: No style library set (Setup, Location of Files)"),
                }
                Outcome::Keep
            }
        }
    }
}

fn retrieve(wp: &mut Wp, p: &std::path::Path, sel: usize) -> Next {
    match load_library(p) {
        Ok(lib) => {
            let clash = lib.iter().any(|s| wp.ed_ref().doc.styles.iter().any(|x| x.name == s.name));
            if clash {
                return Some(YesNo::new("Replace Existing Styles?", false, move |wp, yes| {
                    let e = wp.ed();
                    merge_styles(&mut e.doc, lib, yes);
                    e.modified = true;
                    e.relayout();
                    Some(styles_list(sel))
                }) as Box<dyn Screen>);
            }
            let e = wp.ed();
            merge_styles(&mut e.doc, lib, false);
            e.modified = true;
            e.relayout();
        }
        Err(e) => wp.flash(&format!("ERROR: File not found — {e}")),
    }
    Some(styles_list(sel))
}

/// 5 Delete: 1 Leaving Codes (the style's codes replace its style codes),
/// 2 Including Codes (style codes removed), 3 Definition Only.
fn delete_style(wp: &mut Wp, name: &str, how: usize) {
    let e = wp.ed();
    let Some(pos) = e.doc.styles.iter().position(|s| s.name == name) else { return };
    let st = e.doc.styles.remove(pos);
    if how < 2 {
        let mut out = Vec::with_capacity(e.doc.els.len());
        for el in e.doc.els.drain(..) {
            match &el {
                El::Code(Code::StyleOn(n) | Code::OpenStyle(n)) if n == name => {
                    if how == 0 {
                        out.extend(st.on.iter().cloned());
                    }
                }
                El::Code(Code::StyleOff(n)) if n == name => {
                    if how == 0 {
                        out.extend(st.off.iter().cloned());
                    }
                }
                _ => out.push(el),
            }
        }
        e.doc.els = out;
        e.cursor = e.cursor.min(e.doc.els.len());
    }
    e.modified = true;
    e.relayout();
}

impl Screen for StylesList {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = Self::count(wp);
        if let Some(s) = self.search.as_mut() {
            match i {
                Input::Char(c) => {
                    s.push(*c);
                    let low = s.to_lowercase();
                    if let Some(k) = wp.ed_ref().doc.styles.iter().position(|x| x.name.to_lowercase().starts_with(&low)) {
                        self.sel = k;
                    }
                }
                Input::Func(Func::Backspace) => {
                    s.pop();
                }
                _ => self.search = None,
            }
            return Outcome::Keep;
        }
        match i {
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
            Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            Input::Func(Func::PgUp | Func::ScreenUp) => self.sel = self.sel.saturating_sub(15),
            Input::Func(Func::PgDn | Func::ScreenDown) => self.sel = (self.sel + 15).min(n.saturating_sub(1)),
            Input::Func(Func::Enter) => return self.act(wp, 0),
            Input::Func(Func::Exit | Func::Cancel | Func::Esc) => return Outcome::Close,
            Input::Char('0') | Input::Char(' ') => return Outcome::Close,
            Input::Char('n' | 'N') => self.search = Some(String::new()),
            Input::Char(c) => {
                let k = c.to_digit(10).map(|d| d as usize).filter(|d| (1..=8).contains(d)).map(|d| d - 1).or_else(|| {
                    let lc = c.to_ascii_lowercase();
                    MENU.iter().position(|m| m.split('~').nth(1).and_then(|x| x.chars().next()).map(|x| x.to_ascii_lowercase()) == Some(lc))
                });
                if let Some(k) = k {
                    return self.act(wp, k);
                }
            }
            _ => {}
        }
        Outcome::Keep
    }

    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n;
        if double { self.act(wp, 0) } else { Outcome::Keep }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Styles", st);
        g.text(0, 2, "  Name         Type     Description", st);
        let styles = &wp.ed_ref().doc.styles;
        let rows = g.rows.saturating_sub(5).max(1);
        let top = if self.sel >= rows { self.sel + 1 - rows } else { 0 };
        for (r, (k, s)) in styles.iter().enumerate().skip(top).take(rows).enumerate() {
            let row = 3 + r;
            let line = format!("  {:<13}{:<9}{}", s.name.chars().take(12).collect::<String>(), s.kind.name(), s.description);
            let line: String = line.chars().take(g.cols).collect();
            let style = if k == self.sel { wp.theme.block } else { st };
            if k == self.sel {
                g.fill_row(row, 2, 2 + 11 + 9 + 2 + s.description.chars().count().max(12), ' ', style);
            }
            g.text(0, row, &line, style);
            wp.hit(row, 0, g.cols, HitAction::Select(k));
        }
        let fr = g.rows - 1;
        if let Some(s) = &self.search {
            let c = g.text(0, fr, "(Name Search; Enter or arrows to Exit) ", st);
            let c = g.text(c, fr, s, st);
            g.cursor = Some((c, fr));
            return;
        }
        let mut c = 0;
        for (k, it) in MENU.iter().enumerate() {
            let c0 = c;
            c = g.text(c, fr, &format!("{} ", k + 1), st);
            c = g.text_mn(c, fr, it, st, hi);
            wp.hit(fr, c0, c, HitAction::Input(Input::Char(char::from_digit(k as u32 + 1, 10).unwrap())));
            if k + 1 < MENU.len() {
                c = g.text(c, fr, "; ", st);
            }
        }
        let c = g.text(c, fr, ": 1", st);
        g.cursor = Some((c - 1, fr));
    }
}

// ----------------------------------------------------- Styles: Edit

struct StyleEdit {
    idx: usize,
    /// (field 1 = name, 3 = description, text)
    editing: Option<(u8, String)>,
}

const VAL_COL: usize = 27;

fn rename_style(doc: &mut Document, old: &str, new: &str) {
    if old.is_empty() || old == new {
        return;
    }
    for e in doc.els.iter_mut() {
        if let El::Code(Code::StyleOn(n) | Code::StyleOff(n) | Code::OpenStyle(n)) = e {
            if n == old {
                *n = new.to_string();
            }
        }
    }
}

impl StyleEdit {
    fn style<'a>(&self, wp: &'a Wp) -> Option<&'a StyleDef> {
        wp.ed_ref().doc.styles.get(self.idx)
    }

    fn close(&self, wp: &mut Wp) -> Outcome {
        let e = wp.ed();
        if e.doc.styles.get(self.idx).is_some_and(|s| s.name.trim().is_empty()) {
            e.doc.styles.remove(self.idx);
        } else {
            let name = e.doc.styles.get(self.idx).map(|s| s.name.clone()).unwrap_or_default();
            e.doc.styles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            let sel = e.doc.styles.iter().position(|s| s.name == name).unwrap_or(0);
            e.relayout();
            return Outcome::Replace(styles_list(sel));
        }
        Outcome::Replace(styles_list(self.idx.saturating_sub(1)))
    }

    fn edit_codes(&self, wp: &mut Wp) {
        let idx = self.idx;
        let Some(st) = self.style(wp).cloned() else { return };
        wp.screens.clear();
        crate::subedit::open(wp, "Style", style_edit_els(&st), true, move |wp, els| {
            let e = wp.ed();
            if let Some(s) = e.doc.styles.get_mut(idx) {
                set_style_els(s, els);
            }
            e.modified = true;
            e.relayout();
            wp.push(styles_list(idx));
            wp.push(Box::new(StyleEdit { idx, editing: None }));
        });
    }
}

impl Screen for StyleEdit {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let idx = self.idx;
        if let Some((field, text)) = self.editing.as_mut() {
            match i {
                Input::Char(c) => text.push(*c),
                Input::Func(Func::Backspace) => {
                    text.pop();
                }
                Input::Func(Func::Enter | Func::Exit) => {
                    let (field, text) = (*field, text.trim().to_string());
                    self.editing = None;
                    let e = wp.ed();
                    if field == 1 {
                        if !text.is_empty() && e.doc.styles.iter().enumerate().any(|(k, s)| k != idx && s.name == text) {
                            wp.flash("ERROR: Duplicate style name");
                            return Outcome::Keep;
                        }
                        let old = e.doc.styles.get(idx).map(|s| s.name.clone()).unwrap_or_default();
                        rename_style(&mut e.doc, &old, &text);
                        if let Some(s) = e.doc.styles.get_mut(idx) {
                            s.name = text;
                        }
                    } else if let Some(s) = e.doc.styles.get_mut(idx) {
                        s.description = text;
                    }
                    e.modified = true;
                }
                Input::Func(Func::Cancel | Func::Esc) => self.editing = None,
                _ => {}
            }
            return Outcome::Keep;
        }
        let key = match i {
            Input::Char(c) => c.to_ascii_lowercase(),
            Input::Func(Func::Exit | Func::Cancel | Func::Enter | Func::Esc) => '0',
            _ => return Outcome::Keep,
        };
        let paired = self.style(wp).is_some_and(|s| s.kind != StyleKind::Open);
        match key {
            '1' | 'n' => {
                let v = self.style(wp).map(|s| s.name.clone()).unwrap_or_default();
                self.editing = Some((1, v));
            }
            '2' | 't' => {
                return Outcome::Push(
                    MenuLine::new(&["~Paired", "~Open", "Ou~tline"], move |wp, k| {
                        let e = wp.ed();
                        if let Some(s) = e.doc.styles.get_mut(idx) {
                            let kind = [StyleKind::Paired, StyleKind::Open, StyleKind::Outline][k];
                            if kind == StyleKind::Open && s.kind != StyleKind::Open {
                                s.off.clear();
                            }
                            s.kind = kind;
                        }
                        e.modified = true;
                        e.relayout();
                        None
                    })
                    .prefix("Type: "),
                );
            }
            '3' | 'd' => {
                let v = self.style(wp).map(|s| s.description.clone()).unwrap_or_default();
                self.editing = Some((3, v));
            }
            '4' | 'c' => {
                self.edit_codes(wp);
                return Outcome::Close;
            }
            '5' | 'e' if paired => {
                return Outcome::Push(
                    MenuLine::new(&["~Hrt", "O~ff", "~Off/On"], move |wp, k| {
                        let e = wp.ed();
                        if let Some(s) = e.doc.styles.get_mut(idx) {
                            s.enter = [EnterAction::HRt, EnterAction::Off, EnterAction::OffOn][k];
                        }
                        e.modified = true;
                        None
                    })
                    .prefix("Enter: "),
                );
            }
            '0' | ' ' => return self.close(wp),
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Styles: Edit", st);
        let s = self.style(wp).cloned().unwrap_or_default();
        let paired = s.kind != StyleKind::Open;
        let mut rows: Vec<(char, &str, String)> = vec![
            ('1', "~Name", s.name.clone()),
            ('2', "~Type", s.kind.name().to_string()),
            ('3', "~Description", s.description.clone()),
            ('4', "~Codes", String::new()),
        ];
        if paired {
            rows.push(('5', "~Enter", s.enter.name().to_string()));
        }
        let mut cursor = None;
        for (k, (key, label, val)) in rows.iter().enumerate() {
            let row = 2 + 2 * k;
            g.text(5, row, &format!("{key} - "), st);
            let end = g.text_mn(9, row, label, st, hi);
            wp.hit(row, 5, end.max(VAL_COL), HitAction::Input(Input::Char(*key)));
            let mut v = val.clone();
            if let Some((f, t)) = &self.editing {
                if (*f == 1 && *key == '1') || (*f == 3 && *key == '3') {
                    v = t.clone();
                    cursor = Some((VAL_COL + v.chars().count(), row));
                }
            }
            g.text(VAL_COL, row, &v, st);
        }
        // a preview of the codes, like WP's Reveal Codes line
        let preview: String = style_edit_els(&s)
            .iter()
            .map(|e| match e {
                El::Ch(c) => c.to_string(),
                El::Code(Code::Comment(t)) if t.is_empty() => "[Comment]".into(),
                El::Code(c) => crate::reveal::code_text(c),
            })
            .collect();
        let maxw = g.cols.saturating_sub(VAL_COL + 1);
        g.text(VAL_COL, 8, &preview.chars().take(maxw).collect::<String>(), st);
        let fr = g.rows - 1;
        let c = g.text(0, fr, "Selection: 0", st);
        g.cursor = cursor.or(Some((c - 1, fr)));
    }
}

// =================================================== Shift-F5 Outline

pub fn outline_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~On", "O~ff", "~Move Family", "~Copy Family", "~Delete Family"], |wp, k| match k {
        0 | 1 => {
            wp.ed().insert_code(Code::Outline(k == 0));
            wp.after_edit();
            None
        }
        2 | 3 => {
            let e = wp.ed();
            let Some((s, t)) = family_range(&e.doc, e.cursor) else {
                wp.flash("ERROR: No paragraph number found");
                return None;
            };
            let els = e.doc.els[s..t].to_vec();
            if k == 2 {
                e.delete_range(s, t, false);
            }
            e.clipboard = Some(els);
            wp.after_edit();
            wp.flash("Move cursor; press Enter to retrieve.");
            Some(Box::new(crate::ui::RetrievePending) as Box<dyn Screen>)
        }
        _ => {
            if family_range(&wp.ed_ref().doc, wp.ed_ref().cursor).is_none() {
                wp.flash("ERROR: No paragraph number found");
                return None;
            }
            Some(YesNo::new("Delete Outline Family?", false, |wp, yes| {
                if yes {
                    let e = wp.ed();
                    if let Some((s, t)) = family_range(&e.doc, e.cursor) {
                        e.delete_range(s, t, true);
                    }
                    wp.after_edit();
                }
                None
            }) as Box<dyn Screen>)
        }
    })
    .prefix("Outline: ")
}

pub fn para_num_prompt() -> Box<dyn Screen> {
    Prompt::new("Paragraph Level (Press Enter for Automatic): ", "", |wp, s| {
        let s = s?;
        let lvl = s.trim().parse::<u8>().ok().filter(|l| (1..=8).contains(l));
        if !s.trim().is_empty() && lvl.is_none() {
            wp.flash("ERROR: Invalid entry");
            return None;
        }
        wp.ed().insert_code(Code::ParNum(lvl));
        wp.after_edit();
        None
    })
}

pub fn define_screen(wp: &Wp) -> Box<dyn Screen> {
    let e = wp.ed_ref();
    let d = def_at(&e.doc, e.cursor);
    Box::new(Define { orig: d.clone(), def: d, editing: None, ask: None })
}

struct Define {
    orig: ParNumDef,
    def: ParNumDef,
    /// (field 1 start / 6 levels / 60 attach / 9 style name, text)
    editing: Option<(u8, String)>,
    /// waiting for Y/N for item 7 or 8
    ask: Option<u8>,
}

const LVL_COL: usize = 33;

impl Define {
    fn draw_levels(g: &mut Grid, row: usize, items: &[String], st: Style) {
        for (k, s) in items.iter().enumerate() {
            // the number character sits in the level's column
            let off = s.chars().position(|c| "1aAiI".contains(c)).unwrap_or(0);
            let col = (LVL_COL + 6 * k).saturating_sub(off);
            g.text(col, row, s, st);
        }
    }
    fn commit(&mut self, field: u8, text: String) {
        match field {
            1 => self.def.set_start_text(&text),
            6 => {
                let toks: Vec<&str> = text.split_whitespace().collect();
                let attach: Vec<bool> = (0..8).map(|k| level_attached(&self.def.levels[k], k + 1)).collect();
                let mut levels: Vec<NumLevel> = vec![];
                for k in 0..8 {
                    let t = toks.get(k).copied().unwrap_or("");
                    let l = if t.is_empty() {
                        self.def.levels[k].clone()
                    } else {
                        level_from_display(t, k + 1, attach[k], levels.last(), self.def.levels[k].start)
                    };
                    levels.push(l);
                }
                self.def.levels = levels;
                // then the attach flags
                let a: String = (1..8).map(|k| if level_attached(&self.def.levels[k], k + 1) { 'Y' } else { 'N' }).collect();
                self.editing = Some((60, a));
            }
            60 => {
                let flags: Vec<bool> = text.chars().filter(|c| !c.is_whitespace()).map(|c| matches!(c, 'y' | 'Y' | 'j' | 'J')).collect();
                let disp = self.def.display();
                let mut levels: Vec<NumLevel> = vec![self.def.levels[0].clone()];
                for k in 1..8 {
                    let at = flags.get(k - 1).copied().unwrap_or(false);
                    let own = &disp[k];
                    levels.push(level_from_display(own, k + 1, at, levels.last(), self.def.levels[k].start));
                }
                self.def.levels = levels;
            }
            _ => self.def.outline_style = text.trim().to_string(),
        }
    }
}

impl Screen for Define {
    fn full_screen(&self) -> bool {
        true
    }

    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        if let Some(f) = self.ask.take() {
            if let Input::Char(c) = i {
                let yes = matches!(c, 'y' | 'Y' | 'j' | 'J');
                let no = matches!(c, 'n' | 'N');
                if yes || no {
                    if f == 7 {
                        self.def.enter_inserts = yes;
                    } else {
                        self.def.auto_adjust = yes;
                    }
                }
            }
            return Outcome::Keep;
        }
        if let Some((field, text)) = self.editing.as_mut() {
            match i {
                Input::Char(c) => text.push(*c),
                Input::Func(Func::Backspace) => {
                    text.pop();
                }
                Input::Func(Func::Enter | Func::Exit) => {
                    let (f, t) = (*field, text.clone());
                    self.editing = None;
                    self.commit(f, t);
                }
                Input::Func(Func::Cancel | Func::Esc) => self.editing = None,
                _ => {}
            }
            return Outcome::Keep;
        }
        match i {
            Input::Char(c @ '2'..='5') => {
                let p = preset(*c as usize - '2' as usize);
                self.def.levels = p.levels.iter().zip(&self.def.levels).map(|(n, o)| NumLevel { start: o.start, ..n.clone() }).collect();
            }
            Input::Char('1') => self.editing = Some((1, self.def.start_text())),
            Input::Char('6') => self.editing = Some((6, self.def.display().join(" "))),
            Input::Char('7') => self.ask = Some(7),
            Input::Char('8') => self.ask = Some(8),
            Input::Char('9') => self.editing = Some((9, self.def.outline_style.clone())),
            Input::Char('0') | Input::Char(' ') | Input::Func(Func::Exit | Func::Enter) => {
                if self.def != self.orig {
                    wp.ed().insert_format(Code::ParNumDef(self.def.clone()));
                    wp.after_edit();
                }
                return Outcome::CloseAll;
            }
            Input::Func(Func::Cancel | Func::Esc) => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Paragraph Number Definition", st);
        let item = |g: &mut Grid, row: usize, key: char, label: &str| {
            g.text(5, row, &format!("{key} - "), st);
            let end = g.text_mn(9, row, label, st, hi);
            wp.hit(row, 5, end.max(30), HitAction::Input(Input::Char(key)));
        };
        let mut cursor = None;
        let edit_val = |f: u8| self.editing.as_ref().filter(|(x, _)| *x == f).map(|(_, t)| t.clone());
        item(g, 2, '1', "~Starting Paragraph Number");
        g.text(11, 3, "(in legal style)", st);
        let sv = edit_val(1).unwrap_or_else(|| self.def.start_text());
        g.text(52, 2, &sv, st);
        if edit_val(1).is_some() {
            cursor = Some((52 + sv.chars().count(), 2));
        }
        g.text(51, 4, "Levels", st);
        g.text(0, 5, &format!("{:LVL_COL$}1     2     3     4     5     6     7     8", ""), st);
        for (k, (label, row)) in [("~Paragraph", 6), ("~Outline", 7), ("~Legal (1.1.1)", 8), ("~Bullets", 9)].iter().enumerate() {
            item(g, *row, char::from_digit(k as u32 + 2, 10).unwrap(), label);
            Self::draw_levels(g, *row, &preset(k).display(), st);
        }
        item(g, 10, '6', "~User-defined");
        g.text(5, 12, "Current Definition", st);
        match edit_val(6) {
            Some(t) => {
                g.text(LVL_COL, 12, &t, st);
                cursor = Some((LVL_COL + t.chars().count(), 12));
            }
            None => Self::draw_levels(g, 12, &self.def.display(), st),
        }
        g.text(5, 13, "Attach Previous Level", st);
        match edit_val(60) {
            Some(t) => {
                g.text(LVL_COL + 6, 13, &t, st);
                cursor = Some((LVL_COL + 6 + t.chars().count(), 13));
            }
            None => {
                for k in 1..8 {
                    let yes = level_attached(&self.def.levels[k], k + 1);
                    g.text(LVL_COL + 6 * k, 13, if yes { "Yes" } else { "No" }, st);
                }
            }
        }
        item(g, 15, '7', "~Enter Inserts Paragraph Number");
        g.text(52, 15, if self.def.enter_inserts { "Yes" } else { "No" }, st);
        item(g, 17, '8', "~Automatically Adjust to Current Level");
        g.text(52, 17, if self.def.auto_adjust { "Yes" } else { "No" }, st);
        item(g, 19, '9', "Outline Style ~Name");
        let on = edit_val(9).unwrap_or_else(|| self.def.outline_style.clone());
        g.text(52, 19, &on, st);
        if edit_val(9).is_some() {
            cursor = Some((52 + on.chars().count(), 19));
        }
        let fr = g.rows - 1;
        let c = match (self.ask, &self.editing) {
            (Some(_), _) => g.text(0, fr, "No (Yes) ", st),
            (_, Some((6, _))) => g.text(0, fr, "Levels 1-8, separated by spaces (1 a A i I = numbers, other = bullet)", st),
            (_, Some((60, _))) => g.text(0, fr, "Attach previous level (Y/N for levels 2-8)", st),
            _ => g.text(0, fr, "Selection: 0", st),
        };
        g.cursor = cursor.or(Some((c.saturating_sub(1), fr)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wp() -> Wp {
        let mut wp = Wp::new(80, 25);
        wp.settings.style_library = Some(PathBuf::from("/nonexistent/x.sty"));
        wp
    }

    fn keys(wp: &mut Wp, ks: &[Input]) {
        for k in ks {
            wp.input(k.clone());
        }
    }

    fn typed(s: &str) -> Vec<Input> {
        s.chars().map(Input::Char).collect()
    }

    #[test]
    fn create_and_apply_style() {
        let mut wp = wp();
        wp.edit_func(Func::Style, 0);
        let mut k = vec![Input::Char('3'), Input::Char('1')];
        k.extend(typed("Zitat"));
        k.push(Input::Func(Func::Enter));
        k.push(Input::Char('3'));
        k.extend(typed("Quote"));
        k.push(Input::Func(Func::Enter));
        k.push(Input::Char('5'));
        k.push(Input::Char('2')); // Enter: Off
        keys(&mut wp, &k);
        assert_eq!(wp.ed_ref().doc.styles[0].name, "Zitat");
        assert_eq!(wp.ed_ref().doc.styles[0].enter, EnterAction::Off);
        // set its codes directly (the Codes screen is a sub-editor)
        wp.ed().doc.styles[0].on = vec![El::Code(Code::AttrOn(Attr::Italic))];
        wp.ed().doc.styles[0].off = vec![El::Code(Code::AttrOff(Attr::Italic)), El::Code(Code::HRt)];
        keys(&mut wp, &[Input::Func(Func::Exit), Input::Char('1')]);
        assert!(!wp.has_screens());
        keys(&mut wp, &typed("Hallo"));
        keys(&mut wp, &[Input::Func(Func::Enter)]);
        keys(&mut wp, &typed("Text"));
        let els = &wp.ed_ref().doc.els;
        assert_eq!(els[0], El::Code(Code::StyleOn("Zitat".into())));
        assert_eq!(els[6], El::Code(Code::StyleOff("Zitat".into())));
        assert_eq!(wp.ed_ref().doc.plain_text(0, els.len()), "HalloText");
        let l = &wp.ed_ref().layout;
        assert_eq!(l.lines.len(), 2);
        assert!(l.lines[0].cells[0].attrs.has(Attr::Italic));
        assert!(!l.lines[1].cells[0].attrs.has(Attr::Italic));
    }

    #[test]
    fn outline_mode_keys() {
        let mut wp = wp();
        wp.edit_func(Func::DateOutline, 0);
        keys(&mut wp, &[Input::Char('4'), Input::Char('1')]);
        keys(&mut wp, &[Input::Func(Func::Enter)]);
        keys(&mut wp, &typed("Eins"));
        keys(&mut wp, &[Input::Func(Func::Enter), Input::Func(Func::Tab)]);
        keys(&mut wp, &typed("Zwei"));
        keys(&mut wp, &[Input::Func(Func::Enter), Input::Func(Func::Tab), Input::Func(Func::MarRel)]);
        keys(&mut wp, &typed("Drei"));
        let e = wp.ed_ref();
        let lines: Vec<String> = e.layout.lines.iter().map(|x| x.cells.iter().map(|c| c.ch).collect::<String>().trim().to_string()).collect();
        assert_eq!(lines, vec!["", "I.Eins", "A.Zwei", "II.Drei"], "{:?}", e.doc.els);
        // Delete Family on "Eins" removes all three
        wp.ed().cursor = 3;
        wp.edit_func(Func::DateOutline, 0);
        keys(&mut wp, &[Input::Char('4'), Input::Char('5'), Input::Char('y')]);
        let e = wp.ed_ref();
        assert!(!e.doc.plain_text(0, e.doc.len()).contains("Zwei"));
        assert!(e.doc.plain_text(0, e.doc.len()).contains("Drei"));
    }

    #[test]
    fn define_inserts_code() {
        let mut wp = wp();
        wp.edit_func(Func::DateOutline, 0);
        keys(&mut wp, &[Input::Char('6'), Input::Char('4'), Input::Func(Func::Exit)]);
        assert!(matches!(&wp.ed_ref().doc.els[0], El::Code(Code::ParNumDef(d)) if d.preset_index() == Some(2)));
        wp.edit_func(Func::DateOutline, 0);
        keys(&mut wp, &[Input::Char('5'), Input::Func(Func::Enter)]);
        wp.edit_func(Func::DateOutline, 0);
        keys(&mut wp, &[Input::Char('5'), Input::Char('2'), Input::Func(Func::Enter)]);
        let l = &wp.ed_ref().layout;
        let t: String = l.lines[0].cells.iter().map(|c| c.ch).collect();
        assert_eq!(t, "11.1");
    }
}
