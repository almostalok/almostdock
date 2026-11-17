use tiny_skia::{Color, PixmapMut};

use crate::render::canvas::{draw_glass_squircle, draw_mac_close_button};
use crate::render::spring::Spring;
use crate::render::typography::FontRenderer;
use crate::win32::media::MediaBridge;
use crate::win32::telemetry::{get_process_memory, SystemCpuSampler};

pub enum NotchMode {
    Compact,
    Expanded,
}

pub struct DynamicNotch {
    pub width_spring: Spring,
    pub height_spring: Spring,
    pub mode: NotchMode,
    pub is_hovered: bool,
    pub is_hovering_close: bool,
    pub media_bridge: MediaBridge,
    pub cpu_sampler: SystemCpuSampler,
    pub current_cpu: f32,
    pub current_ram_mb: f64,
}

impl Default for DynamicNotch {
    fn default() -> Self {
        Self {
            width_spring: Spring::new(220.0, 260.0, 26.0),
            height_spring: Spring::new(34.0, 260.0, 26.0),
            mode: NotchMode::Compact,
            is_hovered: false,
            is_hovering_close: false,
            media_bridge: MediaBridge::new(),
            cpu_sampler: SystemCpuSampler::new(),
            current_cpu: 0.0,
            current_ram_mb: 0.0,
        }
    }
}

