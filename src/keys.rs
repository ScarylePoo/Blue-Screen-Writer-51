//! Keyboard model.
//!
//! Physical keystrokes (`KeyIn`) are translated through the active keyboard
//! layout into WordPerfect functions (`Func`) or characters.  Macros play
//! back the same `Input` values, so everything in the program is driven by
//! one input stream — exactly how WP 5.1 macros work (they replay keys).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum K {
    Char(char),
    F(u8),
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PgUp,
    PgDn,
    Ins,
    Del,
    Backspace,
    Enter,
    Tab,
    Esc,
    /// Grey + / - on the numeric keypad (Screen Down / Screen Up).
    PadPlus,
    PadMinus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyIn {
    pub k: K,
    pub m: Mods,
}

/// Every WordPerfect 5.1 editing function.  Names follow the WP 5.1
/// macro editor (`{Bold}`, `{Exit}` …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Func {
    // --- cursor / editing keys
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    PgUp,
    PgDn,
    ScreenUp,
    ScreenDown,
    GoTo,
    Enter,
    Tab,
    MarRel,
    Backspace,
    Del,
    DelWord,
    DelEol,
    DelEop,
    Typeover,
    Esc,
    HardPage,
    HardSpace,
    SoftHyphen,
    Compose,
    ParaUp,
    ParaDown,
    // --- function keys (WP 5.1 template)
    Cancel,       // F1
    SearchFwd,    // F2
    Help,         // F3
    Indent,       // F4
    List,         // F5
    Bold,         // F6
    Exit,         // F7
    Underline,    // F8
    EndField,     // F9
    Save,         // F10
    RevealCodes,  // F11 / Alt-F3
    Block,        // F12 / Alt-F4
    Setup,        // Shift-F1
    SearchBack,   // Shift-F2
    Switch,       // Shift-F3
    DblIndent,    // Shift-F4
    DateOutline,  // Shift-F5
    Center,       // Shift-F6
    Print,        // Shift-F7
    Format,       // Shift-F8
    MergeCodes,   // Shift-F9
    Retrieve,     // Shift-F10
    Thesaurus,    // Alt-F1
    Replace,      // Alt-F2
    MarkText,     // Alt-F5
    FlushRight,   // Alt-F6
    ColumnsTable, // Alt-F7
    Style,        // Alt-F8
    Graphics,     // Alt-F9
    Macro,        // Alt-F10
    Shell,        // Ctrl-F1
    Spell,        // Ctrl-F2
    Screen,       // Ctrl-F3
    Move,         // Ctrl-F4
    TextInOut,    // Ctrl-F5
    TabAlign,     // Ctrl-F6
    Footnote,     // Ctrl-F7
    Font,         // Ctrl-F8
    MergeSort,    // Ctrl-F9
    MacroDefine,  // Ctrl-F10
    /// Alt-letter macro (runs ALTx.WPM).
    AltLetter(char),
    /// Ctrl-letter that has no built-in meaning (may be remapped by WPK).
    CtrlLetter(char),
    /// Window close button — behaves like Exit, then confirms.
    CloseWindow,
    /// Alt-= : the pull-down menu bar.
    MenuBar,
    /// Ctrl-PgUp: macro commands (Pause/Display/Assign/Comment).
    MacroCommands,
    /// Alt-arrows: move by item (table cell / column), Alt-Home prefix.
    ItemLeft,
    ItemRight,
    ItemUp,
    ItemDown,
    AltHome,
    /// The {Keyboard} key: Setup: Keyboard Layout.
    Keyboard,
    /// A WP key code (0x80xx macro key) that has no dedicated variant yet.
    Other(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Char(char),
    Func(Func),
}

