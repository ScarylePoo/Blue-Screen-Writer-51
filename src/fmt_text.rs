//! Plain-text encodings: decoding tables and automatic detection of
//! UTF-8 / UTF-16 (with or without BOM) and the 8-bit code pages a
//! German/Russian translator meets: Windows-1252, Windows-1251,
//! Windows-1250, DOS 850/437, DOS 866, KOI8-R and Mac Roman.
//!
//! Detection decodes the text with every candidate and scores how
//! plausible the non-ASCII characters are: letters inside words of a
//! single script (Latin or Cyrillic) with a sensible upper/lower case
//! pattern score high; control characters, box drawing inside words and
//! words that mix scripts score low.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enc {
    Utf8,
    Utf16Le,
    Utf16Be,
    Cp1252,
    Cp1251,
    Cp1250,
    Cp850,
    Cp437,
    Cp866,
    Koi8r,
    MacRoman,
}

const CP866_80: &str = "АБВГДЕЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдежзийклмноп░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀рстуфхцчшщъыьэюяЁёЄєЇїЎў°∙·√№¤■\u{a0}";
const KOI8R_80: &str = "─│┌┐└┘├┤┬┴┼▀▄█▌▐░▒▓⌠■∙√≈≤≥\u{a0}⌡°²·÷═║╒ё╓╔╕╖╗╘╙╚╛╜╝╞╟╠╡Ё╢╣╤╥╦╧╨╩╪╫╬©юабцдефгхийклмнопярстужвьызшэщчъЮАБЦДЕФГХИЙКЛМНОПЯРСТУЖВЬЫЗШЭЩЧЪ";
const MACROMAN_80: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»…\u{a0}ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{f8ff}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";

fn table(t: &str) -> Vec<char> {
    t.chars().collect()
}

/// Decode one byte of an 8-bit encoding (UTF encodings: Latin-1).
pub fn byte_char(b: u8, enc: Enc) -> char {
    if b < 0x80 {
        return b as char;
    }
    let i = (b - 0x80) as usize;
    let pick = |t: &str| t.chars().nth(i).unwrap_or('\u{fffd}');
    match enc {
        Enc::Cp1252 | Enc::Utf8 | Enc::Utf16Le | Enc::Utf16Be => crate::rtf::decode_byte(b, 1252),
        Enc::Cp1251 => crate::rtf::decode_byte(b, 1251),
        Enc::Cp1250 => crate::rtf::decode_byte(b, 1250),
        Enc::Cp850 => crate::rtf::decode_byte(b, 850),
        Enc::Cp437 => crate::rtf::decode_byte(b, 437),
        Enc::Cp866 => pick(CP866_80),
        Enc::Koi8r => pick(KOI8R_80),
        Enc::MacRoman => pick(MACROMAN_80),
    }
}

/// Decode bytes in a given encoding (no BOM handling).
pub fn decode(bytes: &[u8], enc: Enc) -> String {
    match enc {
        Enc::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Enc::Utf16Le | Enc::Utf16Be => {
            let u: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| if enc == Enc::Utf16Le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
                .collect();
            String::from_utf16_lossy(&u)
        }
        Enc::Cp1252 | Enc::Cp1251 | Enc::Cp1250 | Enc::Cp850 | Enc::Cp437 => {
            let cp = match enc {
                Enc::Cp1251 => 1251,
                Enc::Cp1250 => 1250,
                Enc::Cp850 => 850,
                Enc::Cp437 => 437,
                _ => 1252,
            };
            bytes.iter().map(|&b| crate::rtf::decode_byte(b, cp)).collect()
        }
        Enc::Cp866 | Enc::Koi8r | Enc::MacRoman => {
            let t = table(match enc {
                Enc::Cp866 => CP866_80,
                Enc::Koi8r => KOI8R_80,
                _ => MACROMAN_80,
            });
            bytes.iter().map(|&b| if b < 0x80 { b as char } else { t.get((b - 0x80) as usize).copied().unwrap_or('\u{fffd}') }).collect()
        }
    }
}

/// Windows code page / charset number → encoding (for formats that state
/// their code page).
pub fn enc_for_codepage(cp: u32) -> Option<Enc> {
    Some(match cp {
        1252 | 28591 => Enc::Cp1252,
        1251 => Enc::Cp1251,
        1250 => Enc::Cp1250,
        850 => Enc::Cp850,
        437 => Enc::Cp437,
        866 => Enc::Cp866,
        20866 => Enc::Koi8r,
        10000 => Enc::MacRoman,
        65001 => Enc::Utf8,
        1200 => Enc::Utf16Le,
        1201 => Enc::Utf16Be,
        _ => return None,
    })
}

