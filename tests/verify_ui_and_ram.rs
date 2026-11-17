use oxide_shell::modules::wallpaper::{calculate_average_luminance, WallpaperGenerator};
use oxide_shell::scripting::ScriptEngine;
use oxide_shell::win32::layout::{rects_overlap, LayoutSolver, ScreenGeometry};
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

    let dock_width = 440;
    let layout = LayoutSolver::solve(&geometry, dock_width);

    // 1. Notch: Anchored top-center at y = 0
    assert_eq!(layout.notch_rect.top, 0);
    let notch_center_x = (layout.notch_rect.left + layout.notch_rect.right) / 2;
    assert_eq!(notch_center_x, 1920 / 2);
    assert_eq!(layout.notch_rect.width(), 220);
    assert_eq!(layout.notch_rect.height(), 34);

    // 2. Dock: Anchored bottom-center above taskbar / bottom edge
    let dock_center_x = (layout.dock_rect.left + layout.dock_rect.right) / 2;
    assert_eq!(dock_center_x, 1920 / 2);
    assert!(layout.dock_rect.bottom >= 1080 - 120);
    assert_eq!(layout.dock_rect.width(), dock_width);
    assert_eq!(layout.dock_rect.height(), 76);

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
fn test_wallpaper_non_black() {
    let gen = WallpaperGenerator::new(640, 360);
    let bmp_bytes = gen.generate_bmp_bytes();

    assert!(bmp_bytes.len() > 54);
    assert_eq!(&bmp_bytes[0..2], b"BM");

    let lum = calculate_average_luminance(&bmp_bytes);
    println!("Calculated Average Wallpaper Luminance: {:.2}", lum);

    // Assert wallpaper is never pitch black and within macOS matte studio range
    assert!(
        (12.0..=65.0).contains(&lum),
        "Luminance {:.2} not in acceptable range [12.0, 65.0]",
        lum
    );
}

#[test]
fn test_rhai_programmability() {
    let script_engine = ScriptEngine::new();

    // Verify widgets.rhai execution returns dynamic cards
    let widgets = script_engine.call_widgets_tick();
    assert!(
        !widgets.is_empty(),
        "widgets.rhai returned no dynamic widgets"
    );

    // Verify dock items evaluation
    let dock_items = script_engine.call_dock_items();
    assert!(
        !dock_items.is_empty(),
        "dock_actions.rhai returned no dock items"
    );

    // Verify dock click action handler execution
    script_engine.call_dock_click("terminal");
}

#[test]
fn test_idle_ram_gate() {
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
