//! WordPerfect 5.1 macros (.WPM): the file format, the keystroke encoding
//! and the macro interpreter with all 64 programming commands.
//!
//! Format reference: docs/formats/wpm_wpk_set.md, command table:
//! docs/formats/wp51_macro_commands.tsv, key codes: wp51_keycodes.tsv.
//!
//! How a macro runs: [`run_file`] / [`run_body`] push a frame onto the
//! [`MacroVm`] that lives in `Wp::macro_vm` (it stays there between macros
//! so that variables — `{VAR 0}`…`{VAR 9}` and named ones — are global and
//! persistent, as in WP).  The event loop calls [`step`]; every call runs
//! one keystroke or one command.  Keystrokes are fed through `Wp::input`
//! exactly as if typed.  Commands that wait for the user ({CHAR}, {TEXT},
//! {PAUSE}, {INPUT}, {PAUSE KEY}, {STEP ON}) make `step` return false until
//! [`feed_key`] delivers the key.  Keys pressed while the macro runs are
//! buffered (type-ahead, read by {LOOK}/{CHAR}); Cancel aborts unless
//! {CANCEL OFF}.

use crate::doc::{Attr, Code, El};
use crate::grid::{F_UNDER, Grid};
use crate::keys::{Func, Input};
use crate::screens::{MenuLine, Next, Prompt};
use crate::ui::{Outcome, Screen, ScreenKind, Wp};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

// =================================================================== files

/// The common `\xFFWPC` container of WP 5.x auxiliary files.
pub struct Wpc<'a> {
    pub body_off: usize,
    pub ftype: u8,
    pub major: u8,
    pub minor: u8,
    pub packets: Vec<(u16, &'a [u8])>,
}

impl<'a> Wpc<'a> {
    pub fn packet(&self, t: u16) -> Option<&'a [u8]> {
        self.packets.iter().find(|p| p.0 == t).map(|p| p.1)
    }
}

fn rd16(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*d.get(o)?, *d.get(o + 1)?]))
}

fn rd32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

/// Parse the WPC prefix and walk the chain of index blocks.
pub fn parse_wpc(d: &[u8]) -> Result<Wpc<'_>, String> {
    if d.len() < 16 || d[..4] != [0xFF, b'W', b'P', b'C'] {
        return Err("not a WordPerfect file".into());
    }
    if rd16(d, 12) != Some(0) {
        return Err("file is locked (password)".into());
    }
    let body_off = rd32(d, 4).unwrap_or(0) as usize;
    let mut packets = Vec::new();
    let mut pos = 16usize;
    let mut seen = HashSet::new();
    while pos != 0 && seen.insert(pos) {
        if rd16(d, pos) != Some(0xFFFB) {
            break;
        }
        let cnt = rd16(d, pos + 2).unwrap_or(0) as usize;
        let next = rd32(d, pos + 6).unwrap_or(0) as usize;
        for k in 1..cnt {
            let e = pos + 10 * k;
            let (Some(t), Some(len), Some(off)) = (rd16(d, e), rd32(d, e + 2), rd32(d, e + 6)) else { break };
            if t == 0 {
                continue;
            }
            let off = (off as usize).min(d.len());
            let end = off.saturating_add(len as usize).min(d.len());
            packets.push((t, &d[off..end]));
        }
        pos = next;
    }
    Ok(Wpc { body_off, ftype: d[9], major: d[10], minor: d[11], packets })
}

/// Little-endian u16 words up to (not including) a 0x0000 terminator or EOF.
pub fn words_of(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&w| w != 0).collect()
}

/// WP-document-encoded, NUL terminated string (`C0 idx set C0` = WP char).
pub fn decode_wp_string(b: &[u8]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < b.len() && b[i] != 0 {
        if b[i] == 0xC0 && i + 3 < b.len() && b[i + 3] == 0xC0 {
            s.push_str(&crate::wpchars::wp_to_string(b[i + 2], b[i + 1] as u16));
            i += 4;
            continue;
        }
        let c = b[i];
        s.push(if (0x20..0x7F).contains(&c) { c as char } else { '?' });
        i += 1;
    }
    s
}

/// Inverse of [`decode_wp_string`], limited to `max` bytes (no NUL).
pub fn encode_wp_string(s: &str, max: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for c in s.chars() {
        let enc: Vec<u8> = if (' '..='~').contains(&c) {
            vec![c as u8]
        } else {
            match crate::wpchars::unicode_to_wp(c) {
                Some((set, idx)) if set > 0 && idx < 256 => vec![0xC0, idx as u8, set, 0xC0],
                _ => vec![b'?'],
            }
        };
        if out.len() + enc.len() > max {
            break;
        }
        out.extend(enc);
    }
    out
}

/// A parsed .WPM file.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct WpmFile {
    pub description: String,
    pub words: Vec<u16>,
    pub version: (u8, u8),
}

pub fn parse_wpm(d: &[u8]) -> Result<WpmFile, String> {
    let f = parse_wpc(d)?;
    if f.ftype != 1 {
        return Err(format!("not a macro file (type {})", f.ftype));
    }
    let description = f.packet(1).map(decode_wp_string).unwrap_or_default();
    let words = if f.body_off <= d.len() { words_of(&d[f.body_off..]) } else { Vec::new() };
    Ok(WpmFile { description, words, version: (f.major, f.minor) })
}

pub fn load_wpm(p: &Path) -> Result<WpmFile, String> {
    let d = std::fs::read(p).map_err(|e| e.to_string())?;
    parse_wpm(&d)
}

