use std::f32::consts::PI;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;
use tiny_skia::{Color, PixmapMut};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::render::canvas::{draw_glass_squircle, draw_mac_close_button};
use crate::render::spring::Spring;
use crate::render::typography::FontRenderer;
use crate::scripting::ScriptEngine;
use crate::win32::shell_icons::{bilinear_scale_rgba, resolve_app_path, IconCache};

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
        let base_size = 48.0;
        let max_scale = 1.45;
        let influence_radius = 120.0;
        let width = 680.0;
        let height = 116.0;

        let rhai_items = script_engine.call_dock_items();
        let mut items = Vec::new();

        if !rhai_items.is_empty() {
            for dynamic_item in rhai_items {
                if let Some(map) = dynamic_item.try_cast::<rhai::Map>() {
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

                    let mut target = get_str("target");
                    if target.is_empty() {
                        target = match id.as_str() {
                            "explorer" => "explorer.exe".to_string(),
                            "terminal" => "wt.exe".to_string(),
                            "browser" => "msedge.exe".to_string(),
                            "code" => "Code.exe".to_string(),
                            "taskmgr" => "taskmgr.exe".to_string(),
                            _ => String::new(),
                        };
                    }

                    let resolved = if !target.is_empty() {
                        resolve_app_path(std::path::Path::new(&target))
                    } else {
                        std::path::PathBuf::new()
                    };

                    let icon_rgba = if !target.is_empty() {
                        icon_cache.get_or_extract(&resolved).or_else(|| icon_cache.get_or_extract(std::path::Path::new(&target)))
                    } else {
                        None
                    };

                    items.push(DockItem {
                        id,
                        name,
                        icon_text,
                        badge,
                        target_path: resolved.to_string_lossy().to_string(),
                        scale_spring: Spring::new(1.0, 280.0, 26.0),
                        is_running: true,
                        icon_rgba,
                    });
                }
            }
        }

        if items.is_empty() {
            let defaults = [
                ("explorer", "Explorer", "📁", "explorer.exe"),
                ("terminal", "Terminal", "💻", "wt.exe"),
                ("browser", "Browser", "🌐", "msedge.exe"),
                ("code", "Code", "📝", "Code.exe"),
                ("taskmgr", "Activity", "📊", "taskmgr.exe"),
                ("launcher", "Launcher", "🚀", ""),
            ];
            for (id, name, icon, target) in defaults {
                let resolved = if !target.is_empty() {
                    resolve_app_path(std::path::Path::new(target))
                } else {
                    std::path::PathBuf::new()
                };

                let icon_rgba = if !target.is_empty() {
                    icon_cache.get_or_extract(&resolved)
                } else {
                    None
                };

                items.push(DockItem {
                    id: id.to_string(),
                    name: name.to_string(),
                    icon_text: icon.to_string(),
                    badge: String::new(),
                    target_path: resolved.to_string_lossy().to_string(),
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
            width,
            height,
        }
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        if let Some((x, y)) = pos {
            self.mouse_x = Some(x);
            self.mouse_y = Some(y);

            // Hit-test circular 20x20 close button in top right of frosted pill
            let pill_y = 38.0;
            let close_cx = self.width - 24.0;
            let close_cy = pill_y + 14.0;
            let dist_sq = (x - close_cx).powi(2) + (y - close_cy).powi(2);
            self.is_hovering_close = dist_sq <= 100.0; // 20x20 hitbox (radius 10)
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

        let spacing = 12.0;
        let content_w = (total_items as f32) * self.base_size + ((total_items - 1) as f32) * spacing;
        let start_x = (self.width - content_w) / 2.0;

        self.hovered_index = None;

        for (i, item) in self.items.iter_mut().enumerate() {
            let item_base_x = start_x + (i as f32) * (self.base_size + spacing);
            let item_center = item_base_x + self.base_size / 2.0;

            let target_scale = match self.mouse_x {
                Some(mx) => {
                    let dist = (mx - item_center).abs();

                    if dist < self.base_size * 0.6 {
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
        }

        animating
    }

    /// Handles click on dock. Returns true if close button was clicked.
    pub fn handle_click(&self, script_engine: &ScriptEngine) -> bool {
        if self.is_hovering_close {
            return true;
        }

        if let Some(idx) = self.hovered_index {
            if let Some(item) = self.items.get(idx) {
                if !item.id.is_empty() && item.id != "launcher" {
                    script_engine.call_dock_click(&item.id);
                } else if item.id == "launcher" {
                    if let Ok(mut flag) = script_engine.pending_toggle_launcher.lock() {
                        *flag = true;
                    }
                } else if !item.target_path.is_empty() {
                    let wide_target: Vec<u16> = std::ffi::OsStr::new(&item.target_path)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    let wide_open: Vec<u16> = std::ffi::OsStr::new("open")
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();
                    unsafe {
                        ShellExecuteW(
                            0,
                            wide_open.as_ptr(),
                            wide_target.as_ptr(),
                            std::ptr::null(),
                            std::ptr::null(),
                            SW_SHOWNORMAL,
                        );
                    }
                }
            }
        }

        false
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        // Draw the frosted glass pill (height = 68, y_offset = 38, radius = 22)
        // Upward magnification up to 1.45x has 38px of clean transparent headroom!
        let pill_y = 38.0;
        let pill_h = 68.0;
        let pill_r = 22.0;

        let glass_bg = Color::from_rgba(0.08, 0.09, 0.12, 0.72).unwrap_or(Color::BLACK);

        draw_glass_squircle(
            pixmap,
            10.0,
            pill_y,
            self.width - 20.0,
            pill_h,
            pill_r,
            glass_bg,
            1.0,
        );

        // Circular 20x20 macOS close button (x) in top-right of the pill
        let close_cx = self.width - 24.0;
        let close_cy = pill_y + 14.0;
        draw_mac_close_button(
            pixmap,
            close_cx,
            close_cy,
            7.0,
            self.is_hovering_close,
        );

        let total_items = self.items.len();
        if total_items == 0 {
            return;
        }

        let spacing = 12.0;
        let content_w = (total_items as f32) * self.base_size + ((total_items - 1) as f32) * spacing;
        let start_x = (self.width - content_w) / 2.0;

        for (i, item) in self.items.iter().enumerate() {
            let scale = item.scale_spring.value;
            let size = self.base_size * scale;
            let base_x = start_x + (i as f32) * (self.base_size + spacing);
            // Center scaled icon on its slot
            let icon_x = base_x - (size - self.base_size) / 2.0;
            // Align icon bottom to pill bottom margin
            let icon_y = (pill_y + pill_h - 10.0) - size;
            let radius = size * 0.22;

            if let Some(ref icon_data) = item.icon_rgba {
                // Background backing glass squircle
                draw_glass_squircle(
                    pixmap,
                    icon_x,
                    icon_y,
                    size,
                    size,
                    radius,
                    Color::from_rgba(0.12, 0.14, 0.18, 0.85).unwrap_or(Color::WHITE),
                    0.5,
                );

                // Blit real extracted 48x48 RGBA icon
                composite_rgba_icon(
                    pixmap,
                    icon_data,
                    48,
                    48,
                    icon_x + size * 0.1,
                    icon_y + size * 0.1,
                    size * 0.8,
                );
            } else {
                // Vector fallback squircle
                draw_glass_squircle(
                    pixmap,
                    icon_x,
                    icon_y,
                    size,
                    size,
                    radius,
                    Color::from_rgba(0.18, 0.22, 0.32, 0.85).unwrap_or(Color::WHITE),
                    0.6,
                );

                font.draw_text(
                    pixmap,
                    &item.icon_text,
                    icon_x + size * 0.28,
                    icon_y + size * 0.68,
                    size * 0.44,
                    Color::WHITE,
                );
            }

            // Running indicator dot
            if item.is_running {
                let dot_x = base_x + (self.base_size - 4.0) / 2.0;
                let dot_y = pill_y + pill_h - 6.0;
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
                let badge_x = base_x + (self.base_size - badge_w) / 2.0;
                let badge_y = (icon_y - 22.0).max(2.0);

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
        }
    }
}

/// Bilinear alpha-compositing helper for drawing icons directly into PixmapMut
fn composite_rgba_icon(
    pixmap: &mut PixmapMut,
    icon_data: &[u8],
    src_w: usize,
    src_h: usize,
    dst_x: f32,
    dst_y: f32,
    dst_size: f32,
) {
    let dst_dim = dst_size.round() as usize;
    if dst_dim == 0 {
        return;
    }

    let scaled = bilinear_scale_rgba(icon_data, src_w, src_h, dst_dim, dst_dim);
    let pix_w = pixmap.width() as i32;
    let pix_h = pixmap.height() as i32;
    let dx_start = dst_x.round() as i32;
    let dy_start = dst_y.round() as i32;

    let pix_data = pixmap.data_mut();

    for y in 0..dst_dim as i32 {
        let py = dy_start + y;
        if py < 0 || py >= pix_h {
            continue;
        }
        for x in 0..dst_dim as i32 {
            let px = dx_start + x;
            if px < 0 || px >= pix_w {
                continue;
            }

            let s_idx = ((y as usize) * dst_dim + (x as usize)) * 4;
            let sr = scaled[s_idx] as u32;
            let sg = scaled[s_idx + 1] as u32;
            let sb = scaled[s_idx + 2] as u32;
            let sa = scaled[s_idx + 3] as u32;

            if sa == 0 {
                continue;
            }

            let d_idx = ((py as usize) * (pix_w as usize) + (px as usize)) * 4;
            let dr = pix_data[d_idx] as u32;
            let dg = pix_data[d_idx + 1] as u32;
            let db = pix_data[d_idx + 2] as u32;
            let da = pix_data[d_idx + 3] as u32;

            let inv_sa = 255 - sa;
            pix_data[d_idx] = (sr + (dr * inv_sa) / 255).min(255) as u8;
            pix_data[d_idx + 1] = (sg + (dg * inv_sa) / 255).min(255) as u8;
            pix_data[d_idx + 2] = (sb + (db * inv_sa) / 255).min(255) as u8;
            pix_data[d_idx + 3] = (sa + (da * inv_sa) / 255).min(255) as u8;
        }
    }
}
