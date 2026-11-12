//! Graphics boxes and lines in .docx: pictures as `<w:drawing>` (inline
//! for character boxes, anchored otherwise) with `word/media/` parts, text
//! and equation boxes (and captioned figures) as `wps:txbx` text boxes,
//! graphic lines as filled shapes — and back: Word pictures and text
//! boxes become Figure / Text Box codes.

use crate::doc::*;
use crate::gfx::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

const EMU: i64 = 762; // EMU per WPU (914400 / 1200)
pub const NS: &str = "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"";

static DOC_PR: AtomicU32 = AtomicU32::new(1);

/// A media part to write: (file name inside word/media, bytes, extension).
pub type Media = Vec<(String, Vec<u8>, &'static str)>;

fn emu(w: i32) -> i64 {
    w as i64 * EMU
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Add an image to the media list; returns its relationship id.
fn add_media(b: &GBox, media: &mut Media) -> Option<String> {
    let data = b.data.as_ref()?;
    let (bytes, ext) = match crate::raster::sniff(data)? {
        crate::raster::Format::Wpg => (crate::raster::to_png(&*crate::raster::decode(data)?)?, "png"),
        f => (data.to_vec(), f.ext()),
    };
    // the same picture twice → one part
    if let Some(k) = media.iter().position(|m| m.1 == bytes) {
        return Some(format!("rIdImg{}", k + 1));
    }
    media.push((format!("image{}.{ext}", media.len() + 1), bytes, ext));
    Some(format!("rIdImg{}", media.len()))
}

fn pic_xml(rid: &str, name: &str, cx: i64, cy: i64, rot: i32, flip: bool, id: u32, border: bool) -> String {
    let ln = if border { "<a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill></a:ln>" } else { "" };
    let rot = if rot != 0 { format!(" rot=\"{}\"", (360 - rot).rem_euclid(360) as i64 * 60000) } else { String::new() };
    let flip = if flip { " flipH=\"1\"" } else { "" };
    format!(
        "<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm{rot}{flip}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{ln}</pic:spPr></pic:pic></a:graphicData></a:graphic>",
        esc(name)
    )
}

fn inline_xml(cx: i64, cy: i64, id: u32, name: &str, graphic: &str) -> String {
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:docPr id=\"{id}\" name=\"{}\"/><wp:cNvGraphicFramePr/>{graphic}</wp:inline></w:drawing></w:r>",
        esc(name)
    )
}

struct Pos {
    h: String,
    v: String,
    wrap: &'static str,
    dist: [i64; 4],
}

fn pos_of(b: &GBox, o: &BoxOpts) -> Pos {
    let h = match b.hpos {
        HPos::Left => "<wp:positionH relativeFrom=\"margin\"><wp:align>left</wp:align></wp:positionH>".into(),
        HPos::Right => "<wp:positionH relativeFrom=\"margin\"><wp:align>right</wp:align></wp:positionH>".into(),
        HPos::Center | HPos::Full => "<wp:positionH relativeFrom=\"margin\"><wp:align>center</wp:align></wp:positionH>".into(),
        HPos::Set(x) => format!("<wp:positionH relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionH>", emu(x)),
    };
    let v = match (b.anchor, b.vpos) {
        (Anchor::Page, VPos::Top | VPos::Full) => "<wp:positionV relativeFrom=\"margin\"><wp:align>top</wp:align></wp:positionV>".into(),
        (Anchor::Page, VPos::Center) => "<wp:positionV relativeFrom=\"margin\"><wp:align>center</wp:align></wp:positionV>".into(),
        (Anchor::Page, VPos::Bottom) => "<wp:positionV relativeFrom=\"margin\"><wp:align>bottom</wp:align></wp:positionV>".into(),
        (Anchor::Page, VPos::Set(y)) => format!("<wp:positionV relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionV>", emu(y)),
        (_, VPos::Set(y)) => format!("<wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>{}</wp:posOffset></wp:positionV>", emu(y)),
        _ => "<wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV>".into(),
    };
    let wrap = if !b.wrap {
        "<wp:wrapNone/>"
    } else if b.hpos == HPos::Full || b.hpos == HPos::Center {
        "<wp:wrapTopAndBottom/>"
    } else {
        "<wp:wrapSquare wrapText=\"bothSides\"/>"
    };
    Pos { h, v, wrap, dist: [emu(o.outside[2]), emu(o.outside[3]), emu(o.outside[0]), emu(o.outside[1])] }
}

fn anchor_xml(p: &Pos, cx: i64, cy: i64, id: u32, name: &str, graphic: &str) -> String {
    format!(
        "<w:r><w:drawing><wp:anchor distT=\"{}\" distB=\"{}\" distL=\"{}\" distR=\"{}\" simplePos=\"0\" relativeHeight=\"{}\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/>{}{}<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>{}<wp:docPr id=\"{id}\" name=\"{}\"/><wp:cNvGraphicFramePr/>{graphic}</wp:anchor></w:drawing></w:r>",
        p.dist[0],
        p.dist[1],
        p.dist[2],
        p.dist[3],
        251658240 + id,
        p.h,
        p.v,
        p.wrap,
        esc(name)
    )
}

fn shape_xml(cx: i64, cy: i64, fill: Option<u8>, line: bool, ins: [i64; 4], content: Option<&str>) -> String {
    let fill = match fill {
        Some(g) if g > 0 => {
            let v = 255 - (g.min(100) as u32 * 255 / 100);
            format!("<a:solidFill><a:srgbClr val=\"{v:02X}{v:02X}{v:02X}\"/></a:solidFill>")
        }
        _ => "<a:noFill/>".into(),
    };
    let ln = if line { "<a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill></a:ln>" } else { "<a:ln><a:noFill/></a:ln>" };
    let (tb, body) = match content {
        Some(c) => (" txBox=\"1\"", format!("<wps:txbx><w:txbxContent>{c}</w:txbxContent></wps:txbx>")),
        None => ("", String::new()),
    };
    format!(
        "<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr{tb}/><wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{fill}{ln}</wps:spPr>{body}<wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" anchor=\"t\" anchorCtr=\"0\"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic>",
        ins[0], ins[2], ins[1], ins[3]
    )
}

/// The box as a run (or runs) for document.xml.  `inner` is the box text
/// as paragraphs, `caption` the caption paragraphs (already expanded).
pub fn box_xml(doc: &Document, b: &GBox, o: &BoxOpts, inner: &str, caption: &str, media: &mut Media) -> String {
    let id = DOC_PR.fetch_add(1, Ordering::Relaxed);
    let m = crate::layout::MonoMetrics;
    let avail = doc.init.page_width - doc.init.left - doc.init.right;
    let (w, h) = frame_size(doc, b, o, avail, doc.init.page_height - doc.init.top - doc.init.bottom, &m);
    let name = if b.file.is_empty() { format!("{} {id}", b.kind.name()) } else { b.file.clone() };
    let p = pos_of(b, o);
    let ins = [emu(o.inside[0]), emu(o.inside[1]), emu(o.inside[2]), emu(o.inside[3])];
    let border = o.border.iter().any(|x| *x != 0);
    // the picture itself, fitted into the inner frame
    let picture = |media: &mut Media, border: bool| -> Option<(String, i64, i64)> {
        if b.contents != Contents::Image {
            return None;
        }
        let rid = add_media(b, media)?;
        let (nw, nh) = crate::raster::dims(b.data.as_ref()?)?;
        let (iw, ih) = ((w - o.inside[0] - o.inside[1]).max(120), (h - o.inside[2] - o.inside[3]).max(120));
        let k = (iw as f64 / nw.max(1) as f64).min(ih as f64 / nh.max(1) as f64);
        let (cx, cy) = (emu((nw as f64 * k * b.img.sx as f64 / 100.0) as i32), emu((nh as f64 * k * b.img.sy as f64 / 100.0) as i32));
        Some((pic_xml(&rid, &name, cx, cy, b.img.rot, b.img.mirror, id, border), cx, cy))
    };
    match b.contents {
        Contents::Image if caption.is_empty() => {
            let Some((g, cx, cy)) = picture(media, border) else { return String::new() };
            if b.anchor == Anchor::Char { inline_xml(cx, cy, id, &name, &g) } else { anchor_xml(&p, cx, cy, id, &name, &g) }
        }
        _ => {
            // a text box: picture / text / equation, then the caption
            let mut content = String::new();
            if o.cap_pos == 1 {
                content.push_str(caption);
            }
            match b.contents {
                Contents::Image => {
                    if let Some((g, cx, cy)) = picture(media, false) {
                        content.push_str(&format!("<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>{}</w:p>", inline_xml(cx, cy, id + 100000, &name, &g)));
                    }
                }
                Contents::Equation => content.push_str(&format!(
                    "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\"/><w:i/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    esc(&b.equation)
                )),
                Contents::Text => content.push_str(inner),
                Contents::Empty => {}
            }
            if o.cap_pos != 1 {
                content.push_str(caption);
            }
            if content.is_empty() {
                content.push_str("<w:p/>");
            }
            let total_h = h + if caption.is_empty() { 0 } else { crate::gfx::caption_height(doc, b, o, "1", w, &m) };
            let g = shape_xml(emu(w), emu(total_h), Some(o.gray), border, ins, Some(&content));
            let name = format!("{} {id}", b.kind.name());
            if b.anchor == Anchor::Char { inline_xml(emu(w), emu(total_h), id, &name, &g) } else { anchor_xml(&p, emu(w), emu(total_h), id, &name, &g) }
        }
    }
}

/// A graphic line as a filled shape.
pub fn line_xml(doc: &Document, l: &GLine) -> String {
    let id = DOC_PR.fetch_add(1, Ordering::Relaxed);
    let i = &doc.init;
    let avail = i.page_width - i.left - i.right;
    let gray = Some(l.gray);
    if !l.vertical {
        let len = if l.halign == 3 || l.length <= 0 { avail } else { l.length };
        let g = shape_xml(emu(len), emu(l.width.max(1)), gray, false, [0; 4], None);
        if l.valign == 0 && l.halign == 3 {
            return inline_xml(emu(len), emu(l.width.max(1)), id, "Horizontal Line", &g);
        }
        let x = match l.halign {
            0 | 3 => i.left,
            1 => i.page_width - i.right - len,
            2 => i.left + (avail - len) / 2,
            _ => l.x,
        };
        let v = if l.valign == 0 {
            "<wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV>".to_string()
        } else {
            format!("<wp:positionV relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionV>", emu(l.y))
        };
        let p = Pos { h: format!("<wp:positionH relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionH>", emu(x)), v, wrap: "<wp:wrapNone/>", dist: [0; 4] };
        return anchor_xml(&p, emu(len), emu(l.width.max(1)), id, "Horizontal Line", &g);
    }
    let body = i.page_height - i.top - i.bottom;
    let len = if l.valign == 0 || l.length <= 0 { body } else { l.length };
    let x = match l.halign {
        0 => i.left - 100 - l.width,
        1 => i.page_width - i.right + 100,
        2 => i.left + avail / 2,
        _ => l.x,
    };
    let y = match l.valign {
        0 | 1 => i.top,
        2 => i.top + (body - len) / 2,
        3 => i.page_height - i.bottom - len,
        _ => l.y,
    };
    let g = shape_xml(emu(l.width.max(1)), emu(len), gray, false, [0; 4], None);
    let p = Pos {
        h: format!("<wp:positionH relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionH>", emu(x)),
        v: format!("<wp:positionV relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionV>", emu(y)),
        wrap: "<wp:wrapNone/>",
        dist: [0; 4],
    };
    anchor_xml(&p, emu(l.width.max(1)), emu(len), id, "Vertical Line", &g)
}

// ------------------------------------------------------------------ read

/// All `word/media/*` parts.
pub fn read_media<R: std::io::Read + std::io::Seek>(z: &mut zip::ZipArchive<R>) -> HashMap<String, Vec<u8>> {
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let Ok(mut f) = z.by_index(i) else { continue };
        let name = f.name().to_string();
        if name.starts_with("word/media/") {
            let mut v = vec![];
            if std::io::Read::read_to_end(&mut f, &mut v).is_ok() {
                out.insert(name, v);
            }
        }
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let p = format!("{name}=\"");
    let k = tag.find(&p)? + p.len();
    let e = tag[k..].find('"')? + k;
    Some(tag[k..e].replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"))
}

fn first_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let s = xml.find(&format!("<{tag}"))?;
    let e = xml[s..].find('>')? + s + 1;
    Some(&xml[s..e])
}

fn inner_of<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let s = xml.find(&format!("<{tag}"))?;
    let gt = xml[s..].find('>')? + s + 1;
    let e = xml[gt..].find(&format!("</{tag}>"))? + gt;
    Some(&xml[gt..e])
}

