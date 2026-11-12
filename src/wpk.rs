//! .WPK keyboard layouts (WP 5.1 Setup: Keyboard Layout).
//! Format: docs/formats/wpm_wpk_set.md §3.
//!
//! The key table is indexed by what the PC BIOS returned for a key press
//! (scan code / ASCII).  We translate every slot into our physical-key
//! model (`KeyIn`) and keep only the keys whose action differs from WP's
//! original keyboard; all other keys then use `keys::default_map`.

use crate::keys::{Func, Input, K, KeyIn, Mods};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug)]
pub enum KeyAction {
    /// The key performs this WP function / types this character.
    Input(Input),
    /// The key runs a macro stored inside the keyboard file (raw WPM body
    /// words, as in a .WPM file body).
    Macro { name: String, body: Vec<u16> },
}

#[derive(Clone, Debug, Default)]
pub struct Keyboard {
    pub name: String,
    pub map: HashMap<KeyIn, KeyAction>,
    /// Keyboard macros by number ({KEY MACRO n}).
    pub macros: HashMap<u8, Vec<u16>>,
    /// For each `KeyAction::Macro` name: the key's action on the original
    /// keyboard (macro word), used by {ORIGINAL KEY}.
    pub original: HashMap<String, u16>,
    /// Key descriptions from the file (by key-macro name or key label).
    pub descriptions: HashMap<String, String>,
}

/// Built-in default key table (WP.MRS packet 2, identical to US.WPK packet 1).
const DEFAULT_TABLE_HEX: &str = concat!(
    "0fa65e8051fe57fe45fe52fe54fe59fe55fe49fe4ffe50fe000000000000ffff41fe53fe44fe46fe47fe48fe4afe4bfe",
    "4cfe000000000000ffff00005afe58fe43fe56fe42fe4efe4dfe000000000000ffff0000ffffffffffff208021802280",
    "2380248025802680278028802980ffffffff088017805980ff7f198000001880000055801a805a805d8051802c802d80",
    "2e802f803080318032803380348035804480458046804780488049804a804b804c804d80388039803a803b803c803d80",
    "3e803f80408041803280548053800b800c80588001ff02ff03ff04ff05ff06ff07ff08ff09ff00ff61806b802cfc3a80",
    "3b80000000000000000000000000638000000000000064806e806d80000000000000698067800000ffff6580ffff6680",
    "ffff00006880000000000000000000000000017f0180028003800480058006800780088009800a800b800c800d800e80",
    "0f8010801180128013801480158016801780188019801a801b801c801d80ffff60802000210022002300240025002600",
    "2700280029002a002b002c002d002e002f0030003100320033003400350036003700380039003a003b003c003d003e00",
    "3f0040004100420043004400450046004700480049004a004b004c004d004e004f005000510052005300540055005600",
    "5700580059005a005b005c005d005e005f0060006100620063006400650066006700680069006a006b006c006d006e00",
    "6f0070007100720073007400750076007700780079007a007b007c007d007e005280e0470880e0481780e0495980e04b",
    "1980e04d1880e04f5580e0501a80e0515a80e0525d80e0535180e0735480e0745380e0750b80e0760c80e0775880e084",
    "2cfce08d6380e0916480e0926e80e0936d80ffff00001b011b80080e5080090f09800a1c5f800d1c0a800ae05f800de0",
    "0a80ffff00002a372a002d4a5c802b4e5b802fe02f00ffff00002a372a002d4a2d002b4e2b002fe02f00ffff0000",
);

fn default_table() -> Vec<u8> {
    let h = DEFAULT_TABLE_HEX.as_bytes();
    h.chunks(2).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap()).collect()
}

/// Identity of a slot in the key table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Slot {
    Ext(u8),
    Ascii(u8),
    Pair(u8, u8, u8), // list, ascii, scan
}

struct Entry {
    off: usize,
    slot: Slot,
    value: u16,
}

fn rd16(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*d.get(o)?, *d.get(o + 1)?]))
}

