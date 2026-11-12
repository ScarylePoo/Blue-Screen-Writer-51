//! Shift-F1 Setup — WP 5.1's setup menus, saved to settings.txt.

use crate::menus::{Edit, Form, Item};
use crate::ui::{Screen, Wp};
use std::path::PathBuf;
use std::rc::Rc;

fn yn(b: bool) -> String {
    if b { "Yes".into() } else { "No".into() }
}

/// Change a setting, save and apply it.
fn set(wp: &mut Wp, f: impl FnOnce(&mut crate::settings::Settings)) {
    let mut s = wp.settings.clone();
    f(&mut s);
    if let Err(e) = s.save() {
        wp.flash(&format!("ERROR: cannot save settings — {e}"));
    }
    s.apply(wp);
}

fn item(key: char, label: &'static str, value: impl Fn(&Wp) -> Vec<String> + 'static, edit: Edit) -> Item {
    Item { key, labels: vec![label], value: Rc::new(value), edit }
}

fn with_label(mut it: Item, label: &'static str) -> Item {
    it.labels.push(label);
    it
}

fn unit_name(c: char) -> String {
    match c {
        'c' => "c = centimeters".into(),
        'p' => "p = points".into(),
        'w' => "w = 1200ths of an inch".into(),
        'u' => "u = 4.2 Units (lines/columns)".into(),
        'i' => "i = inches".into(),
        _ => "\" = inches".into(),
    }
}

const UNITS: [&str; 5] = ["\" inches", "c centimeters", "p points", "w 1200ths", "u 4.2 Units"];
const UNIT_CH: [char; 5] = ['"', 'c', 'p', 'w', 'u'];

pub fn setup_menu(wp: &mut Wp) {
    let items = vec![
        item('1', "~Mouse", |_| vec!["(Windows mouse)".into()], Edit::None),
        item('2', "~Display", |_| vec![], Edit::Sub(Rc::new(|_| display()))),
        item('3', "~Environment", |_| vec![], Edit::Sub(Rc::new(|_| environment()))),
        item('4', "~Initial Settings", |_| vec![], Edit::Sub(Rc::new(|_| initial_settings()))),
        item('5', "~Keyboard Layout", |wp| vec![keyboard_name(wp)], Edit::Sub(Rc::new(|wp| Box::new(crate::kbedit::KeyboardList::new(wp))))),
        item('6', "~Location of Files", |_| vec![], Edit::Sub(Rc::new(|_| locations()))),
        item(
            '7',
            "~Windows: open .docx/.doc/.wpd… files",
            |_| vec!["with Blue Screen Writer 51".into()],
            Edit::Choice(
                vec!["Register and choose as default", "Remove registration"],
                Rc::new(|wp, k| {
                    if k == 1 {
                        crate::register::unregister();
                        wp.flash("Registration removed.");
                        return;
                    }
                    let exe = std::env::current_exe().unwrap_or_default();
                    match crate::register::register(&exe) {
                        Ok(()) => {
                            wp.flash("Registered — pick Blue Screen Writer 51 in Windows' Default apps window.");
                            crate::register::open_default_apps();
                        }
                        Err(e) => wp.flash(&format!("ERROR: {e}")),
                    }
                }),
            ),
        ),
    ];
    wp.push(Form::new("Setup", items));
}

fn keyboard_name(wp: &Wp) -> String {
    match &wp.settings.keyboard {
        Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        None if wp.settings.alternate_keys => "ALTRNAT (built in)".into(),
        None => "Original".into(),
    }
}

