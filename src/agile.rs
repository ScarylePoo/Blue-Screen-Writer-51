//! Office password protection for .docx files ([MS-OFFCRYPTO]).
//!
//! * `encrypt` writes ECMA-376 *Agile Encryption* (version 4.4): AES-256-CBC,
//!   SHA-512, spin count 100000, data-integrity HMAC, inside an OLE compound
//!   file together with the `\u{6}DataSpaces` storage Word expects.
//! * `decrypt` reads Agile files (any AES key size, SHA-1/256/384/512) and
//!   ECMA-376 *Standard Encryption* (versions 2.2/3.2/4.2, AES + SHA-1, as
//!   written by Word 2007).
//!
//! The compound file *writer* (CFB version 3, 512-byte sectors, mini stream,
//! DIFAT for big files) lives here; reading uses `msdoc::Cfb`.


use crate::msdoc::Cfb;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use sha2::Digest;

const SPIN_COUNT: u32 = 100_000;
const SEGMENT: usize = 4096;

const BK_VERIFIER_INPUT: [u8; 8] = [0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79];
const BK_VERIFIER_VALUE: [u8; 8] = [0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e];
const BK_KEY_VALUE: [u8; 8] = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];
const BK_HMAC_KEY: [u8; 8] = [0x5f, 0xb2, 0xad, 0x01, 0x0c, 0xb9, 0xe1, 0xf6];
// [MS-OFFCRYPTO] 2.3.4.14 (also what msoffcrypto-tool uses)
const BK_HMAC_VALUE: [u8; 8] = [0xa0, 0x67, 0x7f, 0x02, 0xb2, 0x2c, 0x84, 0x33];

// ---------------------------------------------------------------------------
// public API

/// True if `bytes` is an OLE compound file holding an encrypted Office package.
pub fn is_encrypted_package(bytes: &[u8]) -> bool {
    match Cfb::parse(bytes) {
        Ok(c) => c.stream("EncryptionInfo").is_some() && c.stream("EncryptedPackage").is_some(),
        Err(_) => false,
    }
}

/// Encrypt the plain .docx (zip) bytes with `password` (Agile Encryption).
pub fn encrypt(package: &[u8], password: &str) -> Vec<u8> {
    let (info, pkg) = encrypt_streams(package, password);
    cfb_write(vec![
        Node::Stream("EncryptionInfo".into(), info),
        Node::Stream("EncryptedPackage".into(), pkg),
        dataspaces(),
    ])
}

/// Decrypt an encrypted Office file; returns the plain package (zip) bytes.
pub fn decrypt(bytes: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let cfb = Cfb::parse(bytes)?;
    let info = cfb.stream("EncryptionInfo").ok_or("not an encrypted Office file")?;
    let pkg = cfb.stream("EncryptedPackage").ok_or("not an encrypted Office file")?;
    decrypt_streams(&info, &pkg, password)
}

// ---------------------------------------------------------------------------
// hashing / AES helpers

#[derive(Clone, Copy, PartialEq, Debug)]
enum HashAlg {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlg {
    fn from_name(s: &str) -> Result<HashAlg, String> {
        match s.to_ascii_uppercase().replace('-', "").as_str() {
            "SHA1" => Ok(HashAlg::Sha1),
            "SHA256" => Ok(HashAlg::Sha256),
            "SHA384" => Ok(HashAlg::Sha384),
            "SHA512" => Ok(HashAlg::Sha512),
            _ => Err(format!("unsupported hash algorithm {s}")),
        }
    }
    fn block(self) -> usize {
        match self {
            HashAlg::Sha1 | HashAlg::Sha256 => 64,
            _ => 128,
        }
    }
    fn hash(self, parts: &[&[u8]]) -> Vec<u8> {
        fn h<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut d = D::new();
            for p in parts {
                d.update(p);
            }
            d.finalize().to_vec()
        }
        match self {
            HashAlg::Sha1 => h::<sha1::Sha1>(parts),
            HashAlg::Sha256 => h::<sha2::Sha256>(parts),
            HashAlg::Sha384 => h::<sha2::Sha384>(parts),
            HashAlg::Sha512 => h::<sha2::Sha512>(parts),
        }
    }
    fn hmac(self, key: &[u8], msg: &[u8]) -> Vec<u8> {
        let b = self.block();
        let mut k = if key.len() > b { self.hash(&[key]) } else { key.to_vec() };
        k.resize(b, 0);
        let ipad: Vec<u8> = k.iter().map(|x| x ^ 0x36).collect();
        let opad: Vec<u8> = k.iter().map(|x| x ^ 0x5c).collect();
        let inner = self.hash(&[&ipad, msg]);
        self.hash(&[&opad, &inner])
    }
}

enum Aes {
    A128(aes::Aes128),
    A192(aes::Aes192),
    A256(aes::Aes256),
}

