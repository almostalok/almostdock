use std::num::NonZeroU32;
use std::os::windows::ffi::OsStrExt;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use softbuffer::{Context, Surface};
use tiny_skia::{Color, Pixmap};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, POINT};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_NOREPEAT, VK_SPACE};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Event, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowBuilder};

pub mod config;
pub mod error;
pub mod modules;
pub mod render;
pub mod scripting;
pub mod win32;

use config::OxideConfig;
use modules::dock::MagnifyingDock;
use modules::launcher::AppLauncher;
use modules::notch::DynamicNotch;
use modules::wallpaper::WallpaperGenerator;
use modules::widgets::DesktopWidgets;
use render::canvas::copy_pixmap_to_softbuffer;
use render::typography::FontRenderer;
use scripting::ScriptEngine;
use win32::dwm_glass::{
    apply_acrylic_glass_and_strip_chrome, set_window_bounds, show_native_context_menu,
    start_window_drag, ContextMenuChoice,
};
use win32::layout::{LayoutSolver, ScreenGeometry};
use win32::shell_icons::IconCache;
use win32::telemetry::{get_process_memory, trim_working_set};

const SINGLE_INSTANCE_MUTEX: &str = "OxideShell_SingleInstance_Mutex_Global";
const HOTKEY_LAUNCHER_ID: i32 = 0x5348; // "SH"

fn get_hwnd(window: &Window) -> HWND {
    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            return win32_handle.hwnd.get() as HWND;
        }
    }
    0 as HWND
}