/// The original WP 5.1 keyboard (no .WPK loaded).
pub fn default_map(k: KeyIn) -> Option<Input> {
    use Func::*;
    let m = k.m;
    let f = |x: Func| Some(Input::Func(x));
    match k.k {
        K::F(n) => {
            let r = match (n, m.shift, m.ctrl, m.alt) {
                (1, false, false, false) => Cancel,
                (2, false, false, false) => SearchFwd,
                (3, false, false, false) => Help,
                (4, false, false, false) => Indent,
                (5, false, false, false) => List,
                (6, false, false, false) => Bold,
                (7, false, false, false) => Exit,
                (8, false, false, false) => Underline,
                (9, false, false, false) => EndField,
                (10, false, false, false) => Save,
                (11, false, false, false) => RevealCodes,
                (12, false, false, false) => Block,
                (1, true, false, false) => Setup,
                (2, true, false, false) => SearchBack,
                (3, true, false, false) => Switch,
                (4, true, false, false) => DblIndent,
                (5, true, false, false) => DateOutline,
                (6, true, false, false) => Center,
                (7, true, false, false) => Print,
                (8, true, false, false) => Format,
                (9, true, false, false) => MergeCodes,
                (10, true, false, false) => Retrieve,
                (1, false, false, true) => Thesaurus,
                (2, false, false, true) => Replace,
                (3, false, false, true) => RevealCodes,
                (4, false, false, true) => Block,
                (5, false, false, true) => MarkText,
                (6, false, false, true) => FlushRight,
                (7, false, false, true) => ColumnsTable,
                (8, false, false, true) => Style,
                (9, false, false, true) => Graphics,
                (10, false, false, true) => Macro,
                (1, false, true, false) => Shell,
                (2, false, true, false) => Spell,
                (3, false, true, false) => Screen,
                (4, false, true, false) => Move,
                (5, false, true, false) => TextInOut,
                (6, false, true, false) => TabAlign,
                (7, false, true, false) => Footnote,
                (8, false, true, false) => Font,
                (9, false, true, false) => MergeSort,
                (10, false, true, false) => MacroDefine,
                // Enhanced keyboard extras
                (11, true, false, false) | (11, false, false, true) => RevealCodes,
                (12, true, false, false) | (12, false, false, true) => Block,
                _ => return None,
            };
            f(r)
        }
        K::Left if m.alt && !m.ctrl => f(ItemLeft),
        K::Right if m.alt && !m.ctrl => f(ItemRight),
        K::Up if m.alt && !m.ctrl => f(ItemUp),
        K::Down if m.alt && !m.ctrl => f(ItemDown),
        K::Home if m.alt && !m.ctrl => f(AltHome),
        K::PgUp if m.ctrl => f(MacroCommands),
        K::Left if m.ctrl => f(WordLeft),
        K::Right if m.ctrl => f(WordRight),
        K::Up if m.ctrl => f(ParaUp),
        K::Down if m.ctrl => f(ParaDown),
        K::Left => f(Left),
        K::Right => f(Right),
        K::Up => f(Up),
        K::Down => f(Down),
        K::Home if m.ctrl => f(GoTo),
        K::Home => f(Home),
        K::End if m.ctrl => f(DelEol),
        K::End => f(End),
        K::PgUp => f(PgUp),
        K::PgDn if m.ctrl => f(DelEop),
        K::PgDn => f(PgDn),
        // CUA clipboard keys (free in WP 5.1)
        K::Ins if m.shift && !m.ctrl => f(Other(crate::clipboard::CLIP_PASTE)),
        K::Ins if m.ctrl && !m.shift => f(Other(crate::clipboard::CLIP_COPY)),
        K::Del if m.shift && !m.ctrl => f(Other(crate::clipboard::CLIP_CUT)),
        K::Ins => f(Typeover),
        K::Del => f(Del),
        K::Backspace if m.ctrl => f(DelWord),
        K::Backspace => f(Backspace),
        K::Enter if m.ctrl => f(HardPage),
        K::Enter => f(Enter),
        K::Tab if m.shift => f(MarRel),
        K::Tab => f(Tab),
        K::Esc => f(Esc),
        K::PadPlus => f(ScreenDown),
        K::PadMinus => f(ScreenUp),
        K::Char('=') if m.alt && !m.ctrl => f(MenuBar),
        K::Char(c) if m.alt && !m.ctrl => {
            let c = c.to_ascii_uppercase();
            if c.is_ascii_alphabetic() { f(AltLetter(c)) } else { None }
        }
        K::Char(c) if m.ctrl && !m.alt => match c.to_ascii_lowercase() {
            'v' | '2' => f(Compose),
            '-' => f(SoftHyphen),
            '\\' | ']' => f(CtrlLetter(c)),
            c if c.is_ascii_alphabetic() => f(CtrlLetter(c.to_ascii_uppercase())),
            _ => None,
        },
        // AltGr on European keyboards arrives as Ctrl+Alt+char: plain text.
        K::Char(c) => Some(Input::Char(c)),
    }
}

