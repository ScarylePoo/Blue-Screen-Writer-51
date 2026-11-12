//! RTF (Rich Text Format) reader.
//!
//! Converts an RTF file into the WP code stream, following the same
//! conventions as the .docx reader: run properties become paired attribute
//! codes, paragraph alignment becomes `[Just:…]`, `\page` becomes `[HPg]`,
//! tables are flattened (Tab between cells, HRt after each row), footnotes
//! become `Code::Note` and the primary header/footer `Code::HeadFoot`.
//!
//! Written from the RTF 1.9.1 specification.

use crate::doc::*;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Windows code pages (also used by the .doc reader)

const CP1252_80: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8d}Ž\u{8f}\u{90}‘’“”•–—˜™š›œ\u{9d}žŸ";
const CP1251_80: &str = "ЂЃ‚ѓ„…†‡€‰Љ‹ЊЌЋЏђ‘’“”•–—\u{98}™љ›њќћџ\u{a0}ЎўЈ¤Ґ¦§Ё©Є«¬\u{ad}®Ї°±Ііґµ¶·ё№є»јЅѕї";
const CP1250_80: &str = "€\u{81}‚\u{83}„…†‡\u{88}‰Š‹ŚŤŽŹ\u{90}‘’“”•–—\u{98}™š›śťžź\u{a0}ˇ˘Ł¤Ą¦§¨©Ş«¬\u{ad}®Ż°±˛ł´µ¶·¸ąş»Ľ˝ľżŔÁÂĂÄĹĆÇČÉĘËĚÍÎĎĐŃŇÓÔŐÖ×ŘŮÚŰÜÝŢßŕáâăäĺćçčéęëěíîďđńňóôőö÷řůúűüýţ˙";

fn nth_char(s: &str, i: usize) -> char {
    s.chars().nth(i).unwrap_or('\u{fffd}')
}

/// Decode one byte of a single-byte Windows/DOS code page.
/// Supported: 1250 (Central European), 1251 (Cyrillic), 1252 (Western,
/// also the fallback for anything unknown), 437 and 850 (DOS).
pub fn decode_byte(b: u8, cp: u32) -> char {
    if b < 0x80 {
        return b as char;
    }
    let i = (b - 0x80) as usize;
    match cp {
        1251 => {
            if b >= 0xC0 {
                char::from_u32(0x410 + (b as u32 - 0xC0)).unwrap_or('?')
            } else {
                nth_char(CP1251_80, i)
            }
        }
        1250 => nth_char(CP1250_80, i),
        437 => nth_char(crate::fileio::CP437_HIGH, i),
        850 => nth_char(crate::fileio::CP850_HIGH, i),
        _ => {
            if b < 0xA0 {
                nth_char(CP1252_80, i)
            } else {
                b as char
            }
        }
    }
}

/// Map a character of a symbol font (Symbol, Wingdings) to Unicode.
/// `c` is the code in the font (0x20..0xFF, or the 0xF020..0xF0FF private
/// use form).  Returns None for fonts that are not symbol fonts and for
/// ordinary characters.
pub fn symbol_char(font: &str, c: u32) -> Option<char> {
    let private = (0xF020..=0xF0FF).contains(&c);
    let code = if private { c - 0xF000 } else { c };
    if code > 0xFF {
        return None;
    }
    let f = font.to_ascii_lowercase();
    let mapped = if f.starts_with("symbol") {
        // the complete Symbol font encoding
        const LOW: &str = " !∀#∃%&∋()∗+,−./0123456789:;<=>?≅ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ[∴]⊥_‾αβχδεφγηιϕκλμνοπθρστυϖωξψζ{|}∼";
        const HIGH: &str = "€ϒ′≤⁄∞ƒ♣♦♥♠↔←↑→↓°±″≥×∝∂•÷≠≡≈…⏐⎯↵ℵℑℜ℘⊗⊕∅∩∪⊃⊇⊄⊂⊆∈∉∠∇®©™∏√⋅¬∧∨⇔⇐⇑⇒⇓◊〈®©™∑⎛⎜⎝⎡⎢⎣⎧⎨⎩⎪〉∫⌠⎮⌡⎞⎟⎠⎤⎥⎦⎫⎬⎭";
        match code {
            0x20..=0x7E => LOW.chars().nth((code - 0x20) as usize),
            0xA0..=0xFE => HIGH.chars().nth((code - 0xA0) as usize).filter(|&c| c != '\u{f8ff}'),
            _ => None,
        }
    } else if code <= 0x20 || code == 0xA0 {
        // spaces stay spaces in every font
        None
    } else if f.starts_with("wingdings") || f.starts_with("webdings") || f.starts_with("zapf") {
        Some(match code {
            0x6C | 0x9F | 0xA1 | 0xA4 => '●',
            0x6E | 0x71 | 0x72 | 0x6F | 0x70 => '■',
            0xA7 => '▪',
            0xA8 => '□',
            0x75 | 0x76 | 0x77 => '◆',
            0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xD7 | 0xDF => '►',
            0xFC | 0xFE => '√',
            0xFB | 0xFD => '×',
            _ => '•',
        })
    } else {
        None
    };
    match mapped {
        Some(m) => Some(m),
        None if private => char::from_u32(code),
        None => None,
    }
}

pub fn is_symbol_font(name: &str) -> bool {
    let f = name.to_ascii_lowercase();
    ["symbol", "wingdings", "webdings", "zapf"].iter().any(|p| f.starts_with(p))
}

/// RTF/Windows font charset → code page.
pub fn charset_cp(charset: i32) -> Option<u32> {
    Some(match charset {
        0 => 1252,
        204 => 1251,
        238 => 1250,
        255 => 850,
        // others (Greek, Turkish, Baltic, Asian) fall back to the document
        // code page
        _ => return None,
    })
}

// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Dest {
    Text,
    Skip,
    FontTbl,
    RevTbl,
}

#[derive(Clone)]
struct GState {
    b: bool,
    i: bool,
    ul: u8, // 0 none, 1 single, 2 double
    strike: bool,
    scaps: bool,
    caps: bool,
    sup: bool,
    sub: bool,
    outl: bool,
    shad: bool,
    hidden: bool,
    rev_ins: bool,
    rev_del: bool,
    auth: i32,
    auth_del: i32,
    dttm: i32,
    dttm_del: i32,
    f: Option<i32>,
    fs: u32,
    uc: u32,
    just: Justify,
    intbl: bool,
    dest: Dest,
}

impl GState {
    fn new() -> GState {
        GState {
            b: false,
            i: false,
            ul: 0,
            strike: false,
            scaps: false,
            caps: false,
            sup: false,
            sub: false,
            outl: false,
            shad: false,
            hidden: false,
            rev_ins: false,
            rev_del: false,
            auth: 0,
            auth_del: 0,
            dttm: 0,
            dttm_del: 0,
            f: None,
            fs: 24,
            uc: 1,
            just: Justify::Left,
            intbl: false,
            dest: Dest::Text,
        }
    }
    fn plain(&mut self) {
        let keep = (self.uc, self.just, self.intbl, self.dest);
        *self = GState::new();
        (self.uc, self.just, self.intbl, self.dest) = keep;
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TKind {
    Main,
    Note(NoteKind),
    HeadFoot(u8),
}

struct Target {
    els: Vec<El>,
    attrs: AttrSet,
    font: FontSpec,
    just: Justify,
    para_started: bool,
    pending_tab: bool,
    depth: usize,
    kind: TKind,
    tbl: TableState,
    rev: Option<(bool, String, String)>,
}

fn sync_rev(t: &mut Target, want: Option<(bool, String, String)>) {
    if t.rev == want {
        return;
    }
    if let Some((del, _, _)) = t.rev.take() {
        t.els.push(El::Code(Code::RevOff { del }));
    }
    if let Some((del, author, date)) = want {
        t.els.push(El::Code(Code::RevOn { del, author: author.clone(), date: date.clone() }));
        t.rev = Some((del, author, date));
    }
}

/// Table building state shared by the RTF and .doc readers: produces
/// `[Tbl Def][Row][Cell]…[Cell]…[Row]…[Tbl Off]` like the .docx reader.
#[derive(Default)]
pub(crate) struct TableState {
    pub in_tbl: bool,
    pub def_at: usize,
    pub need_row: bool,
    pub pending_cell: bool,
}

impl TableState {
    /// Make sure a table is open (when `intbl`) or closed (when not), and
    /// that the current cell's codes have been written.
    pub fn sync(&mut self, els: &mut Vec<El>, intbl: bool, cols: &[i32], text_width: i32) {
        if intbl && !self.in_tbl {
            if !matches!(els.last(), None | Some(El::Code(Code::HRt)) | Some(El::Code(Code::HPg))) {
                els.push(El::Code(Code::HRt));
            }
            self.def_at = els.len();
            els.push(El::Code(Code::TblDef { cols: cols.to_vec() }));
            self.in_tbl = true;
            self.need_row = true;
            self.pending_cell = false;
        } else if !intbl && self.in_tbl {
            self.close(els, text_width);
        }
        if self.in_tbl {
            if self.need_row {
                els.push(El::Code(Code::Row));
                els.push(El::Code(Code::Cell));
                self.need_row = false;
                self.pending_cell = false;
            } else if self.pending_cell {
                els.push(El::Code(Code::Cell));
                self.pending_cell = false;
            }
        }
    }

    /// End of a cell: the next content starts a new cell.
    pub fn cell_end(&mut self) {
        if self.in_tbl {
            self.pending_cell = true;
        }
    }

    /// End of a row; `cols` are the row's column widths (used when the
    /// table definition has none yet).
    pub fn row_end(&mut self, els: &mut [El], cols: &[i32]) {
        if !self.in_tbl {
            return;
        }
        self.pending_cell = false;
        self.need_row = true;
        if let Some(El::Code(Code::TblDef { cols: c })) = els.get_mut(self.def_at) {
            if c.len() < cols.len() {
                *c = cols.to_vec();
            }
        }
    }

    pub fn close(&mut self, els: &mut Vec<El>, text_width: i32) {
        if !self.in_tbl {
            return;
        }
        self.in_tbl = false;
        self.need_row = false;
        self.pending_cell = false;
        els.push(El::Code(Code::TblOff));
        let ncols = crate::tables::parse(els, self.def_at).map(|t| t.rows.iter().map(|r| r.len()).max().unwrap_or(1)).unwrap_or(1).max(1);
        if let Some(El::Code(Code::TblDef { cols })) = els.get_mut(self.def_at) {
            cols.retain(|w| *w > 0);
            if cols.len() < ncols {
                let w = (text_width / ncols as i32).max(300);
                cols.resize(ncols, w);
            }
        }
    }
}

/// Column widths (WPU) from cell right boundaries in twips.
pub(crate) fn widths_from_bounds(bounds: &[i32], left: i32) -> Vec<i32> {
    let mut prev = left;
    let mut out = vec![];
    for &b in bounds {
        let w = ((b as i64 - prev as i64) * 1200 / 1440).clamp(120, 1200 * 40) as i32;
        out.push(w);
        prev = b;
    }
    out
}

impl Target {
    fn new(kind: TKind, depth: usize, font: FontSpec) -> Target {
        Target { els: vec![], attrs: AttrSet::default(), font, just: Justify::Left, para_started: false, pending_tab: false, depth, kind, tbl: TableState::default(), rev: None }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    pos: usize,
    stack: Vec<GState>,
    st: GState,
    targets: Vec<Target>,
    fonts: HashMap<i32, (String, Option<u32>)>,
    ansicpg: u32,
    deff: i32,
    base_font: FontSpec,
    init: InitialFormat,
    skip_chars: u32,
    font_def: Option<i32>,
    font_charset: i32,
    font_name: String,
    hf: Vec<El>,
    pending_high: Option<u16>,
    last_sect_hpg: Option<usize>,
    star: bool,
    cellx: Vec<i32>,
    trleft: i32,
    rev_authors: Vec<String>,
    rev_name: String,
}

const SKIP_DESTS: &[&str] = &[
    "colortbl", "stylesheet", "info", "pict", "object", "themedata", "colorschememapping", "latentstyles", "datastore", "listtable",
    "listoverridetable", "rsidtbl", "generator", "xmlnstbl", "mmathPr", "fldinst", "nonshppict", "shp", "shpinst", "pgdsctbl",
    "filetbl", "userprops", "docvar", "wgrffmtfilter", "ftnsep", "ftnsepc", "ftncn", "aftnsep", "aftnsepc", "aftncn", "panose", "falt",
    "bkmkstart", "bkmkend", "template", "comment", "title", "subject", "author", "operator", "keywords", "doccomm", "company", "txe", "xe",
    "tc", "atnid", "atnauthor", "annotation", "protusertbl", "headerl", "headerf", "footerl", "footerf", "fchars", "lchars", "pn",
    "listpicture", "blipuid", "macpict", "wmetafile", "dibitmap", "wbitmap", "do", "pnseclvl", "hl", "oldcprops", "oldpprops",
    "oldsprops", "oldtprops", "factoidname", "fontemb", "fontfile", "objdata", "objclass", "objname", "private", "rxe", "tcn",
    "xform", "atrfstart", "atrfend", "atndate", "atnicn", "atnparent", "atnref", "atntime", "category", "manager", "hlinkbase",
    "buptim", "creatim", "printim", "revtim", "ud", "upr", "background", "sp", "sn", "sv", "nesttableprops", "listtext_skip",
];

impl<'a> Parser<'a> {
    fn cur_cp(&self) -> u32 {
        self.st.f.and_then(|f| self.fonts.get(&f)).and_then(|x| x.1).unwrap_or(self.ansicpg)
    }

    fn cur_font_name(&self) -> String {
        self.st.f.or(Some(self.deff)).and_then(|f| self.fonts.get(&f)).map(|x| x.0.clone()).unwrap_or_default()
    }

    fn target(&mut self) -> &mut Target {
        // targets is never empty (Main is pushed first and never popped)
        let n = self.targets.len();
        &mut self.targets[n - 1]
    }

    fn want_font(&self) -> FontSpec {
        let name = self
            .st
            .f
            .or(Some(self.deff))
            .and_then(|f| self.fonts.get(&f))
            .map(|x| x.0.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.base_font.name.clone());
        FontSpec { name, size_cpt: self.st.fs.clamp(2, 3200) * 50 }
    }

    fn want_attrs(&self) -> AttrSet {
        let s = &self.st;
        let mut w = AttrSet::default();
        w.set(Attr::Bold, s.b);
        w.set(Attr::Italic, s.i);
        w.set(Attr::Underline, s.ul == 1);
        w.set(Attr::DblUnderline, s.ul == 2);
        w.set(Attr::Strikeout, s.strike);
        w.set(Attr::SmallCaps, s.scaps);
        w.set(Attr::Superscript, s.sup);
        w.set(Attr::Subscript, s.sub);
        w.set(Attr::Outline, s.outl);
        w.set(Attr::Shadow, s.shad);
        w
    }

    /// Prepare the target for content: pending table tab, paragraph
    /// justification, attribute and font codes.
    fn prepare(&mut self, sync: bool) {
        let want = self.want_attrs();
        let font = self.want_font();
        let just = self.st.just;
        let intbl = self.st.intbl;
        self.table_sync();
        let rev = self.want_rev();
        let t = self.target();
        sync_rev(t, rev);
        if t.pending_tab {
            t.pending_tab = false;
            t.els.push(El::Code(Code::Tab));
        }
        if !t.para_started {
            t.para_started = true;
            if !intbl && just != t.just {
                t.els.push(El::Code(Code::Just(just)));
                t.just = just;
            }
        }
        if !sync {
            return;
        }
        sync_attrs(&mut t.els, &mut t.attrs, &mut t.font, want, font);
    }

    fn emit_char(&mut self, c: char) {
        if self.st.dest != Dest::Text || self.st.hidden {
            return;
        }
        if (c as u32) < 0x20 && c != '\t' || c == '\u{200b}' || c == '\u{feff}' {
            return;
        }
        self.prepare(true);
        let caps = self.st.caps;
        let t = self.target();
        match c {
            '\t' => t.els.push(El::Code(Code::Tab)),
            '\u{a0}' => t.els.push(El::Code(Code::HardSpace)),
            '\u{ad}' => t.els.push(El::Code(Code::SoftHyphen)),
            '\u{2011}' => t.els.push(El::Code(Code::HardHyphen)),
            c if caps => t.els.extend(c.to_uppercase().map(El::Ch)),
            c => t.els.push(El::Ch(c)),
        }
    }

    fn emit_code(&mut self, code: Code, sync: bool) {
        if self.st.dest != Dest::Text || self.st.hidden {
            return;
        }
        self.prepare(sync);
        self.target().els.push(El::Code(code));
    }

    fn want_rev(&self) -> Option<(bool, String, String)> {
        let s = &self.st;
        let (del, auth, dttm) = if s.rev_del {
            (true, s.auth_del, s.dttm_del)
        } else if s.rev_ins {
            (false, s.auth, s.dttm)
        } else {
            return None;
        };
        let author = usize::try_from(auth).ok().and_then(|a| self.rev_authors.get(a)).cloned().unwrap_or_default();
        let d = dttm as u32;
        let date = if d == 0 {
            String::new()
        } else {
            format!("{:04}-{:02}-{:02}T{:02}:{:02}:00Z", 1900 + ((d >> 20) & 0x1FF), (d >> 16) & 0xF, (d >> 11) & 0x1F, (d >> 6) & 0x1F, d & 0x3F)
        };
        Some((del, author, date))
    }

    fn text_width(&self) -> i32 {
        self.init.page_width - self.init.left - self.init.right
    }

    fn table_sync(&mut self) {
        let intbl = self.st.intbl;
        let cols = widths_from_bounds(&self.cellx, self.trleft);
        let tw = self.text_width();
        let t = self.target();
        t.tbl.sync(&mut t.els, intbl, &cols, tw);
    }

    fn para_end(&mut self, code: Code) {
        if self.st.dest != Dest::Text {
            return;
        }
        self.table_sync();
        let rev = self.want_rev();
        let t = self.target();
        if t.pending_tab {
            t.pending_tab = false;
            t.els.push(El::Code(Code::Tab));
        }
        sync_rev(t, rev);
        t.els.push(El::Code(code));
        t.para_started = false;
    }

    fn text_byte(&mut self, byte: u8) {
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return;
        }
        match self.st.dest {
            Dest::Skip => {}
            Dest::RevTbl => {
                if byte == b';' {
                    let n = std::mem::take(&mut self.rev_name);
                    self.rev_authors.push(n.trim().to_string());
                } else {
                    self.rev_name.push(decode_byte(byte, self.ansicpg));
                }
            }
            Dest::FontTbl => {
                if byte == b';' {
                    self.finish_font();
                } else {
                    let c = decode_byte(byte, self.ansicpg);
                    self.font_name.push(c);
                }
            }
            Dest::Text => {
                if byte == b'\r' || byte == b'\n' || byte == 0 {
                    return;
                }
                let c = decode_byte(byte, self.cur_cp());
                let c = if byte >= 0x20 { symbol_char(&self.cur_font_name(), byte as u32).unwrap_or(c) } else { c };
                self.pending_high = None;
                self.emit_char(c);
            }
        }
    }

    fn finish_font(&mut self) {
        if let Some(f) = self.font_def.take() {
            let name = self.font_name.trim().to_string();
            self.fonts.insert(f, (name, charset_cp(self.font_charset)));
        }
        self.font_name.clear();
        self.font_charset = 0;
    }

    fn open_group(&mut self) {
        self.skip_chars = 0;
        if self.stack.len() > 4000 {
            // absurd nesting: treat the rest as skipped
            self.st.dest = Dest::Skip;
        }
        self.stack.push(self.st.clone());
        self.star = false;
    }

    fn close_group(&mut self) {
        self.skip_chars = 0;
        self.star = false;
        if self.st.dest == Dest::FontTbl {
            if self.font_def.is_some() {
                self.finish_font();
            }
        }
        let was_fonttbl = self.st.dest == Dest::FontTbl;
        if let Some(s) = self.stack.pop() {
            self.st = s;
        }
        if was_fonttbl && self.st.dest != Dest::FontTbl {
            // font table complete: establish the base font
            if let Some((n, _)) = self.fonts.get(&self.deff) {
                if !n.is_empty() {
                    self.base_font.name = n.clone();
                    if let Some(t) = self.targets.first_mut() {
                        if t.els.is_empty() {
                            t.font = self.base_font.clone();
                        }
                    }
                }
            }
        }
        // finished a note / header / footer?
        while self.targets.len() > 1 && self.stack.len() < self.targets[self.targets.len() - 1].depth {
            let mut t = self.targets.pop().unwrap();
            sync_rev(&mut t, None);
            let tw = self.text_width();
            t.tbl.close(&mut t.els, tw);
            let els = finish_els(t.els, t.attrs);
            match t.kind {
                TKind::Note(kind) => {
                    let mut els = els;
                    // drop the space after the reference mark
                    if let Some(p) = els.iter().position(|e| e.is_char()) {
                        if els[p] == El::Ch(' ') {
                            els.remove(p);
                        }
                    }
                    while matches!(els.last(), Some(El::Code(Code::HRt))) {
                        els.pop();
                    }
                    self.emit_code(Code::Note { kind, els }, false);
                }
                TKind::HeadFoot(which) => {
                    let mut els = els;
                    while matches!(els.last(), Some(El::Code(Code::HRt))) {
                        els.pop();
                    }
                    if !els.is_empty() && !self.hf.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) {
                        self.hf.push(El::Code(Code::HeadFoot { which, occ: Occurrence::Every, els }));
                    }
                }
                TKind::Main => {}
            }
        }
    }

    fn push_target(&mut self, kind: TKind) {
        let depth = self.stack.len();
        let font = self.base_font.clone();
        self.targets.push(Target::new(kind, depth, font));
    }

    fn control(&mut self, word: &str, param: Option<i32>) {
        let star = std::mem::take(&mut self.star);
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return;
        }
        let p = param;
        let on = p.map(|v| v != 0).unwrap_or(true);
        if word == "bin" {
            let n = p.unwrap_or(0).max(0) as usize;
            self.pos = self.pos.saturating_add(n).min(self.b.len());
            return;
        }
        if self.st.dest == Dest::Skip {
            return;
        }
        if self.st.dest == Dest::RevTbl {
            if star {
                self.st.dest = Dest::Skip;
            }
            return;
        }
        if self.st.dest == Dest::FontTbl {
            match word {
                "f" => {
                    if self.font_def.is_some() {
                        self.finish_font();
                    }
                    self.font_def = p;
                    self.font_name.clear();
                }
                "fcharset" => self.font_charset = p.unwrap_or(0),
                "u" => {
                    if let Some(c) = char::from_u32(p.unwrap_or(0) as u16 as u32) {
                        self.font_name.push(c);
                    }
                    self.skip_chars = self.st.uc;
                }
                _ if star || SKIP_DESTS.contains(&word) => self.st.dest = Dest::Skip,
                _ => {}
            }
            return;
        }
        match word {
            // ---- document
            "ansicpg" => self.ansicpg = p.unwrap_or(1252).max(0) as u32,
            "ansi" => self.ansicpg = 1252,
            "pc" => self.ansicpg = 437,
            "pca" => self.ansicpg = 850,
            "mac" => self.ansicpg = 1252,
            "deff" => self.deff = p.unwrap_or(0),
            "uc" => self.st.uc = p.unwrap_or(1).clamp(0, 10) as u32,
            "revtbl" => {
                self.st.dest = Dest::RevTbl;
                self.rev_authors.clear();
                self.rev_name.clear();
            }
            "revised" => self.st.rev_ins = on,
            "deleted" => self.st.rev_del = on,
            "revauth" => self.st.auth = p.unwrap_or(0),
            "revauthdel" => self.st.auth_del = p.unwrap_or(0),
            "revdttm" => self.st.dttm = p.unwrap_or(0),
            "revdttmdel" => self.st.dttm_del = p.unwrap_or(0),
            "fonttbl" => {
                self.st.dest = Dest::FontTbl;
                self.font_def = None;
                self.font_name.clear();
            }
            "paperw" => set_pos(&mut self.init.page_width, p),
            "paperh" => set_pos(&mut self.init.page_height, p),
            "margl" => set_pos(&mut self.init.left, p),
            "margr" => set_pos(&mut self.init.right, p),
            "margt" => set_pos(&mut self.init.top, p),
            "margb" => set_pos(&mut self.init.bottom, p),
            // ---- destinations
            "footnote" => {
                self.push_target(TKind::Note(NoteKind::Foot));
                self.st.plain();
                self.st.just = Justify::Left;
                self.st.intbl = false;
            }
            "ftnalt" => {
                if let Some(t) = self.targets.last_mut() {
                    if t.kind == TKind::Note(NoteKind::Foot) {
                        t.kind = TKind::Note(NoteKind::End);
                    }
                }
            }
            "header" | "headerr" | "footer" | "footerr" => {
                let which = if word.starts_with("header") { 0 } else { 2 };
                if self.targets.len() > 1 || self.hf.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: w, .. }) if *w == which)) {
                    self.st.dest = Dest::Skip;
                } else {
                    self.push_target(TKind::HeadFoot(which));
                    self.st.just = Justify::Left;
                    self.st.intbl = false;
                }
            }
            "fldrslt" | "field" | "listtext" | "pntext" | "shptxt" => {}
            _ if star => self.st.dest = Dest::Skip,
            w if SKIP_DESTS.contains(&w) => self.st.dest = Dest::Skip,
            // ---- character formatting
            "plain" => self.st.plain(),
            "b" => self.st.b = on,
            "i" => self.st.i = on,
            "ul" | "ulw" | "uld" | "uldash" | "uldashd" | "uldashdd" | "ulth" | "ulwave" | "ulhwave" | "ulldash" | "ulthd" | "ulthdash"
            | "ulthdashd" | "ulthdashdd" | "ulthldash" | "ululdbwave" => self.st.ul = if on { 1 } else { 0 },
            "uldb" => self.st.ul = if on { 2 } else { 0 },
            "ulnone" => self.st.ul = 0,
            "strike" | "striked" => self.st.strike = on,
            "scaps" => self.st.scaps = on,
            "caps" => self.st.caps = on,
            "outl" => self.st.outl = on,
            "shad" => self.st.shad = on,
            "v" => self.st.hidden = on,
            "super" => {
                self.st.sup = on;
                if on {
                    self.st.sub = false
                }
            }
            "sub" => {
                self.st.sub = on;
                if on {
                    self.st.sup = false
                }
            }
            "nosupersub" => {
                self.st.sup = false;
                self.st.sub = false;
            }
            "f" => self.st.f = p,
            "fs" => self.st.fs = p.unwrap_or(24).clamp(2, 3200) as u32,
            // ---- paragraph formatting
            "pard" => {
                self.st.just = Justify::Left;
                self.st.intbl = false;
            }
            "ql" => self.st.just = Justify::Left,
            "qc" => self.st.just = Justify::Center,
            "qr" => self.st.just = Justify::Right,
            "qj" | "qd" => self.st.just = Justify::Full,
            "intbl" => self.st.intbl = true,
            // ---- special characters
            "par" => self.para_end(Code::HRt),
            "line" => self.para_end(Code::HRt),
            "page" => self.para_end(Code::HPg),
            "sect" => {
                self.para_end(Code::HPg);
                if self.targets.len() == 1 {
                    self.last_sect_hpg = Some(self.targets[0].els.len().saturating_sub(1));
                }
            }
            "sbknone" => {
                if let Some(ix) = self.last_sect_hpg.take() {
                    if self.targets.len() == 1 && ix + 1 == self.targets[0].els.len() && self.targets[0].els[ix] == El::Code(Code::HPg) {
                        self.targets[0].els[ix] = El::Code(Code::HRt);
                    }
                }
            }
            "tab" => self.emit_code(Code::Tab, true),
            "trowd" => {
                self.cellx.clear();
                self.trleft = 0;
            }
            "trleft" => self.trleft = p.unwrap_or(0),
            "cellx" => {
                if self.cellx.len() < 64 {
                    self.cellx.push(p.unwrap_or(0));
                }
            }
            "cell" => {
                if self.st.dest == Dest::Text {
                    if !self.st.intbl {
                        // a cell without \intbl: still a table
                        self.st.intbl = true;
                    }
                    self.prepare(false);
                    let t = self.target();
                    t.tbl.cell_end();
                    t.para_started = false;
                }
            }
            "row" => {
                if self.st.dest == Dest::Text {
                    let cols = widths_from_bounds(&self.cellx, self.trleft);
                    let t = self.target();
                    t.tbl.row_end(&mut t.els, &cols);
                    t.para_started = false;
                }
            }
            "nestcell" => self.emit_code(Code::Tab, false),
            "nestrow" => self.para_end(Code::HRt),
            "emdash" => self.emit_char('—'),
            "endash" => self.emit_char('–'),
            "bullet" => self.emit_char('•'),
            "lquote" => self.emit_char('‘'),
            "rquote" => self.emit_char('’'),
            "ldblquote" => self.emit_char('“'),
            "rdblquote" => self.emit_char('”'),
            "emspace" | "enspace" | "qmspace" => self.emit_char(' '),
            "u" => {
                let v = p.unwrap_or(0);
                let v = if v < 0 { (v + 65536) as u32 } else { v as u32 };
                let u = v as u16;
                if (0xD800..0xDC00).contains(&u) {
                    self.pending_high = Some(u);
                } else if (0xDC00..0xE000).contains(&u) {
                    if let Some(h) = self.pending_high.take() {
                        let c = 0x10000 + (((h as u32) - 0xD800) << 10) + (u as u32 - 0xDC00);
                        if let Some(c) = char::from_u32(c) {
                            self.emit_char(c);
                        }
                    }
                } else {
                    self.pending_high = None;
                    let c = symbol_char(&self.cur_font_name(), v).map(|c| c as u32).unwrap_or(v);
                    if let Some(c) = char::from_u32(c) {
                        self.emit_char(c);
                    }
                }
                self.skip_chars = self.st.uc;
            }
            _ => {}
        }
    }

    fn symbol(&mut self, c: u8) {
        match c {
            b'*' => {
                self.star = true;
                return;
            }
            b'\'' => {
                let h = |x: u8| (x as char).to_digit(16);
                let a = self.b.get(self.pos).copied().and_then(h);
                let b = self.b.get(self.pos + 1).copied().and_then(h);
                if let (Some(a), Some(b)) = (a, b) {
                    self.pos += 2;
                    self.text_byte((a * 16 + b) as u8);
                }
                return;
            }
            _ => {}
        }
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return;
        }
        match c {
            b'\\' | b'{' | b'}' => self.text_byte(c),
            b'~' => self.emit_code(Code::HardSpace, true),
            b'-' => self.emit_code(Code::SoftHyphen, true),
            b'_' => self.emit_code(Code::HardHyphen, true),
            b'\r' | b'\n' => self.para_end(Code::HRt),
            b'\t' => self.emit_code(Code::Tab, true),
            _ => {}
        }
    }

    fn run(&mut self) {
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            self.pos += 1;
            match c {
                b'{' => self.open_group(),
                b'}' => self.close_group(),
                b'\\' => {
                    let Some(&n) = self.b.get(self.pos) else { break };
                    if n.is_ascii_alphabetic() {
                        let s = self.pos;
                        while self.pos < self.b.len() && self.b[self.pos].is_ascii_alphabetic() && self.pos - s < 32 {
                            self.pos += 1;
                        }
                        let word = String::from_utf8_lossy(&self.b[s..self.pos]).into_owned();
                        let mut param: Option<i32> = None;
                        let neg = self.b.get(self.pos) == Some(&b'-') && self.b.get(self.pos + 1).is_some_and(|d| d.is_ascii_digit());
                        if neg {
                            self.pos += 1;
                        }
                        let ds = self.pos;
                        while self.pos < self.b.len() && self.b[self.pos].is_ascii_digit() && self.pos - ds < 10 {
                            self.pos += 1;
                        }
                        if self.pos > ds {
                            let v: i64 = std::str::from_utf8(&self.b[ds..self.pos]).ok().and_then(|x| x.parse().ok()).unwrap_or(0);
                            let v = if neg { -v } else { v };
                            param = Some(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
                        }
                        if self.b.get(self.pos) == Some(&b' ') {
                            self.pos += 1;
                        }
                        self.control(&word, param);
                    } else {
                        self.pos += 1;
                        self.symbol(n);
                    }
                }
                _ => self.text_byte(c),
            }
        }
        // unbalanced file: close everything that is still open
        while !self.stack.is_empty() {
            self.close_group();
        }
    }
}

