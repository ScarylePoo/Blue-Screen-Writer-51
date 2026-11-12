//! Put a 0x00RRGGBB pixel buffer on the window.
//! Windows: plain GDI (SetDIBitsToDevice).  Elsewhere: softbuffer.

use std::rc::Rc;
use winit::window::Window;

#[cfg(windows)]
mod gdi {
    #[repr(C)]
    pub struct BitmapInfoHeader {
        pub size: u32,
        pub width: i32,
        pub height: i32,
        pub planes: u16,
        pub bit_count: u16,
        pub compression: u32,
        pub size_image: u32,
        pub xppm: i32,
        pub yppm: i32,
        pub clr_used: u32,
        pub clr_important: u32,
    }
    #[repr(C)]
    pub struct BitmapInfo {
        pub header: BitmapInfoHeader,
        pub colors: [u32; 1],
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetDC(hwnd: isize) -> isize;
        pub fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        pub fn SetDIBitsToDevice(
            hdc: isize,
            x: i32,
            y: i32,
            w: u32,
            h: u32,
            sx: i32,
            sy: i32,
            start: u32,
            lines: u32,
            bits: *const core::ffi::c_void,
            bmi: *const BitmapInfo,
            usage: u32,
        ) -> i32;
    }
}

pub struct Presenter {
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(not(windows))]
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    #[allow(dead_code)]
    window: Rc<Window>,
}

impl Presenter {
    pub fn new(window: Rc<Window>) -> Presenter {
        #[cfg(windows)]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let hwnd = match window.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
                _ => 0,
            };
            Presenter { hwnd, window }
        }
        #[cfg(not(windows))]
        {
            let ctx = softbuffer::Context::new(window.clone()).expect("softbuffer context");
            let surface = softbuffer::Surface::new(&ctx, window.clone()).expect("surface");
            Presenter { surface, window }
        }
    }

    pub fn present(&mut self, buf: &[u32], w: usize, h: usize) {
        if w == 0 || h == 0 || buf.len() < w * h {
            return;
        }
        #[cfg(windows)]
        unsafe {
            let hdc = gdi::GetDC(self.hwnd);
            if hdc == 0 {
                return;
            }
            let bmi = gdi::BitmapInfo {
                header: gdi::BitmapInfoHeader {
                    size: std::mem::size_of::<gdi::BitmapInfoHeader>() as u32,
                    width: w as i32,
                    height: -(h as i32), // top-down
                    planes: 1,
                    bit_count: 32,
                    compression: 0,
                    size_image: 0,
                    xppm: 0,
                    yppm: 0,
                    clr_used: 0,
                    clr_important: 0,
                },
                colors: [0],
            };
            gdi::SetDIBitsToDevice(hdc, 0, 0, w as u32, h as u32, 0, 0, 0, h as u32, buf.as_ptr() as *const _, &bmi, 0);
            gdi::ReleaseDC(self.hwnd, hdc);
        }
        #[cfg(not(windows))]
        {
            use std::num::NonZeroU32;
            if self.surface.resize(NonZeroU32::new(w as u32).unwrap(), NonZeroU32::new(h as u32).unwrap()).is_err() {
                return;
            }
            if let Ok(mut b) = self.surface.buffer_mut() {
                b.copy_from_slice(&buf[..w * h]);
                let _ = b.present();
            }
        }
    }
}
