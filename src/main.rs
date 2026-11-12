#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Blue Screen Writer 51 — a clean-room, native word processor with the
//! WordPerfect 5.1 for DOS editing experience.

mod agile;
mod aids;
mod app;
mod bidi;
mod comments;
mod columns;
mod clipboard;
mod doc;
mod docx;
mod docx_styles;
mod editor;
mod export;
mod equation;
mod gfx;
mod gfxdocx;
mod gfxui;
mod raster;
mod wpg;
mod filelist;
mod fileio;
mod fmt;
mod fmtdocx;
mod fmtlayout;
mod fmtmenu;
mod fmtpage;
mod fmt_abw;
mod fmt_amipro;
mod fmt_build;
mod fmt_html;
mod fmt_md;
mod fmt_msg;
mod fmt_pdf;
mod fmt_text;
mod fmt_word2;
mod fmt_wordstar;
mod fmt_wordxml;
mod fmt_works;
mod fmt_wp42;
mod fmt_wp6;
mod fmt_wpmac;
mod fmt_write;
mod fonts;
mod grid;
mod history;
mod hyph;
mod keys;
mod layout;
mod linedraw;
mod links;
mod macros;
mod marktext;
mod math;
mod merge;
mod menubar;
mod menus;
mod help;
mod macroedit;
mod kbedit;
mod shell;
mod msdoc;
mod odt;
mod page;
mod password;
mod docinfo;
mod recent;
mod pagerender;
mod png;
mod print;
mod present;
mod render;
mod register;
mod reveal;
mod rtf;
mod screens;
mod settings;
mod sheet;
mod sort;
mod setup;
mod spell;
mod subedit;
mod stream;
mod styles;
mod styles_ui;
mod sys;
mod tables;
mod tabset;
mod track;
mod revisions;
mod ui;
mod wp5;
mod wpchars;
mod wpk;
mod wpm;

use keys::{Func, Input, K, KeyIn, Mods};
use std::path::PathBuf;
use ui::Wp;

/// Route one physical key press: keyboard layout → WP input → program.
pub fn dispatch_key(wp: &mut Wp, ki: KeyIn) {
    wp.last_key = Some(ki);
    if crate::kbedit::capture_key(wp, ki) {
        return;
    }
    let input = if let Some(kb) = &wp.keyboard {
        match kb.map.get(&ki) {
            Some(wpk::KeyAction::Input(i)) => Some(i.clone()),
            Some(wpk::KeyAction::Macro { name, body }) => {
                let (name, body) = (name.clone(), body.clone());
                wpm::run_body(wp, &name, body);
                return;
            }
            None => keys::default_map(ki),
        }
    } else if wp.alternate_keys {
        keys::alternate_map(ki)
    } else {
        keys::default_map(ki)
    };
    let Some(input) = input else { return };
    if wpm::feed_key(wp, &input) {
        return;
    }
    wp.input(input);
}

/// Let a running macro make progress.  Returns true if anything happened.
pub fn pump(wp: &mut Wp) -> bool {
    let mut any = false;
    for _ in 0..20_000 {
        if !wpm::step(wp) {
            break;
        }
        any = true;
    }
    any
}

pub fn ui_close_request(wp: &mut Wp) {
    wp.input(Input::Func(Func::CloseWindow));
}

pub fn window_title(wp: &Wp) -> String {
    let name = wp.ed_ref().path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    match name {
        Some(n) => format!("{n} — Blue Screen Writer 51"),
        None => "Blue Screen Writer 51".into(),
    }
}

/// Parse a test script: "text" in quotes, {Key} names like {F6},
/// {Shift-F8}, {Alt-F4}, {Ctrl-Home}, {Enter}, {Left}…
enum Act {
    Key(KeyIn),
    Mouse(ui::MouseKind, usize, usize),
}