fn display() -> Box<dyn Screen> {
    let items = vec![
        item(
            '1',
            "~Colors/Fonts/Attributes",
            |wp| {
                vec![match wp.settings.colors.as_str() {
                    "wp" => "Classic blue (default)".into(),
                    "mono" => "Black & White".into(),
                    _ => "Your own colours".into(),
                }]
            },
            Edit::Choice(
                vec!["Your own colours", "Classic blue (default)", "Black & White"],
                Rc::new(|wp, k| set(wp, |s| s.colors = ["her", "wp", "mono"][k].into())),
            ),
        ),
        item(
            '2',
            "~Screen Font Attributes",
            |wp| vec![if wp.settings.typographic { "Real bold/italic/underline".into() } else { "Colours only (DOS)".into() }],
            Edit::Choice(vec!["Real bold/italic/underline", "Colours only"], Rc::new(|wp, k| set(wp, |s| s.typographic = k == 0))),
        ),
        item(
            '4',
            "Screen ~Size",
            |wp| {
                vec![if wp.settings.screen_mode == "classic" {
                    "Classic 80 columns (magnified)".into()
                } else if wp.settings.screen_mode == "page" {
                    "Grow with window — whole page".into()
                } else if wp.settings.zoom_px == 0 {
                    "Large letters (automatic)".into()
                } else {
                    format!("Grow with window — zoom {}px", wp.settings.zoom_px)
                }]
            },
            Edit::Choice(
                vec!["Large letters (automatic)", "Whole page", "Classic 80x25", "Larger text", "Smaller text"],
                Rc::new(|wp, k| {
                    set(wp, |s| match k {
                        0 => {
                            s.screen_mode = "fit".into();
                            s.zoom_px = 0;
                        }
                        1 => {
                            s.screen_mode = "page".into();
                            s.zoom_px = 0;
                        }
                        2 => s.screen_mode = "classic".into(),
                        3 => {
                            s.screen_mode = "fit".into();
                            s.zoom_px = (if s.zoom_px == 0 { 18 } else { s.zoom_px } + 4).min(80);
                        }
                        _ => {
                            s.screen_mode = "fit".into();
                            s.zoom_px = (if s.zoom_px == 0 { 18 } else { s.zoom_px }).saturating_sub(4).max(10);
                        }
                    })
                }),
            ),
        ),
        item(
            '3',
            "~Text Screen Type",
            |wp| vec![format!("80x{} 16 Color", wp.settings.rows)],
            Edit::Choice(vec!["80x25", "80x43", "80x50"], Rc::new(|wp, k| set(wp, |s| s.rows = [25, 43, 50][k]))),
        ),
        item('4', "~Menu Options", |_| vec![], Edit::Sub(Rc::new(|_| menu_options()))),
        item(
            '6',
            "~Reveal Codes Window Size",
            |wp| vec![wp.settings.reveal_rows.to_string()],
            Edit::Fields(Rc::new(|wp, v| {
                if let Ok(n) = v[0].trim().parse::<usize>() {
                    set(wp, |s| s.reveal_rows = n.clamp(1, 20));
                }
            })),
        ),
        item(
            '7',
            "~Filename on the Status Line",
            |wp| vec![yn(wp.settings.filename_on_status)],
            Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.filename_on_status = y))),
        ),
        item(
            '8',
            "Co~mments Display",
            |wp| vec![yn(wp.settings.comments_display)],
            Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.comments_display = y))),
        ),
    ];
    Form::new("Setup: Display", items)
}

fn environment() -> Box<dyn Screen> {
    let items = vec![
        item('1', "~Backup Options", |_| vec![], Edit::Sub(Rc::new(|_| backup()))),
        item('2', "B~eep Options", |_| vec!["(Windows sounds)".into()], Edit::None),
        item('T', "~Typing Aids (quotes, spelling, autocorrect)", |_| vec![], Edit::Sub(Rc::new(|_| crate::aids::setup_screen()))),
        item(
            '7',
            "~Prompt for Hyphenation",
            |_| vec![crate::hyph::PROMPT_NAMES[crate::hyph::prompt_mode() as usize].into()],
            Edit::Choice(crate::hyph::PROMPT_NAMES.to_vec(), Rc::new(|_, k| crate::hyph::set_prompt_setting(k as u8))),
        ),
        item('8', "~Units of Measure", |wp| vec![unit_name(wp.settings.unit_entry)], Edit::Sub(Rc::new(|_| units()))),
        Item {
            key: '9',
            labels: vec!["~Alternate Keyboard", "  (F1 - Help, Esc - Cancel, F3 - Repeat)"],
            value: Rc::new(|wp| vec![yn(wp.settings.alternate_keys)]),
            edit: Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.alternate_keys = y))),
        },
    ];
    Form::new("Setup: Environment", items)
}

