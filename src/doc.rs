//! Document model.
//!
//! A document is, exactly like in WordPerfect 5.1, a flat stream of
//! characters interleaved with codes.  What Reveal Codes shows *is* this
//! stream; soft returns / soft pages are not stored, they are produced by
//! layout and only displayed.
//!
//! Units: WPU = 1/1200 inch (the unit WordPerfect 5.x uses internally).

pub const WPU_PER_INCH: i32 = 1200;

/// Character attributes (Font → Size / Appearance in WP 5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Attr {
    Bold,
    Underline,
    DblUnderline,
    Italic,
    Outline,
    Shadow,
    SmallCaps,
    Redline,
    Strikeout,
    Superscript,
    Subscript,
    Fine,
    Small,
    Large,
    VeryLarge,
    ExtraLarge,
}

impl Attr {
    pub const ALL: [Attr; 16] = [
        Attr::Bold,
        Attr::Underline,
        Attr::DblUnderline,
        Attr::Italic,
        Attr::Outline,
        Attr::Shadow,
        Attr::SmallCaps,
        Attr::Redline,
        Attr::Strikeout,
        Attr::Superscript,
        Attr::Subscript,
        Attr::Fine,
        Attr::Small,
        Attr::Large,
        Attr::VeryLarge,
        Attr::ExtraLarge,
    ];

    pub fn bit(self) -> u16 {
        1 << (self as u16)
    }

    /// Reveal Codes mnemonic for the "on" code, e.g. `BOLD`.  The "off"
    /// code is the same text in lower case.
    pub fn mnemonic(self) -> &'static str {
        match self {
            Attr::Bold => "BOLD",
            Attr::Underline => "UND",
            Attr::DblUnderline => "DBL UND",
            Attr::Italic => "ITALC",
            Attr::Outline => "OUTLN",
            Attr::Shadow => "SHADW",
            Attr::SmallCaps => "SM CAP",
            Attr::Redline => "REDLN",
            Attr::Strikeout => "STKOUT",
            Attr::Superscript => "SUPRSCPT",
            Attr::Subscript => "SUBSCPT",
            Attr::Fine => "FINE",
            Attr::Small => "SMALL",
            Attr::Large => "LARGE",
            Attr::VeryLarge => "VRY LARGE",
            Attr::ExtraLarge => "EXT LARGE",
        }
    }

    pub fn from_mnemonic(s: &str) -> Option<Attr> {
        Attr::ALL.into_iter().find(|a| a.mnemonic().eq_ignore_ascii_case(s))
    }
}

/// A set of attributes as a bitmask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AttrSet(pub u16);

