//! The Tiiny App Farm launcher.
//!
//! A window, a tray, and the farm command line tool carried inside the app as
//! its engine. Nothing here decides what an install means; the engine does,
//! and this draws the answer.

pub mod about;
pub mod appwindow;
pub mod badges;
pub mod catalog;
pub mod engine;
pub mod models;
pub mod progress;
pub mod settings;
pub mod state;
pub mod tray;
pub mod trouble;
pub mod update;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use badges::{CardRow, Mark};
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
    /// The one `farm models --watch` child, while the window is open.
    pub watch: Watch,
    /// What the Tiiny had loaded when it was last asked, kept up to date by the
    /// watch. One snapshot, so the window and the buttons never disagree.
    pub snapshot: Mutex<Option<models::Snapshot>>,
    /// Bumped every time the watch folds a change in. A full read of the device
    /// takes a moment, and a change that arrives while it is in flight is
    /// newer than the answer coming back; this is how the older answer knows
    /// not to overwrite it.
    pub snapshot_moved: AtomicU64,
    /// The newer launcher the last look found, if it found one. Held so that a
    /// window opened after the look still learns about it, and so the menu bar
    /// can say the same thing the banner does.
    pub update_ready: Mutex<Option<update::Ready>>,
}

/// One watch on the Tiiny's models, kept alive while the window is open.
///
/// The device is polled by the engine every three seconds, so this is a child
/// process rather than a timer here, and there is exactly one of it: two
/// watches would be two conversations with the same device and twice the
/// traffic for the same answer. It is stopped when the window is put away,
/// because a window nobody is looking at has no reason to keep asking.
#[derive(Default)]
pub struct Watch {
    running: Arc<AtomicBool>,
    /// Which supervisor is the live one. A stop followed quickly by a start
    /// leaves the old supervisor still inside its wait, and a flag it shares
    /// with the new one would tell it to carry on. This number tells it that
    /// its turn is over even though watching has begun again.
    generation: Arc<AtomicU64>,
    child: Arc<Mutex<Option<std::process::Child>>>,
}

