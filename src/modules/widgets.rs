use std::f32::consts::PI;
use tiny_skia::{Color, PixmapMut};

use crate::config::ReminderItem;
use crate::render::canvas::draw_glass_squircle;
use crate::render::typography::FontRenderer;
use crate::win32::telemetry::{get_process_memory, SystemCpuSampler};

pub struct DesktopWidgets {
    pub cpu_sampler: SystemCpuSampler,
    pub reminders: Vec<ReminderItem>,
    pub weather_city: String,
    pub current_cpu: f32,
    pub current_ram_mb: f64,
}

impl DesktopWidgets {
    pub fn new(city: String, reminders: Vec<ReminderItem>) -> Self {
        Self {
            cpu_sampler: SystemCpuSampler::new(),
            reminders,
            weather_city: city,
            current_cpu: 12.0,
            current_ram_mb: 8.5,
        }
    }

    pub fn update(&mut self) {
        self.current_cpu = self.cpu_sampler.sample();
        self.current_ram_mb = get_process_memory().private_usage_mb();
    }

    pub fn toggle_reminder(&mut self, id: u64) {
        if let Some(item) = self.reminders.iter_mut().find(|r| r.id == id) {
            item.completed = !item.completed;
        }
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let glass_bg = Color::from_rgba(0.08, 0.08, 0.12, 0.72).unwrap_or(Color::BLACK);

        // --- Widget 1: Analog & Digital Clock (Top-Left) ---
        let clock_x = 24.0;
        let clock_y = 24.0;
        let clock_w = 260.0;
        let clock_h = 130.0;
        draw_glass_squircle(pixmap, clock_x, clock_y, clock_w, clock_h, 20.0, glass_bg, 1.0);

        // Draw analog clock face
        let face_cx = clock_x + 55.0;
        let face_cy = clock_y + 65.0;
        let face_r = 40.0;
        draw_glass_squircle(
            pixmap,
            face_cx - face_r,
            face_cy - face_r,
            face_r * 2.0,
            face_r * 2.0,
            face_r,
            Color::from_rgba(0.12, 0.14, 0.18, 0.8).unwrap_or(Color::BLACK),
            0.5,
        );

        // Simulated time or system time
        let hour = 10.0f32;
        let minute = 42.0f32;
        let second = 15.0f32;

        let hr_angle = (hour + minute / 60.0) * (PI / 6.0) - PI / 2.0;
        let min_angle = minute * (PI / 30.0) - PI / 2.0;
        let sec_angle = second * (PI / 30.0) - PI / 2.0;

        // Draw hour hand
        let hr_x = face_cx + hr_angle.cos() * 20.0;
        let hr_y = face_cy + hr_angle.sin() * 20.0;
        draw_glass_squircle(pixmap, hr_x - 1.5, hr_y - 1.5, 3.0, 3.0, 1.5, Color::WHITE, 0.5);

        // Draw minute hand
        let min_x = face_cx + min_angle.cos() * 30.0;
        let min_y = face_cy + min_angle.sin() * 30.0;
        draw_glass_squircle(pixmap, min_x - 1.0, min_y - 1.0, 2.0, 2.0, 1.0, Color::WHITE, 0.5);

        // Draw second dot
        let sec_x = face_cx + sec_angle.cos() * 34.0;
        let sec_y = face_cy + sec_angle.sin() * 34.0;
        draw_glass_squircle(
            pixmap,
            sec_x - 1.5,
            sec_y - 1.5,
            3.0,
            3.0,
            1.5,
            Color::from_rgba(1.0, 0.4, 0.3, 1.0).unwrap_or(Color::WHITE),
            0.5,
        );

        // Digital clock text
        font.draw_text(
            pixmap,
            "10:42",
            clock_x + 115.0,
            clock_y + 55.0,
            30.0,
            Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
        );
        font.draw_text(
            pixmap,
            "Friday, Nov 13",
            clock_x + 115.0,
            clock_y + 82.0,
            12.0,
            Color::from_rgba(0.7, 0.75, 0.85, 0.85).unwrap_or(Color::WHITE),
        );

        // --- Widget 2: Weather / AQI (Below Clock) ---
        let wthr_x = 24.0;
        let wthr_y = clock_y + clock_h + 16.0;
        let wthr_w = 260.0;
        let wthr_h = 110.0;
        draw_glass_squircle(pixmap, wthr_x, wthr_y, wthr_w, wthr_h, 20.0, glass_bg, 1.0);

        font.draw_text(
            pixmap,
            &self.weather_city,
            wthr_x + 20.0,
            wthr_y + 36.0,
            15.0,
            Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
        );
        font.draw_text(
            pixmap,
            "72° Partly Cloudy",
            wthr_x + 20.0,
            wthr_y + 60.0,
            13.0,
            Color::from_rgba(0.75, 0.8, 0.9, 0.85).unwrap_or(Color::WHITE),
        );

        // AQI pill
        draw_glass_squircle(
            pixmap,
            wthr_x + 20.0,
            wthr_y + 72.0,
            64.0,
            18.0,
            9.0,
            Color::from_rgba8(50, 165, 90, 204),
            0.5,
        );
        font.draw_text(
            pixmap,
            "AQI 24",
            wthr_x + 32.0,
            wthr_y + 85.0,
            10.0,
            Color::WHITE,
        );

        // --- Widget 3: Interactive Reminders (Right Side) ---
        let rem_x = 300.0;
        let rem_y = 24.0;
        let rem_w = 280.0;
        let rem_h = 256.0;
        draw_glass_squircle(pixmap, rem_x, rem_y, rem_w, rem_h, 20.0, glass_bg, 1.0);

        font.draw_text(
            pixmap,
            "Reminders",
            rem_x + 20.0,
            rem_y + 36.0,
            15.0,
            Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
        );

        for (i, reminder) in self.reminders.iter().enumerate().take(5) {
            let item_y = rem_y + 62.0 + (i as f32 * 36.0);

            // Checkbox
            let check_bg = if reminder.completed {
                Color::from_rgba8(75, 180, 255, 240)
            } else {
                Color::from_rgba8(50, 55, 70, 204)
            };
            draw_glass_squircle(pixmap, rem_x + 20.0, item_y - 12.0, 16.0, 16.0, 8.0, check_bg, 0.6);

            let text_color = if reminder.completed {
                Color::from_rgba(0.5, 0.55, 0.6, 0.7).unwrap_or(Color::WHITE)
            } else {
                Color::from_rgba(0.9, 0.92, 0.98, 0.9).unwrap_or(Color::WHITE)
            };

            font.draw_text(pixmap, &reminder.text, rem_x + 46.0, item_y, 11.5, text_color);
        }
    }
}
