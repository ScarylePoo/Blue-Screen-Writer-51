//! The native window: winit + softbuffer.  Turns key presses into WP
//! input and draws the text grid.

use crate::grid::Grid;
use crate::keys::{K, KeyIn, Mods, default_map};
use crate::render::Renderer;
use crate::ui::Wp;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowId};

pub const BLINK: Duration = Duration::from_millis(530);

struct App {
    wp: Wp,
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    buf: Vec<u32>,
    renderer: Renderer,
    grid: Grid,
    mods: ModifiersState,
    blink_on: bool,
    next_blink: Instant,
    last_alt_f4: Option<Instant>,
    ignore_next_f4: bool,
    mouse_px: (f64, f64),
    mouse_down: bool,
    last_click: Option<(Instant, usize, usize)>,
    last_cell: (usize, usize),
}

/// Translate a winit key event into our key model.
pub fn convert(ev: &KeyEvent, mods: ModifiersState) -> Option<KeyIn> {
    let m = Mods { shift: mods.shift_key(), ctrl: mods.control_key(), alt: mods.alt_key() };
    // Grey keypad + / -  → Screen Down / Up (as in WP 5.1)
    if let PhysicalKey::Code(c) = ev.physical_key {
        match c {
            KeyCode::NumpadAdd => return Some(KeyIn { k: K::PadPlus, m }),
            KeyCode::NumpadSubtract => return Some(KeyIn { k: K::PadMinus, m }),
            _ => {}
        }
    }
    let k = match &ev.logical_key {
        Key::Named(n) => match n {
            NamedKey::F1 => K::F(1),
            NamedKey::F2 => K::F(2),
            NamedKey::F3 => K::F(3),
            NamedKey::F4 => K::F(4),
            NamedKey::F5 => K::F(5),
            NamedKey::F6 => K::F(6),
            NamedKey::F7 => K::F(7),
            NamedKey::F8 => K::F(8),
            NamedKey::F9 => K::F(9),
            NamedKey::F10 => K::F(10),
            NamedKey::F11 => K::F(11),
            NamedKey::F12 => K::F(12),
            NamedKey::ArrowLeft => K::Left,
            NamedKey::ArrowRight => K::Right,
            NamedKey::ArrowUp => K::Up,
            NamedKey::ArrowDown => K::Down,
            NamedKey::Home => K::Home,
            NamedKey::End => K::End,
            NamedKey::PageUp => K::PgUp,
            NamedKey::PageDown => K::PgDn,
            NamedKey::Insert => K::Ins,
            NamedKey::Delete => K::Del,
            NamedKey::Backspace => K::Backspace,
            NamedKey::Enter => K::Enter,
            NamedKey::Tab => K::Tab,
            NamedKey::Escape => K::Esc,
            NamedKey::Space => {
                if m.ctrl || m.alt {
                    K::Char(' ')
                } else {
                    return Some(KeyIn { k: K::Char(' '), m: Mods::default() });
                }
            }
            _ => return None,
        },
        _ => {
            let altgr = m.ctrl && m.alt;
            if (m.ctrl || m.alt) && !altgr {
                // Ctrl/Alt + key: use the unmodified character
                match ev.key_without_modifiers() {
                    Key::Character(s) => K::Char(s.chars().next()?),
                    _ => return None,
                }
            } else {
                // Plain typing (incl. AltGr and dead-key composition)
                let t = ev.text.as_ref()?;
                let c = t.chars().next()?;
                if c.is_control() {
                    return None;
                }
                return Some(KeyIn { k: K::Char(c), m: Mods::default() });
            }
        }
    };
    Some(KeyIn { k, m })
}

impl App {
    fn redraw(&mut self) {
        let Some(size) = self.window.as_ref().map(|w| w.inner_size()) else { return };
        let (wd, ht) = (size.width as usize, size.height as usize);
        if wd == 0 || ht == 0 {
            return;
        }
        // grid size: classic 80×25(+) or "fit" (grows with the window)
        let (cols, rows) = self.grid_size(wd, ht);
        let (Some(w), Some(surface)) = (self.window.as_ref(), self.surface.as_mut()) else { return };
        if cols != self.wp.cols || rows != self.wp.rows {
            self.wp.resize(cols, rows);
        }
        if self.grid.rows != self.wp.rows || self.grid.cols != self.wp.cols {
            self.grid = Grid::new(self.wp.cols, self.wp.rows, self.wp.theme.normal);
        }
        self.renderer.typographic = self.wp.settings.typographic;
        self.renderer.fit(wd, ht, self.grid.cols, self.grid.rows);
        self.wp.draw(&mut self.grid);
        if self.buf.len() != wd * ht {
            self.buf = vec![0; wd * ht];
        }
        let bg = self.wp.theme.screen_bg;
        self.renderer.render(&self.grid, &mut self.buf, wd, ht, self.blink_on, bg);
        let ch = self.renderer.cell_h;
        self.wp.draw_pixels(&mut self.buf, wd, ht, ch);
        surface.present(&self.buf, wd, ht);
        let title = crate::window_title(&self.wp);
        w.set_title(&title);
    }

