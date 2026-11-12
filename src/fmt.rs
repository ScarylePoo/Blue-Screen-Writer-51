//! Shift-F8 Format codes beyond the core layout codes: hyphenation zone,
//! line height, line numbering, paper size/type and labels, block protect,
//! conditional end of page, advance, decimal/align character, language,
//! overstrike, printer functions, underline mode, border options, hard
//! tabs, `[ISRt]`, `[/]` …  All live in the stream as `Code::Fmt(Fmt)`.
//!
//! This module holds the model, Reveal Codes names, the stream encoding,
//! the WP 5.x import, the "format state" at a position and the page
//! numbering helpers used by printing.

use crate::doc::*;

/// Word or letter spacing (Format: Printer Functions, 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    Normal,
    Optimal,
    /// Percent of optimal.
    Percent(u16),
}

impl Spacing {
    pub fn name(self) -> String {
        match self {
            Spacing::Normal => "Normal".into(),
            Spacing::Optimal => "Optimal".into(),
            Spacing::Percent(p) => format!("{p}%"),
        }
    }
    fn parse(s: &str) -> Spacing {
        match s {
            "Normal" => Spacing::Normal,
            "Optimal" | "" => Spacing::Optimal,
            p => Spacing::Percent(p.trim_end_matches('%').parse().unwrap_or(100)),
        }
    }
    /// Scale factor in percent.
    pub fn pct(self) -> i32 {
        match self {
            Spacing::Percent(p) => p as i32,
            _ => 100,
        }
    }
}

/// Advance (Format: Other, 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adv {
    Up,
    Down,
    ToLine,
    Left,
    Right,
    ToPos,
}

impl Adv {
    pub fn name(self) -> &'static str {
        match self {
            Adv::Up => "AdvUp",
            Adv::Down => "AdvDn",
            Adv::ToLine => "AdvToLn",
            Adv::Left => "AdvLft",
            Adv::Right => "AdvRgt",
            Adv::ToPos => "AdvToPos",
        }
    }
    const ALL: [Adv; 6] = [Adv::Up, Adv::Down, Adv::ToLine, Adv::Left, Adv::Right, Adv::ToPos];
}

/// Format: Line Numbering.
#[derive(Clone, Debug, PartialEq)]
pub struct LineNum {
    pub count_blank: bool,
    pub every: u32,
    /// Position of the number from the left paper edge (WPU).
    pub pos: i32,
    pub start: u32,
    pub restart: bool,
}

impl Default for LineNum {
    fn default() -> Self {
        LineNum { count_blank: true, every: 1, pos: 720, start: 1, restart: true }
    }
}

/// Format: Labels.
#[derive(Clone, Debug, PartialEq)]
pub struct Labels {
    pub w: i32,
    pub h: i32,
    pub cols: u32,
    pub rows: u32,
    pub top: i32,
    pub left: i32,
    pub gap_col: i32,
    pub gap_row: i32,
    pub ml: i32,
    pub mr: i32,
    pub mt: i32,
    pub mb: i32,
}

impl Default for Labels {
    /// 3 × 10 address labels (2.63" × 1") on Letter, like WP's LABELS.WPM.
    fn default() -> Self {
        Labels { w: 3156, h: 1200, cols: 3, rows: 10, top: 600, left: 228, gap_col: 168, gap_row: 0, ml: 180, mr: 120, mt: 120, mb: 0 }
    }
}

/// Paper size/type as in `[Paper Sz/Typ:8.5" x 11",Standard]`.  Landscape
/// is simply width > height.
#[derive(Clone, Debug, PartialEq)]
pub struct Paper {
    pub w: i32,
    pub h: i32,
    pub ty: String,
    pub labels: Option<Labels>,
}

