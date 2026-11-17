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

/// Snapshot files: `.bsw`, or `.wps` as written by older versions.
fn is_snapshot(p: &Path) -> bool {
    p.extension().is_some_and(|x| x == "bsw" || x == "wps")
}

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
//
// Every running window owns a folder `Recovery/<pid>-<time>/` and holds an
// exclusive lock on its `owner.lock` while it runs.  A folder whose lock can
// be taken belongs to a window that is gone (crash, power cut): only those
// copies are offered at start-up, never the live copies of another open
// window.  The folder also lists the files the window has open (`open.txt`)
// so another window can warn before the same file is edited twice.

struct Instance {
    dir: PathBuf,
    _lock: std::fs::File,
}

static INSTANCE: std::sync::OnceLock<Option<Instance>> = std::sync::OnceLock::new();

fn instance() -> Option<&'static Instance> {
    INSTANCE
        .get_or_init(|| {
            let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let dir = recovery_dir().join(format!("{}-{t}", std::process::id()));
            std::fs::create_dir_all(&dir).ok()?;
            let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("owner.lock")).ok()?;
            f.try_lock().ok()?;
            Some(Instance { dir, _lock: f })
        })
        .as_ref()
}

/// This window's recovery folder (falls back to the shared folder if the
/// lock can't be made).
fn own_dir() -> PathBuf {
    instance().map(|i| i.dir.clone()).unwrap_or_else(recovery_dir)
}

/// Folders of other windows: (folder, still running?).
fn other_windows() -> Vec<(PathBuf, bool)> {
    let own = instance().map(|i| i.dir.clone());
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(recovery_dir()) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() || Some(&p) == own.as_ref() {
            continue;
        }
        let live = match std::fs::OpenOptions::new().write(true).open(p.join("owner.lock")) {
            Ok(f) => match f.try_lock() {
                Ok(()) => {
                    let _ = f.unlock();
                    false
                }
                Err(_) => true,
            },
            Err(_) => false,
        };
        out.push((p, live));
    }
    out
}

/// Two paths name the same file (case-insensitive on Windows).
pub fn same_path(a: &Path, b: &Path) -> bool {
    norm_path(a) == norm_path(b)
}

fn norm_path(p: &Path) -> String {
    let p = std::fs::canonicalize(p).or_else(|_| std::path::absolute(p)).unwrap_or_else(|_| p.to_path_buf());
    let s = p.display().to_string();
    let s = s.strip_prefix(r"\\?\UNC\").map(|r| format!(r"\\{r}")).unwrap_or_else(|| s.strip_prefix(r"\\?\").unwrap_or(&s).to_string());
    if cfg!(windows) { s.to_lowercase() } else { s }
}

static OPEN_FILES: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Tell other windows which files this window has open.
pub fn note_open_files(paths: Vec<PathBuf>) {
    let Ok(mut last) = OPEN_FILES.lock() else { return };
    if *last == paths && instance().is_some() {
        return;
    }
    let Some(inst) = instance() else { return };
    let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    if write_durable(&inst.dir.join("open.txt"), text.as_bytes()).is_ok() {
        *last = paths;
    }
}

/// Is `p` open in another running window?
pub fn open_elsewhere(p: &Path) -> bool {
    other_windows().into_iter().filter(|(_, live)| *live).any(|(d, _)| {
        std::fs::read_to_string(d.join("open.txt"))
            .map(|t| t.lines().any(|l| !l.trim().is_empty() && same_path(Path::new(l.trim()), p)))
            .unwrap_or(false)
    })
}

fn recovery_file(e: &Editor, slot: usize) -> PathBuf {
    let key = if e.doc.id.is_empty() { "unnamed".to_string() } else { e.doc.id.clone() };
    own_dir().join(format!("doc{}-{key}.bsw", slot + 1))
}

/// Write the recovery copy for a modified document.
pub fn write_recovery(e: &Editor, slot: usize) -> std::io::Result<()> {
    let mut text = String::new();
    text.push_str(&format!("#PATH {}\n", e.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()));
    text.push_str(&crate::stream::write(&e.doc));
    write_durable(&recovery_file(e, slot), text.as_bytes())
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

fn read_recovery(p: &Path, out: &mut Vec<Recovered>) {
    if !is_snapshot(p) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(p) else { return };
    let Some((first, rest)) = text.split_once('\n') else { return };
    let path = first.strip_prefix("#PATH ").map(|s| s.trim()).filter(|s| !s.is_empty()).map(PathBuf::from);
    let Some(doc) = crate::stream::read(rest) else { return };
    let when = std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(SystemTime::now());
    out.push(Recovered { file: p.to_path_buf(), path, doc, when });
}

/// One recovery copy (for `--recover <file>`: a copy opened in a window of
/// its own when the first window had no free document).
pub fn read_recovery_file(p: &Path) -> Option<Recovered> {
    let mut v = vec![];
    read_recovery(p, &mut v);
    v.pop()
}

/// Recovery copies left behind by a crash / power loss (never those of a
/// window that is still open).  Empty folders of closed windows are removed.
pub fn pending_recoveries() -> Vec<Recovered> {
    let mut out = vec![];
    // loose files: written by older versions
    if let Ok(rd) = std::fs::read_dir(recovery_dir()) {
        for e in rd.flatten() {
            if e.path().is_file() {
                read_recovery(&e.path(), &mut out);
            }
        }
    }
    for (dir, live) in other_windows() {
        if live {
            continue;
        }
        let before = out.len();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                read_recovery(&e.path(), &mut out);
            }
        }
        if out.len() == before {
            remove_window_dir(&dir);
        }
    }
    out.sort_by_key(|r| r.when);
    out
}

