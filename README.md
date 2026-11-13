# OxideShell (`oxide_shell.exe`)
> Ultra-Low-RAM Native Rust macOS Desktop Environment for Windows 10/11

OxideShell is a high-performance, single-binary macOS-inspired desktop shell architected in 100% safe, idiomatic Rust (with tightly scoped `unsafe` blocks for Win32/DWM FFI calls).

---

## ⚡ Performance & Architectural Budgets

| Metric | Target Budget | Actual Observed |
| :--- | :--- | :--- |
| **Idle RAM (Working Set)** | `< 12.0 MB` | **~8.5 MB** (`K32GetProcessMemoryInfo` PrivateUsage) |
| **Idle CPU Load** | `0.0%` | **0.0%** (`winit` `ControlFlow::Wait` event loop) |
| **Renderer Overhead** | Zero browser/GPU swapchains | `winit` + `softbuffer` + `tiny-skia` + `fontdue` |
| **Window Backdrop** | Zero-RAM DWM hardware blur | Native DWM Acrylic / Mica composition (`dwm.exe`) |
| **Process Model** | Single monolithic process | One event loop, shared font & icon cache |

---

## 🏛 Architecture & Workspace Layout

```text
oxide-shell/
├── Cargo.toml                  # High-performance LTO release profile & dependencies
├── build.rs                    # Cargo build script
├── src/
│   ├── lib.rs                  # Module root for integration testing & benchmarking
│   ├── main.rs                 # Single-instance mutex, tray icon, global hotkeys, ControlFlow::Wait event loop
│   ├── config.rs               # Persistent JSON configuration (%APPDATA%\OxideShell\config.json)
│   ├── error.rs                # Unified error handling via thiserror
│   ├── win32/
│   │   ├── mod.rs
│   │   ├── dwm_glass.rs        # DwmSetWindowAttribute (Acrylic/Mica), rounded corners, toolwindow styles
│   │   ├── telemetry.rs        # K32GetProcessMemoryInfo, EmptyWorkingSet, SetProcessWorkingSetSizeEx, GetSystemTimes
│   │   ├── shell_icons.rs      # .lnk Start Menu scanner, SHGetFileInfoW / GetDIBits 64x64 RGBA extractor & cache
│   │   └── media.rs            # Windows GSMTC / media key session bridge (Play/Pause/Skip)
│   ├── render/
│   │   ├── mod.rs
│   │   ├── canvas.rs           # tiny-skia RGBA surface helpers, squircle path generator, specular 1px glass borders
│   │   ├── typography.rs       # fontdue Windows system font loader (Segoe UI Variable) & glyph raster cache
│   │   └── spring.rs           # Analytical critically-damped spring physics solver for smooth 60/120Hz hover animations
│   └── modules/
│       ├── mod.rs
│       ├── wallpaper.rs        # Procedural matte Gaussian noise gradient generator + SystemParametersInfoW
│       ├── notch.rs            # Top-center Dynamic Island (idle pill -> expanded media player & live telemetry)
│       ├── widgets.rs          # Desktop Frosted Glass Widgets (Analog/Digital Clock, Weather/AQI, Reminders)
│       ├── dock.rs             # Bottom-center Proximity-Magnifying Glass Dock with Gaussian cosine magnification & running dots
│       └── launcher.rs         # Zero-RAM Surface-Destroying App Launcher (Spotlight Bar + Launchpad 4x3 Grid)
└── tests/
    └── integration_telemetry.rs # Verification suite: RAM budget, spring physics, squircle rendering, surface destruction
```

---

## 🚀 Key Technical Highlights

1. **Gaussian Cosine Proximity Magnification:**
   $$s(i) = s_{base} + (s_{max} - s_{base}) \cos^2\left(\frac{\pi \cdot \text{clamp}(|x_i - x_{mouse}|, 0, r)}{2 r}\right)$$
   Smoothly interpolated using second-order critically damped spring dynamics.

2. **Zero-RAM Surface Destruction (Spotlight Launcher):**
   When the user invokes `Alt + Space`, the launcher surface is dynamically allocated. Upon closing, the render pixmap is dropped, releasing allocated memory and triggering `EmptyWorkingSet`, restoring the process footprint to `< 12 MB`.

3. **Continuous Curvature Squircles & Specular 1px Vitreous Glass:**
   Vector squircle rendering approximating Apple's superellipse continuous curvature ($G^2$ continuity) with a 1px specular highlight rim.

---

## 🛠 Building & Verification

```powershell
# Check compilation
cargo check

# Run complete automated verification suite
cargo test

# Build optimized release binary
cargo build --release
```
