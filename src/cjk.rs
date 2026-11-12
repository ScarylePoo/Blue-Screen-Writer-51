//! Chinese, Japanese and Korean text: double-width characters on the
//! character screen (UAX #11 East Asian Width), line breaking between
//! ideographs with the usual kinsoku rules (UAX #14, JIS X 4051), and the
//! Windows fonts used for CJK characters that the document font lacks.

/// Takes two screen columns (East Asian Wide / Fullwidth).
pub fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F       // Hangul Jamo initial consonants
        | 0x2E80..=0x303E     // CJK radicals, Kangxi, ideographic punctuation
        | 0x3041..=0x33FF     // kana, Bopomofo, Hangul compatibility jamo, CJK symbols
        | 0x3400..=0x4DBF     // CJK extension A
        | 0x4E00..=0x9FFF     // CJK unified ideographs
        | 0xA000..=0xA4CF     // Yi
        | 0xA960..=0xA97F     // Hangul Jamo extended A
        | 0xAC00..=0xD7A3     // Hangul syllables
        | 0xF900..=0xFAFF     // CJK compatibility ideographs
        | 0xFE10..=0xFE19     // vertical forms
        | 0xFE30..=0xFE6F     // CJK compatibility forms, small form variants
        | 0xFF00..=0xFF60     // fullwidth ASCII
        | 0xFFE0..=0xFFE6     // fullwidth signs
        | 0x1F300..=0x1F64F   // pictographs, emoticons
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x2FFFD   // CJK extensions B–F
        | 0x30000..=0x3FFFD)
}

/// Any CJK script character (wide or not, e.g. halfwidth katakana).
pub fn is_cjk(c: char) -> bool {
    is_wide(c) || matches!(c as u32, 0xFF61..=0xFFDC)
}

#[allow(dead_code)]
pub fn has_cjk(s: &str) -> bool {
    s.chars().any(is_cjk)
}

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7A3 | 0xD7B0..=0xD7FF)
}

fn is_kana(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F)
}

/// Text may break before or after this character without a space
/// (ideographs, kana, fullwidth forms).  Korean is written with spaces and
/// breaks at them, like Word does by default.
pub fn breaks_anywhere(c: char) -> bool {
    is_cjk(c) && !is_hangul(c)
}

/// Kinsoku: may not start a line (closing punctuation, small kana, …).
pub fn no_line_start(c: char) -> bool {
    matches!(
        c,
        '、' | '。' | '，' | '．' | '：' | '；' | '？' | '！' | '）' | '］' | '｝' | '〉' | '》' | '」' | '』' | '】' | '〕' | '〗' | '〙' | '〛' | '’' | '”' | '｣'
            | 'ゝ' | 'ゞ' | 'ヽ' | 'ヾ' | 'ー' | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ッ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ' | 'ヵ' | 'ヶ'
            | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ' | '々' | '〻' | '・' | '…' | '‥'
            | ')' | ']' | '}' | ',' | '.' | ':' | ';' | '?' | '!' | '%'
    )
}

/// Kinsoku: may not end a line (opening brackets).
pub fn no_line_end(c: char) -> bool {
    matches!(c, '（' | '［' | '｛' | '〈' | '《' | '「' | '『' | '【' | '〔' | '〖' | '〘' | '〚' | '‘' | '“' | '｢' | '(' | '[' | '{')
}

/// May a line break between `prev` and `c` (no space between them)?
pub fn break_between(prev: char, c: char) -> bool {
    (breaks_anywhere(prev) || breaks_anywhere(c)) && !no_line_start(c) && !no_line_end(prev) && !(prev.is_ascii_alphanumeric() && c.is_ascii_alphanumeric())
}

