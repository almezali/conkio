use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs, io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

pub const VERSION: &str = "1.0.2";
pub const APP_DIR_NAME: &str = "conky-manager";
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_ARCHIVE_FILES: usize = 4000;
pub const MAX_ARCHIVE_DEPTH: usize = 12;
pub const MAX_RECENT: usize = 24;

pub const ASSET_DIRS: &[&str] = &[
    "res", "img", "imgs", "images", "assets", "scripts", "fonts", "lua", "lib", "icons", "include",
    "config",
];
pub const ASSET_SUFFIXES: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".lua", ".ttf", ".otf", ".woff", ".woff2",
    ".sh",
];
pub const TRACKED_COMMANDS: &[&str] = &[
    "curl", "wget", "jq", "playerctl", "sensors", "nmcli", "mpstat", "iostat", "nvidia-smi",
    "amixer", "pactl", "wpctl", "awk", "bc", "python3", "python", "perl", "ruby", "lua", "ffmpeg",
    "convert", "notify-send", "dunstify", "acpi", "upower", "iw", "iwconfig", "htop", "iotop",
    "free", "df", "ping", "ip", "ss", "lscpu", "lsblk", "smartctl", "amdgpu_top", "rocm-smi",
];
pub const COLOR_SLOT_ORDER: &[&str] = &[
    "default_color",
    "color1",
    "color2",
    "color3",
    "color4",
    "color5",
    "color6",
    "color7",
    "color8",
    "color9",
    "color0",
    "color",
    "default_outline_color",
    "default_shade_color",
    "own_window_colour",
];
pub const ALIGNMENTS: [&str; 9] = [
    "top_left",
    "top_middle",
    "top_right",
    "middle_left",
    "middle_middle",
    "middle_right",
    "bottom_left",
    "bottom_middle",
    "bottom_right",
];

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}
pub fn conky_dir() -> PathBuf {
    home().join(".conky")
}
pub fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home().join(".config"))
        .join(APP_DIR_NAME)
}
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| home().join(".local/share"))
        .join(APP_DIR_NAME)
}
pub fn launch_dir() -> PathBuf {
    data_dir().join("launch")
}
pub fn json_path(name: &str) -> PathBuf {
    data_dir().join(name)
}
pub fn backup_root() -> PathBuf {
    app_dir().join("backups")
}
pub fn autostart_file() -> PathBuf {
    home().join(".config/autostart/conky-manager.desktop")
}

pub fn ensure_dirs() -> io::Result<()> {
    fs::create_dir_all(conky_dir())?;
    fs::create_dir_all(app_dir())?;
    fs::create_dir_all(data_dir())?;
    fs::create_dir_all(launch_dir())?;
    fs::create_dir_all(backup_root())?;
    fs::create_dir_all(app_dir().join("downloads"))
}

pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
pub fn save_json<T: Serialize>(path: &Path, obj: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let bytes = serde_json::to_vec_pretty(obj)?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(&bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map(|_| ()).map_err(|e| e.error)
}

pub fn which(name: &str) -> bool {
    env::var_os("PATH")
        .map(|p| {
            env::split_paths(&p).any(|dir| {
                let cand = dir.join(name);
                cand.is_file()
            })
        })
        .unwrap_or(false)
}
pub fn conky_available() -> bool {
    which("conky")
}
pub fn conky_version() -> String {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| {
        Command::new("conky")
            .arg("-v")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().next().map(str::to_owned))
            .unwrap_or_else(|| "Conky unavailable".into())
    }).clone()
}
pub fn read_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
pub fn resolve_path(p: &Path) -> PathBuf {
    let abs = fs::canonicalize(p).unwrap_or_else(|_| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            env::current_dir().unwrap_or_default().join(p)
        }
    });
    abs
}
pub fn theme_id(cfg: &Path) -> String {
    resolve_path(cfg).to_string_lossy().to_string()
}

pub fn normalize_folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| {
            c.is_alphabetic() || c.is_numeric() || matches!(c, '_' | '-' | '+' | '.') || c.is_whitespace()
        })
        .collect();
    let mut cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        cleaned = "conky-theme".into();
    }
    for suf in ["-main", "-master", "-gh-pages", "-devel"] {
        if let Some(stripped) = cleaned.strip_suffix(suf) {
            cleaned = stripped.to_string();
        }
    }
    let cleaned = cleaned.trim_matches(|c| c == '-' || c == '_').to_string();
    if cleaned.is_empty() {
        "conky-theme".into()
    } else {
        cleaned
    }
}

pub fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    for e in WalkDir::new(src).into_iter().filter_map(Result::ok) {
        let rel = e.path().strip_prefix(src).unwrap();
        let target = dst.join(rel);
        if e.file_type().is_symlink() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Refusing symlink in theme: {}", e.path().display())));
        }
        if e.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if e.file_type().is_file() {
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

pub fn unique_destination(parent: &Path, name: &str) -> PathBuf {
    let base = parent.join(name);
    if !base.exists() {
        return base;
    }
    for i in 2..10000 {
        let p = parent.join(format!("{}-{}", name, i));
        if !p.exists() {
            return p;
        }
    }
    parent.join(format!("{}-{}", name, timestamp()))
}
pub fn unique_dest(name: &str) -> PathBuf {
    unique_destination(&conky_dir(), name)
}

pub fn detect_desktop_environment() -> String {
    let combined = format!(
        "{} {} {}",
        env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        env::var("DESKTOP_SESSION").unwrap_or_default(),
        env::var("XDG_SESSION_DESKTOP").unwrap_or_default()
    )
    .to_lowercase();
    for (key, val) in [
        ("hyprland", "hyprland"),
        ("niri", "niri"),
        ("sway", "sway"),
        ("cosmic", "cosmic"),
        ("kde", "kde"),
        ("plasma", "kde"),
        ("gnome", "gnome"),
        ("xfce", "xfce"),
        ("cinnamon", "cinnamon"),
        ("mate", "mate"),
        ("lxqt", "lxqt"),
        ("lxde", "lxde"),
    ] {
        if combined.contains(key) {
            return val.into();
        }
    }
    "generic".into()
}
pub fn display_server_kind() -> String {
    let sess = env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase();
    if sess == "wayland" {
        if env::var("WAYLAND_DISPLAY").is_ok() && env::var("DISPLAY").is_ok() {
            return "xwayland".into();
        }
        return "wayland".into();
    }
    if sess == "x11" || env::var("DISPLAY").is_ok() {
        return "x11".into();
    }
    if sess.is_empty() {
        "unknown".into()
    } else {
        sess
    }
}
pub fn compositor_label() -> String {
    format!(
        "{} · {} · {}",
        detect_desktop_environment().to_uppercase(),
        env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into()),
        display_server_kind()
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub light_mode: bool,
    #[serde(default)]
    pub theme_dir: Option<String>,
    #[serde(default = "default_layer")]
    pub layer: String,
    #[serde(default = "default_window_type")]
    pub window_type: String,
    #[serde(default = "default_smoothness")]
    pub smoothness: String,
    #[serde(default = "default_nice")]
    pub nice_level: i32,
    #[serde(default = "default_max_instances")]
    pub max_instances: i32,
    #[serde(default = "default_healthcheck")]
    pub healthcheck_seconds: f64,
    #[serde(default = "default_true")]
    pub auto_optimize: bool,
    #[serde(default = "default_true")]
    pub full_visual_launch: bool,
    #[serde(default = "default_true")]
    pub create_compat_symlinks: bool,
    #[serde(default = "default_true")]
    pub remember_position: bool,
    #[serde(default = "default_true")]
    pub remember_colors: bool,
    #[serde(default)]
    pub scale_aware_position: bool,
    #[serde(default)]
    pub custom_theme_dirs: Vec<String>,
    #[serde(default)]
    pub login_autostart: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default = "default_preset")]
    pub desktop_preset: String,
    #[serde(default = "default_preview_seconds")]
    pub preview_seconds: i32,
    #[serde(default = "default_ui_theme")]
    pub ui_theme: String,
}
fn default_layer() -> String {
    "above".into()
}
fn default_window_type() -> String {
    "dock".into()
}
fn default_smoothness() -> String {
    "balanced".into()
}
fn default_nice() -> i32 {
    10
}
fn default_max_instances() -> i32 {
    8
}
fn default_healthcheck() -> f64 {
    2.0
}
fn default_true() -> bool {
    true
}
fn default_preset() -> String {
    "auto".into()
}
fn default_preview_seconds() -> i32 { 5 }
fn default_ui_theme() -> String { "auto".into() }
impl Default for Settings {
    fn default() -> Self {
        Self {
            light_mode: false,
            theme_dir: None,
            layer: default_layer(),
            window_type: default_window_type(),
            smoothness: default_smoothness(),
            nice_level: default_nice(),
            max_instances: default_max_instances(),
            healthcheck_seconds: default_healthcheck(),
            auto_optimize: true,
            full_visual_launch: true,
            create_compat_symlinks: true,
            remember_position: true,
            remember_colors: true,
            scale_aware_position: false,
            custom_theme_dirs: Vec::new(),
            login_autostart: false,
            minimize_to_tray: true,
            desktop_preset: default_preset(),
            preview_seconds: default_preview_seconds(),
            ui_theme: default_ui_theme(),
        }
    }
}
pub fn load_settings() -> Settings {
    let mut s: Settings = load_json(&json_path("settings.json"));
    if s.layer != "above" && s.layer != "below" { s.layer = default_layer(); }
    if !["dock", "desktop", "normal", "override"].contains(&s.window_type.as_str()) { s.window_type = default_window_type(); }
    if !["ultra", "balanced", "performance"].contains(&s.smoothness.as_str()) { s.smoothness = default_smoothness(); }
    if !["auto", "light", "dark"].contains(&s.ui_theme.as_str()) { s.ui_theme = default_ui_theme(); }
    s.nice_level = s.nice_level.clamp(-5, 19);
    s.max_instances = s.max_instances.clamp(1, 16);
    if !s.healthcheck_seconds.is_finite() { s.healthcheck_seconds = 2.0; }
    s.healthcheck_seconds = s.healthcheck_seconds.clamp(0.4, 5.0);
    s.preview_seconds = s.preview_seconds.clamp(2, 60);
    s.custom_theme_dirs = s.custom_theme_dirs.iter().map(|d| d.trim()).filter(|d| !d.is_empty()).map(String::from).collect();
    if s.light_mode { s.ui_theme = "light".into(); }
    s
}
pub fn save_settings(s: &Settings) -> io::Result<()> {
    save_json(&json_path("settings.json"), s)
}

