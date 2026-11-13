use tiny_skia::{
    Color, FillRule, Paint, Path, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};

pub fn build_squircle_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    let r = radius.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();

    // macOS continuous curvature smooth squircle approximation using cubic Bézier curves
    let k = 0.552_284_8 * (r / radius); // Bézier circle constant adapted for squircle curvature

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
        // 1. Fill base frosted background
        let mut fill_paint = Paint::default();
        fill_paint.set_color(base_color);
        fill_paint.anti_alias = true;
        pixmap.fill_path(&path, &fill_paint, FillRule::Winding, Transform::identity(), None);

        // 2. Draw 1px specular highlight rim (macOS vitreous glass border)
        let mut stroke_paint = Paint::default();
        let border_alpha = (0.28 * specular_strength).clamp(0.0, 1.0);
        stroke_paint.set_color(Color::from_rgba(1.0, 1.0, 1.0, border_alpha).unwrap_or(Color::WHITE));
        stroke_paint.anti_alias = true;

        let stroke = Stroke {
            width: 1.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);

        // 3. Inner specular top-lip reflection
        if let Some(top_rim_rect) = Rect::from_xywh(x + radius * 0.5, y + 1.0, (w - radius).max(1.0), 1.0) {
            let mut top_lip_paint = Paint::default();
            top_lip_paint.set_color(
                Color::from_rgba(1.0, 1.0, 1.0, (0.4 * specular_strength).clamp(0.0, 1.0))
                    .unwrap_or(Color::WHITE),
            );
            pixmap.fill_rect(top_rim_rect, &top_lip_paint, Transform::identity(), None);
        }
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
        // softbuffer uses 0x00RRGGBB format
        buffer[i] = (r << 16) | (g << 8) | b;
    }
}

