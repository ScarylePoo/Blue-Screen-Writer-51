//! Shift-F7 Print: the WP 5.1 Print screen, View Document, printer
//! selection, and real printing through Windows (any installed printer,
//! incl. "Microsoft Print to PDF").

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::page::{Page, build_pages};
use crate::screens::*;
use crate::ui::{Outcome, Screen, Wp};

pub fn pages_of(wp: &mut Wp) -> Vec<Page> {
    let e = wp.ed();
    e.ensure_layout();
    let m = crate::fonts::TtfMetrics::new();
    match crate::page::print_document(&e.doc, &e.layout) {
        Some(pd) => {
            let l = crate::layout::layout(&pd, &m);
            crate::fmtpage::arrange_labels(build_pages(&pd, &l, &m), &l)
        }
        None => crate::fmtpage::arrange_labels(build_pages(&e.doc, &e.layout, &m), &e.layout),
    }
}

pub fn current_page(wp: &mut Wp) -> u32 {
    let e = wp.ed();
    e.ensure_layout();
    let li = e.layout.line_of(e.cursor);
    e.layout.lines[li].page
}

/// Parse "2,5-7,10-" style page lists (1-based) against `n` pages.
pub fn parse_pages(s: &str, n: u32) -> Vec<u32> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("all") || s == "(All)" {
        return (1..=n).collect();
    }
    let mut out = vec![];
    for part in s.split([',', ' ']).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let a: u32 = a.trim().parse().unwrap_or(1);
            let b: u32 = if b.trim().is_empty() { n } else { b.trim().parse().unwrap_or(n) };
            for p in a.max(1)..=b.min(n) {
                out.push(p);
            }
        } else if let Ok(p) = part.parse::<u32>() {
            if p >= 1 && p <= n {
                out.push(p);
            }
        }
    }
    out
}

pub fn print_menu(wp: &mut Wp) {
    if wp.ed_ref().block_anchor.is_some() {
        wp.push(YesNo::new("Print block?", false, |wp, yes| {
            if yes {
                if let Some(els) = wp.ed_ref().block_els() {
                    let mut d = wp.ed_ref().doc.clone();
                    d.els = els;
                    wp.ed().block_anchor = None;
                    let mut ed = crate::editor::Editor::with_doc(d, None);
                    ed.set_metrics(Box::new(crate::fonts::TtfMetrics::new()));
                    let m = crate::fonts::TtfMetrics::new();
                    let pages = build_pages(&ed.doc, &ed.layout, &m);
                    send_to_printer(wp, &pages, "Block");
                }
            }
            None
        }));
        return;
    }
    wp.push(Box::new(PrintScreen { copies: 1 }));
}

struct PrintScreen {
    copies: u32,
}