pub fn resolve_runtime_settings(user: Settings) -> Settings {
    let mut merged = user;
    let mut de = detect_desktop_environment();
    if merged.desktop_preset != "auto" && !merged.desktop_preset.is_empty() {
        de = merged.desktop_preset.clone();
    }
    let (layer, wtype) = match de.as_str() {
        "kde" => ("above", "dock"),
        "hyprland" | "niri" | "sway" => ("above", "override"),
        "gnome" | "xfce" | "cinnamon" | "mate" | "lxqt" | "lxde" | "cosmic" => ("below", "desktop"),
        _ => ("above", "dock"),
    };
    if merged.desktop_preset == "auto" {
        if merged.layer.is_empty() {
            merged.layer = layer.into();
        }
        if merged.window_type.is_empty() {
            merged.window_type = wtype.into();
        }
    }
    merged
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Position {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub alignment: String,
    #[serde(default)]
    pub gap_x: i32,
    #[serde(default)]
    pub gap_y: i32,
    #[serde(default)]
    pub monitor: String,
    #[serde(default)]
    pub xinerama_head: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PositionsFile {
    #[serde(default)]
    pub default: Position,
    #[serde(default)]
    pub themes: BTreeMap<String, Position>,
}
pub fn default_positions() -> PositionsFile {
    PositionsFile {
        default: Position {
            enabled: true,
            alignment: "top_left".into(),
            gap_x: 30,
            gap_y: 50,
            ..Default::default()
        },
        themes: BTreeMap::new(),
    }
}
pub fn load_positions() -> PositionsFile {
    let mut data = default_positions();
    let loaded: PositionsFile = load_json(&json_path("positions.json"));
    if !loaded.themes.is_empty() || loaded.default.alignment != "" {
        data = loaded;
    }
    if data.default.alignment.is_empty() {
        data.default.alignment = "top_left".into();
    }
    data
}
pub fn save_positions(data: &PositionsFile) {
    let _ = save_json(&json_path("positions.json"), data);
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ColorOverride {
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub smart_lua: bool,
    #[serde(default)]
    pub enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ColorsFile {
    #[serde(default)]
    pub themes: BTreeMap<String, ColorOverride>,
}
pub fn load_colors() -> ColorsFile {
    load_json(&json_path("colors.json"))
}
pub fn save_colors(data: &ColorsFile) {
    let _ = save_json(&json_path("colors.json"), data);
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ThemeLibrary {
    #[serde(default)]
    pub favorites: BTreeMap<String, bool>,
    #[serde(default)]
    pub recent: Vec<RecentEntry>,
    #[serde(default)]
    pub custom_dirs: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RecentEntry {
    pub path: String,
    pub at: i64,
}
pub fn load_library() -> ThemeLibrary {
    load_json(&json_path("library.json"))
}
pub fn save_library(lib: &ThemeLibrary) {
    let _ = save_json(&json_path("library.json"), lib);
}
pub fn is_favorite(cfg: &Path) -> bool {
    load_library()
        .favorites
        .get(&theme_id(cfg))
        .copied()
        .unwrap_or(false)
}
pub fn set_favorite(cfg: &Path, on: bool) {
    let mut lib = load_library();
    let id = theme_id(cfg);
    if on {
        lib.favorites.insert(id, true);
    } else {
        lib.favorites.remove(&id);
    }
    save_library(&lib);
}
pub fn record_recent(cfg: &Path) {
    let mut lib = load_library();
    let id = theme_id(cfg);
    lib.recent.retain(|e| e.path != id);
    lib.recent.insert(
        0,
        RecentEntry {
            path: id,
            at: timestamp() as i64,
        },
    );
    lib.recent.truncate(MAX_RECENT);
    save_library(&lib);
}
pub fn recent_set() -> HashSet<String> {
    load_library().recent.into_iter().map(|e| e.path).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProfileTheme {
    pub path: String,
    #[serde(default)]
    pub monitor: String,
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub colors: ColorOverride,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub startup_delay: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DesktopProfile {
    pub name: String,
    #[serde(default)]
    pub themes: Vec<ProfileTheme>,
    #[serde(default)]
    pub autostart: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProfilesFile {
    #[serde(default)]
    pub profiles: BTreeMap<String, Vec<String>>,
    #[serde(default, rename = "desktop_profiles", alias = "desktop")]
    pub desktop: BTreeMap<String, DesktopProfile>,
    #[serde(default)]
    pub last_profile: String,
}
pub fn load_profiles() -> ProfilesFile {
    let mut p: ProfilesFile = load_json(&json_path("profiles.json"));
    for (name, paths) in p.profiles.clone() {
        if p.desktop.contains_key(&name) {
            continue;
        }
        let themes = paths
            .into_iter()
            .map(|path| {
                let mut pt = ProfileTheme {
                    path: path.clone(),
                    enabled: true,
                    ..Default::default()
                };
                if let Some(pos) = resolve_theme_position(Path::new(&path)) {
                    pt.monitor = pos.monitor.clone();
                    pt.position = pos;
                }
                if let Some(c) = resolve_theme_colors(Path::new(&path)) {
                    pt.colors = c;
                }
                pt
            })
            .collect();
        p.desktop.insert(name.clone(), DesktopProfile { name, themes, autostart: false });
    }
    p
}
pub fn save_profiles(p: &ProfilesFile) -> io::Result<()> {
    save_json(&json_path("profiles.json"), p)
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ManagedProcess {
    pub pid: i32,
    pub theme_id: String,
    pub config_path: String,
    pub launch_config: String,
    pub started_at: i64,
    #[serde(default)]
    pub preview: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RuntimeFile {
    #[serde(default)]
    pub processes: Vec<ManagedProcess>,
}
fn runtime_file() -> PathBuf {
    json_path("runtime.json")
}
fn runtime_mu() -> &'static Mutex<()> {
    static M: OnceLock<Mutex<()>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(()))
}
pub fn load_runtime() -> RuntimeFile {
    load_json(&runtime_file())
}
pub fn save_runtime(r: &RuntimeFile) {
    let _ = save_json(&runtime_file(), r);
}

pub fn pid_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    unsafe { libc::kill(pid, 0) == 0 }
}
pub fn read_proc_cmdline(pid: i32) -> String {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| String::from_utf8_lossy(&b).replace('\0', " "))
        .unwrap_or_default()
}
pub fn scan_conky_processes() -> Vec<(i32, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        if let Ok(o) = Command::new("pgrep").args(["-af", "conky"]).output() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let mut it = line.split_whitespace();
                if let Some(pid) = it.next().and_then(|s| s.parse().ok()) {
                    out.push((pid, it.collect::<Vec<_>>().join(" ")));
                }
            }
        }
        return out;
    };
    let self_pid = std::process::id() as i32;
    for e in entries.flatten() {
        let pid: i32 = match e.file_name().to_str().and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        if pid == self_pid {
            continue;
        }
        let cmdl = read_proc_cmdline(pid);
        if cmdl.is_empty() {
            continue;
        }
        let base = cmdl
            .split_whitespace()
            .next()
            .map(|s| Path::new(s).file_name().unwrap_or_default().to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if base != "conky" && !cmdl.contains(" conky ") && !cmdl.starts_with("conky ") {
            continue;
        }
        if cmdl.contains("conky-manager") && !cmdl.contains(" -c ") {
            continue;
        }
        out.push((pid, cmdl));
    }
    out
}
pub fn prune_runtime() -> RuntimeFile {
    let _g = runtime_mu().lock().unwrap();
    let mut r = load_runtime();
    let alive: HashSet<i32> = scan_conky_processes().into_iter().map(|(p, _)| p).collect();
    r.processes
        .retain(|p| alive.contains(&p.pid) && pid_alive(p.pid));
    save_runtime(&r);
    r
}
pub fn register_managed_process(p: ManagedProcess) {
    let _g = runtime_mu().lock().unwrap();
    let mut r = load_runtime();
    let mut next = Vec::new();
    for old in r.processes {
        if old.theme_id == p.theme_id || old.pid == p.pid {
            if old.pid != p.pid && pid_alive(old.pid) {
                stop_pid(old.pid, false);
            }
            continue;
        }
        next.push(old);
    }
    next.push(p);
    r.processes = next;
    save_runtime(&r);
}
pub fn managed_for_theme(cfg: &Path) -> Option<ManagedProcess> {
    let id = theme_id(cfg);
    prune_runtime()
        .processes
        .into_iter()
        .find(|p| p.theme_id == id)
}
pub fn stop_pid(pid: i32, force: bool) {
    if pid <= 0 || !pid_alive(pid) {
        return;
    }
    let managed_conky = read_proc_cmdline(pid).to_lowercase().contains("conky");
    unsafe {
        // run_conky creates a process group whose PGID equals the leader PID.
        // Signal the group only after verifying it is still a Conky process.
        if managed_conky { libc::kill(-pid, libc::SIGTERM); }
        libc::kill(pid, libc::SIGTERM);
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(1500);
    while std::time::Instant::now() < deadline {
        if !pid_alive(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(80));
    }
    if force {
        unsafe {
            if managed_conky { libc::kill(-pid, libc::SIGKILL); }
            libc::kill(pid, libc::SIGKILL);
        }
    }
}
pub fn stop_managed_theme(cfg: &Path) -> bool {
    let id = theme_id(cfg);
    let r = prune_runtime();
    let mut found = false;
    let mut next = Vec::new();
    for p in r.processes {
        if p.theme_id == id {
            stop_pid(p.pid, true);
            found = true;
        } else {
            next.push(p);
        }
    }
    save_runtime(&RuntimeFile { processes: next });
    found
}
pub fn stop_all_managed() -> usize {
    let r = prune_runtime();
    let n = r.processes.len();
    for p in r.processes {
        stop_pid(p.pid, true);
    }
    save_runtime(&RuntimeFile::default());
    n
}
pub fn is_theme_running(cfg: &Path) -> bool {
    managed_for_theme(cfg).is_some()
}

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub scale: i32,
    pub primary: bool,
    pub head: i32,
    pub connector: String,
}
impl MonitorInfo {
    pub fn label(&self) -> String {
        format!(
            "{}{} {}×{} @ {},{} ×{}{}",
            if self.primary { "★ " } else { "" },
            self.name,
            self.width,
            self.height,
            self.x,
            self.y,
            self.scale,
            if self.connector.is_empty() {
                String::new()
            } else {
                format!(" · {}", self.connector)
            }
        )
    }
}
pub fn list_monitors() -> Vec<MonitorInfo> {
    if let Some(m) = parse_hypr_monitors() {
        return m;
    }
    if let Some(m) = parse_sway_outputs() {
        return m;
    }
    if let Some(m) = parse_xrandr() {
        return m;
    }
    vec![MonitorInfo {
        id: "primary".into(),
        name: "Primary".into(),
        width: 1920,
        height: 1080,
        x: 0,
        y: 0,
        scale: 1,
        primary: true,
        head: 1,
        connector: String::new(),
    }]
}
fn parse_xrandr() -> Option<Vec<MonitorInfo>> {
    let out = Command::new("xrandr").arg("--query").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let re = Regex::new(r"^(\S+)\s+connected(?:\s+primary)?\s+(\d+)x(\d+)\+(\d+)\+(\d+)").ok()?;
    let mut mons = Vec::new();
    let mut head = 1;
    for line in text.lines() {
        if let Some(c) = re.captures(line) {
            let connector = c[1].to_string();
            mons.push(MonitorInfo {
                id: connector.clone(),
                name: connector.clone(),
                width: c[2].parse().unwrap_or(0),
                height: c[3].parse().unwrap_or(0),
                x: c[4].parse().unwrap_or(0),
                y: c[5].parse().unwrap_or(0),
                scale: 1,
                primary: line.contains(" primary "),
                head,
                connector,
            });
            head += 1;
        }
    }
    if mons.is_empty() {
        None
    } else {
        if mons.len() == 1 {
            mons[0].primary = true;
        }
        Some(mons)
    }
}
fn parse_hypr_monitors() -> Option<Vec<MonitorInfo>> {
    let out = Command::new("hyprctl").args(["monitors", "-j"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let arr = v.as_array()?;
    let mut mons = Vec::new();
    for (i, m) in arr.iter().enumerate() {
        let name = m.get("name").and_then(|x| x.as_str()).unwrap_or("monitor");
        mons.push(MonitorInfo {
            id: name.into(),
            name: name.into(),
            width: m.get("width").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            height: m.get("height").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            x: m.get("x").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            y: m.get("y").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            scale: m.get("scale").and_then(|x| x.as_f64()).unwrap_or(1.0).max(1.0) as i32,
            primary: i == 0,
            head: i as i32 + 1,
            connector: name.into(),
        });
    }
    if mons.is_empty() {
        None
    } else {
        Some(mons)
    }
}
fn parse_sway_outputs() -> Option<Vec<MonitorInfo>> {
    let out = Command::new("swaymsg").args(["-t", "get_outputs"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let arr = v.as_array()?;
    let mut mons = Vec::new();
    for (i, m) in arr.iter().enumerate() {
        if m.get("active").and_then(|x| x.as_bool()) == Some(false) {
            continue;
        }
        let name = m.get("name").and_then(|x| x.as_str()).unwrap_or("output");
        let rect = m.get("rect")?;
        mons.push(MonitorInfo {
            id: name.into(),
            name: name.into(),
            width: rect.get("width").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            height: rect.get("height").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            x: rect.get("x").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            y: rect.get("y").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            scale: m.get("scale").and_then(|x| x.as_f64()).unwrap_or(1.0).max(1.0) as i32,
            primary: m.get("primary").and_then(|x| x.as_bool()).unwrap_or(i == 0),
            head: i as i32 + 1,
            connector: name.into(),
        });
    }
    if mons.is_empty() {
        None
    } else {
        Some(mons)
    }
}
pub fn find_monitor(id: &str, head: i32) -> Option<MonitorInfo> {
    let mons = list_monitors();
    if !id.is_empty() {
        if let Some(m) = mons.iter().find(|m| m.id == id || m.name == id || m.connector == id) {
            return Some(m.clone());
        }
    }
    if head > 0 {
        if let Some(m) = mons.iter().find(|m| m.head == head) {
            return Some(m.clone());
        }
    }
    mons.into_iter().find(|m| m.primary).or_else(|| list_monitors().into_iter().next())
}

pub fn color_presets() -> Vec<(&'static str, &'static str, BTreeMap<&'static str, &'static str>)> {
    vec![
        ("tokyo_night", "Tokyo Night", maplit_colors(&[("default_color", "#A9B1D6"), ("color1", "#F7768E"), ("color2", "#7AA2F7"), ("default_outline_color", "#565F89"), ("default_shade_color", "#1A1B26")])),
        ("dracula", "Dracula", maplit_colors(&[("default_color", "#F8F8F2"), ("color1", "#FF79C6"), ("color2", "#8BE9FD"), ("default_outline_color", "#6272A4"), ("default_shade_color", "#282A36")])),
        ("nord", "Nord", maplit_colors(&[("default_color", "#ECEFF4"), ("color1", "#88C0D0"), ("color2", "#81A1C1"), ("default_outline_color", "#4C566A"), ("default_shade_color", "#2E3440")])),
        ("catppuccin_mocha", "Catppuccin Mocha", maplit_colors(&[("default_color", "#CDD6F4"), ("color1", "#F5C2E7"), ("color2", "#89B4FA"), ("default_outline_color", "#6C7086"), ("default_shade_color", "#1E1E2E")])),
        ("rose_pine", "Rosé Pine", maplit_colors(&[("default_color", "#E0DEF4"), ("color1", "#EBBCBA"), ("color2", "#9CCFD8"), ("default_outline_color", "#6E6A86"), ("default_shade_color", "#191724")])),
        ("gruvbox_dark", "Gruvbox Dark", maplit_colors(&[("default_color", "#EBDBB2"), ("color1", "#FB4934"), ("color2", "#83A598"), ("default_outline_color", "#928374"), ("default_shade_color", "#282828")])),
        ("solarized_dark", "Solarized Dark", maplit_colors(&[("default_color", "#839496"), ("color1", "#DC322F"), ("color2", "#268BD2"), ("default_outline_color", "#586E75"), ("default_shade_color", "#002B36")])),
        ("everforest", "Everforest", maplit_colors(&[("default_color", "#D3C6AA"), ("color1", "#E67E80"), ("color2", "#7FBBB3"), ("default_outline_color", "#859289"), ("default_shade_color", "#2D353B")])),
    ]
}
fn maplit_colors(pairs: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, &'static str> {
    pairs.iter().copied().collect()
}
pub fn named_color(raw: &str) -> Option<&'static str> {
    match raw.to_lowercase().as_str() {
        "white" => Some("#FFFFFF"),
        "black" => Some("#000000"),
        "red" => Some("#FF0000"),
        "green" => Some("#00FF00"),
        "blue" => Some("#0000FF"),
        "yellow" => Some("#FFFF00"),
        "orange" => Some("#FFA500"),
        "gray" | "grey" => Some("#808080"),
        _ => None,
    }
}
pub fn normalize_color(value: &str) -> String {
    let raw = value.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    if raw.is_empty() {
        return String::new();
    }
    if let Some(n) = named_color(&raw) {
        return n.into();
    }
    let hex: String = if let Some(rest) = raw.strip_prefix('#') {
        rest.chars().filter(|c| c.is_ascii_hexdigit()).collect()
    } else {
        raw.chars().filter(|c| c.is_ascii_hexdigit()).collect()
    };
    let hex = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex
    };
    if hex.len() == 6 {
        format!("#{}", hex.to_uppercase())
    } else {
        String::new()
    }
}
pub fn is_new_syntax(content: &str) -> bool {
    content.contains("conky.config") && content.contains("conky.text")
}
pub fn is_valid_theme(cfg: &Path) -> bool {
    let text = read_text(cfg);
    text.contains("conky.config") || text.contains("own_window") || text.contains("TEXT")
}
pub fn looks_like_config_name(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with(".conf") || name == "conkyrc" || name.starts_with("conkyrc")
}

pub fn all_theme_roots() -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    let mut add = |p: PathBuf| {
        if p.is_dir() {
            let r = resolve_path(&p);
            if seen.insert(r.clone()) {
                roots.push(r);
            }
        }
    };
    add(home().join(".config/conky"));
    add(conky_dir());
    add(home().join(".local/share/conky"));
    add(PathBuf::from("/usr/share/conky"));
    let s = load_settings();
    if let Some(td) = &s.theme_dir {
        add(PathBuf::from(td));
    }
    for d in s.custom_theme_dirs {
        add(PathBuf::from(d));
    }
    for d in load_library().custom_dirs {
        add(PathBuf::from(d));
    }
    roots
}

pub fn looks_like_theme_root(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let Ok(rd) = fs::read_dir(path) else {
        return false;
    };
    let names: HashSet<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_lowercase()))
        .collect();
    if ASSET_DIRS.iter().any(|n| names.contains(*n)) {
        return true;
    }
    if path.join("start.sh").is_file() {
        return true;
    }
    fs::read_dir(path)
        .ok()
        .map(|it| {
            it.flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("conf"))
        })
        .unwrap_or(false)
}
pub fn resolve_theme_root(cfg: &Path) -> PathBuf {
    let cfg = resolve_path(cfg);
    let dir = cfg.parent().unwrap_or(Path::new(".")).to_path_buf();
    let n = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if n == "config" || n == "conky" || n == "scripts" {
        if let Some(parent) = dir.parent() {
            if looks_like_theme_root(parent) {
                return resolve_path(parent);
            }
        }
    }
    if looks_like_theme_root(&dir) {
        return resolve_path(&dir);
    }
    let mut p = dir.clone();
    for _ in 0..4 {
        p = match p.parent() {
            Some(x) => x.to_path_buf(),
            None => break,
        };
        if p.as_os_str() == "/" {
            break;
        }
        if looks_like_theme_root(&p) {
            return resolve_path(&p);
        }
    }
    resolve_path(&dir)
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub path: String,
    pub root: String,
    pub running: bool,
    pub favorite: bool,
    pub health: String,
    pub recent: bool,
}
pub fn discover_themes() -> Vec<Theme> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let fav = load_library().favorites;
    let recent = recent_set();
    let running = prune_runtime();
    let running_ids: HashSet<String> = running.processes.into_iter().map(|p| p.theme_id).collect();
    for root in all_theme_roots() {
        for e in WalkDir::new(&root).follow_links(false).into_iter().filter_map(Result::ok) {
            if !e.file_type().is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy();
            if !looks_like_config_name(&name) {
                continue;
            }
            if !is_valid_theme(e.path()) {
                continue;
            }
            let resolved = resolve_path(e.path());
            let key = resolved.to_string_lossy().to_string();
            if !seen.insert(key.clone()) {
                continue;
            }
            let theme_root = resolve_theme_root(&resolved);
            let display = format!(
                "{}/{}",
                theme_root
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Theme"),
                resolved.file_name().and_then(|s| s.to_str()).unwrap_or("conky.conf")
            );
            let health = inspect_theme_health(&resolved, false);
            out.push(Theme {
                name: display,
                path: key.clone(),
                root: theme_root.to_string_lossy().to_string(),
                running: running_ids.contains(&key),
                favorite: fav.get(&key).copied().unwrap_or(false),
                health: health.status_label(),
                recent: recent.contains(&key),
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn expand_theme_path(path_str: &str) -> String {
    let mut s = path_str.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    s = s.replace("$HOME", &home().to_string_lossy());
    if let Some(rest) = s.strip_prefix("~/") {
        s = home().join(rest).to_string_lossy().to_string();
    }
    s
}
pub fn find_asset_in_theme(theme_root: &Path, raw_path: &str) -> Option<PathBuf> {
    let expanded = expand_theme_path(raw_path);
    if expanded.is_empty() || expanded.starts_with('-') {
        return None;
    }
    for tok in ["$", "|", "&&", "||", "`"] {
        if expanded.contains(tok) {
            return None;
        }
    }
    let p = Path::new(&expanded);
    if p.is_file() {
        return Some(resolve_path(p));
    }
    let home_s = home().to_string_lossy().to_string();
    if expanded.starts_with(&home_s) {
        if let Ok(rel) = Path::new(&expanded).strip_prefix(home()) {
            let parts: Vec<_> = rel.iter().collect();
            if parts.len() >= 3 && parts[0] == ".config" && parts[1] == "conky" {
                let tail = if parts.len() > 3 {
                    parts[3..].iter().collect::<PathBuf>()
                } else {
                    PathBuf::from(".")
                };
                let remapped = theme_root.join(tail);
                if remapped.is_file() {
                    return Some(resolve_path(&remapped));
                }
            }
            if parts.len() >= 2 && parts[0] == ".conky" {
                let tail = if parts.len() > 2 {
                    parts[2..].iter().collect::<PathBuf>()
                } else {
                    PathBuf::from(".")
                };
                let remapped = theme_root.join(tail);
                if remapped.is_file() {
                    return Some(resolve_path(&remapped));
                }
            }
        }
    }
    for base in [
        theme_root.to_path_buf(),
        theme_root.join("res"),
        theme_root.join("assets"),
        theme_root.join("images"),
        theme_root.join("img"),
        theme_root.join("scripts"),
    ] {
        let remapped = base.join(&expanded);
        if remapped.is_file() {
            return Some(resolve_path(&remapped));
        }
    }
    let filename = Path::new(&expanded).file_name()?.to_string_lossy().to_string();
    if filename.is_empty() || filename == "." || filename == ".." {
        return None;
    }
    let matches: Vec<_> = WalkDir::new(theme_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == filename.as_str())
        .map(|e| e.into_path())
        .collect();
    if matches.len() == 1 {
        Some(resolve_path(&matches[0]))
    } else {
        None
    }
}

fn path_ref_patterns() -> &'static [Regex] {
    static RE: OnceLock<Vec<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        vec![
            Regex::new(r"\$\{image\s+([^}\s]+)").unwrap(),
            Regex::new(r#"(?i)lua_load\s*=\s*['"]([^'"]+)['"]"#).unwrap(),
            Regex::new(r"\$\{(?:exec|execi|execpi|execp|texeci|texecpi)\s+(?:\d+\s+)?((?:~|\$HOME|/)[^}\s|]+)").unwrap(),
            Regex::new(r#"(~/.config/conky/[^\s'"{}|$]+)"#).unwrap(),
            Regex::new(r#"(\$HOME/.config/conky/[^\s'"{}|$]+)"#).unwrap(),
            Regex::new(r#"(~/.conky/[^\s'"{}|$]+)"#).unwrap(),
        ]
    })
}

pub fn rewrite_theme_asset_paths(
    content: &str,
    theme_root: &Path,
    launch_dir_path: Option<&Path>,
) -> (String, i32, Vec<String>) {
    let theme_root = resolve_path(theme_root);
    let mut fixes = 0;
    let mut missing = Vec::new();
    let mut seen_missing = HashSet::new();
    let mut updated = content.to_string();
    for pat in path_ref_patterns() {
        updated = pat
            .replace_all(&updated, |caps: &regex::Captures| {
                let m = caps.get(0).unwrap().as_str();
                let original = caps.get(1).unwrap().as_str();
                if let Some(resolved) = find_asset_in_theme(&theme_root, original) {
                    fixes += 1;
                    return m.replacen(original, &resolved.to_string_lossy(), 1);
                }
                if original.starts_with("~/.cache/")
                    || original.starts_with(&(home().to_string_lossy().to_string() + "/.cache/"))
                {
                    return m.to_string();
                }
                let lower = original.to_lowercase();
                if ASSET_SUFFIXES.iter().any(|e| lower.ends_with(e)) && seen_missing.insert(original.to_string())
                {
                    missing.push(original.to_string());
                }
                m.to_string()
            })
            .into_owned();
    }
    let theme_name = theme_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("theme");
    let home_s = home().to_string_lossy().to_string();
    let replacements = [
        format!("~/.config/conky/{theme_name}"),
        format!("$HOME/.config/conky/{theme_name}"),
        format!("{home_s}/.config/conky/{theme_name}"),
        format!("~/.conky/{theme_name}"),
        format!("$HOME/.conky/{theme_name}"),
        format!("{home_s}/.conky/{theme_name}"),
    ];
    let root_s = theme_root.to_string_lossy().to_string();
    for old in replacements {
        if updated.contains(&old) {
            updated = updated.replace(&old, &root_s);
            fixes += 1;
        }
    }
    if let Some(ldir) = launch_dir_path {
        let script_root = ldir.join("scripts");
        if script_root.is_dir() {
            let old = theme_root.join("scripts").to_string_lossy().to_string();
            if updated.contains(&old) {
                updated = updated.replace(&old, &script_root.to_string_lossy());
                fixes += 1;
            }
        }
    }
    (updated, fixes, missing)
}

pub fn parse_theme_position(content: &str) -> Position {
    let mut res = Position {
        alignment: "top_left".into(),
        gap_x: 30,
        gap_y: 50,
        ..Default::default()
    };
    let re = if is_new_syntax(content) {
        Regex::new(r"(?im)^\s*(alignment|gap_x|gap_y)\s*=\s*([^,\n]+)").unwrap()
    } else {
        Regex::new(r"(?im)^\s*(alignment|gap_x|gap_y)\s+(\S+)").unwrap()
    };
    for m in re.captures_iter(content) {
        let key = m[1].to_lowercase();
        let val = m[2].trim().trim_matches(|c| c == '"' || c == '\'');
        if key == "alignment" {
            res.alignment = val.to_string();
        } else if let Ok(n) = val.split('.').next().unwrap_or("0").parse::<i32>() {
            if key == "gap_x" {
                res.gap_x = n;
            } else {
                res.gap_y = n;
            }
        }
    }
    res
}
pub fn resolve_theme_position(cfg: &Path) -> Option<Position> {
    let p = load_positions();
    let id = theme_id(cfg);
    if let Some(saved) = p.themes.get(&id) {
        if saved.enabled || !saved.alignment.is_empty() || saved.gap_x != 0 || saved.gap_y != 0 {
            let mut s = saved.clone();
            if s.alignment.is_empty() {
                s.alignment = "top_left".into();
            }
            return Some(s);
        }
    }
    None
}
pub fn save_theme_position(cfg: &Path, pos: Position) {
    let mut data = load_positions();
    data.themes.insert(theme_id(cfg), pos);
    save_positions(&data);
}
pub fn parse_theme_colors(content: &str) -> BTreeMap<String, String> {
    let mut slots = BTreeMap::new();
    let re = if is_new_syntax(content) {
        Regex::new(r"(?im)^\s*(default_color|default_outline_color|default_shade_color|own_window_colour|color\d*)\s*=\s*([^,\n]+)").unwrap()
    } else {
        Regex::new(r"(?im)^\s*(default_color|default_outline_color|default_shade_color|own_window_colour|color\d*)\s+(\S+)").unwrap()
    };
    for m in re.captures_iter(content) {
        slots.insert(
            m[1].to_string(),
            m[2].trim().trim_matches(|c| c == '"' || c == '\'').to_string(),
        );
    }
    slots
}
pub fn resolve_theme_colors(cfg: &Path) -> Option<ColorOverride> {
    load_colors().themes.get(&theme_id(cfg)).cloned()
}
pub fn save_theme_colors(cfg: &Path, colors: ColorOverride) {
    let mut data = load_colors();
    data.themes.insert(theme_id(cfg), colors);
    save_colors(&data);
}
fn format_conky_color_value(color: &str, new_syntax: bool) -> String {
    let n = normalize_color(color);
    if new_syntax {
        if n.is_empty() {
            format!("'{}'", color.trim().trim_matches(|c| c == '"' || c == '\''))
        } else {
            format!("'{n}'")
        }
    } else if n.is_empty() {
        color.trim().trim_matches(|c| c == '"' || c == '\'').to_string()
    } else {
        n[1..].to_string()
    }
}
pub fn apply_color_patch(content: &str, colors: &BTreeMap<String, String>) -> (String, i32) {
    let mut content = content.to_string();
    let mut fixes = 0;
    let new_syn = is_new_syntax(&content);
    for (slot, new_color) in colors {
        let value = format_conky_color_value(new_color, new_syn);
        let re = if new_syn {
            Regex::new(&format!(r"(?im)(^\s*{}\s*=\s*)([^,\n]+)(,?\s*$)", regex::escape(slot))).unwrap()
        } else {
            Regex::new(&format!(r"(?im)(^\s*{}\s+)(\S+)(.*$)", regex::escape(slot))).unwrap()
        };
        if re.find(&content).is_some() {
            let mut done = false;
            content = re
                .replace_all(&content, |caps: &regex::Captures| {
                    if done {
                        return caps.get(0).unwrap().as_str().to_string();
                    }
                    done = true;
                    format!("{}{}{}", &caps[1], value, &caps[3])
                })
                .into_owned();
            fixes += 1;
        }
    }
    (content, fixes)
}
pub fn apply_hex_remap(content: &str, remap: &HashMap<String, String>) -> (String, i32) {
    let mut content = content.to_string();
    let mut fixes = 0;
    for (old_c, new_c) in remap {
        let oh = normalize_color(old_c);
        let nh = normalize_color(new_c);
        if oh.is_empty() || nh.is_empty() || oh == nh {
            continue;
        }
        let ob = &oh[1..];
        let nb = nh[1..].to_uppercase();
        for (from, to) in [(format!("0x{ob}"), format!("0x{nb}")), (format!("#{ob}"), format!("#{nb}"))] {
            let re = Regex::new(&format!("(?i){}", regex::escape(&from))).unwrap();
            let n = re.find_iter(&content).count();
            if n > 0 {
                content = re.replace_all(&content, to.as_str()).into_owned();
                fixes += n as i32;
            }
        }
    }
    (content, fixes)
}
pub fn merge_preset_colors(theme_colors: &BTreeMap<String, String>, preset_id: &str) -> BTreeMap<String, String> {
    let mut merged = theme_colors.clone();
    for (id, _, colors) in color_presets() {
        if id == preset_id {
            for (slot, value) in colors {
                if merged.contains_key(slot) {
                    merged.insert(slot.to_string(), value.to_string());
                }
            }
            break;
        }
    }
    merged
}
pub fn build_smart_color_remap(
    original: &BTreeMap<String, String>,
    new_colors: &BTreeMap<String, String>,
) -> HashMap<String, String> {
    let mut remap = HashMap::new();
    for (slot, new_val) in new_colors {
        if let Some(old_val) = original.get(slot) {
            let oh = normalize_color(old_val);
            let nh = normalize_color(new_val);
            if !oh.is_empty() && !nh.is_empty() && oh != nh {
                remap.insert(oh, nh);
            }
        }
    }
    remap
}

fn window_hints(desktop_env: &str, layer: &str, new_syntax: bool) -> String {
    let above = layer == "above";
    let (hi, lo) = match desktop_env {
        "kde" => (
            "undecorated,above,sticky,skip_taskbar,skip_pager",
            "undecorated,below,sticky,skip_taskbar,skip_pager",
        ),
        "gnome" | "cinnamon" | "mate" | "cosmic" => (
            "undecorated,above,skip_taskbar,skip_pager",
            "undecorated,below,skip_taskbar,skip_pager",
        ),
        _ => (
            "undecorated,above,sticky,skip_taskbar,skip_pager",
            "undecorated,below,sticky,skip_taskbar,skip_pager",
        ),
    };
    let raw = if above { hi } else { lo };
    if new_syntax {
        format!("\"{raw}\"")
    } else {
        raw.into()
    }
}
fn compute_desktop_settings(settings: &Settings, new_syntax: bool, position: Option<&Position>) -> BTreeMap<String, String> {
    let runtime = resolve_runtime_settings(settings.clone());
    let (upd, cpu, net) = match runtime.smoothness.as_str() {
        "ultra" => ("1.5", "3", "3"),
        "performance" => ("0.5", "2", "2"),
        _ => ("1.0", "2", "2"),
    };
    let mut de = detect_desktop_environment();
    if runtime.desktop_preset != "auto" && !runtime.desktop_preset.is_empty() {
        de = runtime.desktop_preset.clone();
    }
    let mut base: BTreeMap<String, String> = if new_syntax {
        [
            ("update_interval", "1.0"),
            ("update_interval_on_battery", "1.5"),
            ("cpu_avg_samples", "2"),
            ("net_avg_samples", "2"),
            ("double_buffer", "true"),
            ("no_buffers", "true"),
            ("own_window", "true"),
            ("own_window_argb_visual", "true"),
            ("own_window_argb_value", "0"),
            ("own_window_transparent", "true"),
            ("draw_shades", "false"),
            ("draw_outline", "false"),
            ("draw_borders", "false"),
            ("draw_graph_borders", "false"),
            ("use_xft", "true"),
            ("xftalpha", "1.0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    } else {
        [
            ("update_interval", "1.0"),
            ("cpu_avg_samples", "2"),
            ("net_avg_samples", "2"),
            ("double_buffer", "yes"),
            ("no_buffers", "yes"),
            ("own_window", "yes"),
            ("own_window_argb_visual", "yes"),
            ("own_window_argb_value", "0"),
            ("own_window_transparent", "yes"),
            ("draw_shades", "no"),
            ("draw_outline", "no"),
            ("draw_borders", "no"),
            ("draw_graph_borders", "no"),
            ("use_xft", "yes"),
            ("xftalpha", "1.0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    };
    base.insert("update_interval".into(), upd.into());
    base.insert("cpu_avg_samples".into(), cpu.into());
    base.insert("net_avg_samples".into(), net.into());
    if new_syntax {
        base.insert("own_window_type".into(), format!("\"{}\"", runtime.window_type));
        base.insert(
            "own_window_hints".into(),
            window_hints(&de, &runtime.layer, true),
        );
    } else {
        base.insert("own_window_type".into(), runtime.window_type);
        base.insert(
            "own_window_hints".into(),
            window_hints(&de, &runtime.layer, false),
        );
    }
    if let Some(pos) = position {
        if pos.enabled || !pos.alignment.is_empty() {
            if new_syntax {
                base.insert("alignment".into(), format!("'{}'", pos.alignment));
            } else {
                base.insert("alignment".into(), pos.alignment.clone());
            }
            base.insert("gap_x".into(), pos.gap_x.to_string());
            base.insert("gap_y".into(), pos.gap_y.to_string());
            let mut head = pos.xinerama_head;
            if head <= 0 {
                if let Some(mon) = find_monitor(&pos.monitor, pos.xinerama_head) {
                    head = mon.head;
                }
            }
            if head > 0 {
                base.insert("xinerama_head".into(), head.to_string());
            }
        }
    }
    base
}
fn patch_new_syntax(content: &str, settings: &Settings, position: Option<&Position>) -> String {
    let start = match content.find("conky.config") {
        Some(s) => s,
        None => return content.to_string(),
    };
    let brace = match content[start..].find('{') {
        Some(b) => start + b,
        None => return content.to_string(),
    };
    let mut depth = 0;
    let mut end_idx = None;
    for (i, ch) in content[brace..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_idx = Some(brace + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end_idx) = end_idx else {
        return content.to_string();
    };
    let mut block = content[brace + 1..end_idx].to_string();
    for (key, value) in compute_desktop_settings(settings, true, position) {
        let re = Regex::new(&format!(r"(?m)^\s*{}\s*=\s*.*?,\s*$", regex::escape(&key))).unwrap();
        let repl = format!("    {key} = {value},");
        if re.find(&block).is_some() {
            block = re.replace(&block, repl.as_str()).into_owned();
        } else {
            block = format!("{repl}\n{block}");
        }
    }
    format!("{}{}{}", &content[..=brace], block, &content[end_idx..])
}
fn patch_legacy_syntax(content: &str, settings: &Settings, position: Option<&Position>) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut text_idx = lines.len();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().eq_ignore_ascii_case("TEXT") {
            text_idx = i;
            break;
        }
    }
    let mut head: Vec<String> = lines[..text_idx].iter().map(|s| s.to_string()).collect();
    let tail: Vec<String> = lines[text_idx..].iter().map(|s| s.to_string()).collect();
    let mut key_to_idx = HashMap::new();
    for (i, line) in head.iter().enumerate() {
        let st = line.trim();
        if st.is_empty() || st.starts_with('#') {
            continue;
        }
        if let Some(key) = st.split_whitespace().next() {
            key_to_idx.insert(key.to_string(), i);
        }
    }
    for (key, value) in compute_desktop_settings(settings, false, position) {
        let new_line = format!("{key} {value}");
        if let Some(&idx) = key_to_idx.get(&key) {
            head[idx] = new_line;
        } else {
            head.insert(0, new_line);
            for v in key_to_idx.values_mut() {
                *v += 1;
            }
            key_to_idx.insert(key, 0);
        }
    }
    let mut out = head;
    out.extend(tail);
    out.join("\n") + "\n"
}

#[derive(Clone, Debug, Default)]
pub struct ThemeLaunchBundle {
    pub source_config: PathBuf,
    pub theme_root: PathBuf,
    pub launch_dir: PathBuf,
    pub launch_config: PathBuf,
    pub path_fixes: i32,
    pub missing_assets: Vec<String>,
    pub fonts_dir: Option<PathBuf>,
}
pub fn ensure_compat_install_symlink(theme_root: &Path) {
    if !load_settings().create_compat_symlinks {
        return;
    }
    let theme_root = resolve_path(theme_root);
    let compat = home()
        .join(".config/conky")
        .join(theme_root.file_name().unwrap_or_default());
    let _ = fs::create_dir_all(compat.parent().unwrap());
    if compat.symlink_metadata().is_ok() {
        return;
    }
    let _ = std::os::unix::fs::symlink(&theme_root, &compat);
}
fn materialize_patched_scripts(theme_root: &Path, ldir: &Path, hex_remap: &HashMap<String, String>) -> i32 {
    let scripts_src = theme_root.join("scripts");
    if !scripts_src.is_dir() {
        return 0;
    }
    let scripts_dest = ldir.join("scripts");
    let _ = fs::remove_dir_all(&scripts_dest);
    if copy_tree(&scripts_src, &scripts_dest).is_err() {
        return 0;
    }
    let mut patched = 0;
    for e in WalkDir::new(&scripts_dest).into_iter().filter_map(Result::ok) {
        if !e.file_type().is_file() {
            continue;
        }
        let ext = e
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "sh" && ext != "lua" {
            continue;
        }
        let original = read_text(e.path());
        let (mut updated, mut fixes, _) =
            rewrite_theme_asset_paths(&original, theme_root, Some(ldir));
        if ext == "lua" && !hex_remap.is_empty() {
            let (u, cf) = apply_hex_remap(&updated, hex_remap);
            updated = u;
            fixes += cf;
        }
        if fixes > 0 {
            let _ = fs::write(e.path(), updated);
            patched += 1;
        }
    }
    patched
}
fn in_asset_dirs(name: &str) -> bool {
    ASSET_DIRS.iter().any(|n| n.eq_ignore_ascii_case(name))
}
fn in_asset_suffix(name: &str) -> bool {
    let l = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    ASSET_SUFFIXES.contains(&l.as_str())
}
fn link_theme_assets(theme_root: &Path, ldir: &Path) {
    let Ok(rd) = fs::read_dir(theme_root) else {
        return;
    };
    for item in rd.flatten() {
        let name = item.file_name();
        let ns = name.to_string_lossy();
        if ns.starts_with('.') {
            continue;
        }
        let src = item.path();
        if src.is_dir() && in_asset_dirs(&ns) {
            if ns.eq_ignore_ascii_case("scripts") {
                continue;
            }
            let dest = ldir.join(&name);
            let _ = fs::remove_dir_all(&dest);
            let _ = std::os::unix::fs::symlink(resolve_path(&src), dest);
        } else if src.is_file() && in_asset_suffix(&ns) {
            let dest = ldir.join(&name);
            let _ = fs::remove_file(&dest);
            let _ = std::os::unix::fs::symlink(resolve_path(&src), dest);
        }
    }
}
pub fn conky_env() -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = env::vars().collect();
    let has_disp = env.iter().any(|(k, _)| k == "DISPLAY");
    let has_way = env.iter().any(|(k, _)| k == "WAYLAND_DISPLAY");
    if !has_disp && !has_way {
        env.push(("DISPLAY".into(), ":0".into()));
    }
    env
}
pub fn build_launch_env(bundle: &ThemeLaunchBundle) -> Vec<(String, String)> {
    let mut m: HashMap<String, String> = conky_env().into_iter().collect();
    m.insert(
        "CONKY_THEME_ROOT".into(),
        bundle.theme_root.to_string_lossy().into(),
    );
    m.insert(
        "CONKY_MANAGER_LAUNCH_DIR".into(),
        bundle.launch_dir.to_string_lossy().into(),
    );
    m.insert(
        "CONKY_MANAGER_THEME_ID".into(),
        bundle.source_config.to_string_lossy().into(),
    );
    if let Some(fonts) = &bundle.fonts_dir {
        if fonts.is_dir() {
            let fc_dir = bundle.launch_dir.join(".fontconfig");
            let _ = fs::create_dir_all(&fc_dir);
            let fonts_conf = fc_dir.join("fonts.conf");
            let txt = format!(
                "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<fontconfig>\n  <dir>{}</dir>\n</fontconfig>\n",
                resolve_path(fonts).display()
            );
            let _ = fs::write(&fonts_conf, txt);
            m.insert("FONTCONFIG_FILE".into(), fonts_conf.to_string_lossy().into());
        }
    }
    m.into_iter().collect()
}

pub fn create_theme_launch_bundle(
    cfg_path: &Path,
    apply_desktop_optimize: bool,
    position_override: Option<&Position>,
    color_override: Option<&ColorOverride>,
) -> io::Result<ThemeLaunchBundle> {
    let cfg_path = resolve_path(cfg_path);
    let theme_root = resolve_theme_root(&cfg_path);
    let original = read_text(&cfg_path);
    let mut content = original.clone();
    let original_colors = parse_theme_colors(&original);
    let mut hex_remap = HashMap::new();
    let mut color_fixes = 0;
    let settings = load_settings();
    if apply_desktop_optimize {
        if settings.auto_optimize || position_override.is_some() {
            let pos = position_override.cloned().or_else(|| resolve_theme_position(&cfg_path));
            if is_new_syntax(&content) {
                content = patch_new_syntax(&content, &settings, pos.as_ref());
            } else {
                content = patch_legacy_syntax(&content, &settings, pos.as_ref());
            }
        }
    } else if let Some(pos) = position_override {
        if is_new_syntax(&content) {
            content = patch_new_syntax(&content, &settings, Some(pos));
        } else {
            content = patch_legacy_syntax(&content, &settings, Some(pos));
        }
    }
    if let Some(co) = color_override {
        if !co.colors.is_empty() {
            if co.smart_lua {
                hex_remap = build_smart_color_remap(&original_colors, &co.colors);
            }
            let (c, f) = apply_color_patch(&content, &co.colors);
            content = c;
            color_fixes = f;
        }
    }
    let (content, mut path_fixes, _missing) =
        rewrite_theme_asset_paths(&content, &theme_root, None);
    ensure_compat_install_symlink(&theme_root);
    let bundle_name = normalize_folder_name(&format!(
        "{}_{}",
        theme_root.file_name().and_then(|s| s.to_str()).unwrap_or("theme"),
        cfg_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("conky")
    ));
    let ldir = launch_dir().join(bundle_name);
    let _ = fs::remove_dir_all(&ldir);
    fs::create_dir_all(&ldir)?;
    let patched_scripts = materialize_patched_scripts(&theme_root, &ldir, &hex_remap);
    let (content, extra, missing) = rewrite_theme_asset_paths(&content, &theme_root, Some(&ldir));
    path_fixes += extra + patched_scripts + color_fixes;
    let launch_config = ldir.join(cfg_path.file_name().unwrap_or_default());
    fs::write(&launch_config, content)?;
    link_theme_assets(&theme_root, &ldir);
    let fonts = theme_root.join("fonts");
    Ok(ThemeLaunchBundle {
        source_config: cfg_path,
        theme_root,
        launch_dir: ldir,
        launch_config,
        path_fixes,
        missing_assets: missing,
        fonts_dir: if fonts.is_dir() { Some(fonts) } else { None },
    })
}

pub fn validate_conky_config(config_path: &Path, cwd: &Path, envv: &[(String, String)]) -> bool {
    if !config_path.is_file() {
        return false;
    }
    let mut cmd = Command::new("conky");
    cmd.args(["-c", &config_path.to_string_lossy(), "-i", "1", "-q"])
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in envv {
        cmd.env(k, v);
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

pub fn start_conky_with_healthcheck(
    config_path: &Path,
    cwd: &Path,
    envv: &[(String, String)],
) -> Option<i32> {
    let settings = load_settings();
    let log_path = data_dir().join("manager.log");
    let log_f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
    let mut cmd = if which("nice") {
        let mut c = Command::new("nice");
        c.args([
            "-n",
            &settings.nice_level.to_string(),
            "conky",
            "-q",
            "-c",
            &config_path.to_string_lossy(),
        ]);
        c
    } else {
        let mut c = Command::new("conky");
        c.args(["-q", "-c", &config_path.to_string_lossy()]);
        c
    };
    cmd.current_dir(cwd).process_group(0);
    for (k, v) in envv {
        cmd.env(k, v);
    }
    if let Some(f) = log_f {
        if let Ok(errf) = f.try_clone() {
            cmd.stdout(Stdio::from(f));
            cmd.stderr(Stdio::from(errf));
        }
    }
    let mut child = cmd.spawn().ok()?;
    let pid = child.id() as i32;
    let deadline = std::time::Instant::now()
        + Duration::from_millis((settings.healthcheck_seconds.max(0.4) * 1000.0) as u64);
    while std::time::Instant::now() < deadline {
        if !pid_alive(pid) {
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(200));
    }
    if !pid_alive(pid) {
        let _ = child.wait();
        return None;
    }
    thread::spawn(move || {
        let _ = child.wait();
    });
    Some(pid)
}

#[derive(Clone, Debug)]
pub struct RunResult {
    pub pid: i32,
    pub path: PathBuf,
    pub mode: String,
}
pub fn run_conky(
    cfg_path: &Path,
    position_override: Option<&Position>,
    color_override: Option<&ColorOverride>,
) -> Result<RunResult, String> {
    if !cfg_path.is_file() {
        return Err(format!("config not found: {}", cfg_path.display()));
    }
    if !conky_available() {
        return Err("Conky is not installed".into());
    }
    let settings = load_settings();
    let running = prune_runtime().processes.len() as i32;
    if running >= settings.max_instances.max(1) {
        return Err(format!(
            "maximum managed instances reached ({})",
            settings.max_instances
        ));
    }
    let mut candidates: Vec<(String, PathBuf, ThemeLaunchBundle)> = Vec::new();
    if settings.full_visual_launch {
        if let Ok(b) = create_theme_launch_bundle(cfg_path, true, position_override, color_override) {
            candidates.push(("full-visual".into(), b.launch_config.clone(), b));
        }
    }
    let theme_root = resolve_theme_root(cfg_path);
    ensure_compat_install_symlink(&theme_root);
    let fonts = theme_root.join("fonts");
    let fallback = ThemeLaunchBundle {
        source_config: resolve_path(cfg_path),
        theme_root: theme_root.clone(),
        launch_dir: theme_root.clone(),
        launch_config: resolve_path(cfg_path),
        fonts_dir: if fonts.is_dir() { Some(fonts) } else { None },
        ..Default::default()
    };
    candidates.push(("original-root".into(), resolve_path(cfg_path), fallback));
    let mut last_err = "unknown error".to_string();
    for (name, path, bundle) in candidates {
        let envv = build_launch_env(&bundle);
        if let Some(pid) = start_conky_with_healthcheck(&path, &bundle.launch_dir, &envv) {
            register_managed_process(ManagedProcess {
                pid,
                theme_id: theme_id(cfg_path),
                config_path: theme_id(cfg_path),
                launch_config: path.to_string_lossy().into(),
                started_at: timestamp() as i64,
                preview: false,
            });
            let _ = fs::write(
                data_dir().join("last-launch.txt"),
                format!(
                    "mode={name}\nconfig={}\nlaunch={}\npath_fixes={}\nmissing_assets={}\n",
                    cfg_path.display(),
                    path.display(),
                    bundle.path_fixes,
                    bundle.missing_assets.join(", ")
                ),
            );
            record_recent(cfg_path);
            return Ok(RunResult {
                pid,
                path,
                mode: name,
            });
        }
        last_err = format!("{name} process exited during startup");
    }
    Err(format!(
        "theme failed to start: {} ({last_err})",
        cfg_path.display()
    ))
}
pub fn restart_theme(cfg: &Path) -> Result<RunResult, String> {
    let pos = resolve_theme_position(cfg);
    let col = resolve_theme_colors(cfg);
    stop_managed_theme(cfg);
    thread::sleep(Duration::from_millis(120));
    run_conky(cfg, pos.as_ref(), col.as_ref())
}

#[derive(Clone, Debug, Default)]
pub struct HealthIssue {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub fix: String,
}
#[derive(Clone, Debug, Default)]
pub struct ThemeHealth {
    pub status: String,
    pub issues: Vec<HealthIssue>,
}
impl ThemeHealth {
    pub fn status_label(&self) -> String {
        match self.status.as_str() {
            "broken" => "Broken".into(),
            "needs_fix" => "Needs Fix".into(),
            _ => "Stable".into(),
        }
    }
    pub fn report_text(&self) -> String {
        if self.issues.is_empty() {
            return "No issues detected. This theme looks stable.".into();
        }
        self.issues
            .iter()
            .map(|i| format!("[{}] {}: {}\n  {}", i.severity, i.code, i.message, i.fix))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn exec_command_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\$\{(?:exec|execi|execpi|execp|texeci|texecpi)\s+(?:\d+\s+)?([^}]+)\}")
            .unwrap()
    })
}
pub fn extract_referenced_commands(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut add_line = |line: &str| {
        let mut line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        line = line.strip_prefix("sudo ").unwrap_or(line);
        let cmd = line
            .split_whitespace()
            .next()
            .map(|s| Path::new(s.trim_matches(|c| c == '"' || c == '\'')).file_name().unwrap_or_default().to_string_lossy().to_string())
            .unwrap_or_default();
        if !cmd.is_empty() && !cmd.contains(['=', '$']) {
            seen.insert(cmd);
        }
    };
    for m in exec_command_pattern().captures_iter(text) {
        add_line(&m[1]);
    }
    let low = text.to_lowercase();
    for c in TRACKED_COMMANDS {
        if low.contains(c) {
            seen.insert((*c).into());
        }
    }
    let mut out: Vec<_> = seen.into_iter().collect();
    out.sort();
    out
}
pub fn collect_theme_dependencies(cfg: &Path) -> (Vec<String>, Vec<String>) {
    let root = resolve_theme_root(cfg);
    let mut text = read_text(cfg);
    for e in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        let ext = e.path().extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext == "sh" || ext == "lua" {
            text.push('\n');
            text.push_str(&read_text(e.path()));
        }
    }
    let ignore = [
        "conky", "echo", "cat", "grep", "sed", "head", "tail", "tr", "cut", "date", "sleep",
        "true", "false", "test", "printf", "ls", "basename", "dirname", "uname", "whoami",
    ];
    let mut present = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for cmd in extract_referenced_commands(&text) {
        if ignore.contains(&cmd.as_str()) || !seen.insert(cmd.clone()) {
            continue;
        }
        if which(&cmd) {
            present.push(cmd);
        } else {
            missing.push(cmd);
        }
    }
    (present, missing)
}
pub fn suggested_install_command(pkgs: &[String]) -> String {
    if pkgs.is_empty() {
        return String::new();
    }
    let mapped: Vec<String> = pkgs
        .iter()
        .map(|p| if p == "sensors" { "lm_sensors".into() } else { p.clone() })
        .collect();
    let joined = mapped.join(" ");
    if which("dnf") {
        format!("sudo dnf install -y {joined}")
    } else if which("apt") {
        format!("sudo apt install -y {joined}")
    } else if which("pacman") {
        format!("sudo pacman -S --needed {joined}")
    } else if which("zypper") {
        format!("sudo zypper install -y {joined}")
    } else {
        format!("install: {joined}")
    }
}
pub fn inspect_theme_health(cfg: &Path, full: bool) -> ThemeHealth {
    let mut h = ThemeHealth {
        status: "stable".into(),
        issues: Vec::new(),
    };
    let mut add = |sev: &str, code: &str, msg: &str, fix: &str| {
        h.issues.push(HealthIssue {
            severity: sev.into(),
            code: code.into(),
            message: msg.into(),
            fix: fix.into(),
        });
    };
    if !cfg.is_file() {
        h.status = "broken".into();
        add(
            "error",
            "missing_config",
            "Configuration file is missing.",
            "Restore the theme or re-import it.",
        );
        return h;
    }
    if !is_valid_theme(cfg) {
        h.status = "broken".into();
        add(
            "error",
            "syntax",
            "File does not look like a Conky configuration.",
            "Open the config and repair conky.config / TEXT sections.",
        );
    }
    let (_, _, missing) = rewrite_theme_asset_paths(&read_text(cfg), &resolve_theme_root(cfg), None);
    for m in missing {
        let low = m.to_lowercase();
        let (code, fix) = if low.ends_with(".lua") {
            ("missing_lua", "Place the missing Lua file in the theme folder.")
        } else if low.ends_with(".sh") {
            ("missing_script", "Place the missing script in the theme folder.")
        } else if low.ends_with(".png") || low.ends_with(".jpg") || low.ends_with(".jpeg") || low.ends_with(".svg") {
            ("missing_image", "Place the missing image or icon in the theme folder.")
        } else if low.ends_with(".ttf") || low.ends_with(".otf") {
            ("missing_font", "Install the font or copy it into the theme fonts folder.")
        } else {
            ("broken_path", "Place the missing file in the theme folder or update the path.")
        };
        add("warning", code, &format!("Missing asset: {m}"), fix);
    }
    let (_, miss_cmds) = collect_theme_dependencies(cfg);
    for c in miss_cmds {
        let fix = suggested_install_command(&[c.clone()]);
        add("warning", "missing_dependency", &format!("Missing command: {c}"), &fix);
    }
    let de = detect_desktop_environment();
    let kind = display_server_kind();
    if kind == "wayland" && matches!(de.as_str(), "hyprland" | "sway" | "niri") {
        add(
            "info",
            "wayland",
            &format!("Running on {de} (Wayland). Conky may need XWayland or compositor window rules."),
            "Keep own_window enabled and try Layer = above if the theme is invisible.",
        );
    }
    let ver = conky_version();
    let content = read_text(cfg);
    if content.contains("conky.config") && ver.to_lowercase().contains("conky 1.9") {
        add(
            "warning",
            "conky_version",
            &format!("Theme uses Lua syntax but Conky reports an older version: {ver}"),
            "Install Conky 1.10+ for this theme.",
        );
    }
    if full {
        match create_theme_launch_bundle(cfg, true, resolve_theme_position(cfg).as_ref(), resolve_theme_colors(cfg).as_ref())
        {
            Ok(bundle) => {
                if !validate_conky_config(&bundle.launch_config, &bundle.launch_dir, &build_launch_env(&bundle)) {
                    h.status = "broken".into();
                    add(
                        "error",
                        "runtime",
                        "Conky rejected the launch configuration.",
                        "Check manager.log, then repair the config.",
                    );
                }
            }
            Err(e) => {
                h.status = "broken".into();
                add("error", "launch_bundle", &e.to_string(), "Repair the theme files, then scan again.");
            }
        }
    }
    let has_err = h.issues.iter().any(|i| i.severity == "error");
    let has_warn = h.issues.iter().any(|i| i.severity == "warning");
    h.status = if has_err {
        "broken".into()
    } else if has_warn {
        "needs_fix".into()
    } else {
        "stable".into()
    };
    h
}

pub fn theme_details(cfg: &Path) -> String {
    let root = resolve_theme_root(cfg);
    let health = inspect_theme_health(cfg, false);
    let size = dir_size(&root);
    let pos = resolve_theme_position(cfg).unwrap_or_else(|| parse_theme_position(&read_text(cfg)));
    format!(
        "Name: {}\nConfig: {}\nFolder: {}\nSize: {}\nHealth: {}\nDesktop: {}\nPosition: {}  gap {},{}\nMonitor: {}\nFavorite: {}\nRunning: {}\n\n{}",
        root.file_name().and_then(|s| s.to_str()).unwrap_or("theme"),
        cfg.display(),
        root.display(),
        format_bytes(size),
        health.status_label(),
        compositor_label(),
        pos.alignment,
        pos.gap_x,
        pos.gap_y,
        if pos.monitor.is_empty() { "primary".into() } else { pos.monitor },
        is_favorite(cfg),
        is_theme_running(cfg),
        health.report_text()
    )
}
fn dir_size(root: &Path) -> u64 {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
fn format_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / 1024.0 / 1024.0)
    }
}

pub fn backup_theme_config(cfg: &Path, reason: &str) -> io::Result<PathBuf> {
    let root = resolve_theme_root(cfg);
    let dir = backup_root().join(root.file_name().unwrap_or_default());
    fs::create_dir_all(&dir)?;
    let dest = dir.join(format!(
        "{}.{}.{}.bak",
        cfg.file_name().and_then(|s| s.to_str()).unwrap_or("conky.conf"),
        timestamp(),
        normalize_folder_name(reason)
    ));
    fs::copy(cfg, &dest)?;
    Ok(dest)
}
pub fn repair_theme_config(cfg: &Path) -> io::Result<usize> {
    if !cfg.is_file() { return Err(io::Error::new(io::ErrorKind::NotFound, "theme config not found")); }
    let original = read_text(cfg);
    if original.trim().is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidData, "theme config is empty")); }
    let _ = backup_theme_config(cfg, "before-repair")?;
    let settings = load_settings();
    let pos = resolve_theme_position(cfg);
    let mut content = if is_new_syntax(&original) {
        patch_new_syntax(&original, &settings, pos.as_ref())
    } else {
        patch_legacy_syntax(&original, &settings, pos.as_ref())
    };
    let root = resolve_theme_root(cfg);
    let (rewritten, fixes, missing) = rewrite_theme_asset_paths(&content, &root, None);
    content = rewritten;
    if !missing.is_empty() { /* keep the config usable; report is generated by the next scan */ }
    let changed = content != original;
    if changed { fs::write(cfg, &content)?; }
    Ok(fixes as usize + usize::from(changed))
}
pub fn list_backups(cfg: &Path) -> Vec<PathBuf> {
    let root = resolve_theme_root(cfg);
    let dir = backup_root().join(root.file_name().unwrap_or_default());
    let base = cfg.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let mut out: Vec<_> = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.starts_with(base))
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out.reverse();
    out
}
pub fn restore_backup(cfg: &Path, backup: &Path) -> io::Result<()> {
    fs::copy(backup, cfg)?;
    Ok(())
}
pub fn backup_theme_tree(theme: &Path) -> io::Result<PathBuf> {
    let name = theme.file_name().and_then(|s| s.to_str()).unwrap_or("theme");
    let destination = backup_root().join(format!("{}-{}", name, timestamp()));
    copy_tree(theme, &destination)?;
    Ok(destination)
}

pub fn trash_path(path: &Path) -> io::Result<()> {
    if which("gio") {
        if Command::new("gio").args(["trash", &path.to_string_lossy()]).status()?.success() {
            return Ok(());
        }
    }
    let trash = home().join(".local/share/Trash/files");
    fs::create_dir_all(&trash)?;
    let mut dest = trash.join(path.file_name().unwrap_or_default());
    if dest.exists() {
        dest = trash.join(format!(
            "{}-{}",
            path.file_name().and_then(|s| s.to_str()).unwrap_or("theme"),
            timestamp()
        ));
    }
    fs::rename(path, dest)
}
pub fn clone_theme(cfg: &Path, new_name: &str) -> io::Result<PathBuf> {
    let root = resolve_theme_root(cfg);
    let parent = root.parent().unwrap_or(Path::new("."));
    let mut name = normalize_folder_name(new_name);
    if name.is_empty() {
        name = format!("{}-copy", root.file_name().and_then(|s| s.to_str()).unwrap_or("theme"));
    }
    let dest = unique_destination(parent, &name);
    copy_tree(&root, &dest)?;
    let rel = cfg.strip_prefix(&root).unwrap_or(Path::new(cfg.file_name().unwrap_or_default()));
    let new_cfg = dest.join(rel);
    let rewritten = read_text(&new_cfg).replace(
        &root.to_string_lossy().to_string(),
        &dest.to_string_lossy().to_string(),
    );
    fs::write(&new_cfg, rewritten)?;
    if let Some(pos) = resolve_theme_position(cfg) {
        save_theme_position(&new_cfg, pos);
    }
    if let Some(c) = resolve_theme_colors(cfg) {
        save_theme_colors(&new_cfg, c);
    }
    Ok(new_cfg)
}
pub fn rename_theme(cfg: &Path, new_name: &str) -> io::Result<PathBuf> {
    let root = resolve_theme_root(cfg);
    let parent = root.parent().unwrap_or(Path::new("."));
    let name = normalize_folder_name(new_name);
    if name.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid name"));
    }
    let dest = parent.join(&name);
    if dest == root {
        return Ok(cfg.to_path_buf());
    }
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("a folder named {name} already exists"),
        ));
    }
    fs::rename(&root, &dest)?;
    let rel = cfg.strip_prefix(&root).unwrap_or(Path::new(cfg.file_name().unwrap_or_default()));
    let new_cfg = dest.join(rel);
    let rewritten = read_text(&new_cfg).replace(
        &root.to_string_lossy().to_string(),
        &dest.to_string_lossy().to_string(),
    );
    fs::write(&new_cfg, rewritten)?;
    remap_stored_path(cfg, &new_cfg);
    Ok(new_cfg)
}
fn remap_stored_path(old: &Path, new: &Path) {
    let old_id = theme_id(old);
    let new_id = theme_id(new);
    let mut pos = load_positions();
    if let Some(v) = pos.themes.remove(&old_id) {
        pos.themes.insert(new_id.clone(), v);
        save_positions(&pos);
    }
    let mut cols = load_colors();
    if let Some(v) = cols.themes.remove(&old_id) {
        cols.themes.insert(new_id.clone(), v);
        save_colors(&cols);
    }
    let mut lib = load_library();
    if lib.favorites.remove(&old_id).is_some() {
        lib.favorites.insert(new_id.clone(), true);
    }
    for e in &mut lib.recent {
        if e.path == old_id {
            e.path = new_id.clone();
        }
    }
    save_library(&lib);
}
pub fn export_theme_archive(cfg: &Path, dest: &Path) -> io::Result<()> {
    let root = resolve_theme_root(cfg);
    let status = Command::new("tar")
        .args([
            "-C",
            &root.parent().unwrap_or(Path::new(".")).to_string_lossy(),
            "-czf",
            &dest.to_string_lossy(),
            &root.file_name().unwrap_or_default().to_string_lossy(),
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        let file = fs::File::create(dest)?;
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        archive.append_dir_all(root.file_name().unwrap_or_default(), &root)?;
        archive.into_inner()?.finish()?;
        Ok(())
    }
}

pub fn archive_kind(path: &Path) -> &'static str {
    let lower = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    for ext in [
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".tbz2", ".txz", ".zip", ".tar", ".7z",
    ] {
        if lower.ends_with(ext) {
            return ext.trim_start_matches('.');
        }
    }
    ""
}
pub fn is_archive(p: &Path) -> bool {
    !archive_kind(p).is_empty()
}
fn unsafe_archive_path(name: &str) -> bool {
    let name_s = name.replace('\\', "/");
    let clean = Path::new(&name_s).components().fold(PathBuf::new(), |mut acc, c| {
        acc.push(c.as_os_str());
        acc
    });
    let clean_s = clean.to_string_lossy().replace('\\', "/");
    name.is_empty()
        || name_s.starts_with('/')
        || clean_s == ".."
        || clean_s.starts_with("../")
        || name_s.contains("/../")
        || name.contains("..\\")
}
fn path_within(root: &Path, candidate: &Path) -> bool {
    let r = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let c = if candidate.exists() {
        fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf())
    } else {
        candidate.to_path_buf()
    };
    c == r || c.starts_with(&r)
}

