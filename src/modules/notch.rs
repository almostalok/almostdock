use tiny_skia::{Color, PixmapMut};

use crate::render::canvas::draw_glass_squircle;
use crate::render::spring::Spring;
use crate::render::typography::FontRenderer;
use crate::win32::media::MediaBridge;
use crate::win32::telemetry::{get_process_memory, SystemCpuSampler};

pub enum NotchMode {
    Compact,
    Media,
    Telemetry,
}

pub struct DynamicNotch {
    pub width_spring: Spring,
    pub height_spring: Spring,
    pub mode: NotchMode,
    pub is_hovered: bool,
    pub media_bridge: MediaBridge,
    pub cpu_sampler: SystemCpuSampler,
    pub current_cpu: f32,
    pub current_ram_mb: f64,
}

impl Default for DynamicNotch {
    fn default() -> Self {
        Self {
            width_spring: Spring::new(180.0, 260.0, 26.0),
            height_spring: Spring::new(32.0, 260.0, 26.0),
            mode: NotchMode::Compact,
            is_hovered: false,
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
            self.width_spring.set_target(380.0);
            self.height_spring.set_target(130.0);
            self.mode = NotchMode::Media;
        } else {
            self.width_spring.set_target(180.0);
            self.height_spring.set_target(32.0);
            self.mode = NotchMode::Compact;
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

    pub fn handle_click(&mut self, rel_x: f32, rel_y: f32) {
        if !self.is_hovered {
            self.set_hovered(true);
            return;
        }

        // Expanded media controls click detection
        let h = self.height_spring.value;
        if rel_y > (h - 45.0) {
            let center_x = self.width_spring.value / 2.0;
            if (rel_x - (center_x - 45.0)).abs() < 18.0 {
                self.media_bridge.prev_track();
            } else if (rel_x - center_x).abs() < 20.0 {
                self.media_bridge.play_pause();
            } else if (rel_x - (center_x + 45.0)).abs() < 18.0 {
                self.media_bridge.next_track();
            }
        }
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let w = self.width_spring.value;
        let h = self.height_spring.value;
        let r = 18.0f32.min(h / 2.0);

        // Draw translucent dark glass background
        let bg_color = Color::from_rgba(0.06, 0.06, 0.08, 0.88).unwrap_or(Color::BLACK);
        draw_glass_squircle(pixmap, 0.0, 0.0, w, h, r, bg_color, 1.0);

        if h < 50.0 {
            // --- Compact Idle Pill ---
            // Draw subtle green status dot
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

            // Clock / brief label
            font.draw_text(
                pixmap,
                "Dynamic Island",
                32.0,
                h / 2.0 + 4.0,
                11.0,
                Color::from_rgba(0.9, 0.9, 0.95, 0.85).unwrap_or(Color::WHITE),
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
            // Album art squircle
            draw_glass_squircle(
                pixmap,
                18.0,
                18.0,
                52.0,
                52.0,
                10.0,
                Color::from_rgba(0.75, 0.18, 0.35, 0.95).unwrap_or(Color::WHITE),
                0.8,
            );

            // Track title & artist
            font.draw_text(
                pixmap,
                &self.media_bridge.state.title,
                80.0,
                34.0,
                14.0,
                Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
            );

            font.draw_text(
                pixmap,
                &self.media_bridge.state.artist,
                80.0,
                52.0,
                11.0,
                Color::from_rgba(0.7, 0.72, 0.78, 0.85).unwrap_or(Color::WHITE),
            );

            // Live Telemetry badge (RAM & CPU)
            let telem_str = format!(
                "RAM: {:.1} MB  |  CPU: {:.0}%",
                self.current_ram_mb, self.current_cpu
            );
            font.draw_text(
                pixmap,
                &telem_str,
                80.0,
                68.0,
                10.0,
                Color::from_rgba(0.35, 0.85, 0.55, 0.9).unwrap_or(Color::WHITE),
            );

            // Progress bar
            let prog_w = w - 36.0;
            draw_glass_squircle(
                pixmap,
                18.0,
                80.0,
                prog_w,
                4.0,
                2.0,
                Color::from_rgba(0.2, 0.22, 0.26, 0.8).unwrap_or(Color::BLACK),
                0.0,
            );

            let pct = (self.media_bridge.state.position_secs
                / self.media_bridge.state.duration_secs.max(1.0))
            .clamp(0.0, 1.0);

            draw_glass_squircle(
                pixmap,
                18.0,
                80.0,
                (prog_w * pct).max(4.0),
                4.0,
                2.0,
                Color::from_rgba(1.0, 0.35, 0.45, 0.95).unwrap_or(Color::WHITE),
                0.5,
            );

            // Controls (Prev | Play/Pause | Next)
            let center_x = w / 2.0;
            let ctrl_y = 112.0;

            font.draw_text(
                pixmap,
                "|<<",
                center_x - 52.0,
                ctrl_y,
                13.0,
                Color::from_rgba(0.8, 0.8, 0.85, 0.9).unwrap_or(Color::WHITE),
            );

            let play_label = if self.media_bridge.state.is_playing {
                "||"
            } else {
                ">"
            };
            font.draw_text(
                pixmap,
                play_label,
                center_x - 5.0,
                ctrl_y,
                14.0,
                Color::from_rgba(1.0, 1.0, 1.0, 1.0).unwrap_or(Color::WHITE),
            );

            font.draw_text(
                pixmap,
                ">>|",
                center_x + 36.0,
                ctrl_y,
                13.0,
                Color::from_rgba(0.8, 0.8, 0.85, 0.9).unwrap_or(Color::WHITE),
            );
        }
    }
}