/// Build a WP 5.1 .WPM file (same layout as WP writes).
pub fn encode_wpm(desc: &str, words: &[u16]) -> Vec<u8> {
    let mut dsc = encode_wp_string(desc, 39);
    dsc.push(0);
    let body_off = 0x38 + dsc.len();
    let mut out = Vec::with_capacity(body_off + words.len() * 2 + 2);
    out.extend_from_slice(b"\xFFWPC");
    out.extend_from_slice(&(body_off as u32).to_le_bytes());
    out.extend_from_slice(&[1, 1, 1, 1, 0, 0, 0, 0]);
    out.extend_from_slice(&[0xFB, 0xFF, 4, 0, 40, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(dsc.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x38u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 20]);
    out.extend_from_slice(&dsc);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

// =========================================================== key encoding

/// WP key code 0x80xx → our function.  Codes without a WP 5.1 Native
/// equivalent become `Func::Other(code)`.
pub fn key_func(code: u16) -> Func {
    use Func::*;
    let ctrl = |c: u16| CtrlLetter((b'@' + (c - 0x8000) as u8) as char);
    match code {
        0x8000 | 0x8016 => Compose,
        0x8001..=0x8007 | 0x800D..=0x8015 => ctrl(code),
        0x8008 => Home,
        0x8009 => Tab,
        0x800A => Enter,
        0x800B => DelEol,
        0x800C => DelEop,
        0x8017 => Up,
        0x8018 => Right,
        0x8019 => Left,
        0x801A => Down,
        0x801B => Esc,
        0x801C | 0x801D | 0x801F => ctrl(code),
        0x801E => Keyboard,
        0x8020 => Cancel,
        0x8021 => SearchFwd,
        0x8022 => Help,
        0x8023 => Indent,
        0x8024 => List,
        0x8025 => Bold,
        0x8026 => Exit,
        0x8027 => Underline,
        0x8028 => EndField,
        0x8029 => Save,
        // F11 / F12 (and their Shift/Alt/Ctrl variants) on the enhanced keyboard
        0x802A | 0x8036 | 0x8042 | 0x804E => RevealCodes,
        0x802B | 0x8037 | 0x8043 | 0x804F => Block,
        0x802C => Setup,
        0x802D => SearchBack,
        0x802E => Switch,
        0x802F => DblIndent,
        0x8030 => DateOutline,
        0x8031 => Center,
        0x8032 => Print,
        0x8033 => Format,
        0x8034 => MergeCodes,
        0x8035 => Retrieve,
        0x8038 => Thesaurus,
        0x8039 => Replace,
        0x803A => RevealCodes,
        0x803B => Block,
        0x803C => MarkText,
        0x803D => FlushRight,
        0x803E => ColumnsTable,
        0x803F => Style,
        0x8040 => Graphics,
        0x8041 => Macro,
        0x8044 => Shell,
        0x8045 => Spell,
        0x8046 => Screen,
        0x8047 => Move,
        0x8048 => TextInOut,
        0x8049 => TabAlign,
        0x804A => Footnote,
        0x804B => Font,
        0x804C => MergeSort,
        0x804D => MacroDefine,
        0x8050 => Backspace,
        0x8051 => Del,
        0x8052 => DelWord,
        0x8053 => WordRight,
        0x8054 => WordLeft,
        0x8055 => End,
        0x8058 => GoTo,
        0x8059 => PgUp,
        0x805A => PgDn,
        0x805B => ScreenDown,
        0x805C => ScreenUp,
        0x805D => Typeover,
        0x805E => MarRel,
        0x805F => HardPage,
        0x8060 => SoftHyphen,
        0x8062 => HardSpace,
        0x8063 => ParaUp,
        0x8064 => ParaDown,
        0x8065 => ItemLeft,
        0x8066 => ItemRight,
        0x8067 => ItemUp,
        0x8068 => ItemDown,
        0x8069 => AltHome,
        0x806A => DelWord,
        0x806B => MenuBar,
        _ => Other(code),
    }
}

/// Our function → WP key code (as stored in a macro).
pub fn func_code(f: Func) -> u16 {
    use Func::*;
    match f {
        Compose => 0x8016,
        CtrlLetter(c) => {
            let c = c.to_ascii_uppercase();
            if c.is_ascii_uppercase() || matches!(c, '\\' | ']' | '_') { 0x8000 + (c as u16 - 0x40) } else { 0x8057 }
        }
        RevealCodes => 0x803A,
        Block => 0x803B,
        DelWord => 0x8052,
        MacroCommands => 0xFC2C,
        AltLetter(c) => 0xFE00 | (c.to_ascii_uppercase() as u16 & 0xFF),
        CloseWindow => 0x8026,
        Other(c) => c,
        _ => (0x8000u16..0x8080).find(|&c| key_func(c) == f).unwrap_or(0x8057),
    }
}

/// Character / WP character word → Unicode.
pub fn word_char(w: u16) -> Option<char> {
    match w {
        0x20..=0x7E => Some(w as u8 as char),
        0x0100..=0x0CFF => crate::wpchars::wp_to_unicode((w >> 8) as u8, w & 0xFF),
        _ => None,
    }
}

pub fn char_word(c: char) -> Option<u16> {
    let (set, idx) = crate::wpchars::unicode_to_wp(c)?;
    if set == 0 {
        (0x20..0x7F).contains(&idx).then_some(idx)
    } else if idx < 256 && set <= 12 {
        Some((set as u16) << 8 | idx)
    } else {
        None
    }
}

/// An input (typed char or function) → macro word.
pub fn input_word(i: &Input) -> Option<u16> {
    match i {
        Input::Char(c) => char_word(*c),
        Input::Func(f) => Some(func_code(*f)),
    }
}

// ============================================================ tokenizing

fn is_fmt(w: u16) -> bool {
    matches!(w, 0x0002 | 0x0009 | 0x000A | 0x000D)
}

/// Number of `~`-terminated arguments (-1 = list ending in an empty one).
fn nargs(w: u16) -> i8 {
    match w {
        0xFC01 | 0xFC0A | 0xFC28 | 0xFC3E => 2,
        0xFC07 | 0xFC08 | 0xFC13 => -1,
        0xFC12 => 4,
        0xFC36 => 3,
        0xFC04 | 0xFC09 | 0xFC0B | 0xFC14..=0xFC18 | 0xFC1A..=0xFC1D | 0xFC1F | 0xFC26 | 0xFC2A | 0xFC2B | 0xFC2F
        | 0xFC32..=0xFC35 | 0xFC37..=0xFC39 | 0xFC3B | 0xFC3F => 1,
        _ => 0,
    }
}

/// Read one argument starting at `i`; nested commands consume their own
/// tildes.  Returns (range, next index, terminated?).
fn parse_arg(w: &[u16], mut i: usize, end: usize) -> (Range<usize>, usize, bool) {
    let start = i;
    while i < end {
        if w[i] == 0x7E {
            return (start..i, i + 1, true);
        }
        if nargs(w[i]) != 0 {
            i = parse_cmd(w, i, end).2;
            continue;
        }
        i += 1;
    }
    (start..end, end, false)
}

/// Parse the command at `i` with its arguments.
fn parse_cmd(w: &[u16], i: usize, end: usize) -> (u16, Vec<Range<usize>>, usize) {
    let code = w[i];
    let mut i = i + 1;
    let n = nargs(code);
    let mut args = Vec::new();
    if n > 0 {
        for _ in 0..n {
            let (r, nx, ok) = parse_arg(w, i, end);
            args.push(r);
            i = nx;
            if !ok {
                break;
            }
        }
    } else if n < 0 {
        let (r, nx, ok) = parse_arg(w, i, end);
        args.push(r);
        i = nx;
        if ok {
            loop {
                // An empty argument (after formatting and comments) ends the list.
                let mut j = i;
                while j < end {
                    if is_fmt(w[j]) {
                        j += 1;
                    } else if w[j] == 0xFC0B {
                        j = parse_arg(w, j + 1, end).1;
                    } else {
                        break;
                    }
                }
                if j >= end {
                    i = end;
                    break;
                }
                if w[j] == 0x7E {
                    i = j + 1;
                    break;
                }
                let (r, nx, ok) = parse_arg(w, i, end);
                args.push(r);
                i = nx;
                if !ok {
                    break;
                }
            }
        }
    }
    (code, args, i)
}

#[derive(Clone, Debug)]
enum Tok {
    /// A keystroke / character word.
    W(u16),
    /// A programming command with its argument word ranges.
    C { code: u16, args: Vec<Range<usize>>, span: Range<usize> },
    /// End of the macro body (false) or of a handler action (true).
    End(bool),
}

struct Macro {
    name: String,
    words: Vec<u16>,
    toks: Vec<Tok>,
    labels: HashMap<String, usize>,
    /// IF→ELSE/END IF, ELSE→END IF, WHILE↔END WHILE, FOR↔END FOR.
    partner: HashMap<usize, usize>,
    /// BREAK / NEXT → head token of the enclosing structure.
    enclosing: HashMap<usize, usize>,
    /// ON CANCEL/ERROR/NOT FOUND token → first token of its action (None = empty).
    actions: HashMap<usize, Option<usize>>,
}

fn tokenize(w: &[u16], r: Range<usize>, toks: &mut Vec<Tok>, todo: &mut Vec<(usize, Range<usize>)>) {
    let (mut i, end) = (r.start, r.end);
    while i < end {
        let x = w[i];
        if x == 0 {
            break;
        }
        if is_fmt(x) {
            i += 1;
            continue;
        }
        if nargs(x) != 0 {
            let (code, args, nx) = parse_cmd(w, i, end);
            if code != 0xFC0B {
                if matches!(code, 0xFC1B..=0xFC1D) {
                    todo.push((toks.len(), args.first().cloned().unwrap_or(nx..nx)));
                }
                toks.push(Tok::C { code, args, span: i..nx });
            }
            i = nx;
            continue;
        }
        if (0xFC00..=0xFC3F).contains(&x) && x != 0xFC2C && x != 0xFC3A {
            toks.push(Tok::C { code: x, args: Vec::new(), span: i..i + 1 });
        } else {
            toks.push(Tok::W(x));
        }
        i += 1;
    }
}

/// Argument words without formatting and comments (no expansion).
fn literal(w: &[u16], r: Range<usize>) -> Vec<u16> {
    let mut out = Vec::new();
    let mut i = r.start;
    while i < r.end {
        if is_fmt(w[i]) {
            i += 1;
        } else if w[i] == 0xFC0B {
            i = parse_cmd(w, i, r.end).2;
        } else {
            out.push(w[i]);
            i += 1;
        }
    }
    out
}

fn compile(name: &str, words: Vec<u16>) -> Macro {
    let mut toks = Vec::new();
    let mut todo = Vec::new();
    tokenize(&words, 0..words.len(), &mut toks, &mut todo);
    toks.push(Tok::End(false));
    let main_len = toks.len();
    let mut actions = HashMap::new();
    while let Some((ti, r)) = todo.pop() {
        if literal(&words, r.clone()).is_empty() {
            actions.insert(ti, None);
        } else {
            actions.insert(ti, Some(toks.len()));
            tokenize(&words, r, &mut toks, &mut todo);
            toks.push(Tok::End(true));
        }
    }
    let mut labels = HashMap::new();
    for (i, t) in toks[..main_len].iter().enumerate() {
        if let Tok::C { code: 0xFC16, args, .. } = t {
            if let Some(a) = args.first() {
                labels.entry(norm_name(&literal(&words, a.clone()))).or_insert(i + 1);
            }
        }
    }
    // Static structure: match IF/ELSE/END IF, WHILE, FOR.
    let mut partner = HashMap::new();
    let mut enclosing = HashMap::new();
    let mut else_of: HashMap<usize, usize> = HashMap::new();
    let mut stack: Vec<(u8, usize)> = Vec::new(); // 0 IF, 1 WHILE, 2 FOR
    for (i, t) in toks.iter().enumerate() {
        let code = match t {
            Tok::End(_) => {
                stack.clear();
                continue;
            }
            Tok::W(_) => continue,
            Tok::C { code, .. } => *code,
        };
        let close = |stack: &mut Vec<(u8, usize)>, k: u8| -> Option<usize> {
            let p = stack.iter().rposition(|e| e.0 == k)?;
            let h = stack[p].1;
            stack.truncate(p);
            Some(h)
        };
        match code {
            0xFC15 | 0xFC2F => stack.push((0, i)),
            0xFC0E => {
                if let Some(&(_, h)) = stack.iter().rev().find(|e| e.0 == 0) {
                    if let std::collections::hash_map::Entry::Vacant(v) = else_of.entry(h) {
                        v.insert(i);
                        partner.insert(h, i);
                    }
                }
            }
            0xFC10 => {
                if let Some(h) = close(&mut stack, 0) {
                    let from = else_of.get(&h).copied().unwrap_or(h);
                    partner.insert(from, i);
                }
            }
            0xFC2B => stack.push((1, i)),
            0xFC12 | 0xFC13 => stack.push((2, i)),
            0xFC11 | 0xFC0F => {
                if let Some(h) = close(&mut stack, if code == 0xFC11 { 1 } else { 2 }) {
                    partner.insert(h, i);
                    partner.insert(i, h);
                }
            }
            0xFC03 => {
                if let Some(e) = stack.iter().rev().find(|e| e.0 != 0).or(stack.last()) {
                    enclosing.insert(i, e.1);
                }
            }
            0xFC19 => {
                if let Some(e) = stack.iter().rev().find(|e| e.0 != 0) {
                    enclosing.insert(i, e.1);
                }
            }
            _ => {}
        }
    }
    Macro { name: name.to_string(), words, toks, labels, partner, enclosing, actions }
}

impl Macro {
    /// First End token at or after `from`.
    fn seg_end(&self, from: usize) -> usize {
        (from..self.toks.len()).find(|&k| matches!(self.toks[k], Tok::End(_))).unwrap_or(self.toks.len())
    }
    fn is_code(&self, k: usize, c: u16) -> bool {
        matches!(self.toks.get(k), Some(Tok::C { code, .. }) if *code == c)
    }
    /// Index just after the END of the structure headed at `h`.
    fn after_struct(&self, h: usize) -> usize {
        match self.partner.get(&h) {
            Some(&p) if self.is_code(h, 0xFC15) || self.is_code(h, 0xFC2F) => {
                if self.is_code(p, 0xFC0E) {
                    self.partner.get(&p).map(|e| e + 1).unwrap_or_else(|| self.seg_end(p))
                } else {
                    p + 1
                }
            }
            Some(&p) => p + 1,
            None => self.seg_end(h),
        }
    }
}

/// Variable / label / system-variable name: text, trimmed, upper case.
fn norm_name(w: &[u16]) -> String {
    let mut s = String::new();
    for &x in w {
        match word_char(x) {
            Some(c) => s.push(c),
            None => s.push_str(&format!("{{{x:04X}}}")),
        }
    }
    s.trim().to_uppercase()
}

fn display_text(w: &[u16]) -> String {
    w.iter().filter_map(|&x| word_char(x)).collect()
}

fn digits(n: i64) -> Vec<u16> {
    n.to_string().bytes().map(u16::from).collect()
}

// ============================================================ expressions

#[derive(Clone, Debug, PartialEq)]
enum Val {
    N(i64),
    S(Vec<u16>),
}

#[derive(Clone, Debug, PartialEq)]
enum Et {
    N(i64),
    S(Vec<u16>),
    Op(&'static str),
}

fn is_opch(w: u16) -> bool {
    matches!(w, 0x2B | 0x2D | 0x2A | 0x2F | 0x25 | 0x26 | 0x7C | 0x21 | 0x3D | 0x3C | 0x3E | 0x28 | 0x29)
}

fn lex(w: &[u16]) -> Vec<Et> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < w.len() {
        let c = w[i];
        if c == 0x20 {
            i += 1;
            continue;
        }
        if c == 0x22 {
            let j = w[i + 1..].iter().position(|&x| x == 0x22).map(|p| i + 1 + p).unwrap_or(w.len());
            out.push(Et::S(w[i + 1..j].to_vec()));
            i = j + 1;
            continue;
        }
        if is_opch(c) {
            let nx = w.get(i + 1).copied();
            let op = match (c, nx) {
                (0x21, Some(0x3D)) => "!=",
                (0x3C, Some(0x3D)) => "<=",
                (0x3E, Some(0x3D)) => ">=",
                (0x3C, Some(0x3E)) => "!=",
                (0x2B, _) => "+",
                (0x2D, _) => "-",
                (0x2A, _) => "*",
                (0x2F, _) => "/",
                (0x25, _) => "%",
                (0x26, _) => "&",
                (0x7C, _) => "|",
                (0x21, _) => "!",
                (0x3D, _) => "=",
                (0x3C, _) => "<",
                (0x3E, _) => ">",
                (0x28, _) => "(",
                _ => ")",
            };
            i += if op.len() == 2 { 2 } else { 1 };
            out.push(Et::Op(op));
            continue;
        }
        let j = (i..w.len()).find(|&k| is_opch(w[k]) || w[k] == 0x20 || w[k] == 0x22).unwrap_or(w.len());
        let run = &w[i..j];
        let num = if run.iter().all(|&x| (0x30..=0x39).contains(&x)) {
            run.iter().map(|&x| x as u8 as char).collect::<String>().parse::<i64>().ok()
        } else {
            None
        };
        out.push(match num {
            Some(n) => Et::N(n),
            None => Et::S(run.to_vec()),
        });
        i = j;
    }
    out
}

struct Parser {
    t: Vec<Et>,
    i: usize,
}

fn val_words(v: &Val) -> Vec<u16> {
    match v {
        Val::N(n) => digits(*n),
        Val::S(s) => s.clone(),
    }
}

fn num(v: &Val) -> Option<i64> {
    match v {
        Val::N(n) => Some(*n),
        Val::S(_) => None,
    }
}

fn tf(b: bool) -> Val {
    Val::N(if b { -1 } else { 0 })
}

impl Parser {
    fn peek(&self) -> Option<&'static str> {
        match self.t.get(self.i) {
            Some(Et::Op(o)) => Some(o),
            _ => None,
        }
    }
    fn take(&mut self, ops: &[&str]) -> Option<&'static str> {
        let o = self.peek()?;
        if ops.contains(&o) {
            self.i += 1;
            Some(o)
        } else {
            None
        }
    }
    fn or(&mut self) -> Option<Val> {
        let mut a = self.and()?;
        while self.take(&["|"]).is_some() {
            let b = self.and()?;
            a = Val::N(num(&a)? | num(&b)?);
        }
        Some(a)
    }
    fn and(&mut self) -> Option<Val> {
        let mut a = self.cmp()?;
        while self.take(&["&"]).is_some() {
            let b = self.cmp()?;
            a = Val::N(num(&a)? & num(&b)?);
        }
        Some(a)
    }
    fn cmp(&mut self) -> Option<Val> {
        let mut a = self.add()?;
        while let Some(op) = self.take(&["=", "!=", "<", ">", "<=", ">="]) {
            let b = self.add()?;
            let ord = match (&a, &b) {
                (Val::N(x), Val::N(y)) => x.cmp(y),
                _ => val_words(&a).cmp(&val_words(&b)),
            };
            use std::cmp::Ordering::*;
            a = tf(match op {
                "=" => ord == Equal,
                "!=" => ord != Equal,
                "<" => ord == Less,
                ">" => ord == Greater,
                "<=" => ord != Greater,
                _ => ord != Less,
            });
        }
        Some(a)
    }
    fn add(&mut self) -> Option<Val> {
        let mut a = self.mul()?;
        while let Some(op) = self.take(&["+", "-"]) {
            let b = self.mul()?;
            let (x, y) = (num(&a)?, num(&b)?);
            a = Val::N(if op == "+" { x.wrapping_add(y) } else { x.wrapping_sub(y) });
        }
        Some(a)
    }
    fn mul(&mut self) -> Option<Val> {
        let mut a = self.unary()?;
        while let Some(op) = self.take(&["*", "/", "%"]) {
            let b = self.unary()?;
            let (x, y) = (num(&a)?, num(&b)?);
            a = Val::N(match op {
                "*" => x.wrapping_mul(y),
                "/" => x.checked_div(y)?,
                _ => x.checked_rem(y)?,
            });
        }
        Some(a)
    }
    fn unary(&mut self) -> Option<Val> {
        match self.take(&["-", "!", "+"]) {
            Some("-") => Some(Val::N(num(&self.unary()?)?.wrapping_neg())),
            Some("!") => Some(Val::N(!num(&self.unary()?)?)),
            Some(_) => Some(Val::N(num(&self.unary()?)?)),
            None => self.primary(),
        }
    }
    fn primary(&mut self) -> Option<Val> {
        match self.t.get(self.i).cloned() {
            Some(Et::N(n)) => {
                self.i += 1;
                Some(Val::N(n))
            }
            Some(Et::S(s)) => {
                self.i += 1;
                Some(Val::S(s))
            }
            Some(Et::Op("(")) => {
                self.i += 1;
                let v = self.or()?;
                self.take(&[")"]);
                Some(v)
            }
            _ => Some(Val::S(Vec::new())),
        }
    }
}

