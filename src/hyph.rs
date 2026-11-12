//! Hyphenation: Frank Liang's algorithm over the TeX/LibreOffice pattern
//! files in `assets/hyph/hyph_xx_YY.dic` (libhyphen format: first line the
//! encoding, then `LEFTHYPHENMIN n` etc., then one pattern per line).
//!
//! Also the Setup choice "Prompt for Hyphenation" (Never / When Required /
//! Always) that layout consults.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Patterns {
    pats: HashMap<String, Vec<u8>>,
    maxlen: usize,
    pub lhmin: usize,
    pub rhmin: usize,
}

impl Patterns {
    pub fn parse(text: &str) -> Patterns {
        let mut p = Patterns { pats: HashMap::new(), maxlen: 0, lhmin: 2, rhmin: 2 };
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if n == 0 || line.is_empty() || line.starts_with('%') || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or("");
            match first {
                "LEFTHYPHENMIN" => {
                    p.lhmin = words.next().and_then(|v| v.parse().ok()).unwrap_or(2).max(1);
                    continue;
                }
                "RIGHTHYPHENMIN" => {
                    p.rhmin = words.next().and_then(|v| v.parse().ok()).unwrap_or(2).max(1);
                    continue;
                }
                "NEXTLEVEL" | "NOHYPHEN" => continue,
                f if f.chars().all(|c| c.is_ascii_uppercase()) && f.len() > 3 => continue,
                _ => {}
            }
            // non-standard patterns "c1k/k=k,1,2": keep the standard part
            let pat = first.split('/').next().unwrap_or("");
            let mut letters = String::new();
            let mut digits: Vec<u8> = vec![0];
            for c in pat.chars() {
                if let Some(d) = c.to_digit(10) {
                    *digits.last_mut().unwrap() = d as u8;
                } else {
                    for lc in c.to_lowercase() {
                        letters.push(lc);
                    }
                    digits.push(0);
                }
            }
            let len = letters.chars().count();
            if len == 0 || digits.len() != len + 1 {
                continue;
            }
            p.maxlen = p.maxlen.max(len);
            let e = p.pats.entry(letters).or_insert_with(|| vec![0; len + 1]);
            for (a, b) in e.iter_mut().zip(digits) {
                *a = (*a).max(b);
            }
        }
        p
    }

    /// Allowed break positions in `word`: the number of characters before
    /// each break, ascending.
    pub fn points(&self, word: &str) -> Vec<usize> {
        let chars: Vec<char> = word.chars().flat_map(|c| c.to_lowercase().next()).collect();
        let n = chars.len();
        if n < self.lhmin + self.rhmin {
            return vec![];
        }
        let mut w: Vec<char> = Vec::with_capacity(n + 2);
        w.push('.');
        w.extend(&chars);
        w.push('.');
        let mut vals = vec![0u8; w.len() + 1];
        let mut key = String::new();
        for i in 0..w.len() {
            key.clear();
            for j in i..(i + self.maxlen).min(w.len()) {
                key.push(w[j]);
                if let Some(v) = self.pats.get(&key) {
                    for (k, d) in v.iter().enumerate() {
                        if vals[i + k] < *d {
                            vals[i + k] = *d;
                        }
                    }
                }
            }
        }
        // break after word char a (1-based count `a`) = position a + 1 in w
        (self.lhmin..=n.saturating_sub(self.rhmin)).filter(|&a| a > 0 && vals[a + 1] % 2 == 1).collect()
    }
}

/// Pattern file name for a WP language code.
pub fn file_for(lang: &str) -> Option<&'static str> {
    Some(match lang.to_ascii_uppercase().as_str() {
        "DE" | "SD" | "DA" => "hyph_de_DE.dic",
        "US" | "UK" | "OZ" | "EN" | "CA" => "hyph_en_US.dic",
        "IT" => "hyph_it_IT.dic",
        "FR" | "CF" => "hyph_fr_FR.dic",
        "ES" => "hyph_es_ES.dic",
        "RU" => "hyph_ru_RU.dic",
        _ => return None,
    })
}

fn find(name: &str) -> Option<PathBuf> {
    let mut dirs = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("hyph"));
            dirs.push(d.join("assets").join("hyph"));
            // macOS app bundle
            if let Some(c) = d.parent() {
                dirs.push(c.join("Resources").join("hyph"));
            }
        }
    }
    dirs.push(crate::settings::config_dir().join("hyph"));
    dirs.push(PathBuf::from("assets").join("hyph"));
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("hyph"));
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

