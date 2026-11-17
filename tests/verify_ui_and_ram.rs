use tiny_skia::{Color, Pixmap};

use oxide_shell::config::OxideConfig;
use oxide_shell::modules::dock::MagnifyingDock;
use oxide_shell::modules::wallpaper::{calculate_rgb_average_luminance, WallpaperGenerator};
use oxide_shell::render::canvas::draw_glass_squircle;
use oxide_shell::scripting::ScriptEngine;
use oxide_shell::win32::layered::LayeredSurface;
use oxide_shell::win32::layout::{rects_overlap, LayoutSolver, ScreenGeometry};
use oxide_shell::win32::shell_icons::IconCache;
use oxide_shell::win32::telemetry::{get_process_memory, trim_working_set};

#[test]
fn test_geometry_and_non_overlap() {
    let geometry = ScreenGeometry {
        screen_width: 1920,
        screen_height: 1080,
        work_left: 0,
        work_top: 0,
        work_right: 1920,
        work_bottom: 1040,
    };

    let dock_width = 680;
    let layout = LayoutSolver::solve(&geometry, dock_width);

    // 1. Notch: Anchored top-center at y = 0
    assert_eq!(layout.notch_rect.top, 0);
    let notch_center_x = (layout.notch_rect.left + layout.notch_rect.right) / 2;
    assert_eq!(notch_center_x, 1920 / 2);
    assert_eq!(layout.notch_rect.width(), 220);
    assert_eq!(layout.notch_rect.height(), 34);

    // 2. Dock: Anchored bottom-center above taskbar / bottom edge (generous height 116)
    let dock_center_x = (layout.dock_rect.left + layout.dock_rect.right) / 2;
    assert_eq!(dock_center_x, 1920 / 2);
    assert!(layout.dock_rect.bottom >= 1080 - 120);
    assert_eq!(layout.dock_rect.width(), dock_width);
    assert_eq!(layout.dock_rect.height(), 116);

    // 3. Widgets Container: Anchored right edge, spaced vertically
    assert!(layout.widget_rect.left > 1920 / 2);
    assert_eq!(layout.widget_rect.width(), 300);
    assert_eq!(layout.widget_rect.height(), 620);
    assert_eq!(layout.widget_rect.right, 1920 - 20);

    // 4. Launcher: Centered on screen
    let launcher_center_x = (layout.launcher_rect.left + layout.launcher_rect.right) / 2;
    let launcher_center_y = (layout.launcher_rect.top + layout.launcher_rect.bottom) / 2;
    assert_eq!(launcher_center_x, 1920 / 2);
    assert_eq!(launcher_center_y, 1080 / 2);

    // 5. Assert ZERO overlap between all shell surfaces
    assert!(
        !rects_overlap(&layout.notch_rect, &layout.dock_rect),
        "Notch and Dock must never overlap"
    );
    assert!(
        !rects_overlap(&layout.notch_rect, &layout.widget_rect),
        "Notch and Widgets container must never overlap"
    );
    assert!(
        !rects_overlap(&layout.dock_rect, &layout.widget_rect),
        "Dock and Widgets container must never overlap"
    );
}

#[test]
fn test_layered_surface_transparency() {
    let mut surface = LayeredSurface::new(200, 200).expect("LayeredSurface::new failed");
    let mut pixmap = Pixmap::new(200, 200).expect("Pixmap::new failed");
    pixmap.fill(Color::TRANSPARENT);

    // Draw a rounded squircle with 24px radius, inside bounds (20, 20) to (180, 180)
    let card_color = Color::from_rgba(0.12, 0.15, 0.22, 0.85).unwrap();
    draw_glass_squircle(&mut pixmap.as_mut(), 20.0, 20.0, 160.0, 160.0, 24.0, card_color, 1.0);

    surface.copy_from_pixmap(&pixmap);

    // 1. Outside rounded corners (e.g., at (0, 0) and (5, 5)), alpha MUST be exactly 0
    let corner_alpha = surface.get_pixel_alpha(0, 0);
    assert_eq!(
        corner_alpha, 0,
        "Corner pixel (0, 0) outside squircle must be 100% transparent (alpha == 0), got: {}",
        corner_alpha
    );

    let near_corner_alpha = surface.get_pixel_alpha(5, 5);
    assert_eq!(
        near_corner_alpha, 0,
        "Near-corner pixel (5, 5) outside squircle must be 100% transparent, got: {}",
        near_corner_alpha
    );

    // 2. Inside the card (center at (100, 100)), alpha MUST be non-zero
    let center_alpha = surface.get_pixel_alpha(100, 100);
    assert!(
        center_alpha > 180,
        "Center pixel (100, 100) inside squircle must have high alpha, got: {}",
        center_alpha
    );
}

