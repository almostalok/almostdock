use rhai::{Array, Dynamic, Engine, Map, Scope, AST};
use std::collections::HashMap;
use std::fs::create_dir_all;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::config::OxideConfig;
use crate::modules::wallpaper::WallpaperGenerator;
use crate::win32::telemetry::{get_process_memory, trim_working_set, SystemCpuSampler};

pub struct ScriptEngine {
    engine: Engine,
    asts: HashMap<String, AST>,
    last_modified: HashMap<String, SystemTime>,
    _cpu_sampler: Arc<Mutex<SystemCpuSampler>>,
    pub pending_toggle_launcher: Arc<Mutex<bool>>,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        let cpu_sampler = Arc::new(Mutex::new(SystemCpuSampler::new()));
        let cpu_clone = cpu_sampler.clone();
        let pending_launcher = Arc::new(Mutex::new(false));
        let pending_launcher_clone = pending_launcher.clone();

        // Register Native Rust APIs into Rhai
        engine.register_fn("exec_bg", |cmd: &str| {
            let _ = Command::new("cmd")
                .args(["/c", "start", "", cmd])
                .spawn();
        });

        engine.register_fn("exec", |cmd: &str| -> String {
            if let Ok(output) = Command::new("cmd").args(["/c", cmd]).output() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                String::new()
            }
        });

        engine.register_fn("sys_ram_mb", || -> f64 {
            get_process_memory().private_usage_mb()
        });

        engine.register_fn("sys_cpu_pct", move || -> f64 {
            if let Ok(mut sampler) = cpu_clone.lock() {
                sampler.sample() as f64
            } else {
                0.0
            }
        });

        engine.register_fn("trim_ram", || {
            trim_working_set();
        });

        engine.register_fn("set_wallpaper", |_preset: &str| {
            let gen = WallpaperGenerator::new(1920, 1080);
            let _ = gen.generate_and_apply();
        });

        engine.register_fn("toggle_module", move |name: &str| {
            if name == "launcher" {
                if let Ok(mut flag) = pending_launcher_clone.lock() {
                    *flag = true;
                }
            }
        });

        engine.register_fn("notify", |title: &str, body: &str| {
            println!("[Notification] {}: {}", title, body);
        });

        engine.register_fn("read_file", |path: &str| -> String {
            std::fs::read_to_string(path).unwrap_or_default()
        });

        engine.register_fn("write_file", |path: &str, text: &str| -> bool {
            std::fs::write(path, text).is_ok()
        });

        engine.register_fn("env", |key: &str| -> String {
            std::env::var(key).unwrap_or_default()
        });

        let mut se = Self {
            engine,
            asts: HashMap::new(),
            last_modified: HashMap::new(),
            _cpu_sampler: cpu_sampler,
            pending_toggle_launcher: pending_launcher,
        };

        se.ensure_default_scripts();
        se.load_all_scripts();
        se
    }

    pub fn ensure_default_scripts(&self) {
        let scripts_dir = OxideConfig::scripts_dir();
        if !scripts_dir.exists() {
            let _ = create_dir_all(&scripts_dir);
        }

        let dock_path = scripts_dir.join("dock_actions.rhai");
        if !dock_path.exists() {
            let dock_rhai = r#"
fn get_dock_items() {
    [
        #{ id: "explorer", name: "Explorer", icon: "📁", badge: "" },
        #{ id: "terminal", name: "Terminal", icon: "💻", badge: "zsh" },
        #{ id: "browser",  name: "Browser",  icon: "🌐", badge: "" },
        #{ id: "settings", name: "Settings", icon: "⚙️", badge: "" },
        #{ id: "launcher", name: "Apps",     icon: "🚀", badge: "" },
    ]
}

fn on_click(app_id) {
    if app_id == "explorer" {
        exec_bg("explorer.exe");
    } else if app_id == "terminal" {
        exec_bg("wt.exe");
    } else if app_id == "browser" {
        exec_bg("msedge.exe");
    } else if app_id == "settings" {
        exec_bg("cmd /c start ms-settings:");
    } else if app_id == "launcher" {
        toggle_module("launcher");
    }
}
"#;
            let _ = std::fs::write(&dock_path, dock_rhai.trim());
        }

        let widgets_path = scripts_dir.join("widgets.rhai");
        if !widgets_path.exists() {
            let widgets_rhai = r#"
fn on_tick() {
    let ram = sys_ram_mb();
    let cpu = sys_cpu_pct();
    [
        #{
            type: "clock",
            title: "10:42 AM",
            subtitle: "Friday, Nov 17",
        },
        #{
            type: "vitals",
            title: "System Vitals",
            ram_text: `RAM: ${ram.to_int()} MB`,
            cpu_text: `CPU: ${cpu.to_int()}%`,
            ram_pct: ram / 16384.0 * 100.0,
            cpu_pct: cpu,
        },
        #{
            type: "quick_launch",
            title: "Developer Tools",
            items: [
                #{ id: "term", label: "Terminal", cmd: "wt.exe" },
                #{ id: "code", label: "VS Code", cmd: "code ." },
                #{ id: "taskmgr", label: "Task Manager", cmd: "taskmgr.exe" },
            ],
        },
        #{
            type: "tasks",
            title: "Today's Tasks",
            tasks: [
                #{ id: "t1", text: "Review memory telemetry (< 14MB)", done: true },
                #{ id: "t2", text: "Tune squircle continuous curvature", done: false },
                #{ id: "t3", text: "Verify Acrylic blur compositing", done: false },
            ],
        },
    ]
}

