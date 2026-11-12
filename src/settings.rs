//! Persistent settings (the equivalent of WP{WP}.SET), stored as a small
//! text file in the user's profile:
//! Windows: %APPDATA%\Blue Screen Writer 51\settings.txt

use crate::ui::Wp;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Settings {
    pub rows: usize,
    pub default_dir: Option<PathBuf>,
    pub macro_dirs: Vec<PathBuf>,
    pub keyboard: Option<PathBuf>,
    pub alternate_keys: bool,
    pub date_format: String,
    pub german_dates: bool,
    pub colors: String,
    pub printer: String,
    /// Reveal Codes window size (lines) on a 25-line screen.
    pub reveal_rows: usize,
    /// Units of measure: entry/display and status line ('"', 'c', 'p', 'w', 'u').
    pub unit_entry: char,
    pub unit_status: char,
    /// Timed document backup, minutes (0 = off).
    pub backup_minutes: u32,
    /// Original document backup (.BK!) when saving over a file.
    pub original_backup: bool,
    pub backup_dir: Option<PathBuf>,
    pub repeat_value: u32,
    /// Default initial codes for new documents (stream format, one line).
    pub initial_codes: String,
    /// Draw bold/italic/underline with real typography on the edit screen
    /// (otherwise colours only, like DOS).
    pub typographic: bool,
    pub filename_on_status: bool,
    /// Save named .docx documents automatically every N minutes (0 = off).
    pub autosave_minutes: u32,
    /// Screen: "fit" = the grid grows with the window and shows the whole
    /// page; "classic" = always 80 × `rows`, magnified.
    pub screen_mode: String,
    /// Character cell height in pixels in "fit" mode (0 = automatic: as
    /// large as possible while a whole page fits).
    pub zoom_px: u32,
    /// Setup: Display → Comments Display (comment boxes on the edit screen).
    pub comments_display: bool,
    /// Typing aids, recent files, line rate (see aids.rs).
    pub ext: crate::aids::Ext,
    /// Setup → Location of Files → Style Files: directory and library.
    pub style_dir: Option<PathBuf>,
    pub style_library: Option<PathBuf>,
    /// Setup → Display → Menu Options: the menu bar stays on the top row.
    pub menu_bar_visible: bool,
    /// … with a separator line below it.
    pub menu_bar_separator: bool,
    /// How mnemonic letters are shown: "bold", "underline", "reverse", "none".
    pub menu_letters: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            rows: 25,
            default_dir: None,
            macro_dirs: vec![],
            keyboard: None,
            // Her WP{WP}.SET selects ALTRNAT.WPK (F1 Help, Esc Cancel, F3 Repeat)
            alternate_keys: true,
            date_format: "3 1, 4".into(),
            german_dates: false,
            colors: "her".into(),
            printer: String::new(),
            reveal_rows: 10,
            unit_entry: '"',
            unit_status: '"',
            backup_minutes: 10,
            original_backup: true,
            backup_dir: None,
            repeat_value: 8,
            initial_codes: String::new(),
            typographic: true,
            filename_on_status: true,
            autosave_minutes: 5,
            screen_mode: "fit".into(),
            zoom_px: 0,
            comments_display: true,
            ext: Default::default(),
            style_dir: None,
            style_library: None,
            menu_bar_visible: false,
            menu_bar_separator: false,
            menu_letters: "bold".into(),
        }
    }
}