/// Evaluate a WP 5.1 macro expression (integer arithmetic, `&|!` bitwise,
/// comparisons yield -1/0, quoted strings compare as text).
fn eval(w: &[u16]) -> Option<Val> {
    let mut p = Parser { t: lex(w), i: 0 };
    let v = p.or()?;
    (p.i == p.t.len()).then_some(v)
}

fn truthy(w: &[u16]) -> bool {
    matches!(eval(w), Some(Val::N(n)) if n != 0)
}

/// ASSIGN semantics: an expression with an operator that evaluates to a
/// number is stored as its value, anything else as the literal text (so
/// `{ASSIGN}8~{VAR 8}1~` appends a digit, as CALC.WPM relies on).
fn assign_value(w: Vec<u16>) -> Vec<u16> {
    if !lex(&w).iter().any(|t| matches!(t, Et::Op(_))) {
        return w;
    }
    match eval(&w) {
        Some(Val::N(n)) => digits(n),
        _ => w,
    }
}

// ================================================================ the VM

#[derive(Clone, Copy, PartialEq, Debug)]
enum Cond {
    Cancel,
    Error,
    NotFound,
}

enum ForState {
    Count { var: String, stop: i64, step: i64 },
    Each { var: String, rest: VecDeque<Vec<u16>> },
}

enum Wait {
    No,
    Until(Instant),
    Char(String),
    Text { var: String, buf: Vec<u16>, origin: (usize, usize) },
    Pause,
    PauseKey(u16),
    Step,
}

struct Frame {
    m: Rc<Macro>,
    pc: usize,
    calls: Vec<usize>,
    on_cancel: Option<Option<usize>>,
    on_error: Option<Option<usize>>,
    on_not_found: Option<Option<usize>>,
    /// Where to continue after a condition handler's action runs out.
    resume: Option<usize>,
    fors: HashMap<usize, ForState>,
    /// Words still to be typed (contents of {VARIABLE}, {VAR n} …).
    pending: VecDeque<u16>,
    wait: Wait,
    /// Name of the keyboard-macro key that started this frame.
    key_name: Option<String>,
    stepped: bool,
}

impl Frame {
    fn new(m: Rc<Macro>, key_name: Option<String>) -> Frame {
        Frame {
            m,
            pc: 0,
            calls: Vec::new(),
            on_cancel: None,
            on_error: None,
            on_not_found: None,
            resume: None,
            fors: HashMap::new(),
            pending: VecDeque::new(),
            wait: Wait::No,
            key_name,
            stepped: false,
        }
    }
}

const MAX_NEST: usize = 64;
const MAX_CALLS: usize = 1000;
const A_BOLD: u8 = 1;
const A_UND: u8 = 2;
const A_REV: u8 = 4;
const NOT_FOUND_MSG: &str = "* Not found *";

/// What macro prompts ({PROMPT}, {CHAR}, {TEXT}) drew on the screen.
#[derive(Default)]
struct Canvas {
    cols: usize,
    rows: usize,
    cells: Vec<Option<(char, u8)>>,
    full: bool,
    cursor: Option<(usize, usize)>,
    generation: u64,
}

impl Canvas {
    fn active(&self) -> bool {
        self.full || self.cells.iter().any(|c| c.is_some())
    }
    fn reset(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![None; cols * rows];
        self.full = false;
        self.cursor = None;
    }
    fn set(&mut self, col: usize, row: usize, v: Option<(char, u8)>) {
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col] = v;
        }
    }
    fn clear_row(&mut self, row: usize) {
        for c in 0..self.cols {
            self.set(c, row, None);
        }
    }
}

/// The screen layer that shows the canvas on top of WP.  It is taken off
/// the screen stack whenever the macro feeds keys to WP.
struct Overlay {
    canvas: Rc<RefCell<Canvas>>,
    generation: u64,
}

impl Screen for Overlay {
    fn kind(&self) -> crate::ui::ScreenKind {
        crate::ui::ScreenKind::Overlay
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        // Normally never reached; pass the key on to what is below.
        if let Some(mut top) = wp.screens.pop() {
            let base = wp.screens.len();
            let out = top.input(wp, i);
            let base = base.min(wp.screens.len());
            match out {
                Outcome::Keep => wp.screens.insert(base, top),
                Outcome::Close => {}
                Outcome::CloseAll => {
                    wp.screens.clear();
                    return Outcome::Close;
                }
                Outcome::Replace(n) => wp.screens.insert(base, n),
                Outcome::Push(n) => {
                    wp.screens.insert(base, top);
                    wp.screens.insert(base + 1, n);
                }
            }
        } else {
            wp.message = None;
            wp.edit_input(i.clone());
        }
        if self.canvas.borrow().generation == self.generation { Outcome::Keep } else { Outcome::Close }
    }

    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let c = self.canvas.borrow();
        if c.generation != self.generation {
            return;
        }
        if c.full {
            g.clear(wp.theme.normal);
        }
        for row in 0..c.rows.min(g.rows) {
            for col in 0..c.cols.min(g.cols) {
                if let Some((ch, a)) = c.cells[row * c.cols + col] {
                    let mut st = if a & A_BOLD != 0 { wp.theme.bold } else { wp.theme.normal };
                    if a & A_UND != 0 {
                        st = st.flags(F_UNDER);
                    }
                    if a & A_REV != 0 {
                        st = st.reverse();
                    }
                    g.put(col, row, ch, st);
                }
            }
        }
        if let Some((x, y)) = c.cursor {
            if x < g.cols && y < g.rows {
                g.cursor = Some((x, y));
            }
        }
    }

    fn full_screen(&self) -> bool {
        let c = self.canvas.borrow();
        c.generation == self.generation && c.full
    }
}

/// The macro interpreter.  Lives in `Wp::macro_vm`; keeps the variables
/// between macro runs.
#[derive(Default)]
pub struct MacroVm {
    frames: Vec<Frame>,
    vars: HashMap<String, Vec<u16>>,
    typeahead: VecDeque<Input>,
    cancel_off: bool,
    chain: Option<String>,
    step_mode: bool,
    canvas: Rc<RefCell<Canvas>>,
    /// Screen-stack length right after the overlay was pushed.
    shown: Option<usize>,
    /// Macros started by keystrokes ({Macro}name{Enter}) while another one
    /// runs: WP chains them, i.e. they start when the current macro ends.
    queued: VecDeque<Frame>,
}

impl MacroVm {
    /// Is a macro running (or waiting for the user)?
    pub fn running(&self) -> bool {
        !self.frames.is_empty()
    }

    /// Current value of a variable (`"1"` = {VAR 1}).
    pub fn var(&self, name: &str) -> Vec<u16> {
        self.vars.get(&name.trim().to_uppercase()).cloned().unwrap_or_default()
    }

    pub fn set_var(&mut self, name: &str, v: Vec<u16>) {
        let k = name.trim().to_uppercase();
        if v.is_empty() {
            self.vars.remove(&k);
        } else {
            self.vars.insert(k, v);
        }
    }

    fn hide(&mut self, wp: &mut Wp) {
        if let Some(n) = self.shown.take() {
            if wp.screens.len() == n {
                wp.screens.pop();
            } else {
                // Something got pushed above it: orphan that instance.
                self.canvas.borrow_mut().generation += 1;
            }
        }
    }

    fn sync_overlay(&mut self, wp: &mut Wp) {
        let want = !self.frames.is_empty() && self.canvas.borrow().active();
        if want && self.shown.is_none() {
            let generation = self.canvas.borrow().generation;
            wp.push(Box::new(Overlay { canvas: self.canvas.clone(), generation }));
            self.shown = Some(wp.screens.len());
        } else if !want {
            self.hide(wp);
        }
    }

    fn clear_status(&mut self) {
        let mut c = self.canvas.borrow_mut();
        let r = c.rows.saturating_sub(1);
        c.clear_row(r);
        c.cursor = None;
    }

    fn push_frame(&mut self, wp: &mut Wp, name: &str, words: Vec<u16>, key: Option<String>) {
        if words.iter().all(|&w| is_fmt(w)) {
            return;
        }
        if self.frames.len() >= MAX_NEST {
            wp.flash("ERROR: Macros nested too deeply");
            return;
        }
        self.frames.push(Frame::new(Rc::new(compile(name, words)), key));
    }

    fn push_file(&mut self, wp: &mut Wp, name: &str) -> bool {
        let Some(p) = crate::macros::find_macro(wp, name) else { return false };
        match load_wpm(&p) {
            Ok(f) => {
                let n = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.push_frame(wp, &n, f.words, None);
                true
            }
            Err(_) => false,
        }
    }

    /// Send one keystroke to WP and watch for "not found" / errors.
    fn feed(&mut self, wp: &mut Wp, i: Input) {
        self.hide(wp);
        let cond_msg = |m: &Option<String>| match m.as_deref() {
            Some(NOT_FOUND_MSG) => Some(Cond::NotFound),
            Some(s) if s.starts_with("ERROR") => Some(Cond::Error),
            _ => None,
        };
        if cond_msg(&wp.message).is_some() {
            wp.message = None;
        }
        wp.input(i);
        if let Some(c) = cond_msg(&wp.message) {
            self.raise(wp, c, None);
        }
    }