fn on_click(widget_id, item_id) {
    if item_id == "term" {
        exec_bg("wt.exe");
    } else if item_id == "code" {
        exec_bg("code .");
    } else if item_id == "taskmgr" {
        exec_bg("taskmgr.exe");
    }
}
"#;
            let _ = std::fs::write(&widgets_path, widgets_rhai.trim());
        }

        let notch_path = scripts_dir.join("notch_modules.rhai");
        if !notch_path.exists() {
            let notch_rhai = r#"
fn get_notch_state() {
    #{
        pill_text: "OxideShell",
        status: "ready",
        track_title: "Starboy",
        track_artist: "The Weeknd, Daft Punk",
        is_playing: true,
    }
}

fn on_action(action) {
    if action == "play_pause" {
        // media toggle
    } else if action == "next" {
        // next track
    } else if action == "prev" {
        // prev track
    }
}
"#;
            let _ = std::fs::write(&notch_path, notch_rhai.trim());
        }
    }

    pub fn load_script(&mut self, name: &str, path: &Path) -> bool {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(ast) = self.engine.compile(&content) {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(mtime) = meta.modified() {
                        self.last_modified.insert(name.to_string(), mtime);
                    }
                }
                self.asts.insert(name.to_string(), ast);
                return true;
            }
        }
        false
    }

    pub fn load_all_scripts(&mut self) {
        let scripts_dir = OxideConfig::scripts_dir();
        for script_name in &["dock_actions", "widgets", "notch_modules"] {
            let path = scripts_dir.join(format!("{}.rhai", script_name));
            if path.exists() {
                self.load_script(script_name, &path);
            }
        }
    }

    pub fn check_reload(&mut self) -> bool {
        let mut reloaded = false;
        let scripts_dir = OxideConfig::scripts_dir();

        for script_name in ["dock_actions", "widgets", "notch_modules"] {
            let path = scripts_dir.join(format!("{}.rhai", script_name));
            if path.exists() {
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        let need_reload = self
                            .last_modified
                            .get(script_name)
                            .is_none_or(|last| mtime > *last);
                        if need_reload && self.load_script(script_name, &path) {
                            reloaded = true;
                            println!("[OxideShell] Live hot-reloaded: {}.rhai", script_name);
                        }
                    }
                }
            }
        }

        reloaded
    }

    pub fn call_dock_items(&self) -> Array {
        if let Some(ast) = self.asts.get("dock_actions") {
            let mut scope = Scope::new();
            if let Ok(items) = self.engine.call_fn::<Array>(&mut scope, ast, "get_dock_items", ()) {
                return items;
            }
        }
        Array::new()
    }

    pub fn call_dock_click(&self, app_id: &str) {
        if let Some(ast) = self.asts.get("dock_actions") {
            let mut scope = Scope::new();
            let _ = self.engine.call_fn::<Dynamic>(&mut scope, ast, "on_click", (app_id.to_string(),));
        }
    }

    pub fn call_widgets_tick(&self) -> Array {
        if let Some(ast) = self.asts.get("widgets") {
            let mut scope = Scope::new();
            if let Ok(widgets) = self.engine.call_fn::<Array>(&mut scope, ast, "on_tick", ()) {
                return widgets;
            }
        }
        Array::new()
    }

    pub fn call_widget_click(&self, widget_id: &str, item_id: &str) {
        if let Some(ast) = self.asts.get("widgets") {
            let mut scope = Scope::new();
            let _ = self.engine.call_fn::<Dynamic>(
                &mut scope,
                ast,
                "on_click",
                (widget_id.to_string(), item_id.to_string()),
            );
        }
    }

    pub fn call_notch_state(&self) -> Map {
        if let Some(ast) = self.asts.get("notch_modules") {
            let mut scope = Scope::new();
            if let Ok(state) = self.engine.call_fn::<Map>(&mut scope, ast, "get_notch_state", ()) {
                return state;
            }
        }
        Map::new()
    }

    pub fn call_notch_action(&self, action: &str) {
        if let Some(ast) = self.asts.get("notch_modules") {
            let mut scope = Scope::new();
            let _ = self.engine.call_fn::<Dynamic>(&mut scope, ast, "on_action", (action.to_string(),));
        }
    }
}
