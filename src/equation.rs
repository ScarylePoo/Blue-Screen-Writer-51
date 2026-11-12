//! A small renderer for WP 5.1 equation text (Equation boxes): words,
//! numbers and symbols, `{…}` groups, `OVER` fractions, `SUP`/`^` and
//! `SUB`/`_` scripts, `SQRT`/`NROOT`, Greek letter names and the common
//! operator names.  Output: positioned text runs and rules relative to an
//! origin on the baseline.

use crate::doc::{Attr, AttrSet, FontSpec};
use crate::layout::Metrics;
use crate::page::Run;

pub const EQ_FONT: &str = "Times New Roman";

#[derive(Debug, Clone)]
enum N {
    Text(String, bool),
    Row(Vec<N>),
    Frac(Box<N>, Box<N>),
    Script(Box<N>, Option<Box<N>>, Option<Box<N>>),
    Sqrt(Box<N>, Option<Box<N>>),
    /// space in 1/18 em
    Space(i32),
    Big(char),
}

fn tokens(s: &str) -> Vec<String> {
    let mut out = vec![];
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_alphabetic() {
            let st = i;
            while i < cs.len() && cs[i].is_alphabetic() {
                i += 1;
            }
            out.push(cs[st..i].iter().collect());
        } else if c.is_ascii_digit() || c == '.' {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            out.push(cs[st..i].iter().collect());
        } else {
            let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
            if ["<=", ">=", "!=", "->", "<>", "+-", "-+"].contains(&two.as_str()) {
                out.push(two);
                i += 2;
            } else {
                out.push(c.to_string());
                i += 1;
            }
        }
    }
    out
}

fn greek(w: &str) -> Option<char> {
    const L: [(&str, char); 24] = [
        ("alpha", 'α'), ("beta", 'β'), ("gamma", 'γ'), ("delta", 'δ'), ("epsilon", 'ε'), ("zeta", 'ζ'), ("eta", 'η'), ("theta", 'θ'),
        ("iota", 'ι'), ("kappa", 'κ'), ("lambda", 'λ'), ("mu", 'μ'), ("nu", 'ν'), ("xi", 'ξ'), ("omicron", 'ο'), ("pi", 'π'),
        ("rho", 'ρ'), ("sigma", 'σ'), ("tau", 'τ'), ("upsilon", 'υ'), ("phi", 'φ'), ("chi", 'χ'), ("psi", 'ψ'), ("omega", 'ω'),
    ];
    let lw = w.to_lowercase();
    let (_, c) = L.iter().find(|(n, _)| *n == lw)?;
    // ALPHA / Alpha → capital
    if w.chars().next()?.is_uppercase() {
        return c.to_uppercase().next();
    }
    Some(*c)
}

fn symbol(w: &str) -> Option<&'static str> {
    Some(match w.to_lowercase().as_str() {
        "inf" | "infinity" => "∞",
        "times" => "×",
        "cdot" => "·",
        "div" => "÷",
        "pm" | "plusminus" => "±",
        "mp" | "minusplus" => "∓",
        "partial" => "∂",
        "nabla" | "grad" => "∇",
        "approx" => "≈",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "equiv" => "≡",
        "deg" | "degree" => "°",
        "rightarrow" | "to" => "→",
        "leftarrow" => "←",
        "in" => "∈",
        "dots" | "ldots" | "cdots" => "…",
        "<=" => "≤",
        ">=" => "≥",
        "!=" | "<>" => "≠",
        "->" => "→",
        "+-" => "±",
        "-+" => "∓",
        _ => return None,
    })
}

fn big(w: &str) -> Option<char> {
    Some(match w.to_lowercase().as_str() {
        "int" | "integral" => '∫',
        "oint" => '∮',
        "sum" => '∑',
        "prod" | "product" => '∏',
        _ => return None,
    })
}