impl AttrSet {
    pub fn has(self, a: Attr) -> bool {
        self.0 & a.bit() != 0
    }
    pub fn set(&mut self, a: Attr, on: bool) {
        if on {
            self.0 |= a.bit()
        } else {
            self.0 &= !a.bit()
        }
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn iter(self) -> impl Iterator<Item = Attr> {
        Attr::ALL.into_iter().filter(move |a| self.has(*a))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    Foot,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occurrence {
    Discontinue,
    Every,
    Odd,
    Even,
}

impl Occurrence {
    pub fn name(self) -> &'static str {
        match self {
            Occurrence::Discontinue => "Discontinue",
            Occurrence::Every => "Every page",
            Occurrence::Odd => "Odd pages",
            Occurrence::Even => "Even pages",
        }
    }
    pub fn applies(self, page: u32) -> bool {
        match self {
            Occurrence::Discontinue => false,
            Occurrence::Every => true,
            Occurrence::Odd => page % 2 == 1,
            Occurrence::Even => page % 2 == 0,
        }
    }
}

/// Text column type (Alt-F7, 1 Columns, 3 Define, 1 Type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColKind {
    /// Text snakes down one column and continues at the top of the next.
    Newspaper,
    /// Side-by-side groups; Ctrl-Enter moves to the next column.
    Parallel,
    /// Parallel, and a group is never split by a page break.
    ParallelProtect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Left,
    Center,
    Right,
    Full,
}

impl Justify {
    pub fn name(self) -> &'static str {
        match self {
            Justify::Left => "Left",
            Justify::Center => "Center",
            Justify::Right => "Right",
            Justify::Full => "Full",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStop {
    /// Position in WPU, absolute from the left edge of the page.
    pub pos: i32,
    pub kind: TabKind,
    pub dot_leader: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSet {
    pub stops: Vec<TabStop>,
    /// WP 5.1 "Relative to margin" tabs.  Positions are then relative to
    /// the left margin rather than the page edge.
    pub relative: bool,
}

impl TabSet {
    /// WP 5.1 default: relative tabs every 0.5" from -1" to +... (we only
    /// keep the useful positive part).
    pub fn default_tabs() -> TabSet {
        TabSet {
            stops: (-2..=30)
                .map(|i| TabStop { pos: i * 600, kind: TabKind::Left, dot_leader: false })
                .collect(),
            relative: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontSpec {
    pub name: String,
    /// Point size × 100 (so 12pt = 1200).
    pub size_cpt: u32,
}

impl FontSpec {
    pub fn default_font() -> FontSpec {
        FontSpec { name: "Courier New".into(), size_cpt: 1200 }
    }
}

/// A format code that lives in the text stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Code {
    /// Hard return `[HRt]` — paragraph end.
    HRt,
    /// Hard page `[HPg]`.
    HPg,
    Tab,
    /// `[->Indent]` (F4).
    Indent,
    /// `[->Indent<-]` (Shift-F4).
    DblIndent,
    /// `[<-Mar Rel]` (Shift-Tab).
    MarRel,
    /// `[Center]` (Shift-F6) — centers text up to the next HRt/tab.
    Center,
    /// `[Flsh Rgt]` (Alt-F6).
    FlushRight,
    /// `[ ]` hard space (Home, Space).
    HardSpace,
    /// `[-]` hard hyphen (Home, -)… in WP 5.1 a plain '-' typed is a
    /// "hyphen character" shown as `[-]` too; we store a typed '-' as a
    /// normal char and use this for Home,- "hyphen character".
    HardHyphen,
    /// Soft hyphen `-` shown as `[- ]`… (Ctrl-hyphen).
    SoftHyphen,
    AttrOn(Attr),
    AttrOff(Attr),
    Font(FontSpec),
    /// `[L/R Mar:1",1"]`
    LRMargin { left: i32, right: i32 },
    /// `[T/B Mar:1",1"]`
    TBMargin { top: i32, bottom: i32 },
    /// `[Ln Spacing:1.5]` in hundredths (150).
    LineSpacing(u32),
    /// `[Just:Full]`
    Just(Justify),
    /// `[Tab Set:...]`
    TabSet(TabSet),
    /// `[Center Pg]`
    CenterPage,
    /// `[Comment]`
    Comment(String),
    /// `[Date:...]` — date code (Shift-F5, 2) with the WP format string.
    DateCode(String),
    /// `[Hyph On]` / `[Hyph Off]`
    Hyphenation(bool),
    /// `[W/O On]` / `[W/O Off]` — widow/orphan protection.
    WidowOrphan(bool),
    /// `[Pg Num:n]` new page number.
    PageNumber(u32),
    /// `[Pg Numbering:...]` position 0..9 (0 = none)
    PageNumberPos(u8),
    /// `[Suppress:...]`
    Suppress(u8),
    /// `[Footnote:1;[Note Num]…]` / `[Endnote:…]` — the note text lives
    /// inside the code, exactly like WP.  Numbers are automatic.
    Note { kind: NoteKind, els: Vec<El> },
    /// `[Header A:Every page;…]`, `[Footer B:…]`.
    /// which: 0 Header A, 1 Header B, 2 Footer A, 3 Footer B.
    HeadFoot { which: u8, occ: Occurrence, els: Vec<El> },
    /// `[Tbl Def:I;3,2",2",2"]` — start of a table; column widths in WPU.
    TblDef { cols: Vec<i32> },
    /// `[Row]` — start of a table row.
    Row,
    /// `[Cell]` — start of a table cell.
    Cell,
    /// `[Tbl Off]` — end of the table.
    TblOff,
    /// `[Col Def:Newspaper;2;1",4";4.5",7.5"]` — text column definition
    /// (Alt-F7, 1 Columns, 3 Define): left and right edge of every column,
    /// WPU from the left paper edge.
    ColDef { kind: ColKind, cols: Vec<(i32, i32)> },
    /// `[Col On]` — text columns on (uses the last [Col Def]).
    ColOn,
    /// `[Col Off]` — text columns off.
    ColOff,
    /// Tracked change start: inserted (`del` = false) or deleted text,
    /// with author and ISO date — Word's w:ins / w:del.
    RevOn { del: bool, author: String, date: String },
    /// Tracked change end.
    RevOff { del: bool },
    /// Merge command (WP 5.1 merge language), e.g. `FIELD`, `END RECORD`.
    /// Shown as `{FIELD}` on screen, `[Mrg:FIELD]` in Reveal Codes; its
    /// parameters follow as ordinary text, each ending with `~`.
    Merge(String),
    /// `[Math Def]` — math column definition (columns A–X).
    MathDef(Vec<MathCol>),
    /// `[Math On]` / `[Math Off]`.
    Math(bool),
    /// Math operator `+ = * t T ! N` (shown, not printed).
    MathOp(char),
    /// Table cell formula (Table Edit, Math, Formula): `A1+B1*2`, `+`,
    /// `=`, `*`.  Sits at the start of the cell; the computed value
    /// follows as ordinary text.
    Formula(String),
    /// `[Link:file;range]` — start of a spreadsheet link (Ctrl-F5, 5).
    SsLink { file: String, range: String, table: bool },
    /// `[Link End]`.
    SsLinkEnd,
    /// `[Link:url]` — start of a hyperlink (Word's w:hyperlink); `#name`
    /// points to a bookmark in the document.
    Link(String),
    /// `[Link End]` — end of the hyperlink text.
    LinkEnd,
    /// `[Bookmark:name]` — a named place in the document.
    Bookmark(String),
    /// `[Style On:Name]` — start of a paired style (Alt-F8).  The style's
    /// codes live in `Document::styles` and are applied by layout.
    StyleOn(String),
    /// `[Style Off:Name]` — end of a paired style.
    StyleOff(String),
    /// `[Open Style:Name]` — an open (non-paired) style.
    OpenStyle(String),
    /// `[Par Num:Auto]` (None: level from the tab position) or
    /// `[Par Num:3]` (fixed level 1..8).  The number is computed by layout.
    ParNum(Option<u8>),
    /// `[Par Num Def]` — paragraph numbering definition (Shift-F5, 6).
    ParNumDef(ParNumDef),
    /// `[Outline On]` / `[Outline Off]` — outline mode (Shift-F5, 4).
    Outline(bool),
    /// Further Shift-F8 Format codes (hyphenation zone, line height, line
    /// numbering, paper size, advance, language, hard tabs …) — see fmt.rs.
    Fmt(crate::fmt::Fmt),
    /// Graphics box (Alt-F9): `[Fig Box:1;FILE.WPG;]`, `[Tbl Box:…]`,
    /// `[Text Box:…]`, `[Usr Box:…]`, `[Equ Box:…]` — see gfx.rs.
    GBox(Box<crate::gfx::GBox>),
    /// Box options `[Fig Opt]`, `[Tbl Opt]`… for the following boxes.
    BoxOpt(Box<crate::gfx::BoxOpts>),
    /// `[New Fig Num:n]` …
    BoxNewNum(crate::gfx::BoxKind, u32),
    /// `[Box Num]` — the box number inside a caption.
    BoxNum,
    /// `[HLine:…]` / `[VLine:…]` graphic lines.
    GLine(crate::gfx::GLine),
    /// A code we imported but do not (yet) understand.  Kept verbatim so
    /// nothing is lost; shown with its WP name.
    Unknown { name: String, data: Vec<u8> },
    /// Mark Text (Alt-F5) codes: ToC/list/index/ToA marks, definitions,
    /// cross-references and master-document codes — see [`Mark`].
    Mark(Mark),
    /// Flush right with dot leader (`[Dot Flsh Rgt]`), used by generated
    /// tables of contents/lists/indexes.
    FlushRightDot,
}

/// Mark Text codes (WP 5.1 Alt-F5; WP 5.x group 0xD7 / 0x9B).
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// `[Mark:ToC,1]` (list = false: ToC level n) / `[Mark:List,n]`.
    Begin { list: bool, n: u8 },
    /// `[End Mark:ToC,1]` / `[End Mark:List,n]`.
    End { list: bool, n: u8 },
    /// `[Index:heading;subheading]`.
    Index { heading: String, sub: String },
    /// `[ToA:1;short form;full form]` — section 0 = short form only.
    ToA { section: u8, short: String, full: String },
    /// `[Def Mark:ToC,n:…]` — n levels; page-number style per level
    /// (0 none, 1 follows, 2 (follows), 3 flush right, 4 flush right + leader).
    DefToc { levels: u8, wrap_last: bool, styles: [u8; 5] },
    /// `[Def Mark:List,n:s]`.
    DefList { n: u8, style: u8 },
    /// `[Def Mark:Index,s]` with an optional concordance file.
    DefIndex { style: u8, concordance: String },
    /// `[Def Mark:ToA,n]`.
    DefToA { section: u8, dots: bool, underline: bool, blank: bool },
    /// `[End Def]` — end of generated text.
    EndDef,
    /// `[Ref(name):Pg 2]` — kind: 0 page, 1 paragraph, 2 footnote,
    /// 3 endnote, 4 figure, 5 table box, 6 text box, 7 user box, 8 equation;
    /// `text` is the value shown in the document (updated by Generate).
    Ref { name: String, kind: u8, text: String },
    /// `[Target(name)]`.
    Target { name: String },
    /// `[Subdoc:FILE]` (condensed master document).
    Subdoc(String),
    /// `[Subdoc Start:FILE]` / `[Subdoc End:FILE]` around expanded text.
    SubdocStart(String),
    SubdocEnd(String),
}

impl Mark {
    /// The reference kind's short name as WP shows it in `[Ref(..):Pg 2]`.
    pub fn ref_kind_name(kind: u8) -> &'static str {
        ["Pg", "Par", "Ftn", "End", "Fig", "Tbl", "Txt Box", "Usr Box", "Equ Box"].get(kind as usize).copied().unwrap_or("Pg")
    }
}

impl Code {
    /// Is this a paired-attribute code?
    pub fn attr(&self) -> Option<(Attr, bool)> {
        match self {
            Code::AttrOn(a) => Some((*a, true)),
            Code::AttrOff(a) => Some((*a, false)),
            _ => None,
        }
    }
}

/// One column of a math definition (Alt-F7, Math, Define).
#[derive(Clone, Debug, PartialEq)]
pub struct MathCol {
    /// 0 = Calculation, 1 = Text, 2 = Numeric, 3 = Total.
    pub kind: u8,
    /// Negative numbers in parentheses `(50.00)` (else `-50.00`).
    pub paren: bool,
    /// Number of digits to the right (0–4).
    pub digits: u8,
    /// Calculation formula (calculation columns only).
    pub formula: String,
}

impl MathCol {
    /// WP 5.1 default: 24 numeric columns, minus sign, 2 digits.
    pub fn defaults() -> Vec<MathCol> {
        (0..24).map(|_| MathCol { kind: 2, paren: false, digits: 2, formula: String::new() }).collect()
    }
}

/// One element of the document stream.
#[derive(Clone, Debug, PartialEq)]
pub enum El {
    Ch(char),
    Code(Code),
}

impl El {
    pub fn is_char(&self) -> bool {
        matches!(self, El::Ch(_))
    }
    pub fn code(&self) -> Option<&Code> {
        match self {
            El::Code(c) => Some(c),
            _ => None,
        }
    }
}

/// Style type (Alt-F8, Edit, 2 Type).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StyleKind {
    #[default]
    Paired,
    Open,
    Outline,
}

impl StyleKind {
    pub fn name(self) -> &'static str {
        match self {
            StyleKind::Paired => "Paired",
            StyleKind::Open => "Open",
            StyleKind::Outline => "Outline",
        }
    }
}

/// What Enter does inside a paired style (Alt-F8, Edit, 5 Enter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EnterAction {
    #[default]
    HRt,
    Off,
    OffOn,
}

impl EnterAction {
    pub fn name(self) -> &'static str {
        match self {
            EnterAction::HRt => "HRt",
            EnterAction::Off => "Off",
            EnterAction::OffOn => "Off/On",
        }
    }
}

