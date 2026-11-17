use std::f32::consts::PI;
use std::process::Command;
use std::sync::Arc;
use tiny_skia::{Color, PixmapMut};

use crate::render::canvas::{draw_glass_squircle, draw_mac_close_button};
use crate::render::spring::Spring;
use crate::render::typography::FontRenderer;
use crate::scripting::ScriptEngine;
use crate::win32::shell_icons::IconCache;

#[derive(Clone)]
pub struct DockItem {
    pub id: String,
    pub name: String,
    pub icon_text: String,
    pub badge: String,
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
    pub mouse_y: Option<f32>,
    pub hovered_index: Option<usize>,
    pub is_hovering_close: bool,
    pub width: f32,
    pub height: f32,
}

impl MagnifyingDock {
    pub fn new(script_engine: &ScriptEngine, icon_cache: &IconCache) -> Self {
        let base_size = 54.0;
        let max_scale = 1.55;
        let influence_radius = 140.0;

        let rhai_items = script_engine.call_dock_items();
        let mut items = Vec::new();

        if !rhai_items.is_empty() {
            for dynamic_item in rhai_items {
                if let Some(map) = dynamic_item.try_cast::<rhai::Map>() {
                    // map has keys: id, name, icon, badge
                    let get_str = |k: &str| -> String {
                        map.get(k)
                            .map(|v| v.clone().into_string().unwrap_or_else(|_| v.to_string()))
                            .unwrap_or_default()
                    };

                    let id = get_str("id");
                    let name = {
                        let n = get_str("name");
                        if n.is_empty() { "App".to_string() } else { n }
                    };
                    let icon_text = {
                        let i = get_str("icon");
                        if i.is_empty() { "🚀".to_string() } else { i }
                    };
                    let badge = get_str("badge");

                    let target = match id.as_str() {
                        "explorer" => "explorer.exe".to_string(),
                        "terminal" => "wt.exe".to_string(),
                        "browser" => "msedge.exe".to_string(),
                        "settings" => "cmd.exe".to_string(),
                        _ => String::new(),
                    };

                    let icon_rgba = if !target.is_empty() {
                        icon_cache.get_or_extract(std::path::Path::new(&target))
                    } else {
                        None
                    };

                    items.push(DockItem {
                        id,
                        name,
                        icon_text,
                        badge,
                        target_path: target,
                        scale_spring: Spring::new(1.0, 280.0, 26.0),
                        is_running: true,
                        icon_rgba,
                    });
                }
            }
        }

        if items.is_empty() {
            // Default fallback
            let defaults = [
                ("explorer", "Explorer", "📁", "explorer.exe"),
                ("terminal", "Terminal", "💻", "wt.exe"),
                ("browser", "Edge", "🌐", "msedge.exe"),
                ("launcher", "Apps", "🚀", ""),
            ];
            for (id, name, icon, target) in defaults {
                let icon_rgba = if !target.is_empty() {
                    icon_cache.get_or_extract(std::path::Path::new(target))
                } else {
                    None
                };
                items.push(DockItem {
                    id: id.to_string(),
                    name: name.to_string(),
                    icon_text: icon.to_string(),
                    badge: String::new(),
                    target_path: target.to_string(),
                    scale_spring: Spring::new(1.0, 280.0, 26.0),
                    is_running: true,
                    icon_rgba,
                });
            }
        }

        Self {
            items,
            base_size,
            max_scale,
            influence_radius,
            mouse_x: None,
            mouse_y: None,
            hovered_index: None,
            is_hovering_close: false,
            width: 440.0,
            height: 76.0,
        }
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        if let Some((x, y)) = pos {
            self.mouse_x = Some(x);
            self.mouse_y = Some(y);

            // Hit test close button in top right of dock
            let close_cx = self.width - 12.0;
            let close_cy = 14.0;
            let dist_sq = (x - close_cx).powi(2) + (y - close_cy).powi(2);
            self.is_hovering_close = dist_sq <= 100.0;
        } else {
            self.mouse_x = None;
            self.mouse_y = None;
            self.is_hovering_close = false;
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
        let mut animating = false;

        let total_items = self.items.len();
        if total_items == 0 {
            return false;
        }

        let padding = 16.0;
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

        self.width = current_x - spacing + padding + 18.0;
        self.height = self.base_size * self.max_scale + 22.0;

        animating
    }

    /// Handles click on dock. Returns true if close button was clicked.
    pub fn handle_click(&self, script_engine: &ScriptEngine) -> bool {
        if self.is_hovering_close {
            return true;
        }

        if let Some(idx) = self.hovered_index {
            if let Some(item) = self.items.get(idx) {
                if !item.id.is_empty() {
                    script_engine.call_dock_click(&item.id);
                } else if !item.target_path.is_empty() {
                    let _ = Command::new("cmd")
                        .args(["/c", "start", "", &item.target_path])
                        .spawn();
                }
            }
        }

        false
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let dock_h = self.base_size + 16.0;
        let dock_y = self.height - dock_h;
        // Translucent dark frosted glass
        let glass_bg = Color::from_rgba(0.08, 0.09, 0.12, 0.72).unwrap_or(Color::BLACK);

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

        // Close button (x) in top-right corner
        draw_mac_close_button(
            pixmap,
            self.width - 12.0,
            dock_y + 12.0,
            6.0,
            self.is_hovering_close,
        );

        let padding = 16.0;
        let spacing = 10.0;
        let mut current_x = padding;

        for (i, item) in self.items.iter().enumerate() {
            let scale = item.scale_spring.value;
            let size = self.base_size * scale;
            let icon_y = self.height - size - 12.0;
            let radius = size * 0.22;

            if let Some(ref _icon_data) = item.icon_rgba {
                draw_glass_squircle(
                    pixmap,
                    current_x,
                    icon_y,
                    size,
                    size,
                    radius,
                    Color::from_rgba(0.18, 0.20, 0.26, 0.9).unwrap_or(Color::WHITE),
                    0.6,
                );
            } else {
                let hue_colors = [
                    Color::from_rgba8(50, 128, 245, 230),
                    Color::from_rgba8(245, 90, 65, 230),
                    Color::from_rgba8(40, 205, 115, 230),
                    Color::from_rgba8(165, 90, 245, 230),
                    Color::from_rgba8(245, 175, 45, 230),
                ];
                let color = hue_colors[i % hue_colors.len()];
                draw_glass_squircle(pixmap, current_x, icon_y, size, size, radius, color, 0.8);

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
                    Color::from_rgba(0.95, 0.96, 1.0, 0.95).unwrap_or(Color::WHITE),
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
