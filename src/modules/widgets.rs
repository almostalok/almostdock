use rhai::{Array, Map};
use tiny_skia::{Color, PixmapMut};

use crate::render::canvas::{draw_glass_squircle, draw_mac_close_button};
use crate::render::typography::FontRenderer;
use crate::scripting::ScriptEngine;

#[derive(Clone, Debug)]
pub struct WidgetCard {
    pub card_type: String,
    pub title: String,
    pub subtitle: String,
    pub ram_text: String,
    pub cpu_text: String,
    pub ram_pct: f32,
    pub cpu_pct: f32,
    pub items: Vec<(String, String, String)>, // id, label, cmd
    pub tasks: Vec<(String, String, bool)>,    // id, text, done
}

pub struct DesktopWidgets {
    pub cards: Vec<WidgetCard>,
    pub mouse_pos: Option<(f32, f32)>,
    pub hovered_close_card: Option<usize>,
    pub width: f32,
    pub height: f32,
}

impl Default for DesktopWidgets {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            mouse_pos: None,
            hovered_close_card: None,
            width: 300.0,
            height: 620.0,
        }
    }
}

impl DesktopWidgets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_mouse_position(&mut self, pos: Option<(f32, f32)>) {
        self.mouse_pos = pos;
        self.hovered_close_card = None;