/// Encoding from a charset label (HTML/XML `charset=`, `encoding=`).
pub fn enc_for_label(label: &str) -> Option<Enc> {
    let l = label.trim().trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase().replace('_', "-");
    Some(match l.as_str() {
        "utf-8" | "utf8" => Enc::Utf8,
        "utf-16" | "utf-16le" => Enc::Utf16Le,
        "utf-16be" => Enc::Utf16Be,
        "windows-1252" | "cp1252" | "iso-8859-1" | "latin1" | "latin-1" | "iso-8859-15" | "us-ascii" | "ascii" | "x-cp1252" => Enc::Cp1252,
        "windows-1251" | "cp1251" | "x-cp1251" => Enc::Cp1251,
        "windows-1250" | "cp1250" | "iso-8859-2" => Enc::Cp1250,
        "ibm850" | "cp850" | "850" => Enc::Cp850,
        "ibm437" | "cp437" | "437" => Enc::Cp437,
        "ibm866" | "cp866" | "866" => Enc::Cp866,
        "koi8-r" | "koi8r" | "koi8-u" => Enc::Koi8r,
        "macintosh" | "macroman" | "x-mac-roman" | "mac" => Enc::MacRoman,
        _ => return None,
    })
}

fn is_cyr(c: char) -> bool {
    ('\u{400}'..='\u{4ff}').contains(&c)
}

/// Plausibility of decoding `bytes` as `enc` (higher = better).
fn score(bytes: &[u8], enc: Enc) -> i64 {
    let s: Vec<char> = decode(bytes, enc).chars().collect();
    let high: Vec<bool> = bytes.iter().map(|&b| b >= 0x80).collect();
    let mut sc: i64 = 0;
    let n = s.len().min(high.len());
    let mut i = 0;
    while i < n {
        if !s[i].is_alphabetic() {
            if high[i] {
                let c = s[i];
                sc += match c {
                    '“' | '”' | '„' | '‘' | '’' | '–' | '—' | '…' | '«' | '»' | '°' | '§' | '€' | '•' | '\u{a0}' | '·' | '±' | '²' | '³' | '½' | '№' | '×' | '©' | '®' => 1,
                    '\u{80}'..='\u{9f}' | '\u{fffd}' => -6,
                    '\u{2500}'..='\u{259f}' => -1,
                    _ => -2,
                };
            }
            i += 1;
            continue;
        }
        // a word: maximal run of letters
        let st = i;
        while i < n && s[i].is_alphabetic() {
            i += 1;
        }
        let word = &s[st..i];
        let nhigh = (st..i).filter(|&k| high[k]).count() as i64;
        if nhigh == 0 {
            continue;
        }
        let cyr = word.iter().filter(|c| is_cyr(**c)).count();
        let latin = word.iter().filter(|c| c.is_ascii_alphabetic() || ('\u{c0}'..='\u{24f}').contains(*c)).count();
        let other = word.len() - cyr - latin;
        if (cyr > 0 && latin > 0) || other > 0 {
            sc -= 4 * nhigh;
            continue;
        }
        // Latin words consist mostly of ASCII letters with the odd accented
        // one; a long Latin "word" made of accented letters only is what
        // Cyrillic text looks like in a Western code page.
        if latin > 0 && word.len() >= 4 && nhigh * 2 > word.len() as i64 {
            sc -= 2 * nhigh;
            continue;
        }
        sc += 2 * nhigh;
        // case pattern: Upper after lower inside a word is unusual
        let mut bad_case = 0;
        for w in word.windows(2) {
            if w[0].is_lowercase() && w[1].is_uppercase() {
                bad_case += 1;
            }
        }
        let all_upper = word.len() > 1 && word.iter().all(|c| c.is_uppercase());
        if bad_case > 0 {
            sc -= 2 * bad_case;
        } else if !all_upper {
            // all-caps words are fine but less common than lower case
            sc += nhigh;
        }
    }
    sc
}