impl Paper {
    pub fn landscape(&self) -> bool {
        self.w > self.h
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fmt {
    /// `[HZone:10%,4%]`
    HZone { left: u8, right: u8 },
    /// `[/]` — cancel hyphenation of the following word (Home,/).
    CancelHyph,
    /// `[Ln Height:Auto]` / `[Ln Height:0.167"]`
    LnHeight(Option<i32>),
    /// `[Ln Num:On]` / `[Ln Num:Off]`
    LnNum(Option<LineNum>),
    /// `[Just Lim:60,400]` (percent; expand 0 = unlimited)
    JustLim { compress: u16, expand: u16 },
    /// `[Force:Odd]` / `[Force:Even]`
    Force { odd: bool },
    /// `[Pg Num:iv]` — new page number in roman numerals.
    PgNumRoman(u32),
    /// `[Pg Num Style:Page ^B]`
    PgNumStyle(String),
    /// `[Insert Pg Num:^B]`
    InsertPgNum(String),
    /// `[Paper Sz/Typ:8.5" x 11",Standard]`
    Paper(Paper),
    /// `[Block Pro:On]` / `[Block Pro:Off]`
    BlockPro(bool),
    /// `[Cndl EOP:3]`
    CndlEop(u32),
    /// `[AdvUp:0.1"]` … `[AdvToPos:3"]`
    Adv(Adv, i32),
    /// `[Decml/Algn Char:,,.]`
    DecAlign { dec: char, thou: char },
    /// `[Lang:DE]`
    Lang(String),
    /// `[Ovrstk:e´]`
    Ovrstk(String),
    /// `[Kern:On]` / `[Kern:Off]`
    Kern(bool),
    /// `[Ptr Cmnd:…]`
    PtrCmnd(String),
    /// `[Wrd/Ltr Spacing:Optimal,Optimal]`
    WrdLtr { word: Spacing, letter: Spacing },
    /// `[Bline:On]` / `[Bline:Off]`
    Bline(bool),
    /// `[Leading Adj:0",0"]`
    LeadingAdj { primary: i32, secondary: i32 },
    /// `[Undrln:Spaces,Tabs]`
    Undrln { spaces: bool, tabs: bool },
    /// `[Brdr Opt]` — 6 line styles × (width, shading %, spacing, length).
    BrdrOpt(Vec<i32>),
    /// Hard tabs `[TAB]` `[CNTR TAB]` `[RGT TAB]` `[DEC TAB]` (Home,Tab …).
    HardTab(TabKind),
    /// `[Dec Tab]` from Tab Align (Ctrl-F6).
    DecTab,
    /// `[ISRt]` invisible soft return (Home,Enter).
    ISRt,
}

impl Fmt {
    /// Codes that take room on the screen (the cursor stops on them).
    pub fn occupies_space(&self) -> bool {
        matches!(self, Fmt::HardTab(_) | Fmt::DecTab | Fmt::Ovrstk(_) | Fmt::InsertPgNum(_))
    }
    /// Tab-like codes: the tab kind they force.
    pub fn tab_kind(&self) -> Option<TabKind> {
        match self {
            Fmt::HardTab(k) => Some(*k),
            Fmt::DecTab => Some(TabKind::Decimal),
            _ => None,
        }
    }
    pub fn same_kind(&self, o: &Fmt) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(o)
            && !matches!(self, Fmt::HardTab(_) | Fmt::DecTab | Fmt::ISRt | Fmt::CancelHyph | Fmt::Ovrstk(_) | Fmt::InsertPgNum(_) | Fmt::Adv(..) | Fmt::BlockPro(_) | Fmt::PtrCmnd(_))
    }

    /// The Reveal Codes token.
    pub fn reveal(&self) -> String {
        match self {
            Fmt::HZone { left, right } => format!("[HZone:{left}%,{right}%]"),
            Fmt::CancelHyph => "[/]".into(),
            Fmt::LnHeight(None) => "[Ln Height:Auto]".into(),
            Fmt::LnHeight(Some(h)) => format!("[Ln Height:{}]", fmt_inches(*h)),
            Fmt::LnNum(n) => format!("[Ln Num:{}]", if n.is_some() { "On" } else { "Off" }),
            Fmt::JustLim { compress, expand } => format!("[Just Lim:{compress},{expand}]"),
            Fmt::Force { odd } => format!("[Force:{}]", if *odd { "Odd" } else { "Even" }),
            Fmt::PgNumRoman(n) => format!("[Pg Num:{}]", roman(*n)),
            Fmt::PgNumStyle(s) => format!("[Pg Num Style:{}]", caret_text(s)),
            Fmt::InsertPgNum(s) => format!("[Insert Pg Num:{}]", caret_text(s)),
            Fmt::Paper(p) => {
                let ty = if p.labels.is_some() && p.ty == "Labels" { "Labels".to_string() } else { p.ty.clone() };
                format!("[Paper Sz/Typ:{} x {},{}]", fmt_inches(p.w), fmt_inches(p.h), ty)
            }
            Fmt::BlockPro(on) => format!("[Block Pro:{}]", if *on { "On" } else { "Off" }),
            Fmt::CndlEop(n) => format!("[Cndl EOP:{n}]"),
            Fmt::Adv(a, v) => format!("[{}:{}]", a.name(), fmt_inches(*v)),
            Fmt::DecAlign { dec, thou } => format!("[Decml/Algn Char:{dec},{thou}]"),
            Fmt::Lang(l) => format!("[Lang:{l}]"),
            Fmt::Ovrstk(s) => format!("[Ovrstk:{s}]"),
            Fmt::Kern(on) => format!("[Kern:{}]", if *on { "On" } else { "Off" }),
            Fmt::PtrCmnd(s) => format!("[Ptr Cmnd:{s}]"),
            Fmt::WrdLtr { word, letter } => format!("[Wrd/Ltr Spacing:{},{}]", word.name(), letter.name()),
            Fmt::Bline(on) => format!("[Bline:{}]", if *on { "On" } else { "Off" }),
            Fmt::LeadingAdj { primary, secondary } => format!("[Leading Adj:{},{}]", fmt_inches(*primary), fmt_inches(*secondary)),
            Fmt::Undrln { spaces, tabs } => {
                let p: Vec<&str> = [(*spaces, "Spaces"), (*tabs, "Tabs")].iter().filter(|x| x.0).map(|x| x.1).collect();
                format!("[Undrln:{}]", p.join(","))
            }
            Fmt::BrdrOpt(_) => "[Brdr Opt]".into(),
            Fmt::HardTab(k) => match k {
                TabKind::Left => "[TAB]",
                TabKind::Center => "[CNTR TAB]",
                TabKind::Right => "[RGT TAB]",
                TabKind::Decimal => "[DEC TAB]",
            }
            .into(),
            Fmt::DecTab => "[Dec Tab]".into(),
            Fmt::ISRt => "[ISRt]".into(),
        }
    }

    // ------------------------------------------------------------ stream

    /// Lossless one-line encoding (used after `F|` in the stream).
    pub fn to_stream(&self) -> String {
        let b = |x: bool| if x { "1" } else { "0" };
        match self {
            Fmt::HZone { left, right } => format!("HZ;{left};{right}"),
            Fmt::CancelHyph => "CH".into(),
            Fmt::LnHeight(h) => format!("LH;{}", h.map(|v| v.to_string()).unwrap_or_default()),
            Fmt::LnNum(None) => "LN".into(),
            Fmt::LnNum(Some(n)) => format!("LN;{};{};{};{};{}", b(n.count_blank), n.every, n.pos, n.start, b(n.restart)),
            Fmt::JustLim { compress, expand } => format!("JL;{compress};{expand}"),
            Fmt::Force { odd } => format!("FO;{}", b(*odd)),
            Fmt::PgNumRoman(n) => format!("PR;{n}"),
            Fmt::PgNumStyle(s) => format!("PS;{}", esc(s)),
            Fmt::InsertPgNum(s) => format!("IP;{}", esc(s)),
            Fmt::Paper(p) => {
                let mut s = format!("PA;{};{};{}", p.w, p.h, esc(&p.ty));
                if let Some(l) = &p.labels {
                    s.push_str(&format!(
                        ";{};{};{};{};{};{};{};{};{};{};{};{}",
                        l.w, l.h, l.cols, l.rows, l.top, l.left, l.gap_col, l.gap_row, l.ml, l.mr, l.mt, l.mb
                    ));
                }
                s
            }
            Fmt::BlockPro(on) => format!("BP;{}", b(*on)),
            Fmt::CndlEop(n) => format!("CE;{n}"),
            Fmt::Adv(a, v) => format!("AD;{};{v}", a.name()),
            Fmt::DecAlign { dec, thou } => format!("DA;{};{}", esc(&dec.to_string()), esc(&thou.to_string())),
            Fmt::Lang(l) => format!("LG;{}", esc(l)),
            Fmt::Ovrstk(s) => format!("OS;{}", esc(s)),
            Fmt::Kern(on) => format!("KE;{}", b(*on)),
            Fmt::PtrCmnd(s) => format!("PC;{}", esc(s)),
            Fmt::WrdLtr { word, letter } => format!("WL;{};{}", word.name(), letter.name()),
            Fmt::Bline(on) => format!("BL;{}", b(*on)),
            Fmt::LeadingAdj { primary, secondary } => format!("LA;{primary};{secondary}"),
            Fmt::Undrln { spaces, tabs } => format!("UL;{};{}", b(*spaces), b(*tabs)),
            Fmt::BrdrOpt(v) => format!("BO;{}", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")),
            Fmt::HardTab(k) => format!(
                "HT;{}",
                match k {
                    TabKind::Left => "L",
                    TabKind::Center => "C",
                    TabKind::Right => "R",
                    TabKind::Decimal => "D",
                }
            ),
            Fmt::DecTab => "DT".into(),
            Fmt::ISRt => "IS".into(),
        }
    }

    pub fn from_stream(s: &str) -> Option<Fmt> {
        let p: Vec<String> = split(s);
        let g = |i: usize| p.get(i).map(|x| x.as_str()).unwrap_or("");
        let n = |i: usize| g(i).parse::<i32>().unwrap_or(0);
        let u = |i: usize| g(i).parse::<u32>().unwrap_or(0);
        let b = |i: usize| g(i) == "1";
        let ch = |i: usize, d: char| g(i).chars().next().unwrap_or(d);
        Some(match g(0) {
            "HZ" => Fmt::HZone { left: u(1) as u8, right: u(2) as u8 },
            "CH" => Fmt::CancelHyph,
            "LH" => Fmt::LnHeight(g(1).parse().ok()),
            "LN" if p.len() < 3 => Fmt::LnNum(None),
            "LN" => Fmt::LnNum(Some(LineNum { count_blank: b(1), every: u(2).max(1), pos: n(3), start: u(4), restart: b(5) })),
            "JL" => Fmt::JustLim { compress: u(1) as u16, expand: u(2) as u16 },
            "FO" => Fmt::Force { odd: b(1) },
            "PR" => Fmt::PgNumRoman(u(1)),
            "PS" => Fmt::PgNumStyle(g(1).into()),
            "IP" => Fmt::InsertPgNum(g(1).into()),
            "PA" => Fmt::Paper(Paper {
                w: n(1),
                h: n(2),
                ty: g(3).into(),
                labels: (p.len() >= 16).then(|| Labels {
                    w: n(4),
                    h: n(5),
                    cols: u(6).max(1),
                    rows: u(7).max(1),
                    top: n(8),
                    left: n(9),
                    gap_col: n(10),
                    gap_row: n(11),
                    ml: n(12),
                    mr: n(13),
                    mt: n(14),
                    mb: n(15),
                }),
            }),
            "BP" => Fmt::BlockPro(b(1)),
            "CE" => Fmt::CndlEop(u(1)),
            "AD" => Fmt::Adv(*Adv::ALL.iter().find(|a| a.name() == g(1))?, n(2)),
            "DA" => Fmt::DecAlign { dec: ch(1, '.'), thou: ch(2, ',') },
            "LG" => Fmt::Lang(g(1).into()),
            "OS" => Fmt::Ovrstk(g(1).into()),
            "KE" => Fmt::Kern(b(1)),
            "PC" => Fmt::PtrCmnd(g(1).into()),
            "WL" => Fmt::WrdLtr { word: Spacing::parse(g(1)), letter: Spacing::parse(g(2)) },
            "BL" => Fmt::Bline(b(1)),
            "LA" => Fmt::LeadingAdj { primary: n(1), secondary: n(2) },
            "UL" => Fmt::Undrln { spaces: b(1), tabs: b(2) },
            "BO" => Fmt::BrdrOpt(g(1).split(',').filter_map(|x| x.parse().ok()).collect()),
            "HT" => Fmt::HardTab(match g(1) {
                "C" => TabKind::Center,
                "R" => TabKind::Right,
                "D" => TabKind::Decimal,
                _ => TabKind::Left,
            }),
            "DT" => Fmt::DecTab,
            "IS" => Fmt::ISRt,
            _ => return None,
        })
    }
}

fn esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            ';' => o.push_str("\\s"),
            '|' => o.push_str("\\p"),
            c => o.push(c),
        }
    }
    o
}