impl Aes {
    fn new(key: &[u8]) -> Result<Aes, String> {
        Ok(match key.len() {
            16 => Aes::A128(aes::Aes128::new_from_slice(key).map_err(|e| e.to_string())?),
            24 => Aes::A192(aes::Aes192::new_from_slice(key).map_err(|e| e.to_string())?),
            32 => Aes::A256(aes::Aes256::new_from_slice(key).map_err(|e| e.to_string())?),
            n => return Err(format!("unsupported AES key length {}", n * 8)),
        })
    }
    fn enc(&self, b: &mut [u8]) {
        let g = GenericArray::from_mut_slice(b);
        match self {
            Aes::A128(c) => c.encrypt_block(g),
            Aes::A192(c) => c.encrypt_block(g),
            Aes::A256(c) => c.encrypt_block(g),
        }
    }
    fn dec(&self, b: &mut [u8]) {
        let g = GenericArray::from_mut_slice(b);
        match self {
            Aes::A128(c) => c.decrypt_block(g),
            Aes::A192(c) => c.decrypt_block(g),
            Aes::A256(c) => c.decrypt_block(g),
        }
    }
}

/// AES-CBC; the plaintext is zero-padded to a whole number of blocks.
fn cbc_encrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    let aes = Aes::new(key).expect("valid key length");
    let mut out = data.to_vec();
    out.resize(data.len().div_ceil(16) * 16, 0);
    let mut prev = [0u8; 16];
    prev.copy_from_slice(&iv[..16]);
    for blk in out.chunks_exact_mut(16) {
        for (b, p) in blk.iter_mut().zip(prev.iter()) {
            *b ^= p;
        }
        aes.enc(blk);
        prev.copy_from_slice(blk);
    }
    out
}

/// AES-CBC decryption (a trailing partial block is ignored).
fn cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let aes = Aes::new(key)?;
    if iv.len() < 16 {
        return Err("damaged encryption info (IV)".into());
    }
    let mut out = data[..data.len() / 16 * 16].to_vec();
    let mut prev = [0u8; 16];
    prev.copy_from_slice(&iv[..16]);
    for blk in out.chunks_exact_mut(16) {
        let mut cur = [0u8; 16];
        cur.copy_from_slice(blk);
        aes.dec(blk);
        for (b, p) in blk.iter_mut().zip(prev.iter()) {
            *b ^= p;
        }
        prev = cur;
    }
    Ok(out)
}

fn ecb_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let aes = Aes::new(key)?;
    let mut out = data[..data.len() / 16 * 16].to_vec();
    for blk in out.chunks_exact_mut(16) {
        aes.dec(blk);
    }
    Ok(out)
}

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// Random bytes: std's `RandomState` keys come from the OS generator; mix them
/// (with time and a counter) through SHA-512.
fn random_bytes(n: usize) -> Vec<u8> {
    use std::hash::{BuildHasher, Hasher};
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut seed = Vec::new();
    for i in 0..8usize {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(t);
        h.write_usize(i);
        h.write_usize(&seed as *const _ as usize);
        seed.extend_from_slice(&h.finish().to_le_bytes());
    }
    let mut out = Vec::new();
    let mut ctr = 0u32;
    while out.len() < n {
        out.extend(HashAlg::Sha512.hash(&[&seed, &ctr.to_le_bytes()]));
        ctr += 1;
    }
    out.truncate(n);
    out
}