/// Find the end of the element starting at `s` (handles nesting of the same tag).
fn element_end(xml: &str, s: usize, tag: &str) -> Option<usize> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut depth = 0i32;
    let mut i = s;
    loop {
        let o = xml[i..].find(&open).map(|x| x + i);
        let c = xml[i..].find(&close).map(|x| x + i)?;
        match o {
            Some(o) if o < c => {
                // a self-closing start counts as nothing
                let gt = xml[o..].find('>')? + o;
                if !xml[..gt].ends_with('/') {
                    depth += 1;
                }
                i = gt + 1;
            }
            _ => {
                depth -= 1;
                i = c + close.len();
                if depth <= 0 {
                    return Some(i);
                }
            }
        }
    }
}

fn wpu(emu_s: &str) -> i32 {
    (emu_s.trim().parse::<i64>().unwrap_or(0) / EMU) as i32
}

struct ReadCtx<'a> {
    rels: &'a str,
    media: &'a HashMap<String, Vec<u8>>,
    styles: Option<&'a str>,
    margin_left: i32,
}

fn rel_target(rels: &str, rid: &str) -> Option<String> {
    let mut i = 0;
    while let Some(k) = rels[i..].find("<Relationship ") {
        let s = i + k;
        let e = rels[s..].find('>').map(|x| s + x + 1).unwrap_or(rels.len());
        let t = &rels[s..e];
        if attr(t, "Id").as_deref() == Some(rid) {
            return attr(t, "Target");
        }
        i = e;
    }
    None
}