impl Screen for PrintScreen {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let c = match i {
            Input::Char(c) => c.to_ascii_lowercase(),
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Func(Func::Enter) => return Outcome::Close,
            _ => return Outcome::Keep,
        };
        match c {
            '1' | 'f' => {
                let pages = pages_of(wp);
                let name = wp.doc_label();
                for _ in 0..wp.print_copies.max(1) {
                    send_to_printer(wp, &pages, &name);
                }
                Outcome::Close
            }
            '2' | 'p' => {
                let n = current_page(wp);
                let pages: Vec<Page> = pages_of(wp).into_iter().filter(|p| p.number == n).collect();
                let name = wp.doc_label();
                for _ in 0..wp.print_copies.max(1) {
                    send_to_printer(wp, &pages, &name);
                }
                Outcome::Close
            }
            '3' | 'd' => Outcome::Push(Prompt::new("Document name:", "", |wp, s| {
                let s = s?;
                let p = wp.resolve_path(&s);
                print_file(wp, &p);
                None
            })),
            '5' | 'm' => {
                let copies = wp.print_copies.max(1);
                Outcome::Push(Prompt::new("Page(s):", "", move |wp, s| {
                    let s = s?;
                    let all = pages_of(wp);
                    let want = parse_pages(&s, all.len() as u32);
                    let pages: Vec<Page> = all.into_iter().filter(|p| want.contains(&p.number)).collect();
                    let name = wp.doc_label();
                    for _ in 0..copies {
                        send_to_printer(wp, &pages, &name);
                    }
                    None
                }))
            }
            '6' | 'v' => {
                let pg = current_page(wp);
                Outcome::Push(Box::new(ViewDoc::new(wp, pg)))
            }
            's' => Outcome::Push(Box::new(SelectPrinter::new(wp))),
            'n' => {
                let cur = wp.print_copies;
                let _ = cur;
                Outcome::Push(Prompt::new("Number of copies:", &cur.to_string(), |wp, s| {
                    if let Some(n) = s.and_then(|s| s.trim().parse::<u32>().ok()) {
                        wp.flash(&format!("Copies: {n}"));
                        wp.print_copies = n.clamp(1, 999);
                    }
                    None
                }))
            }
            '4' | 'c' => {
                wp.flash("Print jobs are handled by Windows — see the printer's queue.");
                Outcome::Close
            }
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        g.clear(st);
        g.text(0, 0, "Print", st);
        let items = [
            "1 - ~Full Document",
            "2 - ~Page",
            "3 - ~Document on Disk",
            "4 - ~Control Printer",
            "5 - ~Multiple Pages",
            "6 - ~View Document",
            "7 - ~Initialize Printer",
        ];
        for (k, it) in items.iter().enumerate() {
            let end = g.text_mn(5, 2 + k, it, st, hi);
            wp.hit(2 + k, 5, end, crate::ui::HitAction::Input(Input::Char((b'1' + k as u8) as char)));
        }
        g.text(0, 11, "Options", st);
        let printer = wp.printer_name();
        let opts = [
            ("S - ~Select Printer", printer),
            ("B - ~Binding Offset", "0\"".to_string()),
            ("N - ~Number of Copies", wp.print_copies.to_string()),
            ("U - M~ultiple Copies Generated by", "Blue Screen Writer 51".into()),
            ("G - ~Graphics Quality", "Medium".into()),
            ("T - ~Text Quality", "High".into()),
        ];
        for (k, (l, v)) in opts.iter().enumerate() {
            let end = g.text_mn(5, 13 + k, l, st, hi);
            let key = l.chars().next().unwrap_or(' ').to_ascii_lowercase();
            wp.hit(13 + k, 5, end, crate::ui::HitAction::Input(Input::Char(key)));
            g.text(44, 13 + k, v, st);
        }
        let c = g.text(0, g.rows - 1, "Selection: 0", st);
        g.cursor = Some((c - 1, g.rows - 1));
    }
}

pub fn print_file(wp: &mut Wp, p: &std::path::Path) {
    match crate::fileio::load(p) {
        Ok(d) => {
            let mut ed = crate::editor::Editor::with_doc(d, None);
            ed.set_metrics(Box::new(crate::fonts::TtfMetrics::new()));
            let m = crate::fonts::TtfMetrics::new();
            let pages = build_pages(&ed.doc, &ed.layout, &m);
            send_to_printer(wp, &pages, &p.display().to_string());
        }
        Err(e) => wp.flash(&format!("ERROR: {e}")),
    }
}

// ------------------------------------------------------------ View Document

pub struct ViewDoc {
    pages: Vec<Page>,
    page: usize,
    /// 1 = 100%, 2 = 200%, 3 = full page, 4 = facing pages
    mode: u8,
    scroll_y: i32,
    scroll_x: i32,
}

impl ViewDoc {
    pub fn new(wp: &mut Wp, page: u32) -> ViewDoc {
        let pages = pages_of(wp);
        let idx = (page as usize).saturating_sub(1).min(pages.len().saturating_sub(1));
        ViewDoc { pages, page: idx, mode: 3, scroll_y: 0, scroll_x: 0 }
    }
}