// ---------------------------------------------------------------------------
// base64

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(d: &[u8]) -> String {
    let mut s = String::new();
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for ch in s.bytes() {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return Err("damaged encryption info (base64)".into()),
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Agile encryption

fn password_hash(alg: HashAlg, salt: &[u8], password: &str, spin: u32) -> Vec<u8> {
    let mut h = alg.hash(&[salt, &utf16le(password)]);
    for i in 0..spin {
        h = alg.hash(&[&i.to_le_bytes(), &h]);
    }
    h
}

/// Hash `a || b`, then truncate or pad (with 0x36) to `len` bytes.
fn hash_fit(alg: HashAlg, a: &[u8], b: &[u8], len: usize) -> Vec<u8> {
    let mut k = alg.hash(&[a, b]);
    k.resize(len, 0x36);
    k
}

fn encrypt_streams(package: &[u8], password: &str) -> (Vec<u8>, Vec<u8>) {
    let alg = HashAlg::Sha512;
    let rnd = random_bytes(16 + 16 + 16 + 32 + 64);
    let (key_salt, rest) = rnd.split_at(16);
    let (pw_salt, rest) = rest.split_at(16);
    let (verifier, rest) = rest.split_at(16);
    let (secret, hmac_key) = rest.split_at(32);

    let h = password_hash(alg, pw_salt, password, SPIN_COUNT);
    let evhi = cbc_encrypt(&hash_fit(alg, &h, &BK_VERIFIER_INPUT, 32), pw_salt, verifier);
    let evhv = cbc_encrypt(&hash_fit(alg, &h, &BK_VERIFIER_VALUE, 32), pw_salt, &alg.hash(&[verifier]));
    let ekv = cbc_encrypt(&hash_fit(alg, &h, &BK_KEY_VALUE, 32), pw_salt, secret);

    let mut pkg = (package.len() as u64).to_le_bytes().to_vec();
    for (i, seg) in package.chunks(SEGMENT).enumerate() {
        let iv = hash_fit(alg, key_salt, &(i as u32).to_le_bytes(), 16);
        pkg.extend(cbc_encrypt(secret, &iv, seg));
    }
    let hmac = alg.hmac(hmac_key, &pkg);
    let ehk = cbc_encrypt(secret, &hash_fit(alg, key_salt, &BK_HMAC_KEY, 16), hmac_key);
    let ehv = cbc_encrypt(secret, &hash_fit(alg, key_salt, &BK_HMAC_VALUE, 16), &hmac);

    let params = r#"saltSize="16" blockSize="16" keyBits="256" hashSize="64" cipherAlgorithm="AES" cipherChaining="ChainingModeCBC" hashAlgorithm="SHA512""#;
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n\
<encryption xmlns=\"http://schemas.microsoft.com/office/2006/encryption\" \
xmlns:p=\"http://schemas.microsoft.com/office/2006/keyEncryptor/password\" \
xmlns:c=\"http://schemas.microsoft.com/office/2006/keyEncryptor/certificate\">\
<keyData {params} saltValue=\"{}\"/>\
<dataIntegrity encryptedHmacKey=\"{}\" encryptedHmacValue=\"{}\"/>\
<keyEncryptors><keyEncryptor uri=\"http://schemas.microsoft.com/office/2006/keyEncryptor/password\">\
<p:encryptedKey spinCount=\"{SPIN_COUNT}\" {params} saltValue=\"{}\" encryptedVerifierHashInput=\"{}\" \
encryptedVerifierHashValue=\"{}\" encryptedKeyValue=\"{}\"/></keyEncryptor></keyEncryptors></encryption>",
        b64_encode(key_salt),
        b64_encode(&ehk),
        b64_encode(&ehv),
        b64_encode(pw_salt),
        b64_encode(&evhi),
        b64_encode(&evhv),
        b64_encode(&ekv),
    );
    let mut info = vec![4, 0, 4, 0, 0x40, 0, 0, 0];
    info.extend_from_slice(xml.as_bytes());
    (info, pkg)
}

fn decrypt_streams(info: &[u8], pkg: &[u8], password: &str) -> Result<Vec<u8>, String> {
    if info.len() < 8 {
        return Err("damaged encryption info".into());
    }
    let major = u16::from_le_bytes([info[0], info[1]]);
    let minor = u16::from_le_bytes([info[2], info[3]]);
    match (major, minor) {
        (4, 4) => decrypt_agile(&info[8..], pkg, password),
        (2..=4, 2) => decrypt_standard(&info[8..], pkg, password),
        _ => Err(format!("unsupported encryption (version {major}.{minor})")),
    }
}

/// Attributes (local names) of every element named `local` (any prefix).
fn xml_elements(xml: &str, local: &str) -> Vec<Vec<(String, String)>> {
    let mut res = Vec::new();
    let mut rest = xml;
    while let Some(p) = rest.find('<') {
        rest = &rest[p + 1..];
        let end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(rest.len());
        let name = &rest[..end];
        let lname = name.rsplit(':').next().unwrap_or(name);
        if lname != local {
            continue;
        }
        let mut s = &rest[end..];
        let mut attrs = Vec::new();
        loop {
            s = s.trim_start();
            if s.is_empty() || s.starts_with('>') || s.starts_with('/') {
                break;
            }
            let Some(eq) = s.find('=') else { break };
            let an = s[..eq].trim();
            let an = an.rsplit(':').next().unwrap_or(an).to_string();
            s = s[eq + 1..].trim_start();
            let Some(q) = s.chars().next().filter(|&c| c == '"' || c == '\'') else { break };
            let Some(close) = s[1..].find(q) else { break };
            attrs.push((an, s[1..1 + close].to_string()));
            s = &s[close + 2..];
        }
        res.push(attrs);
    }
    res
}

struct Params {
    hash: HashAlg,
    key_bytes: usize,
    hash_size: usize,
    salt: Vec<u8>,
}

fn attr<'a>(a: &'a [(String, String)], n: &str) -> Result<&'a str, String> {
    a.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str()).ok_or_else(|| format!("damaged encryption info (no {n})"))
}

fn params(a: &[(String, String)]) -> Result<Params, String> {
    let num = |n: &str| -> Result<usize, String> { attr(a, n)?.trim().parse::<usize>().map_err(|_| format!("damaged encryption info ({n})")) };
    if !attr(a, "cipherAlgorithm")?.eq_ignore_ascii_case("AES") {
        return Err(format!("unsupported cipher {}", attr(a, "cipherAlgorithm")?));
    }
    if attr(a, "cipherChaining")? != "ChainingModeCBC" {
        return Err(format!("unsupported cipher mode {}", attr(a, "cipherChaining")?));
    }
    if num("blockSize")? != 16 {
        return Err("unsupported cipher block size".into());
    }
    Ok(Params { hash: HashAlg::from_name(attr(a, "hashAlgorithm")?)?, key_bytes: num("keyBits")? / 8, hash_size: num("hashSize")?, salt: b64_decode(attr(a, "saltValue")?)? })
}

