use std::os::windows::ffi::OsStrExt;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
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
use render::typography::FontRenderer;
use scripting::ScriptEngine;
use win32::dwm_glass::{
    apply_acrylic_glass_and_strip_chrome, set_window_bounds, show_native_context_menu,
    ContextMenuChoice,
};
use win32::layered::LayeredSurface;
use win32::layout::{LayoutSolver, ScreenGeometry};
use win32::shell_icons::IconCache;
use win32::telemetry::{get_process_memory, trim_working_set};

const SINGLE_INSTANCE_MUTEX: &str = "OxideShell_SingleInstance_Mutex_Global";
const HOTKEY_LAUNCHER_ID: i32 = 0x5348; // "SH"

type DragState = (winit::window::WindowId, (i32, i32), (i32, i32));

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
    let wallpaper_gen = WallpaperGenerator::default();
    let _ = wallpaper_gen.generate_and_apply();

    // 5. Initialize Shared Font Renderer & Icon Cache
    let mut font_renderer = FontRenderer::new().unwrap_or_else(|_| {
        eprintln!("Warning: Using fallback font loader.");
        FontRenderer::new().expect("Fallback font failed")
    });
    let icon_cache = IconCache::new();

    // 6. Query Monitor Geometry & Calculate Initial Layout
    let geometry = ScreenGeometry::query_primary();
    let initial_layout = LayoutSolver::solve(&geometry, 680);

    // 7. Create Winit Event Loop
    let event_loop = EventLoop::new()?;

    // 8. Create Bottom Dock Window (Layered Surface 680 x 116)
    let mut dock_x = config.positions.dock_x.unwrap_or(initial_layout.dock_rect.left);
    let mut dock_y = config.positions.dock_y.unwrap_or(initial_layout.dock_rect.top);
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
    let mut dock_layered = LayeredSurface::new(dock_w as u32, dock_h as u32)
        .map_err(std::io::Error::other)?;

    // 9. Create Top Notch Window (Layered Surface 540 x 160 max buffer)
    let mut notch_x = config.positions.notch_x.unwrap_or(initial_layout.notch_rect.left);
    let mut notch_y = config.positions.notch_y.unwrap_or(0);
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
    let mut notch_layered = LayeredSurface::new(540, 160)
        .map_err(std::io::Error::other)?;

    // 10. Create Desktop Widgets Window (Layered Surface 300 x 620, Desktop Layer)
    let mut widgets_x = config.positions.widgets_x.unwrap_or(initial_layout.widget_rect.left);
    let mut widgets_y = config.positions.widgets_y.unwrap_or(initial_layout.widget_rect.top);
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
    apply_acrylic_glass_and_strip_chrome(widgets_hwnd, false);
    set_window_bounds(widgets_hwnd, widgets_x, widgets_y, widgets_w as i32, widgets_h as i32);
    let mut widgets_layered = LayeredSurface::new(widgets_w as u32, widgets_h as u32)
        .map_err(std::io::Error::other)?;

    // Dynamic Launcher Window & Layered Surface (created on demand, destroyed on close)
    let mut launcher_window: Option<Arc<Window>> = None;
    let mut launcher_layered: Option<LayeredSurface> = None;

    // 11. Instantiate Modules
    let mut dock = MagnifyingDock::new(&script_engine, &icon_cache);
    let mut notch = DynamicNotch::new();
    let mut widgets = DesktopWidgets::new();
    widgets.update(&script_engine);
    let mut launcher = AppLauncher::new();

    // Drag-anywhere tracking state
    let mut drag_state: Option<DragState> = None;

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
                // --- DOCK WINDOW EVENTS ---
                if window_id == dock_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            if let Some((drag_id, (start_cx, start_cy), (start_wx, start_wy))) = drag_state {
                                if drag_id == window_id {
                                    let (cur_x, cur_y) = get_cursor_screen_pos();
                                    let dx = cur_x - start_cx;
                                    let dy = cur_y - start_cy;
                                    dock_x = start_wx + dx;
                                    dock_y = start_wy + dy;
                                    set_window_bounds(dock_hwnd, dock_x, dock_y, dock_w as i32, dock_h as i32);
                                    dock_layered.present(dock_hwnd, dock_x, dock_y);
                                }
                            } else {
                                dock.set_mouse_position(Some((position.x as f32, position.y as f32)));
                                dock_window.request_redraw();
                            }
                        }
                        WindowEvent::CursorLeft { .. } => {
                            if drag_state.is_none() {
                                dock.set_mouse_position(None);
                                dock_window.request_redraw();
                            }
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
                                let (cur_x, cur_y) = get_cursor_screen_pos();
                                drag_state = Some((window_id, (cur_x, cur_y), (dock_x, dock_y)));
                            }
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Released,
                            button: MouseButton::Left,
                            ..
                        } => {
                            if drag_state.is_some() {
                                drag_state = None;
                                config.positions.dock_x = Some(dock_x);
                                config.positions.dock_y = Some(dock_y);
                                let _ = config.save();
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            if let Some(mut pixmap) = Pixmap::new(dock_w as u32, dock_h as u32) {
                                pixmap.fill(Color::TRANSPARENT);
                                dock.render(&mut pixmap.as_mut(), &mut font_renderer);
                                dock_layered.copy_from_pixmap(&pixmap);
                                dock_layered.present(dock_hwnd, dock_x, dock_y);
                            }
                        }
                        _ => {}
                    }
                }
                // --- TOP NOTCH WINDOW EVENTS ---
                else if window_id == notch_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            if let Some((drag_id, (start_cx, start_cy), (start_wx, start_wy))) = drag_state {
                                if drag_id == window_id {
                                    let (cur_x, cur_y) = get_cursor_screen_pos();
                                    let dx = cur_x - start_cx;
                                    let dy = cur_y - start_cy;
                                    notch_x = start_wx + dx;
                                    notch_y = start_wy + dy;
                                    let nw = notch.width_spring.value.round() as i32;
                                    let nh = notch.height_spring.value.round() as i32;
                                    set_window_bounds(notch_hwnd, notch_x, notch_y, nw, nh);
                                    notch_layered.present(notch_hwnd, notch_x, notch_y);
                                }
                            } else {
                                notch.set_mouse_position(Some((position.x as f32, position.y as f32)));
                                notch_window.request_redraw();
                            }
                        }
                        WindowEvent::CursorEntered { .. } => {
                            notch.set_hovered(true);
                            let expanded_w = 540;
                            let expanded_h = 160;
                            let nx = (geometry.screen_width - expanded_w) / 2;
                            set_window_bounds(notch_hwnd, nx, 0, expanded_w, expanded_h);
                            notch_window.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            if drag_state.is_none() {
                                notch.set_hovered(false);
                                notch.set_mouse_position(None);
                                let idle_w = 220;
                                let idle_h = 34;
                                let nx = (geometry.screen_width - idle_w) / 2;
                                set_window_bounds(notch_hwnd, nx, 0, idle_w, idle_h);
                                notch_window.request_redraw();
                            }
                        }
                        WindowEvent::DroppedFile(path) => {
                            println!("[OxideShell Shelf] File dropped on Notch: {:?}", path);
                            notch.set_hovered(true);
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
                            let rel_x = (sx - notch_x) as f32;
                            let rel_y = (sy - notch_y) as f32;

                            if notch.is_hovering_close {
                                if notch.is_hovered {
                                    notch.set_hovered(false);
                                    let idle_w = 220;
                                    let idle_h = 34;
                                    let nx = (geometry.screen_width - idle_w) / 2;
                                    set_window_bounds(notch_hwnd, nx, 0, idle_w, idle_h);
                                } else {
                                    config.shell.notch_enabled = false;
                                    notch_window.set_visible(false);
                                    let _ = config.save();
                                }
                            } else if notch.is_hovered {
                                notch.handle_click(rel_x, rel_y);
                            } else {
                                let (cur_x, cur_y) = get_cursor_screen_pos();
                                drag_state = Some((window_id, (cur_x, cur_y), (notch_x, notch_y)));
                            }
                            notch_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Released,
                            button: MouseButton::Left,
                            ..
                        } => {
                            if drag_state.is_some() {
                                drag_state = None;
                                config.positions.notch_x = Some(notch_x);
                                config.positions.notch_y = Some(notch_y);
                                let _ = config.save();
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            let nw = (notch.width_spring.value.round() as u32).max(1);
                            let nh = (notch.height_spring.value.round() as u32).max(1);
                            if let Some(mut pixmap) = Pixmap::new(nw, nh) {
                                pixmap.fill(Color::TRANSPARENT);
                                notch.render(&mut pixmap.as_mut(), &mut font_renderer);
                                let _ = notch_layered.resize(nw, nh);
                                notch_layered.copy_from_pixmap(&pixmap);
                                notch_layered.present(notch_hwnd, notch_x, notch_y);
                            }
                        }
                        _ => {}
                    }
                }
                // --- DESKTOP WIDGETS EVENTS ---
                else if window_id == widgets_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            if let Some((drag_id, (start_cx, start_cy), (start_wx, start_wy))) = drag_state {
                                if drag_id == window_id {
                                    let (cur_x, cur_y) = get_cursor_screen_pos();
                                    let dx = cur_x - start_cx;
                                    let dy = cur_y - start_cy;
                                    widgets_x = start_wx + dx;
                                    widgets_y = start_wy + dy;
                                    set_window_bounds(widgets_hwnd, widgets_x, widgets_y, widgets_w as i32, widgets_h as i32);
                                    widgets_layered.present(widgets_hwnd, widgets_x, widgets_y);
                                }
                            } else {
                                widgets.set_mouse_position(Some((position.x as f32, position.y as f32)));
                                widgets_window.request_redraw();
                            }
                        }
                        WindowEvent::CursorLeft { .. } => {
                            if drag_state.is_none() {
                                widgets.set_mouse_position(None);
                                widgets_window.request_redraw();
                            }
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
                                let (cur_x, cur_y) = get_cursor_screen_pos();
                                drag_state = Some((window_id, (cur_x, cur_y), (widgets_x, widgets_y)));
                            }
                            widgets_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Released,
                            button: MouseButton::Left,
                            ..
                        } => {
                            if drag_state.is_some() {
                                drag_state = None;
                                config.positions.widgets_x = Some(widgets_x);
                                config.positions.widgets_y = Some(widgets_y);
                                let _ = config.save();
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            if let Some(mut pixmap) = Pixmap::new(widgets_w as u32, widgets_h as u32) {
                                pixmap.fill(Color::TRANSPARENT);
                                widgets.render(&mut pixmap.as_mut(), &mut font_renderer);
                                widgets_layered.copy_from_pixmap(&pixmap);
                                widgets_layered.present(widgets_hwnd, widgets_x, widgets_y);
                            }
                        }
                        _ => {}
                    }
                }
                // --- DYNAMIC SPOTLIGHT LAUNCHER EVENTS ---
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
                                        launcher_layered = None;
                                        trim_working_set();
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        launcher.launch_selected();
                                        launcher_window = None;
                                        launcher_layered = None;
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
                                if let Some(ref mut l_surf) = launcher_layered {
                                    if let Some(pixmap) = launcher.render(&mut font_renderer) {
                                        l_surf.copy_from_pixmap(pixmap);
                                        let lx = initial_layout.launcher_rect.left;
                                        let ly = initial_layout.launcher_rect.top;
                                        l_surf.present(get_hwnd(l_win), lx, ly);
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

                // 1. Check if Rhai script or hotkey requested to toggle Launcher
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
                        launcher_layered = None;
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
                            if let Ok(surf) = LayeredSurface::new(lw as u32, lh as u32) {
                                launcher_layered = Some(surf);
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

                // 4. ControlFlow::Wait event loop saves battery and maintains 0.0% CPU when idle
                if dock_anim || notch_anim || launcher.is_open || drag_state.is_some() {
                    elwt.set_control_flow(ControlFlow::Poll);
                } else {
                    elwt.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(1000)));
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
        ContextMenuChoice::ShowAllModules => {
            config.shell.notch_enabled = true;
            config.shell.widgets_enabled = true;
            config.shell.dock_enabled = true;
            notch_window.set_visible(true);
            widgets_window.set_visible(true);
            dock_window.set_visible(true);
            let _ = config.save();
            dock_window.request_redraw();
            notch_window.request_redraw();
            widgets_window.request_redraw();
        }
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
        ContextMenuChoice::RegenerateWallpaper => {
            let gen = WallpaperGenerator::default();
            let _ = gen.generate_and_apply();
        }
        ContextMenuChoice::EditScripts => {
            let _ = Command::new("explorer.exe")
                .arg(OxideConfig::scripts_dir())
                .spawn();
        }
        ContextMenuChoice::ResetPositions => {
            config.positions = Default::default();
            let _ = config.save();

            let layout = LayoutSolver::solve(geometry, 680);
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
