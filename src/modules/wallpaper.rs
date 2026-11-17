use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
};

use crate::config::OxideConfig;
use crate::error::Result;

// CLSID_DesktopWallpaper: C2CF3110-460E-4fc1-B9D0-8A1C0C9CC4BD
const CLSID_DESKTOP_WALLPAPER: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0xC2CF3110,
    data2: 0x460E,
    data3: 0x4FC1,
    data4: [0xB9, 0xD0, 0x8A, 0x1C, 0x0C, 0x9C, 0xC4, 0xBD],
};

// IID_IDesktopWallpaper: B92B56A9-8B55-4E14-9A89-0199BBB6F93B
const IID_IDESKTOP_WALLPAPER: windows_sys::core::GUID = windows_sys::core::GUID {
    data1: 0xB92B56A9,
    data2: 0x8B55,
    data3: 0x4E14,
    data4: [0x9A, 0x89, 0x01, 0x99, 0xBB, 0xB6, 0xF9, 0x3B],
};

#[repr(C)]
#[allow(non_snake_case)]
struct IDesktopWallpaperVtbl {
    pub QueryInterface: unsafe extern "system" fn(this: *mut std::ffi::c_void, riid: *const windows_sys::core::GUID, ppv: *mut *mut std::ffi::c_void) -> i32,
    pub AddRef: unsafe extern "system" fn(this: *mut std::ffi::c_void) -> u32,
    pub Release: unsafe extern "system" fn(this: *mut std::ffi::c_void) -> u32,
    pub SetWallpaper: unsafe extern "system" fn(this: *mut std::ffi::c_void, monitor_id: *const u16, wallpaper: *const u16) -> i32,
    pub GetWallpaper: unsafe extern "system" fn(this: *mut std::ffi::c_void, monitor_id: *const u16, wallpaper: *mut *mut u16) -> i32,
    // (other methods not needed for setting wallpaper)
}

pub struct WallpaperGenerator {
    pub width: u32,
    pub height: u32,
}

impl Default for WallpaperGenerator {
    fn default() -> Self {
        let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        Self {
            width: if sw > 0 { sw as u32 } else { 1920 },
            height: if sh > 0 { sh as u32 } else { 1080 },
        }
    }
}

