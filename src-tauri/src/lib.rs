//! The Tiiny App Farm launcher.
//!
//! A window, a tray, and the farm command line tool carried inside the app as
//! its engine. Nothing here decides what an install means; the engine does,
//! and this draws the answer.

pub mod appwindow;
pub mod catalog;
pub mod engine;
pub mod progress;
pub mod settings;
pub mod state;
pub mod tray;
pub mod trouble;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use engine::{Budget, Engine, EngineError};
use settings::Settings;

/// Everything the app holds for as long as it is open.
pub struct Launcher {
    pub engine: Engine,
    pub settings: Mutex<Settings>,
    /// Where each app's window was last left, so it comes back the same size.
    pub frames: Mutex<appwindow::Frames>,
    /// An app id handed to the launcher by a `tiinyfarm://install/<id>` link
    /// before the window was ready to hear about it.
    pub pending_deep_link: Mutex<Option<String>>,
}

type Answer<T> = Result<T, EngineError>;

/// The window the launcher opens with, and the smallest it will go. Both are
/// in logical points, and both are what docs/shots was measured at.
pub const WINDOW: (f64, f64) = (1100.0, 720.0);
pub const SMALLEST: (f64, f64) = (720.0, 560.0);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub version: String,
    pub farm: String,
    /// Set when the engine was installed from a directory rather than from
    /// PyPI, so nothing can quietly claim a published version it is not.
    pub farm_from: Option<String>,
    /// The interpreter every engine command and every app runs under.
    pub python_path: String,
    pub python: String,
    pub target: String,
    pub runtime_files: u64,
    pub runtime_bytes: u64,
    pub apps_dir: String,
    pub config_dir: String,
    pub catalog: String,
    pub site: String,
}

#[tauri::command]
fn launcher_info(app: tauri::State<'_, Launcher>) -> Info {
    Info {
        version: env!("CARGO_PKG_VERSION").to_string(),
        farm: app.engine.stamp.farm.clone(),
        farm_from: app.engine.stamp.farm_from.clone(),
        python_path: app.engine.python.display().to_string(),
        python: app.engine.stamp.python.clone(),
        target: app.engine.stamp.target.clone(),
        runtime_files: app.engine.stamp.files,
        runtime_bytes: app.engine.stamp.bytes,
        apps_dir: app.engine.apps_dir().display().to_string(),
        config_dir: app.engine.config_dir().display().to_string(),
        catalog: catalog::CATALOG.to_string(),
        site: catalog::SITE.to_string(),
    }
}

async fn on_engine<F, T>(app: tauri::AppHandle, work: F) -> Answer<T>
where
    F: FnOnce(Engine) -> Answer<T> + Send + 'static,
    T: Send + 'static,
{
    let engine = app.state::<Launcher>().engine.clone();
    tauri::async_runtime::spawn_blocking(move || work(engine))
        .await
        .map_err(|error| {
            EngineError::plain(format!("The launcher lost track of that job: {error}."))
        })?
}

#[tauri::command]
async fn farm_list(app: tauri::AppHandle) -> Answer<Value> {
    on_engine(app, |engine| engine.json(&["list"], Budget::PATIENT)).await
}

#[tauri::command]
async fn farm_status(app: tauri::AppHandle) -> Answer<Value> {
    on_engine(app, |engine| engine.json(&["status"], Budget::QUICK)).await
}

#[tauri::command]
async fn farm_check(app: tauri::AppHandle) -> Answer<Value> {
    on_engine(app, |engine| engine.json(&["check"], Budget::PATIENT)).await
}

#[tauri::command]
async fn farm_doctor(app: tauri::AppHandle) -> Answer<Value> {
    on_engine(app, |engine| engine.json(&["doctor"], Budget::PATIENT)).await
}

#[tauri::command]
async fn farm_manifest(id: String) -> Result<Value, String> {
    catalog::manifest(&id).await
}

/// Look for a Tiiny. The engine does the looking: the USB cable, this network
/// and the TiinyOS client, all three, in one answer. The launcher does not have
/// a second idea of where a Tiiny might be.
///
/// The search puts packets on the wire, so it is the first thing that meets
/// macOS Local Network privacy, and the first thing that makes the prompt
/// appear. That is on purpose: it happens on a screen that is already asking
/// about the network, rather than later, in the middle of an install.
#[tauri::command]
async fn device_find(app: tauri::AppHandle) -> Answer<Value> {
    let answer = on_engine(app, |engine| {
        engine.json(&["device", "--find"], Budget::PATIENT)
    })
    .await;
    match answer {
        // An engine older than the command it was asked for says so in words
        // rather than showing somebody argparse's usage line. The window then
        // offers the manual address, which has always worked.
        Err(error) if engine::finder_missing(&error.message) => Ok(json!({
            "command": "device",
            "ok": false,
            "blocked": false,
            "unsupported": true,
            "found": [],
        })),
        other => other,
    }
}

