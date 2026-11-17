use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

pub fn rects_overlap(r1: &Rect, r2: &Rect) -> bool {
    !(r1.right <= r2.left || r1.left >= r2.right || r1.bottom <= r2.top || r1.top >= r2.bottom)
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenGeometry {
    pub screen_width: i32,
    pub screen_height: i32,
    pub work_left: i32,
    pub work_top: i32,
    pub work_right: i32,
    pub work_bottom: i32,
}

impl Default for ScreenGeometry {
    fn default() -> Self {
        Self {
            screen_width: 1920,
            screen_height: 1080,
            work_left: 0,
            work_top: 0,
            work_right: 1920,
            work_bottom: 1040,
        }
    }
}

impl ScreenGeometry {
    pub fn query_primary() -> Self {
        let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };

        let mut work_rect: RECT = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut work_rect as *mut _ as *mut _,
                0,
            )
        };

        if ok != 0 && sw > 0 && sh > 0 {
            Self {
                screen_width: sw,
                screen_height: sh,
                work_left: work_rect.left,
                work_top: work_rect.top,
                work_right: work_rect.right,
                work_bottom: work_rect.bottom,
            }
        } else if sw > 0 && sh > 0 {
            Self {
                screen_width: sw,
                screen_height: sh,
                work_left: 0,
                work_top: 0,
                work_right: sw,
                work_bottom: (sh - 40).max(100),
            }
        } else {
            Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowLayout {
    pub notch_rect: Rect,
    pub notch_expanded_rect: Rect,
    pub dock_rect: Rect,
    pub widget_rect: Rect,
    pub launcher_rect: Rect,
}

pub struct LayoutSolver;

impl LayoutSolver {
    pub fn solve(geo: &ScreenGeometry, dock_width: i32) -> WindowLayout {
        // 1. Top Notch (centered at top, y = 0)
        let notch_w = 220;
        let notch_h = 34;
        let notch_x = (geo.screen_width - notch_w) / 2;
        let notch_rect = Rect::from_xywh(notch_x, 0, notch_w, notch_h);

        // Expanded Notch (540 x 160 centered at y = 0)
        let notch_exp_w = 540;
        let notch_exp_h = 160;
        let notch_exp_x = (geo.screen_width - notch_exp_w) / 2;
        let notch_expanded_rect = Rect::from_xywh(notch_exp_x, 0, notch_exp_w, notch_exp_h);

        // 2. Desktop Widgets Container (docked on right edge: work_right - 320, y = 52, w = 300, h = 620)
        let widget_w = 300;
        let widget_h = 620;
        let widget_x = geo.work_right - 320;
        let widget_y = 52;
        let widget_rect = Rect::from_xywh(widget_x, widget_y, widget_w, widget_h);

        // 3. Bottom Dock (width = 680, height = 116, y = work_bottom - 116)
        let dock_w = dock_width.max(680);
        let dock_h = 116;
        let dock_x = (geo.screen_width - dock_w) / 2;
        let dock_y = geo.work_bottom - dock_h;
        let dock_rect = Rect::from_xywh(dock_x, dock_y, dock_w, dock_h);

        // 4. App Launcher (centered on screen: 720 x 480)
        let launcher_w = 720;
        let launcher_h = 480;
        let launcher_x = (geo.screen_width - launcher_w) / 2;
        let launcher_y = (geo.screen_height - launcher_h) / 2;
        let launcher_rect = Rect::from_xywh(launcher_x, launcher_y, launcher_w, launcher_h);

        WindowLayout {
            notch_rect,
            notch_expanded_rect,
            dock_rect,
            widget_rect,
            launcher_rect,
        }
    }
}
