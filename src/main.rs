use std::num::NonZeroU32;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use softbuffer::{Context, Surface};
use tiny_skia::{Color, Pixmap};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_NOREPEAT, VK_SPACE};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowBuilder;

pub mod config;
pub mod error;
pub mod modules;
pub mod render;
pub mod win32;

use config::OxideConfig;
use modules::dock::MagnifyingDock;
use modules::launcher::AppLauncher;
use modules::notch::DynamicNotch;
use modules::wallpaper::WallpaperGenerator;
use modules::widgets::DesktopWidgets;
use render::canvas::copy_pixmap_to_softbuffer;
use render::typography::FontRenderer;
use win32::dwm_glass::{configure_shell_window, enable_acrylic_glass};
use win32::shell_icons::IconCache;
use win32::telemetry::{get_process_memory, trim_working_set};

const SINGLE_INSTANCE_MUTEX: &str = "OxideShell_SingleInstance_Mutex_Global";
const HOTKEY_LAUNCHER_ID: i32 = 0x5348; // "SH"

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Single-Instance Check
    let mutex_name: Vec<u16> = std::ffi::OsStr::new(SINGLE_INSTANCE_MUTEX)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mutex = unsafe { CreateMutexW(std::ptr::null(), 1, mutex_name.as_ptr()) };
    if mutex == 0 || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        eprintln!("OxideShell is already running on this desktop.");
        return Ok(());
    }

    // 2. Load Configuration
    let config = OxideConfig::load();

    // 3. Register Global Hotkey (Alt + Space for Spotlight Launcher)
    unsafe {
        RegisterHotKey(0, HOTKEY_LAUNCHER_ID, MOD_ALT | MOD_NOREPEAT, VK_SPACE as u32);
    }

    // 4. Initialize procedural desktop wallpaper
    let wallpaper_gen = WallpaperGenerator::new(1920, 1080);
    let _ = wallpaper_gen.generate_and_apply();

    // 5. Initialize shared font renderer & icon cache
    let mut font_renderer = FontRenderer::new().unwrap_or_else(|_| {
        eprintln!("Warning: could not find Segoe UI Variable, using fallback.");
        FontRenderer::new().expect("Fallback font failed")
    });
    let icon_cache = IconCache::new();

    // 6. Create Winit Event Loop
    let event_loop = EventLoop::new()?;

    // 7. Create Dock Window
    let dock_width = 560.0;
    let dock_height = 84.0;
    let dock_window = WindowBuilder::new()
        .with_title("OxideShell Dock")
        .with_inner_size(LogicalSize::new(dock_width, dock_height))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .build(&event_loop)?;

    let dock_window = Arc::new(dock_window);

    // Apply Win32 Acrylic Glass and Topmost non-activating style
    if let Ok(handle) = dock_window.window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            let hwnd = win32_handle.hwnd.get() as HWND;
            enable_acrylic_glass(hwnd, config.dark_mode);
            configure_shell_window(hwnd, true, true);
        }
    }

    // 8. Create Dynamic Notch Window (Top-Center)
    let notch_window = WindowBuilder::new()
        .with_title("OxideShell Notch")
        .with_inner_size(LogicalSize::new(380.0, 130.0))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .build(&event_loop)?;

    let notch_window = Arc::new(notch_window);

    if let Ok(handle) = notch_window.window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            let hwnd = win32_handle.hwnd.get() as HWND;
            enable_acrylic_glass(hwnd, config.dark_mode);
            configure_shell_window(hwnd, true, false);
        }
    }

    // 9. Initialize Softbuffer Surfaces
    let context = Context::new(dock_window.clone())?;
    let mut dock_surface = Surface::new(&context, dock_window.clone())?;
    let mut notch_surface = Surface::new(&context, notch_window.clone())?;

    // 10. Instantiate Modules
    let mut dock = MagnifyingDock::new(&config.pinned_apps, &icon_cache);
    let mut notch = DynamicNotch::new();
    let mut _widgets = DesktopWidgets::new(config.weather_city.clone(), config.reminders.clone());
    let mut launcher = AppLauncher::new();

    // Initial trim to lock idle footprint
    trim_working_set();

    let initial_mem = get_process_memory();
    println!(
        "OxideShell initialized successfully! Idle Private RAM: {:.2} MB",
        initial_mem.private_usage_mb()
    );

    let mut last_frame = Instant::now();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent {
                window_id,
                event: window_event,
            } => {
                if window_id == dock_window.id() {
                    match window_event {
                        WindowEvent::CursorMoved { position, .. } => {
                            dock.set_mouse_position(Some(position.x as f32));
                            dock_window.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            dock.set_mouse_position(None);
                            dock_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } => {
                            dock.handle_click(0.0);
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
                        WindowEvent::CloseRequested => {
                            elwt.exit();
                        }
                        _ => {}
                    }
                } else if window_id == notch_window.id() {
                    match window_event {
                        WindowEvent::CursorEntered { .. } => {
                            notch.set_hovered(true);
                            notch_window.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            notch.set_hovered(false);
                            notch_window.request_redraw();
                        }
                        WindowEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } => {
                            notch.handle_click(190.0, 115.0);
                            notch_window.request_redraw();
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

                // Global Launcher keyboard handlers
                if launcher.is_open {
                    if let WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                state: ElementState::Pressed,
                                logical_key,
                                text,
                                ..
                            },
                        ..
                    } = window_event
                    {
                        match logical_key {
                            Key::Named(NamedKey::Escape) => {
                                launcher.close();
                            }
                            Key::Named(NamedKey::Enter) => {
                                launcher.launch_selected();
                            }
                            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowRight) => {
                                launcher.select_next();
                            }
                            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowLeft) => {
                                launcher.select_prev();
                            }
                            Key::Named(NamedKey::Backspace) => {
                                launcher.on_backspace();
                            }
                            _ => {
                                if let Some(txt) = text {
                                    for c in txt.chars() {
                                        launcher.on_char_input(c);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Event::AboutToWait => {
                let now = Instant::now();
                let dt = now.duration_since(last_frame).as_secs_f32().min(0.05);
                last_frame = now;

                let dock_anim = dock.update(dt);
                let notch_anim = notch.update(dt);

                if dock_anim {
                    dock_window.request_redraw();
                }
                if notch_anim {
                    notch_window.request_redraw();
                }

                let needs_animation = dock_anim || notch_anim;

                // STRICT CPU BUDGET: Wait until next frame if animating, else wait indefinitely
                if needs_animation {
                    elwt.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    elwt.set_control_flow(ControlFlow::Wait);
                }
            }
            _ => {}
        }
    })?;

    Ok(())
}

