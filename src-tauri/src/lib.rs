//! The Tiiny App Farm launcher.
//!
//! A window, a tray, and the farm command line tool carried inside the app as
//! its engine. Nothing here decides what an install means; the engine does,
//! and this draws the answer.

pub mod catalog;
pub mod engine;
pub mod progress;
pub mod settings;
pub mod state;
pub mod tray;

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
    /// An app id handed to the launcher by a `tiinyfarm://install/<id>` link
    /// before the window was ready to hear about it.
    pub pending_deep_link: Mutex<Option<String>>,
}

type Answer<T> = Result<T, EngineError>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub version: String,
    pub farm: String,
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
        let mut args: Vec<&str> = vec!["start", &id];
        if let Some(port) = &port {
            args.push("--port");
            args.push(port);
        }
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

/// Save the Tiiny's address and key. The key goes down the engine's standard
/// input and is dropped as soon as it has been written: it is never an
/// argument, never an environment variable, and never in a log.
#[tauri::command]
async fn device_save(app: tauri::AppHandle, base: String, key: String) -> Answer<DeviceState> {
    on_engine(app.clone(), move |engine| engine.device(&base, &key)).await?;
    let state = device_current(app.state::<Launcher>());
    let _ = app.emit("apps:changed", ());
    Ok(state)
}

/// The same thing, with the key read out of a file this process opens itself.
/// TiinyOS can write the key out rather than have somebody read it off a
/// screen, and a key that is never on a clipboard is a key that never ends up
/// somewhere else.
#[tauri::command]
async fn device_save_from_file(
    app: tauri::AppHandle,
    base: String,
    path: String,
) -> Answer<DeviceState> {
    let file = PathBuf::from(&path);
    on_engine(app.clone(), move |engine| {
        let key = std::fs::read_to_string(&file).map_err(|_| {
            EngineError::plain(format!(
                "There is no readable key file at {}.",
                file.display()
            ))
        })?;
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(EngineError::plain(format!(
                "The file at {} is empty.",
                file.display()
            )));
        }
        engine.device(&base, &key)
    })
    .await?;
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
            farm_install,
            farm_update,
            farm_start,
            farm_stop,
            farm_remove,
            farm_log,
            device_current,
            device_probe,
            device_save,
            device_save_from_file,
            settings_read,
            settings_write,
            take_deep_link,
            open_external,
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
            app.manage(Launcher {
                engine,
                settings: Mutex::new(held.clone()),
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
                .inner_size(1100.0, 720.0)
                .min_inner_size(720.0, 560.0)
                .resizable(true)
                .build()?;
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