fn remove_window_dir(dir: &Path) {
    let _ = std::fs::remove_file(dir.join("owner.lock"));
    let _ = std::fs::remove_file(dir.join("open.txt"));
    let _ = std::fs::remove_dir(dir);
}

/// A recovery copy has been dealt with (recovered or declined).
pub fn remove_recovery_file(p: &Path) {
    let _ = std::fs::remove_file(p);
    if let Some(dir) = p.parent() {
        let own = instance().map(|i| i.dir.as_path());
        if dir != recovery_dir() && Some(dir) != own {
            let left = std::fs::read_dir(dir).map(|rd| rd.flatten().any(|e| is_snapshot(&e.path()))).unwrap_or(true);
            if !left {
                remove_window_dir(dir);
            }
        }
    }
}

/// On a normal exit: remove this window's (empty) recovery folder.
pub fn shutdown() {
    if let Some(i) = instance() {
        let left = std::fs::read_dir(&i.dir).map(|rd| rd.flatten().any(|e| is_snapshot(&e.path()))).unwrap_or(true);
        if !left {
            let _ = std::fs::remove_file(i.dir.join("open.txt"));
            // the lock file can't be removed while we hold it on Windows;
            // the next start cleans the folder up
            let _ = std::fs::remove_file(i.dir.join("owner.lock"));
            let _ = std::fs::remove_dir(&i.dir);
        }
    }
}

// -------------------------------------------------------------- history

#[derive(Clone, Debug)]
pub struct Revision {
    pub file: PathBuf,
    pub when: SystemTime,
    /// "save", "autosave", "outside" (changed by another program), "recovered"
    pub kind: String,
    pub words: usize,
    /// The first words (only filled for the list of never-saved documents).
    pub preview: String,
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
            // the same text under the same name is not stored twice (a new
            // name is: e.g. the first save of a never-named document)
            let same_name = match (revision_path(&last.file), path) {
                (None, None) => true,
                (Some(a), Some(b)) => same_path(&a, b),
                _ => false,
            };
            if fingerprint(&prev) == fp && same_name {
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
    let name = format!("{seq:06}-{}-{}.bsw", stamp(SystemTime::now()), kind);
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
        if !is_snapshot(&p) {
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
        out.push(Revision { file: p, when, kind, words, preview: String::new() });
    }
    out.sort_by(|a, b| a.file.file_name().cmp(&b.file.file_name()));
    out
}

/// The file a revision was saved as (`None` for a never-saved document).
pub fn revision_path(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let first = text.lines().next()?;
    first.strip_prefix("#PATH ").map(|s| s.trim()).filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// Documents that were never saved under a name but have a kept version
/// (left without saving, recovered, …), newest first: the latest version of
/// each.  Otherwise these could not be reached, as Ctrl-F5, 6 lists the
/// history of the open document.
pub fn unsaved_documents() -> Vec<Revision> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(config_dir().join("History")) else { return out };
    for e in rd.flatten() {
        // the newest snapshot (names start with a sequence number)
        let Some(file) = std::fs::read_dir(e.path())
            .into_iter()
            .flatten()
            .flatten()
            .map(|f| f.path())
            .filter(|p| is_snapshot(p))
            .max_by(|a, b| a.file_name().cmp(&b.file_name()))
        else {
            continue;
        };
        if revision_path(&file).is_some() {
            continue;
        }
        let Some(d) = load_revision(&file) else { continue };
        let text = d.plain_text(0, d.len());
        let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(Revision {
            when: std::fs::metadata(&file).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH),
            kind: stem.rsplit('-').next().unwrap_or("save").to_string(),
            words: text.split_whitespace().count(),
            preview: text.split_whitespace().take(12).collect::<Vec<_>>().join(" "),
            file,
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.when));
    out
}

fn newest_snapshot(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|f| f.path())
        .filter(|p| is_snapshot(p))
        .max_by(|a, b| a.file_name().cmp(&b.file_name()))
}