fn backup() -> Box<dyn Screen> {
    let items = vec![
        Item {
            key: '1',
            labels: vec!["~Timed Document Backup", "  Minutes Between Backups"],
            value: Rc::new(|wp| vec![yn(wp.settings.backup_minutes > 0), wp.settings.backup_minutes.to_string()]),
            edit: Edit::Fields(Rc::new(|wp, v| {
                let on = v[0].trim().to_lowercase().starts_with(['y', 'j']);
                let m: u32 = v[1].trim().parse().unwrap_or(10);
                set(wp, |s| s.backup_minutes = if on { m.max(1) } else { 0 });
            })),
        },
        item(
            '4',
            "~Autosave Word documents (minutes, 0 = off)",
            |wp| vec![wp.settings.autosave_minutes.to_string()],
            Edit::Fields(Rc::new(|wp, v| {
                if let Ok(n) = v[0].trim().parse::<u32>() {
                    set(wp, |s| s.autosave_minutes = n);
                }
            })),
        ),
        item(
            '2',
            "~Original Document Backup (.BK!)",
            |wp| vec![yn(wp.settings.original_backup)],
            Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.original_backup = y))),
        ),
        item(
            '3',
            "Backup ~Directory",
            |wp| vec![wp.settings.backup_dir().display().to_string()],
            Edit::Fields(Rc::new(|wp, v| {
                let p = PathBuf::from(v[0].trim());
                set(wp, |s| s.backup_dir = if v[0].trim().is_empty() { None } else { Some(p) });
            })),
        ),
    ];
    let mut f = Form::new("Setup: Backup", items);
    f.footer = "Timed backup files are kept in the Backup Directory. Selection: 0".into();
    f
}

fn units() -> Box<dyn Screen> {
    let items = vec![
        Item {
            key: '1',
            labels: vec!["~Display and Entry of Numbers", "  for Margins, Tabs, etc."],
            value: Rc::new(|wp| vec![unit_name(wp.settings.unit_entry)]),
            edit: Edit::Choice(UNITS.to_vec(), Rc::new(|wp, k| set(wp, |s| s.unit_entry = UNIT_CH[k]))),
        },
        item(
            '2',
            "~Status Line Display",
            |wp| vec![unit_name(wp.settings.unit_status)],
            Edit::Choice(UNITS.to_vec(), Rc::new(|wp, k| set(wp, |s| s.unit_status = UNIT_CH[k]))),
        ),
    ];
    let mut f = Form::new("Setup: Units of Measure", items);
    f.footer = "Legend: \" = inches  i = inches  c = centimeters  p = points  w = 1200ths  u = 4.2 Units".into();
    f
}

fn initial_settings() -> Box<dyn Screen> {
    let items = vec![
        item(
            '2',
            "~Date Format",
            |wp| vec![wp.settings.date_format.clone()],
            Edit::Fields(Rc::new(|wp, v| {
                let f = v[0].clone();
                set(wp, |s| s.date_format = f);
            })),
        ),
        item(
            '3',
            "Date ~Language",
            |wp| vec![if wp.settings.german_dates { "Deutsch".into() } else { "English".into() }],
            Edit::Choice(vec!["English", "Deutsch"], Rc::new(|wp, k| set(wp, |s| s.german_dates = k == 1))),
        ),
        item(
            '5',
            "~Initial Codes",
            |wp| vec![format!("{} code(s)", wp.settings.initial_code_els().len())],
            Edit::Action(Rc::new(|wp| {
                let els = wp.settings.initial_code_els();
                crate::subedit::open(wp, "Initial Codes (all new documents)", els, true, |wp, els| {
                    let codes: Vec<_> = els.into_iter().filter(|e| !e.is_char()).collect();
                    set(wp, |s| s.set_initial_code_els(&codes));
                });
            })),
        ),
        item(
            '6',
            "~Repeat Value",
            |wp| vec![wp.settings.repeat_value.to_string()],
            Edit::Fields(Rc::new(|wp, v| {
                if let Ok(n) = v[0].trim().parse::<u32>() {
                    set(wp, |s| s.repeat_value = n.max(1));
                }
            })),
        ),
    ];
    let mut f = Form::new("Setup: Initial Settings", items);
    f.footer = "Date: 1 Day  2 Month  3 Month name  4 Year  5 Year (2)  6 Weekday  7 Hour(24) 8 Hour(12) 9 Min 0 am/pm".into();
    f
}