impl Watch {
    /// Start watching, if it is not already. Safe to call again.
    pub fn start(&self, app: &tauri::AppHandle) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = self.running.clone();
        let generation = self.generation.clone();
        let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
        let held = self.child.clone();
        let handle = app.clone();
        let mine_still = move |generation: &AtomicU64, running: &AtomicBool| {
            running.load(Ordering::SeqCst) && generation.load(Ordering::SeqCst) == mine
        };
        std::thread::spawn(move || {
            while mine_still(&generation, &running) {
                let engine = handle.state::<Launcher>().engine.clone();
                let reporter = handle.clone();
                let spawned = engine.spawn_streaming(
                    // --json, or the watch prints the sentences a person
                    // reads and the window hears nothing it can use.
                    &["models", "--watch", "--interval", "3", "--json"],
                    move |line| match models::read_watch_line(line) {
                        Some(models::Watched::Changed(change)) => {
                            // The held snapshot is what the window and the
                            // buttons read, so the change lands in it before
                            // anything is told to look again. Without this the
                            // window reads a snapshot that never moved and a
                            // model coming or going is invisible until the
                            // whole device is read afresh.
                            let launcher = reporter.state::<Launcher>();
                            models::fold(&launcher.snapshot, &change);
                            launcher.snapshot_moved.fetch_add(1, Ordering::SeqCst);
                            let _ = reporter.emit("models:change", *change);
                            // The menu bar says which running app lost its
                            // model, and it only knows because it asks the
                            // engine again. Nothing else would make it.
                            tray::refresh(&reporter);
                        }
                        Some(models::Watched::Failed(said)) => {
                            let _ = reporter.emit("models:trouble", said);
                        }
                        None => {}
                    },
                );
                match spawned {
                    Ok(mut child) => {
                        // Somebody may have stopped the watch while the child
                        // was starting. Handing it over then would leave it
                        // running with nobody to kill it.
                        if mine_still(&generation, &running) {
                            if let Ok(mut slot) = held.lock() {
                                *slot = Some(child);
                            }
                        } else {
                            let _ = child.kill();
                            let _ = child.wait();
                            engine.pin_interpreter();
                            break;
                        }
                        // Wait for it to end, however it ends.
                        loop {
                            std::thread::sleep(std::time::Duration::from_millis(300));
                            if !mine_still(&generation, &running) {
                                break;
                            }
                            let done = held
                                .lock()
                                .ok()
                                .and_then(|mut slot| slot.as_mut().map(|child| child.try_wait()));
                            match done {
                                Some(Ok(Some(_))) | Some(Err(_)) | None => break,
                                Some(Ok(None)) => {}
                            }
                        }
                        // The engine moves the saved interpreter when it meets
                        // a Python the local network refuses, so every command
                        // this app runs pins it back afterwards. A watch is a
                        // command like any other; its owner has to do it,
                        // because nothing else knows the child has ended.
                        engine.pin_interpreter();
                    }
                    Err(error) => {
                        let _ = handle.emit("models:trouble", error.message);
                    }
                }
                if !mine_still(&generation, &running) {
                    break;
                }
                // A watch that died is restarted, after a pause, so a device
                // that went away does not become a spawn loop.
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            if let Ok(mut slot) = held.lock() {
                if let Some(mut child) = slot.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        });
    }

    /// Stop watching. The child is killed rather than left to notice.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        // Whichever supervisor is running, its turn is over, even if watching
        // starts again before it has noticed.
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut slot) = self.child.lock() {
            if let Some(mut child) = slot.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

type Answer<T> = Result<T, EngineError>;

/// The window the launcher opens with, and the smallest it will go. Both are
/// in logical points, and both are what docs/shots was measured at.
pub const WINDOW: (f64, f64) = (1100.0, 720.0);
pub const SMALLEST: (f64, f64) = (720.0, 560.0);

/// The About window's label, and the size it is fixed at. It does not resize:
/// there is one column of credits in it and nothing that a wider window would
/// show more of.
pub const ABOUT: &str = "about";
pub const ABOUT_SIZE: (f64, f64) = (420.0, 715.0);

/// The launcher's own log, in its config directory, written by `note`.
pub const LOG: &str = "launcher.log";

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

/// What the Tiiny has loaded and what is on its disk, and what that means for
/// each app in one answer.
///
/// One call rather than one per app, and the deciding happens here rather than
/// in the page, because the page having its own opinion about whether a need is
/// met is how a window ends up disagreeing with the engine it is a face on.
#[tauri::command]
async fn farm_models(app: tauri::AppHandle, apps: Option<Vec<AppNeeds>>) -> Answer<Value> {
    let before = snapshot_mark(&app);
    let answer = on_engine(app.clone(), |engine| {
        engine.json(&["models"], Budget::PATIENT)
    })
    .await?;
    Ok(hold_and_decide(
        &app,
        answer,
        before,
        apps.unwrap_or_default(),
    ))
}

/// Where the held snapshot was before a slow read of the device started.
fn snapshot_mark(app: &tauri::AppHandle) -> u64 {
    app.state::<Launcher>()
        .snapshot_moved
        .load(Ordering::SeqCst)
}

/// Take a whole-device answer, keep it, and say what it means for each app.
///
/// Every command that comes back with the full picture goes through here, so
/// there is one place that decides whether the answer in hand is still the
/// newest thing known and one shape for the window to read.
fn hold_and_decide(
    app: &tauri::AppHandle,
    answer: Value,
    before: u64,
    apps: Vec<AppNeeds>,
) -> Value {
    let mut snapshot = models::Snapshot::read(&answer);
    let launcher = app.state::<Launcher>();
    if let Ok(mut held) = launcher.snapshot.lock() {
        if launcher.snapshot_moved.load(Ordering::SeqCst) == before {
            *held = snapshot.clone();
        } else {
            // The watch folded something in while this read was in flight, so
            // what is held is newer than what came back. Answer with the newer
            // one rather than winding the window backwards.
            snapshot = held.clone();
        }
    }
    answer_about(snapshot.as_ref(), apps, Some(answer))
}

/// One answer about the Tiiny, in the shape the window reads.
///
/// Every path that tells the window about the device comes through here, the
/// slow full read and the watch alike, so the two can never come back with
/// different fields. They did once: the watch path left out how much each model
/// was short by, and the window, reading a field that was not there, put a live
/// Load button on every model the Tiiny had no room for.
fn answer_about(
    snapshot: Option<&models::Snapshot>,
    apps: Vec<AppNeeds>,
    raw: Option<Value>,
) -> Value {
    json!({
        "models": snapshot
            .map(|held| json!(held))
            .or(raw)
            .unwrap_or(Value::Null),
        // What each downloaded model is short by, so the window can draw a
        // Load button without working out for itself what fits.
        "shortBy": snapshot.map(models::Snapshot::short_by).unwrap_or_default(),
        "needs": decide(apps, snapshot),
    })
}

/// Load one model onto the Tiiny, by name, because somebody pressed Load.
///
/// The engine answers with the whole device again, so the window does not have
/// to wait for the watch to catch up with what it just asked for. Whether the
/// model fits is the engine's rule and not a second one here: it refuses with
/// its own sentence, and the pane keeps its own guard only so that a button
/// that cannot work is never offered in the first place. Loading a model can
/// take the device half a minute, so the budget is the long one.
#[tauri::command]
async fn farm_load_model(
    app: tauri::AppHandle,
    id: String,
    apps: Option<Vec<AppNeeds>>,
) -> Answer<Value> {
    let before = snapshot_mark(&app);
    let answer = on_engine(app.clone(), move |engine| {
        engine.json(&["models", "--load", &id], Budget::DOWNLOAD)
    })
    .await?;
    let decided = hold_and_decide(&app, answer, before, apps.unwrap_or_default());
    // A model arriving changes what every app can do, and the menu bar says so
    // for the ones that are running.
    tray::refresh(&app);
    Ok(decided)
}

/// The needs of every app, against the snapshot already held, with no call to
/// the device at all. This is what a watch event leads to.
#[tauri::command]
fn app_needs(app: tauri::AppHandle, apps: Vec<AppNeeds>) -> Value {
    let held = app
        .state::<Launcher>()
        .snapshot
        .lock()
        .ok()
        .and_then(|held| held.clone());
    answer_about(held.as_ref(), apps, None)
}

/// What one app declares it needs, as the window read it off the manifest.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppNeeds {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub prefers: Vec<String>,
}

fn decide(apps: Vec<AppNeeds>, snapshot: Option<&models::Snapshot>) -> Value {
    let mut out = serde_json::Map::new();
    for app in apps {
        let needs = models::needs_of(&app.needs, snapshot);
        let prefers: Vec<Value> = app
            .prefers
            .iter()
            .map(|kind| {
                json!({
                    "kind": kind,
                    "loaded": snapshot.map(|state| state.meeting(kind).is_some()),
                })
            })
            .collect();
        let met: Vec<Value> = app
            .needs
            .iter()
            .map(|kind| {
                json!({
                    "kind": kind,
                    "loaded": snapshot.map(|state| state.meeting(kind).is_some()),
                })
            })
            .collect();
        out.insert(
            app.id.clone(),
            json!({
                "needs": met,
                "prefers": prefers,
                "state": needs,
                "canStart": needs.can_start(),
                "canLoadAndStart": needs.can_load_and_start(),
                "sentence": needs.sentence(&app.name),
            }),
        );
    }
    Value::Object(out)
}

/// Start watching the device's models, or leave the watch that is already
/// running alone.
#[tauri::command]
fn models_watch(app: tauri::AppHandle) {
    let handle = app.clone();
    app.state::<Launcher>().watch.start(&handle);
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
async fn farm_start(
    app: tauri::AppHandle,
    id: String,
    port: Option<u16>,
    load: Option<bool>,
) -> Answer<Value> {
    let answer = on_engine(app.clone(), move |engine| {
        let port = port.map(|p| p.to_string());
        let ours = engine.python.to_string_lossy().to_string();
        let mut args: Vec<&str> = vec!["start", &id];
        // --load is only ever here because somebody pressed Load and start.
        if load.unwrap_or(false) {
            args.push("--load");
        }
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
        // so the budget only has to cover that plus the work around it. A start
        // that loads a model first waits for the device, which is minutes.
        engine.json(
            &args,
            if load.unwrap_or(false) {
                Budget::DOWNLOAD
            } else {
                Budget::PATIENT
            },
        )
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

/// What every card on the farm screen puts in its corners: the version waiting
/// for an app that is planted and behind, and New on an app this launcher has
/// never drawn before.
///
/// The seen map is written here, on the first run only, so that somebody
/// opening the launcher for the first time is not handed a wall of New. After
/// that, looking at the screen changes nothing; opening a card does.
#[tauri::command]
fn catalog_marks(app: tauri::AppHandle, rows: Vec<CardRow>) -> BTreeMap<String, Mark> {
    let launcher = app.state::<Launcher>();
    let held = launcher
        .settings
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    let (marks, write) = badges::marks(&rows, held.seen.as_ref());
    if let Some(seen) = write {
        let next = Settings {
            seen: Some(seen),
            ..held
        };
        // A seen map that could not be saved means New comes back next time,
        // which is a small wrong thing. Failing to draw the farm over it would
        // be a large one.
        if next.write(&launcher.engine.config_dir()).is_ok() {
            if let Ok(mut current) = launcher.settings.lock() {
                *current = next;
            }
        }
    }
    marks
}

/// Somebody opened this card, so it is not New any more.
#[tauri::command]
fn card_seen(app: tauri::AppHandle, id: String, version: Option<String>) -> Result<(), String> {
    let launcher = app.state::<Launcher>();
    let held = launcher
        .settings
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    let seen = badges::opened(held.seen.as_ref(), &id, version.as_deref());
    if held.seen.as_ref() == Some(&seen) {
        return Ok(());
    }
    let next = Settings {
        seen: Some(seen),
        ..held
    };
    next.write(&launcher.engine.config_dir())?;
    if let Ok(mut current) = launcher.settings.lock() {
        *current = next;
    }
    Ok(())
}

// --- The launcher updating itself ---------------------------------------

/// Where the launcher asks about newer versions of itself.
///
/// Normally the endpoint in tauri.conf.json, which is the farm's own feed. The
/// environment variable is for proving the thing works before there is a newer
/// release to prove it with: a real self-update can only be witnessed at the
/// release after the one that adds it. It is read from the environment and
/// never written anywhere, so no build ships pointed somewhere else.
const FEED_OVERRIDE: &str = "FARM_LAUNCHER_UPDATE_FEED";

/// Ask the feed, once.
///
/// Every failure here is quiet. A launcher that cannot reach the farm is a
/// launcher somebody is using offline, and a dialog about it would be the app
/// interrupting to say it has nothing to say.
async fn look_once(app: &tauri::AppHandle) -> Result<Option<update::Ready>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let mut builder = app.updater_builder();
    if let Ok(feed) = std::env::var(FEED_OVERRIDE) {
        let feed = feed.trim().to_string();
        if !feed.is_empty() {
            let url = feed
                .parse::<tauri::Url>()
                .map_err(|error| format!("{FEED_OVERRIDE} is not a URL: {error}."))?;
            builder = builder
                .endpoints(vec![url])
                .map_err(|error| error.to_string())?;
        }
    }
    let updater = builder.build().map_err(|error| error.to_string())?;
    let found = updater.check().await.map_err(|error| error.to_string())?;
    Ok(found.map(|update| update::Ready {
        version: update.version.clone(),
        notes: update.body.clone(),
        date: update.date.map(|date| date.to_string()),
    }))
}

/// One line in the launcher's own log, for the things that are deliberately
/// not said out loud. A log that could not be written is not worth a second
/// failure.
fn note(app: &tauri::AppHandle, line: &str) {
    use std::io::Write;
    let dir = app.state::<Launcher>().engine.config_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG))
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Look now, and again every four hours for as long as the launcher is open.
///
/// Started once, from setup. The schedule itself is `update::next_wait`, which
/// is tested; this is the loop that obeys it.
fn watch_for_updates(app: tauri::AppHandle) {
    // A plain thread that sleeps, rather than a timer on the async runtime.
    // There is no tokio here to ask for one, and a thread asleep for four hours
    // costs a stack and nothing else.
    std::thread::spawn(move || {
        let mut looks = 0u32;
        loop {
            let wait = update::next_wait(looks);
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
            looks = looks.saturating_add(1);
            match tauri::async_runtime::block_on(look_once(&app)) {
                Ok(Some(ready)) => {
                    let dismissed = app
                        .state::<Launcher>()
                        .settings
                        .lock()
                        .ok()
                        .and_then(|held| held.dismissed_update.clone());
                    if let Ok(mut held) = app.state::<Launcher>().update_ready.lock() {
                        *held = Some(ready.clone());
                    }
                    tray::refresh(&app);
                    if update::worth_showing(&ready.version, dismissed.as_deref()) {
                        let _ = app.emit(update::READY_EVENT, ready);
                    }
                }
                Ok(None) => {}
                Err(error) => note(&app, &format!("update check failed: {error}")),
            }
        }
    });
}

/// What the last look found, for a window that opened after it happened.
#[tauri::command]
fn update_pending(app: tauri::State<'_, Launcher>) -> Option<update::Ready> {
    let dismissed = app
        .settings
        .lock()
        .ok()
        .and_then(|held| held.dismissed_update.clone());
    let found = app.update_ready.lock().ok().and_then(|held| held.clone())?;
    update::worth_showing(&found.version, dismissed.as_deref()).then_some(found)
}

/// Not now. This version stays quiet; the next one asks again.
#[tauri::command]
fn update_dismiss(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let launcher = app.state::<Launcher>();
    let held = launcher
        .settings
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    let next = Settings {
        dismissed_update: Some(version.trim().to_string()),
        ..held
    };
    next.write(&launcher.engine.config_dir())?;
    if let Ok(mut current) = launcher.settings.lock() {
        *current = next;
    }
    tray::refresh(&app);
    Ok(())
}

/// Download the newer launcher, install it, and start it again.
///
/// This one is loud when it fails, because somebody pressed a button and is
/// waiting. A signature that does not verify arrives here as an error from the
/// updater and is shown in words rather than being swallowed: an update whose
/// signature is wrong is the one failure nobody should be able to click past.
#[tauri::command]
async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let mut builder = app.updater_builder();
    if let Ok(feed) = std::env::var(FEED_OVERRIDE) {
        let feed = feed.trim().to_string();
        if !feed.is_empty() {
            let url = feed
                .parse::<tauri::Url>()
                .map_err(|error| format!("{FEED_OVERRIDE} is not a URL: {error}."))?;
            builder = builder
                .endpoints(vec![url])
                .map_err(|error| error.to_string())?;
        }
    }
    let updater = builder.build().map_err(|error| error.to_string())?;
    let found = updater
        .check()
        .await
        .map_err(|error| format!("The farm could not be asked about updates: {error}"))?;
    let Some(update) = found else {
        return Err("This is already the newest launcher.".into());
    };

    let total = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let so_far = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let sending = app.clone();
    let counted = total.clone();
    let got = so_far.clone();
    update
        .download_and_install(
            move |chunk, length| {
                if let Some(length) = length {
                    counted.store(length, Ordering::Relaxed);
                }
                let done = got.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
                let whole = counted.load(Ordering::Relaxed);
                // A feed with no length is a bar that cannot be drawn, so the
                // page is told nothing rather than a made up fraction.
                if whole > 0 {
                    let fraction = (done as f64 / whole as f64).min(1.0);
                    let _ = sending.emit(update::PROGRESS_EVENT, fraction);
                }
            },
            || {},
        )
        .await
        .map_err(|error| format!("The update could not be installed: {error}"))?;

    // Everything the apps are doing is theirs; the launcher restarting does not
    // stop them, the same as closing the window does not.
    app.restart();
}

/// Look for a newer launcher now, because somebody asked.
///
/// The same look the four hour loop makes, with the same answer folded into the
/// same held state and the same event emitted, so a check from the About window
/// and a check that happened on its own leave the app in one condition. What
/// comes back is for the person who pressed the button: `None` means this is
/// the newest one, and anything else is already on its way to the banner.
#[tauri::command]
async fn update_look(app: tauri::AppHandle) -> Result<Option<update::Ready>, String> {
    let found = look_once(&app).await?;
    if let Ok(mut held) = app.state::<Launcher>().update_ready.lock() {
        *held = found.clone();
    }
    tray::refresh(&app);
    let Some(ready) = found else {
        return Ok(None);
    };
    let dismissed = app
        .state::<Launcher>()
        .settings
        .lock()
        .ok()
        .and_then(|held| held.dismissed_update.clone());
    // Somebody who went looking has un-dismissed that version by asking: the
    // banner is where the notes and the progress bar are, so the answer is put
    // back on screen rather than only said once in the About window.
    if update::worth_showing(&ready.version, dismissed.as_deref()) {
        let _ = app.emit(update::READY_EVENT, ready.clone());
    }
    Ok(Some(ready))
}

// --- The About window ----------------------------------------------------

/// What the About window draws: the version cargo built, the day the changelog
/// gives it, the licence, and the addresses it is allowed to open.
#[tauri::command]
fn about_info() -> about::About {
    about::about(env!("CARGO_PKG_VERSION"))
}

/// Open the About window, or bring the one that is already open forward.
///
/// One instance, the same rule an app's window follows. A second About is two
/// copies of the same unchanging page, and closing one of them would look like
/// closing both.
#[tauri::command]
fn about_open(app: tauri::AppHandle) -> Result<(), String> {
    about_window(&app).map_err(|error| format!("The About window would not open: {error}."))
}

pub fn about_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ABOUT) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, ABOUT, WebviewUrl::App("about.html".into()))
        .title("About Tiiny App Farm")
        .inner_size(ABOUT_SIZE.0, ABOUT_SIZE.1)
        .resizable(false)
        .maximizable(false)
        .center()
        .build()?;
    // The same thing the main window needs: a window that has never been placed
    // is given a first frame by something between the builder and the window
    // server, and saying the size again once it exists is what is taken. See
    // the note in setup.
    let _ = window.set_size(tauri::LogicalSize::new(ABOUT_SIZE.0, ABOUT_SIZE.1));
    let _ = window.center();
    Ok(())
}

