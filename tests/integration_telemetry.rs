use oxide_shell::config::OxideConfig;
use oxide_shell::modules::dock::MagnifyingDock;
use oxide_shell::modules::launcher::AppLauncher;
use oxide_shell::modules::wallpaper::WallpaperGenerator;
use oxide_shell::render::canvas::{build_squircle_path, draw_glass_squircle};
use oxide_shell::render::spring::Spring;
use oxide_shell::render::typography::FontRenderer;
use oxide_shell::win32::shell_icons::IconCache;
use oxide_shell::win32::telemetry::{get_process_memory, trim_working_set};
use tiny_skia::{Color, Pixmap};

#[test]
fn test_memory_telemetry_and_trim() {
    let before = get_process_memory();
    assert!(before.private_usage_bytes > 0 || before.working_set_bytes > 0);

    trim_working_set();

    let after = get_process_memory();
    // Working set should be reported accurately
    assert!(after.working_set_bytes > 0);
    println!(
        "Memory Telemetry Test: Private Usage: {:.2} MB, Working Set: {:.2} MB",
        after.private_usage_mb(),
        after.working_set_mb()
    );
}

#[test]
fn test_spring_physics_convergence() {
    let mut spring = Spring::new(0.0, 280.0, 28.0);
    spring.set_target(100.0);

    let dt = 1.0 / 60.0;
    let mut frames = 0;
    while spring.update(dt) {
        frames += 1;
        assert!(frames < 500, "Spring failed to settle within 500 frames");
    }

    assert!(spring.is_settled());
    assert!((spring.value - 100.0).abs() < 0.01);
    assert!(spring.velocity.abs() < 0.01);
}

#[test]
fn test_squircle_canvas_rendering() {
    let mut pixmap = Pixmap::new(200, 200).expect("Failed to allocate test pixmap");
    pixmap.fill(Color::TRANSPARENT);

    let path = build_squircle_path(10.0, 10.0, 180.0, 180.0, 36.0);
    assert!(path.is_some(), "Squircle path generation failed");

    let mut mut_view = pixmap.as_mut();
    draw_glass_squircle(
        &mut mut_view,
        10.0,
        10.0,
        180.0,
        180.0,
        36.0,
        Color::from_rgba(0.1, 0.1, 0.15, 0.9).unwrap(),
        1.0,
    );

    // Verify non-empty pixels rendered
    let has_non_zero = pixmap.data().iter().any(|&b| b > 0);
    assert!(has_non_zero, "Expected rendered pixels in squircle");
}

#[test]
fn test_typography_metrics_and_draw() {
    let mut font = FontRenderer::new().expect("System font must be available");
    let (w, h) = font.measure_text("OxideShell", 16.0);
    assert!(w > 0.0);
    assert!(h > 0.0);

    let mut pixmap = Pixmap::new(200, 50).expect("Failed to allocate test pixmap");
    pixmap.fill(Color::TRANSPARENT);

    let mut mut_view = pixmap.as_mut();
    font.draw_text(&mut mut_view, "OxideShell", 10.0, 30.0, 16.0, Color::WHITE);

    let has_pixels = pixmap.data().iter().any(|&b| b > 0);
    assert!(has_pixels, "Text rendering produced no pixels");
}

#[test]
fn test_wallpaper_procedural_generation() {
    let gen = WallpaperGenerator::new(640, 360);
    let bytes = gen.generate_bmp_bytes();

    // BMP magic "BM"
    assert_eq!(&bytes[0..2], b"BM");

    // BMP minimum header size is 54 bytes
    assert!(bytes.len() > 54);

    let expected_row_padding = (4 - ((640 * 3) % 4)) % 4;
    let expected_row_size = 640 * 3 + expected_row_padding;
    let expected_file_size = 54 + expected_row_size * 360;
    assert_eq!(bytes.len(), expected_file_size);
}

#[test]
fn test_config_roundtrip() {
    let config = OxideConfig::default();
    let serialized = serde_json::to_string_pretty(&config).expect("Serialization failed");
    let deserialized: OxideConfig =
        serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(config.dock_base_size, deserialized.dock_base_size);
    assert_eq!(config.pinned_apps.len(), deserialized.pinned_apps.len());
    assert_eq!(config.reminders.len(), deserialized.reminders.len());
}

#[test]
fn test_dock_magnification_physics() {
    let config = OxideConfig::default();
    let icon_cache = IconCache::new();
    let mut dock = MagnifyingDock::new(&config.pinned_apps, &icon_cache);

    // Initial state: not hovered
    dock.set_mouse_position(None);
    dock.update(1.0 / 60.0);
    for item in &dock.items {
        assert_eq!(item.scale_spring.target, 1.0);
    }

    // Hover over the center of the first item
    let first_item_center = 12.0 + (dock.base_size * 1.0) / 2.0;
    dock.set_mouse_position(Some(first_item_center));
    dock.update(1.0 / 60.0);

    // The hovered item target scale should equal max_scale
    assert!((dock.items[0].scale_spring.target - dock.max_scale).abs() < 0.01);
}

#[test]
fn test_launcher_surface_destruction() {
    let mut launcher = AppLauncher::new();
    assert!(!launcher.is_open);
    assert!(launcher.surface.is_none());

    // Open allocates surface
    launcher.open();
    assert!(launcher.is_open);
    assert!(launcher.surface.is_some());

    // Close destroys surface completely and trims working set
    launcher.close();
    assert!(!launcher.is_open);
    assert!(launcher.surface.is_none());
}

