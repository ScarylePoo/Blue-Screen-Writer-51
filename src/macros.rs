//! WordPerfect 5.1 macros (.WPM) — loader and player.  (Interpreter is
//! filled in from docs/formats/wpm_wpk_set.md.)

use crate::keys::{Func, Input};
use crate::screens::*;
use crate::ui::Wp;
use std::path::PathBuf;

pub fn find_macro(wp: &Wp, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let file = if name.to_ascii_lowercase().ends_with(".wpm") { name.to_string() } else { format!("{name}.WPM") };
    let direct = wp.resolve_path(&file);
    if direct.exists() {
        return Some(direct);
    }
    for d in &wp.macro_dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().eq_ignore_ascii_case(&file) {
                    return Some(e.path());
                }
            }
        }
    }
    None
}

pub fn run_prompt(wp: &mut Wp) {
    wp.push(Prompt::new("Macro:", "", |wp, s| {
        let s = s?;
        if s.trim().is_empty() {
            return None;
        }
        run_named(wp, &s);
        None
    }));
}

pub fn run_named(wp: &mut Wp, name: &str) {
    match find_macro(wp, name) {
        Some(p) => crate::wpm::run_file(wp, &p),
        None => wp.flash(&format!("ERROR: File not found -- {}.WPM", name.trim().to_uppercase())),
    }
}

pub fn run_alt(wp: &mut Wp, c: char) {
    let name = format!("ALT{c}");
    if find_macro(wp, &name).is_some() {
        run_named(wp, &name);
    }
}

/// File name for a macro name typed at "Define macro:" (`x` → `X.WPM`).
pub fn macro_file_name(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() {
        "WP{WP}.WPM".to_string()
    } else if n.to_ascii_lowercase().ends_with(".wpm") {
        n.to_string()
    } else {
        format!("{}.WPM", n.to_uppercase())
    }
}

/// Where the macro `name` is (or will be) stored: an existing file of that
/// name, else the first macro directory.
pub fn macro_path(wp: &Wp, name: &str) -> PathBuf {
    if let Some(p) = find_macro(wp, name.trim()).filter(|_| !name.trim().is_empty()) {
        return p;
    }
    let file = macro_file_name(name);
    if name.contains(['\\', '/', ':']) {
        wp.resolve_path(&file)
    } else {
        let dir = wp.macro_dirs.iter().find(|d| d.is_dir()).cloned().unwrap_or_else(|| wp.cwd.clone());
        dir.join(&file)
    }
}

fn start_recording(name: String, desc: &str) -> Next {
    Some(Prompt::new("Description:", desc, move |wp, d| {
        let d = d?;
        wp.recording = Some((name.clone(), d, Vec::new()));
        None
    }))
}

/// Ctrl-F10: start / stop recording.
pub fn define_toggle(wp: &mut Wp) {
    if let Some((name, desc, inputs)) = wp.recording.take() {
        crate::wpm::save_recorded(wp, &name, &desc, &inputs);
        return;
    }
    wp.push(
        Prompt::new("Define macro:", "", |wp, s| {
            let name = s?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let path = macro_path(wp, &name);
            if !path.exists() {
                return start_recording(name, "");
            }
            let old = crate::wpm::load_wpm(&path).map(|f| f.description).unwrap_or_default();
            let file = path.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
            Some(
                MenuLine::new(&["~Replace", "~Edit", "~Description"], move |wp, k| match k {
                    0 => start_recording(name, &old),
                    1 => {
                        crate::macroedit::MacroEdit::open_file(wp, path, None);
                        None
                    }
                    _ => Some(Prompt::new("Description:", &old, move |wp, d| {
                        let d = d?;
                        crate::macroedit::MacroEdit::open_file(wp, path, Some(d));
                        None
                    }) as Box<dyn crate::ui::Screen>),
                })
                .prefix(&format!("{file} is Already Defined.  ")),
            )
        })
        .with_alt_letters(),
    );
}

/// Home, Ctrl-F10: edit a macro (new or existing) in the macro editor.
pub fn edit_prompt(wp: &mut Wp) {
    if wp.recording.is_some() {
        define_toggle(wp);
        return;
    }
    wp.push(
        Prompt::new("Define macro:", "", |wp, s| {
            let name = s?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let path = macro_path(wp, &name);
            if path.exists() {
                crate::macroedit::MacroEdit::open_file(wp, path, None);
                return None;
            }
            Some(Prompt::new("Description:", "", move |wp, d| {
                let d = d?;
                crate::macroedit::MacroEdit::open_file(wp, path, Some(d));
                None
            }))
        })
        .with_alt_letters(),
    );
}

pub fn is_recording_key(i: &Input) -> bool {
    matches!(i, Input::Func(Func::MacroDefine))
}