impl WallpaperGenerator {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Generates a rich macOS Sequoia-style dark radial/mesh gradient:
    /// #1e293b upper-right glow (RGB 30, 41, 59)
    /// #111622 mid (RGB 17, 22, 34)
    /// #090c12 deep matte corners (RGB 9, 12, 18)
    /// With 2.2% monochromatic Gaussian grain.
    pub fn generate_rgb_buffer(&self) -> Vec<u8> {
        let total = (self.width * self.height) as usize;
        let mut buffer = vec![0u8; total * 3];

        let center_x = self.width as f32 * 0.72; // upper-right focal point
        let center_y = self.height as f32 * 0.22;
        let max_dist = ((self.width as f32).powi(2) + (self.height as f32).powi(2)).sqrt() * 0.75;

        // Color stops
        let c0 = (30.0f32, 41.0f32, 59.0f32); // #1e293b upper-right glow
        let c1 = (17.0f32, 22.0f32, 34.0f32); // #111622 mid
        let c2 = (9.0f32, 12.0f32, 18.0f32);   // #090c12 deep matte corners

        let mut seed: u32 = 0x5EED_CAFE;

        for y in 0..self.height {
            let dy = y as f32 - center_y;
            for x in 0..self.width {
                let dx = x as f32 - center_x;
                let dist = (dx * dx + dy * dy).sqrt();
                let t = (dist / max_dist).clamp(0.0, 1.0);

                let (base_r, base_g, base_b) = if t < 0.5 {
                    let sub_t = t * 2.0;
                    (
                        c0.0 + (c1.0 - c0.0) * sub_t,
                        c0.1 + (c1.1 - c0.1) * sub_t,
                        c0.2 + (c1.2 - c0.2) * sub_t,
                    )
                } else {
                    let sub_t = (t - 0.5) * 2.0;
                    (
                        c1.0 + (c2.0 - c1.0) * sub_t,
                        c1.1 + (c2.1 - c1.1) * sub_t,
                        c1.2 + (c2.2 - c1.2) * sub_t,
                    )
                };

                // Monochromatic pseudo-random grain (~2.2% noise: range [-5.6, 5.6])
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let grain = (((seed >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 6.5;

                let r = (base_r + grain).clamp(0.0, 255.0) as u8;
                let g = (base_g + grain).clamp(0.0, 255.0) as u8;
                let b = (base_b + grain).clamp(0.0, 255.0) as u8;

                let idx = ((y * self.width + x) as usize) * 3;
                buffer[idx] = r;
                buffer[idx + 1] = g;
                buffer[idx + 2] = b;
            }
        }

        buffer
    }

    /// Encodes the gradient directly to a valid PNG file and drops pixel memory immediately.
    pub fn generate_and_save_png(&self, path: &Path) -> Result<PathBuf> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let rgb_buffer = self.generate_rgb_buffer();

        let file = File::create(path)?;
        let mut w = std::io::BufWriter::new(file);

        let mut encoder = png::Encoder::new(&mut w, self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);

        let mut writer = encoder.write_header().map_err(|e| crate::error::OxideError::Wallpaper(e.to_string()))?;
        writer.write_image_data(&rgb_buffer).map_err(|e| crate::error::OxideError::Wallpaper(e.to_string()))?;

        drop(rgb_buffer); // Immediate purge from memory

        Ok(path.to_path_buf())
    }

    /// Generates wallpaper.png in %APPDATA%\OxideShell and applies it via COM + SPI fallback
    pub fn generate_and_apply(&self) -> Result<PathBuf> {
        let dir = OxideConfig::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("wallpaper.png");

        self.generate_and_save_png(&path)?;
        apply_system_wallpaper(&path);
        Ok(path)
    }
}

/// Applies wallpaper via COM IDesktopWallpaper, with SystemParametersInfoW fallback
pub fn apply_system_wallpaper(path: &Path) {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // 1. Try Windows COM IDesktopWallpaper interface
    let mut com_applied = false;
    unsafe {
        CoInitializeEx(std::ptr::null_mut(), 0);

        let mut wallpaper_obj: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_DESKTOP_WALLPAPER,
            std::ptr::null_mut(),
            CLSCTX_ALL,
            &IID_IDESKTOP_WALLPAPER,
            &mut wallpaper_obj,
        );

        if hr >= 0 && !wallpaper_obj.is_null() {
            let vtbl = *(wallpaper_obj as *mut *mut IDesktopWallpaperVtbl);
            let set_hr = ((*vtbl).SetWallpaper)(wallpaper_obj, std::ptr::null(), wide.as_ptr());
            if set_hr >= 0 {
                com_applied = true;
            }
            ((*vtbl).Release)(wallpaper_obj);
        }

        CoUninitialize();
    }

    // 2. Fallback to SystemParametersInfoW (works across all Windows versions)
    if !com_applied {
        unsafe {
            SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                wide.as_ptr() as *mut _,
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            );
        }
    }
}

/// Calculates average perceived luminance from raw RGB buffer for validation tests
pub fn calculate_rgb_average_luminance(rgb_data: &[u8]) -> f32 {
    if rgb_data.len() < 3 {
        return 0.0;
    }

    let mut total_lum = 0.0f64;
    let mut count = 0usize;

    let mut i = 0;
    while i + 2 < rgb_data.len() {
        let r = rgb_data[i] as f64;
        let g = rgb_data[i + 1] as f64;
        let b = rgb_data[i + 2] as f64;

        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        total_lum += lum;
        count += 1;

        i += 12; // Sample every 4th pixel for speed
    }

    if count == 0 {
        0.0
    } else {
        (total_lum / count as f64) as f32
    }
}
