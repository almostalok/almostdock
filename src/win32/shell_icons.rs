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
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
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

#[allow(non_snake_case)]
type PfnPrivateExtractIconsW = unsafe extern "system" fn(
    szFileName: *const u16,
    nIconIndex: i32,
    cxIcon: i32,
    cyIcon: i32,
    phicon: *mut HICON,
    piconid: *mut u32,
    nIcons: u32,
    flags: u32,
) -> u32;

#[derive(Clone, Debug)]
pub struct AppInfo {
    pub name: String,
    pub path: PathBuf,
    pub icon: Option<Arc<Vec<u8>>>, // 48x48 RGBA premultiplied pixels
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
            if let Ok(lock) = self.cache.lock() {
                if let Some(icon) = lock.get(&key) {
                    return Some(icon.clone());
                }
            }
        }

        let resolved = resolve_app_path(path);
        let icon_data = extract_icon_48_rgba(&resolved)
            .or_else(|| extract_icon_48_rgba(path))
            .or_else(|| Some(generate_vector_fallback_icon(&key, 48)));

        if let Some(data) = icon_data {
            let arc_icon = Arc::new(data);
            if let Ok(mut lock) = self.cache.lock() {
                lock.insert(key, arc_icon.clone());
            }
            return Some(arc_icon);
        }

        None
    }
}