fn set_pos(v: &mut i32, p: Option<i32>) {
    if let Some(t) = p {
        if t > 0 && t < 1440 * 60 {
            *v = (t as i64 * 1200 / 1440) as i32;
        }
    } else {
    }
}

/// Bring attributes and font in `els` from `cur` to `want` (closing in
/// reverse order, font, opening), exactly like the .docx reader.
pub(crate) fn sync_attrs(els: &mut Vec<El>, cur: &mut AttrSet, cur_font: &mut FontSpec, want: AttrSet, font: FontSpec) {
    // Symbol-font characters are already mapped to Unicode: keep the
    // surrounding text font instead of switching to Symbol/Wingdings.
    let font = if is_symbol_font(&font.name) { FontSpec { name: cur_font.name.clone(), size_cpt: font.size_cpt } } else { font };
    for a in Attr::ALL.iter().rev() {
        if cur.has(*a) && !want.has(*a) {
            els.push(El::Code(Code::AttrOff(*a)));
            cur.set(*a, false);
        }
    }
    if font != *cur_font {
        els.push(El::Code(Code::Font(font.clone())));
        *cur_font = font;
    }
    for a in Attr::ALL {
        if !cur.has(a) && want.has(a) {
            els.push(El::Code(Code::AttrOn(a)));
            cur.set(a, true);
        }
    }
}