    /// Raise a condition: run the innermost handler, else end the macros.
    fn raise(&mut self, wp: &mut Wp, c: Cond, msg: Option<&str>) {
        while let Some(f) = self.frames.last_mut() {
            let h = match c {
                Cond::Cancel => f.on_cancel,
                Cond::Error => f.on_error,
                Cond::NotFound => f.on_not_found,
            };
            match h {
                Some(None) => return, // {ON …}~ with no action: ignore
                Some(Some(start)) => {
                    f.resume = Some(f.pc);
                    f.pc = start;
                    f.wait = Wait::No;
                    f.pending.clear();
                    self.clear_status();
                    return;
                }
                None => {
                    self.frames.pop();
                }
            }
        }
        self.chain = None;
        self.queued.clear();
        if let Some(m) = msg {
            wp.flash(m);
        }
    }

    fn do_return(&mut self) {
        if let Some(f) = self.frames.last_mut() {
            f.resume = None;
            if let Some(r) = f.calls.pop() {
                f.pc = r;
                return;
            }
        }
        self.frames.pop();
    }

    fn goto(&mut self, wp: &mut Wp, label: &str, call: bool) {
        let Some(f) = self.frames.last_mut() else { return };
        let Some(&t) = f.m.labels.get(label) else {
            let msg = format!("ERROR: Label not found -- {label}");
            self.raise(wp, Cond::Error, Some(&msg));
            return;
        };
        if call {
            if f.calls.len() >= MAX_CALLS {
                self.raise(wp, Cond::Error, Some("ERROR: Too many nested CALLs"));
                return;
            }
            let ret = f.resume.take().unwrap_or(f.pc);
            f.calls.push(ret);
        } else {
            f.resume = None;
        }
        f.pc = t;
    }

    // ----------------------------------------------------------- expansion

    /// Expand an argument: strip formatting/comments, substitute variables
    /// and functions ({VARIABLE} {VAR n} {SYSTEM} {STATE} {MID} {LEN} {KTON}
    /// {NTOK} {~}).
    fn expand(&self, wp: &Wp, w: &[u16], r: Range<usize>) -> Vec<u16> {
        let mut out = Vec::new();
        let (mut i, end) = (r.start, r.end.min(w.len()));
        while i < end {
            let x = w[i];
            if is_fmt(x) {
                i += 1;
                continue;
            }
            if nargs(x) != 0 {
                let (code, args, nx) = parse_cmd(w, i, end);
                i = nx;
                let a = |k: usize| self.expand(wp, w, args.get(k).cloned().unwrap_or(0..0));
                match code {
                    0xFC34 => out.extend(self.var(&norm_name(&a(0)))),
                    0xFC35 => out.extend(self.system(wp, &norm_name(&a(0)))),
                    0xFC36 => {
                        let v = self.var(&norm_name(&a(0)));
                        let off = num_or(&a(1), 0).max(0) as usize;
                        let cnt = num_or(&a(2), 0).max(0) as usize;
                        if off < v.len() {
                            out.extend(&v[off..(off + cnt).min(v.len())]);
                        }
                    }
                    0xFC37 => {
                        if let Some(n) = eval(&a(0)).and_then(|v| num(&v)) {
                            out.push(n as u16);
                        }
                    }
                    0xFC38 => out.extend(digits(a(0).first().copied().unwrap_or(0) as i64)),
                    0xFC39 => out.extend(digits(self.var(&norm_name(&a(0))).len() as i64)),
                    _ => {} // comments, shell variables, commands inside actions
                }
                continue;
            }
            match x {
                0xFC29 => out.extend(digits(self.state(wp))),
                0xFC3A => out.push(0x7E),
                0xFF00..=0xFF09 => out.extend(self.var(&(x & 0xF).to_string())),
                0xFC2C => out.push(x),
                0xFC00..=0xFC3F => {}
                _ => out.push(x),
            }
            i += 1;
        }
        out
    }

    /// {STATE}
    fn state(&self, wp: &Wp) -> i64 {
        let e = wp.ed_ref();
        let mut s = (wp.cur as i64 + 1) | 32;
        match top_kind(wp) {
            None => s |= 4,
            Some(ScreenKind::SubEdit) => s |= 4 | 8,
            Some(ScreenKind::Question) => s |= 1024,
            Some(ScreenKind::List) => s |= 2048,
            _ => {}
        }
        if wp.screens.iter().any(|x| x.kind() == ScreenKind::SubEdit) {
            s |= 8;
        }
        if wp.recording.is_some() {
            s |= 16;
        }
        if e.block_anchor.is_some() {
            s |= 128;
        }
        if e.typeover {
            s |= 256;
        }
        if e.reveal {
            s |= 512;
        }
        s
    }

    /// {SYSTEM}name~ (number 1–29 or name).
    fn system(&self, wp: &Wp, key: &str) -> Vec<u16> {
        const NAMES: [&str; 29] = [
            "ATTRIB", "CELL", "COLUMN", "DOCUMENT", "ENDNOTE", "EQUATION", "FIGURE", "FOOTNOTE", "LEFT", "LINE", "LIST",
            "NAME", "MENU", "PAGE", "PATH", "POS", "PRINT", "RIGHT", "TABLEBOX", "TEXTBOX", "USERBOX", "ROW", "CELLATTR",
            "CELLSTATE", "SHELLVER", "KEYSTATE", "ROWSTATE", "VERSION", "ENTRY",
        ];
        let idx = key.parse::<usize>().ok().or_else(|| NAMES.iter().position(|n| *n == key).map(|p| p + 1));
        let e = wp.ed_ref();
        let line = e.layout.lines.get(e.layout.line_of(e.cursor));
        let el_val = |el: Option<&El>| -> Vec<u16> {
            match el {
                Some(El::Ch(c)) => char_word(*c).map(|w| vec![w]).unwrap_or_else(|| digits(0)),
                Some(El::Code(c)) => digits(code_number(c)),
                None => digits(0),
            }
        };
        match idx {
            Some(1) => {
                let a = e.doc.attrs_at(e.cursor);
                digits(Attr::ALL.iter().filter(|x| a.has(**x)).map(|x| 1i64 << attr_index(*x)).sum())
            }
            Some(9) => el_val(e.cursor.checked_sub(1).and_then(|k| e.doc.els.get(k))),
            // at the end of the document WP reports its end-of-file mark, WP char 6,13
            Some(18) => match e.doc.els.get(e.cursor) {
                None => vec![0x060D],
                el => el_val(el),
            },
            Some(10) => digits(line.map(|l| l.y as i64).unwrap_or(0)),
            Some(14) => digits(line.map(|l| l.page as i64).unwrap_or(1)),
            Some(16) => digits(line.map(|l| l.caret_of(e.cursor).1 as i64).unwrap_or(0)),
            Some(12) => {
                let n = e.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
                n.unwrap_or_default().chars().filter_map(char_word).collect()
            }
            Some(15) => {
                let d = e.path.as_ref().and_then(|p| p.parent()).unwrap_or(&wp.cwd);
                let mut s = d.display().to_string();
                if !s.ends_with('\\') && !s.ends_with('/') {
                    s.push('\\');
                }
                s.chars().filter_map(char_word).collect()
            }
            Some(13) => digits(match top_kind(wp) {
                None | Some(ScreenKind::SubEdit) => 65535,
                Some(ScreenKind::Menu(n)) => n as i64,
                _ => 0,
            }),
            Some(28) => "5.1".bytes().map(u16::from).collect(),
            _ => digits(0),
        }
    }

    // ------------------------------------------------------------ running

    /// Run one unit.  Returns false when idle or waiting.
    fn run_one(&mut self, wp: &mut Wp) -> bool {
        if self.frames.is_empty() {
            return self.idle(wp);
        }
        {
            let f = self.frames.last_mut().unwrap();
            match &f.wait {
                Wait::No => {}
                Wait::Until(t) => {
                    if Instant::now() >= *t {
                        f.wait = Wait::No;
                    } else {
                        return false;
                    }
                }
                _ => {
                    if let Some(i) = self.typeahead.pop_front() {
                        self.on_key(wp, i);
                        return true;
                    }
                    return false;
                }
            }
        }
        if let Some(w) = self.frames.last_mut().unwrap().pending.pop_front() {
            self.type_word(wp, w);
            return true;
        }
        let (m, pc) = {
            let f = self.frames.last().unwrap();
            (f.m.clone(), f.pc)
        };
        if self.step_mode {
            let f = self.frames.last_mut().unwrap();
            if !f.stepped {
                f.wait = Wait::Step;
                wp.flash(&format!("Macro step: {}", tok_text(&m, pc)));
                return false;
            }
            f.stepped = false;
        }
        let Some(tok) = m.toks.get(pc) else {
            self.frames.pop();
            return true;
        };
        self.frames.last_mut().unwrap().pc = pc + 1;
        match tok {
            Tok::W(w) => self.type_word(wp, *w),
            Tok::End(true) => {
                let f = self.frames.last_mut().unwrap();
                match f.resume.take() {
                    Some(r) => f.pc = r,
                    None => {
                        self.frames.pop();
                    }
                }
            }
            Tok::End(false) => {
                self.frames.pop();
            }
            Tok::C { code, args, span } => self.exec(wp, &m, pc, *code, args, span.clone()),
        }
        true
    }

    /// No macro frames left: start a chained macro, flush type-ahead, tidy up.
    fn idle(&mut self, wp: &mut Wp) -> bool {
        if let Some(f) = self.queued.pop_front() {
            self.frames.push(f);
            return true;
        }
        if let Some(name) = self.chain.take() {
            if !self.push_file(wp, &name) {
                wp.flash(&format!("ERROR: File not found -- {}.WPM", name.trim().to_uppercase()));
            }
            return true;
        }
        if let Some(i) = self.typeahead.pop_front() {
            self.feed(wp, i);
            return true;
        }
        self.cancel_off = false;
        self.step_mode = false;
        let mut c = self.canvas.borrow_mut();
        if c.active() {
            let (cols, rows) = (c.cols, c.rows);
            c.reset(cols, rows);
        }
        false
    }

    /// A real key while a macro is active.
    fn on_key(&mut self, wp: &mut Wp, i: Input) {
        self.hide(wp);
        let Some(f) = self.frames.last_mut() else {
            self.feed(wp, i);
            return;
        };
        let waiting = !matches!(f.wait, Wait::No | Wait::Until(_));
        if i == Input::Func(Func::Cancel) && !self.cancel_off {
            self.clear_status();
            self.raise(wp, Cond::Cancel, None);
            return;
        }
        if !waiting {
            self.typeahead.push_back(i);
            return;
        }
        let wait = std::mem::replace(&mut f.wait, Wait::No);
        match wait {
            Wait::Char(var) => {
                if let Some(w) = input_word(&i) {
                    self.vars.insert(var, vec![w]);
                }
                self.clear_status();
            }
            Wait::Text { var, mut buf, origin } => match i {
                Input::Func(Func::Enter) => {
                    self.set_var(&var, buf);
                    self.clear_status();
                }
                other => {
                    match other {
                        Input::Func(Func::Backspace) => {
                            buf.pop();
                        }
                        Input::Char(c) => {
                            if let Some(w) = char_word(c) {
                                buf.push(w);
                            }
                        }
                        _ => {}
                    }
                    self.draw_text_input(&buf, origin);
                    self.frames.last_mut().unwrap().wait = Wait::Text { var, buf, origin };
                }
            },
            Wait::Pause => {
                if i != Input::Func(Func::Enter) {
                    self.frames.last_mut().unwrap().wait = Wait::Pause;
                    self.feed(wp, i);
                } else {
                    self.clear_status();
                }
            }
            Wait::PauseKey(k) => {
                if input_word(&i) != Some(k) {
                    self.frames.last_mut().unwrap().wait = Wait::PauseKey(k);
                    self.feed(wp, i);
                }
            }
            Wait::Step => {
                self.frames.last_mut().unwrap().stepped = true;
                wp.message = None;
            }
            Wait::No | Wait::Until(_) => {}
        }
    }

