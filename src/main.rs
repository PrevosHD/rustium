// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    fs,
    io::{ErrorKind, Write},
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Rect, State, Url, Webview, WebviewUrl, Window,
    WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    utils::config::BackgroundThrottlingPolicy,
    webview::{NewWindowResponse, PermissionKind, PermissionResponse, WebviewBuilder},
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

const SIDEBAR: &str = "sidebar";
const SIDEBAR_WIDTH: f64 = 68.0;
const DEFAULT_URL: &str = "https://web.whatsapp.com";
const INJECT: &str = include_str!("inject.js");
// WhatsApp Web rejects the bare WKWebView/WebKitGTK user agent, so present as Safari.
#[cfg(not(windows))]
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

#[derive(Clone, Serialize, Deserialize)]
struct Service {
    id: String,
    name: String,
    url: String,
}

#[derive(Default)]
struct AppState {
    services: Vec<Service>,
    unread: HashMap<String, u32>,
    active: Option<String>,
    panel: bool,
}

type Shared<'a> = State<'a, Mutex<AppState>>;

fn label(id: &str) -> String {
    format!("svc-{id}")
}

/// Unread count from a page title such as `(3) WhatsApp`, `Inbox (1,234) - Gmail` or `(99+) Chat`.
fn unread_count(title: &str) -> u32 {
    title
        .split('(')
        .skip(1)
        .find_map(|part| part.split_once(')')?.0.trim().trim_end_matches('+').replace(',', "").parse().ok())
        .unwrap_or(0)
}

fn new_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:032x}")
}

fn store_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join("services.json"))
}

fn load(app: &AppHandle) -> tauri::Result<Vec<Service>> {
    let path = store_path(app)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            // Save the first service right away: its id names its login storage, so it must survive a restart.
            let services = vec![Service { id: new_id(), name: "WhatsApp".into(), url: DEFAULT_URL.into() }];
            save(app, &services).map_err(std::io::Error::other)?;
            return Ok(services);
        }
        // Any other read error stops the start instead of risking an overwrite of the real list.
        Err(e) => return Err(e.into()),
    };
    match serde_json::from_str(&text) {
        Ok(services) => Ok(services),
        Err(e) => {
            // Keep the unreadable file instead of overwriting it on the next save.
            eprintln!("rustium: {} is invalid ({e}), moved to services.json.bak", path.display());
            fs::rename(&path, path.with_extension("json.bak"))?;
            Ok(Vec::new())
        }
    }
}

fn save(app: &AppHandle, services: &[Service]) -> Result<(), String> {
    let path = store_path(app).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(services).map_err(|e| e.to_string())?;
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap_or(&path))?;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?; // so a power cut cannot leave an empty file behind the rename
        fs::rename(&tmp, &path)
    };
    write().map_err(|e| format!("could not save {}: {e}", path.display()))
}

/// The macOS data store of a service; its id is a 32-digit hex number.
#[cfg(target_os = "macos")]
fn store_id(id: &str) -> [u8; 16] {
    u128::from_str_radix(id, 16).unwrap_or_default().to_be_bytes()
}

#[cfg(not(target_os = "macos"))]
fn services_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    // Local, not roaming, AppData on Windows: browser caches must not travel with the user profile.
    Ok(app.path().app_local_data_dir()?.join("services"))
}