impl Screen for ViewDoc {
    fn full_screen(&self) -> bool {
        true
    }
    fn input(&mut self, _wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Char('1') => self.mode = 1,
            Input::Char('2') => self.mode = 2,
            Input::Char('3') => self.mode = 3,
            Input::Char('4') => self.mode = 4,
            Input::Func(Func::PgDn) => {
                self.page = (self.page + if self.mode == 4 { 2 } else { 1 }).min(self.pages.len() - 1);
                self.scroll_y = 0;
            }
            Input::Func(Func::PgUp) => {
                self.page = self.page.saturating_sub(if self.mode == 4 { 2 } else { 1 });
                self.scroll_y = 0;
            }
            Input::Func(Func::Down) => self.scroll_y += 60,
            Input::Func(Func::Up) => self.scroll_y = (self.scroll_y - 60).max(0),
            Input::Func(Func::Right) => self.scroll_x += 60,
            Input::Func(Func::Left) => self.scroll_x = (self.scroll_x - 60).max(0),
            Input::Func(Func::ScreenDown) => self.scroll_y += 400,
            Input::Func(Func::ScreenUp) => self.scroll_y = (self.scroll_y - 400).max(0),
            Input::Func(Func::Home) => {
                self.scroll_x = 0;
                self.scroll_y = 0;
            }
            Input::Func(Func::GoTo) => {}
            Input::Func(Func::Exit) => return Outcome::CloseAll,
            Input::Func(Func::Cancel) | Input::Func(Func::Enter) | Input::Char(' ') | Input::Char('0') => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        let row = g.rows - 1;
        let mode_s = ["", "1", "2", "3", "4"][self.mode as usize];
        let c = g.text(0, row, "1 100%  2 200%  3 Full Page  4 Facing Pages: ", st);
        g.text(c, row, mode_s, st);
        let right = format!("Doc {} Pg {}", wp.cur + 1, self.pages.get(self.page).map(|p| p.number).unwrap_or(1));
        g.text(g.cols - right.len() - 1, row, &right, st);
    }
    fn draw_pixels(&self, _wp: &Wp, buf: &mut [u32], w: usize, h: usize, cell_h: usize) -> bool {
        let avail_h = h.saturating_sub(cell_h + 8) as f64;
        let bg = 0x5A5A5A;
        for p in buf[..w * (avail_h as usize).min(h)].iter_mut() {
            *p = bg;
        }
        let Some(page) = self.pages.get(self.page) else { return true };
        let dpi = 96.0;
        let (scale, two) = match self.mode {
            1 => (dpi / 1200.0, false),
            2 => (2.0 * dpi / 1200.0, false),
            4 => ((avail_h - 20.0) / page.height as f64, true),
            _ => ((avail_h - 20.0) / page.height as f64, false),
        };
        let pw = (page.width as f64 * scale) as i32;
        let ph = (page.height as f64 * scale) as i32;
        let max_scroll = (ph + 20 - avail_h as i32).max(0);
        let top = 10 - self.scroll_y.min(max_scroll);
        let mut clipped = vec![bg; w * avail_h as usize];
        let ch = avail_h as usize;
        if two {
            // facing pages: even on the left, odd on the right
            let (l, r) = if page.number % 2 == 0 {
                (Some(page), self.pages.get(self.page + 1))
            } else {
                (if self.page > 0 { self.pages.get(self.page - 1) } else { None }, Some(page))
            };
            let total = pw * 2 + 20;
            let x0 = (w as i32 - total) / 2;
            if let Some(lp) = l {
                crate::pagerender::render_page(lp, &mut clipped, w, ch, x0, top, scale);
            }
            if let Some(rp) = r {
                crate::pagerender::render_page(rp, &mut clipped, w, ch, x0 + pw + 20, top, scale);
            }
        } else {
            let x0 = ((w as i32 - pw) / 2).max(10) - self.scroll_x;
            crate::pagerender::render_page(page, &mut clipped, w, ch, x0, top, scale);
        }
        buf[..clipped.len()].copy_from_slice(&clipped);
        true
    }
}

// ----------------------------------------------------------- printers

pub struct SelectPrinter {
    names: Vec<String>,
    sel: usize,
}

impl SelectPrinter {
    pub fn new(wp: &Wp) -> SelectPrinter {
        let names = winprint::list_printers();
        let cur = wp.printer_name();
        let sel = names.iter().position(|n| *n == cur).unwrap_or(0);
        SelectPrinter { names, sel }
    }
}

