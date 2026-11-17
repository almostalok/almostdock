use tiny_skia::{
    Color, FillRule, Paint, Path, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

pub fn build_squircle_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    let r = radius.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();

    // macOS continuous curvature smooth squircle approximation using cubic Bézier curves
    let k = 0.552_284_8 * (r / radius.max(1.0));

    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k * r, y, x + w, y + r - k * r, x + w, y + r);

    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k * r, x + w - r + k * r, y + h, x + w - r, y + h);

    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k * r, y + h, x, y + h - r + k * r, x, y + h - r);

    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k * r, x + r - k * r, y, x + r, y);

    pb.close();
    pb.finish()
}

#[allow(clippy::too_many_arguments)]
pub fn draw_glass_squircle(
    pixmap: &mut PixmapMut,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    base_color: Color,
    specular_strength: f32,
) {
    if let Some(path) = build_squircle_path(x, y, w, h, radius) {
        // 1. Fill base frosted background (e.g. rgba(22, 25, 32, 0.68))
        let mut fill_paint = Paint::default();
        fill_paint.set_color(base_color);
        fill_paint.anti_alias = true;
        pixmap.fill_path(&path, &fill_paint, FillRule::Winding, Transform::identity(), None);

        // 2. Draw 1px specular highlight rim (macOS vitreous glass border)
        let mut stroke_paint = Paint::default();
        let border_alpha = (0.25 * specular_strength).clamp(0.0, 1.0);
        stroke_paint.set_color(Color::from_rgba(1.0, 1.0, 1.0, border_alpha).unwrap_or(Color::WHITE));
        stroke_paint.anti_alias = true;

        let stroke = Stroke {
            width: 1.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);

        // 3. Top-lit specular reflection gradient line
        if let Some(top_rim_rect) = Rect::from_xywh(x + radius * 0.5, y + 1.0, (w - radius).max(1.0), 1.0) {
            let mut top_lip_paint = Paint::default();
            top_lip_paint.set_color(
                Color::from_rgba(1.0, 1.0, 1.0, (0.35 * specular_strength).clamp(0.0, 1.0))
                    .unwrap_or(Color::WHITE),
            );
            pixmap.fill_rect(top_rim_rect, &top_lip_paint, Transform::identity(), None);
        }
    }
}

/// Renders a circular macOS-style close button (x) with hover highlight.
pub fn draw_mac_close_button(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    radius: f32,
    is_hovered: bool,
) {
    let circle_color = if is_hovered {
        Color::from_rgba(1.0, 0.36, 0.33, 0.95).unwrap_or(Color::WHITE)
    } else {
        Color::from_rgba(0.45, 0.48, 0.55, 0.55).unwrap_or(Color::WHITE)
    };

    draw_glass_squircle(
        pixmap,
        cx - radius,
        cy - radius,
        radius * 2.0,
        radius * 2.0,
        radius,
        circle_color,
        if is_hovered { 1.2 } else { 0.4 },
    );

    // Draw the subtle 'x' cross
    let cross_size = radius * 0.42;
    let mut stroke_paint = Paint::default();
    let cross_alpha = if is_hovered { 0.95 } else { 0.7 };
    stroke_paint.set_color(Color::from_rgba(1.0, 1.0, 1.0, cross_alpha).unwrap_or(Color::WHITE));
    stroke_paint.anti_alias = true;
    let stroke = Stroke {
        width: 1.2,
        ..Default::default()
    };

    let mut pb = PathBuilder::new();
    pb.move_to(cx - cross_size, cy - cross_size);
    pb.line_to(cx + cross_size, cy + cross_size);
    pb.move_to(cx + cross_size, cy - cross_size);
    pb.line_to(cx - cross_size, cy + cross_size);
    if let Some(p) = pb.finish() {
        pixmap.stroke_path(&p, &stroke_paint, &stroke, Transform::identity(), None);
    }
}

pub fn copy_pixmap_to_softbuffer(pixmap: &tiny_skia::Pixmap, buffer: &mut [u32]) {
    let src = pixmap.data();
    let count = (pixmap.width() * pixmap.height()) as usize;
    let limit = count.min(buffer.len());

    for i in 0..limit {
        let r = src[i * 4] as u32;
        let g = src[i * 4 + 1] as u32;
        let b = src[i * 4 + 2] as u32;
        let a = src[i * 4 + 3] as u32;

        // softbuffer with DWM acrylic expects 0xAARRGGBB format
        buffer[i] = (a << 24) | (r << 16) | (g << 8) | b;
    }
}