/// Is the paragraph around element `at` Japanese (does it contain kana)?
/// Han characters are then shown with Japanese fonts — their glyph shapes
/// differ from the Chinese ones.
pub fn para_japanese(els: &[crate::doc::El], at: usize) -> bool {
    use crate::doc::{Code, El};
    let brk = |e: &El| matches!(e, El::Code(Code::HRt | Code::HPg | Code::Cell | Code::Row));
    let at = at.min(els.len());
    let mut s = at;
    while s > 0 && at - s < 4000 && !brk(&els[s - 1]) {
        s -= 1;
    }
    els[s..].iter().take(8000).take_while(|e| !brk(e)).any(|e| matches!(e, El::Ch(c) if is_kana(*c)))
}

/// Windows font families for a CJK character, in order of preference
/// (`jp`: the paragraph is Japanese).
pub fn fallback_families(c: char, jp: bool) -> &'static [&'static str] {
    if is_hangul(c) {
        // Windows, then macOS, then Linux fonts
        &["Malgun Gothic", "Gulim", "Batang", "Apple SD Gothic Neo", "AppleGothic", "Noto Sans CJK KR", "Noto Sans KR", "NanumGothic", "UnDotum", "Microsoft YaHei", "Noto Sans CJK SC"]
    } else if is_kana(c) || jp {
        &["Yu Gothic", "MS Gothic", "Meiryo", "MS Mincho", "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Noto Sans CJK JP", "Noto Sans JP", "IPAGothic", "TakaoGothic", "VL Gothic", "Microsoft YaHei", "SimSun", "Noto Sans CJK SC"]
    } else {
        &["Microsoft YaHei", "SimSun", "MS Gothic", "Yu Gothic", "Microsoft JhengHei", "MingLiU", "Malgun Gothic", "PingFang SC", "Hiragino Sans GB", "STHeiti", "Heiti SC", "Noto Sans CJK SC", "Noto Sans SC", "WenQuanYi Micro Hei", "WenQuanYi Zen Hei", "AR PL UMing CN", "Droid Sans Fallback", "Noto Sans CJK JP"]
    }
}

/// The first installed family from `fallback_families` that has `c`.
pub fn fallback_family(c: char, jp: bool) -> Option<&'static str> {
    fallback_families(c, jp).iter().copied().find(|f| crate::fonts::has_glyph(f, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert!(is_wide('中') && is_wide('あ') && is_wide('한') && is_wide('。') && is_wide('Ａ'));
        assert!(!is_wide('a') && !is_wide('ä') && !is_wide('Ж') && !is_wide('ﾊ'));
        assert!(is_cjk('ﾊ'));
    }

    #[test]
    fn kinsoku() {
        assert!(break_between('中', '文'));
        assert!(!break_between('文', '。'), "no line may start with 。");
        assert!(!break_between('「', '中'), "no line may end with 「");
        assert!(break_between('文', 'A') && break_between('A', '文'));
        assert!(!break_between('한', '국'), "Korean breaks at spaces");
    }
}

#[cfg(test)]
mod layout_tests {
    use crate::doc::*;
    use crate::layout::{MonoMetrics, layout};

    #[test]
    fn chinese_wraps_between_characters() {
        // 6.5" line, mono metrics: a wide character is 2 columns (0.2")
        let text = "这是一个用于测试自动换行的较长的中文段落其中没有空格所以程序必须在汉字之间换行。".repeat(2);
        let d = Document::from_text(&text);
        let l = layout(&d, &MonoMetrics);
        assert!(l.lines.len() >= 3, "wraps");
        for ln in &l.lines {
            // no line starts with closing punctuation
            if let Some(c) = ln.cells.first() {
                assert!(!super::no_line_start(c.ch), "line starts with {}", c.ch);
            }
            // two columns per character
            for w in ln.cells.windows(2) {
                assert_eq!(w[1].col - w[0].col, 2);
            }
        }
        // the first line is full: 32 characters of 0.2" fit 6.5"
        assert_eq!(l.lines[0].cells.len(), 32);
        assert!(l.wide_cols > 0);
    }

    #[test]
    fn latin_lines_unchanged() {
        let l = layout(&Document::from_text("plain words only"), &MonoMetrics);
        assert_eq!(l.wide_cols, 0);
        assert_eq!(l.lines[0].cells[1].col - l.lines[0].cells[0].col, 1);
    }
}
