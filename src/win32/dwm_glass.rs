use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, SendMessageW, SetWindowLongPtrW,
    SetWindowPos, TrackPopupMenu, GWL_EXSTYLE, GWL_STYLE, HTCAPTION, HWND_BOTTOM, HWND_TOPMOST,
    MF_SEPARATOR, MF_STRING, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NCLBUTTONDOWN, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

pub const DWMWCP_ROUND: u32 = 2;

#[repr(C)]
pub struct Margins {
    pub cx_left_width: i32,
    pub cx_right_width: i32,
    pub cy_top_height: i32,
    pub cy_bottom_height: i32,
}

#[repr(C)]
struct AccentPolicy {
    accent_state: u32,
    accent_flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

type PfnDwmExtendFrameIntoClientArea = unsafe extern "system" fn(HWND, *const Margins) -> i32;
type PfnSetWindowCompositionAttribute =
    unsafe extern "system" fn(HWND, *mut WindowCompositionAttributeData) -> i32;

/// Strips all window caption, system menu, border styles, and sets clean WS_POPUP.
/// Applies true DWM Acrylic blur behind with rounded corners.
pub fn apply_acrylic_glass_and_strip_chrome(hwnd: HWND, topmost: bool) {
    unsafe {
        // 1. Strip all borders, title bar, maximize/minimize/close buttons
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as isize);

        let mut ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW;
        if topmost {
            ex_style |= WS_EX_TOPMOST;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);

        let z_order = if topmost { HWND_TOPMOST } else { HWND_BOTTOM };
        SetWindowPos(
            hwnd,
            z_order,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
        );

        // 2. Extend DWM frame into client area
        let dwm_module = LoadLibraryA(c"dwmapi.dll".as_ptr() as *const u8);
        if dwm_module != 0 {
            let proc = GetProcAddress(
                dwm_module,
                c"DwmExtendFrameIntoClientArea".as_ptr() as *const u8,
            );
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

        // 3. Rounded corner preference
        let corner = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        // 4. SetWindowCompositionAttribute with ACCENT_ENABLE_ACRYLICBLURBEHIND
        let user32_module = LoadLibraryA(c"user32.dll".as_ptr() as *const u8);
        if user32_module != 0 {
            let proc = GetProcAddress(
                user32_module,
                c"SetWindowCompositionAttribute".as_ptr() as *const u8,
            );
            if let Some(func) = proc {
                let set_composition: PfnSetWindowCompositionAttribute = std::mem::transmute(func);
                let mut policy = AccentPolicy {
                    accent_state: 4,               // ACCENT_ENABLE_ACRYLICBLURBEHIND
                    accent_flags: 2,               // Draw all borders
                    gradient_color: 0xB8111318,    // 72% opacity dark tint
                    animation_id: 0,
                };
                let mut data = WindowCompositionAttributeData {
                    attribute: 19, // WCA_ACCENT_POLICY
                    data: &mut policy as *mut _ as *mut c_void,
                    size: std::mem::size_of::<AccentPolicy>(),
                };
                set_composition(hwnd, &mut data);
            }
        }
    }
}

/// Allows borderless windows to be dragged by clicking any empty region.
pub fn start_window_drag(hwnd: HWND) {
    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
    }
}

/// Sets position and size cleanly without triggering layout recursion.
pub fn set_window_bounds(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        SetWindowPos(
            hwnd,
            0,
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuChoice {
    ToggleNotch = 1001,
    ToggleWidgets = 1002,
    ToggleDock = 1003,
    ResetPositions = 1004,
    OpenConfigFolder = 1005,
    ReloadScripts = 1006,
    Quit = 1007,
    EditScripts = 1008,
    RegenerateWallpaper = 1009,
    ShowAllModules = 1010,
}

fn to_wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn show_native_context_menu(hwnd: HWND, screen_x: i32, screen_y: i32) -> Option<ContextMenuChoice> {
    unsafe {
        let menu = CreatePopupMenu();
        if menu == 0 {
            return None;
        }

        let show_all_label = to_wide("Show All Modules");
        let notch_label = to_wide("Toggle Top Notch");
        let widgets_label = to_wide("Toggle Desktop Widgets");
        let dock_label = to_wide("Toggle Glass Dock");
        let wallpaper_label = to_wide("Regenerate Studio Wallpaper");
        let edit_scripts_label = to_wide("Edit Scripts (%APPDATA%\\OxideShell\\scripts)");
        let reset_label = to_wide("Reset All Positions");
        let open_label = to_wide("Open Config Folder");
        let reload_label = to_wide("Reload Settings & Scripts");
        let quit_label = to_wide("Quit OxideShell");

        AppendMenuW(menu, MF_STRING, ContextMenuChoice::ShowAllModules as usize, show_all_label.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::ToggleNotch as usize, notch_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::ToggleWidgets as usize, widgets_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::ToggleDock as usize, dock_label.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::RegenerateWallpaper as usize, wallpaper_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::EditScripts as usize, edit_scripts_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::ReloadScripts as usize, reload_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::ResetPositions as usize, reset_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::OpenConfigFolder as usize, open_label.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, ContextMenuChoice::Quit as usize, quit_label.as_ptr());

        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            screen_x,
            screen_y,
            0,
            hwnd,
            std::ptr::null(),
        );

        DestroyMenu(menu);

        match cmd {
            1001 => Some(ContextMenuChoice::ToggleNotch),
            1002 => Some(ContextMenuChoice::ToggleWidgets),
            1003 => Some(ContextMenuChoice::ToggleDock),
            1004 => Some(ContextMenuChoice::ResetPositions),
            1005 => Some(ContextMenuChoice::OpenConfigFolder),
            1006 => Some(ContextMenuChoice::ReloadScripts),
            1007 => Some(ContextMenuChoice::Quit),
            1008 => Some(ContextMenuChoice::EditScripts),
            1009 => Some(ContextMenuChoice::RegenerateWallpaper),
            1010 => Some(ContextMenuChoice::ShowAllModules),
            _ => None,
        }
    }
}