fn parse_script(s: &str) -> Vec<Act> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    out.push(Act::Key(KeyIn { k: K::Char(c), m: Mods::default() }));
                }
            }
            '{' => {
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
                // mouse: {Click c,r} {DClick c,r} {Drag c,r} {Up c,r} {Right c,r} {Wheel n}
                let mouse = |kw: &str| -> Option<(usize, usize)> {
                    let r = name.strip_prefix(kw)?.trim();
                    let (a, b) = r.split_once(',')?;
                    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
                };
                if let Some(n) = name.strip_prefix("Wheel ").and_then(|n| n.trim().parse::<i32>().ok()) {
                    out.push(Act::Mouse(ui::MouseKind::Wheel(n), 0, 0));
                    continue;
                }
                let kinds = [
                    ("Click ", ui::MouseKind::Down),
                    ("DClick ", ui::MouseKind::Double),
                    ("Drag ", ui::MouseKind::Drag),
                    ("Up ", ui::MouseKind::Up),
                    ("Right ", ui::MouseKind::Right),
                ];
                if let Some((k, (c, r))) = kinds.iter().find_map(|(kw, k)| mouse(kw).map(|p| (*k, p))) {
                    out.push(Act::Mouse(k, c, r));
                    continue;
                }
                let mut m = Mods::default();
                let mut rest = name.as_str();
                loop {
                    if let Some(r) = rest.strip_prefix("Shift-") {
                        m.shift = true;
                        rest = r;
                    } else if let Some(r) = rest.strip_prefix("Ctrl-") {
                        m.ctrl = true;
                        rest = r;
                    } else if let Some(r) = rest.strip_prefix("Alt-") {
                        m.alt = true;
                        rest = r;
                    } else {
                        break;
                    }
                }
                let k = match rest {
                    "Left" => K::Left,
                    "Right" => K::Right,
                    "Up" => K::Up,
                    "Down" => K::Down,
                    "Home" => K::Home,
                    "End" => K::End,
                    "PgUp" => K::PgUp,
                    "PgDn" => K::PgDn,
                    "Ins" => K::Ins,
                    "Del" => K::Del,
                    "Bksp" | "Backspace" => K::Backspace,
                    "Enter" => K::Enter,
                    "Tab" => K::Tab,
                    "Esc" => K::Esc,
                    "Space" => K::Char(' '),
                    f if f.starts_with('F') && f[1..].parse::<u8>().is_ok() => K::F(f[1..].parse().unwrap()),
                    c if c.chars().count() == 1 => K::Char(c.chars().next().unwrap()),
                    _ => continue,
                };
                out.push(Act::Key(KeyIn { k, m }));
            }
            _ => {}
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--register") {
        let exe = std::env::current_exe().unwrap_or_default();
        match register::register(&exe) {
            Ok(()) => println!("registered"),
            Err(e) => println!("error: {e}"),
        }
        return;
    }
    // --icon-png FILE [SIZE]: write the program icon as a PNG
    if let Some(k) = args.iter().position(|a| a == "--icon-png") {
        let size: u32 = args.get(k + 2).and_then(|s| s.parse().ok()).unwrap_or(1024);
        let file = args.get(k + 1).cloned().unwrap_or_else(|| "icon.png".into());
        let png = register::rgba_png(&register::icon_rgba(size), size);
        match std::fs::write(&file, png) {
            Ok(()) => println!("{file}"),
            Err(e) => println!("error: {e}"),
        }
        return;
    }
    if args.iter().any(|a| a == "--unregister") {
        register::unregister();
        println!("unregistered");
        return;
    }
    if let Some(k) = args.iter().position(|a| a == "--config") {
        if let Some(d) = args.get(k + 1) {
            let _ = settings::CONFIG_OVERRIDE.set(PathBuf::from(d));
        }
    }
    let settings = settings::Settings::load();
    let mut wp = Wp::new(80, settings.rows.max(25));
    settings.apply(&mut wp);

    let mut script: Option<String> = None;
    let mut dump: Option<PathBuf> = None;
    let mut png_out: Option<PathBuf> = None;
    let mut files = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--script" => script = it.next().map(|p| std::fs::read_to_string(p).unwrap_or_default()),
            "--keys" => script = it.next(),
            "--dump" => dump = it.next().map(PathBuf::from),
            "--png" => png_out = it.next().map(PathBuf::from),
            "--config" => {
                it.next();
            }
            _ => files.push(a),
        }
    }
    if let Some(f) = files.first() {
        let p = wp.resolve_path(f);
        if p.exists() {
            wp.retrieve(&p);
        } else {
            wp.ed().path = Some(p);
        }
    }

    wp.offer_recoveries();

    if let Some(s) = script {
        // Headless mode for automated testing.
        links::HEADLESS.store(true, std::sync::atomic::Ordering::Relaxed);
        for a in parse_script(&s) {
            // mouse hits need a drawn screen
            let mut g = grid::Grid::new(wp.cols, wp.rows, wp.theme.normal);
            wp.draw(&mut g);
            match a {
                Act::Key(k) => dispatch_key(&mut wp, k),
                Act::Mouse(kind, c, r) => wp.mouse(kind, c, r),
            }
            pump(&mut wp);
            wp.after_edit();
        }
        let mut g = grid::Grid::new(wp.cols, wp.rows, wp.theme.normal);
        wp.draw(&mut g);
        if let Some(d) = dump {
            let mut s = String::new();
            for r in 0..g.rows {
                let line: String = (0..g.cols).map(|c| g.get(c, r).unwrap().ch).collect();
                s.push_str(line.trim_end());
                s.push('\n');
            }
            if let Some((c, r)) = g.cursor {
                s.push_str(&format!("-- cursor col {c} row {r}\n"));
            }
            let e = wp.ed_ref();
            s.push_str("-- codes: ");
            s.push_str(&ui::els_to_prompt(&e.doc.els));
            s.push('\n');
            let _ = std::fs::write(d, s);
        }
        if let Some(p) = png_out {
            let (w, h) = (1040usize, 650usize);
            let mut r = render::Renderer::new();
            r.fit(w, h, g.cols, g.rows);
            let mut buf = vec![0u32; w * h];
            r.render(&g, &mut buf, w, h, true, wp.theme.screen_bg);
            wp.draw_pixels(&mut buf, w, h, r.cell_h);
            let _ = std::fs::write(p, png::encode(&buf, w, h));
        }
        return;
    }
    app::run(wp);
}
