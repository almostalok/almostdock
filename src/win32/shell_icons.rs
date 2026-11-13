use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

pub const SHGFI_ICON: u32 = 0x000000100;
pub const SHGFI_LARGEICON: u32 = 0x000000000;

#[repr(C)]
#[allow(non_snake_case)]
pub struct SHFILEINFOW {
    pub hIcon: HICON,
    pub iIcon: i32,
    pub dwAttributes: u32,
    pub szDisplayName: [u16; 260],
    pub szTypeName: [u16; 80],
}

#[link(name = "shell32")]
extern "system" {
    fn SHGetFileInfoW(
        pszPath: *const u16,
        dwFileAttributes: u32,
        psfi: *mut SHFILEINFOW,
        cbFileInfo: u32,
        uFlags: u32,
    ) -> usize;
}

#[derive(Clone, Debug)]
pub struct AppInfo {
    pub name: String,
    pub path: PathBuf,
    pub icon: Option<Arc<Vec<u8>>>, // 64x64 RGBA premultiplied pixels
}

pub struct IconCache {
    cache: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

impl Default for IconCache {
    fn default() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl IconCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_extract(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        let key = path.to_string_lossy().to_string();
        {
            let lock = self.cache.lock().ok()?;
            if let Some(icon) = lock.get(&key) {
                return Some(icon.clone());
            }
        }

        let icon_data = extract_hicon_rgba(path)?;
        let arc_icon = Arc::new(icon_data);
        if let Ok(mut lock) = self.cache.lock() {
            lock.insert(key, arc_icon.clone());
        }
        Some(arc_icon)
    }
}

pub fn scan_start_menu_apps() -> Vec<AppInfo> {
    let mut apps = Vec::new();
    let mut paths_to_scan = Vec::new();

    if let Ok(prog_data) = std::env::var("ProgramData") {
        paths_to_scan.push(
            PathBuf::from(prog_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }

    if let Ok(app_data) = std::env::var("APPDATA") {
        paths_to_scan.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }

    for dir in paths_to_scan {
        if dir.exists() {
            scan_dir_recursive(&dir, &mut apps);
        }
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

fn scan_dir_recursive(dir: &Path, apps: &mut Vec<AppInfo>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir_recursive(&path, apps);
            } else if path.extension().and_then(OsStr::to_str) == Some("lnk") {
                let stem = path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or("App")
                    .to_string();

                // Skip uninstaller shortcuts
                let lower = stem.to_lowercase();
                if lower.contains("uninstall") || lower.contains("help") || lower.contains("read me") {
                    continue;
                }

                apps.push(AppInfo {
                    name: stem,
                    path: path.clone(),
                    icon: None,
                });
            }
        }
    }
}

pub fn extract_hicon_rgba(path: &Path) -> Option<Vec<u8>> {
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut shfi: SHFILEINFOW = unsafe { std::mem::zeroed() };
    let res = unsafe {
        SHGetFileInfoW(
            wide_path.as_ptr(),
            0,
            &mut shfi,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };

    if res == 0 || shfi.hIcon == 0 {
        return None;
    }

    let hicon = shfi.hIcon;
    let mut icon_info: ICONINFO = unsafe { std::mem::zeroed() };
    if unsafe { GetIconInfo(hicon, &mut icon_info) } == 0 {
        unsafe { DestroyIcon(hicon) };
        return None;
    }

    let hbm_color = icon_info.hbmColor;
    let hbm_mask = icon_info.hbmMask;

    let hdc = unsafe { CreateCompatibleDC(0 as HWND) };
    if hdc == 0 {
        unsafe {
            if hbm_color != 0 {
                DeleteObject(hbm_color);
            }
            if hbm_mask != 0 {
                DeleteObject(hbm_mask);
            }
            DestroyIcon(hicon);
        }
        return None;
    }

    let mut bm: BITMAP = unsafe { std::mem::zeroed() };
    let target_bmp = if hbm_color != 0 { hbm_color } else { hbm_mask };
    unsafe {
        GetObjectW(
            target_bmp,
            std::mem::size_of::<BITMAP>() as i32,
            &mut bm as *mut _ as *mut _,
        );
    }

    let width = bm.bmWidth.max(1) as usize;
    let height = if hbm_color != 0 {
        bm.bmHeight.max(1) as usize
    } else {
        (bm.bmHeight / 2).max(1) as usize
    };

    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width as i32;
    bmi.bmiHeader.biHeight = -(height as i32); // Top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    let mut bgra_pixels = vec![0u8; width * height * 4];
    let scan_lines = unsafe {
        GetDIBits(
            hdc,
            target_bmp,
            0,
            height as u32,
            bgra_pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };

    unsafe {
        DeleteDC(hdc);
        if hbm_color != 0 {
            DeleteObject(hbm_color);
        }
        if hbm_mask != 0 {
            DeleteObject(hbm_mask);
        }
        DestroyIcon(hicon);
    }

    if scan_lines == 0 {
        return None;
    }

    // Convert BGRA to RGBA premultiplied
    let mut rgba_pixels = vec![0u8; width * height * 4];
    let mut has_alpha = false;

    for i in 0..(width * height) {
        let b = bgra_pixels[i * 4];
        let g = bgra_pixels[i * 4 + 1];
        let r = bgra_pixels[i * 4 + 2];
        let a = bgra_pixels[i * 4 + 3];

        if a > 0 {
            has_alpha = true;
        }

        rgba_pixels[i * 4] = r;
        rgba_pixels[i * 4 + 1] = g;
        rgba_pixels[i * 4 + 2] = b;
        rgba_pixels[i * 4 + 3] = a;
    }

    // If icon was a 24-bit bitmap without an alpha channel, treat all pixels as opaque
    if !has_alpha {
        for i in 0..(width * height) {
            rgba_pixels[i * 4 + 3] = 255;
        }
    }

    Some(rgba_pixels)
}