/// Install, with the engine's own commentary arriving in the window as it is
/// printed. The `--yes` is not a shortcut past the confirmation: the card
/// showed what the app needs and what it asks for before this was pressed,
/// which is the prompt the CLI prints, moved earlier.
#[tauri::command]
async fn farm_install(app: tauri::AppHandle, id: String) -> Answer<Value> {
    work(app, id, "install").await
}

#[tauri::command]
async fn farm_update(app: tauri::AppHandle, id: String) -> Answer<Value> {
    work(app, id, "update").await
}

async fn work(app: tauri::AppHandle, id: String, verb: &'static str) -> Answer<Value> {
    let watcher = app.clone();
    let watched = id.clone();
    let run = on_engine(app.clone(), move |engine| {
        engine
            .json_watching(&[verb, &id, "--yes"], Budget::DOWNLOAD, move |line| {
                if let Some(step) = progress::read_line(line) {
                    let _ = watcher.emit(
                        "install:step",
                        json!({
                            "id": watched,
                            "phase": step.phase,
                            "line": step.line,
                            "fraction": step.phase.fraction(),
                            "bytes": progress::expected_bytes(&step.line),
                        }),
                    );
                }
            })
            .map(|run| run.value)
    })
    .await?;
    let _ = app.emit("apps:changed", ());
    tray::refresh(&app);
    Ok(run)
}

#[tauri::command]
async fn farm_start(app: tauri::AppHandle, id: String, port: Option<u16>) -> Answer<Value> {
    let answer = on_engine(app.clone(), move |engine| {
        let port = port.map(|p| p.to_string());
        let ours = engine.python.to_string_lossy().to_string();
        let mut args: Vec<&str> = vec!["start", &id];
        if let Some(port) = &port {
            args.push("--port");
            args.push(port);
        }
        // Naming the interpreter is not a preference, it is the whole of the
        // local network story. macOS grants that access per application, and
        // every Python the launcher starts is attributed to the launcher, so
        // the one grant has to cover the app as well. `--python` also stops the
        // engine walking to another interpreter on its own.
        args.push("--python");
        args.push(&ours);
        // The engine waits for readiness itself, up to its ten second ceiling,
        // so the budget only has to cover that plus the work around it.
        engine.json(&args, Budget::PATIENT)
    })
    .await?;
    let _ = app.emit("apps:changed", ());
    tray::refresh(&app);
    Ok(answer)
}

#[tauri::command]
async fn farm_stop(app: tauri::AppHandle, id: String) -> Answer<Value> {
    let answer = on_engine(app.clone(), move |engine| {
        engine.json(&["stop", &id], Budget::PATIENT)
    })
    .await?;
    let _ = app.emit("apps:changed", ());
    tray::refresh(&app);
    Ok(answer)
}

/// Remove is the one command with no `--json` mode in farm 0.1.11, so the
/// launcher reads the sentence it prints and the code it exits with.
#[tauri::command]
async fn farm_remove(app: tauri::AppHandle, id: String, purge: bool) -> Answer<String> {
    let said = on_engine(app.clone(), move |engine| {
        let mut args: Vec<&str> = vec!["remove", &id];
        if purge {
            args.push("--purge");
        }
        engine.prose(&args, Budget::PATIENT)
    })
    .await?;
    let _ = app.emit("apps:changed", ());
    tray::refresh(&app);
    Ok(said)
}

