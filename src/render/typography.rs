use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tiny_skia::{Color, PixmapMut};

use crate::error::{OxideError, Result};

pub struct FontRenderer {
    font: fontdue::Font,
    cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl FontRenderer {
    pub fn new() -> Result<Self> {
        let font_candidates = [
            r"C:\Windows\Fonts\SegUIVar.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\segoeuib.ttf",
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\calibri.ttf",
        ];

        let mut font_bytes = None;
        for path_str in font_candidates {
            let path = Path::new(path_str);
            if path.exists() {
                if let Ok(bytes) = fs::read(path) {
                    font_bytes = Some(bytes);
                    break;
                }
            }
        }

        let bytes = font_bytes.ok_or_else(|| {
            OxideError::Font("No compatible Windows system font found in C:\\Windows\\Fonts".to_string())
        })?;

        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(|e| OxideError::Font(format!("Failed to parse font: {}", e)))?;

        Ok(Self {
            font,
            cache: HashMap::new(),
        })
    }

    pub fn measure_text(&self, text: &str, size_px: f32) -> (f32, f32) {
        let mut width = 0.0;
        let mut max_height: f32 = size_px;

        for ch in text.chars() {
            let metrics = self.font.metrics(ch, size_px);
            width += metrics.advance_width;
            max_height = max_height.max(metrics.height as f32);
        }

        (width, max_height)
    }

    pub fn draw_text(
        &mut self,
        pixmap: &mut PixmapMut,
        text: &str,
        start_x: f32,
        start_y: f32,
        size_px: f32,
        color: Color,
    ) {
        let size_key = (size_px * 10.0) as u32;
        let mut pen_x = start_x;
        let pen_y = start_y;

        let pixmap_w = pixmap.width();
        let pixmap_h = pixmap.height();
        let pixels = pixmap.data_mut();

        for ch in text.chars() {
            let (metrics, bitmap) = self
                .cache
                .entry((ch, size_key))
                .or_insert_with(|| self.font.rasterize(ch, size_px));

            let glyph_x = (pen_x + metrics.xmin as f32).round() as i32;
            let glyph_y = (pen_y - metrics.height as f32 - metrics.ymin as f32).round() as i32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha_val = bitmap[row * metrics.width + col];
                    if alpha_val == 0 {
                        continue;
                    }

                    let px = glyph_x + col as i32;
                    let py = glyph_y + row as i32;

                    if px >= 0 && px < pixmap_w as i32 && py >= 0 && py < pixmap_h as i32 {
                        let idx = (py as usize * pixmap_w as usize + px as usize) * 4;

                        let src_alpha = (alpha_val as f32 / 255.0) * color.alpha();
                        let inv_alpha = 1.0 - src_alpha;

                        let src_r = color.red() * src_alpha * 255.0;
                        let src_g = color.green() * src_alpha * 255.0;
                        let src_b = color.blue() * src_alpha * 255.0;
                        let src_a = src_alpha * 255.0;

                        pixels[idx] = (src_r + pixels[idx] as f32 * inv_alpha).min(255.0) as u8;
                        pixels[idx + 1] = (src_g + pixels[idx + 1] as f32 * inv_alpha).min(255.0) as u8;
                        pixels[idx + 2] = (src_b + pixels[idx + 2] as f32 * inv_alpha).min(255.0) as u8;
                        pixels[idx + 3] = (src_a + pixels[idx + 3] as f32 * inv_alpha).min(255.0) as u8;
                    }
                }
            }

            pen_x += metrics.advance_width;
        }
    }
}