/// Looks like UTF-16 without BOM: the high bytes (one parity) are
/// dominated by one small value (0x00 for Latin, 0x04 for Cyrillic…) and
/// the low bytes contain (almost) no zeros.
fn utf16_guess(bytes: &[u8]) -> Option<Enc> {
    let n = bytes.len().min(4096) & !1;
    if n < 8 {
        return None;
    }
    let half = n / 2;
    let check = |hi_par: usize| -> bool {
        let mut counts = [0usize; 256];
        let mut lo_zero = 0;
        for k in 0..half {
            counts[bytes[2 * k + hi_par] as usize] += 1;
            if bytes[2 * k + 1 - hi_par] == 0 {
                lo_zero += 1;
            }
        }
        let (top, val) = counts.iter().enumerate().map(|(v, c)| (*c, v)).max().unwrap_or((0, 0));
        // at least one zero high byte (spaces, punctuation, line ends)
        top * 10 >= half * 6 && val < 0x30 && counts[0] > 0 && lo_zero * 20 < half
    };
    if check(1) {
        Some(Enc::Utf16Le)
    } else if check(0) {
        Some(Enc::Utf16Be)
    } else {
        None
    }
}

/// Detect the encoding of a text file.  Returns the encoding and the
/// length of a byte order mark to skip.
pub fn detect(bytes: &[u8]) -> (Enc, usize) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (Enc::Utf8, 3);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return (Enc::Utf16Le, 2);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return (Enc::Utf16Be, 2);
    }
    if let Some(e) = utf16_guess(bytes) {
        return (e, 0);
    }
    if !bytes.iter().any(|&b| b >= 0x80) {
        return (Enc::Utf8, 0);
    }
    // valid UTF-8 (allow a sequence cut off at the very end)
    match std::str::from_utf8(bytes) {
        Ok(_) => return (Enc::Utf8, 0),
        Err(e) if e.error_len().is_none() && e.valid_up_to() + 4 > bytes.len() => return (Enc::Utf8, 0),
        Err(_) => {}
    }
    (detect_8bit(bytes), 0)
}

/// Best 8-bit code page for `bytes`.
pub fn detect_8bit(bytes: &[u8]) -> Enc {
    // score a sample (the first 64 KB are plenty)
    let sample = &bytes[..bytes.len().min(65536)];
    // order = preference on ties
    let cands = [Enc::Cp1252, Enc::Cp1251, Enc::Cp850, Enc::Cp866, Enc::Koi8r, Enc::Cp1250, Enc::MacRoman];
    let mut best = (i64::MIN, Enc::Cp1252);
    for e in cands {
        let mut s = score(sample, e);
        // slight bias against the rarer code pages
        s -= match e {
            Enc::Cp1250 | Enc::MacRoman => 2,
            _ => 0,
        };
        if s > best.0 {
            best = (s, e);
        }
    }
    best.1
}

/// Decode a text file with automatic encoding detection.
pub fn decode_auto(bytes: &[u8]) -> String {
    let (enc, bom) = detect(bytes);
    if bom == 0 && !matches!(enc, Enc::Utf8 | Enc::Utf16Le | Enc::Utf16Be) {
        if let Some(s) = decode_cjk(bytes) {
            return s;
        }
    }
    decode(&bytes[bom..], enc)
}