fn split(s: &str) -> Vec<String> {
    let mut v = vec![String::new()];
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some('n') => v.last_mut().unwrap().push('\n'),
                Some('s') => v.last_mut().unwrap().push(';'),
                Some('p') => v.last_mut().unwrap().push('|'),
                Some(x) => v.last_mut().unwrap().push(x),
                None => {}
            },
            ';' => v.push(String::new()),
            c => v.last_mut().unwrap().push(c),
        }
    }
    v
}

/// `^B` as WP shows it (the style text keeps a real Ctrl-B).
pub fn caret_text(s: &str) -> String {
    s.replace('\u{2}', "^B")
}

/// The style text with the page number filled in.
pub fn fill_style(style: &str, num: &str) -> String {
    style.replace('\u{2}', num).replace("^B", num)
}

pub fn roman(mut n: u32) -> String {
    let mut s = String::new();
    for (v, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

pub fn parse_roman(s: &str) -> Option<u32> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() || !s.chars().all(|c| "ivxlcdm".contains(c)) {
        return None;
    }
    let val = |c: char| match c {
        'i' => 1,
        'v' => 5,
        'x' => 10,
        'l' => 50,
        'c' => 100,
        'd' => 500,
        _ => 1000,
    };
    let cs: Vec<u32> = s.chars().map(val).collect();
    let mut t = 0i64;
    for (k, &v) in cs.iter().enumerate() {
        if cs.get(k + 1).is_some_and(|&n| n > v) {
            t -= v as i64;
        } else {
            t += v as i64;
        }
    }
    (t > 0).then_some(t as u32)
}

// ============================================================ doc options

/// Document-level settings of Format: Document (kept in the stream header).
#[derive(Clone, Debug, PartialEq)]
pub struct DocOpts {
    /// Redline method: 0 printer dependent, 1 left margin bar, 2 alternating.
    pub redline: u8,
    pub pitch_auto: bool,
    pub pitch: i32,
}

impl Default for DocOpts {
    fn default() -> Self {
        DocOpts { redline: 0, pitch_auto: true, pitch: 120 }
    }
}

impl DocOpts {
    pub fn is_default(&self) -> bool {
        *self == DocOpts::default()
    }
    pub fn to_line(&self) -> String {
        format!("#opts|{}|{}|{}", self.redline, self.pitch_auto as u8, self.pitch)
    }
    pub fn from_line(rest: &str) -> DocOpts {
        let p: Vec<&str> = rest.split('|').collect();
        DocOpts {
            redline: p.first().and_then(|x| x.parse().ok()).unwrap_or(0),
            pitch_auto: p.get(1) != Some(&"0"),
            pitch: p.get(2).and_then(|x| x.parse().ok()).unwrap_or(120),
        }
    }
    pub fn redline_name(&self) -> &'static str {
        ["Printer Dependent", "Left", "Alternating"][self.redline.min(2) as usize]
    }
}