        if let Some((mx, my)) = pos {
            // Check which card's close button is hovered
            let mut card_y = 0.0;
            for (idx, _card) in self.cards.iter().enumerate() {
                let card_h = 135.0;
                let close_cx = self.width - 16.0;
                let close_cy = card_y + 16.0;

                let dist_sq = (mx - close_cx).powi(2) + (my - close_cy).powi(2);
                if dist_sq <= 100.0 {
                    self.hovered_close_card = Some(idx);
                    break;
                }
                card_y += card_h + 14.0;
            }
        }
    }

    pub fn update(&mut self, script_engine: &ScriptEngine) {
        let raw_widgets: Array = script_engine.call_widgets_tick();
        let mut cards = Vec::new();

        let get_str = |m: &Map, k: &str| -> String {
            m.get(k)
                .map(|v| v.clone().into_string().unwrap_or_else(|_| v.to_string()))
                .unwrap_or_default()
        };

        for dynamic_val in raw_widgets {
            if let Some(map) = dynamic_val.try_cast::<Map>() {
                let card_type = get_str(&map, "type");
                let title = get_str(&map, "title");
                let subtitle = get_str(&map, "subtitle");
                let ram_text = get_str(&map, "ram_text");
                let cpu_text = get_str(&map, "cpu_text");
                let ram_pct = map
                    .get("ram_pct")
                    .and_then(|v| {
                        v.as_float()
                            .ok()
                            .or_else(|| v.as_int().ok().map(|i| i as f64))
                    })
                    .unwrap_or(0.0) as f32;
                let cpu_pct = map
                    .get("cpu_pct")
                    .and_then(|v| {
                        v.as_float()
                            .ok()
                            .or_else(|| v.as_int().ok().map(|i| i as f64))
                    })
                    .unwrap_or(0.0) as f32;

                let mut items = Vec::new();
                if let Some(dyn_items) = map.get("items") {
                    if let Some(arr) = dyn_items.clone().try_cast::<Array>() {
                        for item_val in arr {
                            if let Some(item_map) = item_val.try_cast::<Map>() {
                                let id = get_str(&item_map, "id");
                                let label = get_str(&item_map, "label");
                                let cmd = get_str(&item_map, "cmd");
                                items.push((id, label, cmd));
                            }
                        }
                    }
                }

                let mut tasks = Vec::new();
                if let Some(dyn_tasks) = map.get("tasks") {
                    if let Some(arr) = dyn_tasks.clone().try_cast::<Array>() {
                        for task_val in arr {
                            if let Some(task_map) = task_val.try_cast::<Map>() {
                                let id = get_str(&task_map, "id");
                                let text = get_str(&task_map, "text");
                                let done = task_map.get("done").and_then(|v| v.as_bool().ok()).unwrap_or(false);
                                tasks.push((id, text, done));
                            }
                        }
                    }
                }

                cards.push(WidgetCard {
                    card_type,
                    title,
                    subtitle,
                    ram_text,
                    cpu_text,
                    ram_pct,
                    cpu_pct,
                    items,
                    tasks,
                });
            }
        }

        self.cards = cards;
    }

    /// Handles click on widgets container.
    /// Returns Some(index) if a card's close button was clicked, or None.
    pub fn handle_click(&mut self, script_engine: &ScriptEngine) -> Option<usize> {
        if let Some(close_idx) = self.hovered_close_card {
            if close_idx < self.cards.len() {
                self.cards.remove(close_idx);
                return Some(close_idx);
            }
        }

        if let Some((mx, my)) = self.mouse_pos {
            let mut card_y = 0.0;
            for card in &mut self.cards {
                let card_h = 135.0;
                if my >= card_y && my < (card_y + card_h) {
                    let rel_y = my - card_y;

                    if card.card_type == "quick_launch" {
                        // 3 horizontal launch buttons
                        let btn_w = 80.0;
                        let btn_y = 54.0;
                        if rel_y >= btn_y && rel_y <= (btn_y + 36.0) {
                            for (b_idx, (id, _, _)) in card.items.iter().enumerate() {
                                let bx = 16.0 + b_idx as f32 * (btn_w + 10.0);
                                if mx >= bx && mx <= (bx + btn_w) {
                                    script_engine.call_widget_click("quick_launch", id);
                                    break;
                                }
                            }
                        }
                    } else if card.card_type == "tasks" {
                        // Clickable checklist items
                        for (t_idx, (id, _, done)) in card.tasks.iter_mut().enumerate() {
                            let item_y = 44.0 + t_idx as f32 * 26.0;
                            if rel_y >= (item_y - 8.0) && rel_y <= (item_y + 16.0) {
                                *done = !*done;
                                script_engine.call_widget_click("tasks", id);
                                break;
                            }
                        }
                    }
                    break;
                }
                card_y += card_h + 14.0;
            }
        }

        None
    }

    pub fn render(&self, pixmap: &mut PixmapMut, font: &mut FontRenderer) {
        let card_w = self.width;
        let card_h = 135.0;
        let gap = 14.0;
        let mut card_y = 0.0;

        let glass_bg = Color::from_rgba(0.08, 0.09, 0.12, 0.68).unwrap_or(Color::BLACK);

        for (idx, card) in self.cards.iter().enumerate() {
            if card_y + card_h > self.height + 20.0 {
                break;
            }

            // Draw individual frosted glass card with 1px specular rim
            draw_glass_squircle(pixmap, 0.0, card_y, card_w, card_h, 18.0, glass_bg, 1.0);

            // Hover close button (x)
            let is_close_hovered = self.hovered_close_card == Some(idx);
            draw_mac_close_button(
                pixmap,
                card_w - 16.0,
                card_y + 16.0,
                6.0,
                is_close_hovered,
            );

            match card.card_type.as_str() {
                "clock" => {
                    font.draw_text(
                        pixmap,
                        &card.title,
                        18.0,
                        card_y + 54.0,
                        32.0,
                        Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
                    );
                    font.draw_text(
                        pixmap,
                        &card.subtitle,
                        18.0,
                        card_y + 82.0,
                        13.0,
                        Color::from_rgba(0.75, 0.78, 0.86, 0.85).unwrap_or(Color::WHITE),
                    );

                    // Status pill
                    draw_glass_squircle(
                        pixmap,
                        18.0,
                        card_y + 96.0,
                        68.0,
                        18.0,
                        9.0,
                        Color::from_rgba8(40, 175, 95, 204),
                        0.5,
                    );
                    font.draw_text(
                        pixmap,
                        "ONLINE",
                        32.0,
                        card_y + 109.0,
                        10.0,
                        Color::WHITE,
                    );
                }
                "vitals" => {
                    font.draw_text(
                        pixmap,
                        &card.title,
                        18.0,
                        card_y + 32.0,
                        14.0,
                        Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
                    );

                    // RAM info & progress
                    font.draw_text(
                        pixmap,
                        &card.ram_text,
                        18.0,
                        card_y + 60.0,
                        12.0,
                        Color::from_rgba(0.85, 0.88, 0.95, 0.9).unwrap_or(Color::WHITE),
                    );
                    draw_glass_squircle(
                        pixmap,
                        18.0,
                        card_y + 68.0,
                        card_w - 36.0,
                        6.0,
                        3.0,
                        Color::from_rgba(0.15, 0.18, 0.24, 0.8).unwrap_or(Color::BLACK),
                        0.0,
                    );
                    let ram_bar_w = ((card_w - 36.0) * (card.ram_pct / 100.0).clamp(0.05, 1.0)).max(6.0);
                    draw_glass_squircle(
                        pixmap,
                        18.0,
                        card_y + 68.0,
                        ram_bar_w,
                        6.0,
                        3.0,
                        Color::from_rgba8(65, 145, 255, 230),
                        0.6,
                    );

                    // CPU info & progress
                    font.draw_text(
                        pixmap,
                        &card.cpu_text,
                        18.0,
                        card_y + 96.0,
                        12.0,
                        Color::from_rgba(0.85, 0.88, 0.95, 0.9).unwrap_or(Color::WHITE),
                    );
                    draw_glass_squircle(
                        pixmap,
                        18.0,
                        card_y + 104.0,
                        card_w - 36.0,
                        6.0,
                        3.0,
                        Color::from_rgba(0.15, 0.18, 0.24, 0.8).unwrap_or(Color::BLACK),
                        0.0,
                    );
                    let cpu_bar_w = ((card_w - 36.0) * (card.cpu_pct / 100.0).clamp(0.05, 1.0)).max(6.0);
                    draw_glass_squircle(
                        pixmap,
                        18.0,
                        card_y + 104.0,
                        cpu_bar_w,
                        6.0,
                        3.0,
                        Color::from_rgba8(245, 115, 75, 230),
                        0.6,
                    );
                }
                "quick_launch" => {
                    font.draw_text(
                        pixmap,
                        &card.title,
                        18.0,
                        card_y + 32.0,
                        14.0,
                        Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
                    );

                    let btn_w = 80.0;
                    for (b_idx, (_, label, _)) in card.items.iter().enumerate() {
                        let bx = 16.0 + b_idx as f32 * (btn_w + 10.0);
                        let by = card_y + 54.0;
                        draw_glass_squircle(
                            pixmap,
                            bx,
                            by,
                            btn_w,
                            36.0,
                            10.0,
                            Color::from_rgba(0.18, 0.22, 0.30, 0.85).unwrap_or(Color::BLACK),
                            0.7,
                        );
                        let (lw, _) = font.measure_text(label, 11.0);
                        font.draw_text(
                            pixmap,
                            label,
                            bx + (btn_w - lw) / 2.0,
                            by + 22.0,
                            11.0,
                            Color::WHITE,
                        );
                    }
                }
                "tasks" => {
                    font.draw_text(
                        pixmap,
                        &card.title,
                        18.0,
                        card_y + 28.0,
                        14.0,
                        Color::from_rgba(1.0, 1.0, 1.0, 0.95).unwrap_or(Color::WHITE),
                    );

                    for (t_idx, (_, text, done)) in card.tasks.iter().enumerate().take(3) {
                        let ty = card_y + 52.0 + t_idx as f32 * 26.0;

                        // Checkbox squircle
                        let check_color = if *done {
                            Color::from_rgba8(65, 145, 255, 240)
                        } else {
                            Color::from_rgba(0.22, 0.25, 0.32, 0.8).unwrap_or(Color::BLACK)
                        };
                        draw_glass_squircle(pixmap, 18.0, ty - 10.0, 14.0, 14.0, 4.0, check_color, 0.6);

                        let text_color = if *done {
                            Color::from_rgba(0.5, 0.54, 0.62, 0.7).unwrap_or(Color::WHITE)
                        } else {
                            Color::from_rgba(0.88, 0.90, 0.96, 0.9).unwrap_or(Color::WHITE)
                        };
                        font.draw_text(pixmap, text, 40.0, ty + 1.0, 11.0, text_color);
                    }
                }
                _ => {
                    font.draw_text(
                        pixmap,
                        &card.title,
                        18.0,
                        card_y + 36.0,
                        14.0,
                        Color::WHITE,
                    );
                }
            }

            card_y += card_h + gap;
        }
    }
}