/// A style definition (document style list or style library).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StyleDef {
    pub name: String,
    pub kind: StyleKind,
    pub description: String,
    /// Codes (and text) applied at `[Style On]` / `[Open Style]`.
    pub on: Vec<El>,
    /// Codes applied at `[Style Off]` (paired styles only).
    pub off: Vec<El>,
    pub enter: EnterAction,
}

/// Number style of one paragraph-numbering level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumFmt {
    Decimal,
    LowerLetter,
    UpperLetter,
    LowerRoman,
    UpperRoman,
    Bullet,
}

/// One level of a paragraph number definition.  `text` is a pattern in
/// which `%1`…`%8` stand for the numbers of levels 1…8 (Word's lvlText),
/// e.g. `%1.`, `(%2)`, `%1.%2` (legal: attached to the previous level) or
/// a bullet character.
#[derive(Clone, Debug, PartialEq)]
pub struct NumLevel {
    pub fmt: NumFmt,
    pub text: String,
    pub start: u32,
}

/// `[Par Num Def]`: Shift-F5, 6 Define.
#[derive(Clone, Debug, PartialEq)]
pub struct ParNumDef {
    /// Always 8 levels.
    pub levels: Vec<NumLevel>,
    pub enter_inserts: bool,
    pub auto_adjust: bool,
    pub outline_style: String,
}

