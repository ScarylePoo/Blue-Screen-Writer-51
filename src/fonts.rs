//! Installed TrueType/OpenType fonts: discovery, the Base Font list, and
//! real character widths for line breaking.

use crate::doc::FontSpec;
use crate::grid::*;
use crate::keys::{Func, Input};
use crate::layout::Metrics;
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct FontFace {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
    pub index: u32,
}

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn be32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

/// Read (family, subfamily) from an sfnt at offset `base`.
fn sfnt_names(b: &[u8], base: usize) -> Option<(String, String)> {
    let num = be16(b, base + 4)? as usize;
    let mut name_off = None;
    for i in 0..num {
        let r = base + 12 + i * 16;
        if b.get(r..r + 4)? == b"name" {
            name_off = Some(be32(b, r + 8)? as usize);
        }
    }
    let t = name_off?;
    let count = be16(b, t + 2)? as usize;
    let str_off = t + be16(b, t + 4)? as usize;
    let mut best: HashMap<u16, (i32, String)> = HashMap::new();
    for i in 0..count {
        let r = t + 6 + i * 12;
        let plat = be16(b, r)?;
        let enc = be16(b, r + 2)?;
        let lang = be16(b, r + 4)?;
        let id = be16(b, r + 6)?;
        let len = be16(b, r + 8)? as usize;
        let off = be16(b, r + 10)? as usize;
        if !matches!(id, 1 | 2 | 16 | 17) {
            continue;
        }
        let raw = b.get(str_off + off..str_off + off + len)?;
        let (score, s) = match (plat, enc) {
            (3, 1) | (3, 10) | (0, _) => {
                let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                (if lang == 0x409 { 3 } else { 2 }, String::from_utf16_lossy(&u))
            }
            (1, 0) => (1, raw.iter().map(|&c| c as char).collect()),
            _ => continue,
        };
        if best.get(&id).is_none_or(|(s0, _)| score > *s0) {
            best.insert(id, (score, s));
        }
    }
    let fam = best.get(&16).or(best.get(&1))?.1.clone();
    let sub = best.get(&17).or(best.get(&2)).map(|x| x.1.clone()).unwrap_or_else(|| "Regular".into());
    Some((fam, sub))
}

/// Read only the bytes needed for the names: the table directory and the
/// 'name' table (fonts can be many MB; reading them whole is slow).
fn read_faces(path: &Path) -> Vec<FontFace> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let mut read_at = |off: u64, len: usize| -> Option<Vec<u8>> {
        f.seek(SeekFrom::Start(off)).ok()?;
        let mut b = vec![0u8; len];
        let n = f.read(&mut b).ok()?;
        b.truncate(n);
        Some(b)
    };
    // Build a sparse image of the file containing just the parts we need.
    let mut faces_at: Vec<(u64, u32)> = vec![];
    let Some(head) = read_at(0, 12 + 64 * 4) else { return vec![] };
    if head.starts_with(b"ttcf") {
        let n = be32(&head, 8).unwrap_or(0) as usize;
        for i in 0..n.min(64) {
            if let Some(off) = be32(&head, 12 + i * 4) {
                faces_at.push((off as u64, i as u32));
            }
        }
    } else {
        faces_at.push((0, 0));
    }
    let mut out = vec![];
    for (base, index) in faces_at {
        let Some(dir) = read_at(base, 12 + 64 * 16) else { continue };
        let num = be16(&dir, 4).unwrap_or(0) as usize;
        let mut name_tab = None;
        for i in 0..num.min(64) {
            let r = 12 + i * 16;
            if dir.get(r..r + 4) == Some(b"name") {
                name_tab = Some((be32(&dir, r + 8).unwrap_or(0) as u64, be32(&dir, r + 12).unwrap_or(0) as usize));
            }
        }
        let Some((off, len)) = name_tab else { continue };
        let Some(tab) = read_at(off, len.min(1 << 20)) else { continue };
        // fake sfnt: directory with one 'name' entry pointing at offset 28
        let mut fake = vec![0u8; 28];
        fake[4..6].copy_from_slice(&1u16.to_be_bytes());
        fake[12..16].copy_from_slice(b"name");
        fake[20..24].copy_from_slice(&28u32.to_be_bytes());
        fake.extend_from_slice(&tab);
        if let Some((fam, sty)) = sfnt_names(&fake, 0) {
            out.push(FontFace { family: fam, style: sty, path: path.to_path_buf(), index });
        }
    }
    out
}