struct ExtractLimits {
    max_bytes: u64,
    max_files: usize,
    files: usize,
    bytes: u64,
}
impl ExtractLimits {
    fn add(&mut self, n: u64) -> io::Result<()> {
        self.files += 1;
        self.bytes += n;
        if self.files > self.max_files {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("archive contains too many files (limit {})", self.max_files),
            ));
        }
        if self.bytes > self.max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive extracted size exceeds the security limit",
            ));
        }
        Ok(())
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> io::Result<()> {
    let file = fs::File::open(archive)?;
    let mut z = zip::ZipArchive::new(file).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut lim = ExtractLimits {
        max_bytes: MAX_ARCHIVE_BYTES,
        max_files: MAX_ARCHIVE_FILES,
        files: 0,
        bytes: 0,
    };
    for i in 0..z.len() {
        let f = z
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let name = f.name().to_string();
        if unsafe_archive_path(&name) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unsafe path in archive: {name}")));
        }
        if name.matches('/').count() > MAX_ARCHIVE_DEPTH {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("archive path is nested too deeply: {name}")));
        }
        if f.unix_mode().unwrap_or(0) & 0o120000 == 0o120000 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("symlinks are not allowed in theme archives: {name}")));
        }
        drop(f);
    }
    for i in 0..z.len() {
        let mut f = z
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let name = f.mangled_name();
        let out = dest.join(&name);
        if !path_within(dest, &out) && !out.starts_with(dest) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "archive entry escapes destination"));
        }
        if f.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        lim.add(f.size())?;
        fs::create_dir_all(out.parent().unwrap())?;
        let mut o = fs::File::create(out)?;
        io::copy(&mut f, &mut o)?;
    }
    Ok(())
}
fn extract_tar_stream<R: io::Read>(r: R, dest: &Path) -> io::Result<()> {
    let mut archive = tar::Archive::new(r);
    let mut lim = ExtractLimits {
        max_bytes: MAX_ARCHIVE_BYTES,
        max_files: MAX_ARCHIVE_FILES,
        files: 0,
        bytes: 0,
    };
    for entry in archive.entries()? {
        let mut e = entry?;
        let path = e.path()?.into_owned();
        let name = path.to_string_lossy().to_string();
        if unsafe_archive_path(&name) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unsafe path in archive: {name}")));
        }
        if name.matches('/').count() > MAX_ARCHIVE_DEPTH {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("archive path is nested too deeply: {name}")));
        }
        match e.header().entry_type() {
            tar::EntryType::Symlink | tar::EntryType::Link => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("symlinks are not allowed in theme archives: {name}"),
                ));
            }
            tar::EntryType::Directory => {
                fs::create_dir_all(dest.join(&path))?;
            }
            tar::EntryType::Regular => {
                lim.add(e.size())?;
                e.unpack_in(dest)?;
            }
            _ => {}
        }
    }
    Ok(())
}
pub fn extract_archive(archive: &Path, dest: &Path) -> io::Result<()> {
    let kind = archive_kind(archive);
    if kind.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported archive format: {}", archive.display()),
        ));
    }
    fs::create_dir_all(dest)?;
    match kind {
        "zip" => extract_zip(archive, dest),
        "7z" => {
            let bin = if which("7z") {
                "7z"
            } else if which("7za") {
                "7za"
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "7z archives require p7zip (7z command) to be installed",
                ));
            };
            let st = Command::new(bin)
                .args(["x", &archive.to_string_lossy(), &format!("-o{}", dest.display()), "-y"])
                .status()?;
            if st.success() {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::Other, "7z extraction failed"))
            }
        }
        "tar.xz" | "txz" | "xz" => {
            let st = Command::new("tar")
                .args(["-xJf", &archive.to_string_lossy(), "-C", &dest.to_string_lossy()])
                .status()?;
            if st.success() {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::Other, "xz extraction failed"))
            }
        }
        "tar.bz2" | "tbz2" | "bz2" => {
            let f = fs::File::open(archive)?;
            let dec = bzip2::read::BzDecoder::new(f);
            extract_tar_stream(dec, dest)
        }
        "tar.gz" | "tgz" | "gz" => {
            let f = fs::File::open(archive)?;
            let dec = flate2::read::GzDecoder::new(f);
            extract_tar_stream(dec, dest)
        }
        "tar" => {
            let f = fs::File::open(archive)?;
            extract_tar_stream(f, dest)
        }
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "unsupported archive")),
    }
}
fn theme_folder_for_config(cfg: &Path, extract_root: &Path) -> PathBuf {
    let mut folder = cfg.parent().unwrap_or(extract_root).to_path_buf();
    let base = folder
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if (base == "config" || base == "conky" || base == "scripts") && folder.parent() != Some(extract_root)
    {
        if let Some(p) = folder.parent() {
            folder = p.to_path_buf();
        }
    }
    if let Ok(rel) = folder.strip_prefix(extract_root) {
        let mut comps = rel.components();
        if let Some(first) = comps.next() {
            if comps.next().is_some() {
                return extract_root.join(first);
            }
        }
    }
    folder
}
fn discover_install_units(extract_root: &Path) -> Vec<PathBuf> {
    let mut units: BTreeMap<String, PathBuf> = BTreeMap::new();
    for e in WalkDir::new(extract_root).into_iter().filter_map(Result::ok) {
        if !e.file_type().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy();
        if looks_like_config_name(&name) && is_valid_theme(e.path()) {
            let u = theme_folder_for_config(e.path(), extract_root);
            units.insert(resolve_path(&u).to_string_lossy().into(), u);
        }
    }
    if !units.is_empty() {
        let mut out: Vec<_> = units.into_values().collect();
        out.sort_by(|a, b| {
            a.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .cmp(&b.file_name().unwrap_or_default().to_string_lossy().to_lowercase())
        });
        return out;
    }
    let children: Vec<_> = fs::read_dir(extract_root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    if children.len() == 1 {
        return children;
    }
    if !children.is_empty() {
        return children;
    }
    vec![extract_root.to_path_buf()]
}
fn install_tree(source: &Path, target_base: &Path, preferred_name: &str) -> io::Result<String> {
    fs::create_dir_all(target_base)?;
    let mut folder = preferred_name.to_string();
    if folder.is_empty() {
        folder = source
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("theme")
            .into();
    }
    folder = normalize_folder_name(&folder);
    let dest = unique_destination(target_base, &folder);
    copy_tree(source, &dest)?;
    ensure_compat_install_symlink(&dest);
    Ok(dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(folder))
}
pub fn import_folder_to_conky(source: &Path, target_base: Option<&Path>) -> io::Result<Vec<String>> {
    if !source.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a folder"));
    }
    let base = target_base.map(|p| p.to_path_buf()).unwrap_or_else(conky_dir);
    let valid = WalkDir::new(source).into_iter().filter_map(Result::ok).any(|e| e.file_type().is_file() && looks_like_config_name(&e.file_name().to_string_lossy()) && is_valid_theme(e.path()));
    if !valid { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Folder does not contain a valid Conky configuration")); }
    let name = install_tree(
        source,
        &base,
        source.file_name().and_then(|s| s.to_str()).unwrap_or("Imported Theme"),
    )?;
    Ok(vec![name])
}
pub fn import_folder(src: &Path) -> io::Result<String> {
    Ok(import_folder_to_conky(src, None)?.into_iter().next().unwrap_or_default())
}
pub fn import_archive_to_conky(archive: &Path, target_base: Option<&Path>) -> io::Result<Vec<String>> {
    if !archive.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "archive not found"));
    }
    if archive_kind(archive).is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported archive format (zip, tar.gz, tar.xz, tar.bz2, 7z)",
        ));
    }
    let base = target_base.map(|p| p.to_path_buf()).unwrap_or_else(conky_dir);
    let tmp = tempfile::Builder::new()
        .prefix("conky-import-")
        .tempdir_in(&app_dir())
        .or_else(|_| tempfile::tempdir())?;
    extract_archive(archive, tmp.path())?;
    let vis: Vec<_> = fs::read_dir(tmp.path())?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    let work_root = if vis.len() == 1 && vis[0].path().is_dir() {
        vis[0].path()
    } else {
        tmp.path().to_path_buf()
    };
    let default_name = normalize_folder_name(
        work_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("theme"),
    );
    let units = discover_install_units(&work_root);
    let mut installed = Vec::new();
    if units.len() == 1 {
        installed.push(install_tree(&units[0], &base, &default_name)?);
    } else {
        for u in units {
            let n = u
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("theme");
            installed.push(install_tree(&u, &base, n)?);
        }
    }
    Ok(installed)
}
pub fn import_archive(src: &Path) -> io::Result<Vec<String>> {
    import_archive_to_conky(src, None)
}
pub fn import_dropped_path(path: &Path) -> io::Result<Vec<String>> {
    if path.is_dir() {
        import_folder_to_conky(path, None)
    } else if is_archive(path) {
        import_archive_to_conky(path, None)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "drop a theme folder or archive",
        ))
    }
}