    /// Type one macro word (keystroke).
    fn type_word(&mut self, wp: &mut Wp, w: u16) {
        use Func::*;
        match w {
            0x0020..=0x007E => self.feed(wp, Input::Char(w as u8 as char)),
            0x0100..=0x0CFF => {
                if let Some(c) = word_char(w) {
                    self.feed(wp, Input::Char(c));
                }
            }
            0x8056 => {
                for f in [Home, Home, Left] {
                    self.feed(wp, Input::Func(f));
                }
            }
            0x8061 => self.feed(wp, Input::Char('-')),
            0x806C..=0x806E => {
                let k = match w {
                    0x806C => 'a',
                    0x806D => 'm',
                    _ => 'c',
                };
                self.feed(wp, Input::Func(Move));
                self.feed(wp, Input::Char('b'));
                self.feed(wp, Input::Char(k));
            }
            0x8000..=0x80FF => self.feed(wp, Input::Func(key_func(w))),
            0xFC2C => {
                self.hide(wp);
                wp.push(macro_commands_menu());
            }
            0xFC3A => self.feed(wp, Input::Char('~')),
            0xFD00..=0xFDFF => {
                let n = (w & 0xFF) as u8;
                let body = wp.keyboard.as_ref().and_then(|k| k.macros.get(&n).cloned());
                if let Some(b) = body {
                    self.push_frame(wp, &format!("KEY MACRO {n}"), b, None);
                }
            }
            0xFE00..=0xFEFF => {
                let c = (w & 0xFF) as u8 as char;
                self.push_file(wp, &format!("ALT{}", c.to_ascii_uppercase()));
            }
            0xFF00..=0xFF09 => {
                let v = self.var(&(w & 0xF).to_string());
                if let Some(f) = self.frames.last_mut() {
                    for x in v.into_iter().rev() {
                        f.pending.push_front(x);
                    }
                }
            }
            _ => {}
        }
    }

    fn draw_text_input(&mut self, buf: &[u16], origin: (usize, usize)) {
        let mut c = self.canvas.borrow_mut();
        let (x0, y) = origin;
        for x in x0..c.cols {
            c.set(x, y, Some((' ', 0)));
        }
        let mut x = x0;
        for &w in buf {
            if let Some(ch) = word_char(w) {
                c.set(x, y, Some((ch, 0)));
                x += 1;
            }
        }
        c.cursor = Some((x.min(c.cols.saturating_sub(1)), y));
    }

    /// Draw a {PROMPT}/{CHAR}/{TEXT} message with WP's display codes
    /// (^P position, ^L clear, ^] ^\ bold, ^T ^U underline, ^R ^S reverse,
    /// ^N/^O attribute on/off, ^Q all off, ^H home, ^J newline, ^K clear
    /// to end of line, ^M column 0, ^W ^X ^Y ^Z cursor moves).  Returns the
    /// cursor position after the message.
    fn render(&mut self, wp: &Wp, msg: &[u16]) -> (usize, usize) {
        let mut c = self.canvas.borrow_mut();
        if c.cols != wp.cols || c.rows != wp.rows {
            c.reset(wp.cols, wp.rows);
        }
        let (cols, rows) = (c.cols.max(1), c.rows.max(1));
        let (mut col, mut row) = (0usize, rows - 1);
        // A message without positioning codes replaces the status line.
        if !msg.iter().any(|&w| matches!(w, 0x8008 | 0x800C | 0x8010)) {
            for x in 0..cols {
                c.set(x, row, Some((' ', 0)));
            }
        }
        let mut attr = 0u8;
        let attr_bit = |a: u16| match coord(a) {
            12 => A_BOLD,
            11 | 14 => A_UND,
            _ => A_REV,
        };
        let mut i = 0;
        while i < msg.len() {
            let w = msg[i];
            i += 1;
            match w {
                0x8008 => (col, row) = (0, 0),
                0x800A => (col, row) = (0, (row + 1).min(rows - 1)),
                0x800B => {
                    for x in col..cols {
                        c.set(x, row, Some((' ', attr)));
                    }
                }
                0x800C => {
                    c.full = true;
                    c.cells.iter_mut().for_each(|x| *x = Some((' ', 0)));
                    (col, row) = (0, 0);
                }
                0x800D => col = 0,
                0x800E | 0x800F => {
                    if let Some(&a) = msg.get(i) {
                        i += 1;
                        if w == 0x800E {
                            attr |= attr_bit(a);
                        } else {
                            attr &= !attr_bit(a);
                        }
                    }
                }
                0x8010 => {
                    if let Some(&x) = msg.get(i) {
                        col = coord(x).min(cols - 1);
                        row = msg.get(i + 1).map(|&r| coord(r).min(rows - 1)).unwrap_or(row);
                        i += 2;
                    }
                }
                0x8011 => attr = 0,
                0x8012 => attr |= A_REV,
                0x8013 => attr &= !A_REV,
                0x8014 => attr |= A_UND,
                0x8015 => attr &= !A_UND,
                0x801C => attr &= !A_BOLD,
                0x801D => attr |= A_BOLD,
                0x8017 => row = row.saturating_sub(1),
                0x8018 => col = (col + 1).min(cols - 1),
                0x8019 => col = col.saturating_sub(1),
                0x801A => row = (row + 1).min(rows - 1),
                _ => {
                    if let Some(ch) = word_char(w) {
                        c.set(col, row, Some((ch, attr)));
                        col += 1;
                        if col >= cols {
                            col = 0;
                            row = (row + 1).min(rows - 1);
                        }
                    }
                }
            }
        }
        c.cursor = Some((col, row));
        (col, row)
    }

    fn set_wait(&mut self, w: Wait) {
        if let Some(f) = self.frames.last_mut() {
            f.wait = w;
        }
    }

    fn exec(&mut self, wp: &mut Wp, m: &Rc<Macro>, pc: usize, code: u16, args: &[Range<usize>], span: Range<usize>) {
        let a = |s: &Self, wp: &Wp, k: usize| s.expand(wp, &m.words, args.get(k).cloned().unwrap_or(0..0));
        let set_pc = |s: &mut Self, p: usize| {
            if let Some(f) = s.frames.last_mut() {
                f.pc = p;
            }
        };
        match code {
            // {ASSIGN}var~expr~
            0xFC01 => {
                let name = norm_name(&a(self, wp, 0));
                let v = a(self, wp, 1);
                let v = if v.is_empty() { v } else { assign_value(v) };
                self.set_var(&name, v);
            }
            0xFC02 => {} // {BELL}: no sound device
            // {BREAK}
            0xFC03 => match m.enclosing.get(&pc) {
                Some(&h) => {
                    if let Some(f) = self.frames.last_mut() {
                        f.fors.remove(&h);
                    }
                    set_pc(self, m.after_struct(h));
                }
                None => {
                    self.frames.pop();
                }
            },
            0xFC04 => {
                let l = norm_name(&a(self, wp, 0));
                self.goto(wp, &l, true);
            }
            0xFC05 => self.cancel_off = true,
            0xFC06 => self.cancel_off = false,
            // {CASE} / {CASE CALL}
            0xFC07 | 0xFC08 => {
                let key = a(self, wp, 0);
                let keyv = eval(&key).filter(|v| matches!(v, Val::N(_)));
                for pair in args[1.min(args.len())..].chunks(2) {
                    let lit = literal(&m.words, pair[0].clone());
                    let hit = if lit == [0xFC3D] {
                        true
                    } else {
                        let cv = self.expand(wp, &m.words, pair[0].clone());
                        match (&keyv, eval(&cv)) {
                            (Some(Val::N(x)), Some(Val::N(y))) => *x == y,
                            _ => cv == key,
                        }
                    };
                    if hit {
                        let label = pair.get(1).map(|r| norm_name(&self.expand(wp, &m.words, r.clone())));
                        if let Some(l) = label {
                            self.goto(wp, &l, code == 0xFC08);
                        }
                        break;
                    }
                }
            }
            0xFC09 => {
                let n = norm_name(&a(self, wp, 0));
                if !n.is_empty() {
                    self.chain = Some(n);
                }
            }
            // {CHAR}var~message~
            0xFC0A => {
                let var = norm_name(&a(self, wp, 0));
                let msg = a(self, wp, 1);
                self.render(wp, &msg);
                self.set_wait(Wait::Char(var));
            }
            0xFC0C | 0xFC0D | 0xFC30 | 0xFC31 => {} // DISPLAY / MENU on/off
            // {ELSE}: end of the true branch
            0xFC0E => {
                let p = m.partner.get(&pc).map(|e| e + 1).unwrap_or_else(|| m.seg_end(pc));
                set_pc(self, p);
            }
            // {END FOR}
            0xFC0F => {
                let Some(&h) = m.partner.get(&pc) else { return };
                let Some(f) = self.frames.last_mut() else { return };
                let mut again = false;
                let mut assign: Option<(String, Vec<u16>)> = None;
                match f.fors.get_mut(&h) {
                    Some(ForState::Count { var, stop, step }) => {
                        let cur = self.vars.get(var.as_str()).and_then(|v| eval(v)).and_then(|v| num(&v)).unwrap_or(0);
                        let nv = cur.wrapping_add(*step);
                        again = if *step >= 0 { nv <= *stop } else { nv >= *stop };
                        assign = Some((var.clone(), digits(nv)));
                    }
                    Some(ForState::Each { var, rest }) => {
                        if let Some(v) = rest.pop_front() {
                            again = true;
                            assign = Some((var.clone(), v));
                        }
                    }
                    None => {}
                }
                if again {
                    f.pc = h + 1;
                } else {
                    f.fors.remove(&h);
                }
                if let Some((k, v)) = assign {
                    self.set_var(&k, v);
                }
            }
            0xFC10 => {} // {END IF}
            // {END WHILE}: back to the WHILE test
            0xFC11 => {
                if let Some(&h) = m.partner.get(&pc) {
                    set_pc(self, h);
                }
            }
            // {FOR}var~start~stop~step~
            0xFC12 => {
                let var = norm_name(&a(self, wp, 0));
                let start = eval(&a(self, wp, 1)).and_then(|v| num(&v));
                let stop = eval(&a(self, wp, 2)).and_then(|v| num(&v));
                let step = eval(&a(self, wp, 3)).and_then(|v| num(&v)).unwrap_or(1);
                let (Some(start), Some(stop)) = (start, stop) else {
                    self.raise(wp, Cond::Error, Some("ERROR: Invalid FOR expression"));
                    return;
                };
                self.set_var(&var, digits(start));
                let ok = if step >= 0 { start <= stop } else { start >= stop };
                if ok {
                    if let Some(f) = self.frames.last_mut() {
                        f.fors.insert(pc, ForState::Count { var, stop, step });
                    }
                } else {
                    set_pc(self, m.after_struct(pc));
                }
            }
            // {FOR EACH}var~e1~e2~…~~
            0xFC13 => {
                let var = norm_name(&a(self, wp, 0));
                let mut items: VecDeque<Vec<u16>> = (1..args.len()).map(|k| assign_value(a(self, wp, k))).collect();
                match items.pop_front() {
                    Some(first) => {
                        self.set_var(&var, first);
                        if let Some(f) = self.frames.last_mut() {
                            f.fors.insert(pc, ForState::Each { var, rest: items });
                        }
                    }
                    None => set_pc(self, m.after_struct(pc)),
                }
            }
            0xFC14 => {
                let l = norm_name(&a(self, wp, 0));
                self.goto(wp, &l, false);
            }
            // {IF}expr~ / {IF EXISTS}var~
            0xFC15 | 0xFC2F => {
                let v = a(self, wp, 0);
                let t = if code == 0xFC15 { truthy(&v) } else { self.vars.contains_key(&norm_name(&v)) };
                if !t {
                    let p = m.partner.get(&pc).map(|e| e + 1).unwrap_or_else(|| m.seg_end(pc));
                    set_pc(self, p);
                }
            }
            0xFC16 => {} // {LABEL}
            // {LOOK}var~
            0xFC17 => {
                let var = norm_name(&a(self, wp, 0));
                match self.typeahead.pop_front() {
                    Some(i) => {
                        if i == Input::Func(Func::Cancel) && !self.cancel_off {
                            self.raise(wp, Cond::Cancel, None);
                            return;
                        }
                        let w = input_word(&i).map(|w| vec![w]).unwrap_or_default();
                        self.set_var(&var, w);
                    }
                    None => self.set_var(&var, Vec::new()),
                }
            }
            // {NEST}macro~
            0xFC18 => {
                let n = display_text(&a(self, wp, 0));
                if !self.push_file(wp, &n) {
                    let msg = format!("ERROR: File not found -- {}.WPM", n.trim().to_uppercase());
                    self.raise(wp, Cond::Error, Some(&msg));
                }
            }
            // {NEXT}
            0xFC19 => {
                if let Some(&h) = m.enclosing.get(&pc) {
                    if let Some(&e) = m.partner.get(&h) {
                        set_pc(self, e);
                    }
                }
            }
            0xFC1A | 0xFC3E | 0xFC3F => {} // Shell (WP Office) only
            0xFC1B..=0xFC1D => {
                let h = m.actions.get(&pc).copied().flatten();
                if let Some(f) = self.frames.last_mut() {
                    match code {
                        0xFC1B => f.on_cancel = Some(h),
                        0xFC1C => f.on_error = Some(h),
                        _ => f.on_not_found = Some(h),
                    }
                }
            }
            0xFC1E => self.set_wait(Wait::Pause),
            // {PROMPT}message~
            0xFC1F => {
                let msg = a(self, wp, 0);
                let positional = msg.iter().any(|&w| matches!(w, 0x8008 | 0x800A | 0x800C | 0x8010 | 0x8017..=0x801A));
                if positional {
                    self.render(wp, &msg);
                    self.canvas.borrow_mut().cursor = None;
                } else {
                    wp.flash(&display_text(&msg));
                }
            }
            0xFC20 => {
                self.frames.clear();
                self.queued.clear();
                self.chain = None;
            }
            0xFC21 => {
                if let Some(mut top) = self.frames.pop() {
                    top.calls.clear();
                    self.frames.clear();
                    self.frames.push(top);
                }
            }
            0xFC22 => self.do_return(),
            0xFC23..=0xFC25 => {
                self.do_return();
                let c = match code {
                    0xFC23 => Cond::Cancel,
                    0xFC24 => Cond::Error,
                    _ => Cond::NotFound,
                };
                self.raise(wp, c, None);
            }
            0xFC26 => {} // {SPEED}: macros run at full speed
            0xFC27 => self.step_mode = true,
            0xFC2D => self.step_mode = false,
            // {TEXT}var~message~
            0xFC28 => {
                let var = norm_name(&a(self, wp, 0));
                let msg = a(self, wp, 1);
                let origin = self.render(wp, &msg);
                self.draw_text_input(&[], origin);
                self.set_wait(Wait::Text { var, buf: Vec::new(), origin });
            }
            // {WAIT}tenths~
            0xFC2A => {
                let n = eval(&a(self, wp, 0)).and_then(|v| num(&v)).unwrap_or(0).clamp(0, 36_000);
                self.set_wait(Wait::Until(Instant::now() + Duration::from_millis(n as u64 * 100)));
            }
            // {WHILE}expr~
            0xFC2B => {
                if !truthy(&a(self, wp, 0)) {
                    set_pc(self, m.after_struct(pc));
                }
            }
            // {ORIGINAL KEY}
            0xFC2E => {
                let key = self.frames.last().and_then(|f| f.key_name.clone());
                let w = key.and_then(|k| wp.keyboard.as_ref().and_then(|kb| kb.original.get(&k).copied()));
                if let Some(w) = w {
                    if let Some(f) = self.frames.last_mut() {
                        f.pending.push_front(w);
                    }
                }
            }
            // {STATUS PROMPT}message~
            0xFC32 => {
                let msg = display_text(&a(self, wp, 0));
                if msg.is_empty() {
                    wp.message = None;
                } else {
                    wp.flash(&msg);
                }
            }
            // {INPUT}message~
            0xFC33 => {
                let msg = a(self, wp, 0);
                self.render(wp, &msg);
                self.set_wait(Wait::Pause);
            }
            // {PAUSE KEY}key~
            0xFC3B => {
                let k = a(self, wp, 0);
                self.set_wait(match k.first() {
                    Some(&w) => Wait::PauseKey(w),
                    None => Wait::Pause,
                });
            }
            // Functions used outside an argument type their value.
            0xFC29 | 0xFC34..=0xFC39 => {
                let v = self.expand(wp, &m.words, span);
                if let Some(f) = self.frames.last_mut() {
                    for x in v.into_iter().rev() {
                        f.pending.push_front(x);
                    }
                }
            }
            _ => {} // {OTHERWISE}, {LABEL}, invalid codes
        }
    }
}