/// Deletes the stored data of services that no longer exist, e.g. removed ones.
fn purge_orphans(app: &AppHandle) {
    let keep = |app: &AppHandle| -> Vec<String> {
        app.state::<Mutex<AppState>>().lock().unwrap().services.iter().map(|s| s.id.clone()).collect()
    };
    #[cfg(target_os = "macos")]
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(stores) = app.fetch_data_store_identifiers().await else { return };
            let keep: Vec<[u8; 16]> = keep(&app).iter().map(|id| store_id(id)).collect();
            for store in stores.into_iter().filter(|s| !keep.contains(s)) {
                if let Err(e) = app.remove_data_store(store).await {
                    eprintln!("rustium: could not delete an old data store: {e}");
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    if let Ok(entries) = services_dir(app).and_then(|dir| Ok(fs::read_dir(dir)?)) {
        let keep = keep(app);
        for entry in entries.flatten() {
            if !keep.iter().any(|id| entry.file_name() == id.as_str()) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

fn open_external(app: &AppHandle, url: &Url) {
    // Only hand ordinary links to the OS; a page must not launch arbitrary URL handlers.
    if matches!(url.scheme(), "http" | "https" | "mailto")
        && let Err(e) = app.opener().open_url(url.as_str(), None::<&str>)
    {
        eprintln!("rustium: could not open {url}: {e}");
    }
}

fn open_service(window: &Window, service: &Service) -> tauri::Result<Webview> {
    let app = window.app_handle().clone();
    let url: Url = service.url.parse().map_err(|_| tauri::Error::InvalidWebviewUrl("service URL"))?;
    let host = url.host_str().map(str::to_owned);
    let builder = WebviewBuilder::new(label(&service.id), WebviewUrl::External(url))
        .initialization_script(INJECT)
        // Hidden services must keep running, or their notifications stop.
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .on_document_title_changed(|webview, title| on_title(&webview, &title))
        // Saves downloads (photos, documents) to the Downloads folder instead of dropping them.
        .on_download(|_, _| true)
        // Lets files be dragged into the page, e.g. into a chat.
        .disable_drag_drop_handler()
        // Microphone and camera (voice messages, calls) go to the service's own site without asking;
        // any other site the service navigates to gets the web engine's own prompt.
        .on_permission_request(move |webview, kind| match kind {
            PermissionKind::Microphone | PermissionKind::Camera
                if webview.url().ok().as_ref().and_then(Url::host_str) == host.as_deref() =>
            {
                PermissionResponse::Allow
            }
            _ => PermissionResponse::Default,
        })
        .on_new_window(move |url, _| {
            open_external(&app, &url);
            NewWindowResponse::Deny
        });

    // Each service gets its own cookies and storage, so several accounts of one site can run side by side.
    #[cfg(target_os = "macos")]
    let builder = builder.data_store_identifier(store_id(&service.id));
    // shortcut: a data directory per service gives each WebView2 its own browser process on Windows;
    // switch to WebView2 profiles once Tauri exposes them (wry already has with_profile_name).
    #[cfg(not(target_os = "macos"))]
    let builder = builder.data_directory(services_dir(window.app_handle())?.join(&service.id));
    #[cfg(not(windows))]
    let builder = builder.user_agent(USER_AGENT);

    let webview = window.add_child(builder, LogicalPosition::new(SIDEBAR_WIDTH, 0.0), LogicalSize::new(1.0, 1.0))?;
    webview.hide()?;
    Ok(webview)
}

fn snapshot(st: &AppState) -> serde_json::Value {
    let services: Vec<_> = st
        .services
        .iter()
        .map(|s| json!({ "id": s.id, "name": s.name, "url": s.url, "unread": st.unread.get(&s.id).copied().unwrap_or(0) }))
        .collect();
    json!({ "services": services, "active": st.active, "panel": st.panel })
}

// shortcut: on Linux, child webviews sit in a GTK box and ignore set_bounds, so the sidebar
// does not get its fixed width there; needs GtkFixed packing before Linux is usable.
fn layout(window: &Window, panel: bool) -> tauri::Result<()> {
    let size = window.inner_size()?.to_logical::<f64>(window.scale_factor()?);
    for webview in window.webviews() {
        let (x, width) = if webview.label() == SIDEBAR {
            (0.0, if panel { size.width } else { SIDEBAR_WIDTH })
        } else {
            (SIDEBAR_WIDTH, (size.width - SIDEBAR_WIDTH).max(1.0))
        };
        webview.set_bounds(Rect {
            position: LogicalPosition::new(x, 0.0).into(),
            size: LogicalSize::new(width, size.height).into(),
        })?;
    }
    Ok(())
}

/// Applies the state to the window: which service is visible, sidebar contents, dock badge.
/// Never call it while holding the state lock.
fn refresh(window: &Window) -> Result<(), String> {
    let state = window.state::<Mutex<AppState>>();
    let (snap, active, panel, total) = {
        let st = state.lock().unwrap();
        (snapshot(&st), st.active.as_deref().map(label), st.panel, st.unread.values().sum::<u32>())
    };
    for webview in window.webviews().into_iter().filter(|w| w.label() != SIDEBAR) {
        let visible = !panel && active.as_deref() == Some(webview.label());
        if visible { webview.show() } else { webview.hide() }.map_err(|e| e.to_string())?;
    }
    layout(window, panel).map_err(|e| e.to_string())?;
    if let Some(sidebar) = window.get_webview(SIDEBAR) {
        sidebar.eval(format!("render({snap})")).map_err(|e| e.to_string())?;
    }
    // shortcut: no taskbar badge on Windows (needs an overlay icon), add when Windows is tested.
    #[cfg(not(windows))]
    let _ = window.set_badge_count((total > 0).then_some(i64::from(total)));
    #[cfg(windows)]
    let _ = total;
    Ok(())
}

fn on_title(webview: &Webview, title: &str) {
    let Some(id) = webview.label().strip_prefix("svc-") else { return };
    let count = unread_count(title);
    let changed = {
        let state = webview.state::<Mutex<AppState>>();
        let mut st = state.lock().unwrap();
        st.unread.insert(id.to_string(), count) != Some(count)
    };
    if changed && let Err(e) = refresh(&webview.window()) {
        eprintln!("rustium: {e}");
    }
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn list_services(state: Shared<'_>) -> serde_json::Value {
    snapshot(&state.lock().unwrap())
}

// async: creating a webview inside a synchronous command deadlocks on Windows.
#[tauri::command]
async fn add_service(window: Window, state: Shared<'_>, name: String, url: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err("Name must be 1 to 40 characters.".into());
    }
    let url: Url = url.trim().parse().map_err(|_| "Invalid URL.".to_string())?;
    if url.scheme() != "https" {
        return Err("URL must start with https://".into());
    }
    let service = Service { id: new_id(), name: name.into(), url: url.into() };
    {
        let mut st = state.lock().unwrap();
        let mut next = st.services.clone();
        next.push(service.clone());
        save(window.app_handle(), &next)?;
        st.services = next;
        st.active = Some(service.id.clone());
        st.panel = false;
    }
    open_service(&window, &service).map_err(|e| e.to_string())?;
    refresh(&window)
}

// async: closing a webview while WebView2 creates another one deadlocks a synchronous command on Windows.
#[tauri::command]
async fn remove_service(window: Window, state: Shared<'_>, id: String) -> Result<(), String> {
    {
        let mut st = state.lock().unwrap();
        let next: Vec<_> = st.services.iter().filter(|s| s.id != id).cloned().collect();
        save(window.app_handle(), &next)?;
        st.services = next;
        st.unread.remove(&id);
        if st.active.as_deref() == Some(id.as_str()) {
            st.active = st.services.first().map(|s| s.id.clone());
        }
    }
    if let Some(webview) = window.get_webview(&label(&id)) {
        // Log the account out right away; purge_orphans deletes the files on the next start.
        if let Err(e) = webview.clear_all_browsing_data() {
            eprintln!("rustium: could not clear data of {id}: {e}");
        }
        webview.close().map_err(|e| e.to_string())?;
    }
    refresh(&window)
}

#[tauri::command]
fn switch_service(window: Window, state: Shared<'_>, id: String) -> Result<(), String> {
    {
        let mut st = state.lock().unwrap();
        if !st.services.iter().any(|s| s.id == id) {
            return Err("Unknown service.".into());
        }
        st.active = Some(id);
        st.panel = false;
    }
    refresh(&window)
}

#[tauri::command]
fn set_panel(window: Window, state: Shared<'_>, open: bool) -> Result<(), String> {
    state.lock().unwrap().panel = open;
    refresh(&window)
}

/// Called by `inject.js` inside service pages (remote origin, see capabilities/services.json).
#[tauri::command]
fn notify(webview: Webview, state: Shared<'_>, title: String, body: String) -> Result<(), String> {
    let name = {
        let st = state.lock().unwrap();
        st.services.iter().find(|s| label(&s.id) == webview.label()).map(|s| s.name.clone())
    };
    let Some(name) = name else { return Ok(()) };
    // Page content is untrusted: cap what it can put into a system notification.
    let title: String = title.chars().take(100).collect();
    let body: String = body.chars().take(300).collect();
    webview
        .app_handle()
        .notification()
        .builder()
        .title(format!("{name}: {title}"))
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let services = load(&handle)?;
    {
        let state = app.state::<Mutex<AppState>>();
        let mut st = state.lock().unwrap();
        st.active = services.first().map(|s| s.id.clone());
        st.services.clone_from(&services);
    }
    purge_orphans(&handle);

    let window = tauri::window::WindowBuilder::new(app, "main")
        .title("rustium")
        .inner_size(1200.0, 800.0)
        .min_inner_size(480.0, 320.0)
        .build()?;
    window.add_child(
        WebviewBuilder::new(SIDEBAR, WebviewUrl::App("index.html".into())),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(SIDEBAR_WIDTH, 800.0),
    )?;
    for service in &services {
        if let Err(e) = open_service(&window, service) {
            eprintln!("rustium: could not open {}: {e}", service.name);
        }
    }
    refresh(&window)?;

    let win = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Resized(_) => {
            let panel = win.state::<Mutex<AppState>>().lock().unwrap().panel;
            let _ = layout(&win, panel);
        }
        // Closing only hides the window: services keep running and keep notifying.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = win.hide();
        }
        _ => {}
    });

    let open = MenuItem::with_id(app, "open", "Open rustium", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("rustium")
        .menu(&Menu::with_items(app, &[&open, &quit])?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        // A second start only shows the running window; two instances would double every notification.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![list_services, add_service, remove_service, switch_service, set_panel, notify])
        .setup(setup)
        .build(tauri::generate_context!())
        .expect("failed to start rustium")
        .run(|app, event| {
            // The Dock icon reopens the hidden window.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    use super::unread_count;

    #[test]
    fn parses_unread_count_from_title() {
        assert_eq!(unread_count("(3) WhatsApp"), 3);
        assert_eq!(unread_count("WhatsApp"), 0);
        assert_eq!(unread_count("Inbox (12) - me@example.com - Gmail"), 12);
        assert_eq!(unread_count("Team (Work) (5)"), 5);
        assert_eq!(unread_count("(beta"), 0);
        assert_eq!(unread_count("Inbox (1,234) - Gmail"), 1234);
        assert_eq!(unread_count("(99+) Discord"), 99);
    }
}