fn get_cursor_screen_pos() -> (i32, i32) {
    let mut pt: POINT = unsafe { std::mem::zeroed() };
    unsafe { GetCursorPos(&mut pt) };
    (pt.x, pt.y)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Single-Instance Named Mutex
    let mutex_name: Vec<u16> = std::ffi::OsStr::new(SINGLE_INSTANCE_MUTEX)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mutex = unsafe { CreateMutexW(std::ptr::null(), 1, mutex_name.as_ptr()) };
    if mutex == 0 || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        eprintln!("OxideShell is already running on this desktop.");
        return Ok(());
    }

    // 2. Load Configuration & Initialize Scripting Engine
    let mut config = OxideConfig::load();
    let mut script_engine = ScriptEngine::new();

    // 3. Register Global Hotkey (Alt + Space for Spotlight Launcher)
    unsafe {
        RegisterHotKey(0, HOTKEY_LAUNCHER_ID, MOD_ALT | MOD_NOREPEAT, VK_SPACE as u32);
    }

    // 4. Initialize Procedural Matte Wallpaper
    let wallpaper_gen = WallpaperGenerator::new(1920, 1080);
    let _ = wallpaper_gen.generate_and_apply();

    // 5. Initialize Shared Font Renderer & Icon Cache
    let mut font_renderer = FontRenderer::new().unwrap_or_else(|_| {
        eprintln!("Warning: Using fallback font loader.");
        FontRenderer::new().expect("Fallback font failed")
    });
    let icon_cache = IconCache::new();

    // 6. Query Monitor Geometry & Calculate Initial Non-Overlapping Layout
    let geometry = ScreenGeometry::query_primary();
    let initial_layout = LayoutSolver::solve(&geometry, 440);

    // 7. Create Winit Event Loop
    let event_loop = EventLoop::new()?;

    // 8. Create Bottom Dock Window
    let dock_x = config.positions.dock_x.unwrap_or(initial_layout.dock_rect.left);
    let dock_y = config.positions.dock_y.unwrap_or(initial_layout.dock_rect.top);
    let dock_w = initial_layout.dock_rect.width() as f64;
    let dock_h = initial_layout.dock_rect.height() as f64;

    let dock_window = WindowBuilder::new()
        .with_title("OxideShell Dock")
        .with_inner_size(LogicalSize::new(dock_w, dock_h))
        .with_position(LogicalPosition::new(dock_x, dock_y))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(config.shell.dock_enabled)
        .build(&event_loop)?;

    let dock_window = Arc::new(dock_window);
    let dock_hwnd = get_hwnd(&dock_window);
    apply_acrylic_glass_and_strip_chrome(dock_hwnd, true);
    set_window_bounds(dock_hwnd, dock_x, dock_y, dock_w as i32, dock_h as i32);

    // 9. Create Top Notch Window (Anchored at top-center)
    let notch_x = config.positions.notch_x.unwrap_or(initial_layout.notch_rect.left);
    let notch_y = config.positions.notch_y.unwrap_or(0);
    let notch_w = initial_layout.notch_rect.width() as f64;
    let notch_h = initial_layout.notch_rect.height() as f64;

    let notch_window = WindowBuilder::new()
        .with_title("OxideShell Notch")
        .with_inner_size(LogicalSize::new(notch_w, notch_h))
        .with_position(LogicalPosition::new(notch_x, notch_y))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(config.shell.notch_enabled)
        .build(&event_loop)?;

    let notch_window = Arc::new(notch_window);
    let notch_hwnd = get_hwnd(&notch_window);
    apply_acrylic_glass_and_strip_chrome(notch_hwnd, true);
    set_window_bounds(notch_hwnd, notch_x, notch_y, notch_w as i32, notch_h as i32);

    // 10. Create Desktop Widgets Window (Docked on right edge, HWND_BOTTOM)
    let widgets_x = config.positions.widgets_x.unwrap_or(initial_layout.widget_rect.left);
    let widgets_y = config.positions.widgets_y.unwrap_or(initial_layout.widget_rect.top);
    let widgets_w = initial_layout.widget_rect.width() as f64;
    let widgets_h = initial_layout.widget_rect.height() as f64;

    let widgets_window = WindowBuilder::new()
        .with_title("OxideShell Widgets")
        .with_inner_size(LogicalSize::new(widgets_w, widgets_h))
        .with_position(LogicalPosition::new(widgets_x, widgets_y))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(config.shell.widgets_enabled)
        .build(&event_loop)?;

    let widgets_window = Arc::new(widgets_window);
    let widgets_hwnd = get_hwnd(&widgets_window);
    // Widgets sit on the desktop layer like macOS Sonoma
    apply_acrylic_glass_and_strip_chrome(widgets_hwnd, false);
    set_window_bounds(widgets_hwnd, widgets_x, widgets_y, widgets_w as i32, widgets_h as i32);

    // 11. Softbuffer Context & Surfaces
    let context = Context::new(dock_window.clone())?;
    let mut dock_surface = Surface::new(&context, dock_window.clone())?;
    let mut notch_surface = Surface::new(&context, notch_window.clone())?;
    let mut widgets_surface = Surface::new(&context, widgets_window.clone())?;

    // Dynamic Launcher Window & Surface (created on demand, destroyed on close)
    let mut launcher_window: Option<Arc<Window>> = None;
    let mut launcher_surface: Option<Surface<Arc<Window>, Arc<Window>>> = None;

    // 12. Instantiate Modules
    let mut dock = MagnifyingDock::new(&script_engine, &icon_cache);
    let mut notch = DynamicNotch::new();
    let mut widgets = DesktopWidgets::new();
    widgets.update(&script_engine);
    let mut launcher = AppLauncher::new();

    // Initial RAM Purge
    trim_working_set();
    let initial_mem = get_process_memory();
    println!(
        "OxideShell initialized cleanly! Measured Working Set: {:.2} MB",
        initial_mem.working_set_mb()
    );

    let mut last_frame = Instant::now();
    let mut last_hot_reload_check = Instant::now();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                window_id,
                event: window_event,
            } => {
                // --- DOCK EVENTS ---
                if window_id == dock_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            dock.set_mouse_position(Some((position.x as f32, position.y as f32)));
                            dock_window.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            dock.set_mouse_position(None);
                            dock_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Right,
                            ..
                        } => {
                            let (sx, sy) = get_cursor_screen_pos();
                            if let Some(choice) = show_native_context_menu(dock_hwnd, sx, sy) {
                                handle_context_menu_choice(
                                    choice,
                                    &mut config,
                                    &mut script_engine,
                                    &geometry,
                                    &dock_window,
                                    &notch_window,
                                    &widgets_window,
                                    elwt,
                                );
                            }
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } => {
                            if dock.is_hovering_close {
                                config.shell.dock_enabled = false;
                                dock_window.set_visible(false);
                                let _ = config.save();
                            } else if dock.hovered_index.is_some() {
                                dock.handle_click(&script_engine);
                            } else {
                                start_window_drag(dock_hwnd);
                            }
                        }
                        WindowEvent::Moved(pos) => {
                            config.positions.dock_x = Some(pos.x);
                            config.positions.dock_y = Some(pos.y);
                            let _ = config.save();
                        }
                        WindowEvent::RedrawRequested => {
                            let size = dock_window.inner_size();
                            if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                                let _ = dock_surface.resize(w, h);
                                if let Ok(mut buffer) = dock_surface.buffer_mut() {
                                    if let Some(mut pixmap) = Pixmap::new(size.width, size.height) {
                                        pixmap.fill(Color::TRANSPARENT);
                                        dock.render(&mut pixmap.as_mut(), &mut font_renderer);
                                        copy_pixmap_to_softbuffer(&pixmap, &mut buffer);
                                        let _ = buffer.present();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // --- NOTCH EVENTS ---
                else if window_id == notch_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            notch.set_mouse_position(Some((position.x as f32, position.y as f32)));
                            notch_window.request_redraw();
                        }
                        WindowEvent::CursorEntered { .. } => {
                            notch.set_hovered(true);
                            let expanded_w = 540;
                            let expanded_h = 160;
                            let notch_x = (geometry.screen_width - expanded_w) / 2;
                            set_window_bounds(notch_hwnd, notch_x, 0, expanded_w, expanded_h);
                            notch_window.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            notch.set_hovered(false);
                            notch.set_mouse_position(None);
                            let idle_w = 220;
                            let idle_h = 34;
                            let notch_x = (geometry.screen_width - idle_w) / 2;
                            set_window_bounds(notch_hwnd, notch_x, 0, idle_w, idle_h);
                            notch_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Right,
                            ..
                        } => {
                            let (sx, sy) = get_cursor_screen_pos();
                            if let Some(choice) = show_native_context_menu(notch_hwnd, sx, sy) {
                                handle_context_menu_choice(
                                    choice,
                                    &mut config,
                                    &mut script_engine,
                                    &geometry,
                                    &dock_window,
                                    &notch_window,
                                    &widgets_window,
                                    elwt,
                                );
                            }
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } => {
                            let (sx, sy) = get_cursor_screen_pos();
                            let clicked_close = notch.handle_click(sx as f32, sy as f32);
                            if clicked_close {
                                let idle_w = 220;
                                let idle_h = 34;
                                let notch_x = (geometry.screen_width - idle_w) / 2;
                                set_window_bounds(notch_hwnd, notch_x, 0, idle_w, idle_h);
                            } else if !notch.is_hovered {
                                start_window_drag(notch_hwnd);
                            }
                            notch_window.request_redraw();
                        }
                        WindowEvent::Moved(pos) => {
                            config.positions.notch_x = Some(pos.x);
                            config.positions.notch_y = Some(pos.y);
                            let _ = config.save();
                        }
                        WindowEvent::RedrawRequested => {
                            let size = notch_window.inner_size();
                            if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                                let _ = notch_surface.resize(w, h);
                                if let Ok(mut buffer) = notch_surface.buffer_mut() {
                                    if let Some(mut pixmap) = Pixmap::new(size.width, size.height) {
                                        pixmap.fill(Color::TRANSPARENT);
                                        notch.render(&mut pixmap.as_mut(), &mut font_renderer);
                                        copy_pixmap_to_softbuffer(&pixmap, &mut buffer);
                                        let _ = buffer.present();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // --- WIDGETS EVENTS ---
                else if window_id == widgets_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            widgets.set_mouse_position(Some((position.x as f32, position.y as f32)));
                            widgets_window.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            widgets.set_mouse_position(None);
                            widgets_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Right,
                            ..
                        } => {
                            let (sx, sy) = get_cursor_screen_pos();
                            if let Some(choice) = show_native_context_menu(widgets_hwnd, sx, sy) {
                                handle_context_menu_choice(
                                    choice,
                                    &mut config,
                                    &mut script_engine,
                                    &geometry,
                                    &dock_window,
                                    &notch_window,
                                    &widgets_window,
                                    elwt,
                                );
                            }
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } => {
                            let card_closed = widgets.handle_click(&script_engine);
                            if card_closed.is_none() && widgets.mouse_pos.is_some() {
                                start_window_drag(widgets_hwnd);
                            }
                            widgets_window.request_redraw();
                        }
                        WindowEvent::Moved(pos) => {
                            config.positions.widgets_x = Some(pos.x);
                            config.positions.widgets_y = Some(pos.y);
                            let _ = config.save();
                        }
                        WindowEvent::RedrawRequested => {
                            let size = widgets_window.inner_size();
                            if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                                let _ = widgets_surface.resize(w, h);
                                if let Ok(mut buffer) = widgets_surface.buffer_mut() {
                                    if let Some(mut pixmap) = Pixmap::new(size.width, size.height) {
                                        pixmap.fill(Color::TRANSPARENT);
                                        widgets.render(&mut pixmap.as_mut(), &mut font_renderer);
                                        copy_pixmap_to_softbuffer(&pixmap, &mut buffer);
                                        let _ = buffer.present();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // --- DYNAMIC LAUNCHER EVENTS ---
                else if let Some(ref l_win) = launcher_window {
                    if window_id == l_win.id() {
                        match window_event {
                            WindowEvent::KeyboardInput {
                                event:
                                    KeyEvent {
                                        state: ElementState::Pressed,
                                        logical_key,
                                        text,
                                        ..
                                    },
                                ..
                            } => {
                                match logical_key {
                                    Key::Named(NamedKey::Escape) => {
                                        launcher.close();
                                        launcher_window = None;
                                        launcher_surface = None;
                                        trim_working_set();
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        launcher.launch_selected();
                                        launcher_window = None;
                                        launcher_surface = None;
                                        trim_working_set();
                                    }
                                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowRight) => {
                                        launcher.select_next();
                                        l_win.request_redraw();
                                    }
                                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowLeft) => {
                                        launcher.select_prev();
                                        l_win.request_redraw();
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        launcher.on_backspace();
                                        l_win.request_redraw();
                                    }
                                    _ => {
                                        if let Some(txt) = text {
                                            for c in txt.chars() {
                                                launcher.on_char_input(c);
                                            }
                                            l_win.request_redraw();
                                        }
                                    }
                                }
                            }
                            WindowEvent::RedrawRequested => {
                                if let Some(ref mut l_surf) = launcher_surface {
                                    let size = l_win.inner_size();
                                    if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                                        let _ = l_surf.resize(w, h);
                                        if let Ok(mut buffer) = l_surf.buffer_mut() {
                                            if let Some(pixmap) = launcher.render(&mut font_renderer) {
                                                copy_pixmap_to_softbuffer(pixmap, &mut buffer);
                                                let _ = buffer.present();
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Event::AboutToWait => {
                let now = Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f32().min(0.05);
                last_frame = now;

                // 1. Check if Rhai script requested to toggle Launcher
                let mut should_toggle_launcher = false;
                if let Ok(mut flag) = script_engine.pending_toggle_launcher.lock() {
                    if *flag {
                        should_toggle_launcher = true;
                        *flag = false;
                    }
                }

                if should_toggle_launcher {
                    if launcher.is_open {
                        launcher.close();
                        launcher_window = None;
                        launcher_surface = None;
                        trim_working_set();
                    } else {
                        launcher.open();
                        let lw = initial_layout.launcher_rect.width() as f64;
                        let lh = initial_layout.launcher_rect.height() as f64;
                        let lx = initial_layout.launcher_rect.left;
                        let ly = initial_layout.launcher_rect.top;

                        if let Ok(win) = WindowBuilder::new()
                            .with_title("OxideShell Launcher")
                            .with_inner_size(LogicalSize::new(lw, lh))
                            .with_position(LogicalPosition::new(lx, ly))
                            .with_decorations(false)
                            .with_transparent(true)
                            .with_resizable(false)
                            .build(elwt)
                        {
                            let win = Arc::new(win);
                            let win_hwnd = get_hwnd(&win);
                            apply_acrylic_glass_and_strip_chrome(win_hwnd, true);
                            set_window_bounds(win_hwnd, lx, ly, lw as i32, lh as i32);
                            if let Ok(surf) = Surface::new(&context, win.clone()) {
                                launcher_surface = Some(surf);
                                win.request_redraw();
                            }
                            launcher_window = Some(win);
                        }
                    }
                }

                // 2. Check for live hot-reload of config.toml and scripts/*.rhai every 1s
                if now.duration_since(last_hot_reload_check) >= Duration::from_millis(1000) {
                    last_hot_reload_check = now;
                    let config_changed = config.check_reload();
                    let scripts_changed = script_engine.check_reload();

                    if config_changed || scripts_changed {
                        dock_window.set_visible(config.shell.dock_enabled);
                        notch_window.set_visible(config.shell.notch_enabled);
                        widgets_window.set_visible(config.shell.widgets_enabled);

                        widgets.update(&script_engine);
                        dock = MagnifyingDock::new(&script_engine, &icon_cache);

                        dock_window.request_redraw();
                        notch_window.request_redraw();
                        widgets_window.request_redraw();
                    }
                }

                // 3. Update physics animations
                let dock_anim = dock.update(dt);
                let notch_anim = notch.update(dt);

                if dock_anim {
                    dock_window.request_redraw();
                }
                if notch_anim {
                    notch_window.request_redraw();
                }

                let needs_animation = dock_anim || notch_anim;

                // 4. Strict CPU Budget: Wait when idle, WaitUntil(16ms) only when animating
                if needs_animation {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(1000),
                    ));
                }
            }
            _ => {}
        }
    })?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_context_menu_choice(
    choice: ContextMenuChoice,
    config: &mut OxideConfig,
    script_engine: &mut ScriptEngine,
    geometry: &ScreenGeometry,
    dock_window: &Window,
    notch_window: &Window,
    widgets_window: &Window,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
) {
    match choice {
        ContextMenuChoice::ToggleNotch => {
            config.shell.notch_enabled = !config.shell.notch_enabled;
            notch_window.set_visible(config.shell.notch_enabled);
            let _ = config.save();
        }
        ContextMenuChoice::ToggleWidgets => {
            config.shell.widgets_enabled = !config.shell.widgets_enabled;
            widgets_window.set_visible(config.shell.widgets_enabled);
            let _ = config.save();
        }
        ContextMenuChoice::ToggleDock => {
            config.shell.dock_enabled = !config.shell.dock_enabled;
            dock_window.set_visible(config.shell.dock_enabled);
            let _ = config.save();
        }
        ContextMenuChoice::ResetPositions => {
            config.positions = Default::default();
            let _ = config.save();

            let layout = LayoutSolver::solve(geometry, 440);
            let d_hwnd = get_hwnd(dock_window);
            let n_hwnd = get_hwnd(notch_window);
            let w_hwnd = get_hwnd(widgets_window);

            set_window_bounds(
                d_hwnd,
                layout.dock_rect.left,
                layout.dock_rect.top,
                layout.dock_rect.width(),
                layout.dock_rect.height(),
            );
            set_window_bounds(
                n_hwnd,
                layout.notch_rect.left,
                layout.notch_rect.top,
                layout.notch_rect.width(),
                layout.notch_rect.height(),
            );
            set_window_bounds(
                w_hwnd,
                layout.widget_rect.left,
                layout.widget_rect.top,
                layout.widget_rect.width(),
                layout.widget_rect.height(),
            );

            dock_window.request_redraw();
            notch_window.request_redraw();
            widgets_window.request_redraw();
        }
        ContextMenuChoice::OpenConfigFolder => {
            let _ = Command::new("explorer.exe")
                .arg(OxideConfig::config_dir())
                .spawn();
        }
        ContextMenuChoice::ReloadScripts => {
            script_engine.load_all_scripts();
            dock_window.request_redraw();
            notch_window.request_redraw();
            widgets_window.request_redraw();
        }
        ContextMenuChoice::Quit => {
            elwt.exit();
        }
    }
}