/// Chinese/Japanese/Korean legacy encodings (GBK, Big5, Shift-JIS, EUC-JP,
/// EUC-KR), recognised with Firefox's detector.  Only a confident CJK
/// guess that decodes without errors is taken; everything else goes to the
/// 8-bit code page detection above.
pub fn decode_cjk(bytes: &[u8]) -> Option<String> {
    let sample = &bytes[..bytes.len().min(256 << 10)];
    let mut det = chardetng::EncodingDetector::new();
    det.feed(sample, sample.len() == bytes.len());
    let enc = det.guess(None, true);
    let cjk = [encoding_rs::GBK, encoding_rs::GB18030, encoding_rs::BIG5, encoding_rs::SHIFT_JIS, encoding_rs::EUC_JP, encoding_rs::EUC_KR, encoding_rs::ISO_2022_JP];
    if !cjk.contains(&enc) {
        return None;
    }
    let (text, had_errors) = enc.decode_without_bom_handling(bytes);
    // the result must really be mostly CJK, not Latin text read as Han
    let cjk_chars = text.chars().filter(|&c| crate::cjk::is_cjk(c)).count();
    let high = text.chars().filter(|&c| !c.is_ascii()).count();
    if had_errors || cjk_chars * 10 < high * 8 {
        return None;
    }
    Some(text.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_1251(s: &str) -> Vec<u8> {
        s.chars()
            .map(|c| match c {
                'А'..='я' => (c as u32 - 0x410 + 0xC0) as u8,
                'Ё' => 0xA8,
                'ё' => 0xB8,
                c => c as u8,
            })
            .collect()
    }
    fn enc_table(s: &str, t: &str) -> Vec<u8> {
        let t: Vec<char> = t.chars().collect();
        s.chars().map(|c| if (c as u32) < 0x80 { c as u8 } else { 0x80 + t.iter().position(|x| *x == c).unwrap() as u8 }).collect()
    }

    const RU: &str = "Привет, мир! Это проверка русского текста. Ёлка.";
    const DE: &str = "Grüße aus Köln: Äpfel, Öl und Übermaß für die Bäckerei.";

    #[test]
    fn tables_complete() {
        for t in [CP866_80, KOI8R_80, MACROMAN_80] {
            assert_eq!(t.chars().count(), 128);
        }
    }

    #[test]
    fn russian_code_pages() {
        assert_eq!(detect(&enc_1251(RU)).0, Enc::Cp1251);
        assert_eq!(decode_auto(&enc_1251(RU)), RU);
        assert_eq!(decode_auto(&enc_table(RU, CP866_80)), RU);
        assert_eq!(decode_auto(&enc_table(RU, KOI8R_80)), RU);
        let short = "Привет мир и друзья! Это тест.";
        assert_eq!(decode_auto(&enc_1251(short)), short);
        assert_eq!(decode_auto(&enc_table(short, CP866_80)), short);
        assert_eq!(decode_auto(&enc_table(short, KOI8R_80)), short);
    }

    #[test]
    fn german_code_pages() {
        let latin1: Vec<u8> = DE.chars().map(|c| c as u32 as u8).collect();
        assert_eq!(decode_auto(&latin1), DE);
        let cp850: Vec<u8> = DE.chars().map(|c| if (c as u32) < 0x80 { c as u8 } else { 0x80 + crate::fileio::CP850_HIGH.chars().position(|x| x == c).unwrap() as u8 }).collect();
        assert_eq!(decode_auto(&cp850), DE);
        let fr: Vec<u8> = "Été à Noël: déjà vu, ça va très bien.".chars().map(|c| c as u32 as u8).collect();
        assert_eq!(detect(&fr).0, Enc::Cp1252);
        let mac = enc_table(DE, MACROMAN_80);
        assert_eq!(decode(&mac, Enc::MacRoman), DE);
    }

    #[test]
    fn utf() {
        assert_eq!(decode_auto(DE.as_bytes()), DE);
        let mut u16le = vec![];
        for u in RU.encode_utf16() {
            u16le.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_auto(&u16le), RU);
        let mut u16be = vec![0xFE, 0xFF];
        for u in DE.encode_utf16() {
            u16be.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(decode_auto(&u16be), DE);
    }

    #[test]
    fn labels() {
        assert_eq!(enc_for_label("\"Windows-1251\""), Some(Enc::Cp1251));
        assert_eq!(enc_for_label("koi8-r"), Some(Enc::Koi8r));
    }
}

#[cfg(test)]
mod cjk_tests {
    #[test]
    fn legacy_cjk_encodings() {
        let zh = "中文测试：这是一个用于测试的段落。我们需要足够的文字来让检测器做出判断，所以这里写得长一些。";
        let ja = "日本語のテストです。これは文字コードの判定のための文章で、ひらがなとカタカナと漢字を含みます。";
        let ko = "한국어 문장입니다. 문자 인코딩 감지를 위해 충분히 긴 문장을 씁니다. 단어 사이에 공백이 있습니다.";
        for (text, enc) in [(zh, encoding_rs::GBK), (zh, encoding_rs::BIG5), (ja, encoding_rs::SHIFT_JIS), (ja, encoding_rs::EUC_JP), (ko, encoding_rs::EUC_KR)] {
            let (bytes, _, bad) = enc.encode(text);
            if bad {
                continue; // simplified characters missing in Big5
            }
            assert_eq!(super::decode_auto(&bytes), text, "{}", enc.name());
        }
        // German Windows-1252 must stay German
        let de: Vec<u8> = "Grüße aus Köln, schöne Übersetzung für Müller.".chars().map(|c| c as u32 as u8).collect();
        assert_eq!(super::decode_auto(&de), "Grüße aus Köln, schöne Übersetzung für Müller.");
    }
}