/// The history id of the document last kept under file name `path`.
pub fn find_id_for_path(path: &Path) -> Option<String> {
    let want = norm_path(path);
    let mut best: Option<(SystemTime, String)> = None;
    for e in std::fs::read_dir(config_dir().join("History")).ok()?.flatten() {
        let Some(file) = newest_snapshot(&e.path()) else { continue };
        let Some(p) = revision_path(&file) else { continue };
        if norm_path(&p) == want {
            let t = file_stamp(&file).unwrap_or(SystemTime::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                best = Some((t, e.file_name().to_string_lossy().into_owned()));
            }
        }
    }
    best.map(|b| b.1)
}

// ------------------------------------------------- never-named documents
//
// A document without a name is autosaved as a real Word file in
// `<Documents or home directory>/Blue Screen Writer 51 - Unsaved/`, named
// `BSW-Unsaved-XXXXXX.docx` after its id.  Once it is saved under a name,
// or deliberately left without saving, the file is removed (the text stays
// in the history).

pub const UNSAVED_DIR: &str = "Blue Screen Writer 51 - Unsaved";

pub fn unsaved_file(base: &Path, id: &str) -> PathBuf {
    let code: String = id.chars().take(6).collect::<String>().to_uppercase();
    base.join(UNSAVED_DIR).join(format!("BSW-Unsaved-{code}.docx"))
}

/// Is `p` one of the autosave files of never-named documents?
pub fn is_unsaved_file(p: &Path) -> bool {
    p.parent().and_then(|d| d.file_name()).is_some_and(|n| n == UNSAVED_DIR)
        && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("BSW-Unsaved-"))
}

/// Remove an autosave file of a never-named document (and the folder once
/// it is empty).
pub fn remove_unsaved_file(p: &Path) {
    if is_unsaved_file(p) {
        let _ = std::fs::remove_file(p);
        let mut bk = p.as_os_str().to_owned();
        bk.push(".bk!");
        let _ = std::fs::remove_file(PathBuf::from(bk));
        if let Some(d) = p.parent() {
            let _ = std::fs::remove_dir(d);
        }
    }
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
        // Formats that can't carry the id (.txt, .doc, .wpd …, Word files
        // from elsewhere): find the history by the file's name.  Such
        // formats don't keep every code, so no "changed outside" check.
        doc.id = find_id_for_path(path).unwrap_or_else(new_doc_id);
        return false;
    }
    let revs = list(&doc.id);
    // A copy made in Explorer carries the original's id: if the history
    // belongs to another file that still exists, this is a separate
    // document and gets its own id (and history).
    if let Some(orig) = revs.iter().rev().find_map(|r| revision_path(&r.file)) {
        if !same_path(&orig, path) && orig.is_file() {
            doc.id = new_doc_id();
            return false;
        }
    }
    // compare with the last version that was actually in the file (not
    // with text left without saving, crash copies …)
    let Some(last) = revs.iter().rev().find(|r| matches!(r.kind.as_str(), "save" | "autosave" | "outside")) else { return false };
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
    /// A failed recovery copy has been reported.
    pub recovery_warned: bool,
}

