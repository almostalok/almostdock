use std::process::Command;
use tiny_skia::{Color, Pixmap, PixmapMut};

use crate::render::canvas::draw_glass_squircle;
use crate::render::typography::FontRenderer;
use crate::win32::shell_icons::{scan_start_menu_apps, AppInfo};
use crate::win32::telemetry::trim_working_set;

pub struct AppLauncher {
    pub is_open: bool,
    pub search_query: String,
    pub all_apps: Vec<AppInfo>,
    pub filtered_indices: Vec<usize>,
    pub selected_index: usize,
    pub width: u32,
    pub height: u32,
    pub surface: Option<Pixmap>,
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self {
            is_open: false,
            search_query: String::new(),
            all_apps: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            width: 680,
            height: 480,
            surface: None,
        }
    }
}

impl AppLauncher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the launcher, allocating the render surface and scanning/indexing apps.
    pub fn open(&mut self) {
        if self.is_open {
            return;
        }

        self.is_open = true;
        self.search_query.clear();
        self.selected_index = 0;

        if self.all_apps.is_empty() {
            self.all_apps = scan_start_menu_apps();
        }

        self.refilter();

        // Allocate surface only while active
        self.surface = Pixmap::new(self.width, self.height);
    }

    /// Closes the launcher and completely drops the surface buffer and caches,
    /// triggering Win32 working set trimming to drop memory back under 12 MB.
    pub fn close(&mut self) {
        if !self.is_open {
            return;
        }

        self.is_open = false;
        self.search_query.clear();
        self.filtered_indices.clear();

        // Zero-RAM destruction: deallocate surface pixmap
        self.surface = None;

        // Immediately trim working set pages
        trim_working_set();
    }

    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn on_char_input(&mut self, c: char) {
        if !self.is_open {
            return;
        }
        if !c.is_control() {
            self.search_query.push(c);
            self.refilter();
        }
    }

    pub fn on_backspace(&mut self) {
        if !self.is_open {
            return;
        }
        self.search_query.pop();
        self.refilter();
    }

    pub fn select_next(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.filtered_indices.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.filtered_indices.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.filtered_indices.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn launch_selected(&mut self) {
        if let Some(&app_idx) = self.filtered_indices.get(self.selected_index) {
            if let Some(app) = self.all_apps.get(app_idx) {
                let _ = Command::new("cmd")
                    .args(["/c", "start", "", &app.path.to_string_lossy()])
                    .spawn();
            }
        }
        self.close();
    }

    fn refilter(&mut self) {
        let query = self.search_query.trim().to_lowercase();
        self.filtered_indices.clear();

        for (i, app) in self.all_apps.iter().enumerate() {
            if query.is_empty() || app.name.to_lowercase().contains(&query) {
                self.filtered_indices.push(i);
            }
        }

        self.selected_index = 0;
    }

    pub fn render(&mut self, font: &mut FontRenderer) -> Option<&Pixmap> {
        if !self.is_open {
            return None;
        }

        let mut surface = self.surface.take()?;
        surface.fill(Color::TRANSPARENT);

        self.render_contents(&mut surface.as_mut(), font);

        self.surface = Some(surface);
        self.surface.as_ref()
    }

    fn render_contents(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let w = self.width as f32;
        let h = self.height as f32;

        // Frosted glass modal background
        let modal_bg = Color::from_rgba(0.08, 0.08, 0.12, 0.88).unwrap_or(Color::BLACK);
        draw_glass_squircle(pixmap, 0.0, 0.0, w, h, 24.0, modal_bg, 1.2);

        // Spotlight search bar
        let bar_x = 24.0;
        let bar_y = 24.0;
        let bar_w = w - 48.0;
        let bar_h = 44.0;
        let bar_bg = Color::from_rgba(0.14, 0.16, 0.22, 0.9).unwrap_or(Color::BLACK);
        draw_glass_squircle(pixmap, bar_x, bar_y, bar_w, bar_h, 14.0, bar_bg, 0.8);

        // Search text / placeholder
        if self.search_query.is_empty() {
            font.draw_text(
                pixmap,
                "Type to search applications...",
                bar_x + 18.0,
                bar_y + 27.0,
                14.0,
                Color::from_rgba(0.5, 0.55, 0.65, 0.8).unwrap_or(Color::WHITE),
            );
        } else {
            let cursor_query = format!("{} |", self.search_query);
            font.draw_text(
                pixmap,
                &cursor_query,
                bar_x + 18.0,
                bar_y + 27.0,
                15.0,
                Color::from_rgba(1.0, 1.0, 1.0, 0.98).unwrap_or(Color::WHITE),
            );
        }

        // 4x3 Launchpad Squircle Grid
        let grid_start_x = 36.0;
        let grid_start_y = 96.0;
        let cols = 4;
        let cell_w = (w - 72.0) / cols as f32;
        let cell_h = 100.0;
        let icon_size = 52.0;

        for (grid_pos, &app_idx) in self.filtered_indices.iter().take(12).enumerate() {
            let col = grid_pos % cols;
            let row = grid_pos / cols;

            let cx = grid_start_x + (col as f32 * cell_w) + (cell_w - icon_size) / 2.0;
            let cy = grid_start_y + (row as f32 * cell_h);

            let is_selected = grid_pos == self.selected_index;

            // Highlight ring for selected app
            if is_selected {
                draw_glass_squircle(
                    pixmap,
                    cx - 6.0,
                    cy - 6.0,
                    icon_size + 12.0,
                    icon_size + 12.0,
                    18.0,
                    Color::from_rgba8(50, 128, 255, 115),
                    1.0,
                );
            }

            // App Squircle
            draw_glass_squircle(
                pixmap,
                cx,
                cy,
                icon_size,
                icon_size,
                14.0,
                Color::from_rgba(0.22, 0.25, 0.32, 0.95).unwrap_or(Color::WHITE),
                0.8,
            );

            if let Some(app) = self.all_apps.get(app_idx) {
                // App glyph initial
                let letter = app.name.chars().next().unwrap_or('A').to_string();
                font.draw_text(
                    pixmap,
                    &letter,
                    cx + icon_size * 0.32,
                    cy + icon_size * 0.68,
                    icon_size * 0.5,
                    Color::WHITE,
                );

                // App label below icon
                let label_text: String = if app.name.len() > 14 {
                    format!("{}...", &app.name[..12])
                } else {
                    app.name.clone()
                };

                let (label_w, _) = font.measure_text(&label_text, 11.0);
                let label_x = (cx + icon_size / 2.0) - (label_w / 2.0);
                font.draw_text(
                    pixmap,
                    &label_text,
                    label_x,
                    cy + icon_size + 20.0,
                    11.0,
                    Color::from_rgba(0.9, 0.92, 0.96, 0.9).unwrap_or(Color::WHITE),
                );
            }
        }
    }
}