// ============================================================ format state

/// Settings of this module in effect at a position.
#[derive(Clone, Debug)]
pub struct FmtState {
    pub hzone: (u8, u8),
    pub ln_height: Option<i32>,
    pub ln_num: Option<LineNum>,
    pub just_lim: (u16, u16),
    pub dec: char,
    pub thou: char,
    pub lang: String,
    pub kern: bool,
    pub wrd_ltr: (Spacing, Spacing),
    pub bline: bool,
    pub leading: (i32, i32),
    pub undrln: (bool, bool),
    pub paper: Option<Paper>,
    pub pgnum_style: String,
    pub brdr: Vec<i32>,
}

impl Default for FmtState {
    fn default() -> Self {
        FmtState {
            hzone: (10, 4),
            ln_height: None,
            ln_num: None,
            just_lim: (60, 400),
            dec: '.',
            thou: ',',
            lang: "DE".into(),
            kern: false,
            wrd_ltr: (Spacing::Optimal, Spacing::Optimal),
            bline: false,
            leading: (0, 0),
            undrln: (true, false),
            paper: None,
            pgnum_style: "\u{2}".into(),
            brdr: default_borders(),
        }
    }
}

impl FmtState {
    pub fn apply(&mut self, f: &Fmt) {
        match f {
            Fmt::HZone { left, right } => self.hzone = (*left, *right),
            Fmt::LnHeight(h) => self.ln_height = *h,
            Fmt::LnNum(n) => self.ln_num = n.clone(),
            Fmt::JustLim { compress, expand } => self.just_lim = (*compress, *expand),
            Fmt::DecAlign { dec, thou } => {
                self.dec = *dec;
                self.thou = *thou
            }
            Fmt::Lang(l) => self.lang = l.clone(),
            Fmt::Kern(k) => self.kern = *k,
            Fmt::WrdLtr { word, letter } => self.wrd_ltr = (*word, *letter),
            Fmt::Bline(b) => self.bline = *b,
            Fmt::LeadingAdj { primary, secondary } => self.leading = (*primary, *secondary),
            Fmt::Undrln { spaces, tabs } => self.undrln = (*spaces, *tabs),
            Fmt::Paper(p) => self.paper = Some(p.clone()),
            Fmt::PgNumStyle(s) => self.pgnum_style = s.clone(),
            Fmt::BrdrOpt(v) => self.brdr = v.clone(),
            _ => {}
        }
    }
}