    /// How many columns/rows the window shows.
    fn grid_size(&self, w: usize, h: usize) -> (usize, usize) {
        let s = &self.wp.settings;
        if s.screen_mode == "classic" {
            return (80, s.rows.max(25));
        }
        let aspect = self.renderer.cell_aspect().max(0.3);
        let cell_h = if s.zoom_px > 0 {
            s.zoom_px as f32
        } else if s.screen_mode == "page" {
            // as large as possible while a whole page (plus status lines) fits,
            // but never smaller than readable and at least 80 columns wide
            let need_rows = (self.wp.page_lines() + 3).max(25) as f32;
            let by_height = h as f32 / need_rows;
            let by_width = w as f32 / (80.0 * aspect);
            by_height.max(15.0).min(by_width)
        } else {
            // large letters like the classic screen: 80 columns fill the
            // window width (and at least 25 lines fit)
            let by_width = w as f32 / (80.0 * aspect);
            let by_height = h as f32 / 25.0;
            by_width.min(by_height)
        };
        let cell_h = cell_h.max(8.0);
        let cell_w = (cell_h * aspect).max(4.0);
        let cols = ((w as f32 / cell_w) as usize).max(80).min(400);
        let rows = ((h as f32 / cell_h) as usize).max(25).min(200);
        (cols, rows)
    }