impl Screen for SelectPrinter {
    fn full_screen(&self) -> bool {
        true
    }
    fn mouse_select(&mut self, wp: &mut Wp, n: usize, double: bool) -> Outcome {
        self.sel = n.min(self.names.len().saturating_sub(1));
        if double { self.input(wp, &Input::Char('1')) } else { Outcome::Keep }
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        let n = self.names.len().max(1);
        match i {
            Input::Func(Func::Down) => self.sel = (self.sel + 1).min(n - 1),
            Input::Func(Func::Up) => self.sel = self.sel.saturating_sub(1),
            Input::Char('1') | Input::Char('s') | Input::Char('S') | Input::Func(Func::Enter) => {
                if let Some(name) = self.names.get(self.sel) {
                    wp.settings.printer = name.clone();
                    let _ = wp.settings.save();
                }
                return Outcome::Close;
            }
            Input::Func(Func::Exit) | Input::Func(Func::Cancel) | Input::Char('0') => return Outcome::Close,
            _ => {}
        }
        Outcome::Keep
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        g.clear(st);
        g.text(0, 0, "Print: Select Printer", st);
        let def = winprint::default_printer().unwrap_or_default();
        for (k, n) in self.names.iter().enumerate() {
            let mark = if *n == wp.printer_name() { '*' } else { ' ' };
            let s = format!("{mark} {n}{}", if *n == def { "   (Windows default)" } else { "" });
            g.text(1, 2 + k, &s, if k == self.sel { wp.theme.block } else { st });
            wp.hit(2 + k, 0, g.cols, crate::ui::HitAction::Select(k));
        }
        if self.names.is_empty() {
            g.text(2, 2, "No printers are installed in Windows.", st);
        }
        let c = g.text_mn(0, g.rows - 1, "1 ~Select: ", st, wp.theme.bold);
        g.text(c, g.rows - 1, "1", st);
    }
}

/// Save the document as PDF through Windows' built-in PDF printer.
pub fn save_pdf(wp: &mut Wp, path: &std::path::Path) {
    let pages = pages_of(wp);
    let printers = winprint::list_printers();
    let pdf = printers
        .iter()
        .find(|p| p.eq_ignore_ascii_case("Microsoft Print to PDF"))
        .or_else(|| printers.iter().find(|p| p.to_lowercase().contains("microsoft") && p.to_lowercase().contains("pdf")));
    let Some(pdf) = pdf else {
        wp.flash("ERROR: No PDF printer installed (Microsoft Print to PDF)");
        return;
    };
    let _ = std::fs::remove_file(path);
    match winprint::print_pages_to(pdf, &wp.doc_label(), &pages, Some(&path.display().to_string())) {
        Ok(()) => wp.flash(&format!("* Saved {} *", path.display())),
        Err(e) => wp.flash(&format!("ERROR: {e}")),
    }
}

pub fn send_to_printer(wp: &mut Wp, pages: &[Page], title: &str) {
    if pages.is_empty() {
        return;
    }
    let printer = wp.printer_name();
    match winprint::print_pages(&printer, title, pages) {
        Ok(()) => wp.flash(&format!("* Sent {} page(s) to {printer} *", pages.len())),
        Err(e) => wp.flash(&format!("ERROR: Printing failed — {e}")),
    }
}

// ------------------------------------------------------- Windows GDI

#[cfg(windows)]
pub mod winprint {
    use crate::doc::Attr;
    use crate::page::Page;