/// The number a word stands for as an argument of a display code (^P
/// column/row, ^N/^O attribute): WP uses the character's byte in the PC
/// character set, so `[WP 5,19]` (◙, CP437 0x0A) means 10 and `{Home}`
/// (^H) means 8.
fn coord(w: u16) -> usize {
    const CTRL: &str = "☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼";
    static CP437: std::sync::OnceLock<HashMap<u16, usize>> = std::sync::OnceLock::new();
    match w {
        0..=0xFF => w as usize,
        0x8000..=0x80FF => (w & 0xFF) as usize,
        _ => {
            let t = CP437.get_or_init(|| {
                let mut m = HashMap::new();
                for l in include_str!("../docs/formats/wp5_dos_codepages.tsv").lines() {
                    let f: Vec<&str> = l.split('\t').collect();
                    if f.len() >= 5 && f[0] == "cp437" {
                        if let (Ok(b), Ok(s), Ok(i)) = (f[1].parse::<usize>(), f[3].parse::<u16>(), f[4].parse::<u16>()) {
                            m.entry(s << 8 | i).or_insert(b);
                        }
                    }
                }
                m
            });
            if let Some(&b) = t.get(&w) {
                return b;
            }
            let c = word_char(w);
            c.and_then(|c| CTRL.chars().position(|x| x == c)).map(|p| p + 1).unwrap_or((w & 0xFF) as usize)
        }
    }
}

fn num_or(w: &[u16], d: i64) -> i64 {
    eval(w).and_then(|v| num(&v)).unwrap_or(d)
}

/// WP 5.x attribute number (font appearance/size order used in documents).
fn attr_index(a: Attr) -> u32 {
    match a {
        Attr::ExtraLarge => 0,
        Attr::VeryLarge => 1,
        Attr::Large => 2,
        Attr::Small => 3,
        Attr::Fine => 4,
        Attr::Superscript => 5,
        Attr::Subscript => 6,
        Attr::Outline => 7,
        Attr::Italic => 8,
        Attr::Shadow => 9,
        Attr::Redline => 10,
        Attr::DblUnderline => 11,
        Attr::Bold => 12,
        Attr::Strikeout => 13,
        Attr::Underline => 14,
        Attr::SmallCaps => 15,
    }
}

/// The numeric value {SYSTEM}left~/right~ report for a code (WP 5.1's
/// internal code numbers, as used by CODES.WPM / REVEALCO.WPM).
fn code_number(c: &Code) -> i64 {
    match c {
        Code::HRt => 32778,
        Code::HPg => 32780,
        Code::Tab => 49408,
        Code::Indent => 49664,
        Code::DblIndent => 49665,
        Code::MarRel => 49536,
        Code::Center => 49632,
        Code::FlushRight => 49504,
        Code::HardSpace => 40960,
        Code::HardHyphen => 43264,
        Code::SoftHyphen => 44288,
        Code::AttrOn(a) => 0xC300 + attr_index(*a) as i64,
        Code::AttrOff(a) => 0xC400 + attr_index(*a) as i64,
        Code::CenterPage => 34304,
        Code::WidowOrphan(on) => {
            if *on {
                35328
            } else {
                35584
            }
        }
        Code::Hyphenation(on) => {
            if *on {
                40704
            } else {
                40448
            }
        }
        Code::LineSpacing(_) => 0xD002,
        Code::LRMargin { .. } => 0xD001,
        Code::TabSet(_) => 0xD004,
        Code::TBMargin { .. } => 0xD005,
        Code::Just(_) => 0xD006,
        Code::Suppress(_) => 0xD007,
        Code::PageNumberPos(_) => 0xD008,
        Code::Font(_) => 0xD101,
        Code::PageNumber(_) => 0xD304,
        _ => 0xD000,
    }
}

/// The kind of the topmost real screen (the macro overlay doesn't count).
fn top_kind(wp: &Wp) -> Option<ScreenKind> {
    wp.screens.iter().rev().map(|s| s.kind()).find(|k| *k != ScreenKind::Overlay)
}

/// How a token looks in the macro editor (for {STEP ON}).
fn tok_text(m: &Macro, pc: usize) -> String {
    match m.toks.get(pc) {
        Some(Tok::W(w)) => match word_char(*w) {
            Some(c) => c.to_string(),
            None if *w & 0xFF00 == 0x8000 => key_func(*w).macro_name(),
            None => format!("{{{w:04X}}}"),
        },
        Some(Tok::C { code, span, .. }) => {
            let end = span.end.min(span.start + 30);
            let mut s = format!("{{CMD {code:04X}}}");
            s.push_str(&display_text(&m.words[span.start + 1..end.max(span.start + 1)]));
            s
        }
        _ => format!("(end of {})", m.name),
    }
}

// ============================================== Ctrl-PgUp (Macro Commands)

fn block_words(wp: &mut Wp) -> Option<Vec<u16>> {
    let els = wp.ed_ref().block_els()?;
    wp.ed().block_anchor = None;
    Some(
        els.iter()
            .filter_map(|e| match e {
                El::Ch(c) => char_word(*c),
                El::Code(Code::HRt) => Some(0x800A),
                El::Code(Code::Tab) => Some(0x8009),
                El::Code(Code::HardSpace) => Some(0x20),
                _ => None,
            })
            .collect(),
    )
}

/// `1 Pause; 2 Display; 3 Assign; 4 Comment` (Ctrl-PgUp in a macro).
pub fn macro_commands_menu() -> Box<dyn Screen> {
    MenuLine::new(&["~Pause", "~Display", "~Assign", "~Comment"], |_wp, k| -> Next {
        match k {
            2 => Some(Box::new(AssignVar)),
            3 => Some(Prompt::new("Comment:", "", |_, _| None)),
            _ => None,
        }
    })
}

/// Ctrl-PgUp outside macro definition: `Variable: `.
pub fn assign_var_screen() -> Box<dyn Screen> {
    Box::new(AssignVar)
}

/// `Variable: ` — one digit; with Block on the blocked text is assigned.
struct AssignVar;

impl Screen for AssignVar {
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char(c) if c.is_ascii_digit() => {
                let var = c.to_string();
                if let Some(w) = block_words(wp) {
                    wp.macro_vm.get_or_insert_with(Default::default).set_var(&var, w);
                    return Outcome::Close;
                }
                Outcome::Replace(Prompt::new("Value:", "", move |wp, s| {
                    if let Some(s) = s {
                        let w: Vec<u16> = s.chars().filter_map(char_word).collect();
                        wp.macro_vm.get_or_insert_with(Default::default).set_var(&var, assign_value(w));
                    }
                    None
                }))
            }
            Input::Func(Func::Cancel | Func::Esc | Func::Enter | Func::Exit) => Outcome::Close,
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let row = g.rows - 1;
        g.fill_row(row, 0, g.cols, ' ', wp.theme.normal);
        let c = g.text(0, row, "Variable: ", wp.theme.normal);
        g.cursor = Some((c, row));
    }
}

// ============================================================ public API

/// Merge a VM that was created while ours was taken out (a macro started
/// from inside a keystroke) and re-show the overlay.
fn put_back(wp: &mut Wp, mut vm: Box<MacroVm>) {
    if let Some(o) = wp.macro_vm.take() {
        let o = *o;
        if vm.frames.is_empty() {
            vm.frames.extend(o.frames);
        } else {
            vm.queued.extend(o.frames);
        }
        vm.vars.extend(o.vars);
        if o.chain.is_some() {
            vm.chain = o.chain;
        }
        vm.typeahead.extend(o.typeahead);
    }
    vm.sync_overlay(wp);
    wp.macro_vm = Some(vm);
}