pub fn state_at(doc: &Document, idx: usize) -> FmtState {
    let mut s = FmtState::default();
    for e in doc.initial_codes.iter().chain(doc.els[..idx.min(doc.els.len())].iter()) {
        if let El::Code(Code::Fmt(f)) = e {
            s.apply(f);
        }
    }
    s
}

/// WP's default border options: width, shading per style (+ extras).
pub fn default_borders() -> Vec<i32> {
    // single w,shade; double w,shade,spacing; dashed w,shade,space,len;
    // dotted w,shade,space; thick w,shade; extra thick w,shade
    vec![12, 100, 12, 100, 36, 12, 100, 36, 72, 12, 100, 36, 60, 100, 120, 100]
}

// ============================================================ page numbers

/// What printing needs to know per physical page: number text, and
/// suppression bits (`Code::Suppress`).
#[derive(Clone, Debug, Default)]
pub struct PageInfo {
    pub num: u32,
    pub roman: bool,
    pub style: String,
    pub suppress: u8,
    pub center: bool,
}

impl PageInfo {
    pub fn num_text(&self) -> String {
        if self.roman { roman(self.num) } else { self.num.to_string() }
    }
    /// Page number as printed by `[Pg Numbering]` (with the style).
    pub fn styled(&self) -> String {
        fill_style(&self.style, &self.num_text())
    }
}

/// Suppress bits (WP 5.x): 1 page numbering, 2 page number at bottom
/// centre, 4 header A, 8 header B, 0x10 footer A, 0x20 footer B.
pub const SUP_PGNUM: u8 = 1;
pub const SUP_PG_BC: u8 = 2;

pub fn suppress_text(bits: u8) -> String {
    let mut p = vec![];
    if bits & SUP_PGNUM != 0 {
        p.push("PgNum");
    }
    if bits & SUP_PG_BC != 0 {
        p.push("Pg BC");
    }
    for (b, n) in [(4, "HA"), (8, "HB"), (0x10, "FA"), (0x20, "FB")] {
        if bits & b != 0 {
            p.push(n);
        }
    }
    format!("[Suppress:{}]", p.join(","))
}