/// One `<w:drawing>` → a box code.
fn drawing_to_code(x: &str, cx: &ReadCtx) -> Option<Code> {
    let inline = x.contains("<wp:inline");
    let ext = first_tag(x, "wp:extent")?;
    let (w, h) = (wpu(&attr(ext, "cx")?), wpu(&attr(ext, "cy")?));
    let mut b = if x.contains("<wps:txbx") || x.contains("<v:textbox") { GBox::new(BoxKind::Text) } else { GBox::new(BoxKind::Figure) };
    b.width = w.max(120);
    b.height = h.max(120);
    b.auto = 0;
    if inline {
        b.anchor = Anchor::Char;
        b.vpos = VPos::Baseline;
    } else {
        if let Some(ph) = inner_of(x, "wp:positionH") {
            let from = first_tag(x, "wp:positionH").and_then(|t| attr(t, "relativeFrom")).unwrap_or_default();
            b.hpos = match inner_of(ph, "wp:align") {
                Some("left") | Some("inside") => HPos::Left,
                Some("right") | Some("outside") => HPos::Right,
                Some("center") => HPos::Center,
                _ => {
                    let off = inner_of(ph, "wp:posOffset").map(wpu).unwrap_or(0);
                    HPos::Set(if from == "page" { off } else { off + cx.margin_left })
                }
            };
        }
        if let Some(pv) = inner_of(x, "wp:positionV") {
            let from = first_tag(x, "wp:positionV").and_then(|t| attr(t, "relativeFrom")).unwrap_or_default();
            let off = inner_of(pv, "wp:posOffset").map(wpu);
            match (from.as_str(), inner_of(pv, "wp:align"), off) {
                ("page" | "margin", Some(a), _) => {
                    b.anchor = Anchor::Page;
                    b.vpos = match a {
                        "center" => VPos::Center,
                        "bottom" => VPos::Bottom,
                        _ => VPos::Top,
                    };
                }
                ("page", None, Some(o)) => {
                    b.anchor = Anchor::Page;
                    b.vpos = VPos::Set(o);
                }
                (_, _, o) => {
                    b.anchor = Anchor::Para;
                    b.vpos = VPos::Set(o.unwrap_or(0).max(0));
                }
            }
        }
        b.wrap = !x.contains("<wp:wrapNone");
    }
    // a picture
    if let Some(blip) = first_tag(x, "a:blip") {
        if !x.contains("<wps:txbx") {
            let rid = attr(blip, "r:embed")?;
            let target = rel_target(cx.rels, &rid)?;
            let path = format!("word/{}", target.trim_start_matches('/').trim_start_matches("word/"));
            let data = cx.media.get(&path)?;
            let name = path.rsplit('/').next().unwrap_or("image").to_string();
            let name = first_tag(x, "pic:cNvPr").and_then(|t| attr(t, "name")).filter(|n| n.contains('.')).unwrap_or(name);
            b.set_image(&name, data.clone());
            b.auto = 0;
            b.width = w.max(120);
            b.height = h.max(120);
            return Some(Code::GBox(Box::new(b)));
        }
    }
    // a text box
    if let Some(inner) = inner_of(x, "w:txbxContent") {
        let conv = convert(inner, cx);
        let d = crate::docx::parse_document_xml(&format!("<w:document><w:body>{conv}</w:body></w:document>"), cx.styles);
        let mut els = d.els;
        while matches!(els.last(), Some(El::Code(Code::HRt))) {
            els.pop();
        }
        // our own "picture + caption" text box → a Figure with caption
        if let Some(El::Code(Code::GBox(fig))) = els.first().cloned() {
            if fig.kind == BoxKind::Figure && fig.anchor == Anchor::Char {
                let mut f = *fig;
                f.anchor = b.anchor;
                f.vpos = b.vpos;
                f.hpos = b.hpos;
                f.wrap = b.wrap;
                f.width = b.width;
                f.auto = 1;
                f.auto_height();
                let mut cap: Vec<El> = els[1..].to_vec();
                while matches!(cap.first(), Some(El::Code(Code::HRt | Code::Just(_)))) {
                    cap.remove(0);
                }
                f.caption = cap;
                return Some(Code::GBox(Box::new(f)));
            }
        }
        b.text = els;
        b.contents = Contents::Text;
        b.auto = 1;
        return Some(Code::GBox(Box::new(b)));
    }
    None
}