fn locations() -> Box<dyn Screen> {
    let path_item = |key: char, label: &'static str, get: fn(&Wp) -> String, put: fn(&mut crate::settings::Settings, Option<PathBuf>)| {
        item(
            key,
            label,
            move |wp| vec![get(wp)],
            Edit::Fields(Rc::new(move |wp, v| {
                let t = v[0].trim().to_string();
                let p = if t.is_empty() { None } else { Some(PathBuf::from(t)) };
                if let Some(d) = &p {
                    if !d.is_dir() {
                        wp.flash(&format!("ERROR: Directory not found — {}", d.display()));
                        return;
                    }
                }
                set(wp, |s| put(s, p));
            })),
        )
    };
    let items = vec![
        path_item(
            '1',
            "~Backup Files",
            |wp| wp.settings.backup_dir().display().to_string(),
            |s, p| s.backup_dir = p,
        ),
        path_item(
            '2',
            "~Keyboard/Macro Files",
            |wp| wp.settings.macro_dirs.first().map(|p| p.display().to_string()).unwrap_or_default(),
            |s, p| s.macro_dirs = p.into_iter().collect(),
        ),
        with_label(item(
            '5',
            "~Style Files",
            |wp| {
                vec![
                    wp.settings.style_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                    wp.settings.style_library.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                ]
            },
            Edit::Fields(Rc::new(|wp, v| {
                let dir = v[0].trim();
                let dir = if dir.is_empty() { None } else { Some(PathBuf::from(dir)) };
                if let Some(d) = &dir {
                    if !d.is_dir() {
                        wp.flash(&format!("ERROR: Directory not found — {}", d.display()));
                        return;
                    }
                }
                let lib = v.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
                let lib = if lib.is_empty() { None } else { Some(PathBuf::from(lib)) };
                set(wp, |s| {
                    s.style_dir = dir;
                    s.style_library = lib;
                });
            })),
        ), "    Library Filename"),
        path_item(
            '7',
            "~Documents",
            |wp| wp.settings.default_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            |s, p| s.default_dir = p,
        ),
    ];
    Form::new("Setup: Location of Files", items)
}

// ------------------------------------------------------ keyboard layouts

/// Setup → 5 Keyboard Layout (also the {Keyboard} key).
pub fn keyboard_layout(wp: &mut Wp) {
    let l = crate::kbedit::KeyboardList::new(wp);
    wp.push(Box::new(l));
}

/// Change a setting from another module (saved and applied).
pub fn change(wp: &mut Wp, f: impl FnOnce(&mut crate::settings::Settings)) {
    set(wp, f);
}

// ---------------------------------------------------------- menu options

const LETTERS: [&str; 4] = ["bold", "underline", "reverse", "none"];

fn menu_options() -> Box<dyn Screen> {
    let items = vec![
        item(
            '1',
            "Menu ~Letter Display",
            |wp| {
                let k = LETTERS.iter().position(|l| *l == wp.settings.menu_letters).unwrap_or(0);
                vec![["Bold", "Underline", "Reverse", "Normal"][k].into()]
            },
            Edit::Choice(vec!["Bold", "Underline", "Reverse", "Normal"], Rc::new(|wp, k| set(wp, |s| s.menu_letters = LETTERS[k].into()))),
        ),
        item(
            '7',
            "Menu Bar ~Separator Line",
            |wp| vec![yn(wp.settings.menu_bar_separator)],
            Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.menu_bar_separator = y))),
        ),
        item(
            '8',
            "Menu ~Bar Remains Visible",
            |wp| vec![yn(wp.settings.menu_bar_visible)],
            Edit::YesNo(Rc::new(|wp, y| set(wp, |s| s.menu_bar_visible = y))),
        ),
    ];
    let mut f = Form::new("Setup: Menu Options", items);
    f.footer = "Alt-= or the right mouse button shows the menu bar.   Selection: 0".into();
    f
}
