use std::fs::File;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
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
            width: 1920,
            height: 1080,
        }
    }
}

impl WallpaperGenerator {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Generates a rich multi-stop radial matte gradient:
    /// Center-top (#1b2432 -> RGB 27, 36, 50)
    /// Mid (#11151d -> RGB 17, 21, 29)
    /// Corners (#090b10 -> RGB 9, 11, 16)
    /// With 2.0% monochromatic Gaussian noise grain.
    pub fn generate_rgb_buffer(&self) -> Vec<u8> {
        let total = (self.width * self.height) as usize;
        let mut buffer = vec![0u8; total * 3];

        let center_x = self.width as f32 * 0.5;
        let center_y = self.height as f32 * 0.25; // center-top focal point
        let max_dist = ((self.width as f32).powi(2) + (self.height as f32).powi(2)).sqrt() * 0.65;

        // Color stops
        // 0.0: Center-top #1b2432 (27, 36, 50)
        let c0 = (27.0f32, 36.0f32, 50.0f32);
        // 0.5: Mid #11151d (17, 21, 29)
        let c1 = (17.0f32, 21.0f32, 29.0f32);
        // 1.0: Corners #090b10 (9, 11, 16)
        let c2 = (9.0f32, 11.0f32, 16.0f32);

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

                // Monochromatic pseudo-random grain (~2.0% noise: range [-5.0, 5.0])
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let grain = (((seed >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 6.0;

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

    /// Encodes to a standard 24-bit Windows BMP with top-down scanlines (negative height).
    pub fn generate_bmp_bytes(&self) -> Vec<u8> {
        let rgb_data = self.generate_rgb_buffer();
        let row_padding = (4 - ((self.width * 3) % 4)) % 4;
        let row_size = (self.width * 3 + row_padding) as usize;
        let image_size = row_size * self.height as usize;
        let file_size = 54 + image_size;

        let mut bmp = Vec::with_capacity(file_size);

        // BITMAPFILEHEADER (14 bytes)
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]); // Reserved
        bmp.extend_from_slice(&54u32.to_le_bytes()); // Offset to pixel data

        // BITMAPINFOHEADER (40 bytes)
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(self.width as i32).to_le_bytes());
        // Negative height indicates top-down DIB
        bmp.extend_from_slice(&(-(self.height as i32)).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes()); // Planes
        bmp.extend_from_slice(&24u16.to_le_bytes()); // BPP (24-bit)
        bmp.extend_from_slice(&0u32.to_le_bytes()); // Compression (BI_RGB)
        bmp.extend_from_slice(&(image_size as u32).to_le_bytes());
        bmp.extend_from_slice(&2835u32.to_le_bytes()); // 72 DPI X
        bmp.extend_from_slice(&2835u32.to_le_bytes()); // 72 DPI Y
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());

        let pad = vec![0u8; row_padding as usize];

        // Top-down scanlines in BGR byte order
        for y in 0..self.height {
            let row_start = (y * self.width) as usize * 3;
            for x in 0..self.width {
                let px = row_start + (x as usize * 3);
                let r = rgb_data[px];
                let g = rgb_data[px + 1];
                let b = rgb_data[px + 2];
                // BMP is BGR
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
        let path = dir.join("wallpaper_matte.bmp");

        let bytes = self.generate_bmp_bytes();
        let mut file = File::create(&path)?;
        file.write_all(&bytes)?;

        apply_system_wallpaper(&path);
        Ok(path)
    }
}

pub fn calculate_average_luminance(bmp_bytes: &[u8]) -> f32 {
    if bmp_bytes.len() < 54 {
        return 0.0;
    }

    let pixel_data = &bmp_bytes[54..];
    let mut total_lum = 0.0f64;
    let mut count = 0usize;

    // Sample pixels in BGR order (every 3 bytes)
    let mut i = 0;
    while i + 2 < pixel_data.len() {
        let b = pixel_data[i] as f64;
        let g = pixel_data[i + 1] as f64;
        let r = pixel_data[i + 2] as f64;

        // Rec. 601 perceived luminance
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        total_lum += lum;
        count += 1;

        // Step by 12 bytes (every 4th pixel) for high-speed sampling
        i += 12;
    }

    if count == 0 {
        0.0
    } else {
        (total_lum / count as f64) as f32
    }
}

pub fn apply_system_wallpaper(path: &Path) {
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
