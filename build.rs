//! Embed the program icon and version information into bsw51.exe so that
//! Explorer, the taskbar and Task Manager show them.
//!
//! The toolchain has no resource compiler (windres/rc), so this writes the
//! COFF object with a `.rsrc` section itself — the same thing windres would
//! produce — and passes it to the linker.

use std::path::PathBuf;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const RT_VERSION: u32 = 16;
const LANG: u32 = 0x0409;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/bsw51.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = std::fs::read("assets/bsw51.ico").expect("assets/bsw51.ico (run tools/make_icon.py)");
    let mut res: Vec<(u32, u32, Vec<u8>)> = vec![]; // (type, id, data)

    // .ico -> one RT_ICON per image + the RT_GROUP_ICON directory
    let n = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let mut group = vec![0, 0, 1, 0];
    group.extend_from_slice(&(n as u16).to_le_bytes());
    for i in 0..n {
        let e = &ico[6 + 16 * i..6 + 16 * (i + 1)];
        let size = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
        let off = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
        let id = (i + 1) as u32;
        res.push((RT_ICON, id, ico[off..off + size].to_vec()));
        group.extend_from_slice(&e[..12]); // w h colors reserved planes bpp bytes
        group.extend_from_slice(&(id as u16).to_le_bytes());
    }
    res.push((RT_GROUP_ICON, 1, group));
    res.push((RT_VERSION, 1, version_info()));

    let obj = coff_rsrc(&res);
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bsw51_res.o");
    std::fs::write(&out, obj).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

// ------------------------------------------------------------ version info

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect()
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

/// One VS_VERSIONINFO-style block: wLength, wValueLength, wType, key, value, children.
fn block(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0, 0];
    b.extend_from_slice(&value_len.to_le_bytes());
    b.extend_from_slice(&(text as u16).to_le_bytes());
    b.extend_from_slice(&utf16z(key));
    pad4(&mut b);
    b.extend_from_slice(value);
    for c in children {
        pad4(&mut b);
        b.extend_from_slice(c);
    }
    let len = b.len() as u16;
    b[..2].copy_from_slice(&len.to_le_bytes());
    b
}

fn version_info() -> Vec<u8> {
    let ver = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".into());
    let p: Vec<u16> = ver.split('.').map(|x| x.parse().unwrap_or(0)).chain([0, 0, 0, 0]).take(4).collect();
    let (ms, ls) = (((p[0] as u32) << 16) | p[1] as u32, ((p[2] as u32) << 16) | p[3] as u32);
    let mut fixed = vec![];
    for v in [0xFEEF_04BDu32, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0] {
        fixed.extend_from_slice(&v.to_le_bytes());
    }
    let s = |k: &str, v: &str| block(k, &utf16z(v), v.encode_utf16().count() as u16 + 1, true, &[]);
    let strings = [
        s("CompanyName", "Blue Screen Writer 51"),
        s("FileDescription", "Blue Screen Writer 51"),
        s("FileVersion", &ver),
        s("InternalName", "bsw51"),
        s("OriginalFilename", "bsw51.exe"),
        s("ProductName", "Blue Screen Writer 51"),
        s("ProductVersion", &ver),
    ];
    let table = block("040904b0", &[], 0, true, &strings);
    let sfi = block("StringFileInfo", &[], 0, true, &[table]);
    let var = block("Translation", &[0x09, 0x04, 0xB0, 0x04], 4, false, &[]);
    let vfi = block("VarFileInfo", &[], 0, true, &[var]);
    block("VS_VERSION_INFO", &fixed, fixed.len() as u16, false, &[sfi, vfi])
}

// ------------------------------------------------------- COFF .rsrc object

