//! A window per app, instead of a tab in somebody's browser.
//!
//! Open puts a running app in its own window, one per app, with the app's name
//! on it. Closing that window does not stop the app, and pressing Open again
//! brings the same window back rather than making a second one. The browser is
//! still there as a second way out, for anybody who wants their own extensions
//! and their own history.
//!
//! What lives here is the registry and the rules about where a window is
//! allowed to go. The window itself is built in `lib.rs`, where the Tauri
//! handle is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The label a Tauri window gets for one app.
///
/// Tauri labels are a flat namespace shared with `main`, and they may only hold
/// letters, numbers, `-`, `/`, `:` and `_`. App ids are already lowercase
/// letters, digits and hyphens, so the prefix is the whole of the work, and it
/// is what stops an app called `main` taking the launcher's own window.
pub fn label_for(id: &str) -> String {
    format!("app:{id}")
}

/// The app id inside a window label, for a label this module made.
pub fn id_in(label: &str) -> Option<&str> {
    label.strip_prefix("app:").filter(|rest| !rest.is_empty())
}

/// The app ids that have a window open, out of every window the launcher has.
///
/// The launcher's own window is in that list too and is not an app, which is
/// the whole reason labels are prefixed.
pub fn open_ids<'a>(labels: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut ids: Vec<String> = labels
        .into_iter()
        .filter_map(id_in)
        .map(str::to_string)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Whether this app already has a window, which is the difference between
/// making one and bringing one forward.
pub fn already_open<'a>(labels: impl IntoIterator<Item = &'a str>, id: &str) -> bool {
    let wanted = label_for(id);
    labels.into_iter().any(|label| label == wanted)
}

/// Where one app's window was last left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
}

/// What the launcher remembers about app windows between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Frames {
    pub frames: HashMap<String, Frame>,
}

impl Frames {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("windows.json")
    }

    pub fn read(config_dir: &Path) -> Frames {
        std::fs::read_to_string(Self::path(config_dir))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn write(&self, config_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(config_dir)
            .map_err(|error| format!("The window sizes could not be saved: {error}."))?;
        let body = serde_json::to_string_pretty(self)
            .map_err(|error| format!("The window sizes could not be written: {error}."))?;
        std::fs::write(Self::path(config_dir), body + "\n")
            .map_err(|error| format!("The window sizes could not be saved: {error}."))
    }

    /// A frame worth restoring. Nothing absurd, and nothing off every screen:
    /// a window remembered from a monitor that is no longer plugged in should
    /// come back where somebody can see it rather than not at all.
    pub fn sane(&self, id: &str) -> Option<Frame> {
        let frame = *self.frames.get(id)?;
        let sized = frame.width >= 320.0
            && frame.height >= 240.0
            && frame.width <= 20_000.0
            && frame.height <= 20_000.0;
        let placed =
            frame.x > -10_000.0 && frame.x < 20_000.0 && frame.y > -2_000.0 && frame.y < 20_000.0;
        (sized && placed).then_some(frame)
    }

    pub fn remember(&mut self, id: &str, frame: Frame) {
        self.frames.insert(id.to_string(), frame);
    }
}

/// The window an app opens with when nothing is remembered. Wider than tall,
/// because every app in the catalog is a page.
pub const FIRST_TIME: (f64, f64) = (1000.0, 760.0);

/// Whether a link the app followed belongs inside its window.
///
/// An app's window is that app, and nothing else. A link to another origin, or
/// to anything that is not the loopback address the app was started on, goes to
/// the system browser: an app window is not a browser, it has no address bar,
/// and somebody who ends up on a website inside one has no way to tell where
/// they are.
pub fn stays_inside(app_origin: &str, url: &str) -> bool {
    let Some(rest) = url.strip_prefix(app_origin) else {
        return false;
    };
    rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#')
}

