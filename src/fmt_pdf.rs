//! PDF text import: text in reading order, paragraphs, `[HPg]` between
//! pages.
//!
//! Own small PDF reader (no external PDF crate): object parser, classic
//! xref tables, xref streams, object streams, a rebuild-by-scanning
//! fallback for damaged files, Flate/LZW/ASCIIHex/ASCII85/RunLength
//! filters with PNG predictors.  A content-stream interpreter collects the
//! positioned glyphs (simple fonts with Standard/WinAnsi/MacRoman
//! encodings and /Differences, ToUnicode CMaps, Type0 CID fonts, Type3
//! fonts, Form XObjects); a layout pass groups them into lines, columns
//! and paragraphs.  Bold/italic come from the font names.
//!
//! Everything is bounds-checked and budgeted (recursion depth, operator
//! count, stream sizes): malformed input yields an error or partial text,
//! never a panic.

use crate::doc::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const MAX_STREAM: u64 = 64 << 20;
const MAX_OPS: usize = 4_000_000;
const MAX_GLYPHS: usize = 4_000_000;
const MAX_PAGES: usize = 20_000;

// ===========================================================================
// Objects

#[derive(Clone, Debug, PartialEq)]
enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Str(Vec<u8>),
    Name(Vec<u8>),
    Arr(Vec<Obj>),
    Dict(Dict),
    Stream(Dict, Rc<Vec<u8>>),
    Ref(u32),
    /// a bare keyword (content-stream operator, `obj`, `R`…)
    Kw(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Default)]
struct Dict(Vec<(Vec<u8>, Obj)>);

impl Dict {
    fn get(&self, k: &str) -> Option<&Obj> {
        self.0.iter().find(|(n, _)| n == k.as_bytes()).map(|(_, v)| v)
    }
}

impl Obj {
    fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }
    fn name(&self) -> Option<&[u8]> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
    fn dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }
}

// ===========================================================================
// Lexer / parser

fn is_ws(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}
fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

struct Lex<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Lex<'a> {
    fn new(b: &'a [u8], pos: usize) -> Lex<'a> {
        Lex { b, pos }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if is_ws(c) {
                self.pos += 1;
            } else if c == b'%' {
                while let Some(c) = self.peek() {
                    if c == b'\n' || c == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Next object (arrays/dicts parsed recursively).  `refs`: recognise
    /// `n g R`.  None at end of data.
    fn obj(&mut self, depth: u32, refs: bool) -> Option<Obj> {
        if depth > 64 {
            return None;
        }
        self.skip_ws();
        let c = self.peek()?;
        match c {
            b'/' => {
                self.pos += 1;
                let mut n = vec![];
                while let Some(c) = self.peek() {
                    if is_ws(c) || is_delim(c) {
                        break;
                    }
                    self.pos += 1;
                    if c == b'#' {
                        let h = self.b.get(self.pos..self.pos + 2).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                        if let Some(v) = h {
                            n.push(v);
                            self.pos += 2;
                            continue;
                        }
                    }
                    n.push(c);
                }
                Some(Obj::Name(n))
            }
            b'(' => {
                self.pos += 1;
                Some(Obj::Str(self.lit_string()))
            }
            b'<' => {
                if self.b.get(self.pos + 1) == Some(&b'<') {
                    self.pos += 2;
                    let mut d = Dict::default();
                    loop {
                        self.skip_ws();
                        match self.peek() {
                            None => break,
                            Some(b'>') => {
                                self.pos += 1;
                                if self.peek() == Some(b'>') {
                                    self.pos += 1;
                                }
                                break;
                            }
                            _ => {}
                        }
                        let k = self.obj(depth + 1, refs)?;
                        let Obj::Name(k) = k else {
                            if matches!(k, Obj::Kw(_)) {
                                // garbage: stop the dictionary at a keyword like `stream`
                                if let Obj::Kw(w) = &k {
                                    if w == b"stream" || w == b"endobj" {
                                        self.pos -= w.len();
                                        break;
                                    }
                                }
                            }
                            continue;
                        };
                        let v = self.obj(depth + 1, refs).unwrap_or(Obj::Null);
                        if let Obj::Kw(w) = &v {
                            if w == b"stream" || w == b"endobj" {
                                self.pos -= w.len();
                                break;
                            }
                        }
                        d.0.push((k, v));
                    }
                    Some(Obj::Dict(d))
                } else {
                    self.pos += 1;
                    let mut out = vec![];
                    let mut hi: Option<u8> = None;
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == b'>' {
                            break;
                        }
                        let v = match c {
                            b'0'..=b'9' => c - b'0',
                            b'a'..=b'f' => c - b'a' + 10,
                            b'A'..=b'F' => c - b'A' + 10,
                            _ => continue,
                        };
                        match hi.take() {
                            Some(h) => out.push(h << 4 | v),
                            None => hi = Some(v),
                        }
                    }
                    if let Some(h) = hi {
                        out.push(h << 4);
                    }
                    Some(Obj::Str(out))
                }
            }
            b'[' => {
                self.pos += 1;
                let mut a = vec![];
                loop {
                    self.skip_ws();
                    match self.peek() {
                        None => break,
                        Some(b']') => {
                            self.pos += 1;
                            break;
                        }
                        _ => {}
                    }
                    let start = self.pos;
                    match self.obj(depth + 1, refs) {
                        Some(o) => a.push(o),
                        None => break,
                    }
                    if self.pos == start {
                        self.pos += 1;
                    }
                    if a.len() > 1_000_000 {
                        break;
                    }
                }
                Some(Obj::Arr(a))
            }
            b'>' | b']' | b')' | b'{' | b'}' => {
                self.pos += 1;
                Some(Obj::Kw(vec![c]))
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let n = self.number();
                if refs && n >= 0.0 && n.fract() == 0.0 {
                    let save = self.pos;
                    self.skip_ws();
                    if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        let g = self.number();
                        self.skip_ws();
                        if g.fract() == 0.0 && self.peek() == Some(b'R') && self.b.get(self.pos + 1).is_none_or(|&c| is_ws(c) || is_delim(c)) {
                            self.pos += 1;
                            return Some(Obj::Ref(n.min(u32::MAX as f64) as u32));
                        }
                    }
                    self.pos = save;
                }
                Some(Obj::Num(n))
            }
            _ => {
                let s = self.pos;
                while let Some(c) = self.peek() {
                    if is_ws(c) || is_delim(c) {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == s {
                    self.pos += 1;
                }
                let w = &self.b[s..self.pos];
                Some(match w {
                    b"true" => Obj::Bool(true),
                    b"false" => Obj::Bool(false),
                    b"null" => Obj::Null,
                    _ => Obj::Kw(w.to_vec()),
                })
            }
        }
    }

    fn number(&mut self) -> f64 {
        let s = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let t = std::str::from_utf8(&self.b[s..self.pos]).unwrap_or("0");
        // tolerate "--5", "5-", "1.2.3"
        let t = t.trim_start_matches(['+']);
        let neg = t.starts_with('-');
        let t = t.trim_start_matches('-');
        let t: String = {
            let mut seen_dot = false;
            t.chars()
                .take_while(|c| {
                    if *c == '.' {
                        if seen_dot {
                            return false;
                        }
                        seen_dot = true;
                        true
                    } else {
                        c.is_ascii_digit()
                    }
                })
                .collect()
        };
        let v: f64 = if t.is_empty() || t == "." { 0.0 } else { t.parse().unwrap_or(0.0) };
        if neg { -v } else { v }
    }

    fn lit_string(&mut self) -> Vec<u8> {
        let mut out = vec![];
        let mut depth = 1;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                b'\\' => {
                    let Some(e) = self.peek() else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        _ => out.push(e),
                    }
                }
                _ => out.push(c),
            }
        }
        out
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).rposition(|w| w == needle)
}

// ===========================================================================
// Filters

fn decode_stream(d: &Dict, raw: &[u8], pdf: &Pdf) -> Option<Vec<u8>> {
    let filters: Vec<Vec<u8>> = match d.get("Filter").map(|f| pdf.resolve(f)) {
        Some(Obj::Name(n)) => vec![n],
        Some(Obj::Arr(a)) => a.iter().filter_map(|o| o.name().map(|n| n.to_vec())).collect(),
        _ => vec![],
    };
    let parms: Vec<Option<Dict>> = match d.get("DecodeParms").or_else(|| d.get("DP")).map(|f| pdf.resolve(f)) {
        Some(Obj::Dict(p)) => vec![Some(p)],
        Some(Obj::Arr(a)) => a.iter().map(|o| pdf.resolve(o).dict().cloned()).collect(),
        _ => vec![],
    };
    let mut data = raw.to_vec();
    for (i, f) in filters.iter().enumerate() {
        let p = parms.get(i).cloned().flatten();
        data = match f.as_slice() {
            b"FlateDecode" | b"Fl" => predict(inflate(&data), p.as_ref()),
            b"LZWDecode" | b"LZW" => {
                let early = p.as_ref().and_then(|p| p.get("EarlyChange")).and_then(|o| o.num()).unwrap_or(1.0) != 0.0;
                predict(lzw(&data, early), p.as_ref())
            }
            b"ASCIIHexDecode" | b"AHx" => {
                let mut v = b"<".to_vec();
                v.extend(data.iter().take_while(|&&c| c != b'>'));
                v.push(b'>');
                match Lex::new(&v, 0).obj(0, false) {
                    Some(Obj::Str(s)) => s,
                    _ => vec![],
                }
            }
            b"ASCII85Decode" | b"A85" => ascii85(&data),
            b"RunLengthDecode" | b"RL" => runlength(&data),
            // images and the like: not text
            _ => return None,
        };
    }
    Some(data)
}

fn inflate(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut out = vec![];
    let _ = flate2::read::ZlibDecoder::new(data).take(MAX_STREAM).read_to_end(&mut out);
    if out.is_empty() && data.len() > 2 {
        // raw deflate without zlib header
        let _ = flate2::read::DeflateDecoder::new(data).take(MAX_STREAM).read_to_end(&mut out);
    }
    out
}