fn coff_rsrc(res: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    // types sorted, ids sorted within a type
    let mut types: Vec<u32> = res.iter().map(|r| r.0).collect();
    types.sort();
    types.dedup();
    let ids_of = |t: u32| {
        let mut v: Vec<usize> = (0..res.len()).filter(|&i| res[i].0 == t).collect();
        v.sort_by_key(|&i| res[i].1);
        v
    };
    let dir = |n: usize| 16 + 8 * n as u32;

    // layout: root, type dirs, language dirs, data entries, data
    let mut off = dir(types.len());
    let mut type_off = vec![];
    for &t in &types {
        type_off.push(off);
        off += dir(ids_of(t).len());
    }
    let mut lang_off = vec![0u32; res.len()];
    for &t in &types {
        for i in ids_of(t) {
            lang_off[i] = off;
            off += dir(1);
        }
    }
    let mut entry_off = vec![0u32; res.len()];
    for &t in &types {
        for i in ids_of(t) {
            entry_off[i] = off;
            off += 16;
        }
    }
    let mut data_off = vec![0u32; res.len()];
    for &t in &types {
        for i in ids_of(t) {
            off = (off + 7) & !7;
            data_off[i] = off;
            off += res[i].2.len() as u32;
        }
    }

    let mut sec = vec![0u8; ((off + 7) & !7) as usize];
    let put = |sec: &mut Vec<u8>, at: u32, v: u32| sec[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    let header = |sec: &mut Vec<u8>, at: u32, n: usize| {
        sec[at as usize + 14..at as usize + 16].copy_from_slice(&(n as u16).to_le_bytes());
    };
    const SUBDIR: u32 = 0x8000_0000;
    header(&mut sec, 0, types.len());
    for (k, &t) in types.iter().enumerate() {
        put(&mut sec, 16 + 8 * k as u32, t);
        put(&mut sec, 20 + 8 * k as u32, SUBDIR | type_off[k]);
        let ids = ids_of(t);
        header(&mut sec, type_off[k], ids.len());
        for (j, &i) in ids.iter().enumerate() {
            put(&mut sec, type_off[k] + 16 + 8 * j as u32, res[i].1);
            put(&mut sec, type_off[k] + 20 + 8 * j as u32, SUBDIR | lang_off[i]);
            header(&mut sec, lang_off[i], 1);
            put(&mut sec, lang_off[i] + 16, LANG);
            put(&mut sec, lang_off[i] + 20, entry_off[i]);
            // data entry: RVA (relocated), size, codepage, reserved
            put(&mut sec, entry_off[i], data_off[i]);
            put(&mut sec, entry_off[i] + 4, res[i].2.len() as u32);
            let d = data_off[i] as usize;
            sec[d..d + res[i].2.len()].copy_from_slice(&res[i].2);
        }
    }

    // COFF: file header, one section header, raw data, relocations, symbols
    let nrel = res.len();
    let raw_ptr = 20 + 40;
    let rel_ptr = raw_ptr + sec.len();
    let sym_ptr = rel_ptr + 10 * nrel;
    let mut o = vec![];
    o.extend_from_slice(&0x8664u16.to_le_bytes()); // AMD64
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes());
    o.extend_from_slice(&(sym_ptr as u32).to_le_bytes());
    o.extend_from_slice(&1u32.to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(b".rsrc\0\0\0");
    o.extend_from_slice(&0u32.to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes());
    o.extend_from_slice(&(sec.len() as u32).to_le_bytes());
    o.extend_from_slice(&(raw_ptr as u32).to_le_bytes());
    o.extend_from_slice(&(rel_ptr as u32).to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes());
    o.extend_from_slice(&(nrel as u16).to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(&0xC030_0040u32.to_le_bytes()); // data, read/write, align 4
    o.extend_from_slice(&sec);
    for &e in &entry_off {
        o.extend_from_slice(&e.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // symbol 0 = .rsrc
        o.extend_from_slice(&3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB
    }
    o.extend_from_slice(b".rsrc\0\0\0");
    o.extend_from_slice(&0u32.to_le_bytes());
    o.extend_from_slice(&1i16.to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    o.push(3); // static
    o.push(0);
    o.extend_from_slice(&4u32.to_le_bytes()); // empty string table
    o
}