/// Page setup / "document initial codes".
#[derive(Clone, Debug, PartialEq)]
pub struct InitialFormat {
    pub page_width: i32,
    pub page_height: i32,
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
    pub font: FontSpec,
    pub tabs: TabSet,
    pub justify: Justify,
    pub line_spacing: u32,
}

impl Default for InitialFormat {
    fn default() -> Self {
        // WP 5.1 defaults (1" margins, full justification), paper size from
        // the system locale (A4 in Europe, Letter in the US).
        let (pw, ph) = crate::sys::default_paper();
        InitialFormat {
            page_width: pw,
            page_height: ph,
            left: WPU_PER_INCH,
            right: WPU_PER_INCH,
            top: WPU_PER_INCH,
            bottom: WPU_PER_INCH,
            font: FontSpec::default_font(),
            tabs: TabSet::default_tabs(),
            justify: Justify::Full,
            line_spacing: 100,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub descriptive_name: String,
    pub subject: String,
    pub author: String,
    pub typist: String,
    pub comments: String,
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub els: Vec<El>,
    pub init: InitialFormat,
    /// Document initial codes (Shift-F8, 3 Document, 2 Initial Codes).
    pub initial_codes: Vec<El>,
    pub summary: Summary,
    /// Permanent document identity (for revision history); kept in the
    /// .docx custom properties, which Word preserves.
    pub id: String,
    /// Track changes is switched on for this document.
    pub track: bool,
    /// Ctrl-F5 Password: saved as an encrypted .docx (never written to disk
    /// in clear; not part of the code stream).
    pub password: Option<String>,
    /// The document's style list (Alt-F8).
    pub styles: Vec<StyleDef>,
    /// Format: Document settings (redline method, display pitch).
    pub opts: crate::fmt::DocOpts,
}

/// A new random-enough 128-bit id as 32 hex digits.
pub fn new_doc_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut out = String::new();
    for k in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        h.write_u128(t);
        h.write_u32(std::process::id());
        h.write_u64(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) ^ (k << 32));
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

impl Document {
    pub fn new() -> Self {
        Document::default()
    }