/// The last lines of an app's own log, which is what a crashed app leaves
/// behind and what somebody needs to see instead of a dead icon.
#[tauri::command]
fn farm_log(
    app: tauri::State<'_, Launcher>,
    id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    if id.contains(['/', '\\', ':']) || id.contains("..") {
        return Err(format!("{id} is not an app id."));
    }
    let path = app.engine.apps_dir().join(&id).join("farm.log");
    let body = std::fs::read_to_string(&path).map_err(|_| {
        format!(
            "There is no log at {} yet. The app has not been started on this computer.",
            path.display()
        )
    })?;
    let wanted = lines.unwrap_or(40);
    let kept: Vec<&str> = body.lines().rev().take(wanted).collect();
    Ok(kept.into_iter().rev().collect::<Vec<_>>().join("\n"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceState {
    /// The address on file, if there is one. The key is never sent to the page.
    pub base: Option<String>,
    pub configured: bool,
}

#[tauri::command]
fn device_current(app: tauri::State<'_, Launcher>) -> DeviceState {
    let path = app.engine.config_dir().join("device.json");
    let base = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| {
            value
                .get("base")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    DeviceState {
        configured: base.is_some(),
        base,
    }
}

#[tauri::command]
async fn device_probe(base: Option<String>) -> catalog::Probe {
    let address = base.unwrap_or_else(|| catalog::WELL_KNOWN_BASE.to_string());
    catalog::probe(&address).await
}

/// Save the Tiiny's address and key. There is one way in and this is it: the
/// person pastes the key, and it goes down the engine's standard input and is
/// dropped as soon as it has been written. It is never an argument, never an
/// environment variable, never in a log, and the window never shows it again.
///
/// There is deliberately no way to hand the launcher a path to a key file. A
/// path field is a second way in that teaches somebody to leave their key
/// lying about in a file, and the test harness that needs one
/// (`scripts/verify-launcher.mjs`) pipes it into the engine itself, outside
/// the app.
#[tauri::command]
async fn device_save(app: tauri::AppHandle, base: String, key: String) -> Answer<DeviceState> {
    on_engine(app.clone(), move |engine| engine.device(&base, &key)).await?;
    let state = device_current(app.state::<Launcher>());
    let _ = app.emit("apps:changed", ());
    Ok(state)
}

#[tauri::command]
fn settings_read(app: tauri::State<'_, Launcher>) -> Settings {
    app.settings.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
fn settings_write(app: tauri::AppHandle, next: Settings) -> Result<Settings, String> {
    let launcher = app.state::<Launcher>();
    next.write(&launcher.engine.config_dir())?;
    apply_autostart(&app, next.autostart);
    if let Ok(mut held) = launcher.settings.lock() {
        *held = next.clone();
    }
    Ok(next)
}

fn apply_autostart(app: &tauri::AppHandle, wanted: bool) {
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        let _ = if wanted {
            manager.enable()
        } else {
            manager.disable()
        };
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    let _ = (app, wanted);
}

/// An app id the launcher was handed by a link before the window could ask.
#[tauri::command]
fn take_deep_link(app: tauri::State<'_, Launcher>) -> Option<String> {
    app.pending_deep_link
        .lock()
        .ok()
        .and_then(|mut held| held.take())
}

#[tauri::command]
fn open_external(app: tauri::AppHandle, url: String) -> Result<(), String> {
    // Only the two places the launcher ever sends somebody: an app running on
    // this computer, and the farm's own site.
    let allowed = url.starts_with("http://localhost:")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with(catalog::SITE);
    if !allowed {
        return Err(format!("The launcher does not open {url}."));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| format!("That link would not open: {error}."))
}

/// Open a running app in its own window, or bring its window back.
///
/// One window per app, labelled with the app id, so pressing Open twice brings
/// the same window forward rather than making a second one. Closing it does not
/// stop the app: the app is the engine's process and the window is only a way
/// of looking at it.
#[tauri::command]
async fn open_app(app: tauri::AppHandle, id: String, name: Option<String>) -> Answer<Value> {
    let in_browser = app
        .state::<Launcher>()
        .settings
        .lock()
        .map(|held| held.open_in_browser)
        .unwrap_or(false);

    let wanted = id.clone();
    let status = on_engine(app.clone(), move |engine| {
        engine.json(&["status"], Budget::QUICK)
    })
    .await?;
    let url = tray::rows_from(&status)
        .into_iter()
        .find(|row| row.id == wanted)
        .and_then(|row| row.url)
        .ok_or_else(|| {
            EngineError::plain(format!(
                "{wanted} is not running, so there is nothing to open yet. Press Start first."
            ))
        })?;

    if in_browser {
        open_external(app, url.clone()).map_err(EngineError::plain)?;
        return Ok(json!({"id": id, "url": url, "where": "browser"}));
    }
    let title = name.unwrap_or_else(|| id.clone());
    show_app_window(&app, &id, &title, &url)?;
    Ok(json!({"id": id, "url": url, "where": "window"}))
}

/// Build the window, or raise the one that is already there.
fn show_app_window(
    app: &tauri::AppHandle,
    id: &str,
    title: &str,
    url: &str,
) -> Result<(), EngineError> {
    let label = appwindow::label_for(id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    let origin = appwindow::origin_of(url).ok_or_else(|| {
        EngineError::plain(format!(
            "{id} is answering at {url}, which is not an address on this computer, so the launcher will not open a window on it."
        ))
    })?;
    let parsed = tauri::Url::parse(url).map_err(|_| {
        EngineError::plain(format!("{id} gave a link the launcher cannot read: {url}."))
    })?;

    let launcher = app.state::<Launcher>();
    let frame = launcher.frames.lock().ok().and_then(|held| held.sane(id));
    let downloads = app.path().download_dir().ok();

    let opener = app.clone();
    let inside = origin.clone();
    let mut builder = tauri::WebviewWindowBuilder::new(app, &label, WebviewUrl::External(parsed))
        .title(title)
        .resizable(true)
        .min_inner_size(400.0, 320.0)
        // An app's window is that app. A link somewhere else opens in the
        // system browser, because this window has no address bar and nobody
        // could tell where they had ended up.
        .on_navigation(move |going| {
            if appwindow::stays_inside(&inside, going.as_str()) {
                return true;
            }
            use tauri_plugin_opener::OpenerExt;
            let _ = opener.opener().open_url(going.to_string(), None::<&str>);
            false
        })
        // A download goes to the Downloads folder, where somebody will look for
        // it, rather than wherever the web view would otherwise have put it.
        .on_download(move |_, event| {
            if let tauri::webview::DownloadEvent::Requested { url, destination } = event {
                if let Some(dir) = &downloads {
                    let named = url
                        .path_segments()
                        .and_then(|mut parts| parts.next_back())
                        .filter(|name| !name.is_empty() && !name.contains(['/', '\\']))
                        .unwrap_or("download")
                        .to_string();
                    *destination = dir.join(named);
                }
            }
            true
        });
    if let Some(frame) = frame {
        builder = builder
            .inner_size(frame.width, frame.height)
            .position(frame.x, frame.y);
    } else {
        builder = builder
            .inner_size(appwindow::FIRST_TIME.0, appwindow::FIRST_TIME.1)
            .center();
    }

    let window = builder.build().map_err(|error| {
        EngineError::plain(format!("{id} could not be given a window: {error}."))
    })?;

    // Where it was left, kept as it moves, written out when it closes. Closing
    // a window is not stopping an app, so nothing else happens here.
    let remembering = app.clone();
    let remembered = id.to_string();
    let watched = window.clone();
    window.on_window_event(move |event| {
        if !matches!(
            event,
            tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::CloseRequested { .. }
        ) {
            return;
        }
        let (Ok(size), Ok(position), Ok(scale)) = (
            watched.inner_size(),
            watched.outer_position(),
            watched.scale_factor(),
        ) else {
            return;
        };
        let frame = appwindow::Frame {
            width: size.width as f64 / scale,
            height: size.height as f64 / scale,
            x: position.x as f64 / scale,
            y: position.y as f64 / scale,
        };
        let closing = matches!(event, tauri::WindowEvent::CloseRequested { .. });
        let where_to = {
            let launcher = remembering.state::<Launcher>();
            let Ok(mut held) = launcher.frames.lock() else {
                return;
            };
            held.remember(&remembered, frame);
            closing.then(|| (held.clone(), launcher.engine.config_dir()))
        };
        if let Some((held, config)) = where_to {
            let _ = held.write(&config);
        }
    });

    // The app's own icon, where the platform shows one. macOS puts the
    // application's icon on every window of an application and ignores this,
    // which is why it is a best effort rather than a step that can fail.
    let ident = id.to_string();
    let decorated = window.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(icon) = catalog::icon_bytes(&ident).await {
            if let Ok(image) = tauri::image::Image::from_bytes(&icon) {
                let _ = decorated.set_icon(image);
            }
        }
    });
    Ok(())
}

/// Which apps have a window open right now, so the grid can say Open or Show.
#[tauri::command]
fn open_app_windows(app: tauri::AppHandle) -> Vec<String> {
    let windows = app.webview_windows();
    appwindow::open_ids(windows.keys().map(String::as_str))
}

#[tauri::command]
fn show_window(app: tauri::AppHandle) {
    raise(&app);
}

pub fn raise(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// `tiinyfarm://install/story-lantern` becomes `story-lantern`, and anything
/// else becomes nothing.
pub fn app_id_in(link: &str) -> Option<String> {
    let rest = link.strip_prefix("tiinyfarm://")?;
    let rest = rest
        .strip_prefix("install/")
        .or_else(|| rest.strip_prefix("install"))?;
    let id = rest
        .trim_start_matches('/')
        .split(['/', '?', '#'])
        .next()?
        .trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return None;
    }
    Some(id.to_string())
}

pub fn handle_link(app: &tauri::AppHandle, link: &str) {
    if let Some(id) = app_id_in(link) {
        if let Ok(mut held) = app.state::<Launcher>().pending_deep_link.lock() {
            *held = Some(id.clone());
        }
        let _ = app.emit("deep-link:install", id);
        raise(app);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for arg in argv.iter().skip(1) {
                if arg.starts_with("tiinyfarm://") {
                    handle_link(app, arg);
                }
            }
            raise(app);
        }));
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            launcher_info,
            farm_list,
            farm_status,
            farm_check,
            farm_doctor,
            farm_manifest,
            device_find,
            farm_install,
            farm_update,
            farm_start,
            farm_stop,
            farm_remove,
            farm_log,
            device_current,
            device_probe,
            device_save,
            settings_read,
            settings_write,
            take_deep_link,
            open_external,
            open_app,
            open_app_windows,
            show_window,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let resources = app.path().resource_dir()?;
            let mut engine = Engine::locate(&resources)?;
            // A test run points the engine at a scratch home so that a real
            // machine's apps are never touched by one.
            if let Ok(home) = std::env::var("FARM_LAUNCHER_HOME") {
                if !home.trim().is_empty() {
                    engine.home = Some(PathBuf::from(home));
                }
            }
            let held = Settings::read(&engine.config_dir());
            let frames = appwindow::Frames::read(&engine.config_dir());
            app.manage(Launcher {
                engine,
                settings: Mutex::new(held.clone()),
                frames: Mutex::new(frames),
                pending_deep_link: Mutex::new(None),
            });
            apply_autostart(&handle, held.autostart);

            for arg in std::env::args().skip(1) {
                if arg.starts_with("tiinyfarm://") {
                    handle_link(&handle, &arg);
                }
            }
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let listener = handle.clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        handle_link(&listener, url.as_str());
                    }
                });
            }

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Tiiny App Farm")
                .inner_size(WINDOW.0, WINDOW.1)
                .min_inner_size(SMALLEST.0, SMALLEST.1)
                .resizable(true)
                .center()
                .build()?;
            // Measured on a MacBook Pro (Apple M5 Max), macOS 26.6.2, on
            // 2026-09-14: the builder's inner_size alone gave a window of
            // 1197 by 881 points rather than 1100 by 720. Something between
            // the window server and the builder decides a first frame for a
            // window that has never been placed. Saying it again after the
            // window exists is taken, and every screenshot in docs/shots was
            // measured at the size this line asks for.
            let _ = window.set_size(tauri::LogicalSize::new(WINDOW.0, WINDOW.1));
            let _ = window.center();
            // Closing the window does not stop the apps, and it does not stop
            // the launcher either: the tray is still there, and the apps are
            // still running.
            let hide = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hide.hide();
                }
            });

            tray::build(&handle)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the launcher could not start");
}

#[cfg(test)]
mod tests {
    use super::app_id_in;

    #[test]
    fn an_install_link_names_one_app() {
        assert_eq!(
            app_id_in("tiinyfarm://install/story-lantern"),
            Some("story-lantern".into())
        );
        assert_eq!(
            app_id_in("tiinyfarm://install/tiiny-bench/"),
            Some("tiiny-bench".into())
        );
        assert_eq!(
            app_id_in("tiinyfarm://install/onelane?from=site"),
            Some("onelane".into())
        );
    }

    #[test]
    fn a_link_that_is_not_an_install_link_is_ignored() {
        assert_eq!(app_id_in("tiinyfarm://install/"), None);
        assert_eq!(app_id_in("tiinyfarm://something-else/x"), None);
        assert_eq!(app_id_in("https://tiinyapp.farm/apps/story-lantern/"), None);
        assert_eq!(app_id_in("tiinyfarm://install/../../etc/passwd"), None);
        assert_eq!(app_id_in("tiinyfarm://install/Story Lantern"), None);
    }
}