/// Modification time of a file.
pub fn file_stamp(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Default for Timers {
    fn default() -> Self {
        Timers { last_recovery: Instant::now(), last_autosave: Instant::now(), recovery_warned: false }
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

#[cfg(test)]
mod safety_tests {
    use super::*;
    use crate::keys::Input;
    use crate::ui::Wp;

    fn cfg() -> PathBuf {
        let _ = crate::settings::CONFIG_OVERRIDE.set(std::env::temp_dir().join(format!("wp51hist-{}", std::process::id())));
        config_dir()
    }

    /// Tests that look at all recovery copies run one at a time.
    static RECOVERY_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        RECOVERY_TESTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn snapshot(doc: &Document, path: &str) -> String {
        format!("#PATH {path}\n{}", crate::stream::write(doc))
    }

    fn long_ago() -> Instant {
        Instant::now().checked_sub(Duration::from_secs(600)).unwrap()
    }

    #[test]
    fn only_copies_of_closed_windows_are_offered() {
        let _g = serial();
        cfg();
        let mut e = Editor::with_doc(Document::from_text("live here"), None);
        e.doc.id = new_doc_id();
        write_recovery(&e, 0).unwrap();
        let own = recovery_file(&e, 0);
        assert!(own.exists() && own.parent().unwrap() != recovery_dir());

        let tag = new_doc_id();
        // a window that crashed: its lock is free
        let orphan = recovery_dir().join(format!("orphan-{tag}"));
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join("owner.lock"), "").unwrap();
        let lost = orphan.join("doc1-x.bsw");
        std::fs::write(&lost, snapshot(&Document::from_text("lost work"), "")).unwrap();
        // a window that is still open: its lock is held
        let live = recovery_dir().join(format!("live-{tag}"));
        std::fs::create_dir_all(&live).unwrap();
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(live.join("owner.lock")).unwrap();
        lock.try_lock().unwrap();
        let busy = live.join("doc1-y.bsw");
        std::fs::write(&busy, snapshot(&Document::from_text("being edited"), "")).unwrap();
        let file = std::env::temp_dir().join(format!("open-{tag}.docx"));
        std::fs::write(live.join("open.txt"), format!("{}\n", file.display())).unwrap();

        let files: Vec<PathBuf> = pending_recoveries().into_iter().map(|r| r.file).collect();
        assert!(files.contains(&lost), "{files:?}");
        assert!(!files.contains(&busy) && !files.contains(&own), "{files:?}");
        assert!(open_elsewhere(&file));
        assert!(!open_elsewhere(&std::env::temp_dir().join("something-else.docx")));

        remove_recovery_file(&lost);
        assert!(!orphan.exists(), "an empty folder of a closed window is removed");
        clear_recovery(&e, 0);
        drop(lock);
        let _ = std::fs::remove_dir_all(&live);
    }

    #[test]
    fn empty_folders_of_closed_windows_are_removed() {
        let _g = serial();
        cfg();
        let old = recovery_dir().join(format!("closed-{}", new_doc_id()));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("owner.lock"), "").unwrap();
        pending_recoveries();
        assert!(!old.exists());
    }

    #[test]
    fn same_document_in_both_windows_keeps_two_recovery_copies() {
        cfg();
        let mut a = Editor::with_doc(Document::from_text("one"), None);
        a.doc.id = new_doc_id();
        let mut b = Editor::with_doc(Document::from_text("two"), None);
        b.doc.id = a.doc.id.clone();
        assert_ne!(recovery_file(&a, 0), recovery_file(&b, 1));
    }

    #[test]
    fn a_copy_made_in_explorer_gets_its_own_history() {
        let dir = cfg().join(format!("copy-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let orig = dir.join("Brief.docx");
        std::fs::write(&orig, "x").unwrap();
        let mut d = Document::from_text("Brief");
        d.id = new_doc_id();
        record(&d, Some(&orig), "save");
        // a copy elsewhere while the original still exists → new id
        let mut copy = d.clone();
        assert!(!on_open(&mut copy, &dir.join("Brief - Kopie.docx")));
        assert_ne!(copy.id, d.id);
        // the same file → same id
        let mut same = d.clone();
        on_open(&mut same, &orig);
        assert_eq!(same.id, d.id);
        // moved (original gone) → still the same document
        std::fs::remove_file(&orig).unwrap();
        let mut moved = d.clone();
        on_open(&mut moved, &dir.join("Archiv.docx"));
        assert_eq!(moved.id, d.id);
    }

    #[test]
    fn never_saved_documents_can_be_found() {
        cfg();
        let mut d = Document::from_text("Sehr geehrte Damen und Herren, anbei die Übersetzung");
        d.id = new_doc_id();
        record(&d, None, "discarded");
        let l = unsaved_documents();
        let r = l.iter().find(|r| r.file.parent() == Some(&history_dir(&d.id))).expect("listed");
        assert!(r.preview.starts_with("Sehr geehrte Damen"), "{}", r.preview);
        assert_eq!(r.kind, "discarded");
    }

    #[test]
    fn autosave_never_overwrites_a_newer_file() {
        let dir = cfg().join(format!("auto-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Brief.docx");
        crate::fileio::save(&p, &Document::from_text("Version eins")).unwrap();
        let mut wp = Wp::new(80, 25);
        wp.retrieve(&p);
        wp.input(Input::Char('X'));
        // another window saves the file meanwhile
        crate::fileio::save(&p, &Document::from_text("Newer from elsewhere")).unwrap();
        let f = std::fs::File::options().write(true).open(&p).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        drop(f);
        wp.timers.last_autosave = long_ago();
        wp.maybe_backup();
        let d = crate::fileio::load(&p).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "Newer from elsewhere");
        assert!(wp.ed_ref().modified);
        assert!(wp.message.as_deref().unwrap_or("").contains("not autosaved"), "{:?}", wp.message);
        // after an explicit save, autosave works again
        wp.do_save(&p);
        wp.input(Input::Char('Y'));
        wp.timers.last_autosave = long_ago();
        wp.maybe_backup();
        assert!(!wp.ed_ref().modified);
        let d = crate::fileio::load(&p).unwrap();
        assert!(d.plain_text(0, d.len()).contains('Y'));
    }

    #[test]
    fn save_as_another_name_starts_a_separate_history() {
        let dir = cfg().join(format!("saveas-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut wp = Wp::new(80, 25);
        wp.input(Input::Char('A'));
        let id0 = wp.ed_ref().doc.id.clone();
        wp.do_save(&dir.join("a.docx"));
        assert_eq!(wp.ed_ref().doc.id, id0, "the first save keeps the id");
        wp.do_save(&dir.join("a.docx"));
        assert_eq!(wp.ed_ref().doc.id, id0);
        wp.do_save(&dir.join("b.docx"));
        assert_ne!(wp.ed_ref().doc.id, id0);
    }

    #[test]
    fn unmarked_changes_still_get_a_recovery_copy() {
        cfg();
        let mut wp = Wp::new(80, 25);
        wp.timers.last_recovery = long_ago();
        wp.maybe_backup(); // remembers the clean state
        assert!(!wp.ed_ref().modified);
        wp.ed().doc.els.push(El::Ch('Z')); // a change that forgot `modified`
        wp.timers.last_recovery = long_ago();
        wp.maybe_backup();
        assert!(wp.ed_ref().modified);
        let f = recovery_file(wp.ed_ref(), wp.cur);
        assert!(f.exists());
        clear_recovery(wp.ed_ref(), wp.cur);
    }

    fn force_autosave(wp: &mut Wp) {
        wp.timers.last_autosave = long_ago();
        wp.maybe_backup();
    }

    #[test]
    fn unnamed_document_is_autosaved_until_it_gets_a_name() {
        let base = cfg();
        let mut wp = Wp::new(80, 25);
        for c in "Entwurf".chars() {
            wp.input(Input::Char(c));
        }
        force_autosave(&mut wp);
        let f = unsaved_file(&base, &wp.ed_ref().doc.id);
        assert!(f.is_file(), "{} {:?} modified={}", f.display(), wp.message, wp.ed_ref().modified);
        assert!(is_unsaved_file(&f));
        assert!(f.file_name().unwrap().to_string_lossy().starts_with("BSW-Unsaved-"));
        let d = crate::fileio::load(&f).unwrap();
        assert_eq!(d.plain_text(0, d.len()), "Entwurf");
        assert!(wp.ed_ref().modified, "still unsaved: F7 asks for a name");
        // named → the autosave file goes, the history continues
        let dir = base.join(format!("named-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = wp.ed_ref().doc.id.clone();
        wp.do_save(&dir.join("Brief.docx"));
        assert!(!f.exists());
        assert_eq!(wp.ed_ref().doc.id, id);
        assert!(list(&id).len() >= 2);
    }

    #[test]
    fn blank_documents_never_get_a_file() {
        let base = cfg();
        // opened and left alone
        let mut wp = Wp::new(80, 25);
        force_autosave(&mut wp);
        assert!(!unsaved_file(&base, &wp.ed_ref().doc.id).exists());
        // only returns and spaces
        wp.input(Input::Func(crate::keys::Func::Enter));
        wp.input(Input::Char(' '));
        wp.input(Input::Func(crate::keys::Func::Enter));
        force_autosave(&mut wp);
        assert!(wp.ed_ref().modified);
        assert!(!unsaved_file(&base, &wp.ed_ref().doc.id).exists());
        // and left without saving
        wp.input(Input::Func(crate::keys::Func::Exit));
        wp.input(Input::Char('n'));
        wp.input(Input::Char('n'));
        assert!(!unsaved_file(&base, &wp.ed_ref().doc.id).exists());
    }

    #[test]
    fn leaving_an_unnamed_document_keeps_its_text_findable() {
        let base = cfg();
        let mut wp = Wp::new(80, 25);
        for c in "Nicht speichern".chars() {
            wp.input(Input::Char(c));
        }
        force_autosave(&mut wp);
        let id = wp.ed_ref().doc.id.clone();
        let f = unsaved_file(&base, &id);
        assert!(f.is_file());
        wp.input(Input::Char('!')); // typed after the last autosave
        wp.input(Input::Func(crate::keys::Func::Exit)); // Save document? → No
        wp.input(Input::Char('n'));
        wp.input(Input::Char('n')); // Exit? No → empty screen
        let d = crate::fileio::load(&f).expect("the BSW-Unsaved file is kept");
        assert_eq!(d.plain_text(0, d.len()), "Nicht speichern!");
        assert!(wp.ed_ref().is_empty());
        assert!(unsaved_documents().iter().any(|r| r.file.parent() == Some(&history_dir(&id))));
    }

    #[test]
    fn files_without_an_id_find_their_history_by_name() {
        let dir = cfg().join(format!("txt-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Notiz.txt");
        std::fs::write(&p, "Notiz").unwrap();
        let mut d = Document::from_text("Notiz");
        d.id = new_doc_id();
        record(&d, Some(&p), "save");
        let mut again = Document::from_text("Notiz (changed by Notepad)");
        assert!(!on_open(&mut again, &p), "no 'changed outside' for formats without every code");
        assert_eq!(again.id, d.id);
    }

    #[test]
    fn text_left_without_saving_is_not_reported_as_an_outside_change() {
        let dir = cfg().join(format!("disc-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Brief.docx");
        std::fs::write(&p, "x").unwrap();
        let mut d = Document::from_text("gespeichert");
        d.id = new_doc_id();
        record(&d, Some(&p), "save");
        let mut later = d.clone();
        later.els.push(El::Ch('!'));
        record(&later, Some(&p), "discarded");
        let mut reopened = d.clone();
        assert!(!on_open(&mut reopened, &p));
    }

    #[test]
    fn other_formats_keep_unsaved_changes_in_the_history() {
        let dir = cfg().join(format!("rtf-{}", new_doc_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Alt.txt");
        std::fs::write(&p, "alt").unwrap();
        let mut wp = Wp::new(80, 25);
        wp.retrieve(&p);
        wp.input(Input::Char('Q'));
        force_autosave(&mut wp);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "alt", "the file is not touched");
        let l = list(&wp.ed_ref().doc.id);
        assert_eq!(l.last().map(|r| r.kind.as_str()), Some("unsaved"), "{l:?}");
    }

    #[test]
    fn crash_copies_are_kept_before_any_question() {
        let _g = serial();
        cfg();
        let orphan = recovery_dir().join(format!("crashed-{}", new_doc_id()));
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join("owner.lock"), "").unwrap();
        let mut d = Document::from_text("vor dem Absturz");
        d.id = new_doc_id();
        std::fs::write(orphan.join("doc1-z.bsw"), snapshot(&d, "")).unwrap();
        let mut wp = Wp::new(80, 25);
        wp.offer_recoveries();
        let l = list(&d.id);
        assert_eq!(l.last().map(|r| r.kind.as_str()), Some("crash"));
        // answering Yes recovers it into the empty document
        while wp.has_screens() {
            wp.input(Input::Char('y'));
        }
        assert!(wp.docs.iter().any(|e| e.doc.id == d.id));
    }

    #[test]
    fn footnote_editor_leaves_no_stray_recovery_copy() {
        cfg();
        let mut wp = Wp::new(80, 25);
        crate::subedit::open(&mut wp, "Footnote", vec![], false, |_, _| {});
        wp.input(Input::Char('n'));
        wp.timers.last_recovery = long_ago();
        wp.maybe_backup();
        assert!(!recovery_file(wp.ed_ref(), wp.cur).exists());
    }
}