    pub fn from_text(s: &str) -> Self {
        let mut d = Document::new();
        for ch in s.chars() {
            match ch {
                '\n' => d.els.push(El::Code(Code::HRt)),
                '\r' => {}
                '\t' => d.els.push(El::Code(Code::Tab)),
                '\u{c}' => d.els.push(El::Code(Code::HPg)),
                c => d.els.push(El::Ch(c)),
            }
        }
        d
    }

    pub fn len(&self) -> usize {
        self.els.len()
    }

    pub fn is_empty(&self) -> bool {
        self.els.is_empty()
    }

    /// Plain text of a range (codes → nothing / \n / \t).
    pub fn plain_text(&self, from: usize, to: usize) -> String {
        let mut s = String::new();
        for e in &self.els[from.min(self.els.len())..to.min(self.els.len())] {
            match e {
                El::Ch(c) => s.push(*c),
                El::Code(Code::HRt) => s.push('\n'),
                El::Code(Code::HPg) => s.push('\u{c}'),
                El::Code(Code::Tab | Code::Indent | Code::DblIndent) => s.push('\t'),
                El::Code(Code::HardSpace) => s.push('\u{a0}'),
                El::Code(Code::HardHyphen) => s.push('-'),
                _ => {}
            }
        }
        s
    }