pub fn font_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    #[cfg(windows)]
    {
        let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        v.push(windir.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join(r"Microsoft\Windows\Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        v.push("/Library/Fonts".into());
        v.push("/System/Library/Fonts".into());
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Library/Fonts"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        v.push("/usr/share/fonts".into());
        v.push("/usr/local/share/fonts".into());
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(&h).join(".local/share/fonts"));
            v.push(PathBuf::from(h).join(".fonts"));
        }
    }
    v
}

fn scan_dir(d: &Path, out: &mut Vec<FontFace>, depth: u32) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 4 {
            scan_dir(&p, out, depth + 1);
            continue;
        }
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            out.extend(read_faces(&p));
        }
    }
}

/// All installed faces (scanned once, on first use).
pub fn installed() -> &'static Vec<FontFace> {
    static FACES: OnceLock<Vec<FontFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut v = vec![];
        for d in font_dirs() {
            scan_dir(&d, &mut v, 0);
        }
        v.sort_by(|a, b| a.family.to_lowercase().cmp(&b.family.to_lowercase()).then(a.style.cmp(&b.style)));
        v
    })
}

pub fn families() -> Vec<String> {
    let mut f: Vec<String> = installed().iter().map(|x| x.family.clone()).collect();
    f.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    f
}

/// Map a font name to an installed family.  Old WordPerfect documents use
/// printer font names like "Courier 10pt 10 Pitch", "Times Roman 12pt",
/// "Helv 10pt", "Dutch Roman" — map those to the Windows equivalents.
pub fn resolve_family(name: &str) -> String {
    let fams = families();
    if fams.iter().any(|f| f.eq_ignore_ascii_case(name)) {
        return name.to_string();
    }
    let l = name.to_lowercase();
    let pick = |cands: &[&str]| cands.iter().find(|c| fams.iter().any(|f| f.eq_ignore_ascii_case(c))).map(|c| c.to_string());
    let mapped = if l.contains("courier") || l.contains("line printer") || l.contains("letter gothic") || l.contains("prestige") || l.contains("pica") || l.contains("elite") || l.contains("cpi") || l.contains("pitch") {
        pick(&["Courier New", "Courier"])
    } else if l.contains("times") || l.contains("roman") || l.contains("dutch") || l.contains("tms") || l.contains("century") || l.contains("serif") && !l.contains("sans") {
        pick(&["Times New Roman", "Liberation Serif"])
    } else if l.contains("helv") || l.contains("swiss") || l.contains("univers") || l.contains("arial") || l.contains("sans") || l.contains("optima") {
        pick(&["Arial", "Liberation Sans"])
    } else if l.contains("symbol") {
        pick(&["Symbol"])
    } else if l.contains("garamond") {
        pick(&["Garamond", "Times New Roman"])
    } else {
        None
    };
    if let Some(m) = mapped {
        return m;
    }
    // longest installed family that the name starts with ("Arial 12pt")
    if let Some(f) = fams.iter().filter(|f| l.starts_with(&f.to_lowercase())).max_by_key(|f| f.len()) {
        return f.clone();
    }
    pick(&["Courier New"]).unwrap_or_else(|| name.to_string())
}

/// Best face for a family name (+ bold/italic).
pub fn find_face(family: &str, bold: bool, italic: bool) -> Option<&'static FontFace> {
    let family = &resolve_family(family);
    let cands: Vec<&FontFace> = installed().iter().filter(|f| f.family.eq_ignore_ascii_case(family)).collect();
    if cands.is_empty() {
        return None;
    }
    let want = |f: &FontFace| {
        let s = f.style.to_lowercase();
        let b = s.contains("bold");
        let i = s.contains("italic") || s.contains("oblique");
        (b == bold) as i32 * 2 + (i == italic) as i32 * 2 + (s == "regular" || s == "normal" || s == "book") as i32
    };
    cands.into_iter().max_by_key(|f| want(f))
}

// ------------------------------------------------- CJK font substitution

thread_local! {
    static GLYPH_FONTS: RefCell<HashMap<String, Option<fontdue::Font>>> = RefCell::new(HashMap::new());
}

