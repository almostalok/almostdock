use std::fs::File;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
};

use crate::config::OxideConfig;
use crate::error::Result;

pub struct WallpaperGenerator {
    pub width: u32,
    pub height: u32,
}

impl Default for WallpaperGenerator {
    fn default() -> Self {
        Self {
            width: 3840,
            height: 2160,
        }
    }
}

impl WallpaperGenerator {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Generates a smooth macOS Sonoma/Sequoia inspired dual-tone dusk gradient with Gaussian noise dither.
    pub fn generate_rgb_buffer(&self) -> Vec<u8> {
        let total_pixels = (self.width * self.height) as usize;
        let mut buffer = vec![0u8; total_pixels * 3];

        // Gradient color stops (Deep Midnight Obsidian -> Aurora Iris)
        let top_r = 15.0f32;
        let top_g = 18.0f32;
        let top_b = 32.0f32;

        let bot_r = 38.0f32;
        let bot_g = 26.0f32;
        let bot_b = 64.0f32;

        let inv_h = 1.0 / (self.height as f32);
        let inv_w = 1.0 / (self.width as f32);

        // Simple high-speed linear congruential pseudo-noise for dither
        let mut seed: u32 = 0x1337BEEF;

        for y in 0..self.height {
            let ty = y as f32 * inv_h;

            for x in 0..self.width {
                let tx = x as f32 * inv_w;
                let t = (ty * 0.75 + tx * 0.25).clamp(0.0, 1.0);

                // LCG PRNG for subtle ±2 LSB noise
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = ((seed >> 24) as f32 / 255.0 - 0.5) * 4.0;

                let r = (top_r + (bot_r - top_r) * t + noise).clamp(0.0, 255.0) as u8;
                let g = (top_g + (bot_g - top_g) * t + noise).clamp(0.0, 255.0) as u8;
                let b = (top_b + (bot_b - top_b) * t + noise).clamp(0.0, 255.0) as u8;

                let idx = ((y * self.width + x) as usize) * 3;
                buffer[idx] = r;
                buffer[idx + 1] = g;
                buffer[idx + 2] = b;
            }
        }

        buffer
    }

    /// Encodes the 24-bit RGB buffer into a standard uncompressed Windows BMP file.
    pub fn generate_bmp_bytes(&self) -> Vec<u8> {
        let rgb_data = self.generate_rgb_buffer();
        let row_padding = (4 - ((self.width * 3) % 4)) % 4;
        let row_size = (self.width * 3 + row_padding) as usize;
        let image_size = row_size * self.height as usize;
        let file_size = 54 + image_size;

        let mut bmp = Vec::with_capacity(file_size);

        // --- BMP Header (14 bytes) ---
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]); // Reserved
        bmp.extend_from_slice(&54u32.to_le_bytes()); // Pixel data offset

        // --- DIB Header / BITMAPINFOHEADER (40 bytes) ---
        bmp.extend_from_slice(&40u32.to_le_bytes()); // Header size
        bmp.extend_from_slice(&(self.width as i32).to_le_bytes());
        bmp.extend_from_slice(&(self.height as i32).to_le_bytes()); // Bottom-up
        bmp.extend_from_slice(&1u16.to_le_bytes()); // Planes
        bmp.extend_from_slice(&24u16.to_le_bytes()); // Bits per pixel
        bmp.extend_from_slice(&0u32.to_le_bytes()); // Compression (BI_RGB)
        bmp.extend_from_slice(&(image_size as u32).to_le_bytes());
        bmp.extend_from_slice(&2835u32.to_le_bytes()); // 72 DPI X
        bmp.extend_from_slice(&2835u32.to_le_bytes()); // 72 DPI Y
        bmp.extend_from_slice(&0u32.to_le_bytes()); // Colors used
        bmp.extend_from_slice(&0u32.to_le_bytes()); // Important colors

        // BMP rows are bottom-up and BGR ordered
        let pad = vec![0u8; row_padding as usize];
        for y in (0..self.height).rev() {
            let row_start = (y * self.width) as usize * 3;
            for x in 0..self.width {
                let px = row_start + (x as usize * 3);
                let r = rgb_data[px];
                let g = rgb_data[px + 1];
                let b = rgb_data[px + 2];
                bmp.push(b);
                bmp.push(g);
                bmp.push(r);
            }
            bmp.extend_from_slice(&pad);
        }

        bmp
    }

    pub fn generate_and_apply(&self) -> Result<PathBuf> {
        let dir = OxideConfig::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("wallpaper.bmp");

        let bytes = self.generate_bmp_bytes();
        let mut file = File::create(&path)?;
        file.write_all(&bytes)?;

        apply_system_wallpaper(&path);
        Ok(path)
    }
}

pub fn apply_system_wallpaper(path: &std::path::Path) {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            wide.as_ptr() as *mut _,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        );
    }
}
