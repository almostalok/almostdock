use std::ffi::c_void;
use tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ, HDC,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

/// A reusable Win32 32-bit top-down layered surface.
/// Backed by a CreateDIBSection and presentable via UpdateLayeredWindow.
pub struct LayeredSurface {
    width: u32,
    height: u32,
    mem_dc: HDC,
    hbitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits_ptr: *mut u8,
}

// LayeredSurface owns GDI handles for a single thread, implement Send safely
unsafe impl Send for LayeredSurface {}

impl LayeredSurface {
    /// Creates a new 32-bit top-down layered surface.
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);

        unsafe {
            let mem_dc = CreateCompatibleDC(0);
            if mem_dc == 0 {
                return Err("Failed to create compatible memory DC".to_string());
            }

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width as i32;
            bmi.bmiHeader.biHeight = -(height as i32); // Negative = top-down DIB
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut bits_raw: *mut c_void = std::ptr::null_mut();
            let hbitmap = CreateDIBSection(
                mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits_raw,
                0,
                0,
            );

            if hbitmap == 0 || bits_raw.is_null() {
                DeleteDC(mem_dc);
                return Err("Failed to create 32-bit DIB section".to_string());
            }

            let old_bitmap = SelectObject(mem_dc, hbitmap as HGDIOBJ);

            Ok(Self {
                width,
                height,
                mem_dc,
                hbitmap,
                old_bitmap,
                bits_ptr: bits_raw as *mut u8,
            })
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Resizes the layered surface if the dimensions have changed.
    pub fn resize(&mut self, new_width: u32, new_height: u32) -> Result<(), String> {
        let new_width = new_width.max(1);
        let new_height = new_height.max(1);

        if self.width == new_width && self.height == new_height {
            return Ok(());
        }

        let new_surf = Self::new(new_width, new_height)?;
        let old = std::mem::replace(self, new_surf);
        drop(old); // Frees previous GDI handles
        Ok(())
    }

    /// Copies a premultiplied RGBA Pixmap into the 32-bit DIB section (swapping R and B to 0xAARRGGBB / BGRA).
    pub fn copy_from_pixmap(&mut self, pixmap: &Pixmap) {
        let min_w = self.width.min(pixmap.width());
        let min_h = self.height.min(pixmap.height());

        unsafe {
            // Zero the entire DIB buffer first
            std::ptr::write_bytes(self.bits_ptr, 0, (self.width * self.height * 4) as usize);

            let src_data = pixmap.data();
            let src_stride = (pixmap.width() * 4) as usize;
            let dst_stride = (self.width * 4) as usize;

            for y in 0..min_h as usize {
                let src_row = &src_data[y * src_stride..(y + 1) * src_stride];
                let dst_row = std::slice::from_raw_parts_mut(
                    self.bits_ptr.add(y * dst_stride),
                    dst_stride,
                );

                for x in 0..min_w as usize {
                    let s = x * 4;
                    let d = x * 4;
                    let r = src_row[s];
                    let g = src_row[s + 1];
                    let b = src_row[s + 2];
                    let a = src_row[s + 3];

                    // Little-endian memory: B, G, R, A = 0xAARRGGBB in u32
                    dst_row[d] = b;
                    dst_row[d + 1] = g;
                    dst_row[d + 2] = r;
                    dst_row[d + 3] = a;
                }
            }
        }
    }

    /// Reads alpha byte of pixel at (x, y) for testing and verification.
    pub fn get_pixel_alpha(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height || self.bits_ptr.is_null() {
            return 0;
        }
        unsafe {
            let offset = ((y * self.width + x) * 4 + 3) as usize;
            *self.bits_ptr.add(offset)
        }
    }

    /// Reads RGB bytes of pixel at (x, y) for testing and verification.
    pub fn get_pixel_rgb(&self, x: u32, y: u32) -> (u8, u8, u8) {
        if x >= self.width || y >= self.height || self.bits_ptr.is_null() {
            return (0, 0, 0);
        }
        unsafe {
            let offset = ((y * self.width + x) * 4) as usize;
            let b = *self.bits_ptr.add(offset);
            let g = *self.bits_ptr.add(offset + 1);
            let r = *self.bits_ptr.add(offset + 2);
            (r, g, b)
        }
    }

    /// Presents the surface to an overlay window via UpdateLayeredWindow.
    /// Sets window screen position (dst_x, dst_y) and per-pixel alpha atomically.
    pub fn present(&self, hwnd: HWND, dst_x: i32, dst_y: i32) -> bool {
        if hwnd == 0 || self.mem_dc == 0 {
            return false;
        }

        let pt_dst = POINT {
            x: dst_x,
            y: dst_y,
        };
        let size = SIZE {
            cx: self.width as i32,
            cy: self.height as i32,
        };
        let pt_src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        unsafe {
            let res = UpdateLayeredWindow(
                hwnd,
                0,
                &pt_dst,
                &size,
                self.mem_dc,
                &pt_src,
                0,
                &blend,
                ULW_ALPHA,
            );
            res != 0
        }
    }
}

impl Drop for LayeredSurface {
    fn drop(&mut self) {
        unsafe {
            if self.mem_dc != 0 {
                SelectObject(self.mem_dc, self.old_bitmap);
                DeleteObject(self.hbitmap as HGDIOBJ);
                DeleteDC(self.mem_dc);
            }
        }
    }
}