    #[repr(C)]
    struct DocInfoW {
        cb_size: i32,
        doc_name: *const u16,
        output: *const u16,
        datatype: *const u16,
        fw_type: u32,
    }
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        xppm: i32,
        yppm: i32,
        clr_used: u32,
        clr_important: u32,
    }
    #[repr(C)]
    struct PrinterInfo4W {
        printer_name: *const u16,
        server_name: *const u16,
        attributes: u32,
    }
    #[link(name = "winspool")]
    unsafe extern "system" {
        fn OpenPrinterW(name: *const u16, h: *mut isize, def: *const core::ffi::c_void) -> i32;
        fn ClosePrinter(h: isize) -> i32;
        fn DocumentPropertiesW(hwnd: isize, h: isize, name: *const u16, out: *mut u8, inp: *const u8, mode: u32) -> i32;
        fn GetDefaultPrinterW(buf: *mut u16, size: *mut u32) -> i32;
        fn EnumPrintersW(flags: u32, name: *const u16, level: u32, buf: *mut u8, cb: u32, needed: *mut u32, returned: *mut u32) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateDCW(driver: *const u16, device: *const u16, port: *const u16, init: *const core::ffi::c_void) -> isize;
        fn DeleteDC(hdc: isize) -> i32;
        fn ResetDCW(hdc: isize, dm: *const u8) -> isize;
        fn StartDocW(hdc: isize, di: *const DocInfoW) -> i32;
        fn EndDoc(hdc: isize) -> i32;
        fn StartPage(hdc: isize) -> i32;
        fn EndPage(hdc: isize) -> i32;
        fn GetDeviceCaps(hdc: isize, index: i32) -> i32;
        fn CreateFontW(
            h: i32,
            w: i32,
            esc: i32,
            orient: i32,
            weight: i32,
            italic: u32,
            underline: u32,
            strike: u32,
            charset: u32,
            out_prec: u32,
            clip_prec: u32,
            quality: u32,
            pitch: u32,
            face: *const u16,
        ) -> isize;
        fn SelectObject(hdc: isize, obj: isize) -> isize;
        fn DeleteObject(obj: isize) -> i32;
        fn SetBkMode(hdc: isize, mode: i32) -> i32;
        fn SetTextAlign(hdc: isize, align: u32) -> u32;
        fn SetTextColor(hdc: isize, color: u32) -> u32;
        fn PatBlt(hdc: isize, x: i32, y: i32, w: i32, h: i32, rop: u32) -> i32;
        fn ExtTextOutW(hdc: isize, x: i32, y: i32, opts: u32, rect: *const core::ffi::c_void, s: *const u16, n: u32, dx: *const i32) -> i32;
        fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
        fn StretchDIBits(
            hdc: isize,
            xd: i32,
            yd: i32,
            wd: i32,
            hd: i32,
            xs: i32,
            ys: i32,
            ws: i32,
            hs: i32,
            bits: *const core::ffi::c_void,
            bmi: *const BitmapInfoHeader,
            usage: u32,
            rop: u32,
        ) -> i32;
    }

    /// Pictures of graphics boxes: composed at up to ~300 dpi, flattened
    /// on white and stretched to the device with StretchDIBits.
    unsafe fn draw_images(hdc: isize, page: &Page, dpix: f64, dpiy: f64, offx: f64, offy: f64) {
        for im in &page.images {
            let (x, y, w, h) = im.clip;
            let dx = (x as f64 * dpix / 1200.0 - offx).round() as i32;
            let dy = (y as f64 * dpiy / 1200.0 - offy).round() as i32;
            let dw = ((w as f64 * dpix / 1200.0).round() as i32).max(1);
            let dh = ((h as f64 * dpiy / 1200.0).round() as i32).max(1);
            let k = (300.0 / dpix.max(1.0)).min(1.0);
            let (pw, ph) = (((dw as f64 * k) as usize).clamp(1, 4000), ((dh as f64 * k) as usize).clamp(1, 4000));
            let px: Vec<u32> = im.compose(pw, ph).into_iter().map(|p| crate::raster::over(0xFFFFFF, p)).collect();
            let bih = BitmapInfoHeader {
                size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                width: pw as i32,
                height: -(ph as i32),
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                xppm: 0,
                yppm: 0,
                clr_used: 0,
                clr_important: 0,
            };
            unsafe {
                SetStretchBltMode(hdc, 4); // HALFTONE
                StretchDIBits(hdc, dx, dy, dw, dh, 0, 0, pw as i32, ph as i32, px.as_ptr() as *const _, &bih, 0, 0x00CC_0020);
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn from_wide(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        unsafe {
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        }
    }

    pub fn default_printer() -> Option<String> {
        let mut buf = vec![0u16; 512];
        let mut n = buf.len() as u32;
        if unsafe { GetDefaultPrinterW(buf.as_mut_ptr(), &mut n) } != 0 {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
            return Some(String::from_utf16_lossy(&buf[..len]));
        }
        None
    }

    pub fn list_printers() -> Vec<String> {
        const LOCAL: u32 = 2;
        const CONNECTIONS: u32 = 4;
        let mut needed = 0u32;
        let mut ret = 0u32;
        unsafe {
            EnumPrintersW(LOCAL | CONNECTIONS, std::ptr::null(), 4, std::ptr::null_mut(), 0, &mut needed, &mut ret);
        }
        if needed == 0 {
            return vec![];
        }
        let mut buf = vec![0u8; needed as usize + 16];
        // align to 8 for the struct array
        let ok = unsafe { EnumPrintersW(LOCAL | CONNECTIONS, std::ptr::null(), 4, buf.as_mut_ptr(), needed, &mut needed, &mut ret) };
        if ok == 0 {
            return vec![];
        }
        let mut out = vec![];
        for i in 0..ret as usize {
            unsafe {
                let p = (buf.as_ptr() as *const PrinterInfo4W).add(i);
                let info = std::ptr::read_unaligned(p);
                out.push(from_wide(info.printer_name));
            }
        }
        out.sort();
        out
    }

    /// The printer's DEVMODE set up for a page of `w` x `h` WPU: orientation
    /// (landscape when wider than high) and the paper size.
    fn devmode_for(printer: &str, w: i32, h: i32) -> Option<Vec<u8>> {
        let name = wide(printer);
        let mut hp: isize = 0;
        unsafe {
            if OpenPrinterW(name.as_ptr(), &mut hp, std::ptr::null()) == 0 {
                return None;
            }
            let size = DocumentPropertiesW(0, hp, name.as_ptr(), std::ptr::null_mut(), std::ptr::null(), 0);
            if size < 84 {
                ClosePrinter(hp);
                return None;
            }
            let mut dm = vec![0u8; size as usize];
            if DocumentPropertiesW(0, hp, name.as_ptr(), dm.as_mut_ptr(), std::ptr::null(), 2) < 0 {
                ClosePrinter(hp);
                return None;
            }
            let mut fields = u32::from_le_bytes([dm[72], dm[73], dm[74], dm[75]]);
            let land = w > h;
            dm[76..78].copy_from_slice(&(if land { 2i16 } else { 1i16 }).to_le_bytes());
            fields |= 1; // DM_ORIENTATION
            // paper (portrait dimensions, tenths of a millimetre)
            let (s, l) = ((w.min(h) as i64 * 254 / 1200) as i32, (w.max(h) as i64 * 254 / 1200) as i32);
            let std_sizes: [(i32, i32, i16); 7] = [(2159, 2794, 1), (2159, 3556, 5), (2100, 2970, 9), (1480, 2100, 11), (1048, 2413, 20), (1100, 2200, 27), (1620, 2290, 28)];
            match std_sizes.iter().find(|(a, b, _)| (a - s).abs() <= 20 && (b - l).abs() <= 20) {
                Some(&(_, _, code)) => {
                    dm[78..80].copy_from_slice(&code.to_le_bytes());
                    fields |= 2; // DM_PAPERSIZE
                }
                None => {
                    dm[78..80].copy_from_slice(&0i16.to_le_bytes());
                    dm[80..82].copy_from_slice(&(l as i16).to_le_bytes());
                    dm[82..84].copy_from_slice(&(s as i16).to_le_bytes());
                    fields |= 4 | 8; // DM_PAPERLENGTH | DM_PAPERWIDTH
                }
            }
            dm[72..76].copy_from_slice(&fields.to_le_bytes());
            let mut out = vec![0u8; size as usize];
            let ok = DocumentPropertiesW(0, hp, name.as_ptr(), out.as_mut_ptr(), dm.as_ptr(), 2 | 8) >= 0;
            ClosePrinter(hp);
            Some(if ok { out } else { dm })
        }
    }

    pub fn print_pages(printer: &str, title: &str, pages: &[Page]) -> Result<(), String> {
        print_pages_to(printer, title, pages, None)
    }

    /// `output`: print to this file instead (e.g. a PDF with "Microsoft
    /// Print to PDF" — no dialog appears).
    pub fn print_pages_to(printer: &str, title: &str, pages: &[Page], output: Option<&str>) -> Result<(), String> {
        let out_w = output.map(wide);
        let driver = wide("WINSPOOL");
        let dev = wide(printer);
        let size_of = |p: &Page| (p.width, p.height);
        let mut cur_size = pages.first().map(size_of).unwrap_or((10200, 13200));
        let mut dm = devmode_for(printer, cur_size.0, cur_size.1);
        let init_ptr = dm.as_ref().map(|d| d.as_ptr() as *const core::ffi::c_void).unwrap_or(std::ptr::null());
        let hdc = unsafe { CreateDCW(driver.as_ptr(), dev.as_ptr(), std::ptr::null(), init_ptr) };
        if hdc == 0 {
            return Err(format!("cannot open printer \"{printer}\""));
        }
        let name = wide(title);
        let di = DocInfoW {
            cb_size: std::mem::size_of::<DocInfoW>() as i32,
            doc_name: name.as_ptr(),
            output: out_w.as_ref().map(|w| w.as_ptr()).unwrap_or(std::ptr::null()),
            datatype: std::ptr::null(),
            fw_type: 0,
        };
        unsafe {
            if StartDocW(hdc, &di) <= 0 {
                DeleteDC(hdc);
                return Err("the print job was cancelled".into());
            }
            let mut dpix = GetDeviceCaps(hdc, 88) as f64; // LOGPIXELSX
            let mut dpiy = GetDeviceCaps(hdc, 90) as f64; // LOGPIXELSY
            let mut offx = GetDeviceCaps(hdc, 112) as f64; // PHYSICALOFFSETX
            let mut offy = GetDeviceCaps(hdc, 113) as f64;
            for page in pages {
                // [Paper Sz/Typ] changes: switch orientation/paper between pages
                if size_of(page) != cur_size {
                    cur_size = size_of(page);
                    dm = devmode_for(printer, cur_size.0, cur_size.1);
                    if let Some(d) = &dm {
                        ResetDCW(hdc, d.as_ptr());
                        dpix = GetDeviceCaps(hdc, 88) as f64;
                        dpiy = GetDeviceCaps(hdc, 90) as f64;
                        offx = GetDeviceCaps(hdc, 112) as f64;
                        offy = GetDeviceCaps(hdc, 113) as f64;
                    }
                }
                StartPage(hdc);
                SetBkMode(hdc, 1);
                SetTextAlign(hdc, 24); // TA_BASELINE
                draw_images(hdc, page, dpix, dpiy, offx, offy);
                for &(x, y, w, h) in &page.rules {
                    let dx = (x as f64 * dpix / 1200.0 - offx) as i32;
                    let dy = (y as f64 * dpiy / 1200.0 - offy) as i32;
                    let dw = ((w as f64 * dpix / 1200.0) as i32).max(1);
                    let dh = ((h as f64 * dpiy / 1200.0) as i32).max(1);
                    PatBlt(hdc, dx, dy, dw, dh, 0x0000_0042); // BLACKNESS
                }
                for run in &page.runs {
                    let h = (run.size_cpt as f64 / 100.0 / 72.0 * dpiy).round() as i32;
                    let weight = if run.attrs.has(Attr::Bold) { 700 } else { 400 };
                    let face = wide(&crate::fonts::resolve_family(&run.font));
                    let font = CreateFontW(
                        -h,
                        0,
                        0,
                        0,
                        weight,
                        run.attrs.has(Attr::Italic) as u32,
                        (run.attrs.has(Attr::Underline) || run.attrs.has(Attr::DblUnderline)) as u32,
                        run.attrs.has(Attr::Strikeout) as u32,
                        1, // DEFAULT_CHARSET
                        4, // OUT_TT_PRECIS
                        0,
                        5, // CLEARTYPE_QUALITY
                        0,
                        face.as_ptr(),
                    );
                    let old = SelectObject(hdc, font);
                    SetTextColor(hdc, if run.attrs.has(Attr::Redline) { 0x0000C0 } else { 0 });
                    let text: Vec<u16> = run.text.iter().collect::<String>().encode_utf16().collect();
                    // per-UTF16-unit advances in device units
                    let mut dx: Vec<i32> = Vec::with_capacity(text.len());
                    let mut acc = run.x as f64 * dpix / 1200.0;
                    let mut prev = acc.round() as i32;
                    for (ci, ch) in run.text.iter().enumerate() {
                        acc += run.adv[ci] as f64 * dpix / 1200.0;
                        let nx = acc.round() as i32;
                        dx.push(nx - prev);
                        if ch.len_utf16() == 2 {
                            dx.push(0);
                        }
                        prev = nx;
                    }
                    let x = (run.x as f64 * dpix / 1200.0 - offx).round() as i32;
                    let y = (run.y as f64 * dpiy / 1200.0 - offy).round() as i32;
                    ExtTextOutW(hdc, x, y, 0, std::ptr::null(), text.as_ptr(), text.len() as u32, dx.as_ptr());
                    SelectObject(hdc, old);
                    DeleteObject(font);
                }
                EndPage(hdc);
            }
            EndDoc(hdc);
            DeleteDC(hdc);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub mod winprint {
    use crate::page::Page;
    pub fn default_printer() -> Option<String> {
        None
    }
    pub fn list_printers() -> Vec<String> {
        vec![]
    }
    pub fn print_pages(_printer: &str, _title: &str, _pages: &[Page]) -> Result<(), String> {
        Err("printing is only implemented on Windows so far".into())
    }
    pub fn print_pages_to(_printer: &str, _title: &str, _pages: &[Page], _out: Option<&str>) -> Result<(), String> {
        Err("printing is only implemented on Windows so far".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_lists() {
        assert_eq!(parse_pages("", 3), vec![1, 2, 3]);
        assert_eq!(parse_pages("2,4-5", 6), vec![2, 4, 5]);
        assert_eq!(parse_pages("3-", 5), vec![3, 4, 5]);
    }
}