fn decode(b: &[u8]) -> String {
    let first = b.split(|&c| c == b'\n').next().unwrap_or(&[]);
    let enc = String::from_utf8_lossy(first).trim().to_ascii_uppercase();
    if enc.starts_with("UTF") {
        String::from_utf8_lossy(b).into_owned()
    } else if enc.contains("KOI8") {
        // not shipped; best effort
        String::from_utf8_lossy(b).into_owned()
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

type Cache = Mutex<HashMap<String, Option<Arc<Patterns>>>>;
static CACHE: std::sync::LazyLock<Cache> = std::sync::LazyLock::new(Default::default);

/// Patterns for a WP language code (loaded once, cached).
pub fn patterns(lang: &str) -> Option<Arc<Patterns>> {
    let file = file_for(lang)?;
    let mut c = CACHE.lock().ok()?;
    if let Some(p) = c.get(file) {
        return p.clone();
    }
    let p = find(file).and_then(|p| std::fs::read(p).ok()).map(|b| Arc::new(Patterns::parse(&decode(&b))));
    c.insert(file.to_string(), p.clone());
    p
}

/// Hyphenation points of a word in a language.
pub fn hyphenate(lang: &str, word: &str) -> Vec<usize> {
    patterns(lang).map(|p| p.points(word)).unwrap_or_default()
}

/// Setup: Prompt for Hyphenation — 0 Never, 1 When Required, 2 Always.
pub static PROMPT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

pub fn prompt_mode() -> u8 {
    PROMPT.load(std::sync::atomic::Ordering::Relaxed)
}

pub const PROMPT_NAMES: [&str; 3] = ["Never", "When Required", "Always"];

fn prompt_file() -> PathBuf {
    crate::settings::config_dir().join("hyph_prompt.txt")
}

pub fn load_prompt_setting() {
    if let Ok(s) = std::fs::read_to_string(prompt_file()) {
        if let Ok(v) = s.trim().parse::<u8>() {
            PROMPT.store(v.min(2), std::sync::atomic::Ordering::Relaxed);
        }
    }
}

pub fn set_prompt_setting(v: u8) {
    PROMPT.store(v.min(2), std::sync::atomic::Ordering::Relaxed);
    let _ = std::fs::create_dir_all(crate::settings::config_dir());
    let _ = std::fs::write(prompt_file(), v.min(2).to_string());
}


// ============================================================ layout glue

use crate::doc::{Code, El};

/// The line being broken: margins and the left hyphenation zone.
pub struct Zone {
    pub lm: i32,
    pub rm: i32,
    /// `[HZone]` left, percent of the line width.
    pub left_pct: u8,
}

pub enum Decision {
    /// Break before element `e`, showing a hyphen.
    Break(usize),
    /// Ask the user (first letter, end of word, suggested break); wrap meanwhile.
    Ask(usize, usize, usize),
    Wrap,
}

fn is_word_code(c: &Code) -> bool {
    matches!(c, Code::AttrOn(_) | Code::AttrOff(_) | Code::Font(_) | Code::RevOn { .. } | Code::RevOff { .. })
}

/// A word overflows the right margin at element `i` (the character that
/// does not fit).  `ws` is where the word starts (after the last break
/// opportunity), `caret[k - line_start]` the position before element k
/// (k < i), `cur` the position before `i`.
#[allow(clippy::too_many_arguments)]
pub fn decide(els: &[El], line_start: usize, ws: usize, i: usize, caret: &[(i32, i32)], cur: i32, hw: i32, z: &Zone, lang: &str) -> Decision {
    let pos_at = |e: usize| if e < i { caret.get(e - line_start).map(|k| k.1).unwrap_or(cur) } else { cur };
    // [/] cancels hyphenation of this word
    if ws > 0 && matches!(els.get(ws - 1), Some(El::Code(Code::Fmt(crate::fmt::Fmt::CancelHyph)))) {
        return Decision::Wrap;
    }
    if els[ws..i].iter().any(|e| matches!(e, El::Code(Code::Fmt(crate::fmt::Fmt::CancelHyph)))) {
        return Decision::Wrap;
    }
    // the alphabetic core of the word
    let mut a = ws;
    while a < i && !matches!(els[a], El::Ch(c) if c.is_alphabetic()) {
        a += 1;
    }
    let mut letters: Vec<(usize, char)> = vec![];
    let mut k = a;
    while k < els.len() {
        match &els[k] {
            El::Ch(c) if c.is_alphabetic() => letters.push((k, *c)),
            El::Code(c) if is_word_code(c) => {}
            _ => break,
        }
        k += 1;
    }
    let we = k;
    if letters.len() < 4 || we <= i || a >= i {
        return Decision::Wrap;
    }
    // a word that starts inside the hyphenation zone simply wraps
    let zone_left = z.rm - (z.rm - z.lm) * z.left_pct as i32 / 100;
    if ws > line_start && pos_at(a) >= zone_left {
        return Decision::Wrap;
    }
    let fits = |idx: usize| {
        let e = letters[idx].0;
        e <= i && e > line_start && pos_at(e) + hw <= z.rm
    };
    let word: String = letters.iter().map(|x| x.1).collect();
    let pts = hyphenate(lang, &word);
    let best = pts.iter().rev().copied().find(|&p| p < letters.len() && fits(p));
    let mode = prompt_mode();
    let fit_any = (2..letters.len().saturating_sub(1)).rev().find(|&p| fits(p));
    match (mode, best) {
        (0, Some(p)) | (1, Some(p)) => Decision::Break(letters[p].0),
        (0, None) => Decision::Wrap,
        (_, b) => match b.or(fit_any) {
            Some(p) => Decision::Ask(a, we, letters[p].0),
            None => Decision::Wrap,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(lang: &str, w: &str) -> String {
        let pts = hyphenate(lang, w);
        let mut s = String::new();
        for (i, c) in w.chars().enumerate() {
            if pts.contains(&i) {
                s.push('-');
            }
            s.push(c);
        }
        s
    }

    #[test]
    fn liang_tiny() {
        let p = Patterns::parse("UTF-8\nLEFTHYPHENMIN 1\nRIGHTHYPHENMIN 1\n1ba\n");
        assert_eq!(p.points("abab"), vec![1]);
        assert_eq!(p.points("abba"), vec![2]);
    }

    #[test]
    fn german() {
        let s = show("DE", "Übersetzung");
        assert!(s.contains("Über-set-zung") || s.contains("Über-setzung") || s.contains("Ü-ber"), "{s}");
        assert_eq!(show("DE", "Silbentrennung"), "Sil-ben-tren-nung");
    }

    #[test]
    fn english_and_others() {
        assert_eq!(show("US", "hyphenation"), "hy-phen-ation");
        assert!(show("IT", "parola").contains('-'));
        assert!(show("FR", "dictionnaire").contains('-'));
        assert!(show("ES", "diccionario").contains('-'));
        assert!(show("RU", "словарь").contains('-'));
    }
}