/// Number, style, suppression and centring of every page, from the codes
/// on each page (`[Pg Num]`, `[Force]`, `[Pg Num Style]`, `[Suppress]`,
/// `[Center Pg]`).  Codes on a page take effect on that page.
pub fn page_infos(doc: &Document, layout: &crate::layout::Layout) -> Vec<PageInfo> {
    let pages = layout.pages.max(1) as usize;
    // codes per page
    let mut per_page: Vec<Vec<&Code>> = vec![vec![]; pages];
    let mut init: Vec<&Code> = vec![];
    for e in &doc.initial_codes {
        if let El::Code(c) = e {
            init.push(c);
        }
    }
    for l in &layout.lines {
        let p = (l.page.max(1) as usize - 1).min(pages - 1);
        for k in l.start..l.end.min(doc.els.len()) {
            if let El::Code(c) = &doc.els[k] {
                per_page[p].push(c);
            }
        }
    }
    let mut out = Vec::with_capacity(pages);
    let mut num: u32 = 0;
    let mut roman_on = false;
    let mut style = String::from("\u{2}");
    for c in &init {
        match c {
            Code::PageNumber(n) => {
                num = n.saturating_sub(1);
                roman_on = false
            }
            Code::Fmt(Fmt::PgNumRoman(n)) => {
                num = n.saturating_sub(1);
                roman_on = true
            }
            Code::Fmt(Fmt::PgNumStyle(s)) => style = s.clone(),
            _ => {}
        }
    }
    for codes in per_page.iter() {
        num += 1;
        let mut info = PageInfo::default();
        for c in codes {
            match c {
                Code::PageNumber(n) => {
                    num = *n;
                    roman_on = false
                }
                Code::Fmt(Fmt::PgNumRoman(n)) => {
                    num = *n;
                    roman_on = true
                }
                Code::Fmt(Fmt::Force { odd }) => {
                    if (num % 2 == 1) != *odd {
                        num += 1;
                    }
                }
                Code::Fmt(Fmt::PgNumStyle(s)) => style = s.clone(),
                Code::Suppress(b) => info.suppress |= *b,
                Code::CenterPage => info.center = true,
                _ => {}
            }
        }
        info.num = num;
        info.roman = roman_on;
        info.style = style.clone();
        out.push(info);
    }
    out
}

// ============================================================ paper list

/// A paper definition of Format: Paper Size/Type.
#[derive(Clone, Debug, PartialEq)]
pub struct PaperDef {
    pub name: String,
    pub paper: Paper,
}

pub fn mm(v: f64) -> i32 {
    (v * 1200.0 / 25.4).round() as i32
}

pub fn default_papers() -> Vec<PaperDef> {
    let d = |name: &str, w: i32, h: i32, ty: &str| PaperDef { name: name.into(), paper: Paper { w, h, ty: ty.into(), labels: None } };
    let mut v = vec![
        d("Standard", 10200, 13200, "Standard"),
        d("Standard Landscape", 13200, 10200, "Standard"),
        d("A4", mm(210.0), mm(297.0), "Standard"),
        d("A4 Landscape", mm(297.0), mm(210.0), "Standard"),
        d("A5", mm(148.0), mm(210.0), "Standard"),
        d("Legal", 10200, 16800, "Standard"),
        d("Legal Landscape", 16800, 10200, "Standard"),
        d("Envelope", 11400, 4800, "Envelope"),
        d("Envelope DL", mm(220.0), mm(110.0), "Envelope"),
        d("Envelope C5", mm(229.0), mm(162.0), "Envelope"),
        d("Half Sheet", 6600, 10200, "Standard"),
        d("US Government", 9600, 13200, "Standard"),
        d("Letterhead", mm(210.0), mm(297.0), "Letterhead"),
        d("Bond", mm(210.0), mm(297.0), "Bond"),
        d("Transparency", mm(210.0), mm(297.0), "Transparency"),
        d("Cardstock", mm(210.0), mm(297.0), "Cardstock"),
    ];
    v.push(PaperDef {
        name: "Labels 3 x 10 (Letter)".into(),
        paper: Paper { w: 10200, h: 13200, ty: "Labels".into(), labels: Some(Labels::default()) },
    });
    v.push(PaperDef {
        name: "Labels 3 x 8 (A4, 70 x 36 mm)".into(),
        paper: Paper {
            w: mm(210.0),
            h: mm(297.0),
            ty: "Labels".into(),
            labels: Some(Labels { w: mm(70.0), h: mm(36.0), cols: 3, rows: 8, top: mm(4.5), left: 0, gap_col: 0, gap_row: 0, ml: 180, mr: 120, mt: 120, mb: 0 }),
        },
    });
    v.push(d("[ALL OTHERS]", 10200, 13200, "[ALL OTHERS]"));
    v
}

fn papers_file() -> std::path::PathBuf {
    crate::settings::config_dir().join("papers.txt")
}

/// The paper list (user's file, or the defaults).
pub fn load_papers() -> Vec<PaperDef> {
    let Ok(t) = std::fs::read_to_string(papers_file()) else { return default_papers() };
    let v: Vec<PaperDef> = t
        .lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once('\t')?;
            match Fmt::from_stream(rest)? {
                Fmt::Paper(p) => Some(PaperDef { name: name.into(), paper: p }),
                _ => None,
            }
        })
        .collect();
    if v.is_empty() { default_papers() } else { v }
}

pub fn save_papers(v: &[PaperDef]) {
    let t: String = v.iter().map(|d| format!("{}\t{}\n", d.name.replace(['\t', '\n'], " "), Fmt::Paper(d.paper.clone()).to_stream())).collect();
    let _ = std::fs::create_dir_all(crate::settings::config_dir());
    let _ = std::fs::write(papers_file(), t);
}