fn start(wp: &mut Wp, name: &str, words: Vec<u16>, key: Option<String>) {
    let mut vm = wp.macro_vm.take().unwrap_or_default();
    vm.push_frame(wp, name, words, key);
    put_back(wp, vm);
}

/// Run a .WPM file (Alt-F10, Alt-letter).
pub fn run_file(wp: &mut Wp, p: &Path) {
    match load_wpm(p) {
        Ok(f) => {
            let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            start(wp, &name, f.words, None);
        }
        Err(e) => wp.flash(&format!("ERROR: {e} -- {}", p.display())),
    }
}

/// Run a macro body that came from a keyboard file.
pub fn run_body(wp: &mut Wp, name: &str, body: Vec<u16>) {
    start(wp, name, body, Some(name.to_string()));
}

/// Ctrl-F10 finished: write the recorded keystrokes as a WP 5.1 macro.
pub fn save_recorded(wp: &mut Wp, name: &str, desc: &str, inputs: &[Input]) {
    let mut words = vec![0xFC0C]; // {DISPLAY OFF}, as WP records
    words.extend(inputs.iter().filter_map(input_word));
    let path = crate::macros::macro_path(wp, name);
    if let Err(e) = std::fs::write(&path, encode_wpm(desc, &words)) {
        wp.flash(&format!("ERROR: Can't write {} -- {e}", path.display()));
    }
}

/// Advance the running macro.  Returns true if it did something (call
/// again), false when idle / waiting / finished.
pub fn step(wp: &mut Wp) -> bool {
    let Some(mut vm) = wp.macro_vm.take() else { return false };
    let r = vm.run_one(wp);
    put_back(wp, vm);
    r
}

/// Offer a real keystroke to a running macro.  Returns true if consumed.
pub fn feed_key(wp: &mut Wp, i: &Input) -> bool {
    let Some(mut vm) = wp.macro_vm.take() else { return false };
    if vm.frames.is_empty() {
        wp.macro_vm = Some(vm);
        return false;
    }
    vm.on_key(wp, i.clone());
    put_back(wp, vm);
    true
}

/// When a macro is sleeping in {WAIT}: the moment it wants to continue
/// (the event loop uses this as its wake-up time).
pub fn next_deadline(wp: &Wp) -> Option<Instant> {
    match wp.macro_vm.as_ref()?.frames.last()?.wait {
        Wait::Until(t) => Some(t),
        _ => None,
    }
}

/// Is a macro running or waiting?
#[allow(dead_code)]
pub fn is_running(wp: &Wp) -> bool {
    wp.macro_vm.as_ref().is_some_and(|v| v.running())
}

