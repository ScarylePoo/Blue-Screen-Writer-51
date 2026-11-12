//! Crash recovery (autosave), automatic saving, and revision history.
//!
//! * **Recovery**: while a document has unsaved changes, a recovery copy is
//!   written every 30 s (atomically: temp file + flush + rename) to
//!   `<config>/Recovery/`.  On start-up any leftover recovery copy is
//!   offered back ("Recover …? Yes (No)").  A clean save or a deliberate
//!   "don't save" removes it.
//! * **Revision history**: every save stores a snapshot under
//!   `<config>/History/<document id>/`.  The id lives in the .docx custom
//!   properties (Word keeps those), so the history follows the document even
//!   if it is edited in Word in between — such outside edits are detected
//!   on open and recorded as their own revision.
//! * **Autosave**: named .docx documents are saved to their own file every
//!   few minutes (Setup → Environment → Backup); the history keeps every
//!   earlier version, so this can always be undone.

use crate::doc::*;
use crate::editor::Editor;
use crate::settings::config_dir;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub const RECOVERY_EVERY: Duration = Duration::from_secs(30);

pub fn recovery_dir() -> PathBuf {
    config_dir().join("Recovery")
}

pub fn history_dir(id: &str) -> PathBuf {
    config_dir().join("History").join(id)
}

/// Write `data` to `p` so that a power cut can never leave a half-written
/// file: temp file, flush to disk, then rename over the old one.
pub fn write_durable(p: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("tmp~");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if std::fs::rename(&tmp, p).is_err() {
        let _ = std::fs::remove_file(p);
        std::fs::rename(&tmp, p)?;
    }
    Ok(())
}

fn stamp(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let secs = secs + crate::sys::utc_offset_at(secs);
    let (y, m, d) = crate::filelist::civil_from_days(secs.div_euclid(86400));
    let r = secs.rem_euclid(86400);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", r / 3600, (r % 3600) / 60, r % 60)
}

/// Content fingerprint (ignores the id line).
pub fn fingerprint(doc: &Document) -> u64 {
    let mut d = doc.clone();
    d.id.clear();
    let s = crate::stream::write(&d);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ------------------------------------------------------------- recovery

fn recovery_file(e: &Editor, slot: usize) -> PathBuf {
    let key = if e.doc.id.is_empty() { format!("unnamed-{slot}") } else { e.doc.id.clone() };
    recovery_dir().join(format!("{key}.wps"))
}

/// Write the recovery copy for a modified document.
pub fn write_recovery(e: &Editor, slot: usize) {
    let mut text = String::new();
    text.push_str(&format!("#PATH {}\n", e.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()));
    text.push_str(&crate::stream::write(&e.doc));
    let _ = write_durable(&recovery_file(e, slot), text.as_bytes());
}

pub fn clear_recovery(e: &Editor, slot: usize) {
    let _ = std::fs::remove_file(recovery_file(e, slot));
}

pub struct Recovered {
    pub file: PathBuf,
    pub path: Option<PathBuf>,
    pub doc: Document,
    pub when: SystemTime,
}

/// Recovery copies left behind by a crash / power loss.
pub fn pending_recoveries() -> Vec<Recovered> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(recovery_dir()) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "wps") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let Some((first, rest)) = text.split_once('\n') else { continue };
        let path = first.strip_prefix("#PATH ").map(|s| s.trim()).filter(|s| !s.is_empty()).map(PathBuf::from);
        let Some(doc) = crate::stream::read(rest) else { continue };
        let when = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::now());
        out.push(Recovered { file: p, path, doc, when });
    }
    out.sort_by_key(|r| r.when);
    out
}

// -------------------------------------------------------------- history

#[derive(Clone, Debug)]
pub struct Revision {
    pub file: PathBuf,
    pub when: SystemTime,
    /// "save", "autosave", "outside" (changed by another program), "recovered"
    pub kind: String,
    pub words: usize,
}