// ============================================================ WP 5.x import

fn u16le(d: &[u8], k: usize) -> u16 {
    if k + 1 < d.len() { u16::from_le_bytes([d[k], d[k + 1]]) } else { 0 }
}

fn wpu_of(s: &str) -> i32 {
    let s = s.trim().trim_end_matches('"');
    s.parse::<f64>().map(|v| (v * 1200.0).round() as i32).unwrap_or(0)
}

/// Convert a code the WP 5.x reader named (and kept raw) into a real code.
/// `data` is the whole function group, so `data[k]` is offset `k` of the
/// format description.
pub fn from_wp5(name: &str, data: &[u8]) -> Option<Code> {
    let (head, arg) = name.split_once(':').unwrap_or((name, ""));
    let f = match head {
        "ISRt" => Fmt::ISRt,
        "/" => Fmt::CancelHyph,
        "Block Pro" => Fmt::BlockPro(arg == "On"),
        "Ln Height" => Fmt::LnHeight(if arg == "Auto" { None } else { Some(wpu_of(arg)) }),
        "HZone" => {
            let (l, r) = arg.split_once(',')?;
            Fmt::HZone { left: l.trim_end_matches('%').parse().ok()?, right: r.trim_end_matches('%').parse().ok()? }
        }
        "Paper Sz/Typ" => {
            let (size, ty) = arg.split_once(',').unwrap_or((arg, "Standard"));
            let (w, h) = size.split_once(" x ")?;
            let mut p = Paper { w: wpu_of(w), h: wpu_of(h), ty: ty.trim().to_string(), labels: None };
            // 5.1 labels (+197…): flags at +222, then rows/cols, sizes
            if data.len() > 245 && data[222] & 1 != 0 {
                p.labels = Some(Labels {
                    ml: u16le(data, 197) as i32,
                    mr: u16le(data, 199) as i32,
                    mt: u16le(data, 201) as i32,
                    mb: u16le(data, 203) as i32,
                    cols: (data[223] as u32).max(1),
                    rows: (data[224] as u32).max(1),
                    top: u16le(data, 225) as i32,
                    left: u16le(data, 227) as i32,
                    w: u16le(data, 229) as i32,
                    h: u16le(data, 231) as i32,
                    gap_col: u16le(data, 233) as i32,
                    gap_row: u16le(data, 235) as i32,
                });
                p.ty = "Labels".into();
            }
            Fmt::Paper(p)
        }
        "Decml/Algn Char" => {
            let mut c = arg.chars();
            let dec = c.next().unwrap_or('.');
            let _ = c.next();
            Fmt::DecAlign { dec, thou: c.next().unwrap_or(',') }
        }
        "Undrln" => Fmt::Undrln { spaces: arg.contains("Spaces"), tabs: arg.contains("Tabs") },
        "Ln Num" => {
            if arg == "On" && data.len() > 13 {
                let fl = data[9];
                Fmt::LnNum(Some(LineNum {
                    count_blank: fl & 0x40 != 0,
                    every: ((fl & 0x1F) as u32).max(1),
                    restart: fl & 0x20 != 0,
                    pos: u16le(data, 10) as i32,
                    start: (u16le(data, 12) as u32).max(1),
                }))
            } else if arg == "On" {
                Fmt::LnNum(Some(LineNum::default()))
            } else {
                Fmt::LnNum(None)
            }
        }
        h if h.starts_with("Adv") => {
            let a = *Adv::ALL.iter().find(|a| a.name() == h)?;
            Fmt::Adv(a, wpu_of(arg))
        }
        "Force" => Fmt::Force { odd: arg == "Odd" },
        "Bline" => Fmt::Bline(arg == "On"),
        "Wrd/Ltr Spacing" => {
            let (w, l) = arg.split_once(',')?;
            let sp = |s: &str| match s.trim_end_matches('%').parse::<u16>() {
                Ok(0) | Err(_) => Spacing::Optimal,
                Ok(p) => Spacing::Percent(p),
            };
            Fmt::WrdLtr { word: sp(w), letter: sp(l) }
        }
        "Just Lim" => {
            let (a, b) = arg.split_once(',')?;
            Fmt::JustLim { compress: a.parse().ok()?, expand: b.parse().ok()? }
        }
        "Lang" if !arg.is_empty() => Fmt::Lang(arg.to_string()),
        "Pg Num Style" => Fmt::PgNumStyle(arg.to_string()),
        "Insert Pg Num" => Fmt::InsertPgNum(arg.to_string()),
        "Pg Num" => Fmt::PgNumRoman(parse_roman(arg)?),
        "Ptr Cmnd" => Fmt::PtrCmnd(arg.to_string()),
        "Cndl EOP" => Fmt::CndlEop(arg.parse().ok()?),
        "Kern" => Fmt::Kern(arg == "On"),
        "Leading Adj" => {
            let (a, b) = arg.split_once(',')?;
            Fmt::LeadingAdj { primary: wpu_of(a), secondary: wpu_of(b) }
        }
        "Border Opt" => Fmt::BrdrOpt(default_borders()),
        _ => return None,
    };
    Some(Code::Fmt(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_roundtrip_all() {
        let all = vec![
            Fmt::HZone { left: 10, right: 4 },
            Fmt::CancelHyph,
            Fmt::LnHeight(None),
            Fmt::LnHeight(Some(200)),
            Fmt::LnNum(None),
            Fmt::LnNum(Some(LineNum::default())),
            Fmt::JustLim { compress: 60, expand: 400 },
            Fmt::Force { odd: true },
            Fmt::PgNumRoman(4),
            Fmt::PgNumStyle("Seite \u{2}; a|b".into()),
            Fmt::InsertPgNum("\u{2}".into()),
            Fmt::Paper(Paper { w: 13200, h: 10200, ty: "Standard".into(), labels: None }),
            Fmt::Paper(Paper { w: 10200, h: 13200, ty: "Labels".into(), labels: Some(Labels::default()) }),
            Fmt::BlockPro(true),
            Fmt::CndlEop(3),
            Fmt::Adv(Adv::ToPos, 3600),
            Fmt::DecAlign { dec: ',', thou: '.' },
            Fmt::Lang("DE".into()),
            Fmt::Ovrstk("e\u{301}".into()),
            Fmt::Kern(true),
            Fmt::PtrCmnd("<27>E".into()),
            Fmt::WrdLtr { word: Spacing::Percent(120), letter: Spacing::Normal },
            Fmt::Bline(true),
            Fmt::LeadingAdj { primary: 24, secondary: -12 },
            Fmt::Undrln { spaces: true, tabs: true },
            Fmt::BrdrOpt(default_borders()),
            Fmt::HardTab(TabKind::Center),
            Fmt::DecTab,
            Fmt::ISRt,
        ];
        for f in all {
            let s = f.to_stream();
            assert!(!s.contains('\n') && !s.contains('|'), "{s}");
            assert_eq!(Fmt::from_stream(&s), Some(f.clone()), "{s}");
            assert!(f.reveal().starts_with('['));
        }
    }

    #[test]
    fn reveal_names() {
        assert_eq!(Fmt::HZone { left: 10, right: 4 }.reveal(), "[HZone:10%,4%]");
        assert_eq!(Fmt::DecAlign { dec: ',', thou: '.' }.reveal(), "[Decml/Algn Char:,,.]");
        assert_eq!(Fmt::Paper(Paper { w: 10200, h: 13200, ty: "Standard".into(), labels: None }).reveal(), "[Paper Sz/Typ:8.5\" x 11\",Standard]");
        assert_eq!(Fmt::PgNumStyle("Page \u{2}".into()).reveal(), "[Pg Num Style:Page ^B]");
        assert_eq!(suppress_text(1 | 4 | 8 | 0x10 | 0x20), "[Suppress:PgNum,HA,HB,FA,FB]");
        assert_eq!(Fmt::HardTab(TabKind::Left).reveal(), "[TAB]");
        assert_eq!(Fmt::Adv(Adv::Down, 120).reveal(), "[AdvDn:0.1\"]");
    }

    #[test]
    fn document_stream_roundtrip() {
        let mut d = Document::from_text("Hallo");
        d.opts.redline = 2;
        d.opts.pitch_auto = false;
        d.els.insert(0, El::Code(Code::Fmt(Fmt::Paper(Paper { w: 10200, h: 13200, ty: "Labels".into(), labels: Some(Labels::default()) }))));
        d.els.push(El::Code(Code::Fmt(Fmt::PgNumStyle("a|b;c\\d".into()))));
        d.els.push(El::Code(Code::Suppress(5)));
        d.initial_codes.push(El::Code(Code::Fmt(Fmt::Lang("FR".into()))));
        let s = crate::stream::write(&d);
        let r = crate::stream::read(&s).unwrap();
        assert_eq!(r.els, d.els);
        assert_eq!(r.initial_codes, d.initial_codes);
        assert_eq!(r.opts, d.opts);
    }

    #[test]
    fn romans() {
        assert_eq!(roman(14), "xiv");
        assert_eq!(parse_roman("xiv"), Some(14));
        assert_eq!(parse_roman("12"), None);
    }

    #[test]
    fn wp5_names() {
        assert_eq!(from_wp5("HZone:10%,4%", &[]), Some(Code::Fmt(Fmt::HZone { left: 10, right: 4 })));
        assert_eq!(from_wp5("Ln Height:Auto", &[]), Some(Code::Fmt(Fmt::LnHeight(None))));
        assert_eq!(from_wp5("AdvRgt:0.5\"", &[]), Some(Code::Fmt(Fmt::Adv(Adv::Right, 600))));
        assert_eq!(from_wp5("Lang:DE", &[]), Some(Code::Fmt(Fmt::Lang("DE".into()))));
        assert_eq!(from_wp5("Block Pro:Off", &[]), Some(Code::Fmt(Fmt::BlockPro(false))));
        assert!(from_wp5("Mrg:FIELD", &[]).is_none());
    }
}