/// The origin an app was started on, from the link the engine gave.
pub fn origin_of(url: &str) -> Option<String> {
    let after = url.strip_prefix("http://")?;
    let host_and_port = after.split(['/', '?', '#']).next()?;
    let (host, port) = host_and_port.rsplit_once(':')?;
    if !matches!(host, "localhost" | "127.0.0.1") || port.parse::<u16>().is_err() {
        return None;
    }
    Some(format!("http://{host_and_port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_the_app_id_and_cannot_collide_with_the_launchers_own_window() {
        assert_eq!(label_for("story-lantern"), "app:story-lantern");
        assert_eq!(id_in("app:story-lantern"), Some("story-lantern"));
        // An app called "main" gets "app:main" and leaves the launcher alone.
        assert_ne!(label_for("main"), "main");
        assert_eq!(id_in("main"), None);
        assert_eq!(id_in("app:"), None);
    }

    #[test]
    fn the_registry_knows_which_apps_have_a_window_and_leaves_the_launchers_out() {
        let labels = ["main", "app:story-lantern", "app:daybreak"];
        assert_eq!(open_ids(labels), vec!["daybreak", "story-lantern"]);
        // Pressing Open a second time finds the window rather than making one.
        assert!(already_open(labels, "story-lantern"));
        assert!(!already_open(labels, "onelane"));
        // And closing one leaves the other, and the launcher, alone.
        let after = ["main", "app:daybreak"];
        assert_eq!(open_ids(after), vec!["daybreak"]);
        assert!(!already_open(after, "story-lantern"));
    }

    #[test]
    fn a_remembered_frame_comes_back_and_a_silly_one_does_not() {
        let mut frames = Frames::default();
        frames.remember(
            "story-lantern",
            Frame {
                width: 900.0,
                height: 700.0,
                x: 100.0,
                y: 80.0,
            },
        );
        assert_eq!(frames.sane("story-lantern").unwrap().width, 900.0);

        frames.remember(
            "tiny",
            Frame {
                width: 4.0,
                height: 4.0,
                x: 0.0,
                y: 0.0,
            },
        );
        assert_eq!(frames.sane("tiny"), None);

        // A monitor that is no longer plugged in.
        frames.remember(
            "far",
            Frame {
                width: 900.0,
                height: 700.0,
                x: 40_000.0,
                y: 0.0,
            },
        );
        assert_eq!(frames.sane("far"), None);

        assert_eq!(frames.sane("never-opened"), None);
    }

    #[test]
    fn what_was_remembered_comes_back_off_disk() {
        let dir = std::env::temp_dir().join(format!("farm-windows-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut frames = Frames::default();
        frames.remember(
            "daybreak",
            Frame {
                width: 1200.0,
                height: 800.0,
                x: 20.0,
                y: 30.0,
            },
        );
        frames.write(&dir).unwrap();
        assert_eq!(Frames::read(&dir), frames);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_windows_file_that_is_not_readable_is_no_memory_rather_than_a_crash() {
        let dir = std::env::temp_dir().join(format!("farm-windows-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(Frames::path(&dir), "not json").unwrap();
        assert_eq!(Frames::read(&dir), Frames::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_app_own_pages_stay_in_its_window() {
        let origin = "http://localhost:8421";
        assert!(stays_inside(origin, "http://localhost:8421"));
        assert!(stays_inside(origin, "http://localhost:8421/"));
        assert!(stays_inside(origin, "http://localhost:8421/show"));
        assert!(stays_inside(
            origin,
            "http://localhost:8421/api/pages?page=2"
        ));
        assert!(stays_inside(origin, "http://localhost:8421/show#top"));
    }

    #[test]
    fn anything_else_goes_to_the_system_browser() {
        let origin = "http://localhost:8421";
        // Another app on this machine is not this app.
        assert!(!stays_inside(origin, "http://localhost:8422/"));
        // A prefix that is not a path boundary is not this app either.
        assert!(!stays_inside(origin, "http://localhost:84210/"));
        assert!(!stays_inside(origin, "https://tiinyapp.farm/"));
        assert!(!stays_inside(origin, "https://github.com/webdevtodayjason"));
        assert!(!stays_inside(origin, "http://172.17.7.177/v1"));
        assert!(!stays_inside(origin, "file:///etc/passwd"));
    }

    #[test]
    fn the_origin_is_read_off_the_link_the_engine_gave() {
        assert_eq!(
            origin_of("http://localhost:8421/show").as_deref(),
            Some("http://localhost:8421")
        );
        assert_eq!(
            origin_of("http://127.0.0.1:8811/").as_deref(),
            Some("http://127.0.0.1:8811")
        );
        assert_eq!(
            origin_of("http://localhost:8421").as_deref(),
            Some("http://localhost:8421")
        );
    }

    #[test]
    fn anything_that_is_not_a_loopback_port_gets_no_window() {
        // An app window only ever points at something on this machine.
        assert_eq!(origin_of("https://tiinyapp.farm/"), None);
        assert_eq!(origin_of("http://172.17.7.177/v1"), None);
        assert_eq!(origin_of("http://localhost/"), None);
        assert_eq!(origin_of("http://evil.example:8421/"), None);
        assert_eq!(origin_of("not a url"), None);
    }
}