fn predict(data: Vec<u8>, p: Option<&Dict>) -> Vec<u8> {
    let Some(p) = p else { return data };
    let g = |k: &str, d: f64| p.get(k).and_then(|o| o.num()).unwrap_or(d);
    let pred = g("Predictor", 1.0) as i64;
    if pred < 10 {
        return data;
    }
    let colors = g("Colors", 1.0).clamp(1.0, 64.0) as usize;
    let bpc = g("BitsPerComponent", 8.0).clamp(1.0, 16.0) as usize;
    let cols = g("Columns", 1.0).clamp(1.0, 1e6) as usize;
    let bpp = (colors * bpc).div_ceil(8).max(1);
    let row = (colors * bpc * cols).div_ceil(8);
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        if chunk.len() < 2 {
            break;
        }
        let ft = chunk[0];
        let mut cur: Vec<u8> = chunk[1..].to_vec();
        cur.resize(row, 0);
        for i in 0..row {
            let a = if i >= bpp { cur[i - bpp] as i32 } else { 0 };
            let b = prev[i] as i32;
            let c = if i >= bpp { prev[i - bpp] as i32 } else { 0 };
            let add = match ft {
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let pp = a + b - c;
                    let (pa, pb, pc) = ((pp - a).abs(), (pp - b).abs(), (pp - c).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => 0,
            };
            cur[i] = (cur[i] as i32 + add) as u8;
        }
        out.extend_from_slice(&cur);
        prev = cur;
    }
    out
}

fn lzw(data: &[u8], early: bool) -> Vec<u8> {
    let mut out = vec![];
    let mut dict: Vec<Vec<u8>> = (0..256).map(|i| vec![i as u8]).collect();
    dict.push(vec![]);
    dict.push(vec![]);
    let mut bits = 9;
    let mut acc: u32 = 0;
    let mut nacc = 0;
    let mut prev: Option<Vec<u8>> = None;
    for &b in data {
        acc = (acc << 8) | b as u32;
        nacc += 8;
        while nacc >= bits {
            let code = ((acc >> (nacc - bits)) & ((1 << bits) - 1)) as usize;
            nacc -= bits;
            if code == 256 {
                dict.truncate(258);
                bits = 9;
                prev = None;
                continue;
            }
            if code == 257 {
                return out;
            }
            let entry = if code < dict.len() {
                dict[code].clone()
            } else if let Some(p) = &prev {
                let mut e = p.clone();
                e.push(p.first().copied().unwrap_or(0));
                e
            } else {
                return out;
            };
            out.extend_from_slice(&entry);
            if out.len() as u64 > MAX_STREAM {
                return out;
            }
            if let Some(mut p) = prev.take() {
                p.push(entry.first().copied().unwrap_or(0));
                if dict.len() < 4096 {
                    dict.push(p);
                }
            }
            prev = Some(entry);
            let lim = dict.len() + if early { 1 } else { 0 };
            bits = if lim >= 2048 {
                12
            } else if lim >= 1024 {
                11
            } else if lim >= 512 {
                10
            } else {
                9
            };
        }
    }
    out
}

fn ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let mut grp: Vec<u32> = vec![];
    let mut i = 0;
    if data.starts_with(b"<~") {
        i = 2;
    }
    while i < data.len() {
        let c = data[i];
        i += 1;
        match c {
            b'~' => break,
            b'z' if grp.is_empty() => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                grp.push((c - b'!') as u32);
                if grp.len() == 5 {
                    let v = grp.iter().fold(0u64, |a, &d| a * 85 + d as u64) as u32;
                    out.extend_from_slice(&v.to_be_bytes());
                    grp.clear();
                }
            }
            _ => {}
        }
    }
    if grp.len() > 1 {
        let n = grp.len();
        while grp.len() < 5 {
            grp.push(84);
        }
        let v = grp.iter().fold(0u64, |a, &d| a * 85 + d as u64) as u32;
        out.extend_from_slice(&v.to_be_bytes()[..n - 1]);
    }
    out
}

fn runlength(data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let mut i = 0;
    while i < data.len() {
        let n = data[i] as usize;
        i += 1;
        if n == 128 {
            break;
        } else if n < 128 {
            let e = (i + n + 1).min(data.len());
            out.extend_from_slice(&data[i..e]);
            i = e;
        } else if let Some(&b) = data.get(i) {
            out.extend(std::iter::repeat_n(b, 257 - n));
            i += 1;
        }
    }
    out
}

// ===========================================================================
// File structure

#[derive(Clone, Copy, Debug)]
enum Loc {
    Off(usize),
    InStm(u32, u32),
}

struct Pdf<'a> {
    data: &'a [u8],
    xref: HashMap<u32, Loc>,
    trailer: Dict,
    cache: RefCell<HashMap<u32, Obj>>,
    objstm: RefCell<HashMap<u32, Rc<(Vec<(u32, usize)>, Vec<u8>)>>>,
    depth: Cell<u32>,
    scanned: RefCell<Option<HashMap<u32, usize>>>,
}