/// The app's own menu bar.
///
/// It exists for one item. macOS builds an About item into every application
/// menu and wires it to a panel that says the name, the version and the
/// copyright line out of the bundle; this replaces that item with the launcher's
/// own window. A menu is set whole rather than edited, so everything Tauri's
/// default menu carried is written out here again.
///
/// Two of those are not decoration. **Edit** is how Cmd+V reaches a web view on
/// macOS, and the one place anybody types into this app is the masked field the
/// Tiiny's key is pasted into: no Edit menu, no paste, and no way in. **Window**
/// is how Cmd+W closes the About window.
fn menubar(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, Submenu};

    let about = MenuItemBuilder::with_id(ABOUT, "About Tiiny App Farm").build(app)?;

    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::with_items(
            app,
            "Tiiny App Farm",
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, None)?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, None)?,
            ],
        )?;
        let file = Submenu::with_items(app, "File", true, &[&PredefinedMenuItem::close_window(app, None)?])?;
        let view = Submenu::with_items(app, "View", true, &[&PredefinedMenuItem::fullscreen(app, None)?])?;
        let help = Submenu::with_items(app, "Help", true, &[])?;
        Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows and Linux have no application menu, so About lives where
        // every other desktop app on those two puts it.
        let file = Submenu::with_items(
            app,
            "File",
            true,
            &[
                &PredefinedMenuItem::close_window(app, None)?,
                &PredefinedMenuItem::quit(app, None)?,
            ],
        )?;
        let help = Submenu::with_items(app, "Help", true, &[&about])?;
        Menu::with_items(app, &[&file, &edit, &window, &help])
    }
}