// ================================================================= tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Func;
    use std::path::PathBuf;

    pub(crate) fn her_dir() -> Option<PathBuf> {
        ["I:\\Claude Space\\wp51", "/mnt/i/Claude Space/wp51"].iter().map(PathBuf::from).find(|p| p.is_dir())
    }

    fn run(wp: &mut Wp) {
        for _ in 0..2_000_000 {
            if !step(wp) {
                match next_deadline(wp) {
                    Some(t) => std::thread::sleep(t.saturating_duration_since(Instant::now())),
                    None => break,
                }
            }
        }
    }

    fn text(wp: &Wp) -> String {
        wp.ed_ref()
            .doc
            .els
            .iter()
            .map(|e| match e {
                El::Ch(c) => c.to_string(),
                El::Code(Code::HRt) => "\n".into(),
                El::Code(c) => format!("[{c:?}]"),
            })
            .collect()
    }

    /// Build a macro body from a tiny script: plain text, `{K:xxxx}` raw
    /// words and `{name}` for commands.
    fn body(s: &str) -> Vec<u16> {
        let cmds: &[(&str, u16)] = &[
            ("ASSIGN", 0xFC01),
            ("BREAK", 0xFC03),
            ("CALL", 0xFC04),
            ("CASE", 0xFC07),
            ("CASE CALL", 0xFC08),
            ("CHAR", 0xFC0A),
            (";", 0xFC0B),
            ("ELSE", 0xFC0E),
            ("END FOR", 0xFC0F),
            ("END IF", 0xFC10),
            ("END WHILE", 0xFC11),
            ("FOR", 0xFC12),
            ("FOR EACH", 0xFC13),
            ("GO", 0xFC14),
            ("IF", 0xFC15),
            ("LABEL", 0xFC16),
            ("LOOK", 0xFC17),
            ("NEXT", 0xFC19),
            ("ON CANCEL", 0xFC1B),
            ("ON ERROR", 0xFC1C),
            ("ON NOT FOUND", 0xFC1D),
            ("PAUSE", 0xFC1E),
            ("PROMPT", 0xFC1F),
            ("QUIT", 0xFC20),
            ("RETURN", 0xFC22),
            ("TEXT", 0xFC28),
            ("STATE", 0xFC29),
            ("WAIT", 0xFC2A),
            ("WHILE", 0xFC2B),
            ("IF EXISTS", 0xFC2F),
            ("VARIABLE", 0xFC34),
            ("SYSTEM", 0xFC35),
            ("MID", 0xFC36),
            ("NTOK", 0xFC37),
            ("KTON", 0xFC38),
            ("LEN", 0xFC39),
            ("OTHERWISE", 0xFC3D),
            ("CANCEL OFF", 0xFC05),
            ("Enter", 0x800A),
            ("Search", 0x8021),
            ("Bold", 0x8025),
            ("Home", 0x8008),
            ("Left", 0x8019),
            ("Right", 0x8018),
            ("Block", 0x803B),
            ("End", 0x8055),
            ("Cancel", 0x8020),
            ("CHAIN", 0xFC09),
            ("NEST", 0xFC18),
        ];
        let mut out = Vec::new();
        let mut it = s.chars().peekable();
        while let Some(c) = it.next() {
            if c == '{' {
                let mut n = String::new();
                for c in it.by_ref() {
                    if c == '}' {
                        break;
                    }
                    n.push(c);
                }
                if let Some(h) = n.strip_prefix("K:") {
                    out.push(u16::from_str_radix(h, 16).unwrap());
                } else if let Some(v) = n.strip_prefix("VAR ") {
                    out.push(0xFF00 + v.parse::<u16>().unwrap());
                } else {
                    out.push(cmds.iter().find(|x| x.0 == n).unwrap_or_else(|| panic!("{n}")).1);
                }
            } else if c != '\n' {
                out.push(char_word(c).unwrap());
            }
        }
        out
    }

    fn play(wp: &mut Wp, s: &str) {
        run_body(wp, "test", body(s));
        run(wp);
    }

    #[test]
    fn expressions() {
        let e = |s: &str| eval(&s.chars().map(|c| c as u16).collect::<Vec<_>>());
        assert_eq!(e("1+2*3"), Some(Val::N(7)));
        assert_eq!(e("(1+2)*3"), Some(Val::N(9)));
        assert_eq!(e("7/2"), Some(Val::N(3)));
        assert_eq!(e("5=5"), Some(Val::N(-1)));
        assert_eq!(e("5!=5|2<3"), Some(Val::N(-1)));
        assert_eq!(e("300&256"), Some(Val::N(256)));
        assert_eq!(e("!0"), Some(Val::N(-1)));
        assert_eq!(e("\"ab\"=\"ab\""), Some(Val::N(-1)));
        assert_eq!(e("\" \"=\" \""), Some(Val::N(-1)));
        assert_eq!(e("y=y"), Some(Val::N(-1)));
        assert_eq!(e("53776/256=210|1=2"), Some(Val::N(-1)));
        assert_eq!(e("1-2-93"), Some(Val::N(-94)));
        assert!(matches!(e("hello"), Some(Val::S(_))));
        assert_eq!(e("1/0"), None);
        assert_eq!(e("+1"), Some(Val::N(1)));
    }

    #[test]
    fn parse_all_her_macros() {
        let Some(dir) = her_dir() else { return };
        let mut n = 0;
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wpm")) {
                    let f = load_wpm(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                    let m = compile("x", f.words);
                    assert!(matches!(m.toks.last(), Some(Tok::End(_))));
                    n += 1;
                }
            }
        }
        assert!(n >= 400, "only {n} macros found");
        // known descriptions
        let wpm = her_dir().unwrap().join("WPM");
        assert_eq!(load_wpm(&wpm.join("AE.WPM")).unwrap().description, "umlaut ae");
        assert_eq!(load_wpm(&wpm.join("COE.WPM")).unwrap().description, "capitalized Ö");
        let b = load_wpm(&wpm.join("BOLD.WPM")).unwrap();
        assert_eq!(b.words, vec![0xFC0C, 0x8008, 0x8008, 0x8019, 0x803B, 0x8055, 0x8025]);
    }

    #[test]
    fn her_simple_macros() {
        let Some(dir) = her_dir() else { return };
        let wpm = dir.join("WPM");
        let mut wp = Wp::new(80, 25);
        wp.macro_dirs = vec![wpm.clone()];
        run_file(&mut wp, &wpm.join("AE.WPM"));
        run(&mut wp);
        run_file(&mut wp, &wpm.join("COE.WPM"));
        run(&mut wp);
        assert_eq!(text(&wp), "äÖ");

        let mut wp = Wp::new(80, 25);
        for c in "Hello world".chars() {
            wp.input(Input::Char(c));
        }
        run_file(&mut wp, &wpm.join("BOLD.WPM"));
        run(&mut wp);
        assert_eq!(text(&wp), "[AttrOn(Bold)]Hello world[AttrOff(Bold)]");

        let mut wp = Wp::new(80, 25);
        run_file(&mut wp, &wpm.join("GERQUOTE.WPM"));
        run(&mut wp);
        assert_eq!(text(&wp), "„");
        assert!(!wp.has_screens());
    }

    #[test]
    fn programming_commands() {
        let mut wp = Wp::new(80, 25);
        play(
            &mut wp,
            "{ASSIGN}n~0~{ASSIGN}s~~\
             {WHILE}{VARIABLE}n~<5~{ASSIGN}n~{VARIABLE}n~+1~\
             {IF}{VARIABLE}n~=3~{ELSE}{VARIABLE}n~{END IF}{END WHILE}\
             -{FOR}i~1~3~1~{CALL}sub~{END FOR}\
             -{CASE}{VARIABLE}n~~1~one~5~five~~X{GO}end~\
             {LABEL}five~F{GO}end~\
             {LABEL}one~1\
             {LABEL}sub~<{VARIABLE}i~>{RETURN}\
             {LABEL}end~",
        );
        assert_eq!(text(&wp), "1245-<1><2><3>-F");

        // FOR EACH, BREAK, NEXT, VAR, MID/LEN/KTON/NTOK, IF EXISTS, OTHERWISE
        let mut wp = Wp::new(80, 25);
        play(
            &mut wp,
            "{FOR EACH}x~a~b~c~~{VARIABLE}x~{END FOR}\
             {ASSIGN}k~0~{WHILE}1~{ASSIGN}k~{VARIABLE}k~+1~{IF}{VARIABLE}k~=2~{NEXT}{END IF}\
             {IF}{VARIABLE}k~>3~{BREAK}{END IF}{VARIABLE}k~{END WHILE}\
             {ASSIGN}1~hello~{VAR 1}{MID}1~1~3~{LEN}1~\
             {KTON}A~{NTOK}66~\
             {IF EXISTS}1~Y{ELSE}N{END IF}{IF EXISTS}zz~Y{ELSE}N{END IF}\
             {CASE}q~a~x~{OTHERWISE}~o~~{LABEL}x~X{LABEL}o~O",
        );
        assert_eq!(text(&wp), "abc13helloell565BYNO");
    }

    #[test]
    fn variables_persist_and_quit() {
        let mut wp = Wp::new(80, 25);
        play(&mut wp, "{ASSIGN}5~abc~{ASSIGN}foo~7*6~");
        play(&mut wp, "{VAR 5}{VARIABLE}FOO~{QUIT}zzz");
        assert_eq!(text(&wp), "abc42");
    }

    #[test]
    fn not_found_handling() {
        let mut wp = Wp::new(80, 25);
        for c in "abc".chars() {
            wp.input(Input::Char(c));
        }
        wp.input(Input::Func(Func::Home));
        wp.input(Input::Func(Func::Home));
        wp.input(Input::Func(Func::Up));
        // without a handler, a failing search ends the macro
        play(&mut wp, "{Search}zz{Search}X");
        assert_eq!(text(&wp), "abc");
        play(&mut wp, "{ON NOT FOUND}{GO}nf~~{Search}zz{Search}X{LABEL}nf~N");
        assert!(text(&wp).starts_with('N'), "{}", text(&wp));
        // a loop that runs until the search fails
        let mut wp = Wp::new(80, 25);
        for c in "a.b.c.".chars() {
            wp.input(Input::Char(c));
        }
        wp.input(Input::Func(Func::Home));
        wp.input(Input::Func(Func::Home));
        wp.input(Input::Func(Func::Up));
        play(&mut wp, "{LABEL}l~{Search}.{Search}!{GO}l~");
        assert_eq!(text(&wp), "a.!b.!c.!");
        // GO to a missing label → ON ERROR
        let mut wp = Wp::new(80, 25);
        play(&mut wp, "{ON ERROR}{GO}e~~{GO}nowhere~X{LABEL}e~E");
        assert_eq!(text(&wp), "E");
    }

    #[test]
    fn waiting_commands() {
        let mut wp = Wp::new(80, 25);
        run_body(&mut wp, "t", body("{CHAR}c~Key? ~[{VARIABLE}c~]{TEXT}t~Name: ~<{VARIABLE}t~>{PAUSE}!"));
        run(&mut wp);
        assert!(is_running(&wp));
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let last: String = (0..10).map(|c| g.get(c, 24).unwrap().ch).collect();
        assert_eq!(last, "Key?      ");
        assert!(feed_key(&mut wp, &Input::Char('x')));
        run(&mut wp);
        for c in "Bob".chars() {
            feed_key(&mut wp, &Input::Char(c));
        }
        feed_key(&mut wp, &Input::Func(Func::Enter));
        run(&mut wp);
        assert_eq!(text(&wp), "[x]<Bob>");
        // during PAUSE keys go to the document until Enter
        feed_key(&mut wp, &Input::Char('p'));
        run(&mut wp);
        feed_key(&mut wp, &Input::Func(Func::Enter));
        run(&mut wp);
        assert!(!is_running(&wp));
        assert_eq!(text(&wp), "[x]<Bob>p!");
        assert!(!wp.has_screens());
        // Cancel aborts; {ON CANCEL} catches it
        let mut wp = Wp::new(80, 25);
        run_body(&mut wp, "t", body("{CHAR}c~?~X"));
        run(&mut wp);
        feed_key(&mut wp, &Input::Func(Func::Cancel));
        run(&mut wp);
        assert!(!is_running(&wp));
        assert_eq!(text(&wp), "");
        run_body(&mut wp, "t", body("{ON CANCEL}{GO}c~~{CHAR}c~?~X{LABEL}c~C"));
        run(&mut wp);
        feed_key(&mut wp, &Input::Func(Func::Cancel));
        run(&mut wp);
        assert_eq!(text(&wp), "C");
        // WAIT and LOOK
        let mut wp = Wp::new(80, 25);
        play(&mut wp, "{WAIT}1~{LOOK}k~{IF EXISTS}k~K{ELSE}-{END IF}");
        assert_eq!(text(&wp), "-");
    }

    #[test]
    fn infinite_loop_is_cancelable() {
        let mut wp = Wp::new(80, 25);
        run_body(&mut wp, "t", body("{LABEL}x~{GO}x~"));
        for _ in 0..10_000 {
            assert!(step(&mut wp));
        }
        feed_key(&mut wp, &Input::Func(Func::Cancel));
        run(&mut wp);
        assert!(!is_running(&wp));
    }

    #[test]
    fn record_save_reload_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wpm_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut wp = Wp::new(80, 25);
        wp.macro_dirs = vec![dir.clone()];
        let inputs = vec![
            Input::Char('H'),
            Input::Char('ü'),
            Input::Func(Func::Enter),
            Input::Char('x'),
            Input::Func(Func::Home),
            Input::Func(Func::Home),
            Input::Func(Func::Left),
            Input::Func(Func::Block),
            Input::Func(Func::End),
            Input::Func(Func::Bold),
        ];
        // reference: type it directly
        let mut direct = Wp::new(80, 25);
        for i in &inputs {
            direct.input(i.clone());
        }
        save_recorded(&mut wp, "rt", "Grüße test", &inputs);
        let p = dir.join("RT.WPM");
        let f = load_wpm(&p).unwrap();
        assert_eq!(f.description, "Grüße test");
        assert_eq!(f.words[0], 0xFC0C);
        assert_eq!(f.words.len(), inputs.len() + 1);
        run_file(&mut wp, &p);
        run(&mut wp);
        assert_eq!(text(&wp), text(&direct));
        assert_eq!(wp.ed_ref().cursor, direct.ed_ref().cursor);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Run every one of her macros on a small document, answering any
    /// question with Enter, then Cancel.  Nothing may panic or hang.
    /// (Slow: run with `cargo test smoke -- --ignored --nocapture`.)
    #[test]
    #[ignore]
    fn smoke_all_her_macros() {
        let Some(dir) = her_dir() else { return };
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("WPM")).unwrap().flatten().map(|e| e.path()).collect();
        files.retain(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wpm")));
        files.sort();
        let tmp = std::env::temp_dir().join(format!("wpm_smoke_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut bad = Vec::new();
        let mut finished = 0;
        let mut unfinished = Vec::new();
        for p in &files {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut wp = Wp::new(80, 25);
                wp.cwd = tmp.clone();
                wp.macro_dirs = vec![dir.join("WPM")];
                for c in "Erste Zeile mit Text. Zweiter Satz.".chars() {
                    wp.input(Input::Char(c));
                }
                wp.input(Input::Func(Func::Enter));
                for c in "Noch eine Zeile".chars() {
                    wp.input(Input::Char(c));
                }
                wp.input(Input::Func(Func::Up));
                run_file(&mut wp, p);
                let mut answered = 0;
                for _ in 0..200_000 {
                    if step(&mut wp) {
                        continue;
                    }
                    if !is_running(&wp) {
                        break;
                    }
                    if next_deadline(&wp).is_some() {
                        // don't actually sleep in the smoke test
                        if let Some(f) = wp.macro_vm.as_mut().unwrap().frames.last_mut() {
                            f.wait = Wait::No;
                        }
                        continue;
                    }
                    answered += 1;
                    let k = if answered > 20 { Func::Cancel } else { Func::Enter };
                    if answered > 40 {
                        wp.macro_vm.as_mut().unwrap().frames.clear();
                        break;
                    }
                    feed_key(&mut wp, &Input::Func(k));
                }
                let done = !is_running(&wp);
                wp.macro_vm.as_mut().map(|v| v.frames.clear());
                let mut g = Grid::new(80, 25, wp.theme.normal);
                wp.draw(&mut g);
                done
            }));
            match r {
                Ok(true) => finished += 1,
                Ok(false) => unfinished.push(p.file_name().unwrap().to_string_lossy().into_owned()),
                Err(_) => bad.push(p.file_name().unwrap().to_string_lossy().into_owned()),
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!("smoke: {} macros, {finished} ran to completion, unfinished {unfinished:?}, panics: {bad:?}", files.len());
        assert!(bad.is_empty(), "panics in {bad:?}");
    }

    /// Run one macro on a small two-document setup the way the smoke test
    /// does (Enter to every question, later Cancel).  True = it finished.
    fn runs_to_end(dir: &Path, p: &Path, cancel_loops: bool) -> bool {
        let tmp = std::env::temp_dir().join(format!("wpm_end_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut wp = Wp::new(80, 25);
        wp.cwd = tmp.clone();
        wp.macro_dirs = vec![dir.join("WPM")];
        for c in "Erste Zeile mit Text. Zweiter Satz.".chars() {
            wp.input(Input::Char(c));
        }
        wp.input(Input::Func(Func::Enter));
        for c in "Noch eine Zeile".chars() {
            wp.input(Input::Char(c));
        }
        wp.input(Input::Func(Func::Up));
        run_file(&mut wp, p);
        let mut answered = 0;
        for _ in 0..200_000 {
            if step(&mut wp) {
                continue;
            }
            if !is_running(&wp) {
                break;
            }
            if next_deadline(&wp).is_some() {
                if let Some(f) = wp.macro_vm.as_mut().unwrap().frames.last_mut() {
                    f.wait = Wait::No;
                }
                continue;
            }
            answered += 1;
            if answered > 40 {
                break;
            }
            let k = if answered > 20 { Func::Cancel } else { Func::Enter };
            feed_key(&mut wp, &Input::Func(k));
        }
        // a macro that loops until the user presses Cancel (REVEALTX walks
        // left looking for a line start that is not there): Cancel ends it
        // through its {ON CANCEL} handler, as it would in WP.
        if is_running(&wp) && cancel_loops {
            feed_key(&mut wp, &Input::Func(Func::Cancel));
            for _ in 0..200_000 {
                if !step(&mut wp) {
                    break;
                }
            }
        }
        let done = !is_running(&wp);
        let _ = std::fs::remove_dir_all(&tmp);
        done
    }

    #[test]
    fn formerly_endless_macros_finish() {
        let Some(dir) = her_dir() else { return };
        for name in ["COUNT.WPM", "UX.WPM", "REVEALCO.WPM"] {
            assert!(runs_to_end(&dir, &dir.join("WPM").join(name), false), "{name} did not finish");
        }
        assert!(runs_to_end(&dir, &dir.join("WPM").join("REVEALTX.WPM"), true), "REVEALTX did not stop on Cancel");
    }

    #[test]
    fn nest_chain_and_keystroke_chaining() {
        let dir = std::env::temp_dir().join(format!("wpm_chain_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CH2.WPM"), encode_wpm("second", &body("B"))).unwrap();
        std::fs::write(dir.join("ALTQ.WPM"), encode_wpm("alt q", &body("Q"))).unwrap();
        let mut wp = Wp::new(80, 25);
        wp.cwd = dir.clone();
        wp.macro_dirs = vec![dir.clone()];
        play(&mut wp, "{NEST}ch2~A");
        play(&mut wp, "{CHAIN}ch2~C");
        play(&mut wp, "{K:8041}ch2{Enter}D");
        play(&mut wp, "{K:FE51}E");
        assert_eq!(text(&wp), "BACBDBQE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Her pop-up calculator: positioned prompts, {CHAR}, CASE, MID/LEN/NTOK.
    #[test]
    fn her_calculator() {
        let Some(dir) = her_dir() else { return };
        let mut wp = Wp::new(80, 25);
        run_file(&mut wp, &dir.join("WPM").join("CALC.WPM"));
        run(&mut wp);
        for c in "12+3=".chars() {
            assert!(is_running(&wp));
            feed_key(&mut wp, &Input::Char(c));
            run(&mut wp);
        }
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        let screen: String = (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>() + "\n").collect();
        assert!(screen.contains("CALCULATOR") && screen.contains("15.00"), "{screen}");
        feed_key(&mut wp, &Input::Func(Func::Exit));
        run(&mut wp);
        assert!(!is_running(&wp));
        assert_eq!(text(&wp), "");
    }

    #[test]
    fn ctrl_pgup_assign_from_block() {
        let mut wp = Wp::new(80, 25);
        for c in "word".chars() {
            wp.input(Input::Char(c));
        }
        play(&mut wp, "{Home}{Left}{Block}{End}{K:FC2C}31{Right} {VAR 1}");
        assert_eq!(text(&wp), "word word");
    }
}
