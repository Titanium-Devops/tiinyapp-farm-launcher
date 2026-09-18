//! The launcher's own settings. Small, and none of them are secrets: the
//! device key lives in the engine's device file at mode 0600 and never here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Start with the computer. Off, because nobody asked for another thing
    /// that starts with the computer.
    pub autostart: bool,
    /// Put `farm` on the shell's PATH. Off, because a consumer did not ask for
    /// a command they have never heard of.
    pub farm_on_path: bool,
    /// The address of the Tiiny somebody typed in by hand, so the next run
    /// does not ask again.
    pub manual_base: Option<String>,
    /// Open an app in the system browser rather than in its own window. Off,
    /// because a window with the app's name on it is what somebody expects of
    /// an app, and taking a tab from whatever they were doing is not. Anybody
    /// who would rather have their own extensions and their own history turns
    /// this on, and Open in browser is on every row either way.
    pub open_in_browser: bool,
    /// Every app id this launcher has already put on a screen, against the
    /// version it was first shown at. `None` means it has never drawn the farm
    /// at all, which is a different thing from having drawn it and found
    /// nothing: the first screen marks everything seen without a word, so that
    /// nobody's first look at the farm is a wall of New badges.
    pub seen: Option<BTreeMap<String, String>>,
}

impl Settings {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("launcher.json")
    }

    pub fn read(config_dir: &Path) -> Settings {
        std::fs::read_to_string(Self::path(config_dir))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn write(&self, config_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(config_dir)
            .map_err(|error| format!("The settings folder could not be made: {error}."))?;
        let body = serde_json::to_string_pretty(self)
            .map_err(|error| format!("The settings could not be written: {error}."))?;
        std::fs::write(Self::path(config_dir), body + "\n")
            .map_err(|error| format!("The settings could not be saved: {error}."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_on_until_somebody_turns_it_on() {
        let settings = Settings::default();
        assert!(!settings.autostart);
        assert!(!settings.farm_on_path);
        assert!(!settings.open_in_browser);
        assert_eq!(settings.manual_base, None);
        assert_eq!(settings.seen, None);
    }

    #[test]
    fn a_settings_file_that_is_not_readable_is_the_defaults_rather_than_a_crash() {
        let dir =
            std::env::temp_dir().join(format!("farm-launcher-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(Settings::path(&dir), "this is not json").unwrap();
        assert_eq!(Settings::read(&dir), Settings::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn what_was_saved_is_what_comes_back() {
        let dir =
            std::env::temp_dir().join(format!("farm-launcher-roundtrip-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let settings = Settings {
            autostart: true,
            farm_on_path: false,
            manual_base: Some("http://172.17.7.177/v1".into()),
            open_in_browser: true,
            seen: Some(BTreeMap::from([("story-lantern".into(), "0.1.3".into())])),
        };
        settings.write(&dir).unwrap();
        assert_eq!(Settings::read(&dir), settings);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_settings_file_written_before_a_field_existed_still_reads() {
        let dir =
            std::env::temp_dir().join(format!("farm-launcher-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(Settings::path(&dir), r#"{"autostart":true}"#).unwrap();
        let settings = Settings::read(&dir);
        assert!(settings.autostart);
        assert!(!settings.farm_on_path);
        // A launcher that has been run before this field existed has still
        // shown somebody the farm, so nothing on it is New.
        assert_eq!(settings.seen, None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