fn table_entries(p: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut pos = 0;
    for blk in 0..2 {
        let (Some(&first), Some(&last)) = (p.get(pos), p.get(pos + 1)) else { return out };
        pos += 2;
        for code in first..=last {
            let Some(v) = rd16(p, pos) else { return out };
            let slot = if blk == 0 { Slot::Ext(code) } else { Slot::Ascii(code) };
            out.push(Entry { off: pos, slot, value: v });
            pos += 2;
        }
    }
    let mut list = 0u8;
    while pos + 4 <= p.len() {
        let (a, s) = (p[pos], p[pos + 1]);
        let v = rd16(p, pos + 2).unwrap();
        if a == 0xFF && s == 0xFF {
            list += 1;
            pos += 4;
            continue;
        }
        out.push(Entry { off: pos + 2, slot: Slot::Pair(list, a, s), value: v });
        pos += 4;
    }
    out
}

fn ki(k: K, shift: bool, ctrl: bool, alt: bool) -> Option<KeyIn> {
    Some(KeyIn { k, m: Mods { shift, ctrl, alt } })
}

/// Extended key (ASCII 0) by BIOS scan code.
fn ext_key(scan: u8) -> Option<KeyIn> {
    let alt = |c: char| ki(K::Char(c), false, false, true);
    match scan {
        0x0F => ki(K::Tab, true, false, false),
        0x10..=0x19 => alt("qwertyuiop".as_bytes()[(scan - 0x10) as usize] as char),
        0x1E..=0x26 => alt("asdfghjkl".as_bytes()[(scan - 0x1E) as usize] as char),
        0x2C..=0x32 => alt("zxcvbnm".as_bytes()[(scan - 0x2C) as usize] as char),
        0x1A => alt('['),
        0x1B => alt(']'),
        0x1C => ki(K::Enter, false, false, true),
        0x27 => alt(';'),
        0x28 => alt('\''),
        0x29 => alt('`'),
        0x2B => alt('\\'),
        0x33 => alt(','),
        0x34 => alt('.'),
        0x35 => alt('/'),
        0x3B..=0x44 => ki(K::F(scan - 0x3A), false, false, false),
        0x54..=0x5D => ki(K::F(scan - 0x53), true, false, false),
        0x5E..=0x67 => ki(K::F(scan - 0x5D), false, true, false),
        0x68..=0x71 => ki(K::F(scan - 0x67), false, false, true),
        0x47 => ki(K::Home, false, false, false),
        0x48 => ki(K::Up, false, false, false),
        0x49 => ki(K::PgUp, false, false, false),
        0x4A => ki(K::PadMinus, false, false, true),
        0x4B => ki(K::Left, false, false, false),
        0x4D => ki(K::Right, false, false, false),
        0x4E => ki(K::PadPlus, false, false, true),
        0x4F => ki(K::End, false, false, false),
        0x50 => ki(K::Down, false, false, false),
        0x51 => ki(K::PgDn, false, false, false),
        0x52 => ki(K::Ins, false, false, false),
        0x53 => ki(K::Del, false, false, false),
        0x73 => ki(K::Left, false, true, false),
        0x74 => ki(K::Right, false, true, false),
        0x75 => ki(K::End, false, true, false),
        0x76 => ki(K::PgDn, false, true, false),
        0x77 => ki(K::Home, false, true, false),
        0x78..=0x81 => alt("1234567890".as_bytes()[(scan - 0x78) as usize] as char),
        0x82 => alt('-'),
        0x83 => alt('='),
        0x84 => ki(K::PgUp, false, true, false),
        0x85 => ki(K::F(11), false, false, false),
        0x86 => ki(K::F(12), false, false, false),
        0x87 => ki(K::F(11), true, false, false),
        0x88 => ki(K::F(12), true, false, false),
        0x89 => ki(K::F(11), false, true, false),
        0x8A => ki(K::F(12), false, true, false),
        0x8B => ki(K::F(11), false, false, true),
        0x8C => ki(K::F(12), false, false, true),
        0x8D => ki(K::Up, false, true, false),
        0x8E => ki(K::PadMinus, false, true, false),
        0x90 => ki(K::PadPlus, false, true, false),
        0x91 => ki(K::Down, false, true, false),
        0x92 => ki(K::Ins, false, true, false),
        0x93 => ki(K::Del, false, true, false),
        0x94 => ki(K::Tab, false, true, false),
        0x97 => ki(K::Home, false, false, true),
        0x98 => ki(K::Up, false, false, true),
        0x99 => ki(K::PgUp, false, false, true),
        0x9B => ki(K::Left, false, false, true),
        0x9D => ki(K::Right, false, false, true),
        0x9F => ki(K::End, false, false, true),
        0xA0 => ki(K::Down, false, false, true),
        0xA1 => ki(K::PgDn, false, false, true),
        0xA2 => ki(K::Ins, false, false, true),
        0xA3 => ki(K::Del, false, false, true),
        0xA5 => ki(K::Tab, false, false, true),
        _ => None,
    }
}