/// Escape, from the page. The window closes itself in Rust rather than being
/// handed the permission to close windows, because that permission is not
/// needed for anything else this app does.
#[tauri::command]
fn about_close(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(ABOUT) {
        let _ = window.close();
    }
}

/// Show the launcher's own log in the file manager.
///
/// The log is the file `note` writes, and it only exists once something has
/// been worth writing down. A launcher that has had nothing to say says that,
/// rather than opening a folder and leaving somebody looking for a file that
/// was never made.
#[tauri::command]
fn open_log(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    let path = app.state::<Launcher>().engine.config_dir().join(LOG);
    if !path.is_file() {
        return Err(
            "The launcher has not had to write anything down yet, so there is no log."
                .to_string(),
        );
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|error| format!("The log is at {} and would not open: {error}.", path.display()))?;
    Ok(path.display().to_string())
}

/// How many seeds each app on the screen has been given, and the pile that
/// draws. Read from the farm rather than from the engine, and never waited on:
/// the cards are already up by the time this answers.
#[tauri::command]
async fn social_counts(ids: Vec<String>) -> BTreeMap<String, catalog::Social> {
    catalog::social_counts(&ids).await
}

#[tauri::command]
fn settings_read(app: tauri::State<'_, Launcher>) -> Settings {
    app.settings.lock().map(|s| s.clone()).unwrap_or_default()
}