impl DynamicNotch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.is_hovered = hovered;
        if hovered {
            self.width_spring.set_target(540.0);
            self.height_spring.set_target(160.0);
            self.mode = NotchMode::Expanded;
        } else {
            self.width_spring.set_target(220.0);
            self.height_spring.set_target(34.0);
            self.mode = NotchMode::Compact;
            self.is_hovering_close = false;
        }
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        if let Some((x, y)) = pos {
            let w = self.width_spring.value;
            let close_cx = w - 16.0;
            let close_cy = 16.0;
            let dist_sq = (x - close_cx).powi(2) + (y - close_cy).powi(2);
            self.is_hovering_close = dist_sq <= 100.0;
        } else {
            self.is_hovering_close = false;
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
        let w_anim = self.width_spring.update(dt);
        let h_anim = self.height_spring.update(dt);

        self.media_bridge.update_progress(dt);

        if self.is_hovered {
            self.current_cpu = self.cpu_sampler.sample();
            self.current_ram_mb = get_process_memory().private_usage_mb();
        }

        w_anim || h_anim
    }

    /// Returns true if close button was clicked.
    pub fn handle_click(&mut self, rel_x: f32, rel_y: f32) -> bool {
        if self.is_hovering_close {
            self.set_hovered(false);
            return true;
        }

        if !self.is_hovered {
            self.set_hovered(true);
            return false;
        }

        // Expanded media controls click detection
        let h = self.height_spring.value;
        if rel_y > (h - 48.0) {
            let center_x = self.width_spring.value / 2.0;
            if (rel_x - (center_x - 52.0)).abs() < 20.0 {
                self.media_bridge.prev_track();
            } else if (rel_x - center_x).abs() < 24.0 {
                self.media_bridge.play_pause();
            } else if (rel_x - (center_x + 52.0)).abs() < 20.0 {
                self.media_bridge.next_track();
            }
        }

        false
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let w = self.width_spring.value;
        let h = self.height_spring.value;
        let r = 18.0f32.min(h / 2.0);

        // Translucent dark glass background (premultiplied RGBA)
        let bg_color = Color::from_rgba(0.07, 0.08, 0.11, 0.78).unwrap_or(Color::BLACK);
        draw_glass_squircle(pixmap, 0.0, 0.0, w, h, r, bg_color, 1.0);

        if h < 55.0 {
            // --- Compact Idle Pill ---
            // Green status dot
            draw_glass_squircle(
                pixmap,
                14.0,
                (h - 8.0) / 2.0,
                8.0,
                8.0,
                4.0,
                Color::from_rgba(0.2, 0.85, 0.35, 0.95).unwrap_or(Color::WHITE),
                0.5,
            );

            // Clock / pill label
            font.draw_text(
                pixmap,
                "Dynamic Island",
                30.0,
                h / 2.0 + 4.0,
                11.0,
                Color::from_rgba(0.92, 0.94, 0.98, 0.9).unwrap_or(Color::WHITE),
            );

            // Audio wave indicator
            draw_glass_squircle(
                pixmap,
                w - 28.0,
                (h - 10.0) / 2.0,
                3.0,
                10.0,
                1.5,
                Color::from_rgba(1.0, 0.35, 0.45, 0.9).unwrap_or(Color::WHITE),
                0.2,
            );
            draw_glass_squircle(
                pixmap,
                w - 22.0,
                (h - 14.0) / 2.0,
                3.0,
                14.0,
                1.5,
                Color::from_rgba(1.0, 0.35, 0.45, 0.9).unwrap_or(Color::WHITE),
                0.2,
            );
            draw_glass_squircle(
                pixmap,
                w - 16.0,
                (h - 8.0) / 2.0,
                3.0,
                8.0,
                1.5,
                Color::from_rgba(1.0, 0.35, 0.45, 0.9).unwrap_or(Color::WHITE),
                0.2,
            );
        } else {
            // --- Expanded Media & Telemetry View ---
            // Close button (x)
            draw_mac_close_button(
                pixmap,
                w - 18.0,
                18.0,
                6.5,
                self.is_hovering_close,
            );

            // Album art squircle
            draw_glass_squircle(
                pixmap,
                22.0,
                22.0,
                64.0,
                64.0,
                12.0,
                Color::from_rgba(0.75, 0.18, 0.35, 0.95).unwrap_or(Color::WHITE),
                0.8,
            );

            // Track title & artist
            font.draw_text(
                pixmap,
                &self.media_bridge.state.title,
                98.0,
                42.0,
                16.0,
                Color::from_rgba(1.0, 1.0, 1.0, 0.98).unwrap_or(Color::WHITE),
            );

            font.draw_text(
                pixmap,
                &self.media_bridge.state.artist,
                98.0,
                64.0,
                12.0,
                Color::from_rgba(0.72, 0.75, 0.84, 0.88).unwrap_or(Color::WHITE),
            );

            // Live Telemetry badge (RAM & CPU)
            let telem_str = format!(
                "RAM: {:.1} MB  |  CPU: {:.0}%",
                self.current_ram_mb, self.current_cpu
            );
            font.draw_text(
                pixmap,
                &telem_str,
                98.0,
                82.0,
                11.0,
                Color::from_rgba(0.35, 0.88, 0.58, 0.95).unwrap_or(Color::WHITE),
            );

            // Progress bar
            let prog_w = w - 44.0;
            draw_glass_squircle(
                pixmap,
                22.0,
                100.0,
                prog_w,
                5.0,
                2.5,
                Color::from_rgba(0.18, 0.20, 0.26, 0.8).unwrap_or(Color::BLACK),
                0.0,
            );

            let pct = (self.media_bridge.state.position_secs
                / self.media_bridge.state.duration_secs.max(1.0))
            .clamp(0.0, 1.0);

            draw_glass_squircle(
                pixmap,
                22.0,
                100.0,
                (prog_w * pct).max(6.0),
                5.0,
                2.5,
                Color::from_rgba(1.0, 0.35, 0.45, 0.95).unwrap_or(Color::WHITE),
                0.5,
            );

            // Controls (Prev | Play/Pause | Next)
            let center_x = w / 2.0;
            let ctrl_y = 138.0;

            font.draw_text(
                pixmap,
                "|<<",
                center_x - 58.0,
                ctrl_y,
                14.0,
                Color::from_rgba(0.85, 0.88, 0.95, 0.9).unwrap_or(Color::WHITE),
            );

            let play_label = if self.media_bridge.state.is_playing {
                "||"
            } else {
                ">"
            };
            font.draw_text(
                pixmap,
                play_label,
                center_x - 6.0,
                ctrl_y,
                16.0,
                Color::from_rgba(1.0, 1.0, 1.0, 1.0).unwrap_or(Color::WHITE),
            );

            font.draw_text(
                pixmap,
                ">>|",
                center_x + 40.0,
                ctrl_y,
                14.0,
                Color::from_rgba(0.85, 0.88, 0.95, 0.9).unwrap_or(Color::WHITE),
            );
        }
    }
}
