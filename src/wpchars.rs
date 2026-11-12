//! WordPerfect 5.x character sets (0–12) ↔ Unicode.
//! Data: docs/formats/wp5_charsets.tsv (compiled into the program).

use std::collections::HashMap;
use std::sync::OnceLock;

static TSV: &str = include_str!("../docs/formats/wp5_charsets.tsv");

struct Tables {
    to_uni: HashMap<(u8, u16), String>,
    from_uni: HashMap<char, (u8, u16)>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut to_uni = HashMap::new();
        let mut from_uni = HashMap::new();
        for line in TSV.lines() {
            if line.starts_with('#') || line.starts_with("set\t") {
                continue;
            }
            let mut f = line.split('\t');
            let (Some(set), Some(idx), Some(uni)) = (f.next(), f.next(), f.next()) else { continue };
            let (Ok(set), Ok(idx)) = (set.trim().parse::<u8>(), idx.trim().parse::<u16>()) else { continue };
            let s: String = uni
                .split_whitespace()
                .filter_map(|u| u.strip_prefix("U+").and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32))
                .collect();
            if s.is_empty() {
                continue;
            }
            let mut it = s.chars();
            if let (Some(c), None) = (it.next(), it.next()) {
                from_uni.entry(c).or_insert((set, idx));
            }
            to_uni.insert((set, idx), s);
        }
        Tables { to_uni, from_uni }
    })
}

/// WP character → Unicode text (may be more than one code point).
/// Unknown characters map to the Private Use Area so nothing is lost.
pub fn wp_to_string(set: u8, idx: u16) -> String {
    if set == 0 && (0x20..0x7F).contains(&idx) {
        return (idx as u8 as char).to_string();
    }
    match tables().to_uni.get(&(set, idx)) {
        Some(s) => s.clone(),
        None => char::from_u32(0xE000 + set as u32 * 256 + idx as u32).map(|c| c.to_string()).unwrap_or_default(),
    }
}

pub fn wp_to_unicode(set: u8, idx: u16) -> Option<char> {
    wp_to_string(set, idx).chars().next()
}

pub fn unicode_to_wp(c: char) -> Option<(u8, u16)> {
    if (' '..='~').contains(&c) {
        return Some((0, c as u16));
    }
    let u = c as u32;
    if (0xE000..0xE000 + 13 * 256).contains(&u) {
        let v = u - 0xE000;
        return Some(((v / 256) as u8, (v % 256) as u16));
    }
    tables().from_uni.get(&c).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn umlaut_and_cyrillic() {
        assert_eq!(wp_to_unicode(1, 31), Some('ä'));
        assert_eq!(unicode_to_wp('ä'), Some((1, 31)));
        // Cyrillic set 10 is present
        let c = wp_to_unicode(10, 0).unwrap();
        assert!(('\u{400}'..='\u{4ff}').contains(&c), "{c:?}");
    }
}