/// Does the installed font `family` have a glyph for `ch`?
pub fn has_glyph(family: &str, ch: char) -> bool {
    GLYPH_FONTS.with(|m| {
        let mut m = m.borrow_mut();
        let f = m.entry(family.to_lowercase()).or_insert_with(|| {
            let face = find_face(family, false, false)?;
            let data = std::fs::read(&face.path).ok()?;
            fontdue::Font::from_bytes(data, fontdue::FontSettings { collection_index: face.index, ..Default::default() }).ok()
        });
        f.as_ref().is_some_and(|f| f.lookup_glyph_index(ch) != 0)
    })
}

/// The font a character is printed with: the document font, or for
/// Chinese/Japanese/Korean characters it lacks, a Windows CJK font
/// (Microsoft YaHei, Yu Gothic, Malgun Gothic …), as Word does.
pub fn font_for_char(family: &str, ch: char, jp: bool) -> String {
    if crate::cjk::is_cjk(ch) && !has_glyph(family, ch) {
        if let Some(f) = crate::cjk::fallback_family(ch, jp) {
            return f.to_string();
        }
    }
    family.to_string()
}

// ----------------------------------------------------------------- metrics

/// Widths from the real TrueType font, so line breaks match the printout.
pub struct TtfMetrics {
    fonts: RefCell<HashMap<String, Option<fontdue::Font>>>,
    /// family → (fast table for chars < 0x500, other chars)
    adv: RefCell<HashMap<String, (Vec<f32>, HashMap<char, f32>)>>,
}

impl TtfMetrics {
    pub fn new() -> TtfMetrics {
        TtfMetrics { fonts: RefCell::new(HashMap::new()), adv: RefCell::new(HashMap::new()) }
    }

    fn compute(&self, family: &str, ch: char) -> Option<f32> {
        let mut fonts = self.fonts.borrow_mut();
        let f = fonts.entry(family.to_lowercase()).or_insert_with(|| {
            let face = find_face(family, false, false)?;
            let data = std::fs::read(&face.path).ok()?;
            fontdue::Font::from_bytes(data, fontdue::FontSettings { collection_index: face.index, ..Default::default() }).ok()
        });
        let font = f.as_ref()?;
        Some(if font.lookup_glyph_index(ch) == 0 && ch != ' ' {
            // printed with a CJK fallback font: full width
            if crate::cjk::is_wide(ch) {
                return Some(1.0);
            }
            font.metrics('n', 1000.0).advance_width / 1000.0
        } else {
            font.metrics(ch, 1000.0).advance_width / 1000.0
        })
    }

    fn advance_em(&self, family: &str, ch: char) -> Option<f32> {
        // no allocation on the hot path: look up by &str, then by char
        {
            let adv = self.adv.borrow();
            if let Some((table, other)) = adv.get(family) {
                let c = ch as usize;
                if c < table.len() {
                    let v = table[c];
                    if v >= 0.0 {
                        return Some(v);
                    }
                    if v == -2.0 {
                        return None;
                    }
                } else if let Some(v) = other.get(&ch) {
                    return Some(*v);
                }
            }
        }
        let a = self.compute(family, ch);
        let mut adv = self.adv.borrow_mut();
        let entry = adv.entry(family.to_string()).or_insert_with(|| (vec![-1.0; 0x500], HashMap::new()));
        let c = ch as usize;
        if c < entry.0.len() {
            entry.0[c] = a.unwrap_or(-2.0);
        } else if let Some(v) = a {
            entry.1.insert(ch, v);
        }
        a
    }
}

impl Metrics for TtfMetrics {
    fn width(&self, font: &FontSpec, size_cpt: u32, ch: char) -> i32 {
        let em_wpu = size_cpt as f32 * 12.0 / 72.0; // cpt*1200/7200
        match self.advance_em(&font.name, ch) {
            Some(a) => (a * em_wpu).round() as i32,
            None => (em_wpu * 0.6).round() as i32,
        }
    }
}

// --------------------------------------------------------- Base Font list

pub struct FontList {
    names: Vec<String>,
    sel: usize,
    top: usize,
    current: FontSpec,
    search: Option<String>,
    pub set_initial: bool,
}