const FUNCS: [&str; 18] = ["sin", "cos", "tan", "cot", "sec", "csc", "log", "ln", "exp", "lim", "max", "min", "det", "sinh", "cosh", "tanh", "arcsin", "arccos"];

struct P {
    t: Vec<String>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&str> {
        self.t.get(self.i).map(|s| s.as_str())
    }
    fn is(&self, k: &str) -> bool {
        self.peek().is_some_and(|t| t.eq_ignore_ascii_case(k))
    }

    fn row(&mut self, braced: bool) -> N {
        let mut items: Vec<N> = vec![];
        while let Some(t) = self.peek() {
            if t == "}" {
                if braced {
                    self.i += 1;
                    break;
                }
                self.i += 1;
                continue;
            }
            if self.is("over") {
                self.i += 1;
                let num = items.pop().unwrap_or(N::Row(vec![]));
                let den = self.scripted();
                items.push(N::Frac(Box::new(num), Box::new(den)));
                continue;
            }
            items.push(self.scripted());
        }
        N::Row(items)
    }

    fn scripted(&mut self) -> N {
        let base = self.atom();
        let (mut sup, mut sub) = (None, None);
        loop {
            if self.is("sup") || self.is("^") || self.is("to") {
                self.i += 1;
                sup = Some(Box::new(self.atom()));
            } else if self.is("sub") || self.is("_") || self.is("from") {
                self.i += 1;
                sub = Some(Box::new(self.atom()));
            } else {
                break;
            }
        }
        if sup.is_none() && sub.is_none() { base } else { N::Script(Box::new(base), sup, sub) }
    }

    fn atom(&mut self) -> N {
        let Some(t) = self.peek().map(|s| s.to_string()) else { return N::Row(vec![]) };
        self.i += 1;
        let lw = t.to_lowercase();
        match lw.as_str() {
            "{" => self.row(true),
            "sqrt" => N::Sqrt(Box::new(self.atom()), None),
            "nroot" => {
                let n = self.atom();
                N::Sqrt(Box::new(self.atom()), Some(Box::new(n)))
            }
            "left" | "right" => match self.peek().map(|s| s.to_string()) {
                Some(d) => {
                    self.i += 1;
                    let d = match d.to_lowercase().as_str() {
                        "." => String::new(),
                        "lbrace" => "{".into(),
                        "rbrace" => "}".into(),
                        "line" => "|".into(),
                        _ => d,
                    };
                    N::Text(d, false)
                }
                None => N::Row(vec![]),
            },
            "~" => N::Space(6),
            "`" => N::Space(2),
            "bold" | "ital" | "func" | "italic" | "alignl" | "alignr" | "alignc" | "stack" | "matrix" | "horiz" | "vert" => self.atom(),
            "lbrace" => N::Text("{".into(), false),
            "rbrace" => N::Text("}".into(), false),
            _ => {
                if let Some(c) = big(&t) {
                    N::Big(c)
                } else if let Some(c) = greek(&t) {
                    N::Text(c.to_string(), false)
                } else if let Some(s) = symbol(&t).filter(|_| t.chars().any(|c| c.is_alphabetic()) || t.len() > 1) {
                    N::Row(vec![N::Space(3), N::Text(s.into(), false), N::Space(3)])
                } else if t.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    N::Text(t, false)
                } else if t.chars().all(char::is_alphabetic) {
                    N::Text(t.clone(), !FUNCS.contains(&lw.as_str()))
                } else if ["+", "-", "=", "<", ">"].contains(&t.as_str()) {
                    let s = if t == "-" { "−".to_string() } else { t };
                    N::Row(vec![N::Space(3), N::Text(s, false), N::Space(3)])
                } else {
                    N::Text(t, false)
                }
            }
        }
    }
}

enum Item {
    Text { x: i32, y: i32, s: String, size: u32, italic: bool },
    Rule { x: i32, y: i32, w: i32, h: i32 },
}

struct Bx {
    w: i32,
    asc: i32,
    desc: i32,
    items: Vec<Item>,
}

