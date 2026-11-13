use std::ffi::c_void;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_BOTTOM, HWND_TOPMOST,
    SWP_NOMOVE, SWP_NOSIZE, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST,
};

// Windows 11 DWM backdrop types
pub const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
pub const DWMSBT_AUTO: u32 = 0;
pub const DWMSBT_NONE: u32 = 1;
pub const DWMSBT_MAINWINDOW: u32 = 2; // Mica
pub const DWMSBT_TRANSIENTWINDOW: u32 = 3; // Acrylic
pub const DWMSBT_TABBEDWINDOW: u32 = 4; // Mica Alt

pub const DWMWCP_ROUND: u32 = 2;

#[repr(C)]
pub struct Margins {
    pub cx_left_width: i32,
    pub cx_right_width: i32,
    pub cy_top_height: i32,
    pub cy_bottom_height: i32,
}

type PfnDwmExtendFrameIntoClientArea = unsafe extern "system" fn(HWND, *const Margins) -> i32;

#[allow(clippy::manual_c_str_literals)]
pub fn enable_acrylic_glass(hwnd: HWND, dark_mode: bool) {
    unsafe {
        // 1. Enable immersive dark mode
        let dark: u32 = if dark_mode { 1 } else { 0 };
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        // 2. Set rounded corner preference
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        // 3. Set Acrylic backdrop blur (Windows 11 build 22621+)
        let backdrop = DWMSBT_TRANSIENTWINDOW;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        // 4. Extend frame into entire client area (dynamic resolution)
        let dwm_module = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
        if dwm_module != 0 {
            let proc = GetProcAddress(dwm_module, b"DwmExtendFrameIntoClientArea\0".as_ptr());
            if let Some(func) = proc {
                let extend_frame: PfnDwmExtendFrameIntoClientArea = std::mem::transmute(func);
                let margins = Margins {
                    cx_left_width: -1,
                    cx_right_width: -1,
                    cy_top_height: -1,
                    cy_bottom_height: -1,
                };
                extend_frame(hwnd, &margins);
            }
        }
    }
}

pub fn configure_shell_window(hwnd: HWND, topmost: bool, no_activate: bool) {
    unsafe {
        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;

        // Hide from taskbar and Alt-Tab
        ex_style |= WS_EX_TOOLWINDOW;
        ex_style &= !WS_EX_APPWINDOW;

        // Support transparency
        ex_style |= WS_EX_LAYERED;

        if no_activate {
            // Prevent dock / widgets from stealing focus on click
            ex_style |= WS_EX_NOACTIVATE;
        }

        if topmost {
            ex_style |= WS_EX_TOPMOST;
        }

        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);

        let z_order = if topmost {
            HWND_TOPMOST
        } else {
            HWND_BOTTOM
        };

        SetWindowPos(
            hwnd,
            z_order,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
    }
}