    /// Attribute state in effect *before* element `idx`.
    pub fn attrs_at(&self, idx: usize) -> AttrSet {
        let mut a = AttrSet::default();
        for e in &self.els[..idx.min(self.els.len())] {
            if let El::Code(c) = e {
                if let Some((at, on)) = c.attr() {
                    a.set(at, on);
                }
            }
        }
        a
    }

    /// For a paired attribute code at `idx`, find its partner.
    pub fn partner_of(&self, idx: usize) -> Option<usize> {
        // paired styles: [Style On:X] … [Style Off:X]
        match self.els.get(idx)? {
            El::Code(Code::StyleOn(n)) => {
                let off = El::Code(Code::StyleOff(n.clone()));
                return self.els[idx + 1..].iter().position(|e| *e == off).map(|p| p + idx + 1);
            }
            El::Code(Code::StyleOff(n)) => {
                let on = El::Code(Code::StyleOn(n.clone()));
                return self.els[..idx].iter().rposition(|e| *e == on);
            }
            _ => {}
        }
        let (attr, on) =self.els.get(idx)?.code()?.attr()?;
        if on {
            for (i, e) in self.els.iter().enumerate().skip(idx + 1) {
                match e.code().and_then(|c| c.attr()) {
                    Some((a, false)) if a == attr => return Some(i),
                    Some((a, true)) if a == attr => return None,
                    _ => {}
                }
            }
        } else {
            for i in (0..idx).rev() {
                match self.els[i].code().and_then(|c| c.attr()) {
                    Some((a, true)) if a == attr => return Some(i),
                    Some((a, false)) if a == attr => return None,
                    _ => {}
                }
            }
        }
        None
    }