impl Bx {
    fn shift(mut self, dx: i32, dy: i32) -> Vec<Item> {
        for it in &mut self.items {
            match it {
                Item::Text { x, y, .. } | Item::Rule { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
            }
        }
        self.items
    }
}

fn em(size: u32) -> i32 {
    (size as i64 * 12 / 72) as i32
}

fn text_w(m: &dyn Metrics, s: &str, size: u32, italic: bool) -> i32 {
    let f = FontSpec { name: EQ_FONT.into(), size_cpt: size };
    let w: i32 = s.chars().map(|c| m.width(&f, size, c)).sum();
    if italic { w + em(size) / 12 } else { w }
}

fn lay(n: &N, size: u32, m: &dyn Metrics) -> Bx {
    let e = em(size);
    match n {
        N::Text(s, it) => Bx { w: text_w(m, s, size, *it), asc: e * 3 / 4, desc: e / 4, items: vec![Item::Text { x: 0, y: 0, s: s.clone(), size, italic: *it }] },
        N::Space(k) => Bx { w: e * k / 18, asc: 0, desc: 0, items: vec![] },
        N::Big(c) => {
            let bs = size * 3 / 2;
            let be = em(bs);
            Bx { w: text_w(m, &c.to_string(), bs, false) + e / 8, asc: be * 3 / 4, desc: be / 3, items: vec![Item::Text { x: 0, y: be / 8, s: c.to_string(), size: bs, italic: false }] }
        }
        N::Row(v) => {
            let mut b = Bx { w: 0, asc: e * 3 / 4, desc: e / 4, items: vec![] };
            for c in v {
                let cb = lay(c, size, m);
                let (w, asc, desc) = (cb.w, cb.asc, cb.desc);
                b.items.extend(cb.shift(b.w, 0));
                b.w += w;
                b.asc = b.asc.max(asc);
                b.desc = b.desc.max(desc);
            }
            b
        }
        N::Frac(a, d) => {
            let fs = (size * 85 / 100).max(600);
            let (na, nd) = (lay(a, fs, m), lay(d, fs, m));
            let w = na.w.max(nd.w) + e / 3;
            let axis = e / 4;
            let gap = e / 8;
            let th = (e / 20).max(8);
            let (naw, nasc, ndesc) = (na.w, na.asc, na.desc);
            let (daw, dasc, ddesc) = (nd.w, nd.asc, nd.desc);
            let mut items = na.shift((w - naw) / 2, -axis - gap - ndesc);
            items.extend(nd.shift((w - daw) / 2, -axis + gap + dasc + th));
            items.push(Item::Rule { x: e / 12, y: -axis, w: w - e / 6, h: th });
            Bx { w, asc: axis + gap + nasc + ndesc, desc: (gap + dasc + ddesc + th - axis).max(0), items }
        }
        N::Script(base, sup, sub) => {
            let b = lay(base, size, m);
            let ss = (size * 70 / 100).max(500);
            let (bw, basc, bdesc) = (b.w, b.asc, b.desc);
            let mut items = b.shift(0, 0);
            let (mut w, mut asc, mut desc) = (bw, basc, bdesc);
            let mut sw = 0;
            if let Some(s) = sup {
                let sb = lay(s, ss, m);
                let up = (basc - sb.asc / 2).max(e * 2 / 5);
                asc = asc.max(up + sb.asc);
                sw = sw.max(sb.w);
                items.extend(sb.shift(bw, -up));
            }
            if let Some(s) = sub {
                let sb = lay(s, ss, m);
                let down = (bdesc + sb.asc / 3).max(e / 5);
                desc = desc.max(down + sb.desc);
                sw = sw.max(sb.w);
                items.extend(sb.shift(bw, down));
            }
            w += sw;
            Bx { w, asc, desc, items }
        }
        N::Sqrt(x, n) => {
            let r = lay(x, size, m);
            let th = (e / 20).max(8);
            let h = r.asc + r.desc + e / 8;
            let rs = ((size as i64 * h as i64 / e.max(1) as i64) as u32).max(size);
            let rw = text_w(m, "√", rs, false);
            let mut items = vec![];
            let mut x0 = 0;
            let mut asc = r.asc + e / 8 + th;
            if let Some(n) = n {
                let nb = lay(n, (size * 60 / 100).max(500), m);
                x0 = nb.w;
                asc = asc.max(r.asc / 2 + nb.asc + nb.desc);
                let up = r.asc / 2 + nb.desc;
                items.extend(nb.shift(0, -up));
            }
            let (rwid, rasc, rdesc) = (r.w, r.asc, r.desc);
            items.push(Item::Text { x: x0, y: rdesc, s: "√".into(), size: rs, italic: false });
            items.extend(r.shift(x0 + rw, 0));
            items.push(Item::Rule { x: x0 + rw - th, y: -(rasc + e / 8) - th / 2, w: rwid + th, h: th });
            Bx { w: x0 + rw + rwid + e / 12, asc, desc: rdesc, items }
        }
    }
}

/// Laid-out equation relative to (0, baseline).
pub struct Eq {
    pub w: i32,
    pub asc: i32,
    pub desc: i32,
    pub runs: Vec<Run>,
    pub rules: Vec<(i32, i32, i32, i32)>,
}

/// Like `layout`, but smaller if needed to fit `width`.
pub fn layout_fit(src: &str, size_cpt: u32, width: i32, m: &dyn Metrics) -> Eq {
    let e = layout(src, size_cpt, m);
    if e.w <= width || e.w == 0 {
        return e;
    }
    let s = ((size_cpt as i64 * width as i64 / e.w as i64) as u32).max(400);
    layout(src, s, m)
}

pub fn layout(src: &str, size_cpt: u32, m: &dyn Metrics) -> Eq {
    let mut p = P { t: tokens(src), i: 0 };
    let n = p.row(false);
    let b = lay(&n, size_cpt, m);
    let mut runs = vec![];
    let mut rules = vec![];
    for it in b.items {
        match it {
            Item::Text { x, y, s, size, italic } => {
                if s.is_empty() {
                    continue;
                }
                let f = FontSpec { name: EQ_FONT.into(), size_cpt: size };
                let text: Vec<char> = s.chars().collect();
                let adv = text.iter().map(|c| m.width(&f, size, *c)).collect();
                let mut attrs = AttrSet::default();
                attrs.set(Attr::Italic, italic);
                runs.push(Run { x, y, text, adv, font: EQ_FONT.into(), size_cpt: size, attrs });
            }
            Item::Rule { x, y, w, h } => rules.push((x, y, w, h)),
        }
    }
    Eq { w: b.w, asc: b.asc, desc: b.desc, runs, rules }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::MonoMetrics;

    #[test]
    fn fraction_and_scripts() {
        let e = layout("x sup 2 + {a+b} over 2 - sqrt {alpha}", 1200, &MonoMetrics);
        let text: String = e.runs.iter().flat_map(|r| r.text.iter()).collect();
        assert!(text.contains('x') && text.contains('2') && text.contains('α') && text.contains('√'), "{text}");
        assert_eq!(e.rules.len(), 2, "fraction bar + root bar");
        // the superscript sits above the baseline
        let sup = e.runs.iter().find(|r| r.text == vec!['2']).unwrap();
        assert!(sup.y < 0);
        assert!(e.asc > 200 && e.w > 0);
    }

    #[test]
    fn greek_and_case() {
        assert_eq!(greek("alpha"), Some('α'));
        assert_eq!(greek("OMEGA"), Some('Ω'));
        assert_eq!(greek("Omega"), Some('Ω'));
        assert_eq!(greek("pi"), Some('π'));
        assert_eq!(greek("pig"), None);
    }
}