/// Set by `--config <dir>` (portable installs, tests).
pub static CONFIG_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn config_dir() -> PathBuf {
    if let Some(p) = CONFIG_OVERRIDE.get() {
        return p.clone();
    }
    #[cfg(windows)]
    {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("Blue Screen Writer 51");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join(".config/blue-screen-writer-51");
        }
    }
    PathBuf::from(".")
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.txt")
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(Self::path()) else { return s };
        s.macro_dirs.clear();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let p = || if v.is_empty() { None } else { Some(PathBuf::from(v)) };
            match k.trim() {
                "rows" => s.rows = v.parse().unwrap_or(25).clamp(25, 60),
                "default_dir" => s.default_dir = p(),
                "macro_dir" => s.macro_dirs.extend(p()),
                "keyboard" => s.keyboard = p(),
                "alternate_keys" => s.alternate_keys = v == "yes",
                "date_format" => s.date_format = v.to_string(),
                "german_dates" => s.german_dates = v == "yes",
                "colors" => s.colors = v.to_string(),
                "printer" => s.printer = v.to_string(),
                "reveal_rows" => s.reveal_rows = v.parse().unwrap_or(10).clamp(1, 40),
                "unit_entry" => s.unit_entry = v.chars().next().unwrap_or('"'),
                "unit_status" => s.unit_status = v.chars().next().unwrap_or('"'),
                "backup_minutes" => s.backup_minutes = v.parse().unwrap_or(10),
                "original_backup" => s.original_backup = v == "yes",
                "backup_dir" => s.backup_dir = p(),
                "repeat_value" => s.repeat_value = v.parse().unwrap_or(8).max(1),
                "initial_codes" => s.initial_codes = v.to_string(),
                "typographic" => s.typographic = v == "yes",
                "filename_on_status" => s.filename_on_status = v == "yes",
                "autosave_minutes" => s.autosave_minutes = v.parse().unwrap_or(5),
                "screen_mode" => s.screen_mode = v.to_string(),
                "zoom_px" => s.zoom_px = v.parse().unwrap_or(0),
                "comments_display" => s.comments_display = v == "yes",
                "style_dir" => s.style_dir = p(),
                "style_library" => s.style_library = p(),
                "menu_bar_visible" => s.menu_bar_visible = v == "yes",
                "menu_bar_separator" => s.menu_bar_separator = v == "yes",
                "menu_letters" => s.menu_letters = v.to_string(),
                k => {
                    s.ext.load_kv(k, v);
                }
            }
        }
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        let mut t = String::from("# Blue Screen Writer 51 settings\n");
        let opt = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        t.push_str(&format!("rows={}\n", self.rows));
        t.push_str(&format!("default_dir={}\n", opt(&self.default_dir)));
        for d in &self.macro_dirs {
            t.push_str(&format!("macro_dir={}\n", d.display()));
        }
        t.push_str(&format!("keyboard={}\n", opt(&self.keyboard)));
        t.push_str(&format!("alternate_keys={}\n", yes(self.alternate_keys)));
        t.push_str(&format!("date_format={}\n", self.date_format));
        t.push_str(&format!("german_dates={}\n", yes(self.german_dates)));
        t.push_str(&format!("colors={}\n", self.colors));
        t.push_str(&format!("printer={}\n", self.printer));
        t.push_str(&format!("reveal_rows={}\n", self.reveal_rows));
        t.push_str(&format!("unit_entry={}\n", self.unit_entry));
        t.push_str(&format!("unit_status={}\n", self.unit_status));
        t.push_str(&format!("backup_minutes={}\n", self.backup_minutes));
        t.push_str(&format!("original_backup={}\n", yes(self.original_backup)));
        t.push_str(&format!("backup_dir={}\n", opt(&self.backup_dir)));
        t.push_str(&format!("repeat_value={}\n", self.repeat_value));
        t.push_str(&format!("initial_codes={}\n", self.initial_codes));
        t.push_str(&format!("typographic={}\n", yes(self.typographic)));
        t.push_str(&format!("filename_on_status={}\n", yes(self.filename_on_status)));
        t.push_str(&format!("autosave_minutes={}\n", self.autosave_minutes));
        t.push_str(&format!("screen_mode={}\n", self.screen_mode));
        t.push_str(&format!("zoom_px={}\n", self.zoom_px));
        t.push_str(&format!("comments_display={}\n", yes(self.comments_display)));
        t.push_str(&self.ext.save_kv());
        t.push_str(&format!("style_dir={}\n", opt(&self.style_dir)));
        t.push_str(&format!("style_library={}\n", opt(&self.style_library)));
        t.push_str(&format!("menu_bar_visible={}\n", yes(self.menu_bar_visible)));
        t.push_str(&format!("menu_bar_separator={}\n", yes(self.menu_bar_separator)));
        t.push_str(&format!("menu_letters={}\n", self.menu_letters));
        std::fs::create_dir_all(config_dir())?;
        std::fs::write(Self::path(), t)
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.backup_dir.clone().unwrap_or_else(|| config_dir().join("Backup"))
    }

    /// Default initial codes as elements.
    pub fn initial_code_els(&self) -> Vec<crate::doc::El> {
        if self.initial_codes.is_empty() {
            return vec![];
        }
        let text = format!("#WPSTREAM 1\n{}", self.initial_codes.replace("\\n", "\n"));
        crate::stream::read(&text).map(|d| d.els).unwrap_or_default()
    }

    pub fn set_initial_code_els(&mut self, els: &[crate::doc::El]) {
        let mut d = crate::doc::Document::new();
        d.els = els.to_vec();
        let s = crate::stream::write(&d);
        self.initial_codes = s.lines().filter(|l| l.starts_with('k')).collect::<Vec<_>>().join("\\n");
    }

    pub fn apply(&self, wp: &mut Wp) {
        if let Some(d) = &self.default_dir {
            if d.is_dir() {
                wp.cwd = d.clone();
            }
        }
        wp.macro_dirs = self.macro_dirs.clone();
        if wp.macro_dirs.is_empty() {
            // next to the program: .\WPM and the program folder itself
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    wp.macro_dirs.push(dir.join("WPM"));
                    wp.macro_dirs.push(dir.to_path_buf());
                }
            }
        }
        wp.alternate_keys = self.alternate_keys;
        wp.keyboard = None;
        if let Some(k) = &self.keyboard {
            match crate::wpk::load(k) {
                Ok(kb) => wp.keyboard = Some(kb),
                Err(e) => wp.flash(&format!("Keyboard {}: {e}", k.display())),
            }
        }
        wp.repeat_value = self.repeat_value;
        crate::hyph::load_prompt_setting();
        wp.rows = self.rows.max(25);
        use std::sync::atomic::Ordering::Relaxed;
        crate::doc::UNIT_ENTRY.store(self.unit_entry as u8, Relaxed);
        crate::doc::UNIT_STATUS.store(self.unit_status as u8, Relaxed);
        crate::fileio::ORIGINAL_BACKUP.store(self.original_backup, Relaxed);
        if crate::layout::COMMENTS_DISPLAY.swap(self.comments_display, Relaxed) != self.comments_display {
            for e in wp.docs.iter_mut() {
                e.relayout();
            }
        }
        wp.settings = self.clone();
        wp.apply_doc_theme();
        // new, empty documents get the default initial codes
        for e in wp.docs.iter_mut() {
            if e.is_empty() && e.path.is_none() && e.doc.initial_codes.is_empty() {
                e.doc.initial_codes = self.initial_code_els();
                e.relayout();
            }
        }
    }
}