impl Func {
    /// Name as shown in the WP 5.1 macro editor, e.g. `{Bold}`.
    pub fn macro_name(self) -> String {
        use Func::*;
        let s = match self {
            Left => "Left",
            Right => "Right",
            Up => "Up",
            Down => "Down",
            WordLeft => "Word Left",
            WordRight => "Word Right",
            Home => "Home",
            End => "End",
            PgUp => "Page Up",
            PgDn => "Page Down",
            ScreenUp => "Screen Up",
            ScreenDown => "Screen Down",
            GoTo => "Goto",
            Enter => "Enter",
            Tab => "Tab",
            MarRel => "Left Mar Rel",
            Backspace => "Backspace",
            Del => "Del",
            DelWord => "Del Word",
            DelEol => "Del to EOL",
            DelEop => "Del to EOP",
            Typeover => "Typeover",
            Esc => "Esc",
            HardPage => "HPg",
            HardSpace => "Hard Space",
            SoftHyphen => "Soft Hyphen",
            Compose => "Compose",
            ParaUp => "Para Up",
            ParaDown => "Para Down",
            Cancel => "Cancel",
            SearchFwd => "Search",
            Help => "Help",
            Indent => "Indent",
            List => "List",
            Bold => "Bold",
            Exit => "Exit",
            Underline => "Underline",
            EndField => "End Field",
            Save => "Save",
            RevealCodes => "Reveal Codes",
            Block => "Block",
            Setup => "Setup",
            SearchBack => "Search Left",
            Switch => "Switch",
            DblIndent => "L/R Indent",
            DateOutline => "Date/Outline",
            Center => "Center",
            Print => "Print",
            Format => "Format",
            MergeCodes => "Merge Codes",
            Retrieve => "Retrieve",
            Thesaurus => "Thesaurus",
            Replace => "Replace",
            MarkText => "Mark Text",
            FlushRight => "Flush Right",
            ColumnsTable => "Columns/Table",
            Style => "Style",
            Graphics => "Graphics",
            Macro => "Macro",
            Shell => "Shell",
            Spell => "Spell",
            Screen => "Screen",
            Move => "Move",
            TextInOut => "Text In/Out",
            TabAlign => "Tab Align",
            Footnote => "Footnote",
            Font => "Font",
            MergeSort => "Merge/Sort",
            MacroDefine => "Macro Define",
            AltLetter(c) => return format!("{{Alt{c}}}"),
            CtrlLetter(c) => return format!("{{^{c}}}"),
            CloseWindow => "Exit",
            MenuBar => "Menu Bar",
            MacroCommands => "Macro Commands",
            ItemLeft => "Item Left",
            ItemRight => "Item Right",
            ItemUp => "Item Up",
            ItemDown => "Item Down",
            AltHome => "Alt Home",
            Keyboard => "Keyboard",
            Other(c) => return format!("{{KEY {c:#06x}}}"),
        };
        format!("{{{s}}}")
    }
}

/// Her keyboard: ALTRNAT.WPK — F1 Help, Esc Cancel, F3 Repeat (Esc).
pub fn alternate_map(k: KeyIn) -> Option<Input> {
    let plain = !k.m.shift && !k.m.ctrl && !k.m.alt;
    match k.k {
        K::F(1) if plain => Some(Input::Func(Func::Help)),
        K::Esc if plain => Some(Input::Func(Func::Cancel)),
        K::F(3) if plain => Some(Input::Func(Func::Esc)),
        _ => default_map(k),
    }
}