/// Key producing an ASCII code (range B of the table).
fn ascii_key(c: u8) -> Option<KeyIn> {
    let ctrl = |c: char| ki(K::Char(c), false, true, false);
    match c {
        0x01..=0x1A => ctrl((b'a' + c - 1) as char),
        0x1B => ctrl('['),
        0x1C => ctrl('\\'),
        0x1D => ctrl(']'),
        0x1F => ctrl('-'),
        0x20..=0x7E => ki(K::Char(c as char), false, false, false),
        0x7F => ki(K::Backspace, false, true, false),
        _ => None,
    }
}

fn slot_key(s: Slot) -> Option<KeyIn> {
    match s {
        Slot::Ext(scan) => ext_key(scan),
        Slot::Ascii(c) => ascii_key(c),
        // grey (E0-prefixed) cursor keys are the same keys for us
        Slot::Pair(0, _, scan) => ext_key(scan),
        Slot::Pair(1, a, s) => match (a, s) {
            (0x1B, 0x01) => ki(K::Esc, false, false, false),
            (0x08, 0x0E) => ki(K::Backspace, false, false, false),
            (0x09, 0x0F) => ki(K::Tab, false, false, false),
            (0x0A, _) => ki(K::Enter, false, true, false),
            (0x0D, _) => ki(K::Enter, false, false, false),
            _ => None,
        },
        // keypad with NumLock off (WP's Screen Up / Down keys)
        Slot::Pair(2, 0x2D, 0x4A) => ki(K::PadMinus, false, false, false),
        Slot::Pair(2, 0x2B, 0x4E) => ki(K::PadPlus, false, false, false),
        Slot::Pair(..) => None,
    }
}

/// Human-readable key label for macro names / descriptions.
pub fn key_label(k: &KeyIn) -> String {
    let mut s = String::new();
    if k.m.shift {
        s.push_str("Shift-");
    }
    if k.m.ctrl {
        s.push_str("Ctrl-");
    }
    if k.m.alt {
        s.push_str("Alt-");
    }
    match k.k {
        K::Char(' ') => s.push_str("Space"),
        K::Char(c) if k.m.ctrl || k.m.alt => s.push(c.to_ascii_uppercase()),
        K::Char(c) => s.push(c),
        K::F(n) => s.push_str(&format!("F{n}")),
        K::Backspace => s.push_str("Bksp"),
        K::PgUp => s.push_str("PgUp"),
        K::PgDn => s.push_str("PgDn"),
        K::PadPlus => s.push_str("Grey +"),
        K::PadMinus => s.push_str("Grey -"),
        other => s.push_str(&format!("{other:?}")),
    }
    s
}

pub fn load(p: &Path) -> std::io::Result<Keyboard> {
    let d = std::fs::read(p)?;
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    parse(&d, &name).map_err(std::io::Error::other)
}