pub fn add_custom_theme_dir(dir: &Path) -> io::Result<()> {
    let mut s = load_settings();
    let p = dir.to_string_lossy().to_string();
    if !s.custom_theme_dirs.iter().any(|d| d == &p) {
        s.custom_theme_dirs.push(p.clone());
        save_settings(&s)?;
    }
    let mut lib = load_library();
    if !lib.custom_dirs.iter().any(|d| d == &p) {
        lib.custom_dirs.push(p);
        save_library(&lib);
    }
    Ok(())
}

pub fn write_autostart(enable: bool, profile: &str) -> io::Result<()> {
    let path = home().join(".config/autostart/conky-manager.desktop");
    let legacy = home().join(".config/autostart/conky-kde-manager.desktop");
    if !enable {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&legacy);
        return Ok(());
    }
    let profile = profile.trim();
    if profile.is_empty() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "a profile is required")); }
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("conkio"));
    let escape = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%");
    let exec = format!("\"{}\" --run-profile \"{}\"", escape(&exe.display().to_string()), escape(profile));
    let desktop = format!("[Desktop Entry]\nType=Application\nName=Conkio\nComment=Start Conky profile on login\nExec={exec}\nTerminal=false\nHidden=false\nX-GNOME-Autostart-enabled=true\nX-GNOME-Autostart-Delay=3\nX-KDE-autostart-after=panel\nX-KDE-StartupNotify=false\nStartupNotify=false\n");
    fs::write(&path, desktop)?;
    let _ = fs::remove_file(legacy);
    Ok(())
}