/// Resolves well-known system executable names to absolute disk paths
pub fn resolve_app_path(path: &Path) -> PathBuf {
    if path.is_absolute() && path.exists() {
        return path.to_path_buf();
    }

    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("").to_lowercase();

    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let prog_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
    let prog_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();

    match name.as_str() {
        "explorer" | "explorer.exe" => {
            let p = PathBuf::from(&sys_root).join("explorer.exe");
            if p.exists() { return p; }
        }
        "taskmgr" | "taskmgr.exe" => {
            let p = PathBuf::from(&sys_root).join("System32").join("Taskmgr.exe");
            if p.exists() { return p; }
        }
        "cmd" | "cmd.exe" => {
            let p = PathBuf::from(&sys_root).join("System32").join("cmd.exe");
            if p.exists() { return p; }
        }
        "wt" | "wt.exe" | "terminal" => {
            let p = PathBuf::from(&local_app_data).join("Microsoft").join("WindowsApps").join("wt.exe");
            if p.exists() { return p; }
            let p_cmd = PathBuf::from(&sys_root).join("System32").join("cmd.exe");
            if p_cmd.exists() { return p_cmd; }
        }
        "browser" | "msedge" | "msedge.exe" | "edge" => {
            let p_edge = PathBuf::from(&prog_files_x86).join("Microsoft").join("Edge").join("Application").join("msedge.exe");
            if p_edge.exists() { return p_edge; }
            let p_chrome = PathBuf::from(&prog_files).join("Google").join("Chrome").join("Application").join("chrome.exe");
            if p_chrome.exists() { return p_chrome; }
        }
        "code" | "code.exe" | "vscode" => {
            let p_code = PathBuf::from(&local_app_data).join("Programs").join("Microsoft VS Code").join("Code.exe");
            if p_code.exists() { return p_code; }
            let p_code2 = PathBuf::from(&prog_files).join("Microsoft VS Code").join("Code.exe");
            if p_code2.exists() { return p_code2; }
        }
        _ => {}
    }

    path.to_path_buf()
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

    // Append standard built-in productivity tools
    let built_ins = [
        ("File Explorer", resolve_app_path(Path::new("explorer.exe"))),
        ("Terminal", resolve_app_path(Path::new("wt.exe"))),
        ("Web Browser", resolve_app_path(Path::new("msedge.exe"))),
        ("Task Manager", resolve_app_path(Path::new("taskmgr.exe"))),
        ("VS Code", resolve_app_path(Path::new("Code.exe"))),
    ];

    for (name, path) in built_ins {
        if path.exists() {
            apps.push(AppInfo {
                name: name.to_string(),
                path,
                icon: None,
            });
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

/// Extracts real 48x48 RGBA icon using PrivateExtractIconsW with SHGetFileInfoW fallback
pub fn extract_icon_48_rgba(path: &Path) -> Option<Vec<u8>> {
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // 1. Try PrivateExtractIconsW from user32.dll at exact 48x48
    let mut extracted_hicon: HICON = 0;
    unsafe {
        let user32 = LoadLibraryA(c"user32.dll".as_ptr() as *const u8);
        if user32 != 0 {
            let proc = GetProcAddress(user32, c"PrivateExtractIconsW".as_ptr() as *const u8);
            if let Some(func) = proc {
                let private_extract: PfnPrivateExtractIconsW = std::mem::transmute(func);
                let mut icon_id = 0u32;
                let count = private_extract(
                    wide_path.as_ptr(),
                    0,
                    48,
                    48,
                    &mut extracted_hicon,
                    &mut icon_id,
                    1,
                    0,
                );
                if count == 0 {
                    extracted_hicon = 0;
                }
            }
        }
    }

    // 2. Fallback to SHGetFileInfoW if PrivateExtractIconsW failed
    if extracted_hicon == 0 {
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
        if res != 0 && shfi.hIcon != 0 {
            extracted_hicon = shfi.hIcon;
        }
    }

    if extracted_hicon == 0 {
        return None;
    }

    // 3. Extract DIB bits from HICON
    let mut icon_info: ICONINFO = unsafe { std::mem::zeroed() };
    if unsafe { GetIconInfo(extracted_hicon, &mut icon_info) } == 0 {
        unsafe { DestroyIcon(extracted_hicon) };
        return None;
    }

    let hbm_color = icon_info.hbmColor;
    let hbm_mask = icon_info.hbmMask;

    let hdc = unsafe { CreateCompatibleDC(0 as HWND) };
    if hdc == 0 {
        unsafe {
            if hbm_color != 0 { DeleteObject(hbm_color); }
            if hbm_mask != 0 { DeleteObject(hbm_mask); }
            DestroyIcon(extracted_hicon);
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
    bmi.bmiHeader.biHeight = -(height as i32); // Top-down
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
        if hbm_color != 0 { DeleteObject(hbm_color); }
        if hbm_mask != 0 { DeleteObject(hbm_mask); }
        DestroyIcon(extracted_hicon);
    }

    if scan_lines == 0 {
        return None;
    }

    // Convert BGRA to RGBA premultiplied
    let mut raw_rgba = vec![0u8; width * height * 4];
    let mut has_alpha = false;

    for i in 0..(width * height) {
        let b = bgra_pixels[i * 4];
        let g = bgra_pixels[i * 4 + 1];
        let r = bgra_pixels[i * 4 + 2];
        let a = bgra_pixels[i * 4 + 3];

        if a > 0 {
            has_alpha = true;
        }

        raw_rgba[i * 4] = r;
        raw_rgba[i * 4 + 1] = g;
        raw_rgba[i * 4 + 2] = b;
        raw_rgba[i * 4 + 3] = a;
    }

    if !has_alpha {
        for i in 0..(width * height) {
            raw_rgba[i * 4 + 3] = 255;
        }
    }

    // Scale to exact 48x48 if dimensions differ
    if width == 48 && height == 48 {
        Some(raw_rgba)
    } else {
        Some(bilinear_scale_rgba(&raw_rgba, width, height, 48, 48))
    }
}

/// Bilinear interpolation scaler for 32-bit RGBA pixel buffers
pub fn bilinear_scale_rgba(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Vec<u8> {
    let mut dst = vec![0u8; dst_w * dst_h * 4];
    let x_ratio = if dst_w > 1 { (src_w - 1) as f32 / (dst_w - 1) as f32 } else { 0.0 };
    let y_ratio = if dst_h > 1 { (src_h - 1) as f32 / (dst_h - 1) as f32 } else { 0.0 };

    for y in 0..dst_h {
        let src_y = (y as f32 * y_ratio).clamp(0.0, (src_h - 1) as f32);
        let y_low = src_y.floor() as usize;
        let y_high = (y_low + 1).min(src_h - 1);
        let y_diff = src_y - y_low as f32;

        for x in 0..dst_w {
            let src_x = (x as f32 * x_ratio).clamp(0.0, (src_w - 1) as f32);
            let x_low = src_x.floor() as usize;
            let x_high = (x_low + 1).min(src_w - 1);
            let x_diff = src_x - x_low as f32;

            let d_idx = (y * dst_w + x) * 4;

            for c in 0..4 {
                let a = src[(y_low * src_w + x_low) * 4 + c] as f32;
                let b = src[(y_low * src_w + x_high) * 4 + c] as f32;
                let d = src[(y_high * src_w + x_low) * 4 + c] as f32;
                let e = src[(y_high * src_w + x_high) * 4 + c] as f32;

                let val = a * (1.0 - x_diff) * (1.0 - y_diff)
                    + b * x_diff * (1.0 - y_diff)
                    + d * (1.0 - x_diff) * y_diff
                    + e * x_diff * y_diff;

                dst[d_idx + c] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    dst
}

/// Generates a clean macOS-style vector squircle fallback icon when an executable has no embedded icon
pub fn generate_vector_fallback_icon(key: &str, size: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; size * size * 4];
    let lower = key.to_lowercase();

    // Select premium dark aesthetic theme
    let (bg_r, bg_g, bg_b) = if lower.contains("explorer") || lower.contains("folder") {
        (56u8, 128u8, 240u8)
    } else if lower.contains("terminal") || lower.contains("cmd") || lower.contains("wt") {
        (30u8, 32u8, 38u8)
    } else if lower.contains("browser") || lower.contains("edge") || lower.contains("chrome") {
        (40u8, 160u8, 220u8)
    } else if lower.contains("code") {
        (35u8, 140u8, 230u8)
    } else if lower.contains("taskmgr") {
        (45u8, 180u8, 120u8)
    } else {
        (130u8, 80u8, 220u8)
    };

    let radius = size as f32 * 0.24;
    let center = size as f32 * 0.5;

    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - center).abs();
            let dy = (y as f32 - center).abs();
            let half = center - radius;

            let alpha = if dx <= half && dy <= half {
                255u8
            } else if dx > half && dy > half {
                let corner_dist = ((dx - half).powi(2) + (dy - half).powi(2)).sqrt();
                if corner_dist <= radius - 0.5 {
                    255u8
                } else if corner_dist <= radius + 0.5 {
                    ((radius + 0.5 - corner_dist) * 255.0) as u8
                } else {
                    0u8
                }
            } else if dx <= center && dy <= center {
                255u8
            } else {
                0u8
            };

            if alpha > 0 {
                let idx = (y * size + x) * 4;
                pixels[idx] = bg_r;
                pixels[idx + 1] = bg_g;
                pixels[idx + 2] = bg_b;
                pixels[idx + 3] = alpha;
            }
        }
    }

    pixels
}
