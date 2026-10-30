mod engine;

use engine::*;
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use std::io::Read;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU32;

slint::include_modules!();

#[derive(Default)]
struct State {
    settings: Settings,
    themes: Vec<Theme>,
    selected_path: Option<String>,
    selected: Option<usize>,
    checked_paths: std::collections::HashSet<String>,
    query: String,
    filter: i32,
    fingerprint: u64,
}
#[derive(Clone, Debug)]
enum PromptKind {
    Clone(PathBuf),
    Rename(PathBuf),
    CreateProfile(Vec<String>),
}

#[derive(Clone, Debug)]
enum TrayAction {
    Show,
    Marketplace,
    RunSelected,
    StopSelected,
    StopAll,
    Refresh,
    Profile(String),
    Quit,
}
struct ConkioTray {
    events: Arc<Mutex<Vec<TrayAction>>>,
}
impl ksni::Tray for ConkioTray {
    const MENU_ON_ACTIVATE: bool = true;
    fn id(&self) -> String {
        "com.almezali.Conkio".into()
    }
    fn title(&self) -> String {
        "Conkio".into()
    }
    fn icon_name(&self) -> String {
        "com.almezali.Conkio".into()
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.events.lock().unwrap().push(TrayAction::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let push = |events: &Arc<Mutex<Vec<TrayAction>>>, action: TrayAction| {
            let events = events.clone();
            Box::new(move |_tray: &mut Self| {
                events.lock().unwrap().push(action.clone());
            })
        };
        vec![
            ksni::menu::StandardItem {
                label: "Open Conkio".into(),
                activate: push(&self.events, TrayAction::Show),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Theme Marketplace".into(),
                activate: push(&self.events, TrayAction::Marketplace),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Run selected theme".into(),
                activate: push(&self.events, TrayAction::RunSelected),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Stop selected theme".into(),
                activate: push(&self.events, TrayAction::StopSelected),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Stop all managed themes".into(),
                activate: push(&self.events, TrayAction::StopAll),
                ..Default::default()
            }
            .into(),
            {
                let mut items = Vec::new();
                for name in load_profiles().desktop.keys().chain(load_profiles().profiles.keys()) {
                    items.push(
                        ksni::menu::StandardItem {
                            label: name.clone(),
                            activate: push(&self.events, TrayAction::Profile(name.clone())),
                            ..Default::default()
                        }
                        .into(),
                    );
                }
                if items.is_empty() {
                    items.push(
                        ksni::menu::StandardItem {
                            label: "(no profiles)".into(),
                            enabled: false,
                            ..Default::default()
                        }
                        .into(),
                    );
                }
                ksni::menu::SubMenu {
                    label: "Switch profile".into(),
                    submenu: items,
                    ..Default::default()
                }
                .into()
            },
            ksni::menu::StandardItem {
                label: "Refresh themes".into(),
                activate: push(&self.events, TrayAction::Refresh),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            ksni::menu::StandardItem {
                label: "Quit Conkio".into(),
                activate: push(&self.events, TrayAction::Quit),
                ..Default::default()
            }
            .into(),
        ]
    }
}
fn start_tray(events: Arc<Mutex<Vec<TrayAction>>>) {
    thread::spawn(move || {
        use ksni::blocking::TrayMethods;
        if let Ok(_handle) = (ConkioTray { events }).spawn() {
            loop {
                thread::park();
            }
        }
    });
}


fn visible_themes(themes: &[Theme], query: &str, filter: i32) -> Vec<Theme> {
    let q = query.trim().to_lowercase();
    themes
        .iter()
        .filter(|t| {
            let qok = q.is_empty()
                || t.name.to_lowercase().contains(&q)
                || t.path.to_lowercase().contains(&q);
            let fok = match filter {
                1 => t.favorite,
                2 => t.running,
                3 => t.recent,
                4 => t.health == "Needs Fix",
                5 => t.health == "Broken",
                _ => true,
            };
            qok && fok
        })
        .cloned()
        .collect()
}
fn theme_rows(themes: &[Theme], query: &str, filter: i32) -> Vec<ThemeRow> {
    visible_themes(themes, query, filter)
        .into_iter()
        .map(|t| ThemeRow {
            name: t.name.into(),
            path: t.path.into(),
            running: t.running,
            favorite: t.favorite,
            health: t.health.into(),
        })
        .collect()
}
fn selected_theme(st: &State) -> Option<Theme> {
    let path = st.selected_path.as_ref()?;
    st.themes.iter().find(|t| &t.path == path).cloned()
}
fn visible_theme_at(st: &State, index: i32) -> Option<Theme> {
    visible_themes(&st.themes, &st.query, st.filter).into_iter().nth(index.max(0) as usize)
}
fn selected_index(st: &State) -> i32 {
    let vis = visible_themes(&st.themes, &st.query, st.filter);
    st.selected_path
        .as_ref()
        .and_then(|p| vis.iter().position(|t| &t.path == p))
        .map(|i| i as i32)
        .unwrap_or(-1)
}
fn reload_themes(st: &mut State) {
    st.themes = discover_themes();
    st.fingerprint = theme_fingerprint();
}
fn apply_ui(ui: &ConkioWindow, st: &State) {
    ui.set_themes(ModelRc::new(VecModel::from(theme_rows(
        &st.themes, &st.query, st.filter,
    ))));
    ui.set_selected_index(selected_index(st));
    ui.set_filter_index(st.filter);
    ui.set_search_text(st.query.clone().into());
    // Cached/static environment data: do not spawn processes on every UI refresh.
    ui.set_conky_installed(conky_available());
    ui.set_conky_version(conky_version().into());
    ui.set_desktop_info(compositor_label().into());
    ui.set_light_mode(st.settings.light_mode);
    let profiles: Vec<slint::SharedString> = {
        let pf = load_profiles();
        let mut names: Vec<String> = pf.desktop.keys().cloned().collect();
        for k in pf.profiles.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
        names.sort();
        names.into_iter().map(|s| s.into()).collect()
    };
    ui.set_profiles(ModelRc::new(VecModel::from(profiles)));
    let mons = list_monitors();
    ui.set_monitors(ModelRc::new(VecModel::from(
        mons.iter()
            .map(|m| slint::SharedString::from(m.label()))
            .collect::<Vec<_>>(),
    )));
    if let Some(theme) = selected_theme(st) {
        let cfg = Path::new(&theme.path);
        ui.set_details_text(theme_details(cfg).into());
        let health = inspect_theme_health(cfg, false);
        ui.set_health_text(health.report_text().into());
        let pos = resolve_theme_position(cfg)
            .unwrap_or_else(|| parse_theme_position(&read_text(cfg)));
        ui.set_alignment(pos.alignment.clone().into());
        if let Some(idx) = ALIGNMENTS.iter().position(|a| *a == pos.alignment) {
            ui.set_alignment_index(idx as i32);
        }
        ui.set_gap_x(pos.gap_x);
        ui.set_gap_y(pos.gap_y);
        let idx = mons
            .iter()
            .position(|m| m.id == pos.monitor || m.name == pos.monitor)
            .or_else(|| mons.iter().position(|m| m.head == pos.xinerama_head))
            .unwrap_or(0);
        ui.set_monitor_index(idx as i32);
        let colors = resolve_theme_colors(cfg)
            .map(|c| c.colors)
            .unwrap_or_else(|| parse_theme_colors(&read_text(cfg)));
        let slots: Vec<String> = COLOR_SLOT_ORDER
            .iter()
            .filter_map(|s| colors.get(*s).map(|v| format!("{s}={v}")))
            .collect();
        ui.set_color_text(slots.join("\n").into());
        let backups = list_backups(cfg)
            .into_iter()
            .take(8)
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ui.set_backups_text(if backups.is_empty() {
            "No backups yet.".into()
        } else {
            backups.into()
        });
    } else {
        ui.set_details_text("Select a theme to inspect its files, health, and desktop placement.".into());
        ui.set_health_text("".into());
        ui.set_backups_text("".into());
        ui.set_color_text("".into());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Theme Marketplace — direct port of the Conky Manager (Go/GTK) marketplace.
//  Stores: GNOME-Look / KDE-Look / Pling through the OCS API (category 124).
// ─────────────────────────────────────────────────────────────────────────────
const STORE_CAT: &str = "124";
const PAGE_SIZE: usize = 24;
const ALLOWED_HOSTS: [&str; 10] = [
    "gitlab.com",
    "codeberg.org",
    "pling.com",
    "www.pling.com",
    "gnome-look.org",
    "www.gnome-look.org",
    "kde-look.org",
    "www.kde-look.org",
    "opendesktop.org",
    "www.opendesktop.org",
];
// Same order as the dropdown labels in MarketplaceWindow (main.slint).
const SORT_IDS: [&str; 3] = ["downloads", "latest", "rating"];
const CATEGORY_QUERIES: [&str; 9] = [
    "",
    "minimal",
    "system monitor",
    "weather",
    "music",
    "network",
    "cpu gpu",
    "dark",
    "light",
];

struct OnlineStore {
    id: &'static str,
    label: &'static str,
    browse_url: &'static str,
    api_host: &'static str,
    details_api_host: &'static str,
    page_base: &'static str,
}
const ONLINE_STORES: [OnlineStore; 4] = [
    OnlineStore {
        id: "gnome-look",
        label: "GNOME-Look.org",
        browse_url: "https://www.gnome-look.org/browse?cat=124&ord=downloads",
        api_host: "api.pling.com",
        details_api_host: "api.pling.com",
        page_base: "https://www.gnome-look.org/p/",
    },
    OnlineStore {
        id: "kde-look",
        label: "KDE-Look.org",
        browse_url: "https://www.kde-look.org/browse?cat=124&ord=downloads",
        api_host: "api.pling.com",
        details_api_host: "api.pling.com",
        page_base: "https://www.kde-look.org/p/",
    },
    OnlineStore {
        id: "pling",
        label: "Pling.com",
        browse_url: "https://www.pling.com/browse?cat=124&ord=downloads",
        api_host: "api.pling.com",
        details_api_host: "api.pling.com",
        page_base: "https://www.pling.com/p/",
    },
    OnlineStore {
        id: "opendesktop",
        label: "OpenDesktop.org",
        browse_url: "https://www.opendesktop.org/browse?cat=124&ord=downloads",
        api_host: "api.pling.com",
        details_api_host: "api.pling.com",
        page_base: "https://www.opendesktop.org/p/",
    },
];

fn online_store(index: i32) -> &'static OnlineStore {
    &ONLINE_STORES[(index.max(0) as usize).min(ONLINE_STORES.len() - 1)]
}

#[derive(Clone, Debug, Default)]
struct OnlineProductData {
    product_id: i64,
    name: String,
    summary: String,
    author: String,
    version: String,
    downloads: i64,
    score: f64,
    preview_url: String,
    page_url: String,
}

#[derive(Default)]
struct MarketState {
    products: Vec<OnlineProductData>,
    total: usize,
    page: u32,
    request_id: u64,
    loading: bool,
    selected: Option<usize>,
    preview_url: String,
}

fn user_agent() -> String {
    format!("Conkio/{}", VERSION)
}

// ── JSON helpers (anyToString / anyToInt / anyToFloat / asMapSlice) ──
fn any_to_string(v: Option<&serde_json::Value>) -> String {
    match v {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}
fn any_to_int(v: Option<&serde_json::Value>) -> i64 {
    match v {
        Some(serde_json::Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}
fn any_to_float(v: Option<&serde_json::Value>) -> f64 {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
fn as_map_slice(v: Option<&serde_json::Value>) -> Vec<&serde_json::Map<String, serde_json::Value>> {
    match v {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|x| x.as_object()).collect(),
        Some(serde_json::Value::Object(m)) => vec![m],
        _ => Vec::new(),
    }
}
fn strip_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        match rest[start + 1..].find('>') {
            // `<[^>]+>` needs at least one character between the brackets.
            Some(len) if len > 0 => {
                out.push_str(&rest[..start]);
                out.push(' ');
                rest = &rest[start + 1 + len + 1..];
            }
            _ => {
                out.push_str(&rest[..start + 1]);
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
// ── Download safety (hostAllowed / validateDownloadURL) ──
fn host_allowed(host: &str) -> bool {
    let host = host.to_lowercase();
    if let Some(digits) = host
        .strip_prefix("files")
        .and_then(|r| r.strip_suffix(".pling.com"))
    {
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    ALLOWED_HOSTS
        .iter()
        .any(|a| host == *a || host.ends_with(&format!(".{}", a)))
}
fn validate_download_url(raw: &str) -> Result<(), String> {
    let u = url::Url::parse(raw).map_err(|e| e.to_string())?;
    let host = u.host_str().unwrap_or("").to_lowercase();
    if u.scheme() != "https" || host.is_empty() {
        return Err("only HTTPS downloads are allowed".into());
    }
    if !host_allowed(&host) {
        return Err(format!("download host not allowed: {}", host));
    }
    Ok(())
}

// ── OCS API ──
fn ocs_request(
    api_host: &str,
    endpoint: &str,
    params: &[(&str, String)],
) -> Result<serde_json::Value, String> {
    let mut u = url::Url::parse(&format!("https://{}/ocs/v1/content/{}", api_host, endpoint))
        .map_err(|e| e.to_string())?;
    {
        let mut q = u.query_pairs_mut();
        for (k, v) in params {
            q.append_pair(k, v);
        }
        q.append_pair("format", "json");
    }
    let agent = ureq::AgentBuilder::new()
        .user_agent(&user_agent())
        .timeout(Duration::from_secs(45))
        .build();
    let resp = agent.get(u.as_str()).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("store API returned HTTP {}", code),
        other => other.to_string(),
    })?;
    let payload: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    if let Some(status) = payload.get("status").and_then(|s| s.as_str()) {
        if !status.is_empty() && status != "ok" {
            let msg = payload
                .get("message")
                .and_then(|m| m.as_str())
                .filter(|m| !m.is_empty())
                .unwrap_or("Store API request failed");
            return Err(msg.to_string());
        }
    }
    Ok(payload)
}

fn browse_online_store(
    store: &OnlineStore,
    query: &str,
    page: u32,
    per_page: usize,
    sort: &str,
) -> Result<(Vec<OnlineProductData>, usize), String> {
    let mut params: Vec<(&str, String)> = vec![
        ("page", page.to_string()),
        ("itemsperpage", per_page.to_string()),
        ("ord", sort.to_string()),
        ("categories", STORE_CAT.to_string()),
    ];
    let search = query.trim();
    if !search.is_empty() {
        params.push(("search", search.to_string()));
    } else if store.id == "kde-look" {
        params.push(("search", "conky".to_string()));
    }
    let payload = ocs_request(store.api_host, "data", &params)?;
    let total = any_to_int(payload.get("totalitems")).max(0) as usize;
    let mut products = Vec::new();
    for item in as_map_slice(payload.get("data")) {
        let tn = any_to_string(item.get("typename")).trim().to_lowercase();
        let xt = any_to_string(item.get("xdg_type")).trim().to_lowercase();
        // Category 124 is already the Conky category: reject only an explicit
        // non-Conky classification instead of losing valid catalog entries.
        if (!tn.is_empty() && tn != "conky") && (!xt.is_empty() && xt != "conky") {
            continue;
        }
        let id = any_to_int(item.get("id"));
        let mut preview = any_to_string(item.get("previewpic1"));
        if preview.is_empty() {
            preview = any_to_string(item.get("smallpreviewpic1"));
        }
        if preview.is_empty() {
            preview = any_to_string(item.get("previewpic2"));
        }
        let mut name = any_to_string(item.get("name"));
        if name.is_empty() {
            name = "Unnamed theme".into();
        }
        products.push(OnlineProductData {
            product_id: id,
            name,
            summary: strip_html(&any_to_string(item.get("summary"))),
            author: any_to_string(item.get("personid")),
            version: any_to_string(item.get("version")),
            downloads: any_to_int(item.get("downloads")),
            score: any_to_float(item.get("score")),
            preview_url: preview,
            page_url: format!("{}{}", store.page_base, id),
        });
    }
    Ok((products, total))
}

fn fetch_online_product_details(
    store: &OnlineStore,
    product_id: i64,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let payload = ocs_request(store.details_api_host, &format!("data/{}", product_id), &[])?;
    as_map_slice(payload.get("data"))
        .first()
        .map(|m| (*m).clone())
        .ok_or_else(|| format!("theme not found on {}: {}", store.label, product_id))
}

fn pick_theme_download(
    details: &serde_json::Map<String, serde_json::Value>,
) -> Result<(String, String), String> {
    let mut links: Vec<(String, String)> = Vec::new();
    for i in 1..=5 {
        let link = any_to_string(details.get(&format!("downloadlink{}", i)));
        let mut name = any_to_string(details.get(&format!("downloadname{}", i)));
        if name.is_empty() {
            name = format!("download-{}", i);
        }
        if !link.is_empty() {
            links.push((name, link));
        }
    }
    let exts = [".zip", ".tar.gz", ".tar.xz", ".tar.bz2", ".tar", ".tgz", ".txz", ".7z"];
    for (name, link) in &links {
        let lower = name.to_lowercase();
        if exts.iter().any(|e| lower.ends_with(e)) {
            return Ok((name.clone(), link.clone()));
        }
    }
    links
        .into_iter()
        .next()
        .ok_or_else(|| "no downloadable archive found for this theme".to_string())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn archive_suffix(path: &Path) -> String {
    let kind = archive_kind(path);
    if kind.is_empty() {
        String::new()
    } else {
        format!(".{kind}")
    }
}

fn download_file(
    raw_url: &str,
    dest: &Path,
    cancel: &AtomicBool,
    cb: &dyn Fn(f64, &str),
) -> Result<(), String> {
    validate_download_url(raw_url)?;
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Connect/idle timeouts (not a hard cap on the whole body) so large themes can finish.
    let agent = ureq::AgentBuilder::new()
        .user_agent(&user_agent())
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(120))
        .build();
    let resp = agent.get(raw_url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("download failed: HTTP {}", code),
        other => other.to_string(),
    })?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    if total > MAX_ARCHIVE_BYTES {
        return Err("download is too large (over 512 MiB)".into());
    }
    let mut reader = resp.into_reader();
    let mut out = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let result = (|| -> Result<(), String> {
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("download canceled".into());
            }
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            io::Write::write_all(&mut out, &buf[..n]).map_err(|e| e.to_string())?;
            downloaded += n as u64;
            if downloaded > MAX_ARCHIVE_BYTES {
                return Err("download is too large (over 512 MiB)".into());
            }
            if total > 0 {
                cb(
                    downloaded as f64 / total as f64,
                    &format!("Downloading… {} KB", downloaded / 1024),
                );
            }
        }
        Ok(())
    })();
    if result.is_err() {
        drop(out);
        let _ = fs::remove_file(dest);
        return result;
    }
    cb(1.0, "Download complete");
    Ok(())
}

fn download_and_install_product(
    store: &OnlineStore,
    product_id: i64,
    cancel: &AtomicBool,
    cb: &dyn Fn(f64, &str),
) -> Result<Vec<String>, String> {
    cb(0.05, &format!("Reading theme info from {}…", store.label));
    let details = fetch_online_product_details(store, product_id)?;
    let (download_name, download_url) = pick_theme_download(&details)?;
    cb(0.12, &format!("Downloading {}…", download_name));
    let name_path = PathBuf::from(&download_name);
    let suffix = {
        let known = archive_suffix(&name_path);
        if known.is_empty() {
            name_path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_else(|| ".zip".to_string())
        } else {
            known
        }
    };
    let archive_path = app_dir()
        .join("downloads")
        .join(format!("store-{}-{}{}", product_id, timestamp(), suffix));
    // Download progress occupies 12 % … 88 % of the bar so it never moves backwards.
    download_file(&download_url, &archive_path, cancel, &|f, m| {
        cb(0.12 + f.clamp(0.0, 1.0) * 0.76, m)
    })?;
    let result = (|| -> Result<Vec<String>, String> {
        let mut want = any_to_string(details.get("downloadsha256")).trim().to_string();
        if want.is_empty() {
            want = any_to_string(details.get("sha256")).trim().to_string();
        }
        if !want.is_empty() {
            let got = sha256_file(&archive_path)
                .map_err(|e| format!("could not hash downloaded archive: {}", e))?;
            if !got.eq_ignore_ascii_case(&want) {
                return Err(format!(
                    "SHA256 mismatch for {}\nexpected {}\ngot {}",
                    download_name, want, got
                ));
            }
        }
        cb(0.9, "Extracting and installing into ~/.conky…");
        let kind = archive_kind(&archive_path);
        let installed = if !kind.is_empty() {
            let names = import_archive(&archive_path).map_err(|e| e.to_string())?;
            if names.is_empty() {
                return Err("no theme folder was found inside the downloaded archive".into());
            }
            names
        } else {
            let theme_name = any_to_string(details.get("name"));
            let mut folder = normalize_folder_name(&theme_name);
            if folder == "conky-theme" {
                folder = normalize_folder_name(&download_name);
            }
            let dest = unique_dest(&folder);
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            let file_name = archive_path.file_name().unwrap_or_default();
            fs::copy(&archive_path, dest.join(file_name)).map_err(|e| e.to_string())?;
            vec![dest
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(folder)]
        };
        Ok(installed)
    })();
    let _ = fs::remove_file(&archive_path);
    let installed = result?;
    cb(1.0, &format!("Installed {} item(s)", installed.len()));
    Ok(installed)
}

fn fetch_preview(url: &str) -> Option<(u32, u32, Vec<u8>)> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let agent = ureq::AgentBuilder::new()
        .user_agent(&user_agent())
        .timeout(Duration::from_secs(20))
        .build();
    let resp = agent.get(url).call().ok()?;
    let mut data = Vec::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut data)
        .ok()?;
    if data.is_empty() {
        return None;
    }
    let img = image::load_from_memory(&data).ok()?.thumbnail(640, 360).to_rgba8();
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

// ── Marketplace window controller ──
fn product_row(p: &OnlineProductData) -> OnlineProduct {
    let author = if p.author.is_empty() {
        "Unknown author"
    } else {
        p.author.as_str()
    };
    OnlineProduct {
        id: p.product_id as i32,
        name: p.name.clone().into(),
        subtitle: format!(
            "{}  ·  {} downloads  ·  {:.1} rating",
            author, p.downloads, p.score
        )
        .into(),
    }
}

fn market_model_update(w: &MarketplaceWindow, rows: Vec<OnlineProduct>, append: bool) {
    let model = w.get_products();
    if let Some(vm) = model.as_any().downcast_ref::<VecModel<OnlineProduct>>() {
        if append {
            for r in rows {
                vm.push(r);
            }
        } else {
            vm.set_vec(rows);
        }
    } else {
        w.set_products(ModelRc::new(VecModel::from(rows)));
    }
}

fn market_show_product(w: &MarketplaceWindow, ms: &Arc<Mutex<MarketState>>, index: Option<usize>) {
    let store = online_store(w.get_store_index());
    let product = {
        let mut m = ms.lock().unwrap();
        let found = index.and_then(|i| m.products.get(i).cloned().map(|p| (i, p)));
        match &found {
            Some((i, p)) => {
                m.selected = Some(*i);
                m.preview_url = p.preview_url.clone();
            }
            None => {
                m.selected = None;
                m.preview_url.clear();
            }
        }
        found
    };
    w.set_preview(slint::Image::default());
    w.set_has_preview(false);
    let Some((i, p)) = product else {
        w.set_selected_index(-1);
        w.set_detail_title("Select a theme".into());
        w.set_detail_desc(
            "Choose a result to inspect its description, author, version, and store page.".into(),
        );
        w.set_detail_meta("".into());
        w.set_link_label(format!("Open {} in browser", store.label).into());
        w.set_link_url(store.browse_url.into());
        return;
    };
    w.set_selected_index(i as i32);
    w.set_detail_title(p.name.clone().into());
    let summary = if p.summary.is_empty() {
        "No description was supplied by the store.".to_string()
    } else {
        p.summary.clone()
    };
    w.set_detail_desc(summary.into());
    let version = if p.version.is_empty() {
        "not specified".to_string()
    } else {
        p.version.clone()
    };
    w.set_detail_meta(
        format!(
            "Author: {}\nDownloads: {}\nRating: {:.1}\nVersion: {}\nStore: {}",
            p.author, p.downloads, p.score, version, store.label
        )
        .into(),
    );
    w.set_link_label(format!("View details on {}", store.label).into());
    w.set_link_url(p.page_url.clone().into());
    if !p.preview_url.is_empty() {
        let purl = p.preview_url.clone();
        let weak = w.as_weak();
        let ms = ms.clone();
        thread::spawn(move || {
            if let Some((width, height, pixels)) = fetch_preview(&purl) {
                let _ = weak.upgrade_in_event_loop(move |w| {
                    if ms.lock().unwrap().preview_url != purl {
                        return;
                    }
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        &pixels, width, height,
                    );
                    w.set_preview(slint::Image::from_rgba8(buffer));
                    w.set_has_preview(true);
                });
            }
        });
    }
}

fn market_load(w: &MarketplaceWindow, ms: &Arc<Mutex<MarketState>>, reset: bool) {
    let store = online_store(w.get_store_index());
    let sort = SORT_IDS[(w.get_sort_index().max(0) as usize).min(SORT_IDS.len() - 1)];
    let extra = CATEGORY_QUERIES[(w.get_category_index().max(0) as usize).min(CATEGORY_QUERIES.len() - 1)];
    let mut query = w.get_query().to_string();
    if !extra.is_empty() && !query.to_lowercase().contains(extra) {
        query = format!("{} {}", query, extra).trim().to_string();
    }
    let (request_id, page) = {
        let mut m = ms.lock().unwrap();
        if !reset && m.loading {
            return;
        }
        if reset {
            m.page = 1;
            m.products.clear();
            m.total = 0;
        } else {
            m.page += 1;
        }
        m.loading = true;
        m.request_id += 1;
        (m.request_id, m.page)
    };
    if reset {
        market_model_update(w, Vec::new(), false);
        market_show_product(w, ms, None);
        w.set_status_text(format!("Searching {}…", store.label).into());
    } else {
        w.set_status_text("Loading more results…".into());
    }
    w.set_loading(true);
    w.set_can_load_more(false);
    let weak = w.as_weak();
    let ms = ms.clone();
    thread::spawn(move || {
        let result = browse_online_store(store, &query, page, PAGE_SIZE, sort);
        let _ = weak.upgrade_in_event_loop(move |w| {
            market_loaded(&w, &ms, request_id, reset, store, result)
        });
    });
}

fn market_loaded(
    w: &MarketplaceWindow,
    ms: &Arc<Mutex<MarketState>>,
    request_id: u64,
    reset: bool,
    store: &OnlineStore,
    result: Result<(Vec<OnlineProductData>, usize), String>,
) {
    if ms.lock().unwrap().request_id != request_id {
        return; // superseded by a newer search
    }
    w.set_loading(false);
    match result {
        Err(error) => {
            {
                let mut m = ms.lock().unwrap();
                m.loading = false;
                if !reset && m.page > 1 {
                    m.page -= 1;
                }
            }
            w.set_can_load_more(false);
            w.set_status_text(format!("Store unavailable: {}", error).into());
        }
        Ok((found, count)) => {
            let (rows, len, total) = {
                let mut m = ms.lock().unwrap();
                m.loading = false;
                let start = m.products.len();
                m.products.extend(found);
                m.total = count;
                let rows: Vec<OnlineProduct> = m.products[start..].iter().map(product_row).collect();
                (rows, m.products.len(), m.total)
            };
            market_model_update(w, rows, !reset);
            if reset && len > 0 {
                market_show_product(w, ms, Some(0));
            }
            let status = if total > 0 {
                format!("Showing {} of {} results · {}", len, total, store.label)
            } else {
                format!("Showing {} results · {}", len, store.label)
            };
            w.set_status_text(status.into());
            w.set_can_load_more(total > len);
        }
    }
}

fn open_marketplace(
    w: &MarketplaceWindow,
    main: Option<&ConkioWindow>,
    ms: &Arc<Mutex<MarketState>>,
) {
    if let Some(m) = main {
        w.set_light_mode(m.get_light_mode());
    }
    if w.window().is_visible() {
        let _ = w.show();
        return;
    }
    // A closed marketplace starts fresh, exactly like the GTK dialog.
    w.set_store_index(0);
    w.set_sort_index(0);
    w.set_category_index(0);
    w.set_query("".into());
    let _ = w.show();
    market_load(w, ms, true);
}

// ── Download dialog (progress + cancel + retry) ──
#[derive(Clone)]
struct DownloadCtx {
    main: slint::Weak<ConkioWindow>,
    dl: slint::Weak<DownloadWindow>,
    state: Arc<Mutex<State>>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

fn start_download(ctx: DownloadCtx, title: String, store_index: i32, product_id: i64) {
    ctx.busy.store(true, Ordering::SeqCst);
    ctx.cancel.store(false, Ordering::SeqCst);
    let mut light = false;
    if let Some(m) = ctx.main.upgrade() {
        light = m.get_light_mode();
        m.set_busy(true);
        m.set_status_text(format!("Downloading {}…", title).into());
    }
    if let Some(d) = ctx.dl.upgrade() {
        d.set_light_mode(light);
        d.set_item_title(title.clone().into());
        d.set_progress(0.0);
        d.set_status_text("Connecting…".into());
        let _ = d.show();
    }
    thread::spawn(move || {
        let store = online_store(store_index);
        let dl = ctx.dl.clone();
        let progress = move |fraction: f64, message: &str| {
            let message = message.to_string();
            let _ = dl.upgrade_in_event_loop(move |d| {
                d.set_progress(fraction.clamp(0.0, 1.0) as f32);
                d.set_status_text(message.into());
            });
        };
        let result = download_and_install_product(store, product_id, &ctx.cancel, &progress);
        let canceled = result.is_err() && ctx.cancel.load(Ordering::SeqCst);
        let done = ctx.clone();
        let _ = ctx.main.upgrade_in_event_loop(move |m| {
            if let Some(d) = done.dl.upgrade() {
                let _ = d.hide();
            }
            done.busy.store(false, Ordering::SeqCst);
            m.set_busy(false);
            if canceled {
                m.set_status_text("Download canceled.".into());
                return;
            }
            match result {
                Err(error) => {
                    m.set_status_text(format!("Download failed: {}", error).into());
                    let answer = rfd::MessageDialog::new()
                        .set_level(rfd::MessageLevel::Error)
                        .set_title("Download Error")
                        .set_description(format!("{}\n\nRetry this download?", error))
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show();
                    if matches!(answer, rfd::MessageDialogResult::Yes) {
                        start_download(done, title, store_index, product_id);
                    }
                }
                Ok(installed) => {
                    {
                        let mut st = done.state.lock().unwrap();
                        st.themes = discover_themes();
                        m.set_themes(ModelRc::new(VecModel::from(theme_rows(&st.themes, "", 0))));
                    }
                    m.set_status_text(format!("Installed theme: {}", title).into());
                    rfd::MessageDialog::new()
                        .set_level(rfd::MessageLevel::Info)
                        .set_title("Theme Installed")
                        .set_description(format!(
                            "{} was installed into ~/.conky\n\nFolders: {}",
                            title,
                            installed.join(", ")
                        ))
                        .set_buttons(rfd::MessageButtons::Ok)
                        .show();
                }
            }
        });
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("Conkio {}", VERSION);
        return Ok(());
    }
    ensure_dirs()?;
    if args.iter().any(|a| a == "--list-profiles") {
        let pf = load_profiles();
        let mut names: Vec<String> = pf.desktop.keys().cloned().collect();
        for n in pf.profiles.keys() { if !names.contains(n) { names.push(n.clone()); } }
        names.sort(); for n in names { println!("{n}"); } return Ok(());
    }
    if args.iter().any(|a| a == "--stop-all") { println!("Stopped {} managed theme(s)", stop_all_managed()); return Ok(()); }
    if let Some(i) = args.iter().position(|a| a == "--stop-profile") {
        let name = args.get(i + 1).map(String::as_str).unwrap_or("").trim();
        if name.is_empty() { eprintln!("missing profile name"); std::process::exit(1); }
        let count = stop_all_managed(); println!("Stopped {count} managed theme(s) from profile {name}"); return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--run-profile") {
        let name = args.get(i + 1).map(String::as_str).unwrap_or("").trim();
        if name.is_empty() { eprintln!("missing profile name"); std::process::exit(1); }
        match run_profile_by_name(name) {
            Ok(n) => { println!("Started {n} theme(s) from profile {name}"); return Ok(()); }
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        }
    }
    let ui = ConkioWindow::new()?;
    let marketplace_window = MarketplaceWindow::new()?;
    let download_window = DownloadWindow::new()?;
    let settings_window = SettingsWindow::new()?;
    let about_window = AboutWindow::new()?;
    let prompt_window = PromptWindow::new()?;
    let prompt_kind: Arc<Mutex<Option<PromptKind>>> = Arc::new(Mutex::new(None));
    let state = Arc::new(Mutex::new(State {
        settings: load_settings(),
        themes: discover_themes(),
        ..Default::default()
    }));
    let themes = theme_rows(&state.lock().unwrap().themes, "", 0);
    ui.set_themes(ModelRc::new(VecModel::from(themes)));
    marketplace_window.set_products(ModelRc::new(VecModel::<OnlineProduct>::default()));
    let market_state = Arc::new(Mutex::new(MarketState::default()));
    let busy_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    ui.set_conky_installed(conky_available());
    ui.set_conky_version(conky_version().into());
    ui.set_status_text(format!("Conkio v{} ready", VERSION).into());
    ui.set_light_mode(state.lock().unwrap().settings.light_mode);
    marketplace_window.set_light_mode(state.lock().unwrap().settings.light_mode);
    let tray_events = Arc::new(Mutex::new(Vec::<TrayAction>::new()));
    start_tray(tray_events.clone());
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_refresh(move || {
            if let Some(u) = u.upgrade() {
                let mut st = s.lock().unwrap();
                reload_themes(&mut st);
                apply_ui(&u, &st);
                u.set_status_text("Theme list refreshed".into());
            }
        });
    }
    {
        let s = state.clone();
        ui.on_check_theme(move |i, checked| {
            let mut st = s.lock().unwrap();
            let path = visible_theme_at(&st, i).map(|theme| theme.path);
            if let Some(path) = path {
                if checked { st.checked_paths.insert(path); }
                else { st.checked_paths.remove(&path); }
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_context_theme(move |i| {
            let mut st = s.lock().unwrap();
            st.selected = None;
            st.selected_path = visible_theme_at(&st, i).map(|t| t.path);
            if let Some(u) = u.upgrade() {
                apply_ui(&u, &st);
                u.set_context_index(i);
                u.set_context_visible(true);
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_select_theme(move |i| {
            let mut st = s.lock().unwrap();
            st.selected = None;
            st.selected_path = visible_theme_at(&st, i).map(|t| t.path);
            if let Some(u) = u.upgrade() {
                apply_ui(&u, &st);
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_toggle_favorite(move |i| {
            let mut st = s.lock().unwrap();
            if let Some(path) = visible_theme_at(&st, i).map(|t| t.path) {
                let Some(theme) = st.themes.iter_mut().find(|t| t.path == path) else { return; };
                theme.favorite = !theme.favorite;
                let value = theme.favorite;
                let name = theme.name.clone();
                set_favorite(&PathBuf::from(&theme.path), value);
                if let Some(u) = u.upgrade() {
                    u.set_status_text(
                        format!(
                            "{} {} favorites",
                            name,
                            if value { "added to" } else { "removed from" }
                        )
                        .into(),
                    );
                    u.set_themes(ModelRc::new(VecModel::from(theme_rows(&st.themes, "", 0))));
                }
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_backup_selected(move || {
            let st = s.lock().unwrap();
            let Some(theme) = selected_theme(&st) else {
                return;
            };
            let result = backup_theme_tree(Path::new(&theme.root));
            if let Some(u) = u.upgrade() {
                u.set_status_text(
                    match result {
                        Ok(path) => format!("Backup created: {}", path.display()),
                        Err(e) => format!("Backup failed: {}", e),
                    }
                    .into(),
                );
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_export_selected(move || {
            let st = s.lock().unwrap();
            let Some(theme) = selected_theme(&st) else {
                return;
            };
            let Some(destination) = rfd::FileDialog::new()
                .set_title("Export theme")
                .set_file_name(format!("{}.tar.gz", theme.name))
                .save_file()
            else {
                return;
            };
            let result = export_theme_archive(Path::new(&theme.path), &destination);
            if let Some(u) = u.upgrade() {
                u.set_status_text(
                    match result {
                        Ok(()) => format!("Theme exported to {}", destination.display()),
                        Err(e) => format!("Export failed: {}", e),
                    }
                    .into(),
                );
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_delete_selected(move || {
            let mut st = s.lock().unwrap();
            let Some(theme) = selected_theme(&st) else {
                return;
            };
            let result = (|| -> io::Result<()> {
                backup_theme_tree(Path::new(&theme.root))?;
                trash_path(Path::new(&theme.root))
            })();
            if result.is_ok() {
                st.themes = discover_themes();
                st.selected = None;
                st.selected_path = None;
            }
            if let Some(u) = u.upgrade() {
                u.set_selected_index(-1);
                u.set_themes(ModelRc::new(VecModel::from(theme_rows(&st.themes, "", 0))));
                u.set_status_text(
                    match result {
                        Ok(()) => format!("{} removed; backup retained", theme.name),
                        Err(e) => format!("Delete failed: {}", e),
                    }
                    .into(),
                );
            }
        });
    }
    {
        let s = state.clone();
        let prompt = prompt_window.as_weak();
        let kind = prompt_kind.clone();
        ui.on_create_profile(move || {
            let st = s.lock().unwrap();
            let mut paths: Vec<String> = st.themes.iter().filter(|t| st.checked_paths.contains(&t.path)).map(|t| t.path.clone()).collect();
            if paths.is_empty() { if let Some(theme) = selected_theme(&st) { paths.push(theme.path); } }
            drop(st);
            if paths.is_empty() { return; }
            let Some(p) = prompt.upgrade() else { return };
            *kind.lock().unwrap() = Some(PromptKind::CreateProfile(paths));
            p.set_title_text("Create profile".into());
            p.set_prompt_label("Profile name".into());
            p.set_prompt_value("My profile".into());
            let _ = p.show();
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_import_folder(move || {
            let Some(folder) = rfd::FileDialog::new()
                .set_title("Import Conky theme folder")
                .pick_folder()
            else {
                return;
            };
            let result = import_folder(&folder);
            let message = match result {
                Ok(name) => format!("Imported theme: {}", name),
                Err(e) => format!("Import failed: {}", e),
            };
            let mut st = s.lock().unwrap();
            st.themes = discover_themes();
            if let Some(u) = u.upgrade() {
                apply_ui(&u, &st);
                u.set_status_text(message.into());
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_import_archive(move || {
            let Some(archive) = rfd::FileDialog::new()
                .set_title("Import Conky theme archive")
                .add_filter("Archives", &["zip", "tar", "gz", "tgz", "bz2", "xz"])
                .pick_file()
            else {
                return;
            };
            let result = import_archive(&archive);
            let message = match result {
                Ok(names) => format!("Imported {} theme(s)", names.len()),
                Err(e) => format!("Import failed: {}", e),
            };
            let mut st = s.lock().unwrap();
            st.themes = discover_themes();
            if let Some(u) = u.upgrade() {
                apply_ui(&u, &st);
                u.set_status_text(message.into());
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_search_changed(move |query| {
            let mut st = s.lock().unwrap();
            st.query = query.to_string();
            if let Some(u) = u.upgrade() {
                apply_ui(&u, &st);
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_filter_changed(move |filter| {
            let mut st = s.lock().unwrap();
            st.filter = filter;
            if let Some(u) = u.upgrade() {
                u.set_filter_index(filter);
                u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                    &st.themes,
                    &st.query,
                    filter,
                ))));
                u.set_selected_index(selected_index(&st));
            }
        });
    }
    {
        let u = ui.as_weak();
        ui.on_monitor_changed(move |i| {
            if let Some(u) = u.upgrade() {
                u.set_monitor_index(i);
                u.set_status_text("Monitor selected — press Apply position to save".into());
            }
        });
    }
    {
        let u = ui.as_weak();
        ui.on_set_alignment(move |alignment| {
            let alignment = alignment.to_string();
            if !ALIGNMENTS.contains(&alignment.as_str()) {
                return;
            }
            if let Some(u) = u.upgrade() {
                u.set_alignment(alignment.clone().into());
                if let Some(idx) = ALIGNMENTS.iter().position(|a| *a == alignment) {
                    u.set_alignment_index(idx as i32);
                }
            }
        });
    }
    {
        let u = ui.as_weak();
        ui.on_nudge(move |dx, dy| {
            if let Some(u) = u.upgrade() {
                u.set_gap_x(u.get_gap_x() + dx);
                u.set_gap_y(u.get_gap_y() + dy);
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_stop_all(move || {
            let stopped = stop_all_managed();
            if let Some(u) = u.upgrade() {
                let mut st = s.lock().unwrap();
                st.themes = discover_themes();
                u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                    &st.themes,
                    &st.query,
                    st.filter,
                ))));
                u.set_status_text(
                    if stopped == 0 {
                        "No managed Conky processes".into()
                    } else {
                        format!("Stopped {stopped} managed theme(s)").into()
                    },
                );
            }
        });
    }
    {
        let u = ui.as_weak();
        ui.on_toggle_maximize(move || {
            if let Some(u) = u.upgrade() {
                let window = u.window();
                window.set_maximized(!window.is_maximized());
                u.set_is_maximized(window.is_maximized());
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_files_dropped(move |paths| {
            let mut imported = 0usize;
            let mut errors: Vec<String> = Vec::new();
            for path in paths.iter() {
                match import_dropped_path(&PathBuf::from(path.as_str())) {
                    Ok(names) => imported += names.len(),
                    Err(e) => errors.push(e.to_string()),
                }
            }
            if let Some(u) = u.upgrade() {
                let mut st = s.lock().unwrap();
                st.themes = discover_themes();
                apply_ui(&u, &st);
                u.set_status_text(
                    if errors.is_empty() {
                        format!("Imported {imported} theme(s) from drop").into()
                    } else {
                        format!("Import issues: {}", errors.join("; ")).into()
                    },
                );
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_scan_health(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let cfg = PathBuf::from(&theme.path);
            if let Some(u) = u.upgrade() {
                u.set_status_text(format!("Deep scanning {}…", theme.name).into());
                let health = inspect_theme_health(&cfg, true);
                u.set_health_text(health.report_text().into());
                u.set_status_text(
                    format!("Deep scan complete: {}", health.status_label()).into(),
                );
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_restore_backup(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let cfg = PathBuf::from(&theme.path);
            let Some(latest) = list_backups(&cfg).first().cloned() else {
                if let Some(u) = u.upgrade() {
                    u.set_status_text("No backups found for this theme".into());
                }
                return;
            };
            let _ = backup_theme_config(&cfg, "before-restore");
            let result = restore_backup(&cfg, &latest);
            if let Some(u) = u.upgrade() {
                u.set_status_text(
                    match result {
                        Ok(()) => format!("Restored {}", latest.display()).into(),
                        Err(e) => format!("Restore failed: {e}").into(),
                    },
                );
                let st = s.lock().unwrap();
                apply_ui(&u, &st);
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_apply_position(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let Some(u) = u.upgrade() else { return };
            let cfg = PathBuf::from(&theme.path);
            let monitor = list_monitors()
                .get(u.get_monitor_index().max(0) as usize)
                .map(|m| m.id.clone())
                .unwrap_or_default();
            let pos = Position {
                enabled: true,
                alignment: u.get_alignment().to_string(),
                gap_x: u.get_gap_x(),
                gap_y: u.get_gap_y(),
                monitor,
                ..Default::default()
            };
            save_theme_position(&cfg, pos);
            u.set_status_text(format!("Position saved for {}", theme.name).into());
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_apply_palette(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let Some(u) = u.upgrade() else { return };
            let cfg = PathBuf::from(&theme.path);
            let presets = color_presets();
            let idx = (u.get_palette_index().max(0) as usize).min(presets.len() - 1);
            let (preset_id, preset_name, _) = presets[idx];
            let existing = resolve_theme_colors(&cfg)
                .map(|c| c.colors)
                .unwrap_or_else(|| parse_theme_colors(&read_text(&cfg)));
            let merged = merge_preset_colors(&existing, preset_id);
            save_theme_colors(
                &cfg,
                ColorOverride {
                    colors: merged.clone(),
                    smart_lua: true,
                    enabled: true,
                },
            );
            let slots: Vec<String> = COLOR_SLOT_ORDER
                .iter()
                .filter_map(|slot| {
                    merged.get(*slot).map(|v| format!("{slot}={v}"))
                })
                .collect();
            u.set_color_text(slots.join("\n").into());
            u.set_status_text(
                format!(
                    "Palette {} applied to {} — restart the theme to see it",
                    preset_name, theme.name
                )
                .into(),
            );
        });
    }
    {
        let u = ui.as_weak();
        ui.on_pick_color(move || {
            let Some(u) = u.upgrade() else { return };
            let slot = u.get_active_color_slot().to_string();
            let current = u.get_color_text().lines().find(|line| line.trim_start().starts_with(&(slot.clone() + "="))).and_then(|line| line.split('=').nth(1)).unwrap_or("").trim().to_string();
            match pick_native_color(&current) {
                Some(color) => {
                    let mut lines: Vec<String> = u
                        .get_color_text()
                        .lines()
                        .map(|line| line.to_string())
                        .collect();
                    let mut replaced = false;
                    for line in lines.iter_mut() {
                        if line.trim_start().starts_with(&(slot.clone() + "=")) {
                            *line = format!("{slot}={color}");
                            replaced = true;
                        }
                    }
                    if !replaced {
                        lines.push(format!("{slot}={color}"));
                    }
                    u.set_color_text(lines.join("\n").into());
                    u.set_status_text(
                        format!("Picked {color} — press Apply colors to save").into(),
                    );
                }
                None => {
                    u.set_status_text(
                        "No native color picker found (install zenity or kdialog)".into(),
                    );
                }
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_apply_colors(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let Some(u) = u.upgrade() else { return };
            let cfg = PathBuf::from(&theme.path);
            let mut colors = BTreeMap::new();
            for line in u.get_color_text().lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let value = normalize_color(value);
                    if !value.is_empty() {
                        colors.insert(key.trim().to_string(), value);
                    }
                }
            }
            if colors.is_empty() {
                u.set_status_text("No valid colors to apply (use name=value lines)".into());
                return;
            }
            let _ = backup_theme_config(&cfg, "before-colors");
            save_theme_colors(
                &cfg,
                ColorOverride {
                    colors,
                    smart_lua: true,
                    enabled: true,
                },
            );
            u.set_status_text(
                format!("Colors saved for {} — restart the theme to apply", theme.name)
                    .into(),
            );
            let st = s.lock().unwrap();
            apply_ui(&u, &st);
        });
    }
    {
        let u = ui.as_weak();
        ui.on_run_profile(move || {
            let Some(u) = u.upgrade() else { return };
            let Some(name) = u.get_profiles().row_data(u.get_profile_index().max(0) as usize)
            else {
                return;
            };
            let name = name.to_string();
            let w = u.as_weak();
            u.set_status_text(format!("Starting profile {name}…").into());
            thread::spawn(move || {
                let result = run_profile_by_name(&name);
                let _ = w.upgrade_in_event_loop(move |u| {
                    u.set_status_text(
                        match result {
                            Ok(count) => {
                                format!("Profile {name} started ({count} theme(s))").into()
                            }
                            Err(e) => format!("Profile {name} failed: {e}").into(),
                        },
                    );
                });
            });
        });
    }
    {
        let u = ui.as_weak();
        ui.on_delete_profile(move || {
            let Some(u) = u.upgrade() else { return };
            let Some(name) = u.get_profiles().row_data(u.get_profile_index().max(0) as usize)
            else {
                return;
            };
            let name = name.to_string();
            let mut profiles = load_profiles();
            profiles.profiles.remove(&name);
            profiles.desktop.remove(&name);
            if profiles.last_profile == name {
                profiles.last_profile = profiles
                    .profiles
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_default();
            }
            match save_profiles(&profiles) {
                Ok(()) => {
                    u.set_status_text(format!("Profile {name} deleted").into());
                    let mut all_names: Vec<String> = profiles.desktop.keys().cloned().collect();
                    for name in profiles.profiles.keys() { if !all_names.contains(name) { all_names.push(name.clone()); } }
                    all_names.sort();
                    let names: Vec<SharedString> = all_names.into_iter().map(SharedString::from).collect();
                    u.set_profiles(ModelRc::new(VecModel::from(names)));
                    u.set_profile_index(0);
                }
                Err(e) => u.set_status_text(format!("Delete failed: {e}").into()),
            }
        });
    }
    {
        let u = ui.as_weak();
        ui.on_autostart_profile(move || {
            let Some(u) = u.upgrade() else { return };
            let Some(name) = u.get_profiles().row_data(u.get_profile_index().max(0) as usize)
            else {
                return;
            };
            let name = name.to_string();
            let mut profiles = load_profiles();
            for profile in profiles.desktop.values_mut() { profile.autostart = false; }
            if let Some(profile) = profiles.desktop.get_mut(&name) {
                profile.autostart = true;
            }
            profiles.last_profile = name.clone();
            match save_profiles(&profiles) {
                Ok(()) => {
                    let _ = write_autostart(true, &name);
                    u.set_status_text(
                        format!("Profile {name} will start with your desktop session").into(),
                    );
                }
                Err(e) => u.set_status_text(format!("Autostart failed: {e}").into()),
            }
        });
    }
    {
        let s = state.clone();
        let prompt = prompt_window.as_weak();
        let kind = prompt_kind.clone();
        ui.on_clone_selected(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let Some(p) = prompt.upgrade() else { return };
            *kind.lock().unwrap() = Some(PromptKind::Clone(PathBuf::from(&theme.path)));
            p.set_title_text("Clone theme".into());
            p.set_prompt_label(format!("New folder name for {} copy", theme.name).into());
            p.set_prompt_value(format!("{}-copy", theme.name).into());
            let _ = p.show();
        });
    }
    {
        let s = state.clone();
        let prompt = prompt_window.as_weak();
        let kind = prompt_kind.clone();
        ui.on_rename_selected(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let Some(p) = prompt.upgrade() else { return };
            *kind.lock().unwrap() = Some(PromptKind::Rename(PathBuf::from(&theme.path)));
            p.set_title_text("Rename theme".into());
            p.set_prompt_label(format!("New folder name for {}", theme.name).into());
            p.set_prompt_value(theme.name.clone().into());
            let _ = p.show();
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        let prompt = prompt_window.as_weak();
        let kind = prompt_kind.clone();
        prompt_window.on_accepted(move |name| {
            if let Some(p) = prompt.upgrade() {
                let _ = p.hide();
            }
            let Some(action) = kind.lock().unwrap().take() else { return };
            if let PromptKind::CreateProfile(paths) = action {
                let mut profiles = load_profiles();
                let safe_name = name.trim();
                if safe_name.is_empty() { return; }
                let entry = profiles.profiles.entry(safe_name.to_string()).or_default();
                for path in paths { if !entry.contains(&path) { entry.push(path); } }
                let snapshot = entry.clone();
                profiles.desktop.insert(safe_name.to_string(), capture_profile(safe_name, &snapshot));
                profiles.last_profile = safe_name.to_string();
                let result = save_profiles(&profiles);
                if let Some(u) = u.upgrade() { u.set_status_text(match result { Ok(()) => format!("Profile {safe_name} saved"), Err(e) => format!("Profile save failed: {e}") }.into()); }
                return;
            }
            let (cfg, is_clone) = match action {
                PromptKind::Clone(cfg) => (cfg, true),
                PromptKind::Rename(cfg) => (cfg, false),
                PromptKind::CreateProfile(_) => unreachable!(),
            };
            let result = if is_clone { clone_theme(&cfg, name.as_str()) } else { rename_theme(&cfg, name.as_str()) };
            let mut st = s.lock().unwrap();
            match result {
                Ok(new_path) => {
                    st.themes = discover_themes();
                    st.selected_path = Some(new_path.display().to_string());
                    if let Some(u) = u.upgrade() {
                        apply_ui(&u, &st);
                        u.set_status_text("Done".into());
                    }
                }
                Err(e) => {
                    if let Some(u) = u.upgrade() {
                        u.set_status_text(format!("Operation failed: {e}").into());
                    }
                }
            }
        });
    }
    {
        let prompt = prompt_window.as_weak();
        prompt_window.on_cancelled(move || {
            if let Some(p) = prompt.upgrade() {
                let _ = p.hide();
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_run_selected(move || {
            let (themes, s2) = {
                let st = s.lock().unwrap();
                let mut picked: Vec<Theme> = st.themes.iter().filter(|t| st.checked_paths.contains(&t.path)).cloned().collect();
                if picked.is_empty() { if let Some(t) = selected_theme(&st) { picked.push(t); } }
                (picked, s.clone())
            };
            if themes.is_empty() { return; }
            let u2 = u.clone();
            thread::spawn(move || {
                let mut started = 0usize;
                for theme in &themes {
                    if run_conky(Path::new(&theme.path), None, None).is_ok() { started += 1; }
                }
                let theme = themes[0].clone();
                let _ = s2.lock().unwrap().themes = discover_themes();
                let _ = u2.upgrade_in_event_loop(move |u| {
                    {
                        let st = s2.lock().unwrap();
                        u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                            &st.themes,
                            &st.query,
                            st.filter,
                        ))));
                    }
                    u.set_status_text(
                        format!("Started {started} of {} selected theme(s); last {}", themes.len(), theme.name).into(),
                    );
                });
            });
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_stop_selected(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let cfg = PathBuf::from(&theme.path);
            let stopped = stop_managed_theme(&cfg);
            let still = is_theme_running(&cfg);
            if let Some(u) = u.upgrade() {
                let mut st = s.lock().unwrap();
                st.themes = discover_themes();
                u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                    &st.themes,
                    &st.query,
                    st.filter,
                ))));
                u.set_status_text(
                    if stopped && !still {
                        format!("Stopped {}", theme.name)
                    } else if still {
                        format!("{} is still running (external process)", theme.name)
                    } else {
                        format!("{} is not managed by Conkio", theme.name)
                    }
                    .into(),
                );
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_restart_selected(move || {
            let theme = {
                let st = s.lock().unwrap();
                selected_theme(&st)
            };
            let Some(theme) = theme else { return };
            let s2 = s.clone();
            let u2 = u.clone();
            thread::spawn(move || {
                let cfg = PathBuf::from(&theme.path);
                let result = restart_theme(&cfg);
                let _ = s2.lock().unwrap().themes = discover_themes();
                let _ = u2.upgrade_in_event_loop(move |u| {
                    {
                        let st = s2.lock().unwrap();
                        u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                            &st.themes,
                            &st.query,
                            st.filter,
                        ))));
                    }
                    u.set_status_text(
                        match result {
                            Ok(r) => format!(
                                "Restarted {} (pid {}, {} mode: {})",
                                theme.name,
                                r.pid,
                                r.mode,
                                r.path.display()
                            )
                            .into(),
                            Err(e) => format!("Restart failed: {e}").into(),
                        },
                    );
                });
            });
        });
    }
    {
        let u = ui.as_weak();
        ui.on_remove_selected(move || {
            if let Some(u) = u.upgrade() {
                u.set_status_text(
                    "Removal requires confirmation in the full management view".into(),
                );
            }
        });
    }
    {
        let u = ui.as_weak();
        ui.on_navigate(move |page| {
            if let Some(u) = u.upgrade() {
                u.set_page_title(page.to_string().into());
                u.set_status_text(format!("{} workspace", page).into());
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        ui.on_toggle_theme(move || {
            let mut st = s.lock().unwrap();
            st.settings.light_mode = !st.settings.light_mode;
            let _ = save_settings(&st.settings);
            if let Some(u) = u.upgrade() {
                u.set_light_mode(st.settings.light_mode);
                u.set_status_text(if st.settings.light_mode {
                    "Light theme enabled".into()
                } else {
                    "Dark theme enabled".into()
                });
            }
        });
    }
    {
        let s = state.clone();
        let w = settings_window.as_weak();
        ui.on_open_settings(move || {
            if let Some(w) = w.upgrade() {
                let st = s.lock().unwrap();
                w.set_theme_directory(
                    st.settings
                        .theme_dir
                        .clone()
                        .unwrap_or_else(|| conky_dir().to_string_lossy().into())
                        .into(),
                );
                w.set_desktop_info(compositor_label().into());
                w.set_layer(st.settings.layer.clone().into());
                w.set_window_type(st.settings.window_type.clone().into());
                w.set_smoothness(st.settings.smoothness.clone().into());
                w.set_desktop_preset(st.settings.desktop_preset.clone().into());
                w.set_nice_level(st.settings.nice_level);
                w.set_max_instances(st.settings.max_instances);
                w.set_preview_seconds(st.settings.preview_seconds);
                w.set_login_autostart(st.settings.login_autostart);
                w.set_minimize_to_tray(st.settings.minimize_to_tray);
                let _ = w.show();
            }
        });
    }
    {
        let w = about_window.as_weak();
        ui.on_show_about(move || {
            if let Some(w) = w.upgrade() {
                let _ = w.show();
            }
        });
    }
    {
        let w = settings_window.as_weak();
        settings_window.on_close_settings(move || {
            if let Some(w) = w.upgrade() {
                let _ = w.hide();
            }
        });
    }
    {
        let s = state.clone();
        let u = ui.as_weak();
        settings_window.on_appearance_changed(move |light| {
            let mut st = s.lock().unwrap();
            st.settings.light_mode = light;
            let _ = save_settings(&st.settings);
            if let Some(u) = u.upgrade() {
                u.set_light_mode(light);
            }
        });
    }
    {
        let s = state.clone();
        let w = settings_window.as_weak();
        let u = ui.as_weak();
        settings_window.on_add_theme_folder(move || {
            let Some(folder) = rfd::FileDialog::new()
                .set_title("Add a Conky themes folder")
                .pick_folder()
            else {
                return;
            };
            match add_custom_theme_dir(&folder) {
                Ok(()) => {
                    if let Some(w) = w.upgrade() {
                        w.set_theme_directory(folder.to_string_lossy().into_owned().into());
                    }
                    if let Some(u) = u.upgrade() {
                        u.set_status_text(format!("Added {}", folder.display()).into());
                        let mut st = s.lock().unwrap();
                        st.themes = discover_themes();
                        apply_ui(&u, &st);
                    }
                }
                Err(e) => {
                    if let Some(u) = u.upgrade() {
                        u.set_status_text(format!("Add folder failed: {e}").into());
                    }
                }
            }
        });
    }
    {
        let s = state.clone();
        settings_window.on_login_toggled(move |enabled| {
            let mut st = s.lock().unwrap();
            st.settings.login_autostart = enabled;
            let _ = save_settings(&st.settings);
            if enabled {
                let _ = write_autostart(true, &load_profiles().last_profile);
            } else {
                let _ = fs::remove_file(autostart_file());
            }
        });
    }
    {
        let s = state.clone();
        settings_window.on_tray_toggled(move |enabled| {
            let mut st = s.lock().unwrap();
            st.settings.minimize_to_tray = enabled;
            let _ = save_settings(&st.settings);
        });
    }
    {
        let s = state.clone();
        let w = settings_window.as_weak();
        let u = ui.as_weak();
        settings_window.on_save_settings(move || {
            let mut st = s.lock().unwrap();
            if let Some(w) = w.upgrade() {
                st.settings.login_autostart = w.get_login_autostart();
                st.settings.minimize_to_tray = w.get_minimize_to_tray();
                st.settings.layer = w.get_layer().to_string();
                st.settings.window_type = w.get_window_type().to_string();
                st.settings.smoothness = w.get_smoothness().to_string();
                st.settings.desktop_preset = w.get_desktop_preset().to_string();
                st.settings.nice_level = w.get_nice_level();
                st.settings.max_instances = w.get_max_instances();
                st.settings.preview_seconds = w.get_preview_seconds();
            }
            let _ = save_settings(&st.settings);
            if let Some(u) = u.upgrade() {
                u.set_light_mode(st.settings.light_mode);
                u.set_status_text("Settings saved".into());
            }
            if let Some(w) = w.upgrade() {
                let _ = w.hide();
            }
        });
    }
    {
        let w = about_window.as_weak();
        about_window.on_close_about(move || {
            if let Some(w) = w.upgrade() {
                let _ = w.hide();
            }
        });
    }
    // ── Theme Marketplace (GNOME-Look / KDE-Look / Pling) ──
    let dl_ctx = DownloadCtx {
        main: ui.as_weak(),
        dl: download_window.as_weak(),
        state: state.clone(),
        busy: busy_flag.clone(),
        cancel: cancel_flag.clone(),
    };
    {
        let cancel = cancel_flag.clone();
        let d = download_window.as_weak();
        download_window.on_cancel(move || {
            cancel.store(true, Ordering::SeqCst);
            if let Some(d) = d.upgrade() {
                d.set_status_text("Canceling…".into());
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let main = ui.as_weak();
        let ms = market_state.clone();
        ui.on_open_marketplace(move || {
            if let Some(w) = w.upgrade() {
                open_marketplace(&w, main.upgrade().as_ref(), &ms);
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        marketplace_window.on_close_marketplace(move || {
            if let Some(w) = w.upgrade() {
                let _ = w.hide();
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        marketplace_window.on_store_changed(move |_| {
            if let Some(w) = w.upgrade() {
                market_load(&w, &ms, true);
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        marketplace_window.on_sort_changed(move |_| {
            if let Some(w) = w.upgrade() {
                market_load(&w, &ms, true);
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        marketplace_window.on_category_changed(move |_| {
            if let Some(w) = w.upgrade() {
                market_load(&w, &ms, true);
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        marketplace_window.on_search(move |_| {
            if let Some(w) = w.upgrade() {
                market_load(&w, &ms, true);
            }
        });
    }
    {
        // Like the GTK version: no request per keystroke, Enter searches.
        let w = marketplace_window.as_weak();
        marketplace_window.on_search_edited(move |_| {
            if let Some(w) = w.upgrade() {
                let label = online_store(w.get_store_index()).label;
                w.set_status_text(format!("Press Enter to search {}.", label).into());
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        marketplace_window.on_select_product(move |i| {
            if let Some(w) = w.upgrade() {
                market_show_product(&w, &ms, Some(i.max(0) as usize));
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        marketplace_window.on_load_more(move || {
            if let Some(w) = w.upgrade() {
                market_load(&w, &ms, false);
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        marketplace_window.on_open_link(move |link| {
            let link = link.to_string();
            if w.upgrade().is_some() && (link.starts_with("https://") || link.starts_with("http://")) {
                thread::spawn(move || {
                    let _ = Command::new("xdg-open").arg(link).status();
                });
            }
        });
    }
    {
        let w = marketplace_window.as_weak();
        let ms = market_state.clone();
        let ctx = dl_ctx.clone();
        marketplace_window.on_install_selected(move || {
            let Some(w) = w.upgrade() else { return };
            let product = {
                let m = ms.lock().unwrap();
                m.selected.and_then(|i| m.products.get(i).cloned())
            };
            let Some(product) = product else { return };
            if ctx.busy.load(Ordering::SeqCst) {
                if let Some(main) = ctx.main.upgrade() {
                    main.set_status_text("Please wait until the current operation finishes.".into());
                }
                return;
            }
            let store_index = w.get_store_index();
            let _ = w.hide();
            start_download(ctx.clone(), product.name, store_index, product.product_id);
        });
    }
    let tray_market = marketplace_window.as_weak();
    let tray_market_state = market_state.clone();
    let event_ui = ui.as_weak();
    let event_state = state.clone();
    let event_timer = Timer::default();
    let tick_counter = Arc::new(AtomicU32::new(0));
    let tick = tick_counter.clone();
    event_timer.start(TimerMode::Repeated, Duration::from_millis(1000), move || {
        let n = tick.fetch_add(1, Ordering::Relaxed);
        if n % 5 == 4 {
            let fp = theme_fingerprint();
            let mut st = event_state.lock().unwrap();
            if st.fingerprint == 0 {
                st.fingerprint = fp;
            } else if st.fingerprint != fp {
                st.fingerprint = fp;
                st.themes = discover_themes();
                if let Some(u) = event_ui.upgrade() {
                    apply_ui(&u, &st);
                }
            }
        }
        let actions = { std::mem::take(&mut *tray_events.lock().unwrap()) };
        for action in actions {
            match action {
                TrayAction::Show => {
                    if let Some(u) = event_ui.upgrade() {
                        let _ = u.show();
                    }
                }
                TrayAction::Marketplace => {
                    if let Some(w) = tray_market.upgrade() {
                        open_marketplace(&w, event_ui.upgrade().as_ref(), &tray_market_state);
                    }
                }
                TrayAction::RunSelected => {
                    let theme = {
                        let st = event_state.lock().unwrap();
                        selected_theme(&st)
                    };
                    let Some(theme) = theme else { continue };
                    let cfg = PathBuf::from(&theme.path);
                    let status = match run_conky(&cfg, None, None) {
                        Ok(r) => format!(
                            "Running {} (pid {}, {} mode: {})",
                            theme.name,
                            r.pid,
                            r.mode,
                            r.path.display()
                        ),
                        Err(e) => format!("Run failed: {e}"),
                    };
                    let mut st = event_state.lock().unwrap();
                    st.themes = discover_themes();
                    if let Some(u) = event_ui.upgrade() {
                        u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                            &st.themes,
                            &st.query,
                            st.filter,
                        ))));
                        u.set_status_text(status.into());
                    }
                }
                TrayAction::StopSelected => {
                    let theme = {
                        let st = event_state.lock().unwrap();
                        selected_theme(&st)
                    };
                    let Some(theme) = theme else { continue };
                    let cfg = PathBuf::from(&theme.path);
                    let status = if stop_managed_theme(&cfg) {
                        format!("Stopped {}", theme.name)
                    } else {
                        format!("{} is not managed by Conkio", theme.name)
                    };
                    let mut st = event_state.lock().unwrap();
                    st.themes = discover_themes();
                    if let Some(u) = event_ui.upgrade() {
                        u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                            &st.themes,
                            &st.query,
                            st.filter,
                        ))));
                        u.set_status_text(status.into());
                    }
                }
                TrayAction::StopAll => {
                    let stopped = stop_all_managed();
                    let mut st = event_state.lock().unwrap();
                    st.themes = discover_themes();
                    if let Some(u) = event_ui.upgrade() {
                        u.set_themes(ModelRc::new(VecModel::from(theme_rows(
                            &st.themes,
                            &st.query,
                            st.filter,
                        ))));
                        u.set_status_text(
                            if stopped == 0 {
                                "No managed Conky processes".into()
                            } else {
                                format!("Stopped {stopped} managed theme(s)").into()
                            },
                        );
                    }
                }
                TrayAction::Refresh => {
                    let mut st = event_state.lock().unwrap();
                    reload_themes(&mut st);
                    if let Some(u) = event_ui.upgrade() {
                        apply_ui(&u, &st);
                        u.set_status_text("Theme list refreshed".into());
                    }
                }
                TrayAction::Profile(name) => {
                    let result = run_profile_by_name(&name);
                    if let Some(u) = event_ui.upgrade() {
                        u.set_status_text(
                            match result {
                                Ok(count) => {
                                    format!("Profile {name} started ({count} theme(s))")
                                }
                                Err(e) => format!("Profile {name} failed: {e}"),
                            }
                            .into(),
                        );
                    }
                }
                TrayAction::Quit => {
                    let _ = slint::quit_event_loop();
                }
            }
        }
    });
    ui.run()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_detection() {
        assert!(is_archive(Path::new("a.zip")));
        assert!(is_archive(Path::new("a.tar.gz")));
        assert!(!is_archive(Path::new("a.conf")));
    }
    #[test]
    fn unique_names() {
        let _ = unique_dest("test");
    }
    #[test]
    fn copy_tree_preserves_nested_files() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let nested = source.path().join("scripts").join("launch.sh");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, b"#!/bin/sh\necho conkio\n").unwrap();
        let destination = target.path().join("theme");
        copy_tree(source.path(), &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("scripts/launch.sh")).unwrap(),
            fs::read(nested).unwrap()
        );
    }
    #[test]
    fn rejects_unknown_archive_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.txt");
        fs::write(&path, b"not an archive").unwrap();
        assert!(import_archive(&path).is_err());
    }
    #[test]
    fn marketplace_host_allowlist() {
        assert!(host_allowed("www.pling.com"));
        assert!(host_allowed("files04.pling.com"));
        assert!(host_allowed("dl.opendesktop.org"));
        assert!(host_allowed("gitlab.com"));
        assert!(!host_allowed("evil.com"));
        assert!(!host_allowed("pling.com.evil.com"));
        assert!(!host_allowed("evilpling.com"));
        assert!(validate_download_url("https://files04.pling.com/a.zip").is_ok());
        assert!(validate_download_url("http://www.pling.com/a.zip").is_err());
        assert!(validate_download_url("https://pling.com@evil.com/a.zip").is_err());
    }
    #[test]
    fn marketplace_text_helpers() {
        assert_eq!(strip_html("<p>Hello <b>world</b></p>\n  ok"), "Hello world ok");
        assert_eq!(strip_html("a < b and c"), "a < b and c");
        assert_eq!(normalize_folder_name("My Theme-main"), "My Theme");
        assert_eq!(normalize_folder_name("###"), "conky-theme");
    }
    #[test]
    fn marketplace_download_choice() {
        let details: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"downloadlink1":"https://files.pling.com/a.png","downloadname1":"preview.png",
                "downloadlink2":"https://files.pling.com/b.zip","downloadname2":"theme.zip"}"#,
        )
        .unwrap();
        assert_eq!(pick_theme_download(&details).unwrap().0, "theme.zip");
    }
}
