//! The menu bar on a Mac, the tray on Windows.
//!
//! What is running, and the three things somebody wants to do to a running app
//! without going back to the window. Closing the window leaves this behind,
//! which is the whole reason it exists.

use serde_json::Value;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::engine::Budget;
use crate::Launcher;

const ID: &str = "farm-tray";

/// A row in the tray, read off `farm status --json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub url: Option<String>,
    pub update_available: Option<String>,
    pub health: Option<String>,
}

/// Turn the engine's status answer into tray rows. Anything the answer does
/// not carry is left out rather than guessed at.
pub fn rows_from(status: &Value) -> Vec<Row> {
    status
        .get("running")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some(Row {
                        id: row.get("id")?.as_str()?.to_string(),
                        url: row.get("url").and_then(Value::as_str).map(str::to_string),
                        update_available: row
                            .get("updateAvailable")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        health: row
                            .get("health")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The words beside an app's name in the menu.
pub fn label_for(row: &Row) -> String {
    let mut label = row.id.clone();
    if let Some(port) = row.url.as_deref().and_then(port_in) {
        label.push_str(&format!("  port {port}"));
    }
    match row.health.as_deref() {
        Some("unavailable") => label.push_str("  not answering"),
        Some("unversioned") => {}
        _ => {}
    }
    if row.update_available.is_some() {
        label.push_str("  update ready");
    }
    label
}

fn port_in(url: &str) -> Option<u16> {
    url.rsplit(':').next()?.split('/').next()?.parse().ok()
}

fn menu<R: Runtime>(app: &AppHandle<R>, rows: &[Row]) -> tauri::Result<Menu<R>> {
    let mut builder = MenuBuilder::new(app);
    if rows.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("nothing", "Nothing is running")
                .enabled(false)
                .build(app)?,
        );
    }
    for row in rows {
        let mut sub = SubmenuBuilder::new(app, label_for(row));
        sub = sub.item(
            &MenuItemBuilder::with_id(format!("open:{}", row.id), "Open")
                .enabled(row.url.is_some())
                .build(app)?,
        );
        sub = sub.item(&MenuItemBuilder::with_id(format!("stop:{}", row.id), "Stop").build(app)?);
        sub = sub.item(
            &MenuItemBuilder::with_id(
                format!("update:{}", row.id),
                match &row.update_available {
                    Some(version) => format!("Update to {version}"),
                    None => "Update".to_string(),
                },
            )
            .enabled(row.update_available.is_some())
            .build(app)?,
        );
        builder = builder.item(&sub.build()?);
    }
    builder = builder.item(&PredefinedMenuItem::separator(app)?);
    builder = builder.item(&MenuItemBuilder::with_id("show", "Show the farm").build(app)?);
    builder = builder.item(&PredefinedMenuItem::separator(app)?);
    builder = builder.item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?);
    builder.build()
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let icon = tray_icon(app);
    let tray = TrayIconBuilder::with_id(ID)
        .tooltip("Tiiny App Farm")
        .icon_as_template(true)
        .menu(&menu(app, &[])?)
        .on_menu_event(|app, event| on_menu(app, event.id().0.as_str()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                crate::raise(tray.app_handle());
            }
        });
    let tray = match icon {
        Some(icon) => tray.icon(icon),
        None => tray,
    };
    tray.build(app)?;
    refresh(app);
    Ok(())
}

fn tray_icon(app: &AppHandle) -> Option<tauri::image::Image<'static>> {
    let path = app.path().resource_dir().ok()?.join("icons/tray/44x44.png");
    tauri::image::Image::from_path(path).ok()
}

/// Read the engine's status and redraw the menu. Called after anything that
/// could have changed what is running.
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = handle.state::<Launcher>().engine.clone();
        let answer =
            tauri::async_runtime::spawn_blocking(move || engine.json(&["status"], Budget::QUICK))
                .await;
        let rows = match answer {
            Ok(Ok(value)) => rows_from(&value),
            _ => Vec::new(),
        };
        if let (Some(tray), Ok(menu)) = (handle.tray_by_id(ID), menu(&handle, &rows)) {
            let _ = TrayIcon::set_menu(&tray, Some(menu));
        }
    });
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => crate::raise(app),
        "quit" => app.exit(0),
        other => {
            let Some((verb, ident)) = other.split_once(':') else {
                return;
            };
            let ident = ident.to_string();
            let verb = verb.to_string();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let engine = handle.state::<Launcher>().engine.clone();
                let ident_for_open = ident.clone();
                let verb_for_work = verb.clone();
                let done =
                    tauri::async_runtime::spawn_blocking(move || match verb_for_work.as_str() {
                        "stop" => engine.json(&["stop", &ident], Budget::PATIENT),
                        "update" => engine.json(&["update", &ident, "--yes"], Budget::DOWNLOAD),
                        _ => engine.json(&["status"], Budget::QUICK),
                    })
                    .await;
                if verb == "open" {
                    if let Ok(Ok(value)) = &done {
                        if let Some(url) = rows_from(value)
                            .into_iter()
                            .find(|row| row.id == ident_for_open)
                            .and_then(|row| row.url)
                        {
                            use tauri_plugin_opener::OpenerExt;
                            let _ = handle.opener().open_url(url, None::<&str>);
                        }
                    }
                }
                use tauri::Emitter;
                let _ = handle.emit("apps:changed", ());
                refresh(&handle);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status() -> Value {
        json!({
            "command": "status",
            "running": [
                {"id": "story-lantern", "pid": 4242, "ports": [8420], "port": 8420,
                 "url": "http://localhost:8420/", "uptime": 91, "version": "0.1.2",
                 "installed": "0.1.2", "restartToUpdate": false, "health": "ok",
                 "updateAvailable": null},
                {"id": "tiiny-bench", "pid": 4243, "ports": [8422], "port": 8422,
                 "url": "http://localhost:8422", "uptime": 12, "version": "0.1.0",
                 "installed": "0.1.0", "restartToUpdate": false, "health": "unavailable",
                 "updateAvailable": "0.2.0"}
            ]
        })
    }

    #[test]
    fn the_tray_lists_what_is_running() {
        let rows = rows_from(&status());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "story-lantern");
        assert_eq!(rows[1].update_available.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn nothing_running_is_no_rows_rather_than_an_error() {
        assert!(rows_from(&json!({"command": "status", "running": []})).is_empty());
        assert!(rows_from(&json!({"command": "status"})).is_empty());
        assert!(rows_from(&json!(null)).is_empty());
    }

    #[test]
    fn a_row_says_the_port_and_anything_wrong_with_it() {
        let rows = rows_from(&status());
        assert_eq!(label_for(&rows[0]), "story-lantern  port 8420");
        assert_eq!(
            label_for(&rows[1]),
            "tiiny-bench  port 8422  not answering  update ready"
        );
    }

    #[test]
    fn a_row_with_no_url_still_draws() {
        let row = Row {
            id: "onelane".into(),
            url: None,
            update_available: None,
            health: None,
        };
        assert_eq!(label_for(&row), "onelane");
    }

    #[test]
    fn the_port_is_read_off_the_link_the_engine_gave() {
        assert_eq!(port_in("http://localhost:8420/"), Some(8420));
        assert_eq!(port_in("http://localhost:8421"), Some(8421));
        assert_eq!(port_in("http://localhost:8420/read"), Some(8420));
        assert_eq!(port_in("not a url"), None);
    }
}