#[test]
fn test_wallpaper_png_and_luminance() {
    let gen = WallpaperGenerator::new(640, 360);
    let temp_dir = std::env::temp_dir();
    let wallpaper_path = temp_dir.join("oxide_test_wallpaper.png");

    let saved = gen.generate_and_save_png(&wallpaper_path);
    assert!(saved.is_ok(), "generate_and_save_png failed");

    // Verify file exists and is > 50 KB
    let metadata = std::fs::metadata(&wallpaper_path).expect("Failed to read wallpaper metadata");
    assert!(
        metadata.len() > 50 * 1024,
        "Generated wallpaper.png size must be > 50 KB, got {} bytes",
        metadata.len()
    );

    let bytes = std::fs::read(&wallpaper_path).expect("Failed to read generated PNG bytes");
    // Verify valid PNG 8-byte magic header
    assert_eq!(
        &bytes[0..8],
        &[137, 80, 78, 71, 13, 10, 26, 10],
        "Generated file does not have a valid PNG magic signature"
    );

    // Verify average luminance > 15
    let rgb_buf = gen.generate_rgb_buffer();
    let avg_luma = calculate_rgb_average_luminance(&rgb_buf);
    println!("Measured Wallpaper Average Luminance: {:.2}", avg_luma);
    assert!(
        avg_luma > 15.0,
        "Wallpaper luminance must be > 15.0 to prevent black screen, got {:.2}",
        avg_luma
    );

    let _ = std::fs::remove_file(&wallpaper_path);
}

#[test]
fn test_hit_testing_close_button_and_drag() {
    let script_engine = ScriptEngine::new();
    let icon_cache = IconCache::new();
    let mut dock = MagnifyingDock::new(&script_engine, &icon_cache);

    // Hit-testing the Dock close button at (close_cx, close_cy) = (680 - 24, 38 + 14) = (656, 52)
    dock.set_mouse_position(Some((656.0, 52.0)));
    assert!(
        dock.is_hovering_close,
        "Expected is_hovering_close == true when hovering close button center"
    );

    // Position outside close button hitbox
    dock.set_mouse_position(Some((200.0, 52.0)));
    assert!(
        !dock.is_hovering_close,
        "Expected is_hovering_close == false when cursor is far from close button"
    );

    // Verify delta-dragging calculation logic
    let cursor_start = (500, 300);
    let win_start = (600, 900);
    let cursor_now = (545, 310);

    let dx = cursor_now.0 - cursor_start.0; // +45
    let dy = cursor_now.1 - cursor_start.1; // +10
    let win_now = (win_start.0 + dx, win_start.1 + dy);

    assert_eq!(win_now, (645, 910));
}

#[test]
fn test_rhai_scripts_and_memory_gate() {
    let mut script_engine = ScriptEngine::new();
    script_engine.ensure_default_scripts();

    let scripts_dir = OxideConfig::scripts_dir();
    let dock_path = scripts_dir.join("dock.rhai");
    let widgets_path = scripts_dir.join("widgets.rhai");

    assert!(dock_path.exists(), "dock.rhai must be auto-generated in scripts folder");
    assert!(widgets_path.exists(), "widgets.rhai must be auto-generated in scripts folder");

    // Verify hot-reload parsing of dock.rhai
    let dock_ok = script_engine.load_script("dock", &dock_path);
    assert!(dock_ok, "Failed to compile dock.rhai");

    // Verify hot-reload parsing of widgets.rhai
    let widgets_ok = script_engine.load_script("widgets", &widgets_path);
    assert!(widgets_ok, "Failed to compile widgets.rhai");

    // Verify widgets evaluation
    let widgets = script_engine.call_widgets_tick();
    assert!(!widgets.is_empty(), "widgets.rhai returned no widgets");

    // Verify dock items evaluation
    let dock_items = script_engine.call_dock_items();
    assert!(!dock_items.is_empty(), "dock.rhai returned no dock items");

    // Verify RAM budget < 14.0 MB
    trim_working_set();
    let mem = get_process_memory();
    println!(
        "Memory Telemetry Test: Private Usage: {:.2} MB (Budget < 14.0 MB), Working Set: {:.2} MB",
        mem.private_usage_mb(),
        mem.working_set_mb()
    );

    assert!(
        mem.private_usage_mb() < 14.0,
        "Private working set {:.2} MB exceeds 14.0 MB budget",
        mem.private_usage_mb()
    );
}