/// Parse a .WPK file.
pub fn parse(d: &[u8], name: &str) -> Result<Keyboard, String> {
    let f = crate::wpm::parse_wpc(d)?;
    if f.ftype != 3 {
        return Err(format!("not a keyboard file (type {})", f.ftype));
    }
    let table = f.packet(1).ok_or("keyboard file has no key table")?;
    let mut kb = Keyboard { name: name.to_string(), ..Default::default() };

    // packet 2: keyboard macros
    if let Some(p) = f.packet(2) {
        let count = p.first().copied().unwrap_or(0);
        let mut i = 1;
        for _ in 0..count {
            let (Some(&n), Some(len)) = (p.get(i), rd16(p, i + 1)) else { break };
            let end = (i + 3 + len as usize).min(p.len());
            kb.macros.insert(n, crate::wpm::words_of(&p[(i + 3).min(end)..end]));
            i = end;
        }
    }

    let defaults: HashMap<Slot, u16> = table_entries(&default_table()).into_iter().map(|e| (e.slot, e.value)).collect();
    let mut by_off: HashMap<usize, String> = HashMap::new();
    for e in table_entries(table) {
        let Some(key) = slot_key(e.slot) else { continue };
        let def = defaults.get(&e.slot).copied();
        if def == Some(e.value) || e.value == 0xFFFF || e.value == 0x7FFF {
            continue;
        }
        let label = key_label(&key);
        let action = match e.value {
            0x0000 => KeyAction::Macro { name: format!("(unassigned {label})"), body: Vec::new() },
            v @ (0x0020..=0x007E | 0x0100..=0x0CFF) => match crate::wpm::word_char(v) {
                Some(c) => KeyAction::Input(Input::Char(c)),
                None => continue,
            },
            0x8061 => KeyAction::Input(Input::Char('-')),
            v @ (0x8056 | 0x806C..=0x806E | 0xFC2C | 0xFF00..=0xFF09) => {
                KeyAction::Macro { name: format!("{{{v:04X}}} {label}"), body: vec![v] }
            }
            v @ 0x8000..=0x80FF => KeyAction::Input(Input::Func(crate::wpm::key_func(v))),
            v @ 0xFE00..=0xFEFF => {
                let c = (v & 0xFF) as u8 as char;
                if !c.is_ascii_alphabetic() {
                    continue;
                }
                KeyAction::Input(Input::Func(Func::AltLetter(c.to_ascii_uppercase())))
            }
            v @ 0xFD00..=0xFDFF => {
                let n = (v & 0xFF) as u8;
                let name = format!("KEY MACRO {n} ({label})");
                if let Some(d) = def {
                    kb.original.insert(name.clone(), d);
                }
                KeyAction::Macro { name: name.clone(), body: kb.macros.get(&n).cloned().unwrap_or_default() }
            }
            _ => continue,
        };
        let id = match &action {
            KeyAction::Macro { name, .. } => name.clone(),
            KeyAction::Input(_) => label,
        };
        by_off.insert(e.off, id);
        kb.map.insert(key, action);
    }

    // packet 3: key descriptions (45-byte records)
    if let Some(p) = f.packet(3) {
        for r in p.chunks_exact(45) {
            let off = u16::from_le_bytes([r[2], r[3]]) as usize;
            let len = (r[4] as usize).min(40);
            let text = crate::wpm::decode_wp_string(&r[5..5 + len]);
            if let Some(id) = by_off.get(&off) {
                kb.descriptions.insert(id.clone(), text);
            }
        }
    }
    Ok(kb)
}

// ------------------------------------------------- editing and writing

/// One redefined key, as edited in Setup → Keyboard Layout → Edit.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyDef {
    pub key: KeyIn,
    /// The action: one word (character, key, `{ALT x}` …) or a whole macro.
    pub words: Vec<u16>,
    pub desc: String,
}

impl KeyDef {
    /// Can the action be stored directly in the key table?
    fn simple(&self) -> Option<u16> {
        match self.words.as_slice() {
            [] => Some(0),
            &[w @ (0x0020..=0x007E | 0x0100..=0x0CFF | 0x8000..=0x80FF | 0xFC2C | 0xFE00..=0xFEFF | 0xFF00..=0xFF09)] => Some(w),
            _ => None,
        }
    }
}

/// The action word the original keyboard gives `key`.
pub fn default_word(key: &KeyIn) -> Option<u16> {
    table_entries(&default_table()).into_iter().find(|e| slot_key(e.slot).as_ref() == Some(key)).map(|e| e.value)
}

/// Read the redefined keys of a .WPK file (for editing).
pub fn read_defs(d: &[u8]) -> Result<Vec<KeyDef>, String> {
    let f = crate::wpm::parse_wpc(d)?;
    if f.ftype != 3 {
        return Err(format!("not a keyboard file (type {})", f.ftype));
    }
    let table = f.packet(1).ok_or("keyboard file has no key table")?;
    let mut macros: HashMap<u8, Vec<u16>> = HashMap::new();
    if let Some(p) = f.packet(2) {
        let count = p.first().copied().unwrap_or(0);
        let mut i = 1;
        for _ in 0..count {
            let (Some(&n), Some(len)) = (p.get(i), rd16(p, i + 1)) else { break };
            let end = (i + 3 + len as usize).min(p.len());
            macros.insert(n, crate::wpm::words_of(&p[(i + 3).min(end)..end]));
            i = end;
        }
    }
    let mut descs: HashMap<usize, String> = HashMap::new();
    if let Some(p) = f.packet(3) {
        for r in p.chunks_exact(45) {
            let off = u16::from_le_bytes([r[2], r[3]]) as usize;
            let len = (r[4] as usize).min(40);
            descs.insert(off, crate::wpm::decode_wp_string(&r[5..5 + len]));
        }
    }
    let defaults: HashMap<Slot, u16> = table_entries(&default_table()).into_iter().map(|e| (e.slot, e.value)).collect();
    let mut out: Vec<KeyDef> = Vec::new();
    for e in table_entries(table) {
        let Some(key) = slot_key(e.slot) else { continue };
        if defaults.get(&e.slot) == Some(&e.value) || e.value == 0xFFFF || e.value == 0x7FFF || out.iter().any(|d| d.key == key) {
            continue;
        }
        let words = match e.value {
            0 => vec![],
            v @ 0xFD00..=0xFDFF => macros.get(&((v & 0xFF) as u8)).cloned().unwrap_or_default(),
            v => vec![v],
        };
        out.push(KeyDef { key, words, desc: descs.get(&e.off).cloned().unwrap_or_default() });
    }
    Ok(out)
}