/// Close open attributes, collapse `[HPg][HRt]`, normalize.
pub(crate) fn finish_els(mut els: Vec<El>, attrs: AttrSet) -> Vec<El> {
    for a in Attr::ALL.iter().rev() {
        if attrs.has(*a) {
            els.push(El::Code(Code::AttrOff(*a)));
        }
    }
    let mut out: Vec<El> = Vec::with_capacity(els.len());
    for e in els {
        if e == El::Code(Code::HRt) && out.last() == Some(&El::Code(Code::HPg)) {
            continue;
        }
        out.push(e);
    }
    let mut d = Document { els: out, ..Default::default() };
    d.normalize_attrs();
    d.els
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if !bytes.starts_with(b"{\\rtf") {
        return Err("not an RTF file".into());
    }
    let mut init = InitialFormat::default();
    init.justify = Justify::Left;
    let base_font = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    let mut p = Parser {
        b: bytes,
        pos: 0,
        stack: vec![],
        st: GState::new(),
        targets: vec![Target::new(TKind::Main, 0, base_font.clone())],
        fonts: HashMap::new(),
        ansicpg: 1252,
        deff: 0,
        base_font,
        init,
        skip_chars: 0,
        font_def: None,
        font_charset: 0,
        font_name: String::new(),
        hf: vec![],
        pending_high: None,
        last_sect_hpg: None,
        star: false,
        cellx: vec![],
        trleft: 0,
        rev_authors: vec![],
        rev_name: String::new(),
    };
    p.run();
    let mut main = p.targets.swap_remove(0);
    sync_rev(&mut main, None);
    let tw = p.text_width();
    main.tbl.close(&mut main.els, tw);
    let mut els = finish_els(main.els, main.attrs);
    if matches!(els.last(), Some(El::Code(Code::HRt))) {
        els.pop();
    }
    let mut d = Document::new();
    d.init = p.init;
    d.init.font = p.base_font;
    d.init.justify = Justify::Left;
    if let Some(El::Code(Code::Just(j))) = els.first().cloned() {
        d.init.justify = j;
        els.remove(0);
    }
    d.els = p.hf;
    d.els.extend(els);
    Ok(d)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn text(d: &Document) -> String {
        d.plain_text(0, d.len())
    }

    /// "a|b/c|d" for the first table in `els`.
    pub(crate) fn table_text(els: &[El]) -> String {
        let Some(s) = els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))) else { return String::new() };
        let mut out = String::new();
        let mut first_row = true;
        let mut first_cell = true;
        for e in &els[s + 1..] {
            match e {
                El::Code(Code::Row) => {
                    if !first_row {
                        out.push('/');
                    }
                    first_row = false;
                    first_cell = true;
                }
                El::Code(Code::Cell) => {
                    if !first_cell {
                        out.push('|');
                    }
                    first_cell = false;
                }
                El::Code(Code::TblOff) => break,
                El::Code(Code::HRt) => out.push('~'),
                El::Ch(c) => out.push(*c),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn codepage_tables() {
        assert_eq!(CP1252_80.chars().count(), 32);
        assert_eq!(CP1251_80.chars().count(), 64);
        assert_eq!(CP1250_80.chars().count(), 128);
        assert_eq!(decode_byte(0xCF, 1251), 'П');
        assert_eq!(decode_byte(0xFF, 1251), 'я');
        assert_eq!(decode_byte(0xA8, 1251), 'Ё');
        assert_eq!(decode_byte(0xFC, 1252), 'ü');
        assert_eq!(decode_byte(0x80, 1252), '€');
        assert_eq!(decode_byte(0xB9, 1250), 'ą');
    }

    #[test]
    fn basic_formatting() {
        let r = br"{\rtf1\ansi\deff0{\fonttbl{\f0 Arial;}}\pard\qc {\b Fett}\par\pard Gr\'fc\'dfe {\i kursiv}\tab x\par}";
        let d = read(r).unwrap();
        assert_eq!(text(&d), "Fett\nGrüße kursiv\tx");
        assert_eq!(d.init.justify, Justify::Center);
        assert_eq!(d.init.font.name, "Arial");
        assert_eq!(d.els[0], El::Code(Code::AttrOn(Attr::Bold)));
        assert!(d.els.contains(&El::Code(Code::Just(Justify::Left))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
    }

    #[test]
    fn cyrillic_codepages() {
        let r = br"{\rtf1\ansi\ansicpg1251 \'cf\'f0\'e8\'e2\'e5\'f2}";
        assert_eq!(text(&read(r).unwrap()), "Привет");
        let r = br"{\rtf1\ansi{\fonttbl{\f0 Times;}{\f1\fcharset204 Arial Cyr;}}A {\f1 \'cc\'e8\'f0} \u1044\'3f\uc0\u1045 }";
        assert_eq!(text(&read(r).unwrap()), "A Мир ДЕ");
    }

    #[test]
    fn footnote_header_table() {
        let r = br"{\rtf1{\header \pard\qc Kopf\par}Text{\super\chftn}{\footnote\pard\plain{\super\chftn} Note.}\par\trowd\cellx1440\cellx2880\pard\intbl A\cell B\cell\row\pard Ende\page Zwei}";
        let d = read(r).unwrap();
        assert!(matches!(&d.els[0], El::Code(Code::HeadFoot { which: 0, els, .. }) if els.iter().filter(|e| e.is_char()).count() == 4));
        let note = d.els.iter().find_map(|e| match e {
            El::Code(Code::Note { kind: NoteKind::Foot, els }) => Some(els.clone()),
            _ => None,
        });
        let note = note.expect("footnote");
        assert_eq!(Document { els: note, ..Default::default() }.plain_text(0, 99), "Note.");
        let t = d.els.iter().position(|e| matches!(e, El::Code(Code::TblDef { .. }))).expect("table");
        let tb = crate::tables::parse(&d.els, t).unwrap();
        assert_eq!(tb.rows.len(), 1);
        assert_eq!(tb.cols.len(), 2);
        assert!(matches!(&d.els[t], El::Code(Code::TblDef { cols }) if cols == &vec![1200, 1200]), "{:?}", &d.els[t]);
        let tail: Vec<El> = d.els[t + 1..].to_vec();
        let want: Vec<El> = vec![El::Code(Code::Row), El::Code(Code::Cell), El::Ch('A'), El::Code(Code::Cell), El::Ch('B'), El::Code(Code::TblOff), El::Ch('E')];
        assert_eq!(&tail[..7], &want[..]);
        assert!(text(&d).ends_with("Ende\u{c}Zwei"), "{:?}", text(&d));
    }

    #[test]
    fn skips_destinations_and_binary() {
        let r = b"{\\rtf1{\\info{\\title Secret}}{\\*\\generator X;}{\\pict\\bin3 }}}abc}Hi{\\field{\\*\\fldinst PAGE}{\\fldrslt 7}}}";
        assert_eq!(text(&read(r).unwrap()), "Hi7");
    }

    #[test]
    fn tracked_changes() {
        let r = br"{\rtf1{\*\revtbl {Unknown;}{Maria;}}A{\revised\revauth1\revdttm116087424 neu}{\deleted\revauthdel1 alt}B}";
        let d = read(r).unwrap();
        assert_eq!(text(&d), "AneualtB");
        assert!(d.els.contains(&El::Code(Code::RevOn { del: false, author: "Maria".into(), date: "2010-11-11T10:00:00Z".into() })), "{:?}", d.els);
        assert!(d.els.contains(&El::Code(Code::RevOn { del: true, author: "Maria".into(), date: String::new() })));
        assert_eq!(d.els.iter().filter(|e| matches!(e, El::Code(Code::RevOff { .. }))).count(), 2);
    }

    #[test]
    fn symbol_fonts() {
        assert_eq!(symbol_char("Wingdings", 0xF0A7), Some('▪'));
        assert_eq!(symbol_char("Wingdings", 0xF020), Some(' '));
        assert_eq!(symbol_char("Wingdings", 0x20), None);
        assert_eq!(symbol_char("Symbol", 0x61), Some('α'));
        assert_eq!(symbol_char("Arial", 0x61), None);
        assert_eq!(symbol_char("Arial", 0xF041), Some('A'));
        let r = br"{\rtf1{\fonttbl{\f0 Arial;}{\f1 Symbol;}}a{\f1 a\'b7}}";
        assert_eq!(text(&read(r).unwrap()), "aα•");
    }

    #[test]
    fn sample_files() {
        for f in ["tests/data/sample.rtf", "tests/data/sample_lo.rtf"] {
            let Ok(b) = std::fs::read(f) else { continue };
            let d = read(&b).unwrap();
            let t = text(&d);
            assert!(t.contains("Grüße aus München: ÄÖÜäöüß."), "{f}: {t:?}");
            assert!(t.contains("Russisch: Привет мир"), "{f}: {t}");
            assert_eq!(table_text(&d.els), "Zelle A1|Zelle B1/Zelle A2|Zelle B2", "{f}");
            assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::Note { .. }))), "{f}");
            assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: 0, .. }))), "{f}");
            assert!(d.els.iter().any(|e| matches!(e, El::Code(Code::HeadFoot { which: 2, .. }))), "{f}");
            assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::DblUnderline))), "{f}");
            assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Strikeout))), "{f}");
            assert_eq!(d.init.justify, Justify::Center, "{f}");
            assert_eq!(d.init.page_width, 11906 * 1200 / 1440, "{f}");
            assert_eq!(d.init.left, 1417 * 1200 / 1440, "{f}");
        }
    }

    #[test]
    fn never_panics_on_garbage() {
        let base = std::fs::read("tests/data/sample_lo.rtf").unwrap_or_else(|_| br"{\rtf1 {\b x}\'4 \u-1\bin99999999 {{{".to_vec());
        for cut in (0..base.len()).step_by(7) {
            let _ = read(&base[..cut]);
        }
        let mut seed: u32 = 12345;
        for _ in 0..200 {
            let mut v = base.clone();
            for _ in 0..20 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let i = (seed as usize >> 4) % v.len().max(1);
                if !v.is_empty() {
                    v[i] = (seed >> 24) as u8;
                }
            }
            let _ = read(&v);
        }
    }
}

#[cfg(test)]
mod symbol_tests {
    #[test]
    fn symbol_font_trademarks() {
        assert_eq!(super::symbol_char("Symbol", 0xE2), Some('®'));
        assert_eq!(super::symbol_char("Symbol", 0xF0E4), Some('™'));
        assert_eq!(super::symbol_char("Symbol", 0x61), Some('α'));
        assert_eq!(super::symbol_char("Symbol", 0xB7), Some('•'));
    }
}