impl<'a> Pdf<'a> {
    fn open(data: &'a [u8]) -> Result<Pdf<'a>, String> {
        let mut p = Pdf {
            data,
            xref: HashMap::new(),
            trailer: Dict::default(),
            cache: Default::default(),
            objstm: Default::default(),
            depth: Cell::new(0),
            scanned: RefCell::new(None),
        };
        // header offset (garbage before %PDF- shifts every offset)
        let base = find(&data[..data.len().min(1024)], b"%PDF-", 0).unwrap_or(0);
        let mut ok = false;
        if let Some(sx) = rfind(&data[data.len().saturating_sub(4096)..], b"startxref") {
            let sx = data.len().saturating_sub(4096) + sx + 9;
            let mut l = Lex::new(data, sx);
            if let Some(Obj::Num(off)) = l.obj(0, false) {
                let mut seen = HashSet::new();
                let off = off.max(0.0) as usize;
                ok = p.load_xref(off, &mut seen, 0) || (base > 0 && p.load_xref(off + base, &mut seen, 0));
            }
        }
        if !ok || p.trailer.get("Root").is_none() || p.root().is_none() {
            p.rebuild();
        }
        if p.trailer.get("Encrypt").is_some_and(|e| *e != Obj::Null) {
            return Err("PDF is encrypted/password protected — cannot import".into());
        }
        if p.root().is_none() {
            return Err("damaged PDF file (no document catalog)".into());
        }
        Ok(p)
    }

    fn root(&self) -> Option<Dict> {
        self.resolve(self.trailer.get("Root")?).dict().cloned()
    }

    fn load_xref(&mut self, off: usize, seen: &mut HashSet<usize>, depth: u32) -> bool {
        if depth > 64 || !seen.insert(off) || off >= self.data.len() {
            return false;
        }
        let mut l = Lex::new(self.data, off);
        l.skip_ws();
        let trailer;
        if self.data[l.pos..].starts_with(b"xref") {
            l.pos += 4;
            loop {
                l.skip_ws();
                if self.data[l.pos.min(self.data.len())..].starts_with(b"trailer") {
                    l.pos += 7;
                    break;
                }
                let (Some(Obj::Num(start)), Some(Obj::Num(count))) = (l.obj(0, false), l.obj(0, false)) else { return false };
                let (start, count) = (start.max(0.0) as u32, count.clamp(0.0, 10_000_000.0) as u32);
                for k in 0..count {
                    let (Some(Obj::Num(o)), Some(Obj::Num(_g)), Some(Obj::Kw(t))) = (l.obj(0, false), l.obj(0, false), l.obj(0, false)) else { return false };
                    if t == b"n" && o > 0.0 {
                        self.xref.entry(start.saturating_add(k)).or_insert(Loc::Off(o as usize));
                    }
                }
            }
            match l.obj(0, false) {
                Some(Obj::Dict(d)) => trailer = d,
                _ => return false,
            }
        } else {
            // xref stream
            let Some((_, Obj::Stream(d, raw))) = self.parse_indirect(off) else { return false };
            let Some(data) = decode_stream(&d, &raw, self) else { return false };
            let w: Vec<usize> = match d.get("W") {
                Some(Obj::Arr(a)) => a.iter().map(|o| o.num().unwrap_or(0.0).clamp(0.0, 8.0) as usize).collect(),
                _ => return false,
            };
            if w.len() < 3 {
                return false;
            }
            let size = d.get("Size").and_then(|o| o.num()).unwrap_or(0.0).max(0.0) as u32;
            let index: Vec<u32> = match d.get("Index") {
                Some(Obj::Arr(a)) => a.iter().map(|o| o.num().unwrap_or(0.0).max(0.0) as u32).collect(),
                _ => vec![0, size],
            };
            let rec = w[0] + w[1] + w[2];
            if rec == 0 {
                return false;
            }
            let field = |r: &[u8], s: usize, n: usize| -> u64 { r[s..s + n].iter().fold(0u64, |a, &b| (a << 8) | b as u64) };
            let mut pos = 0;
            for pair in index.chunks(2) {
                let (start, count) = (pair[0], pair.get(1).copied().unwrap_or(0));
                for k in 0..count {
                    let Some(r) = data.get(pos..pos + rec) else { break };
                    pos += rec;
                    let t = if w[0] == 0 { 1 } else { field(r, 0, w[0]) };
                    let f2 = field(r, w[0], w[1]);
                    let f3 = field(r, w[0] + w[1], w[2]);
                    let num = start.saturating_add(k);
                    match t {
                        1 => {
                            self.xref.entry(num).or_insert(Loc::Off(f2 as usize));
                        }
                        2 => {
                            self.xref.entry(num).or_insert(Loc::InStm(f2 as u32, f3 as u32));
                        }
                        _ => {}
                    }
                }
            }
            trailer = d;
        }
        if self.trailer.0.is_empty() {
            self.trailer = trailer.clone();
        } else {
            for (k, v) in &trailer.0 {
                if self.trailer.get(&String::from_utf8_lossy(k)).is_none() {
                    self.trailer.0.push((k.clone(), v.clone()));
                }
            }
        }
        if let Some(x) = trailer.get("XRefStm").and_then(|o| o.num()) {
            self.load_xref(x.max(0.0) as usize, seen, depth + 1);
        }
        if let Some(prev) = trailer.get("Prev").and_then(|o| o.num()) {
            self.load_xref(prev.max(0.0) as usize, seen, depth + 1);
        }
        true
    }

    /// Rebuild the cross reference by scanning for `n g obj`.
    fn rebuild(&mut self) {
        let map = self.scan();
        for (n, off) in &map {
            self.xref.insert(*n, Loc::Off(*off));
        }
        self.cache.borrow_mut().clear();
        // objects inside object streams
        let mut extra = vec![];
        for (&n, _) in map.iter() {
            if let Some(Obj::Stream(d, _)) = self.get(n) {
                if d.get("Type").and_then(|t| t.name()) == Some(b"ObjStm") {
                    if let Some(st) = self.objstm_data(n) {
                        for (i, (num, _)) in st.0.iter().enumerate() {
                            extra.push((*num, Loc::InStm(n, i as u32)));
                        }
                    }
                }
            }
        }
        for (num, loc) in extra {
            self.xref.entry(num).or_insert(loc);
        }
        // trailer: the last `trailer` dictionary with a /Root, else the catalog
        let mut pos = self.data.len();
        while let Some(t) = rfind(&self.data[..pos], b"trailer") {
            let mut l = Lex::new(self.data, t + 7);
            if let Some(Obj::Dict(d)) = l.obj(0, true) {
                if d.get("Root").is_some() {
                    for (k, v) in d.0 {
                        if self.trailer.get(&String::from_utf8_lossy(&k)).is_none() || k == b"Root" {
                            self.trailer.0.retain(|(kk, _)| *kk != k);
                            self.trailer.0.push((k, v));
                        }
                    }
                    break;
                }
            }
            pos = t;
        }
        if self.root().is_none() {
            let mut nums: Vec<u32> = self.xref.keys().copied().collect();
            nums.sort();
            for n in nums {
                if let Some(d) = self.get(n).and_then(|o| o.dict().cloned()) {
                    if d.get("Type").and_then(|t| t.name()) == Some(b"Catalog") {
                        self.trailer.0.retain(|(k, _)| k != b"Root");
                        self.trailer.0.push((b"Root".to_vec(), Obj::Ref(n)));
                        break;
                    }
                }
            }
        }
    }

    fn scan(&self) -> HashMap<u32, usize> {
        if let Some(m) = self.scanned.borrow().as_ref() {
            return m.clone();
        }
        let d = self.data;
        let mut map = HashMap::new();
        let mut i = 0;
        while let Some(k) = find(d, b"obj", i) {
            i = k + 3;
            if k == 0 || d.get(k + 3).is_some_and(|&c| !(is_ws(c) || is_delim(c))) {
                continue;
            }
            // walk back: ws gen ws num
            let mut j = k;
            while j > 0 && is_ws(d[j - 1]) {
                j -= 1;
            }
            let ge = j;
            while j > 0 && d[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j == ge {
                continue;
            }
            while j > 0 && is_ws(d[j - 1]) {
                j -= 1;
            }
            let ne = j;
            while j > 0 && d[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j == ne || (j > 0 && !(is_ws(d[j - 1]) || is_delim(d[j - 1]))) {
                continue;
            }
            if let Ok(n) = std::str::from_utf8(&d[j..ne]).unwrap_or("x").parse::<u32>() {
                map.insert(n, j);
            }
        }
        *self.scanned.borrow_mut() = Some(map.clone());
        map
    }

    /// Parse `n g obj … endobj` at `off`.
    fn parse_indirect(&self, off: usize) -> Option<(u32, Obj)> {
        let mut l = Lex::new(self.data, off);
        let Some(Obj::Num(n)) = l.obj(0, false) else { return None };
        let Some(Obj::Num(_)) = l.obj(0, false) else { return None };
        let Some(Obj::Kw(k)) = l.obj(0, false) else { return None };
        if k != b"obj" {
            return None;
        }
        let o = l.obj(0, true)?;
        let num = n.max(0.0) as u32;
        if let Obj::Dict(d) = &o {
            l.skip_ws();
            if self.data[l.pos.min(self.data.len())..].starts_with(b"stream") {
                l.pos += 6;
                if self.data.get(l.pos) == Some(&b'\r') {
                    l.pos += 1;
                }
                if self.data.get(l.pos) == Some(&b'\n') {
                    l.pos += 1;
                }
                let start = l.pos;
                let len = d.get("Length").and_then(|v| match v {
                    Obj::Ref(r) if *r != num => self.get(*r).and_then(|o| o.num()),
                    o => o.num(),
                });
                let mut end = None;
                if let Some(len) = len {
                    let e = start.saturating_add(len.max(0.0) as usize);
                    if e <= self.data.len() {
                        let mut ll = Lex::new(self.data, e);
                        ll.skip_ws();
                        if self.data[ll.pos.min(self.data.len())..].starts_with(b"endstream") {
                            end = Some(e);
                        }
                    }
                }
                let end = end.or_else(|| {
                    let e = find(self.data, b"endstream", start)?;
                    let mut e2 = e;
                    if e2 > start && self.data[e2 - 1] == b'\n' {
                        e2 -= 1;
                    }
                    if e2 > start && self.data[e2 - 1] == b'\r' {
                        e2 -= 1;
                    }
                    Some(e2)
                });
                let end = end.unwrap_or(self.data.len()).min(start + MAX_STREAM as usize).min(self.data.len());
                let raw = self.data.get(start..end).unwrap_or(&[]).to_vec();
                return Some((num, Obj::Stream(d.clone(), Rc::new(raw))));
            }
        }
        Some((num, o))
    }

    fn get(&self, num: u32) -> Option<Obj> {
        if let Some(o) = self.cache.borrow().get(&num) {
            return Some(o.clone());
        }
        if self.depth.get() > 32 {
            return None;
        }
        self.depth.set(self.depth.get() + 1);
        let r = self.get_uncached(num);
        self.depth.set(self.depth.get() - 1);
        if let Some(o) = &r {
            self.cache.borrow_mut().insert(num, o.clone());
        }
        r
    }

    fn get_uncached(&self, num: u32) -> Option<Obj> {
        match self.xref.get(&num).copied() {
            Some(Loc::Off(off)) => {
                if let Some((n, o)) = self.parse_indirect(off) {
                    if n == num {
                        return Some(o);
                    }
                }
                // wrong offset: look the object up by scanning
                let off = *self.scan().get(&num)?;
                self.parse_indirect(off).map(|(_, o)| o)
            }
            Some(Loc::InStm(stm, idx)) => {
                let st = self.objstm_data(stm)?;
                let (n, off) = *st.0.get(idx as usize).filter(|(n, _)| *n == num).or_else(|| st.0.iter().find(|(n, _)| *n == num))?;
                let _ = n;
                let mut l = Lex::new(&st.1, off);
                l.obj(0, true)
            }
            None => {
                let off = *self.scan().get(&num)?;
                self.parse_indirect(off).map(|(_, o)| o)
            }
        }
    }

    fn objstm_data(&self, stm: u32) -> Option<Rc<(Vec<(u32, usize)>, Vec<u8>)>> {
        if let Some(s) = self.objstm.borrow().get(&stm) {
            return Some(s.clone());
        }
        let Some(Obj::Stream(d, raw)) = self.get(stm) else { return None };
        let data = decode_stream(&d, &raw, self)?;
        let n = d.get("N").and_then(|o| o.num()).unwrap_or(0.0).clamp(0.0, 1e6) as usize;
        let first = d.get("First").and_then(|o| o.num()).unwrap_or(0.0).max(0.0) as usize;
        let mut l = Lex::new(&data, 0);
        let mut idx = vec![];
        for _ in 0..n {
            let (Some(Obj::Num(a)), Some(Obj::Num(b))) = (l.obj(0, false), l.obj(0, false)) else { break };
            idx.push((a.max(0.0) as u32, first.saturating_add(b.max(0.0) as usize)));
        }
        let rc = Rc::new((idx, data));
        self.objstm.borrow_mut().insert(stm, rc.clone());
        Some(rc)
    }

    fn resolve(&self, o: &Obj) -> Obj {
        let mut o = o.clone();
        for _ in 0..16 {
            match o {
                Obj::Ref(r) => o = self.get(r).unwrap_or(Obj::Null),
                _ => return o,
            }
        }
        Obj::Null
    }

    fn rdict(&self, o: Option<&Obj>) -> Option<Dict> {
        self.resolve(o?).dict().cloned()
    }

    /// Pages in order: (page dict, inherited resources, rotate).
    fn pages(&self) -> Vec<(Dict, Dict, i32)> {
        let mut out = vec![];
        let Some(root) = self.root() else { return out };
        let mut seen = HashSet::new();
        if let Some(p) = root.get("Pages") {
            self.walk(p, Dict::default(), 0, &mut out, &mut seen, 0);
        }
        out
    }

    fn walk(&self, node: &Obj, res: Dict, rot: i32, out: &mut Vec<(Dict, Dict, i32)>, seen: &mut HashSet<u32>, depth: u32) {
        if depth > 64 || out.len() >= MAX_PAGES {
            return;
        }
        if let Obj::Ref(r) = node {
            if !seen.insert(*r) {
                return;
            }
        }
        let Some(d) = self.resolve(node).dict().cloned() else { return };
        let res = self.rdict(d.get("Resources")).unwrap_or(res);
        let rot = d.get("Rotate").map(|o| self.resolve(o)).and_then(|o| o.num()).map(|v| v as i32).unwrap_or(rot);
        match self.resolve(d.get("Kids").unwrap_or(&Obj::Null)) {
            Obj::Arr(kids) if d.get("Type").and_then(|t| t.name()) != Some(b"Page") => {
                for k in &kids {
                    self.walk(k, res.clone(), rot, out, seen, depth + 1);
                }
            }
            _ => out.push((d, res, rot)),
        }
    }
}

// ===========================================================================
// Fonts

const STD_HIGH: &[(u8, char)] = &[
    (0x27, '’'),
    (0x60, '‘'),
    (0xA1, '¡'),
    (0xA2, '¢'),
    (0xA3, '£'),
    (0xA4, '⁄'),
    (0xA5, '¥'),
    (0xA6, 'ƒ'),
    (0xA7, '§'),
    (0xA8, '¤'),
    (0xA9, '\''),
    (0xAA, '“'),
    (0xAB, '«'),
    (0xAC, '‹'),
    (0xAD, '›'),
    (0xAE, 'ﬁ'),
    (0xAF, 'ﬂ'),
    (0xB1, '–'),
    (0xB2, '†'),
    (0xB3, '‡'),
    (0xB4, '·'),
    (0xB6, '¶'),
    (0xB7, '•'),
    (0xB8, '‚'),
    (0xB9, '„'),
    (0xBA, '”'),
    (0xBB, '»'),
    (0xBC, '…'),
    (0xBD, '‰'),
    (0xBF, '¿'),
    (0xC1, '`'),
    (0xC2, '´'),
    (0xC3, 'ˆ'),
    (0xC4, '˜'),
    (0xC5, '¯'),
    (0xC6, '˘'),
    (0xC7, '˙'),
    (0xC8, '¨'),
    (0xCA, '˚'),
    (0xCB, '¸'),
    (0xCD, '˝'),
    (0xCE, '˛'),
    (0xCF, 'ˇ'),
    (0xD0, '—'),
    (0xE1, 'Æ'),
    (0xE3, 'ª'),
    (0xE8, 'Ł'),
    (0xE9, 'Ø'),
    (0xEA, 'Œ'),
    (0xEB, 'º'),
    (0xF1, 'æ'),
    (0xF5, 'ı'),
    (0xF8, 'ł'),
    (0xF9, 'ø'),
    (0xFA, 'œ'),
    (0xFB, 'ß'),
];

fn base_encoding(name: &[u8]) -> Vec<Option<char>> {
    let mut t: Vec<Option<char>> = vec![None; 256];
    match name {
        b"StandardEncoding" => {
            for c in 0x20..0x7Fu8 {
                t[c as usize] = Some(c as char);
            }
            for &(b, c) in STD_HIGH {
                t[b as usize] = Some(c);
            }
        }
        b"MacRomanEncoding" => {
            for c in 0x20..=0xFFu8 {
                if c != 0x7F {
                    t[c as usize] = Some(crate::fmt_text::byte_char(c, crate::fmt_text::Enc::MacRoman));
                }
            }
        }
        _ => {
            // WinAnsiEncoding (also the fallback)
            for c in 0x20..=0xFFu8 {
                if c != 0x7F {
                    let ch = crate::rtf::decode_byte(c, 1252);
                    t[c as usize] = Some(if ('\u{80}'..='\u{9f}').contains(&ch) { '•' } else { ch });
                }
            }
        }
    }
    t
}

const LATIN1_NAMES: [&str; 96] = [
    "nbspace", "exclamdown", "cent", "sterling", "currency", "yen", "brokenbar", "section", "dieresis", "copyright", "ordfeminine", "guillemotleft", "logicalnot", "sfthyphen", "registered", "macron", "degree", "plusminus", "twosuperior", "threesuperior", "acute", "mu", "paragraph", "periodcentered", "cedilla", "onesuperior", "ordmasculine", "guillemotright", "onequarter", "onehalf", "threequarters", "questiondown", "Agrave", "Aacute", "Acircumflex", "Atilde", "Adieresis", "Aring", "AE", "Ccedilla", "Egrave", "Eacute", "Ecircumflex", "Edieresis", "Igrave", "Iacute", "Icircumflex", "Idieresis", "Eth", "Ntilde", "Ograve", "Oacute", "Ocircumflex", "Otilde", "Odieresis", "multiply", "Oslash", "Ugrave", "Uacute", "Ucircumflex", "Udieresis", "Yacute", "Thorn", "germandbls", "agrave", "aacute", "acircumflex", "atilde", "adieresis", "aring", "ae", "ccedilla", "egrave", "eacute", "ecircumflex", "edieresis", "igrave", "iacute", "icircumflex", "idieresis", "eth", "ntilde", "ograve", "oacute", "ocircumflex", "otilde", "odieresis", "divide", "oslash", "ugrave", "uacute", "ucircumflex", "udieresis", "yacute", "thorn", "ydieresis",
];

const ASCII_NAMES: [&str; 95] = [
    "space", "exclam", "quotedbl", "numbersign", "dollar", "percent", "ampersand", "quotesingle", "parenleft", "parenright", "asterisk", "plus", "comma", "hyphen", "period", "slash", "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "colon", "semicolon", "less", "equal", "greater", "question", "at", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "bracketleft", "backslash", "bracketright", "asciicircum", "underscore", "grave", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z", "braceleft", "bar", "braceright", "asciitilde",
];

const EXTRA_NAMES: &[(&str, &str)] = &[
    ("Euro", "€"),
    ("quotesinglbase", "‚"),
    ("florin", "ƒ"),
    ("quotedblbase", "„"),
    ("ellipsis", "…"),
    ("dagger", "†"),
    ("daggerdbl", "‡"),
    ("circumflex", "ˆ"),
    ("perthousand", "‰"),
    ("Scaron", "Š"),
    ("guilsinglleft", "‹"),
    ("OE", "Œ"),
    ("Zcaron", "Ž"),
    ("quoteleft", "‘"),
    ("quoteright", "’"),
    ("quotedblleft", "“"),
    ("quotedblright", "”"),
    ("bullet", "•"),
    ("endash", "–"),
    ("emdash", "—"),
    ("tilde", "˜"),
    ("trademark", "™"),
    ("scaron", "š"),
    ("guilsinglright", "›"),
    ("oe", "œ"),
    ("zcaron", "ž"),
    ("Ydieresis", "Ÿ"),
    ("space", " "),
    ("nonbreakingspace", "\u{a0}"),
    ("minus", "−"),
    ("fraction", "⁄"),
    ("fi", "fi"),
    ("fl", "fl"),
    ("ff", "ff"),
    ("ffi", "ffi"),
    ("ffl", "ffl"),
    ("dotlessi", "ı"),
    ("Lslash", "Ł"),
    ("lslash", "ł"),
    ("ring", "˚"),
    ("breve", "˘"),
    ("dotaccent", "˙"),
    ("hungarumlaut", "˝"),
    ("ogonek", "˛"),
    ("caron", "ˇ"),
    ("periodcentered", "·"),
    ("middot", "·"),
    ("mu1", "µ"),
    ("afii61352", "№"),
    ("numero", "№"),
    ("Delta", "∆"),
    ("Omega", "Ω"),
    ("pi", "π"),
    ("notequal", "≠"),
    ("lessequal", "≤"),
    ("greaterequal", "≥"),
    ("infinity", "∞"),
    ("summation", "∑"),
    ("radical", "√"),
    ("approxequal", "≈"),
    ("partialdiff", "∂"),
    ("product", "∏"),
    ("integral", "∫"),
    ("lozenge", "◊"),
    ("arrowright", "→"),
    ("arrowleft", "←"),
    ("checkmark", "✓"),
    ("hyphenminus", "-"),
    ("softhyphen", "\u{ad}"),
];

/// Adobe glyph name → Unicode text.
fn glyph_name(name: &[u8]) -> Option<String> {
    let n = std::str::from_utf8(name).ok()?;
    let n = n.split('.').next().unwrap_or(n);
    if n.is_empty() {
        return None;
    }
    if n.contains('_') {
        let parts: Option<String> = n.split('_').map(|p| glyph_name(p.as_bytes())).collect();
        return parts;
    }
    if let Some(i) = ASCII_NAMES.iter().position(|x| *x == n) {
        return Some(((0x20 + i) as u8 as char).to_string());
    }
    if let Some(i) = LATIN1_NAMES.iter().position(|x| *x == n) {
        return char::from_u32(0xA0 + i as u32).map(|c| c.to_string());
    }
    if let Some((_, s)) = EXTRA_NAMES.iter().find(|(k, _)| *k == n) {
        return Some(s.to_string());
    }
    if let Some(h) = n.strip_prefix("uni") {
        if h.len() >= 4 && h.len() % 4 == 0 && h.bytes().all(|c| c.is_ascii_hexdigit()) {
            let units: Vec<u16> = h.as_bytes().chunks(4).filter_map(|c| u16::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok()).collect();
            return Some(String::from_utf16_lossy(&units));
        }
    }
    if let Some(h) = n.strip_prefix('u') {
        if (4..=6).contains(&h.len()) && h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return u32::from_str_radix(h, 16).ok().and_then(char::from_u32).map(|c| c.to_string());
        }
    }
    if let Some(num) = n.strip_prefix("afii").and_then(|v| v.parse::<u32>().ok()) {
        let u = match num {
            10017..=10022 => 0x410 + num - 10017,
            10023 => 0x401,
            10024..=10049 => 0x416 + num - 10024,
            10065..=10070 => 0x430 + num - 10065,
            10071 => 0x451,
            10072..=10097 => 0x436 + num - 10072,
            10051..=10062 => [0x402, 0x403, 0x404, 0x405, 0x406, 0x407, 0x408, 0x409, 0x40A, 0x40B, 0x40C, 0x40E][(num - 10051) as usize],
            10145 => 0x40F,
            10099..=10110 => [0x452, 0x453, 0x454, 0x455, 0x456, 0x457, 0x458, 0x459, 0x45A, 0x45B, 0x45C, 0x45E][(num - 10099) as usize],
            10193 => 0x45F,
            10050 => 0x490,
            10098 => 0x491,
            _ => return None,
        };
        return char::from_u32(u).map(|c| c.to_string());
    }
    None
}

struct Font {
    cid: bool,
    codespace: Vec<(usize, u32, u32)>,
    uni: HashMap<u32, String>,
    simple: Vec<Option<String>>,
    widths: HashMap<u32, f64>,
    dw: f64,
    /// glyph space → text space (0.001, or a Type3 FontMatrix)
    scale: f64,
    bold: bool,
    italic: bool,
    symbol_name: Option<String>,
}

impl Font {
    fn codes(&self, s: &[u8]) -> Vec<(u32, usize)> {
        let mut out = vec![];
        let mut i = 0;
        while i < s.len() {
            if !self.cid {
                out.push((s[i] as u32, 1));
                i += 1;
                continue;
            }
            let mut took = None;
            for n in 1..=4usize {
                if i + n > s.len() {
                    break;
                }
                let v = s[i..i + n].iter().fold(0u32, |a, &b| (a << 8) | b as u32);
                if self.codespace.iter().any(|&(l, lo, hi)| l == n && v >= lo && v <= hi) {
                    took = Some((v, n));
                    break;
                }
            }
            let (v, n) = took.unwrap_or_else(|| {
                let n = 2.min(s.len() - i);
                (s[i..i + n].iter().fold(0u32, |a, &b| (a << 8) | b as u32), n)
            });
            out.push((v, n));
            i += n;
        }
        out
    }
    fn text(&self, code: u32) -> Option<String> {
        if let Some(s) = self.uni.get(&code) {
            return Some(s.clone());
        }
        if self.cid {
            return None;
        }
        if let Some(f) = &self.symbol_name {
            if let Some(c) = crate::rtf::symbol_char(f, code) {
                return Some(c.to_string());
            }
        }
        self.simple.get(code as usize).cloned().flatten()
    }
    fn width(&self, code: u32) -> f64 {
        self.widths.get(&code).copied().unwrap_or(self.dw) * self.scale
    }
}

fn parse_cmap(data: &[u8], font: &mut Font) {
    let mut l = Lex::new(data, 0);
    let mut stack: Vec<Obj> = vec![];
    let mut mode = 0; // 1 codespace, 2 bfchar, 3 bfrange
    let mut total = 0usize;
    let utf16 = |b: &[u8]| -> String {
        let u: Vec<u16> = b.chunks(2).map(|c| if c.len() == 2 { u16::from_be_bytes([c[0], c[1]]) } else { c[0] as u16 }).collect();
        String::from_utf16_lossy(&u)
    };
    let val = |b: &[u8]| -> u32 { b.iter().take(4).fold(0u32, |a, &x| (a << 8) | x as u32) };
    while let Some(o) = l.obj(0, false) {
        total += 1;
        if total > 2_000_000 {
            break;
        }
        match o {
            Obj::Kw(k) => {
                match k.as_slice() {
                    b"begincodespacerange" => mode = 1,
                    b"beginbfchar" => mode = 2,
                    b"beginbfrange" => mode = 3,
                    b"endcodespacerange" => {
                        for p in stack.chunks(2) {
                            if let [Obj::Str(a), Obj::Str(b)] = p {
                                if !a.is_empty() && a.len() <= 4 {
                                    font.codespace.push((a.len(), val(a), val(b)));
                                }
                            }
                        }
                        mode = 0;
                    }
                    b"endbfchar" => {
                        for p in stack.chunks(2) {
                            if let [Obj::Str(a), Obj::Str(b)] = p {
                                font.uni.insert(val(a), utf16(b));
                            } else if let [Obj::Str(a), Obj::Name(n)] = p {
                                if let Some(s) = glyph_name(n) {
                                    font.uni.insert(val(a), s);
                                }
                            }
                        }
                        mode = 0;
                    }
                    b"endbfrange" => {
                        for p in stack.chunks(3) {
                            let [Obj::Str(a), Obj::Str(b), dst] = p else { continue };
                            let (lo, hi) = (val(a), val(b));
                            if hi < lo || hi - lo > 65535 {
                                continue;
                            }
                            match dst {
                                Obj::Str(d) => {
                                    let mut units: Vec<u16> = d.chunks(2).map(|c| if c.len() == 2 { u16::from_be_bytes([c[0], c[1]]) } else { c[0] as u16 }).collect();
                                    for code in lo..=hi {
                                        font.uni.insert(code, String::from_utf16_lossy(&units));
                                        if let Some(last) = units.last_mut() {
                                            *last = last.wrapping_add(1);
                                        }
                                    }
                                }
                                Obj::Arr(arr) => {
                                    for (k, d) in arr.iter().enumerate() {
                                        if let Obj::Str(d) = d {
                                            font.uni.insert(lo.saturating_add(k as u32), utf16(d));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        mode = 0;
                    }
                    _ => {}
                }
                stack.clear();
            }
            o if mode != 0 => stack.push(o),
            _ => {}
        }
    }
}

fn load_font(pdf: &Pdf, fd: &Dict) -> Font {
    let sub = fd.get("Subtype").and_then(|o| o.name()).unwrap_or(b"").to_vec();
    let cid = sub == b"Type0";
    let base = fd.get("BaseFont").and_then(|o| o.name()).map(|n| String::from_utf8_lossy(n).into_owned()).unwrap_or_default();
    let base = match base.split_once('+') {
        Some((p, rest)) if p.len() == 6 => rest.to_string(),
        _ => base,
    };
    let lower = base.to_ascii_lowercase();
    let mut f = Font {
        cid,
        codespace: vec![],
        uni: HashMap::new(),
        simple: vec![],
        widths: HashMap::new(),
        dw: if cid { 1000.0 } else if lower.contains("courier") || lower.contains("mono") { 600.0 } else { 500.0 },
        scale: 0.001,
        bold: ["bold", "black", "heavy", "semibold", "demi"].iter().any(|k| lower.contains(k)),
        italic: ["italic", "oblique", "kursiv", "slanted"].iter().any(|k| lower.contains(k)),
        symbol_name: None,
    };
    // descriptor flags
    let desc_of = |d: &Dict| pdf.rdict(d.get("FontDescriptor"));
    let mut desc = desc_of(fd);
    let mut descendant: Option<Dict> = None;
    if cid {
        if let Obj::Arr(a) = pdf.resolve(fd.get("DescendantFonts").unwrap_or(&Obj::Null)) {
            descendant = a.first().and_then(|o| pdf.resolve(o).dict().cloned());
        }
        if let Some(dd) = &descendant {
            desc = desc_of(dd).or(desc);
            if let Some(v) = dd.get("DW").map(|o| pdf.resolve(o)).and_then(|o| o.num()) {
                f.dw = v;
            }
            if let Obj::Arr(w) = pdf.resolve(dd.get("W").unwrap_or(&Obj::Null)) {
                let mut i = 0;
                while i < w.len() && f.widths.len() < 200_000 {
                    let Some(c0) = pdf.resolve(&w[i]).num() else { break };
                    match w.get(i + 1).map(|o| pdf.resolve(o)) {
                        Some(Obj::Arr(ws)) => {
                            for (k, wv) in ws.iter().enumerate() {
                                if let Some(v) = pdf.resolve(wv).num() {
                                    f.widths.insert((c0.max(0.0) as u32).saturating_add(k as u32), v);
                                }
                            }
                            i += 2;
                        }
                        Some(Obj::Num(c1)) => {
                            let v = w.get(i + 2).map(|o| pdf.resolve(o)).and_then(|o| o.num()).unwrap_or(f.dw);
                            let (a, b) = (c0.max(0.0) as u32, c1.max(0.0) as u32);
                            if b >= a && b - a < 65536 {
                                for c in a..=b {
                                    f.widths.insert(c, v);
                                }
                            }
                            i += 3;
                        }
                        _ => break,
                    }
                }
            }
        }
    }
    if let Some(d) = &desc {
        let flags = d.get("Flags").map(|o| pdf.resolve(o)).and_then(|o| o.num()).unwrap_or(0.0) as i64;
        if flags & 64 != 0 {
            f.italic = true;
        }
        if flags & (1 << 18) != 0 {
            f.bold = true;
        }
        if d.get("FontWeight").map(|o| pdf.resolve(o)).and_then(|o| o.num()).is_some_and(|w| w >= 600.0) {
            f.bold = true;
        }
        if d.get("ItalicAngle").map(|o| pdf.resolve(o)).and_then(|o| o.num()).is_some_and(|a| a.abs() > 5.0) {
            f.italic = true;
        }
    }
    if !cid {
        let symbolic = desc.as_ref().and_then(|d| d.get("Flags")).map(|o| pdf.resolve(o)).and_then(|o| o.num()).is_some_and(|v| (v as i64) & 4 != 0);
        let enc = fd.get("Encoding").map(|o| pdf.resolve(o));
        let mut base_name: Vec<u8> = if sub == b"Type1" && !symbolic { b"StandardEncoding".to_vec() } else { b"WinAnsiEncoding".to_vec() };
        let mut diffs: Option<Vec<Obj>> = None;
        match &enc {
            Some(Obj::Name(n)) => base_name = n.clone(),
            Some(Obj::Dict(d)) => {
                if let Some(n) = d.get("BaseEncoding").and_then(|o| o.name()) {
                    base_name = n.to_vec();
                }
                if let Obj::Arr(a) = pdf.resolve(d.get("Differences").unwrap_or(&Obj::Null)) {
                    diffs = Some(a);
                }
            }
            _ => {}
        }
        if lower.starts_with("symbol") || lower.starts_with("zapfdingbats") || lower.starts_with("wingdings") {
            if enc.is_none() {
                f.symbol_name = Some(base.clone());
            }
        }
        f.simple = base_encoding(&base_name).into_iter().map(|c| c.map(|c| c.to_string())).collect();
        if let Some(d) = diffs {
            let mut code = 0usize;
            for o in d {
                match o {
                    Obj::Num(n) => code = n.clamp(0.0, 1e6) as usize,
                    Obj::Name(n) => {
                        if code < 256 {
                            f.simple[code] = glyph_name(&n);
                        }
                        code += 1;
                    }
                    _ => {}
                }
            }
        }
        let first = fd.get("FirstChar").map(|o| pdf.resolve(o)).and_then(|o| o.num()).unwrap_or(0.0).clamp(0.0, 255.0) as u32;
        if let Obj::Arr(w) = pdf.resolve(fd.get("Widths").unwrap_or(&Obj::Null)) {
            for (k, v) in w.iter().enumerate().take(256) {
                if let Some(v) = pdf.resolve(v).num() {
                    f.widths.insert(first + k as u32, v);
                }
            }
        }
        if let Some(d) = &desc {
            if let Some(mw) = d.get("MissingWidth").map(|o| pdf.resolve(o)).and_then(|o| o.num()) {
                if mw > 0.0 {
                    f.dw = mw;
                }
            }
        }
        if sub == b"Type3" {
            if let Obj::Arr(m) = pdf.resolve(fd.get("FontMatrix").unwrap_or(&Obj::Null)) {
                if let Some(a) = m.first().and_then(|o| o.num()) {
                    if a.abs() > 1e-9 && a.abs() < 10.0 {
                        f.scale = a.abs();
                    }
                }
            }
        }
    }
    if let Some(Obj::Stream(d, raw)) = fd.get("ToUnicode").map(|o| pdf.resolve(o)) {
        if let Some(data) = decode_stream(&d, &raw, pdf) {
            parse_cmap(&data, &mut f);
        }
    }
    f
}

// ===========================================================================
// Content interpreter

type Mat = [f64; 6];
const IDENT: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(a: &Mat, b: &Mat) -> Mat {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

#[derive(Clone, Debug)]
struct Glyph {
    x: f64,
    y: f64,
    /// writing direction (unit vector, device space)
    dx: f64,
    dy: f64,
    adv: f64,
    size: f64,
    text: String,
    bold: bool,
    italic: bool,
}

#[derive(Clone)]
struct GState {
    ctm: Mat,
    font: Option<Rc<Font>>,
    size: f64,
    tc: f64,
    tw: f64,
    tz: f64,
    tl: f64,
    rise: f64,
}

struct Interp<'p, 'a> {
    pdf: &'p Pdf<'a>,
    fonts: HashMap<u32, Rc<Font>>,
    glyphs: Vec<Glyph>,
    ops: usize,
}

impl<'p, 'a> Interp<'p, 'a> {
    fn font(&mut self, res: &Dict, name: &[u8]) -> Option<Rc<Font>> {
        let fonts = self.pdf.rdict(res.get("Font"))?;
        let entry = fonts.0.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())?;
        if let Obj::Ref(r) = entry {
            if let Some(f) = self.fonts.get(&r) {
                return Some(f.clone());
            }
            let d = self.pdf.resolve(&entry).dict().cloned()?;
            let f = Rc::new(load_font(self.pdf, &d));
            self.fonts.insert(r, f.clone());
            return Some(f);
        }
        let d = self.pdf.resolve(&entry).dict().cloned()?;
        Some(Rc::new(load_font(self.pdf, &d)))
    }

    fn run(&mut self, content: &[u8], res: &Dict, ctm: Mat, depth: u32) {
        if depth > 12 {
            return;
        }
        let mut gs = GState { ctm, font: None, size: 0.0, tc: 0.0, tw: 0.0, tz: 1.0, tl: 0.0, rise: 0.0 };
        let mut stack: Vec<GState> = vec![];
        let mut tm = IDENT;
        let mut tlm = IDENT;
        let mut args: Vec<Obj> = vec![];
        let mut l = Lex::new(content, 0);
        loop {
            if self.ops >= MAX_OPS || self.glyphs.len() >= MAX_GLYPHS {
                return;
            }
            let start = l.pos;
            let Some(o) = l.obj(0, false) else { break };
            if l.pos == start {
                l.pos += 1;
                continue;
            }
            let Obj::Kw(op) = o else {
                if args.len() < 64 {
                    args.push(o);
                }
                continue;
            };
            self.ops += 1;
            let n = |i: usize| args.get(i).and_then(|o| o.num()).unwrap_or(0.0);
            match op.as_slice() {
                b"q" => {
                    if stack.len() < 256 {
                        stack.push(gs.clone());
                    }
                }
                b"Q" => {
                    if let Some(g) = stack.pop() {
                        gs = g;
                    }
                }
                b"cm" if args.len() >= 6 => gs.ctm = mul(&[n(0), n(1), n(2), n(3), n(4), n(5)], &gs.ctm),
                b"BT" => {
                    tm = IDENT;
                    tlm = IDENT;
                }
                b"Tf" => {
                    if let Some(name) = args.first().and_then(|o| o.name()).map(|n| n.to_vec()) {
                        gs.font = self.font(res, &name);
                    }
                    gs.size = n(1);
                }
                b"Tc" => gs.tc = n(0),
                b"Tw" => gs.tw = n(0),
                b"Tz" => gs.tz = n(0) / 100.0,
                b"TL" => gs.tl = n(0),
                b"Ts" => gs.rise = n(0),
                b"Td" => {
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, n(0), n(1)], &tlm);
                    tm = tlm;
                }
                b"TD" => {
                    gs.tl = -n(1);
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, n(0), n(1)], &tlm);
                    tm = tlm;
                }
                b"Tm" if args.len() >= 6 => {
                    tlm = [n(0), n(1), n(2), n(3), n(4), n(5)];
                    tm = tlm;
                }
                b"T*" => {
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                    tm = tlm;
                }
                b"Tj" => {
                    if let Some(Obj::Str(s)) = args.first() {
                        self.show(s, &gs, &mut tm);
                    }
                }
                b"'" => {
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                    tm = tlm;
                    if let Some(Obj::Str(s)) = args.first() {
                        self.show(s, &gs, &mut tm);
                    }
                }
                b"\"" => {
                    gs.tw = n(0);
                    gs.tc = n(1);
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                    tm = tlm;
                    if let Some(Obj::Str(s)) = args.get(2) {
                        self.show(s, &gs, &mut tm);
                    }
                }
                b"TJ" => {
                    if let Some(Obj::Arr(a)) = args.first() {
                        for e in a {
                            match e {
                                Obj::Str(s) => self.show(s, &gs, &mut tm),
                                Obj::Num(v) if v.is_finite() => {
                                    let tx = -v / 1000.0 * gs.size * gs.tz;
                                    tm = mul(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], &tm);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                b"Do" => {
                    if let Some(name) = args.first().and_then(|o| o.name()) {
                        if let Some(xo) = self.pdf.rdict(res.get("XObject")) {
                            if let Some((_, r)) = xo.0.iter().find(|(k, _)| k == name) {
                                if let Obj::Stream(d, raw) = self.pdf.resolve(r) {
                                    if d.get("Subtype").and_then(|o| o.name()) == Some(b"Form") {
                                        let m = match d.get("Matrix").map(|o| self.pdf.resolve(o)) {
                                            Some(Obj::Arr(a)) if a.len() >= 6 => {
                                                let g = |i: usize| a[i].num().unwrap_or(0.0);
                                                [g(0), g(1), g(2), g(3), g(4), g(5)]
                                            }
                                            _ => IDENT,
                                        };
                                        let sub_res = self.pdf.rdict(d.get("Resources")).unwrap_or_else(|| res.clone());
                                        if let Some(data) = decode_stream(&d, &raw, self.pdf) {
                                            self.run(&data, &sub_res, mul(&m, &gs.ctm), depth + 1);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                b"BI" => {
                    // inline image: skip to EI
                    let mut p = l.pos;
                    if let Some(id) = find(content, b"ID", p) {
                        p = id + 2;
                        while let Some(e) = find(content, b"EI", p) {
                            let before = e == 0 || is_ws(content[e - 1]);
                            let after = content.get(e + 2).is_none_or(|&c| is_ws(c));
                            p = e + 2;
                            if before && after {
                                break;
                            }
                        }
                        l.pos = p.min(content.len());
                    } else {
                        l.pos = content.len();
                    }
                }
                _ => {}
            }
            args.clear();
        }
    }

    fn show(&mut self, s: &[u8], gs: &GState, tm: &mut Mat) {
        let Some(font) = gs.font.clone() else { return };
        for (code, nb) in font.codes(s) {
            let w = font.width(code);
            let trm = mul(&mul(&[gs.size * gs.tz, 0.0, 0.0, gs.size, 0.0, gs.rise], tm), &gs.ctm);
            let mut tx = (w * gs.size + gs.tc) * gs.tz;
            if nb == 1 && code == 32 {
                tx += gs.tw * gs.tz;
            }
            let m = mul(tm, &gs.ctm);
            let (ax, ay) = (tx * m[0], tx * m[1]);
            let adv = (ax * ax + ay * ay).sqrt();
            let dl = (trm[0] * trm[0] + trm[1] * trm[1]).sqrt();
            let size = (trm[2] * trm[2] + trm[3] * trm[3]).sqrt();
            if let Some(t) = font.text(code) {
                if dl > 1e-9 && trm[4].is_finite() && trm[5].is_finite() && size.is_finite() && adv.is_finite() && !t.is_empty() {
                    self.glyphs.push(Glyph { x: trm[4], y: trm[5], dx: trm[0] / dl, dy: trm[1] / dl, adv, size, text: t, bold: font.bold, italic: font.italic });
                }
            }
            *tm = mul(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], tm);
        }
    }
}

// ===========================================================================
// Layout: glyphs → lines → columns → paragraphs

#[derive(Clone, Debug)]
struct Item {
    text: String,
    bold: bool,
    italic: bool,
    sup: bool,
}

#[derive(Clone, Debug)]
struct Line {
    glyphs: Vec<Glyph>,
    y: f64,
    size: f64,
    x0: f64,
    x1: f64,
}

fn is_space(t: &str) -> bool {
    t.chars().all(|c| c.is_whitespace())
}

/// Normalise a page: rotate so the dominant writing direction is +x and
/// drop glyphs running in other directions.
fn normalise(mut gl: Vec<Glyph>) -> Vec<Glyph> {
    let (sx, sy) = gl.iter().fold((0.0, 0.0), |(a, b), g| (a + g.dx, b + g.dy));
    let len = (sx * sx + sy * sy).sqrt();
    if len < 1e-9 {
        return gl;
    }
    let (cx, cy) = (sx / len, sy / len);
    gl.retain(|g| g.dx * cx + g.dy * cy > 0.85);
    if (cx - 1.0).abs() > 1e-6 {
        for g in gl.iter_mut() {
            // rotate by -angle: x' = x*c + y*s, y' = -x*s + y*c
            let (x, y) = (g.x * cx + g.y * cy, -g.x * cy + g.y * cx);
            g.x = x;
            g.y = y;
        }
    }
    gl
}

fn group_lines(mut gl: Vec<Glyph>) -> Vec<Line> {
    gl.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<Line> = vec![];
    for g in gl {
        let mut placed = false;
        let k0 = lines.len().saturating_sub(4);
        for line in lines[k0..].iter_mut().rev() {
            let tol = 0.45 * line.size.max(g.size).max(1.0);
            if (line.y - g.y).abs() <= tol {
                if g.size > line.size * 1.05 && !is_space(&g.text) {
                    line.size = g.size;
                    line.y = g.y;
                }
                line.glyphs.push(g.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            let size = g.size;
            let y = g.y;
            lines.push(Line { glyphs: vec![g], y, size, x0: 0.0, x1: 0.0 });
        }
    }
    for l in lines.iter_mut() {
        finish_line(l);
    }
    lines.retain(|l| !l.glyphs.is_empty());
    lines
}

fn finish_line(l: &mut Line) {
    l.glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
    // drop overprinted duplicates (fake bold / shadow)
    let mut out: Vec<Glyph> = Vec::with_capacity(l.glyphs.len());
    for g in l.glyphs.drain(..) {
        if let Some(p) = out.iter().rev().take(3).find(|p| p.text == g.text && (p.x - g.x).abs() < 0.15 * g.size.max(1.0) && (p.y - g.y).abs() < 0.15 * g.size.max(1.0)) {
            let _ = p;
            continue;
        }
        out.push(g);
    }
    l.glyphs = out;
    let ink: Vec<&Glyph> = l.glyphs.iter().filter(|g| !is_space(&g.text)).collect();
    if ink.is_empty() {
        l.glyphs.clear();
        return;
    }
    l.x0 = ink.iter().map(|g| g.x).fold(f64::MAX, f64::min);
    l.x1 = ink.iter().map(|g| g.x + g.adv).fold(f64::MIN, f64::max);
    // baseline/size of the dominant (largest) text
    let maxs = ink.iter().map(|g| g.size).fold(0.0, f64::max);
    let big: Vec<&&Glyph> = ink.iter().filter(|g| g.size >= maxs * 0.9).collect();
    l.size = maxs;
    // most common baseline among the large glyphs
    let mut ys: Vec<f64> = big.iter().map(|g| g.y).collect();
    ys.sort_by(|a, b| a.total_cmp(b));
    l.y = ys.get(ys.len() / 2).copied().unwrap_or(l.y);
}

/// Items of a line: text runs with spaces/tabs from the geometry.
fn line_items(l: &Line) -> Vec<Item> {
    let mut out: Vec<Item> = vec![];
    let mut prev_end: Option<f64> = None;
    let mut prev_space = true;
    for g in &l.glyphs {
        let size = l.size.max(g.size).max(1.0);
        if is_space(&g.text) {
            if !prev_space {
                out.push(Item { text: " ".into(), bold: g.bold, italic: g.italic, sup: false });
                prev_space = true;
            }
            prev_end = Some(g.x + g.adv);
            continue;
        }
        if let Some(pe) = prev_end {
            let gap = g.x - pe;
            if gap > 2.5 * size {
                if prev_space {
                    if let Some(last) = out.last_mut() {
                        if last.text == " " {
                            last.text = "\t".into();
                        }
                    }
                } else {
                    out.push(Item { text: "\t".into(), bold: false, italic: false, sup: false });
                }
            } else if gap > 0.18 * size && !prev_space {
                out.push(Item { text: " ".into(), bold: g.bold, italic: g.italic, sup: false });
            }
        }
        let sup = g.y - l.y > 0.2 * l.size && g.size < 0.85 * l.size;
        out.push(Item { text: g.text.clone(), bold: g.bold, italic: g.italic, sup });
        prev_space = false;
        prev_end = Some(g.x + g.adv);
    }
    while out.last().is_some_and(|i| i.text == " " || i.text == "\t") {
        out.pop();
    }
    out
}

/// Split a page's lines into reading-order blocks: two-column regions are
/// read column by column.
fn columns(lines: Vec<Line>) -> Vec<(u32, Line)> {
    let mut out: Vec<(u32, Line)> = vec![];
    if lines.len() < 10 {
        return lines.into_iter().map(|l| (0, l)).collect();
    }
    let minx = lines.iter().map(|l| l.x0).fold(f64::MAX, f64::min);
    let maxx = lines.iter().map(|l| l.x1).fold(f64::MIN, f64::max);
    let w = maxx - minx;
    if w <= 1.0 || w > 1e5 {
        return lines.into_iter().map(|l| (0, l)).collect();
    }
    let nb = (w / 2.0).ceil() as usize + 1;
    let mut cover = vec![0usize; nb];
    for l in &lines {
        let mut hit = vec![false; nb];
        for g in l.glyphs.iter().filter(|g| !is_space(&g.text)) {
            let a = ((g.x - minx) / 2.0).max(0.0) as usize;
            let b = ((g.x + g.adv - minx) / 2.0).max(0.0) as usize;
            for h in hit.iter_mut().take(b.min(nb - 1) + 1).skip(a.min(nb - 1)) {
                *h = true;
            }
        }
        for (c, h) in cover.iter_mut().zip(hit) {
            if h {
                *c += 1;
            }
        }
    }
    let limit = lines.len() / 20;
    let (lo, hi) = (nb / 4, nb * 3 / 4);
    let mut best: Option<(usize, usize)> = None;
    let mut i = lo;
    while i < hi {
        if cover[i] <= limit {
            let s = i;
            while i < hi && cover[i] <= limit {
                i += 1;
            }
            if best.is_none_or(|(a, b)| i - s > b - a) {
                best = Some((s, i));
            }
        } else {
            i += 1;
        }
    }
    let Some((a, b)) = best else { return lines.into_iter().map(|l| (0, l)).collect() };
    let avg_size = lines.iter().map(|l| l.size).sum::<f64>() / lines.len() as f64;
    if ((b - a) as f64) * 2.0 < avg_size.max(6.0) {
        return lines.into_iter().map(|l| (0, l)).collect();
    }
    let gx = minx + (a + b) as f64; // centre of the gutter (buckets are 2 units)
    // both sides must hold dense text on many lines
    let both: Vec<&Line> = lines.iter().filter(|l| l.x0 < gx && l.x1 > gx).collect();
    let dense = |l: &Line, left: bool| -> f64 {
        let w: f64 = l.glyphs.iter().filter(|g| (g.x < gx) == left && !is_space(&g.text)).map(|g| g.adv).sum();
        w / if left { gx - minx } else { maxx - gx }.max(1.0)
    };
    let mut dl: Vec<f64> = both.iter().map(|l| dense(l, true)).collect();
    let mut dr: Vec<f64> = both.iter().map(|l| dense(l, false)).collect();
    if both.len() * 10 < lines.len() * 4 || both.len() < 5 {
        return lines.into_iter().map(|l| (0, l)).collect();
    }
    dl.sort_by(|a, b| a.total_cmp(b));
    dr.sort_by(|a, b| a.total_cmp(b));
    if dl[dl.len() / 2] < 0.6 || dr[dr.len() / 2] < 0.6 {
        return lines.into_iter().map(|l| (0, l)).collect();
    }
    // regions: runs of lines that do not cross the gutter
    let crosses = |l: &Line| l.glyphs.iter().any(|g| !is_space(&g.text) && g.x < gx && g.x + g.adv > gx);
    let mut region = 0u32;
    let mut left: Vec<Line> = vec![];
    let mut right: Vec<Line> = vec![];
    let flush = |left: &mut Vec<Line>, right: &mut Vec<Line>, out: &mut Vec<(u32, Line)>, region: &mut u32| {
        if !left.is_empty() {
            *region += 1;
            out.extend(left.drain(..).map(|l| (*region, l)));
        }
        if !right.is_empty() {
            *region += 1;
            out.extend(right.drain(..).map(|l| (*region, l)));
        }
    };
    for l in lines {
        if crosses(&l) {
            flush(&mut left, &mut right, &mut out, &mut region);
            region += 1;
            out.push((region, l));
            continue;
        }
        let (mut a, mut b) = (l.clone(), l);
        a.glyphs.retain(|g| g.x < gx);
        b.glyphs.retain(|g| g.x >= gx);
        finish_line(&mut a);
        finish_line(&mut b);
        if !a.glyphs.is_empty() {
            left.push(a);
        }
        if !b.glyphs.is_empty() {
            right.push(b);
        }
    }
    flush(&mut left, &mut right, &mut out, &mut region);
    out
}

struct PLine {
    items: Vec<Item>,
    region: u32,
    y: f64,
    size: f64,
    x0: f64,
    x1: f64,
    first_word: f64,
}

/// Paragraphs of one page.
fn paragraphs(lines: Vec<(u32, Line)>) -> Vec<Vec<Item>> {
    let pl: Vec<PLine> = lines
        .into_iter()
        .map(|(region, l)| {
            let mut fw = 0.0;
            for g in &l.glyphs {
                if is_space(&g.text) {
                    if fw > 0.0 {
                        break;
                    }
                } else {
                    fw += g.adv;
                }
            }
            PLine { items: line_items(&l), region, y: l.y, size: l.size, x0: l.x0, x1: l.x1, first_word: fw }
        })
        .filter(|p| !p.items.is_empty())
        .collect();
    // geometry per region
    let mut reg_left: HashMap<u32, f64> = HashMap::new();
    let mut reg_right: HashMap<u32, Vec<f64>> = HashMap::new();
    for p in &pl {
        let e = reg_left.entry(p.region).or_insert(f64::MAX);
        *e = e.min(p.x0);
        reg_right.entry(p.region).or_default().push(p.x1);
    }
    let reg_right: HashMap<u32, f64> = reg_right
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(|a, b| a.total_cmp(b));
            // 90th percentile = the usual right margin
            (k, v[((v.len() - 1) * 9) / 10])
        })
        .collect();
    // typical line spacing
    let mut gaps: Vec<f64> = pl.windows(2).filter(|w| w[0].region == w[1].region).map(|w| w[0].y - w[1].y).filter(|g| *g > 0.0).collect();
    gaps.sort_by(|a, b| a.total_cmp(b));
    let typical = gaps.get(gaps.len() / 3).copied();

    let mut out: Vec<Vec<Item>> = vec![];
    let mut cur: Vec<Item> = vec![];
    for (i, p) in pl.iter().enumerate() {
        let brk = if i == 0 {
            true
        } else {
            let q = &pl[i - 1];
            let size = q.size.max(p.size).max(1.0);
            let lh = typical.filter(|t| *t > 0.8 * size && *t < 2.0 * size).unwrap_or(1.25 * size);
            let vgap = q.y - p.y;
            let left = reg_left.get(&p.region).copied().unwrap_or(p.x0);
            let right = reg_right.get(&p.region).copied().unwrap_or(p.x1);
            let has_tab = |l: &PLine| l.items.iter().any(|it| it.text == "\t");
            let starts_bullet = p.items.first().is_some_and(|it| it.text.starts_with(['•', '▪', '●', '◦', '■', '–', '-']) && it.text.chars().count() == 1);
            // the next column of a two-column block continues the paragraph
            // when the previous column ended with a full line
            let col_cont = p.region == q.region + 1 && p.y > q.y;
            let q_right = reg_right.get(&q.region).copied().unwrap_or(q.x1);
            if col_cont {
                (q.size - p.size).abs() > 0.15 * size || has_tab(q) || has_tab(p) || starts_bullet || q.x1 + 0.3 * size + p.first_word < q_right - 0.5 * size
            } else {
                p.region != q.region
                    || vgap <= 0.2 * size
                    || vgap > 1.3 * lh
                    || (q.size - p.size).abs() > 0.15 * size
                    || has_tab(q)
                    || has_tab(p)
                    || starts_bullet
                    || (p.x0 > q.x0 + 1.0 * size && (q.x0 - left).abs() < 0.5 * size && q.x1 < right - 2.0 * size)
                    || q.x1 + 0.3 * size + p.first_word < right - 0.5 * size
            }
        };
        if brk {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur = p.items.clone();
            continue;
        }
        // join: de-hyphenate "Wort-" + "ende"
        let last = cur.iter().rev().find(|it| !it.text.is_empty()).map(|it| it.text.clone()).unwrap_or_default();
        let next_lower = p.items.first().and_then(|it| it.text.chars().next()).is_some_and(|c| c.is_lowercase());
        let before_hyphen_letter = {
            let all: String = cur.iter().rev().take(3).map(|i| i.text.as_str()).collect::<Vec<_>>().into_iter().rev().collect();
            let mut cs = all.chars().rev();
            let h = cs.next();
            matches!(h, Some('-') | Some('\u{ad}')) && cs.next().is_some_and(|c| c.is_alphabetic())
        };
        if before_hyphen_letter && next_lower {
            if let Some(it) = cur.iter_mut().rev().find(|it| !it.text.is_empty()) {
                it.text.pop();
            }
        } else if last.ends_with('\u{ad}') {
            if let Some(it) = cur.iter_mut().rev().find(|it| !it.text.is_empty()) {
                it.text.pop();
            }
        } else {
            let b = p.items.first().map(|i| (i.bold, i.italic)).unwrap_or((false, false));
            cur.push(Item { text: " ".into(), bold: b.0, italic: b.1, sup: false });
        }
        cur.extend(p.items.iter().cloned());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ===========================================================================

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    if !bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
        return Err("not a PDF file".into());
    }
    let pdf = Pdf::open(bytes)?;
    let pages = pdf.pages();
    if pages.is_empty() {
        return Err("damaged PDF file (no pages)".into());
    }
    let base = FontSpec { name: "Times New Roman".into(), size_cpt: 1200 };
    let mut b = crate::fmt_build::Builder::new(base.clone());
    let mut it = Interp { pdf: &pdf, fonts: HashMap::new(), glyphs: vec![], ops: 0 };
    let mut any_text = false;
    let mut first_page_size: Option<(f64, f64)> = None;
    for (pi, (page, res, _rot)) in pages.iter().enumerate() {
        if first_page_size.is_none() {
            if let Obj::Arr(mb) = pdf.resolve(page.get("MediaBox").unwrap_or(&Obj::Null)) {
                let v: Vec<f64> = mb.iter().filter_map(|o| pdf.resolve(o).num()).collect();
                if v.len() == 4 {
                    first_page_size = Some(((v[2] - v[0]).abs(), (v[3] - v[1]).abs()));
                }
            }
        }
        let mut content: Vec<u8> = vec![];
        let streams: Vec<Obj> = match pdf.resolve(page.get("Contents").unwrap_or(&Obj::Null)) {
            Obj::Arr(a) => a.iter().map(|o| pdf.resolve(o)).collect(),
            o => vec![o],
        };
        for s in streams {
            if let Obj::Stream(d, raw) = s {
                if let Some(data) = decode_stream(&d, &raw, &pdf) {
                    if content.len() as u64 + data.len() as u64 > MAX_STREAM * 2 {
                        break;
                    }
                    content.extend_from_slice(&data);
                    content.push(b'\n');
                }
            }
        }
        it.glyphs.clear();
        it.run(&content, res, IDENT, 0);
        let glyphs = normalise(std::mem::take(&mut it.glyphs));
        if pi > 0 {
            b.page();
        }
        if glyphs.is_empty() {
            continue;
        }
        any_text = true;
        let lines = group_lines(glyphs);
        let paras = paragraphs(columns(lines));
        for p in paras {
            for item in p {
                b.set(Attr::Bold, item.bold);
                b.set(Attr::Italic, item.italic);
                b.set(Attr::Superscript, item.sup);
                b.text(&item.text);
            }
            b.set(Attr::Superscript, false);
            b.para();
        }
        if it.ops >= MAX_OPS {
            break;
        }
    }
    if !any_text {
        return Err("This PDF contains no text (probably scanned pages) — nothing to import".into());
    }
    let mut d = b.into_document(base);
    if let Some((w, h)) = first_page_size {
        if w > 72.0 && h > 72.0 && w < 14400.0 && h < 14400.0 {
            d.init.page_width = (w * 1200.0 / 72.0).round() as i32;
            d.init.page_height = (h * 1200.0 / 72.0).round() as i32;
        }
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a PDF from object bodies (1-based numbering) with a correct xref.
    fn build_pdf(objs: &[String]) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = vec![];
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
        }
        let x = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offs {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }

    fn stream(s: &str) -> String {
        format!("<< /Length {} >>\nstream\n{}\nendstream", s.len(), s)
    }

    fn text(d: &Document) -> String {
        crate::fmt_build::debug_text(&d.els)
    }

    #[test]
    fn type1_winansi_differences() {
        // ä = 0xE4 in WinAnsi; code 0x80 remapped to "afii10017" (А), 0x81 to "germandbls"
        let content = "BT /F1 12 Tf 72 720 Td (Gr\\374\\337e aus K\\366ln \\200\\201) Tj 0 -14 Td (zweite Zeile) Tj ET \
                       BT /F2 12 Tf 72 600 Td (Neuer Absatz fett) Tj ET";
        let objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> /Contents 4 0 R >>".to_string(),
            stream(content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding << /BaseEncoding /WinAnsiEncoding /Differences [128 /afii10017 /germandbls] >> >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_string(),
        ];
        let d = read(&build_pdf(&objs)).unwrap();
        assert_eq!(text(&d), "Grüße aus Köln Аß zweite Zeile\nNeuer Absatz fett");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert_eq!(d.init.page_width, 595 * 1200 / 72 + 1);
    }

    #[test]
    fn identity_h_tounicode_two_pages() {
        let cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap 1 begincodespacerange <0000> <FFFF> endcodespacerange \
                    2 beginbfchar <0001> <041F> <0002> <0440> endbfchar 1 beginbfrange <0003> <0005> <0438> endbfrange endcmap end end";
        // codes: 1=П 2=р 3=и 4=й 5=к
        let c1 = "BT /F1 12 Tf 1 0 0 1 72 720 Tm <00010002000300050003> Tj ET";
        let c2 = "BT /F1 12 Tf 72 720 Td [<0001>-200<0002>] TJ ET";
        let objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 8 0 R] /Count 2 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
            stream(c1),
            "<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+DejaVuSans /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>".to_string(),
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /DejaVuSans /DW 600 /W [1 [700 600] 3 5 550] >>".to_string(),
            stream(cmap),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 9 0 R >>".to_string(),
            stream(c2),
        ];
        let d = read(&build_pdf(&objs)).unwrap();
        assert_eq!(text(&d), "Прики\n[HPg]\nП р");
    }

    #[test]
    fn encrypted_is_rejected() {
        let mut pdf = build_pdf(&["<< /Type /Catalog /Pages 2 0 R >>".into(), "<< /Type /Pages /Kids [] /Count 0 >>".into()]);
        let s = String::from_utf8_lossy(&pdf).replace("/Root 1 0 R", "/Root 1 0 R /Encrypt << /Filter /Standard >>");
        pdf = s.into_bytes();
        let e = read(&pdf).unwrap_err();
        assert!(e.contains("encrypted"), "{e}");
    }

    #[test]
    fn broken_xref_is_rebuilt() {
        let content = "BT /F1 12 Tf 72 720 Td (Hallo Welt) Tj ET";
        let objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
            stream(content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".to_string(),
        ];
        let mut pdf = build_pdf(&objs);
        // destroy the xref offsets and the startxref
        let s = String::from_utf8_lossy(&pdf).replace("startxref", "startxrex").replace(" 00000 n", " 00000 x");
        pdf = s.into_bytes();
        let d = read(&pdf).unwrap();
        assert_eq!(text(&d), "Hallo Welt");
    }

    #[test]
    fn filters() {
        assert_eq!(ascii85(b"87cURD]i,\"Ebo80~>"), b"Hello World!");
        assert_eq!(runlength(&[2, b'a', b'b', b'c', 254, b'x', 128]), b"abcxxx");
        assert_eq!(glyph_name(b"adieresis").as_deref(), Some("ä"));
        assert_eq!(glyph_name(b"afii10071").as_deref(), Some("ё"));
        assert_eq!(glyph_name(b"uni0416").as_deref(), Some("Ж"));
        assert_eq!(glyph_name(b"f_i").as_deref(), Some("fi"));
        // LZW example from the PDF reference
        let lzw_in = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
        assert_eq!(lzw(&lzw_in, true), vec![45, 45, 45, 45, 45, 65, 45, 45, 45, 66]);
    }

    fn sample() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/sample.pdf")).unwrap()
    }

    #[test]
    fn libreoffice_sample() {
        let d = read(&sample()).unwrap();
        let t = text(&d);
        assert!(t.contains("Grüße aus Köln: Äpfel, Öl und Übermaß für die Bäckerei. Dieser Absatz ist absichtlich recht lang, damit er über mehrere Zeilen umbrochen wird und die Absatzerkennung geprüft werden kann.\n"), "{t}");
        assert!(t.contains("Привет, мир! Это проверка русского текста. fett und kursiv Text."), "{t}");
        assert!(t.contains("A1\tB1"), "{t}");
        assert!(t.contains("Nach der Tabelle."), "{t}");
        assert!(t.contains("Eine Fußnote."), "{t}");
        assert!(t.contains("[HPg]\nZweite Seite."), "{t}");
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Bold))));
        assert!(d.els.contains(&El::Code(Code::AttrOn(Attr::Italic))));
    }

    #[test]
    fn two_columns() {
        let b = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/formats/columns.pdf")).unwrap();
        let d = read(&b).unwrap();
        let t = text(&d);
        let paras: Vec<&str> = t.split('\n').collect();
        assert_eq!(paras.len(), 12, "{t}");
        for (i, p) in paras.iter().enumerate() {
            assert!(p.starts_with(&format!("Absatz {} beginnt hier. Dieser Text steht in zwei Spalten", i + 1)), "{p}");
            assert!(p.ends_with("der richtigen Reihenfolge gelesen werden."), "{p}");
        }
    }

    #[test]
    fn never_panics_on_damaged_files() {
        let s = sample();
        let mut lens: Vec<usize> = (0..s.len()).step_by(997).collect();
        lens.extend([0, 5, 9, 100, s.len() - 1]);
        for n in lens {
            let _ = read(&s[..n]);
        }
        let mut seed = 12345u32;
        for _ in 0..60 {
            let mut d = s.clone();
            for _ in 0..20 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let i = (seed as usize >> 4) % d.len();
                d[i] = (seed >> 24) as u8;
            }
            let _ = read(&d);
        }
        let _ = read(b"%PDF-1.4\n1 0 obj << /Kids [1 0 R] /Pages 1 0 R /Type /Catalog >> endobj trailer << /Root 1 0 R >>");
        let _ = read(b"%PDF-1.4 [[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[");
    }
}