/// Replace drawings (and their VML fallbacks) by `<w:wp51box w:s="…"/>`
/// placeholders carrying the box code in stream form.
pub fn convert_drawings(xml: &str, rels: &str, media: &HashMap<String, Vec<u8>>, styles: Option<&str>) -> String {
    let margin_left = first_tag(xml, "w:pgMar").and_then(|t| attr(t, "w:left")).and_then(|v| v.parse::<i32>().ok()).map(|t| t * 1200 / 1440).unwrap_or(1200);
    let cx = ReadCtx { rels, media, styles, margin_left };
    convert(xml, &cx)
}

fn convert(xml: &str, cx: &ReadCtx) -> String {
    if !xml.contains("<w:drawing") && !xml.contains("<w:pict") {
        return xml.to_string();
    }
    let mut out = String::with_capacity(xml.len());
    let mut i = 0;
    loop {
        let cands = [("mc:AlternateContent", xml[i..].find("<mc:AlternateContent")), ("w:drawing", xml[i..].find("<w:drawing")), ("w:pict", xml[i..].find("<w:pict"))];
        let Some((tag, k)) = cands.iter().filter_map(|(t, k)| k.map(|k| (*t, k))).min_by_key(|x| x.1) else {
            out.push_str(&xml[i..]);
            break;
        };
        let s = i + k;
        out.push_str(&xml[i..s]);
        let Some(e) = element_end(xml, s, tag) else {
            out.push_str(&xml[s..]);
            break;
        };
        let el = &xml[s..e];
        let drawing = match tag {
            "mc:AlternateContent" => el.find("<w:drawing").and_then(|d| element_end(el, d, "w:drawing").map(|de| &el[d..de])),
            "w:drawing" => Some(el),
            _ => None,
        };
        if let Some(code) = drawing.and_then(|d| drawing_to_code(d, cx)) {
            out.push_str(&format!("<w:wp51box w:s=\"{}\"/>", esc(&crate::stream::code_to_str(&code))));
        }
        i = e;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let r = crate::raster::Raster::new(8, 4, 0xFF336699);
        crate::raster::to_png(&r).unwrap()
    }

    fn strip_stream(bytes: &[u8]) -> Vec<u8> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for i in 0..z.len() {
                let mut f = z.by_index(i).unwrap();
                let name = f.name().to_string();
                let mut data = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut data).unwrap();
                if name == "wp51/stream.txt" {
                    data = b"#HASH 0\n".to_vec();
                }
                w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                std::io::Write::write_all(&mut w, &data).unwrap();
            }
            w.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn docx_pictures_and_text_boxes() {
        let mut d = Document::from_text("Vorher\nNachher");
        let mut f = GBox::new(BoxKind::Figure);
        f.set_image("bild.png", png());
        let mut wpg = GBox::new(BoxKind::Figure);
        wpg.set_image("square.wpg", crate::wpg::tests_sample());
        wpg.anchor = Anchor::Char;
        let mut t = GBox::new(BoxKind::Text);
        t.text = "Kasten".chars().map(El::Ch).collect();
        d.els.insert(7, El::Code(Code::GBox(Box::new(f))));
        d.els.insert(8, El::Code(Code::GBox(Box::new(wpg))));
        d.els.push(El::Code(Code::GBox(Box::new(t))));
        d.els.push(El::Code(Code::GLine(GLine::new(false, 7800))));
        let bytes = crate::docx::write_docx(&d).unwrap();
        // the parts are there
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let media = read_media(&mut z);
        assert_eq!(media.len(), 2, "{:?}", media.keys());
        let mut docxml = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut docxml).unwrap();
        assert!(docxml.contains("<wp:anchor") && docxml.contains("<wp:inline") && docxml.contains("<wps:txbx>"), "{docxml}");
        // lossless through our private stream
        let d2 = crate::docx::read_docx(&bytes).unwrap();
        assert_eq!(d.els, d2.els);
        // and through Word's XML alone
        let d3 = crate::docx::read_docx(&strip_stream(&bytes)).unwrap();
        let boxes: Vec<&GBox> = d3.els.iter().filter_map(|e| if let El::Code(Code::GBox(b)) = e { Some(&**b) } else { None }).collect();
        assert_eq!(boxes.len(), 3, "{:?}", d3.els);
        assert_eq!(boxes[0].kind, BoxKind::Figure);
        assert_eq!(boxes[0].data.as_deref().map(|v| v.as_slice()), Some(png().as_slice()));
        assert_eq!(boxes[1].anchor, Anchor::Char);
        assert!(crate::raster::sniff(boxes[1].data.as_ref().unwrap()) == Some(crate::raster::Format::Png));
        assert_eq!(boxes[2].kind, BoxKind::Text);
        let td = Document { els: boxes[2].text.clone(), ..Default::default() };
        assert!(td.plain_text(0, td.els.len()).contains("Kasten"));
        assert!(d3.plain_text(0, d3.els.len()).contains("Nachher"));
        assert!(!d3.plain_text(0, d3.els.len()).contains("Kasten"), "text box text stays in the box");
    }
}
