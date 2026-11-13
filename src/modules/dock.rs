use std::f32::consts::PI;
use std::process::Command;
use std::sync::Arc;
use tiny_skia::{Color, PixmapMut};

use crate::config::PinnedApp;
use crate::render::canvas::draw_glass_squircle;
use crate::render::spring::Spring;
use crate::render::typography::FontRenderer;
use crate::win32::shell_icons::IconCache;

pub struct DockItem {
    pub name: String,
    pub target_path: String,
    pub scale_spring: Spring,
    pub is_running: bool,
    pub icon_rgba: Option<Arc<Vec<u8>>>,
}

pub struct MagnifyingDock {
    pub items: Vec<DockItem>,
    pub base_size: f32,
    pub max_scale: f32,
    pub influence_radius: f32,
    pub mouse_x: Option<f32>,
    pub hovered_index: Option<usize>,
    pub width: f32,
    pub height: f32,
}

impl MagnifyingDock {
    pub fn new(pinned: &[PinnedApp], icon_cache: &IconCache) -> Self {
        let base_size = 54.0;
        let max_scale = 1.55;
        let influence_radius = 140.0;

        let items = pinned
            .iter()
            .map(|app| {
                let icon_rgba = icon_cache.get_or_extract(std::path::Path::new(&app.target_path));
                DockItem {
                    name: app.name.clone(),
                    target_path: app.target_path.clone(),
                    scale_spring: Spring::new(1.0, 280.0, 26.0),
                    is_running: true, // running indicator sample
                    icon_rgba,
                }
            })
            .collect();

        Self {
            items,
            base_size,
            max_scale,
            influence_radius,
            mouse_x: None,
            hovered_index: None,
            width: 400.0,
            height: 76.0,
        }
    }

    pub fn set_mouse_position(&mut self, mouse_x: Option<f32>) {
        self.mouse_x = mouse_x;
    }

    pub fn update(&mut self, dt: f32) -> bool {
        let mut animating = false;

        // Calculate icon centers
        let total_items = self.items.len();
        if total_items == 0 {
            return false;
        }

        let padding = 12.0;
        let spacing = 10.0;
        let mut current_x = padding;

        self.hovered_index = None;

        for (i, item) in self.items.iter_mut().enumerate() {
            let target_scale = match self.mouse_x {
                Some(mx) => {
                    let icon_center = current_x + (self.base_size * item.scale_spring.value) / 2.0;
                    let dist = (mx - icon_center).abs();

                    if dist < (self.base_size * item.scale_spring.value) / 2.0 {
                        self.hovered_index = Some(i);
                    }

                    if dist <= self.influence_radius {
                        // Gaussian cosine magnification curve
                        let cos_val = (PI * dist / (2.0 * self.influence_radius)).cos();
                        1.0 + (self.max_scale - 1.0) * cos_val * cos_val
                    } else {
                        1.0
                    }
                }
                None => 1.0,
            };

            item.scale_spring.set_target(target_scale);
            if item.scale_spring.update(dt) {
                animating = true;
            }

            current_x += (self.base_size * item.scale_spring.value) + spacing;
        }

        self.width = current_x - spacing + padding;
        self.height = self.base_size * self.max_scale + 22.0;

        animating
    }

    pub fn handle_click(&self, _click_x: f32) {
        if let Some(idx) = self.hovered_index {
            if let Some(item) = self.items.get(idx) {
                // Launch app
                let _ = Command::new("cmd")
                    .args(["/c", "start", "", &item.target_path])
                    .spawn();
            }
        }
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let dock_h = self.base_size + 16.0;
        let dock_y = self.height - dock_h;
        let glass_bg = Color::from_rgba(0.06, 0.06, 0.09, 0.82).unwrap_or(Color::BLACK);

        // Render glass tray dock background
        draw_glass_squircle(
            pixmap,
            0.0,
            dock_y,
            self.width,
            dock_h,
            22.0,
            glass_bg,
            1.0,
        );

        let padding = 12.0;
        let spacing = 10.0;
        let mut current_x = padding;

        for (i, item) in self.items.iter().enumerate() {
            let scale = item.scale_spring.value;
            let size = self.base_size * scale;
            let icon_y = self.height - size - 12.0;
            let radius = size * 0.22;

            // Draw app icon or squircle fallback
            if let Some(ref _icon_data) = item.icon_rgba {
                // Draw frosted icon backdrop
                draw_glass_squircle(
                    pixmap,
                    current_x,
                    icon_y,
                    size,
                    size,
                    radius,
                    Color::from_rgba(0.18, 0.2, 0.26, 0.9).unwrap_or(Color::WHITE),
                    0.6,
                );
            } else {
                // Minimalist vector app squircle with accent gradient
                let hue_colors = [
                    Color::from_rgba8(50, 128, 245, 240),
                    Color::from_rgba8(245, 90, 65, 240),
                    Color::from_rgba8(40, 205, 115, 240),
                    Color::from_rgba8(165, 90, 245, 240),
                ];
                let color = hue_colors[i % hue_colors.len()];
                draw_glass_squircle(pixmap, current_x, icon_y, size, size, radius, color, 0.8);

                // Initial letter
                let first_char = item.name.chars().next().unwrap_or('A').to_string();
                font.draw_text(
                    pixmap,
                    &first_char,
                    current_x + size * 0.32,
                    icon_y + size * 0.68,
                    size * 0.48,
                    Color::WHITE,
                );
            }

            // Running indicator dot
            if item.is_running {
                let dot_x = current_x + (size - 4.0) / 2.0;
                let dot_y = self.height - 6.0;
                draw_glass_squircle(
                    pixmap,
                    dot_x,
                    dot_y,
                    4.0,
                    4.0,
                    2.0,
                    Color::from_rgba(0.9, 0.95, 1.0, 0.9).unwrap_or(Color::WHITE),
                    0.2,
                );
            }

            // Hover tooltip
            if self.hovered_index == Some(i) {
                let (tip_w, _) = font.measure_text(&item.name, 11.0);
                let badge_w = tip_w + 16.0;
                let badge_x = current_x + (size - badge_w) / 2.0;
                let badge_y = icon_y - 24.0;

                draw_glass_squircle(
                    pixmap,
                    badge_x,
                    badge_y,
                    badge_w,
                    18.0,
                    9.0,
                    Color::from_rgba(0.04, 0.04, 0.06, 0.9).unwrap_or(Color::BLACK),
                    0.4,
                );

                font.draw_text(
                    pixmap,
                    &item.name,
                    badge_x + 8.0,
                    badge_y + 13.0,
                    11.0,
                    Color::WHITE,
                );
            }

            current_x += size + spacing;
        }
    }
}