fn decrypt_agile(xml: &[u8], pkg: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let xml = String::from_utf8_lossy(xml);
    let kd = xml_elements(&xml, "keyData").into_iter().next().ok_or("damaged encryption info (no keyData)")?;
    let kd = params(&kd)?;
    let ek = xml_elements(&xml, "encryptedKey")
        .into_iter()
        .find(|a| a.iter().any(|(k, _)| k == "encryptedVerifierHashInput"))
        .ok_or("this file is not protected by a password (unsupported key encryptor)")?;
    let pp = params(&ek)?;
    let spin: u32 = attr(&ek, "spinCount")?.trim().parse().map_err(|_| "damaged encryption info (spinCount)")?;
    let salt_size: usize = attr(&ek, "saltSize")?.trim().parse().map_err(|_| "damaged encryption info (saltSize)")?;

    let h = password_hash(pp.hash, &pp.salt, password, spin);
    let iv = {
        let mut v = pp.salt.clone();
        v.resize(16, 0x36);
        v
    };
    let mut input = cbc_decrypt(&hash_fit(pp.hash, &h, &BK_VERIFIER_INPUT, pp.key_bytes), &iv, &b64_decode(attr(&ek, "encryptedVerifierHashInput")?)?)?;
    input.truncate(salt_size);
    let value = cbc_decrypt(&hash_fit(pp.hash, &h, &BK_VERIFIER_VALUE, pp.key_bytes), &iv, &b64_decode(attr(&ek, "encryptedVerifierHashValue")?)?)?;
    let want = pp.hash.hash(&[&input]);
    if value.len() < want.len() || value[..want.len()] != want[..] {
        return Err("wrong password".into());
    }
    let mut secret = cbc_decrypt(&hash_fit(pp.hash, &h, &BK_KEY_VALUE, pp.key_bytes), &iv, &b64_decode(attr(&ek, "encryptedKeyValue")?)?)?;
    if secret.len() < kd.key_bytes {
        return Err("damaged encryption info (key)".into());
    }
    secret.truncate(kd.key_bytes);

    // data integrity (optional)
    if let Some(di) = xml_elements(&xml, "dataIntegrity").into_iter().next() {
        let hk = cbc_decrypt(&secret, &hash_fit(kd.hash, &kd.salt, &BK_HMAC_KEY, 16), &b64_decode(attr(&di, "encryptedHmacKey")?)?)?;
        let hv = cbc_decrypt(&secret, &hash_fit(kd.hash, &kd.salt, &BK_HMAC_VALUE, 16), &b64_decode(attr(&di, "encryptedHmacValue")?)?)?;
        let hs = kd.hash_size;
        if hk.len() < hs || hv.len() < hs || kd.hash.hmac(&hk[..hs], pkg) != hv[..hs] {
            return Err("the encrypted file is damaged (integrity check failed)".into());
        }
    }

    if pkg.len() < 8 {
        return Err("damaged encrypted package".into());
    }
    let size = u64::from_le_bytes(pkg[..8].try_into().unwrap());
    let mut out = Vec::with_capacity(pkg.len());
    for (i, seg) in pkg[8..].chunks(SEGMENT).enumerate() {
        let iv = hash_fit(kd.hash, &kd.salt, &(i as u32).to_le_bytes(), 16);
        out.extend(cbc_decrypt(&secret, &iv, seg)?);
    }
    if size > out.len() as u64 {
        return Err("damaged encrypted package (too short)".into());
    }
    out.truncate(size as usize);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Standard encryption (Word 2007): AES-ECB, SHA-1, 50000 iterations

fn standard_key(salt: &[u8], password: &str, key_bytes: usize) -> Vec<u8> {
    let alg = HashAlg::Sha1;
    let h = password_hash(alg, salt, password, 50_000);
    let hf = alg.hash(&[&h, &0u32.to_le_bytes()]);
    let derive = |pad: u8| {
        let mut b = [pad; 64];
        for (x, y) in b.iter_mut().zip(hf.iter()) {
            *x ^= y;
        }
        alg.hash(&[&b])
    };
    let mut k = derive(0x36);
    k.extend(derive(0x5c));
    k.truncate(key_bytes);
    k
}

fn decrypt_standard(d: &[u8], pkg: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let u32_at = |o: usize| d.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or("damaged encryption info");
    let hs = u32_at(0)? as usize;
    let alg_id = u32_at(4 + 8)?;
    let key_bits = u32_at(4 + 16)?;
    let key_bytes = match (alg_id, key_bits) {
        (0x660E, _) | (0, 0) => 16,
        (0x660F, _) => 24,
        (0x6610, _) => 32,
        (0x6801, _) => return Err("unsupported encryption (RC4 CryptoAPI)".into()),
        _ => return Err(format!("unsupported encryption algorithm {alg_id:#x}")),
    };
    let v = 4 + hs;
    let salt_size = u32_at(v)? as usize;
    let salt = d.get(v + 4..v + 4 + salt_size).ok_or("damaged encryption info")?;
    let o = v + 4 + salt_size;
    let enc_verifier = d.get(o..o + 16).ok_or("damaged encryption info")?;
    let hash_size = u32_at(o + 16)? as usize;
    let enc_hash = d.get(o + 20..o + 20 + 32).ok_or("damaged encryption info")?;
    let key = standard_key(salt, password, key_bytes);
    let verifier = ecb_decrypt(&key, enc_verifier)?;
    let hash = ecb_decrypt(&key, enc_hash)?;
    let want = HashAlg::Sha1.hash(&[&verifier]);
    if hash_size > hash.len() || hash_size > want.len() || hash[..hash_size] != want[..hash_size] {
        return Err("wrong password".into());
    }
    if pkg.len() < 8 {
        return Err("damaged encrypted package".into());
    }
    let size = u64::from_le_bytes(pkg[..8].try_into().unwrap());
    let mut out = ecb_decrypt(&key, &pkg[8..])?;
    if size > out.len() as u64 {
        return Err("damaged encrypted package (too short)".into());
    }
    out.truncate(size as usize);
    Ok(out)
}

// ---------------------------------------------------------------------------
// \u{6}DataSpaces storage ([MS-OFFCRYPTO] 2.1.x, 2.3.4.x)

/// UNICODE-LP-P4: u32 byte length, UTF-16LE, padded to 4 bytes.
fn lp_p4(s: &str) -> Vec<u8> {
    let u = utf16le(s);
    let mut v = (u.len() as u32).to_le_bytes().to_vec();
    v.extend(u);
    v.resize(v.len().div_ceil(4) * 4, 0);
    v
}

fn dataspaces() -> Node {
    let versions = [1u8, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]; // reader, updater, writer 1.0
    let mut version = lp_p4("Microsoft.Container.DataSpaces");
    version.extend_from_slice(&versions);

    let mut entry = vec![];
    entry.extend(1u32.to_le_bytes()); // reference component count
    entry.extend(0u32.to_le_bytes()); // type: stream
    entry.extend(lp_p4("EncryptedPackage"));
    entry.extend(lp_p4("StrongEncryptionDataSpace"));
    let mut map = vec![];
    map.extend(8u32.to_le_bytes());
    map.extend(1u32.to_le_bytes());
    map.extend((entry.len() as u32 + 4).to_le_bytes());
    map.extend(entry);

    let mut def = vec![];
    def.extend(8u32.to_le_bytes());
    def.extend(1u32.to_le_bytes());
    def.extend(lp_p4("StrongEncryptionTransform"));

    let id = lp_p4("{FF9A3F03-56EF-4613-BDD5-5A41C1D07246}");
    let mut primary = vec![];
    primary.extend((8 + id.len() as u32).to_le_bytes());
    primary.extend(1u32.to_le_bytes()); // transform type
    primary.extend(id);
    primary.extend(lp_p4("Microsoft.Container.EncryptionTransform"));
    primary.extend_from_slice(&versions);
    primary.extend(0u32.to_le_bytes()); // EncryptionName (empty)
    primary.extend(0u32.to_le_bytes()); // EncryptionBlockSize
    primary.extend(0u32.to_le_bytes()); // CipherMode
    primary.extend(4u32.to_le_bytes()); // Reserved

    Node::Storage(
        "\u{6}DataSpaces".into(),
        vec![
            Node::Stream("Version".into(), version),
            Node::Stream("DataSpaceMap".into(), map),
            Node::Storage("DataSpaceInfo".into(), vec![Node::Stream("StrongEncryptionDataSpace".into(), def)]),
            Node::Storage(
                "TransformInfo".into(),
                vec![Node::Storage("StrongEncryptionTransform".into(), vec![Node::Stream("\u{6}Primary".into(), primary)])],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------
// Compound File writer (CFB version 3)

const SECTOR: usize = 512;
const MINI: usize = 64;
const CUTOFF: usize = 4096;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FREESECT: u32 = 0xFFFF_FFFF;
const FATSECT: u32 = 0xFFFF_FFFD;
const DIFSECT: u32 = 0xFFFF_FFFC;
const NOSTREAM: u32 = 0xFFFF_FFFF;

pub(crate) enum Node {
    Stream(String, Vec<u8>),
    Storage(String, Vec<Node>),
}

struct WEntry<'a> {
    name: Vec<u16>,
    typ: u8,
    data: &'a [u8],
    left: u32,
    right: u32,
    child: u32,
    red: bool,
    start: u32,
}

/// CFB sibling order: shorter names first, then case-insensitive UTF-16 compare.
fn name_cmp(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    let up = |c: u16| if (b'a' as u16..=b'z' as u16).contains(&c) { c - 32 } else { c };
    a.len().cmp(&b.len()).then_with(|| a.iter().map(|&c| up(c)).cmp(b.iter().map(|&c| up(c))))
}

/// Build a balanced binary tree over sorted `idx`; nodes on the deepest level of
/// an incomplete tree are red, everything else black (a valid red-black tree).
fn build_tree(es: &mut [WEntry], idx: &[usize]) -> u32 {
    let n = idx.len();
    let full = (n + 1).is_power_of_two();
    let levels = (usize::BITS - n.leading_zeros()) as usize; // ceil(log2(n+1)) for the balanced build
    let red_depth = if full { usize::MAX } else { levels - 1 };
    fn go(es: &mut [WEntry], idx: &[usize], depth: usize, red_depth: usize) -> u32 {
        if idx.is_empty() {
            return NOSTREAM;
        }
        let mid = idx.len() / 2;
        let l = go(es, &idx[..mid], depth + 1, red_depth);
        let r = go(es, &idx[mid + 1..], depth + 1, red_depth);
        let e = &mut es[idx[mid]];
        e.left = l;
        e.right = r;
        e.red = depth == red_depth;
        idx[mid] as u32
    }
    go(es, idx, 0, red_depth)
}

fn flatten<'a>(es: &mut Vec<WEntry<'a>>, parent: usize, nodes: &'a [Node]) {
    let mut kids = Vec::new();
    for n in nodes {
        let (name, typ, data): (&str, u8, &[u8]) = match n {
            Node::Stream(s, d) => (s, 2, d),
            Node::Storage(s, _) => (s, 1, &[]),
        };
        let mut name: Vec<u16> = name.encode_utf16().collect();
        name.truncate(31);
        kids.push(es.len());
        es.push(WEntry { name, typ, data, left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, red: false, start: ENDOFCHAIN });
    }
    let mut sorted = kids.clone();
    sorted.sort_by(|&a, &b| name_cmp(&es[a].name, &es[b].name));
    es[parent].child = build_tree(es, &sorted);
    for (n, &k) in nodes.iter().zip(kids.iter()) {
        if let Node::Storage(_, ch) = n {
            flatten(es, k, ch);
        }
    }
}

pub(crate) fn cfb_write(root: Vec<Node>) -> Vec<u8> {
    let mut es = vec![WEntry { name: "Root Entry".encode_utf16().collect(), typ: 5, data: &[], left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, red: false, start: ENDOFCHAIN }];
    flatten(&mut es, 0, &root);

    // mini stream
    let mut mini = Vec::new();
    let mut minifat: Vec<u32> = Vec::new();
    for e in es.iter_mut().filter(|e| e.typ == 2 && !e.data.is_empty() && e.data.len() < CUTOFF) {
        let first = minifat.len() as u32;
        let n = e.data.len().div_ceil(MINI);
        for i in 0..n {
            minifat.push(if i + 1 == n { ENDOFCHAIN } else { first + i as u32 + 1 });
        }
        e.start = first;
        mini.extend_from_slice(e.data);
        mini.resize(minifat.len() * MINI, 0);
    }

    // regular sectors: big streams, mini stream, mini FAT, directory, FAT, DIFAT
    let mut fat: Vec<u32> = Vec::new();
    let alloc = |fat: &mut Vec<u32>, n: usize| -> u32 {
        if n == 0 {
            return ENDOFCHAIN;
        }
        let first = fat.len() as u32;
        for i in 0..n {
            fat.push(if i + 1 == n { ENDOFCHAIN } else { first + i as u32 + 1 });
        }
        first
    };
    let mut big: Vec<usize> = Vec::new();
    for (i, e) in es.iter_mut().enumerate() {
        if e.typ == 2 && e.data.len() >= CUTOFF {
            e.start = alloc(&mut fat, e.data.len().div_ceil(SECTOR));
            big.push(i);
        }
    }
    let mini_start = alloc(&mut fat, mini.len().div_ceil(SECTOR));
    es[0].start = mini_start;
    let minifat_bytes = minifat.len() * 4;
    let minifat_sectors = minifat_bytes.div_ceil(SECTOR);
    let minifat_start = alloc(&mut fat, minifat_sectors);
    let dir_sectors = es.len().div_ceil(SECTOR / 128);
    let dir_start = alloc(&mut fat, dir_sectors);

    let content = fat.len();
    let (mut nfat, mut ndif) = (0usize, 0usize);
    loop {
        let total = content + nfat + ndif;
        let f = total.div_ceil(SECTOR / 4);
        let d = f.saturating_sub(109).div_ceil(SECTOR / 4 - 1);
        if f == nfat && d == ndif {
            break;
        }
        (nfat, ndif) = (f, d);
    }
    let fat_start = fat.len() as u32;
    fat.extend(std::iter::repeat_n(FATSECT, nfat));
    let dif_start = fat.len() as u32;
    fat.extend(std::iter::repeat_n(DIFSECT, ndif));
    let total = fat.len();
    fat.resize(nfat * SECTOR / 4, FREESECT);

    let mut out = Vec::with_capacity((total + 1) * SECTOR);
    // header
    out.extend_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&0x003Eu16.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&9u16.to_le_bytes());
    out.extend_from_slice(&6u16.to_le_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&0u32.to_le_bytes()); // directory sectors (0 in v3)
    out.extend_from_slice(&(nfat as u32).to_le_bytes());
    out.extend_from_slice(&dir_start.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(CUTOFF as u32).to_le_bytes());
    out.extend_from_slice(&(if minifat_sectors > 0 { minifat_start } else { ENDOFCHAIN }).to_le_bytes());
    out.extend_from_slice(&(minifat_sectors as u32).to_le_bytes());
    out.extend_from_slice(&(if ndif > 0 { dif_start } else { ENDOFCHAIN }).to_le_bytes());
    out.extend_from_slice(&(ndif as u32).to_le_bytes());
    for i in 0..109 {
        let v = if i < nfat { fat_start + i as u32 } else { FREESECT };
        out.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(out.len(), SECTOR);

    let pad = |out: &mut Vec<u8>| out.resize(out.len().div_ceil(SECTOR) * SECTOR, 0);
    for &i in &big {
        out.extend_from_slice(es[i].data);
        pad(&mut out);
    }
    out.extend_from_slice(&mini);
    pad(&mut out);
    for v in &minifat {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let mut p = out.len().div_ceil(SECTOR) * SECTOR;
    out.resize(p, 0xFF); // unused mini FAT entries = FREESECT
    // directory
    for e in &es {
        let mut b = [0u8; 128];
        for (i, &u) in e.name.iter().enumerate() {
            b[2 * i..2 * i + 2].copy_from_slice(&u.to_le_bytes());
        }
        b[64..66].copy_from_slice(&((e.name.len() as u16 + 1) * 2).to_le_bytes());
        b[66] = e.typ;
        b[67] = if e.red { 0 } else { 1 };
        b[68..72].copy_from_slice(&e.left.to_le_bytes());
        b[72..76].copy_from_slice(&e.right.to_le_bytes());
        b[76..80].copy_from_slice(&e.child.to_le_bytes());
        let (start, size) = match e.typ {
            5 => (if mini.is_empty() { ENDOFCHAIN } else { e.start }, mini.len() as u64),
            2 => (e.start, e.data.len() as u64),
            _ => (0, 0),
        };
        b[116..120].copy_from_slice(&start.to_le_bytes());
        b[120..128].copy_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&b);
    }
    while out.len() < p + dir_sectors * SECTOR {
        let mut b = [0u8; 128];
        b[68..80].fill(0xFF);
        out.extend_from_slice(&b);
    }
    p = out.len();
    debug_assert_eq!(p, (1 + fat_start as usize) * SECTOR);
    for v in &fat {
        out.extend_from_slice(&v.to_le_bytes());
    }
    // DIFAT sectors
    let per = SECTOR / 4 - 1;
    for d in 0..ndif {
        for j in 0..per {
            let i = 109 + d * per + j;
            let v = if i < nfat { fat_start + i as u32 } else { FREESECT };
            out.extend_from_slice(&v.to_le_bytes());
        }
        let next = if d + 1 < ndif { dif_start + d as u32 + 1 } else { ENDOFCHAIN };
        out.extend_from_slice(&next.to_le_bytes());
    }
    debug_assert_eq!(out.len(), (total + 1) * SECTOR);
    out
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 + i / 7) as u8).collect()
    }

    #[test]
    fn roundtrip_and_wrong_password() {
        for n in [0, 1, 100, 4096, 4097, 20000] {
            let p = sample(n);
            let enc = encrypt(&p, "Geheim äöü 🔒");
            assert!(is_encrypted_package(&enc));
            assert_eq!(decrypt(&enc, "Geheim äöü 🔒").unwrap(), p, "size {n}");
            if n == 100 {
                assert_eq!(decrypt(&enc, "geheim äöü 🔒").unwrap_err(), "wrong password");
                assert_eq!(decrypt(&enc, "").unwrap_err(), "wrong password");
            }
        }
        assert!(!is_encrypted_package(b"PK\x03\x04 not a cfb"));
        assert!(!is_encrypted_package(&cfb_write(vec![Node::Stream("WordDocument".into(), vec![1; 10])])));
    }

    #[test]
    fn hmac_detects_tampering() {
        let p = sample(9000);
        let (info, mut pkg) = encrypt_streams(&p, "pw");
        assert_eq!(decrypt_streams(&info, &pkg, "pw").unwrap(), p);
        pkg[5000] ^= 1;
        assert!(decrypt_streams(&info, &pkg, "pw").unwrap_err().contains("integrity"));
    }

    #[test]
    fn base64() {
        for n in 0..20 {
            let d = sample(n);
            assert_eq!(b64_decode(&b64_encode(&d)).unwrap(), d);
        }
        assert_eq!(b64_encode(b"Ma"), "TWE=");
    }

    #[test]
    fn hmac_known_answer() {
        // RFC 4231 test case 2
        let mac = HashAlg::Sha512.hmac(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(&mac[..8], &[0x16, 0x4b, 0x7a, 0x7b, 0xfc, 0xf8, 0x19, 0xe2]);
    }

    #[test]
    fn cfb_streams_of_many_sizes() {
        let sizes = [0usize, 100, 4095, 4096, 70000, 8 << 20];
        let nodes: Vec<Node> = sizes.iter().map(|&n| Node::Stream(format!("S{n}"), sample(n))).collect();
        let mut nodes = nodes;
        nodes.push(dataspaces());
        let bytes = cfb_write(nodes);
        assert_eq!(bytes.len() % 512, 0);
        let c = Cfb::parse(&bytes).unwrap();
        for n in sizes {
            assert_eq!(c.stream(&format!("S{n}")).unwrap(), sample(n), "size {n}");
        }
        // each size alone, too
        for n in sizes {
            let bytes = cfb_write(vec![Node::Stream("X".into(), sample(n))]);
            assert_eq!(Cfb::parse(&bytes).unwrap().stream("X").unwrap(), sample(n), "alone {n}");
        }
    }

    #[test]
    fn cfb_difat_boundary() {
        // One big stream + 1 directory sector + 109 FAT sectors = 13952 sectors,
        // exactly what 109 header DIFAT entries can map (109 * 128).
        let limit = 13842 * 512;
        for (n, want_dif) in [(limit, 0u32), (limit + 1, 1)] {
            let bytes = cfb_write(vec![Node::Stream("Big".into(), sample(n))]);
            let nfat = u32::from_le_bytes(bytes[0x2C..0x30].try_into().unwrap());
            let ndif = u32::from_le_bytes(bytes[0x48..0x4C].try_into().unwrap());
            assert_eq!(ndif, want_dif, "size {n}");
            assert_eq!(nfat, if want_dif == 0 { 109 } else { 110 });
            assert_eq!(Cfb::parse(&bytes).unwrap().stream("Big").unwrap(), sample(n));
        }
    }

    #[test]
    fn sibling_trees_are_red_black() {
        fn check(es: &[WEntry], id: u32, lo: Option<&[u16]>, hi: Option<&[u16]>, parent_red: bool) -> usize {
            if id == NOSTREAM {
                return 1;
            }
            let e = &es[id as usize];
            assert!(!(parent_red && e.red));
            if let Some(lo) = lo {
                assert_eq!(name_cmp(lo, &e.name), std::cmp::Ordering::Less);
            }
            if let Some(hi) = hi {
                assert_eq!(name_cmp(&e.name, hi), std::cmp::Ordering::Less);
            }
            let l = check(es, e.left, lo, Some(&e.name), e.red);
            let r = check(es, e.right, Some(&e.name), hi, e.red);
            assert_eq!(l, r);
            l + !e.red as usize
        }
        for n in 1..70 {
            let nodes: Vec<Node> = (0..n).map(|i| Node::Stream(format!("n{}", i * 7919 % 1000), vec![])).collect();
            let mut es = vec![WEntry { name: vec![], typ: 5, data: &[], left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, red: false, start: 0 }];
            flatten(&mut es, 0, &nodes);
            let root = es[0].child;
            assert!(!es[root as usize].red);
            check(&es, root, None, None, false);
        }
    }

    /// Test-only Standard Encryption writer (AES-128, as Word 2007 writes it).
    fn encrypt_standard(package: &[u8], password: &str) -> Vec<u8> {
        let salt = random_bytes(16);
        let verifier = random_bytes(16);
        let key = standard_key(&salt, password, 16);
        let aes = Aes::new(&key).unwrap();
        let ecb = |d: &[u8]| {
            let mut v = d.to_vec();
            v.resize(d.len().div_ceil(16) * 16, 0);
            v.chunks_exact_mut(16).for_each(|b| aes.enc(b));
            v
        };
        let csp = utf16le("Microsoft Enhanced RSA and AES Cryptographic Provider\0");
        let mut header = vec![];
        for v in [0x24u32, 0, 0x660E, 0x8004, 128, 0x18, 0, 0] {
            header.extend(v.to_le_bytes());
        }
        header.extend(csp);
        let mut info = vec![3, 0, 2, 0, 0x24, 0, 0, 0];
        info.extend((header.len() as u32).to_le_bytes());
        info.extend(header);
        info.extend(16u32.to_le_bytes());
        info.extend(&salt);
        info.extend(ecb(&verifier));
        info.extend(20u32.to_le_bytes());
        info.extend(ecb(&HashAlg::Sha1.hash(&[&verifier])));
        let mut pkg = (package.len() as u64).to_le_bytes().to_vec();
        pkg.extend(ecb(package));
        cfb_write(vec![Node::Stream("EncryptionInfo".into(), info), Node::Stream("EncryptedPackage".into(), pkg), dataspaces()])
    }

    #[test]
    fn standard_encryption_roundtrip() {
        let p = sample(5000);
        let enc = encrypt_standard(&p, "pw");
        assert_eq!(decrypt(&enc, "pw").unwrap(), p);
        assert_eq!(decrypt(&enc, "px").unwrap_err(), "wrong password");
    }

    /// A small .docx encrypted by msoffcrypto-tool 5 (`-e -p Passw0rd!`).
    #[test]
    fn reads_msoffcrypto_tool_file() {
        let enc = include_bytes!("../tests/fixtures/agile_msoffcrypto.docx");
        assert!(is_encrypted_package(enc));
        let plain = decrypt(enc, "Passw0rd!").unwrap();
        assert!(plain.starts_with(b"PK\x03\x04"));
        assert_eq!(plain, include_bytes!("../tests/fixtures/agile_plain.docx"));
        assert_eq!(decrypt(enc, "password").unwrap_err(), "wrong password");
    }

    /// Interop helper: `BSW51_AGILE_IN=plain.docx BSW51_AGILE_OUT=enc.docx`
    /// [`BSW51_AGILE_STANDARD=1`]; password "Passw0rd!".
    #[test]
    #[ignore]
    fn dump_encrypted() {
        let inp = std::env::var("BSW51_AGILE_IN").unwrap();
        let out = std::env::var("BSW51_AGILE_OUT").unwrap();
        let p = std::fs::read(inp).unwrap();
        let enc = if std::env::var("BSW51_AGILE_STANDARD").is_ok() { encrypt_standard(&p, "Passw0rd!") } else { encrypt(&p, "Passw0rd!") };
        std::fs::write(out, enc).unwrap();
    }
}