    /// Zoom in/out (Ctrl+wheel, Ctrl+=, Ctrl+0 = automatic).
    fn zoom(&mut self, delta: i32) {
        let cur = if self.wp.settings.zoom_px > 0 { self.wp.settings.zoom_px as i32 } else { self.renderer.cell_h as i32 };
        let nz = if delta == 0 { 0 } else { (cur + delta * 2).clamp(10, 80) as u32 };
        let mut s = self.wp.settings.clone();
        s.zoom_px = nz;
        if s.screen_mode != "fit" {
            s.screen_mode = "fit".into();
        }
        let _ = s.save();
        self.wp.settings = s;
        self.wp.flash(if nz == 0 {
            "Zoom: large letters (automatic)"
        } else {
            "Zoom: Ctrl+=  Ctrl+Shift+-  Ctrl+0"
        });
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Pixel position → text cell.
    fn cell_at(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let r = &self.renderer;
        let cx = (x - r.off_x as f64) / r.cell_w as f64;
        let cy = (y - r.off_y as f64) / r.cell_h as f64;
        if cx < 0.0 || cy < 0.0 {
            return None;
        }
        let (c, row) = (cx as usize, cy as usize);
        if c < self.grid.cols && row < self.grid.rows { Some((c, row)) } else { None }
    }

    fn mouse(&mut self, kind: crate::ui::MouseKind, col: usize, row: usize) {
        self.blink_on = true;
        self.next_blink = Instant::now() + BLINK;
        self.wp.mouse(kind, col, row);
        crate::pump(&mut self.wp);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn handle_key(&mut self, ki: KeyIn) {
        self.blink_on = true;
        self.next_blink = Instant::now() + BLINK;
        crate::dispatch_key(&mut self.wp, ki);
        crate::pump(&mut self.wp);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Blue Screen Writer 51")
            .with_inner_size(LogicalSize::new(1040.0, 650.0))
            .with_min_inner_size(LogicalSize::new(400.0, 250.0))
            .with_window_icon(winit::window::Icon::from_rgba(crate::register::icon_rgba(64), 64, 64).ok());
        let w = Rc::new(el.create_window(attrs).expect("create window"));
        let surface = crate::present::Presenter::new(w.clone());
        self.window = Some(w);
        self.surface = Some(surface);
        el.set_control_flow(ControlFlow::WaitUntil(self.next_blink));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                // Alt-F4 is Block in WordPerfect; Windows turns it into a
                // close request (sometimes before the key event arrives).
                if self.last_alt_f4.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) {
                    return;
                }
                if self.mods.alt_key() {
                    self.last_alt_f4 = Some(Instant::now());
                    self.ignore_next_f4 = true;
                    self.handle_key(KeyIn { k: K::F(4), m: Mods { shift: false, ctrl: false, alt: true } });
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }
                if !self.wp.docs.iter().any(|d| d.modified) {
                    el.exit();
                    return;
                }
                if !self.wp.has_screens() {
                    crate::ui_close_request(&mut self.wp);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } => {
                if event.state == ElementState::Pressed {
                    if let Some(ki) = convert(&event, self.mods) {
                        // zoom keys (not used by the classic keyboard)
                        if ki.m.ctrl && !ki.m.alt {
                            match ki.k {
                                K::Char('=') | K::Char('+') => {
                                    self.zoom(1);
                                    return;
                                }
                                K::Char('0') => {
                                    self.zoom(0);
                                    return;
                                }
                                // Ctrl+Shift+minus (plain Ctrl+minus is the soft hyphen)
                                K::Char('_') | K::Char('-') if ki.m.shift || ki.k == K::Char('_') => {
                                    self.zoom(-1);
                                    return;
                                }
                                _ => {}
                            }
                        }
                        if ki.k == K::F(4) && ki.m.alt {
                            if std::mem::take(&mut self.ignore_next_f4) {
                                return; // already handled via the close request
                            }
                            self.last_alt_f4 = Some(Instant::now());
                        }
                        self.handle_key(ki);
                        if self.wp.quit {
                            el.exit();
                            return;
                        }
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_px = (position.x, position.y);
                if self.mouse_down {
                    if let Some((c, r)) = self.cell_at(position.x, position.y) {
                        if (c, r) != self.last_cell {
                            self.last_cell = (c, r);
                            self.mouse(crate::ui::MouseKind::Drag, c, r);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some((c, r)) = self.cell_at(self.mouse_px.0, self.mouse_px.1) else { return };
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        self.mouse_down = true;
                        self.last_cell = (c, r);
                        let double = self.last_click.is_some_and(|(t, lc, lr)| t.elapsed() < Duration::from_millis(450) && lc == c && lr == r);
                        if double {
                            self.last_click = None;
                            self.mouse(crate::ui::MouseKind::Double, c, r);
                        } else if self.mods.control_key() && crate::links::ctrl_click(&mut self.wp, c, r) {
                            // Ctrl-click on a hyperlink: opened
                            self.mouse_down = false;
                        } else {
                            self.last_click = Some((Instant::now(), c, r));
                            self.mouse(crate::ui::MouseKind::Down, c, r);
                        }
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        self.mouse_down = false;
                        self.mouse(crate::ui::MouseKind::Up, c, r);
                    }
                    (MouseButton::Right, ElementState::Pressed) => self.mouse(crate::ui::MouseKind::Right, c, r),
                    _ => {}
                }
                if self.wp.quit {
                    el.exit();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let n = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0).round() as i32,
                };
                if n != 0 && self.mods.control_key() {
                    self.zoom(n.signum());
                    return;
                }
                if n != 0 {
                    let (c, r) = self.cell_at(self.mouse_px.0, self.mouse_px.1).unwrap_or((0, 0));
                    self.mouse(crate::ui::MouseKind::Wheel(n), c, r);
                }
            }
            WindowEvent::Focused(true) => {
                self.mods = ModifiersState::empty();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_blink {
            self.blink_on = !self.blink_on;
            self.next_blink = now + BLINK;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        self.wp.maybe_backup();
        // A running macro keeps going in the background of the event loop.
        if crate::pump(&mut self.wp) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        if self.wp.quit {
            el.exit();
        }
        // a macro sleeping in {WAIT} wakes the loop exactly when it is due
        let wake = match crate::wpm::next_deadline(&self.wp) {
            Some(t) if t < self.next_blink => t,
            _ => self.next_blink,
        };
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

pub fn run(wp: Wp) {
    let el = EventLoop::new().expect("event loop");
    let grid = Grid::new(wp.cols, wp.rows, wp.theme.normal);
    let mut app = App {
        wp,
        window: None,
        surface: None,
        buf: Vec::new(),
        renderer: Renderer::new(),
        grid,
        mods: ModifiersState::empty(),
        blink_on: true,
        next_blink: Instant::now() + BLINK,
        last_alt_f4: None,
        ignore_next_f4: false,
        mouse_px: (0.0, 0.0),
        mouse_down: false,
        last_click: None,
        last_cell: (0, 0),
    };
    let _ = default_map;
    el.run_app(&mut app).expect("run");
}