impl FontList {
    pub fn new(current: FontSpec) -> FontList {
        let names = families();
        let sel = names.iter().position(|n| n.eq_ignore_ascii_case(&current.name)).unwrap_or(0);
        FontList { names, sel, top: sel.saturating_sub(5), current, search: None, set_initial: false }
    }

    fn choose(&self, wp: &mut Wp) -> Outcome {
        let Some(name) = self.names.get(self.sel).cloned() else { return Outcome::Close };
        let cur = self.current.size_cpt as f64 / 100.0;
        let initial = self.set_initial;
        Outcome::Replace(Prompt::new("Point size:", &format!("{cur}"), move |wp, s| {
            let s = s?;
            let pts: f64 = s.trim().trim_end_matches("pt").replace(',', ".").parse().ok()?;
            if !(1.0..=999.0).contains(&pts) {
                return None;
            }
            let f = FontSpec { name: name.clone(), size_cpt: (pts * 100.0).round() as u32 };
            if initial {
                let e = wp.ed();
                e.doc.init.font = f;
                e.modified = true;
                e.relayout();
            } else {
                wp.ed().insert_format(crate::doc::Code::Font(f));
            }
            wp.after_edit();
            None
        }))
        .into_with(wp)
    }
}

trait IntoWith {
    fn into_with(self, wp: &mut Wp) -> Outcome;
}
impl IntoWith for Outcome {
    fn into_with(self, _wp: &mut Wp) -> Outcome {
        self
    }
}

impl Screen for FontList {
    fn full_screen(&self) -> bool {
        true
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n.min(self.names.len().saturating_sub(1));
        if double { self.choose(wp) } else { Outcome::Keep }
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = self.names.len().max(1);
        let vis = wp.rows.saturating_sub(4);
        if let Some(s) = &mut self.search {
            match i {
                Input::Char(c) => {
                    s.push(*c);
                    let p = s.to_lowercase();
                    if let Some(k) = self.names.iter().position(|x| x.to_lowercase().starts_with(&p)) {
                        self.sel = k;
                    }
                }
                Input::Func(Func::Backspace) => {
                    s.pop();
                }
                Input::Func(Func::Enter) => {
                    self.search = None;
                    return self.choose(wp);
                }
                _ => self.search = None,
            }
        } else {
            match i {
                Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n - 1),
                Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
                Input::Func(Func::PgDn) | Input::Func(Func::ScreenDown) => self.sel = (self.sel + vis).min(n - 1),
                Input::Func(Func::PgUp) | Input::Func(Func::ScreenUp) => self.sel = self.sel.saturating_sub(vis),
                Input::Char('1') | Input::Func(Func::Enter) => return self.choose(wp),
                Input::Func(Func::SearchFwd) => self.search = Some(String::new()),
                Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Char('0') => return Outcome::Close,
                Input::Char(c) if !c.is_ascii_digit() => {
                    // typing a name searches incrementally (Name Search)
                    let s = c.to_string();
                    let p = s.to_lowercase();
                    if let Some(k) = self.names.iter().position(|x| x.to_lowercase().starts_with(&p)) {
                        self.sel = k;
                    }
                    self.search = Some(s);
                }
                _ => {}
            }
        }
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + vis {
            self.top = self.sel + 1 - vis;
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, if self.set_initial { "Document Initial Font" } else { "Base Font" }, st);
        let vis = g.rows.saturating_sub(4);
        for (r, name) in self.names.iter().enumerate().skip(self.top).take(vis) {
            let y = 2 + r - self.top;
            let mark = if name.eq_ignore_ascii_case(&self.current.name) { '*' } else { ' ' };
            let s = format!("{mark} {name}");
            g.text(1, y, &s, if r == self.sel { wp.theme.block } else { st });
            wp.hit(y, 0, g.cols, crate::ui::HitAction::Select(r));
        }
        if self.names.is_empty() {
            g.text(2, 3, "No TrueType/OpenType fonts were found.", st);
        }
        let fr = g.rows - 1;
        if let Some(s) = &self.search {
            g.text(0, fr, &format!("(Name Search; Enter or arrows to Exit) {s}"), st);
        } else {
            let c = g.text_mn(0, fr, "1 ~Select; ~N Name search: ", st, wp.theme.bold);
            g.text(c, fr, "1", st);
            g.cursor = Some((c, fr));
        }
    }
}