    /// Remove redundant attribute codes created by editing:
    /// `[BOLD][bold]` with nothing in between, `[bold][BOLD]` adjacent, and
    /// off-codes with no matching on, on-codes left dangling at end.
    pub fn normalize_attrs(&mut self) {
        loop {
            let mut changed = false;
            let mut i = 0;
            while i + 1 < self.els.len() {
                let a = self.els[i].code().and_then(|c| c.attr());
                let b = self.els[i + 1].code().and_then(|c| c.attr());
                if let (Some((x, xon)), Some((y, yon))) = (a, b) {
                    if x == y && xon != yon {
                        self.els.drain(i..i + 2);
                        changed = true;
                        continue;
                    }
                }
                i += 1;
            }
            // Drop off-codes whose attr is not on, and duplicate on-codes.
            let mut state = AttrSet::default();
            let mut j = 0;
            while j < self.els.len() {
                if let Some((at, on)) = self.els[j].code().and_then(|c| c.attr()) {
                    if state.has(at) == on {
                        self.els.remove(j);
                        changed = true;
                        continue;
                    }
                    state.set(at, on);
                }
                j += 1;
            }
            if !changed {
                break;
            }
        }
    }
}

/// Setup → Environment → Units of Measure: '"' inches, 'c' cm, 'p' points,
/// 'w' 1200ths, 'u' WP 4.2 units.  (entry/display of numbers, status line)
pub static UNIT_ENTRY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(b'"');
pub static UNIT_STATUS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(b'"');

pub fn fmt_unit(wpu: i32, unit: u8) -> String {
    match unit {
        b'c' => fmt_dec(wpu as f64 * 2.54 / 1200.0, 2) + "c",
        b'p' => fmt_dec(wpu as f64 * 72.0 / 1200.0, 1) + "p",
        b'w' => format!("{wpu}w"),
        b'u' => fmt_dec(wpu as f64 / 120.0, 0) + "u",
        b'i' => fmt_inches_raw(wpu).replace('"', "i"),
        _ => fmt_inches_raw(wpu),
    }
}

fn fmt_dec(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

/// A measurement in the user's chosen unit (WP calls it "inches" but it
/// follows Setup: Units of Measure).
pub fn fmt_inches(wpu: i32) -> String {
    fmt_unit(wpu, UNIT_ENTRY.load(std::sync::atomic::Ordering::Relaxed))
}

/// For the status line (Ln / Pos).
pub fn fmt_status(wpu: i32) -> String {
    fmt_unit(wpu, UNIT_STATUS.load(std::sync::atomic::Ordering::Relaxed))
}

/// Render a WPU distance in inches as WP shows it: `1"`, `1.5"`, `0.33"`.
pub fn fmt_inches_raw(wpu: i32) -> String {
    let hundredths = ((wpu as i64 * 100) as f64 / WPU_PER_INCH as f64).round() as i64;
    let neg = hundredths < 0;
    let h = hundredths.abs();
    let mut s = format!("{}", h / 100);
    let frac = h % 100;
    if frac != 0 {
        if frac % 10 == 0 {
            s.push_str(&format!(".{}", frac / 10));
        } else {
            s.push_str(&format!(".{:02}", frac));
        }
    }
    if neg {
        s.insert(0, '-');
    }
    s.push('"');
    s
}

/// Parse a measurement typed by the user (`1.5`, `1.5"`, `1 1/2`, `3c`
/// cm, `36p` points, `10m` mm…) into WPU.
pub fn parse_measure(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, unit) = match s.char_indices().find(|(_, c)| c.is_ascii_alphabetic() || *c == '"' || *c == '\'') {
        Some((i, _)) => (s[..i].trim(), s[i..].trim()),
        None => (s, ""),
    };
    let mut val = 0f64;
    // Support "1 1/2"
    for part in num.split_whitespace() {
        if let Some((a, b)) = part.split_once('/') {
            let a: f64 = a.trim().replace(',', ".").parse().ok()?;
            let b: f64 = b.trim().replace(',', ".").parse().ok()?;
            if b == 0.0 {
                return None;
            }
            val += a / b;
        } else {
            val += part.replace(',', ".").parse::<f64>().ok()?;
        }
    }
    let default_unit = UNIT_ENTRY.load(std::sync::atomic::Ordering::Relaxed);
    let unit = if unit.is_empty() { (default_unit as char).to_string() } else { unit.to_string() };
    let inches = match unit.to_ascii_lowercase().as_str() {
        "\"" | "i" | "in" => val,
        "c" | "cm" => val / 2.54,
        "m" | "mm" => val / 25.4,
        "p" | "pt" => val / 72.0,
        "w" => val / 1200.0,
        "u" => val / 6.0, // WP 4.2 units: lines/columns — approx
        _ => return None,
    };
    Some((inches * WPU_PER_INCH as f64).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inches_format() {
        assert_eq!(fmt_inches(1200), "1\"");
        assert_eq!(fmt_inches(1800), "1.5\"");
        assert_eq!(fmt_inches(1400), "1.17\"");
        assert_eq!(fmt_inches(0), "0\"");
    }

    #[test]
    fn measure_parse() {
        assert_eq!(parse_measure("1.5"), Some(1800));
        assert_eq!(parse_measure("1 1/2\""), Some(1800));
        assert_eq!(parse_measure("2,54c"), Some(1200));
        assert_eq!(parse_measure("72p"), Some(1200));
    }

    #[test]
    fn normalize() {
        let mut d = Document::new();
        d.els = vec![
            El::Code(Code::AttrOn(Attr::Bold)),
            El::Code(Code::AttrOff(Attr::Bold)),
            El::Ch('a'),
            El::Code(Code::AttrOff(Attr::Italic)),
        ];
        d.normalize_attrs();
        assert_eq!(d.els, vec![El::Ch('a')]);
    }
}
