# OxideShell (`oxide_shell.exe`)
> Ultra-Low-RAM Native Rust macOS Desktop Environment for Windows 10/11

OxideShell is a high-performance, single-binary macOS-inspired desktop shell architected in 100% safe, idiomatic Rust with tightly scoped `unsafe` blocks for Win32/DWM FFI calls.

---

## ⚡ Performance & Architectural Budgets

| Metric | Target Budget | Actual Observed |
| :--- | :--- | :--- |
| **Idle RAM (Working Set)** | `< 14.0 MB` | **~8.5 MB** (`K32GetProcessMemoryInfo` PrivateUsage) |
| **Idle CPU Load** | `0.0%` | **0.0%** (`winit` `ControlFlow::Wait` event loop) |
| **Renderer Overhead** | Zero browser / Direct3D swapchains | `winit` + `softbuffer` + `tiny-skia` + `fontdue` |
| **Window Backdrop** | Zero-RAM DWM hardware blur | Native DWM Acrylic blur (`SetWindowCompositionAttribute`) |
| **Programmability** | Embedded live scripting | Rhai 1.19 (`.rhai`) + `%APPDATA%\OxideShell\config.toml` |
| **Binary Footprint** | Single monolithic executable | **~3.39 MB** release binary |

---

## 🏗️ Architecture & Workspace Layout

```text
oxide-shell/
├── Cargo.toml                  # High-performance release profile & dependencies
├── build.rs                    # Cargo build script
├── src/
│   ├── lib.rs                  # Module root for integration testing & benchmarking
│   ├── main.rs                 # Single-instance mutex, tray icon, hotkeys, ControlFlow::Wait event loop
│   ├── config.rs               # Persistent TOML configuration (%APPDATA%\OxideShell\config.toml)
│   ├── error.rs                # Unified error handling via thiserror
│   ├── scripting/
│   │   └── mod.rs              # Rhai scripting engine with native Win32/system bindings & live hot-reload
│   ├── win32/
│   │   ├── mod.rs
│   │   ├── layout.rs           # Multi-monitor safe ScreenGeometry & LayoutSolver (zero overlap)
│   │   ├── dwm_glass.rs        # WS_POPUP acrylic glass blur, borderless window drag, native popup menus
│   │   ├── telemetry.rs        # K32GetProcessMemoryInfo, EmptyWorkingSet, CPU samplers
│   │   ├── shell_icons.rs      # .lnk Start Menu scanner & 64x64 RGBA icon extractor
│   │   └── media.rs            # Windows GSMTC / media key session bridge (Play/Pause/Skip)
│   ├── render/
│   │   ├── mod.rs
│   │   ├── canvas.rs           # tiny-skia squircle rasterizer, 1px top-lit specular borders, close buttons
│   │   ├── typography.rs       # fontdue Segoe UI Variable font loader & glyph raster cache
│   │   └── spring.rs           # Analytical critically-damped spring physics solver
│   └── modules/
│       ├── mod.rs
│       ├── wallpaper.rs        # Procedural radial matte studio gradient + 2% grain (never pitch black)
│       ├── notch.rs            # Top Dynamic Island (idle pill -> expanded media player & telemetry)
│       ├── widgets.rs          # Desktop Frosted Glass Widgets (spaced 14px cards, close buttons)
│       ├── dock.rs             # Proximity-Magnifying Glass Dock with Gaussian cosine scaling & running dots
│       └── launcher.rs         # Zero-RAM Surface-Destroying App Launcher (Spotlight + 4x3 Grid)
└── tests/
    ├── integration_telemetry.rs # RAM telemetry, spring physics, squircle rendering, surface destruction
    └── verify_ui_and_ram.rs     # Geometry non-overlap, wallpaper luminance, Rhai execution, RAM gate
```

---

## 🚀 Key Technical Highlights

1. **Explicit Multi-Monitor Geometry Solver (Zero Stacking):**
   - Automatically queries primary monitor work area (`SPI_GETWORKAREA`) and full display metrics (`SM_CXSCREEN`, `SM_CYSCREEN`).
   - Mathematically calculates positions for Top Notch, Bottom Dock, Desktop Widgets, and Spotlight Launcher, guaranteeing zero overlap.

2. **True Borderless macOS Frosted Glass (`WS_POPUP`):**
   - Strips all native Windows caption/border chrome (`WS_POPUP | WS_VISIBLE`, `WS_EX_TOOLWINDOW`).
   - Uses `DwmExtendFrameIntoClientArea` with negative margins and `SetWindowCompositionAttribute` (`ACCENT_ENABLE_ACRYLICBLURBEHIND`) for silky hardware-accelerated translucent glass without solid grey box artifacts.

3. **Drag-Anywhere & macOS Close Controls:**
   - Any window can be repositioned by clicking and dragging anywhere on the glass backdrop (via `WM_NCLBUTTONDOWN` / `HTCAPTION`). Window coordinates persist in `config.toml`.
   - Hovering reveals circular macOS-style `(×)` close buttons to dismiss individual cards or modules.
   - Right-clicking any module opens a native Win32 context menu with toggles, settings reload, wallpaper generation, and quit options.

4. **Embedded Live Scripting (Rhai):**
   - User-programmable scripts located in `%APPDATA%\OxideShell\scripts\`:
     - `dock_actions.rhai`: Dock items, badge counters, and click handlers.
     - `widgets.rhai`: Dynamic widget cards (Clock, System Vitals, Quick Launch, Tasks).
     - `notch_modules.rhai`: Dynamic Island modes and telemetry strings.
   - Hot-reloads live on disk changes every 1 second without restarting the process.

5. **Procedural Radial Matte Wallpaper Engine:**
   - Generates a radial matte studio gradient (`#1b2432` → `#11151d` → `#090b10`) infused with 2% uniform grain.
   - Average luminance verified to fall within `[12.0, 65.0]` (never pitch black).

6. **Gaussian Cosine Proximity Magnification:**
   $$s(i) = s_{base} + (s_{max} - s_{base}) \cos^2\left(\frac{\pi \cdot \text{clamp}(|x_i - x_{mouse}|, 0, r)}{2 r}\right)$$
   Smoothly animated using analytical critically damped spring physics.

7. **Zero-RAM Surface Destruction:**
   - Spotlight Launcher (`Alt + Space` or Dock Rocket) allocates its raster surface on open and completely drops it on close, calling `EmptyWorkingSet` to keep memory `< 12 MB`.

---

## 🛠️ Building & Verification

```powershell
# Set Cargo in PATH if needed
$env:Path = "C:\Users\anshu\.cargo\bin;" + $env:Path

# Check compilation
cargo check

# Run complete 12-test automated verification suite
cargo test

# Enforce zero warnings with Clippy
cargo clippy --all-targets -- -D warnings

# Build optimized release binary
cargo build --release
```
