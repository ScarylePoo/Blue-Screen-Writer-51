//! Hyperlinks and bookmarks: `[Link:url]…[Link End]`, `[Bookmark:name]`.
//! Ctrl-click on link text (or Enter on a `[Link:…]` code in Reveal Codes)
//! opens the address in the browser; `#name` jumps to a bookmark.

use crate::doc::*;
use crate::ui::Wp;

/// Set in headless test mode: links are only recorded, not launched.
pub static HEADLESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Addresses "opened" (for tests).
pub static OPENED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The link target in effect at element `i` (inside `[Link]…[Link End]`),
/// or the `[Link:…]` code itself at `i`.
pub fn link_at(els: &[El], i: usize) -> Option<String> {
    if let Some(El::Code(Code::Link(u))) = els.get(i) {
        return Some(u.clone());
    }
    for k in (0..i.min(els.len())).rev() {
        match &els[k] {
            El::Code(Code::Link(u)) => return Some(u.clone()),
            El::Code(Code::LinkEnd) => return None,
            _ => {}
        }
    }
    None
}

/// Index of `[Bookmark:name]`.
pub fn find_bookmark(els: &[El], name: &str) -> Option<usize> {
    els.iter().position(|e| matches!(e, El::Code(Code::Bookmark(n)) if n.eq_ignore_ascii_case(name)))
}

/// Make a typed address something the browser understands.
pub fn normalize_url(u: &str) -> String {
    let u = u.trim();
    let lower = u.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("mailto:") || lower.starts_with("file:") || u.starts_with('#') || u.starts_with("\\\\") || u.get(1..3) == Some(":\\") {
        u.to_string()
    } else if u.contains('@') && !u.contains('/') {
        format!("mailto:{u}")
    } else {
        format!("https://{u}")
    }
}

/// Follow a link: bookmark jump or open in the default browser / mail program.
pub fn follow(wp: &mut Wp, url: &str) {
    if let Some(name) = url.strip_prefix('#') {
        match find_bookmark(&wp.ed_ref().doc.els, name) {
            Some(i) => {
                wp.ed().block_anchor = None;
                wp.ed().cursor = i + 1;
                wp.after_edit();
            }
            None => wp.flash(&format!("ERROR: Bookmark not found — {name}")),
        }
        return;
    }
    let url = normalize_url(url);
    OPENED.lock().unwrap().push(url.clone());
    wp.flash(&format!("Opening {url}"));
    if HEADLESS.load(std::sync::atomic::Ordering::Relaxed) || cfg!(test) {
        return;
    }
    if let Err(e) = open_external(&url) {
        wp.flash(&format!("ERROR: Can't open {url} — {e}"));
    }
}