#[tauri::command]
fn settings_write(app: tauri::AppHandle, next: Settings) -> Result<Settings, String> {
    let launcher = app.state::<Launcher>();
    // The seen map belongs to the launcher, not to the copy of the settings the
    // window happens to be holding. See Settings::from_window.
    let held = launcher
        .settings
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    let next = next.from_window(&held);
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

/// Everywhere the launcher will send somebody, and nowhere else.
///
/// An app running on this computer, the farm's own site, and the handful of
/// addresses the About window credits. Written as one function rather than
/// inside the command so that the rule can be tested without an app around it.
/// The About window's addresses are exact matches rather than prefixes; see
/// `about::ALLOWED`.
pub fn opens(url: &str) -> bool {
    url.starts_with("http://localhost:")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with(catalog::SITE)
        || about::allows(url)
}

#[tauri::command]
fn open_external(app: tauri::AppHandle, url: String) -> Result<(), String> {
    if !opens(&url) {
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
        // The window is back, so the watch comes back with it.
        let handle = app.clone();
        app.state::<Launcher>().watch.start(&handle);
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
        .menu(menubar)
        // The only item this app puts on the menu bar of its own, and the
        // reason the menu is built by hand at all.
        .on_menu_event(|app, event| {
            if event.id().0.as_str() == ABOUT {
                let _ = about_window(app);
            }
        })
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
            farm_models,
            farm_load_model,
            app_needs,
            models_watch,
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
            catalog_marks,
            card_seen,
            social_counts,
            update_pending,
            update_dismiss,
            update_install,
            update_look,
            about_info,
            about_open,
            about_close,
            open_log,
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
                watch: Watch::default(),
                snapshot: Mutex::new(None),
                snapshot_moved: AtomicU64::new(0),
                update_ready: Mutex::new(None),
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
            let watching = handle.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hide.hide();
                    // A window nobody is looking at has no reason to keep
                    // asking the Tiiny what it has loaded.
                    watching.state::<Launcher>().watch.stop();
                }
            });
            app.state::<Launcher>().watch.start(&handle);

            tray::build(&handle)?;
            // Last, so that the first look never delays the window appearing.
            watch_for_updates(handle.clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the launcher could not start")
        .run(|handle, event| {
            // Quitting takes the watch with it. Without this the interpreter
            // the watch runs in outlives the launcher and keeps asking the
            // Tiiny what it has loaded, for ever.
            if matches!(event, tauri::RunEvent::Exit) {
                handle.state::<Launcher>().watch.stop();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{about, app_id_in, opens};

    #[test]
    fn every_address_the_about_window_credits_is_one_the_launcher_will_open() {
        for url in about::ALLOWED {
            assert!(opens(url), "the About window offers {url} and the opener refuses it, which is a row that looks like a link and is not one");
        }
    }

    #[test]
    fn the_launcher_opens_an_app_on_this_computer_the_farm_and_nothing_else() {
        assert!(opens("http://localhost:8420/"));
        assert!(opens("http://127.0.0.1:8420/"));
        assert!(opens("https://tiinyapp.farm/apps/story-lantern/"));
        assert!(!opens("http://192.168.1.5:8420/"));
        assert!(!opens("https://example.com"));
        assert!(!opens("file:///etc/passwd"));
        assert!(!opens(""));
    }


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