pub fn run_profile_by_name(name: &str) -> Result<usize, String> {
    if !conky_available() {
        return Err("Conky is not installed".into());
    }
    let profiles = load_profiles();
    let mut items = if let Some(dp) = profiles.desktop.get(name) {
        dp.themes.clone()
    } else {
        profiles
            .profiles
            .get(name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|path| ProfileTheme {
                path,
                enabled: true,
                ..Default::default()
            })
            .collect()
    };
    items.retain(|t| t.enabled && Path::new(&t.path).is_file());
    if items.is_empty() {
        return Err(format!("profile {name} has no runnable themes"));
    }
    stop_all_managed();
    let max_n = load_settings().max_instances.max(1) as usize;
    items.truncate(max_n);
    let mut started = 0;
    for t in items {
        if t.startup_delay > 0 {
            thread::sleep(Duration::from_secs(t.startup_delay as u64));
        }
        let mut pos = t.position.clone();
        let pos_ptr = if pos.enabled || !pos.alignment.is_empty() || !t.monitor.is_empty() {
            if !t.monitor.is_empty() {
                pos.monitor = t.monitor.clone();
                if let Some(mon) = find_monitor(&t.monitor, pos.xinerama_head) {
                    pos.xinerama_head = mon.head;
                }
            }
            Some(pos)
        } else {
            None
        };
        let col = if t.colors.enabled && !t.colors.colors.is_empty() {
            Some(t.colors.clone())
        } else {
            resolve_theme_colors(Path::new(&t.path))
        };
        if run_conky(Path::new(&t.path), pos_ptr.as_ref(), col.as_ref()).is_ok() {
            started += 1;
        }
    }
    let mut pf = load_profiles();
    pf.last_profile = name.into();
    let _ = save_profiles(&pf);
    Ok(started)
}