#[cfg(windows)]
pub fn open_external(target: &str) -> Result<(), String> {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(hwnd: isize, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
    }
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (op, file) = (w("open"), w(target));
    // SAFETY: valid NUL-terminated UTF-16 strings that live across the call.
    let r = unsafe { ShellExecuteW(0, op.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
    if r > 32 { Ok(()) } else { Err(format!("error {r}")) }
}

#[cfg(not(windows))]
pub fn open_external(target: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open").arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Enter in Reveal Codes with the cursor on a `[Link:…]` code.
pub fn enter_on_link(wp: &mut Wp) -> bool {
    let e = wp.ed_ref();
    if !e.reveal || e.block_anchor.is_some() {
        return false;
    }
    let Some(El::Code(Code::Link(u))) = e.doc.els.get(e.cursor) else { return false };
    let u = u.clone();
    follow(wp, &u);
    true
}

/// Ctrl-click at a screen cell: follow the link under the mouse.
pub fn ctrl_click(wp: &mut Wp, col: usize, row: usize) -> bool {
    if wp.has_screens() {
        return false;
    }
    let Some(i) = wp.doc_index_at(col, row) else { return false };
    match link_at(&wp.ed_ref().doc.els, i) {
        Some(u) => {
            follow(wp, &u);
            true
        }
        None => false,
    }
}

/// Block + Ctrl-F5 "Hyperlink": wrap the block (or insert at the cursor)
/// `[Link:url]…[Link End]`.
pub fn insert_link(wp: &mut Wp, url: &str) {
    let url = url.trim().to_string();
    if url.is_empty() {
        return;
    }
    let e = wp.ed();
    if let Some((s, t)) = e.block_range() {
        e.block_anchor = None;
        e.doc.els.insert(t, El::Code(Code::LinkEnd));
        e.doc.els.insert(s, El::Code(Code::Link(url)));
        e.cursor = t + 2;
        e.modified = true;
        e.relayout();
    } else {
        let mut els = vec![El::Code(Code::Link(url.clone()))];
        els.extend(url.trim_start_matches('#').chars().map(El::Ch));
        els.push(El::Code(Code::LinkEnd));
        e.insert_els(&els);
    }
    wp.after_edit();
}

pub fn insert_bookmark(wp: &mut Wp, name: &str) {
    let name = name.trim();
    if !name.is_empty() {
        wp.ed().insert_code(Code::Bookmark(name.to_string()));
        wp.after_edit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut d = Document::from_text("See here now");
        d.els.insert(4, El::Code(Code::Link("https://example.org/a?b=1&c=2".into())));
        d.els.insert(9, El::Code(Code::LinkEnd));
        d.els.insert(0, El::Code(Code::Bookmark("Anfang".into())));
        d
    }

    #[test]
    fn link_lookup() {
        let d = sample();
        let i = d.els.iter().position(|e| *e == El::Ch('h')).unwrap();
        assert_eq!(link_at(&d.els, i).as_deref(), Some("https://example.org/a?b=1&c=2"));
        assert_eq!(link_at(&d.els, 1), None);
        assert_eq!(link_at(&d.els, d.els.len() - 1), None);
        assert_eq!(find_bookmark(&d.els, "anfang"), Some(0));
        assert_eq!(normalize_url("www.x.de"), "https://www.x.de");
        assert_eq!(normalize_url("a@b.de"), "mailto:a@b.de");
    }

    #[test]
    fn docx_roundtrip_private_and_xml() {
        let d = sample();
        let bytes = crate::docx::write_docx(&d).unwrap();
        let d2 = crate::docx::read_docx(&bytes).unwrap();
        assert_eq!(d.els, d2.els);
        // Word's view of it (document.xml only)
        let mut parts = crate::docx::Parts::default();
        let xml = crate::docx::document_xml(&d, &mut parts);
        assert!(xml.contains("<w:hyperlink r:id=\"rIdL0\""), "{xml}");
        assert!(xml.contains("w:bookmarkStart"));
        assert_eq!(parts.links, vec!["https://example.org/a?b=1&c=2".to_string()]);
    }

    #[test]
    fn docx_word_style_links() {
        // a hyperlink as Word writes it, an anchor link and a HYPERLINK field
        let xml = r#"<w:document xmlns:w="w" xmlns:r="r"><w:body><w:p><w:bookmarkStart w:id="0" w:name="_GoBack"/><w:bookmarkStart w:id="1" w:name="Ziel"/><w:bookmarkEnd w:id="1"/><w:hyperlink r:id="rId9" w:history="1"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>Web</w:t></w:r></w:hyperlink><w:r><w:t xml:space="preserve"> </w:t></w:r><w:hyperlink w:anchor="Ziel"><w:r><w:t>Sprung</w:t></w:r></w:hyperlink><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "http://f.de" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Feld</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:body></w:document>"#;
        let mut d = crate::docx::parse_document_xml(xml, None);
        let rels = r#"<Relationships><Relationship Id="rId9" Type="x/hyperlink" Target="https://w.de/" TargetMode="External"/></Relationships>"#;
        crate::docx::resolve_links_pub(&mut d.els, rels);
        let codes: Vec<String> = d.els.iter().filter_map(|e| e.code()).map(crate::reveal::code_text).collect();
        assert_eq!(codes, vec!["[Bookmark:Ziel]", "[Link:https://w.de/]", "[Link End]", "[Link:#Ziel]", "[Link End]", "[Link:http://f.de]", "[Link End]"]);
        assert_eq!(d.plain_text(0, d.len()), "Web SprungFeld");
    }

    #[test]
    fn stream_roundtrip() {
        let d = sample();
        let s = crate::stream::write(&d);
        assert_eq!(crate::stream::read(&s).unwrap().els, d.els);
    }
}