/// Write a .WPK keyboard file with these redefined keys.
pub fn encode(defs: &[KeyDef]) -> Vec<u8> {
    let mut table = default_table();
    let entries = table_entries(&default_table());
    let mut macros: Vec<(u8, Vec<u16>)> = Vec::new();
    let mut descs: Vec<(Slot, usize, String)> = Vec::new();
    for d in defs {
        let value = match d.simple() {
            Some(v) => v,
            None => {
                let n = macros.len() as u8 + 1;
                macros.push((n, d.words.clone()));
                0xFD00 | n as u16
            }
        };
        let mut first = None;
        for e in entries.iter().filter(|e| slot_key(e.slot).as_ref() == Some(&d.key)) {
            table[e.off..e.off + 2].copy_from_slice(&value.to_le_bytes());
            first.get_or_insert((e.slot, e.off));
        }
        if let Some((slot, off)) = first {
            if !d.desc.is_empty() {
                descs.push((slot, off, d.desc.clone()));
            }
        }
    }
    let mut p2 = Vec::new();
    if !macros.is_empty() {
        p2.push(macros.len() as u8);
        for (n, w) in &macros {
            let mut b: Vec<u8> = w.iter().flat_map(|x| x.to_le_bytes()).collect();
            b.extend_from_slice(&[0, 0]);
            p2.push(*n);
            p2.extend_from_slice(&(b.len() as u16).to_le_bytes());
            p2.extend(b);
        }
    }
    let mut p3 = Vec::new();
    for (slot, off, text) in &descs {
        let (a, sc) = match *slot {
            Slot::Ext(s) => (0, s),
            Slot::Ascii(c) => (c, 0),
            Slot::Pair(_, a, s) => (a, s),
        };
        let mut rec = vec![a, sc];
        rec.extend_from_slice(&(*off as u16).to_le_bytes());
        let mut t = crate::wpm::encode_wp_string(text, 39);
        t.push(0);
        rec.push(t.len() as u8);
        t.resize(40, 0);
        rec.extend(t);
        p3.extend(rec);
    }
    let packets: Vec<(u16, &[u8])> = vec![(1, &table), (2, &p2), (3, &p3)];
    let header = 16 + 10 * 5;
    let total = header + packets.iter().map(|p| p.1.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"\xFFWPC");
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&[1, 3, 1, 1, 0, 0, 0, 0]);
    out.extend_from_slice(&[0xFB, 0xFF, 5, 0, 50, 0, 0, 0, 0, 0]);
    let mut off = header;
    for (t, p) in &packets {
        if p.is_empty() {
            out.extend_from_slice(&[0u8; 10]);
            continue;
        }
        out.extend_from_slice(&t.to_le_bytes());
        out.extend_from_slice(&(p.len() as u32).to_le_bytes());
        out.extend_from_slice(&(off as u32).to_le_bytes());
        off += p.len();
    }
    out.extend_from_slice(&[0u8; 10]);
    for (_, p) in &packets {
        out.extend_from_slice(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn her_dir() -> Option<PathBuf> {
        ["I:\\Claude Space\\wp51", "/mnt/i/Claude Space/wp51"].iter().map(PathBuf::from).find(|p| p.is_dir())
    }

    fn key(k: K) -> KeyIn {
        KeyIn { k, m: Mods::default() }
    }

    #[test]
    fn default_table_is_identity() {
        let d = default_table();
        assert_eq!(d.len(), 718);
        // every real key slot of the default table maps to a KeyIn
        let n = table_entries(&d).iter().filter(|e| slot_key(e.slot).is_some()).count();
        assert!(n > 200, "{n}");
    }

    #[test]
    fn wpk_wpm_all_her_keyboards() {
        let Some(dir) = her_dir() else { return };
        let mut n = 0;
        for sub in ["", "WPM"] {
            for e in std::fs::read_dir(dir.join(sub)).unwrap().flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wpk")) {
                    let kb = load(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                    n += 1;
                    for a in kb.map.values() {
                        if let KeyAction::Macro { name, body } = a {
                            if name.starts_with("KEY MACRO") {
                                assert!(!body.is_empty(), "{}: {name}", p.display());
                            }
                        }
                    }
                }
            }
        }
        assert!(n >= 8, "{n}");

        let alt = load(&dir.join("WPM").join("ALTRNAT.WPK")).unwrap();
        let f = |k: KeyIn| match alt.map.get(&k) {
            Some(KeyAction::Input(Input::Func(f))) => Some(*f),
            _ => None,
        };
        assert_eq!(f(key(K::F(1))), Some(Func::Help));
        assert_eq!(f(key(K::F(3))), Some(Func::Esc));
        assert_eq!(f(key(K::Esc)), Some(Func::Cancel));
        assert!(!alt.map.contains_key(&key(K::F(2))));
        assert!(!alt.map.contains_key(&key(K::Char('a'))));
        assert_eq!(alt.map.len(), 3, "{:?}", alt.map.keys().collect::<Vec<_>>());
        assert_eq!(alt.descriptions.get("F1").map(|s| s.as_str()), Some("Help"));

        let us = load(&dir.join("WPM").join("US.WPK")).unwrap();
        assert!(us.map.is_empty());

        // DE_DOS: Alt-Q types '@' (German AltGr), Alt-M types µ
        let de = load(&dir.join("WPM").join("DE_DOS.WPK")).or_else(|_| load(&dir.join("DE_DOS.WPK"))).unwrap();
        let altk = |c| KeyIn { k: K::Char(c), m: Mods { alt: true, ..Default::default() } };
        assert!(matches!(de.map.get(&altk('q')), Some(KeyAction::Input(Input::Char('@')))));
        assert!(matches!(de.map.get(&altk('m')), Some(KeyAction::Input(Input::Char('µ')))));

        let m = load(&dir.join("WPM").join("MACROS.WPK")).unwrap();
        assert_eq!(m.macros.len(), 18);
        assert!(m.map.values().any(|a| matches!(a, KeyAction::Macro { .. })));
        load(&dir.join("SHORTCUT.WPK")).unwrap();

        // every keyboard survives read_defs → encode → read_defs
        for name in ["ALTRNAT.WPK", "MACROS.WPK", "DE_DOS.WPK"] {
            let p = dir.join("WPM").join(name);
            let Ok(d) = std::fs::read(&p) else { continue };
            let defs = read_defs(&d).unwrap();
            let again = read_defs(&encode(&defs)).unwrap();
            assert_eq!(defs.len(), again.len(), "{name}");
            for (a, b) in defs.iter().zip(&again) {
                assert_eq!((a.key, &a.words), (b.key, &b.words), "{name}");
            }
        }
    }

    #[test]
    fn encode_roundtrip() {
        let defs = vec![
            KeyDef { key: key(K::F(1)), words: vec![0x8022], desc: "Help".into() },
            KeyDef { key: KeyIn { k: K::Char('q'), m: Mods { alt: true, ..Default::default() } }, words: vec![b'@' as u16], desc: String::new() },
            KeyDef { key: KeyIn { k: K::Char('k'), m: Mods { ctrl: true, ..Default::default() } }, words: vec![0xFC0C, b'h' as u16, b'i' as u16], desc: "greeting".into() },
        ];
        let bytes = encode(&defs);
        let kb = parse(&bytes, "T.WPK").unwrap();
        assert!(matches!(kb.map.get(&key(K::F(1))), Some(KeyAction::Input(Input::Func(Func::Help)))));
        assert_eq!(kb.descriptions.get("F1").map(|s| s.as_str()), Some("Help"));
        let back = read_defs(&bytes).unwrap();
        assert_eq!(back.len(), 3);
        assert!(back.iter().any(|d| d.words == vec![0xFC0C, b'h' as u16, b'i' as u16] && d.desc == "greeting"));
    }
}