pub fn capture_profile(name: &str, paths: &[String]) -> DesktopProfile {
    let themes = paths
        .iter()
        .map(|path| {
            let mut pt = ProfileTheme {
                path: path.clone(),
                enabled: true,
                ..Default::default()
            };
            if let Some(pos) = resolve_theme_position(Path::new(path)) {
                pt.monitor = pos.monitor.clone();
                pt.position = pos;
            }
            if let Some(c) = resolve_theme_colors(Path::new(path)) {
                pt.colors = c;
            }
            pt
        })
        .collect();
    DesktopProfile {
        name: name.into(),
        themes,
        autostart: false,
    }
}

pub fn pick_native_color(initial: &str) -> Option<String> {
    let start = if initial.is_empty() { "#A9B1D6" } else { initial };
    if which("zenity") {
        let o = Command::new("zenity")
            .args(["--color-selection", &format!("--color={start}")])
            .output()
            .ok()?;
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let n = normalize_color(&s);
            if !n.is_empty() {
                return Some(n);
            }
        }
    }
    if which("kdialog") {
        let o = Command::new("kdialog")
            .args(["--getcolor", start])
            .output()
            .ok()?;
        if o.status.success() {
            let n = normalize_color(String::from_utf8_lossy(&o.stdout).trim());
            if !n.is_empty() {
                return Some(n);
            }
        }
    }
    None
}

pub fn theme_fingerprint() -> u64 {
    // Cheap metadata-only fingerprint. Never call discover_themes() here: that performs
    // health checks and process scans and used to make the idle UI expensive.
    let mut n = 0u64;
    for root in all_theme_roots() {
        let Ok(meta) = fs::metadata(&root) else { continue };
        n = n.wrapping_mul(31).wrapping_add(root.to_string_lossy().len() as u64);
        if let Ok(modified) = meta.modified() {
            n = n.wrapping_add(modified.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten().take(512) {
                if let Ok(m) = entry.metadata() {
                    n = n.wrapping_mul(31).wrapping_add(entry.file_name().to_string_lossy().len() as u64);
                    if let Ok(t) = m.modified() { n = n.wrapping_add(t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()); }
                }
            }
        }
    }
    n
}