/// Store a snapshot of `doc` (no-op if identical to the latest one).
pub fn record(doc: &Document, path: Option<&Path>, kind: &str) {
    if doc.id.is_empty() {
        return;
    }
    let dir = history_dir(&doc.id);
    let fp = fingerprint(doc);
    let existing = list(&doc.id);
    if let Some(last) = existing.last() {
        if let Some(prev) = load_revision(&last.file) {
            if fingerprint(&prev) == fp {
                return;
            }
        }
    }
    // sequence number keeps names unique and sorted even within one second
    let seq = existing
        .last()
        .and_then(|r| r.file.file_name()?.to_str()?.split('-').next()?.parse::<u64>().ok())
        .map(|n| n + 1)
        .unwrap_or(1);
    let name = format!("{seq:06}-{}-{}.wps", stamp(SystemTime::now()), kind);
    let mut text = format!("#PATH {}\n", path.map(|p| p.display().to_string()).unwrap_or_default());
    text.push_str(&crate::stream::write(doc));
    let _ = write_durable(&dir.join(name), text.as_bytes());
    prune(&doc.id, 500);
}

pub fn list(id: &str) -> Vec<Revision> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(history_dir(id)) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "wps") {
            continue;
        }
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let kind = stem.rsplit('-').next().unwrap_or("save").to_string();
        let when = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let words = std::fs::read_to_string(&p)
            .ok()
            .and_then(|t| t.split_once('\n').and_then(|(_, r)| crate::stream::read(r)))
            .map(|d| d.plain_text(0, d.len()).split_whitespace().count())
            .unwrap_or(0);
        out.push(Revision { file: p, when, kind, words });
    }
    out.sort_by(|a, b| a.file.file_name().cmp(&b.file.file_name()));
    out
}

pub fn load_revision(file: &Path) -> Option<Document> {
    let text = std::fs::read_to_string(file).ok()?;
    let (_, rest) = text.split_once('\n')?;
    crate::stream::read(rest)
}

fn prune(id: &str, keep: usize) {
    let l = list(id);
    if l.len() > keep {
        for r in &l[..l.len() - keep] {
            let _ = std::fs::remove_file(&r.file);
        }
    }
}

/// After opening a document: give it an id if it has none, and record a
/// revision if its content differs from the last one we know (= it was
/// changed by another program in between).
pub fn on_open(doc: &mut Document, path: &Path) -> bool {
    if doc.id.is_empty() {
        doc.id = new_doc_id();
        return false;
    }
    let revs = list(&doc.id);
    let Some(last) = revs.last() else { return false };
    let Some(prev) = load_revision(&last.file) else { return false };
    if fingerprint(&prev) != fingerprint(doc) {
        record(doc, Some(path), "outside");
        return true;
    }
    false
}

/// State for timed recovery / autosave, one per program.
pub struct Timers {
    pub last_recovery: Instant,
    pub last_autosave: Instant,
}

impl Default for Timers {
    fn default() -> Self {
        Timers { last_recovery: Instant::now(), last_autosave: Instant::now() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durable_write_and_history() {
        let dir = std::env::temp_dir().join(format!("wp51hist-{}", std::process::id()));
        let _ = crate::settings::CONFIG_OVERRIDE.set(dir.clone());
        let mut d = Document::from_text("eins");
        d.id = new_doc_id();
        record(&d, None, "save");
        record(&d, None, "save"); // identical → not stored again
        d.els.push(El::Ch('!'));
        record(&d, None, "save");
        let l = list(&d.id);
        assert_eq!(l.len(), 2, "{l:?}");
        let back = load_revision(&l[1].file).unwrap();
        assert_eq!(back.plain_text(0, back.len()), "eins!");
        // an outside change is detected
        let mut outside = d.clone();
        outside.els.push(El::Ch('?'));
        assert!(on_open(&mut outside, Path::new("x.docx")));
        assert_eq!(list(&d.id).len(), 3);
    }
}
