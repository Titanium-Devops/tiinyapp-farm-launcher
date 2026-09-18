//! The launcher keeping itself up to date.
//!
//! Everything else in this app updates: the engine is pinned inside it and the
//! apps are updated from the catalog. Until now the launcher itself was the one
//! thing that could not, which meant every fix reached only the people who
//! happened to visit the download page again.
//!
//! The rules live here so they can be tested without a window, a network or a
//! release: when to look, and whether what was found is worth putting on the
//! screen. The plumbing that actually asks Tauri's updater is in lib.rs.

use std::time::Duration;

use serde::Serialize;

/// How often a launcher that is left open looks again.
///
/// Four hours rather than four minutes because nothing here is urgent: the
/// worst case of a late look is that somebody sees the banner tomorrow. A
/// launcher is a thing people leave open for days, so this is also the only
/// look most copies ever make after the one at startup.
pub const EVERY: Duration = Duration::from_secs(4 * 60 * 60);

/// How long to wait before the look after this many have already happened.
///
/// The first is immediate. A launcher somebody has just opened is the best
/// moment to notice a new one, and a four hour wait before the first look would
/// mean a machine that is only ever used for an hour at a time never looks at
/// all. That case is the whole reason this function is written down and tested
/// rather than being a sleep at the top of a loop.
pub fn next_wait(looks: u32) -> Duration {
    if looks == 0 {
        Duration::ZERO
    } else {
        EVERY
    }
}

/// What the window is told when a newer launcher is waiting.
///
/// The field names are what the page reads, so a rename here is a banner that
/// stops appearing. A test holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready {
    pub version: String,
    /// What the release said about itself, when the feed carried any.
    pub notes: Option<String>,
    /// When it was published, as the feed spelled it.
    pub date: Option<String>,
}

/// The event the window listens for. One name, in one place, because a typo in
/// either half is a banner nobody ever sees and nothing that says why.
pub const READY_EVENT: &str = "update:ready";
/// How far the download has got, 0 to 1.
pub const PROGRESS_EVENT: &str = "update:progress";

/// Whether a found version is worth putting a banner up for.
///
/// Somebody who said Not now to a version has said it about that version, not
/// about the idea of updating: the next one asks again. Anything without a
/// version number is not something to interrupt anybody about.
pub fn worth_showing(found: &str, dismissed: Option<&str>) -> bool {
    let found = found.trim();
    if found.is_empty() {
        return false;
    }
    dismissed.map(str::trim) != Some(found)
}

/// Whether an explicit look should forget that somebody put this version away.
///
/// Not now is an answer about a version, and pressing Check for updates is a
/// different answer about the same one. Without this, the About window would
/// find a version, say the farm window has the button that installs it, and
/// send somebody to a banner that `worth_showing` is still keeping quiet: a
/// hand off to a thing that is not there.
pub fn asking_undismisses(found: &str, dismissed: Option<&str>) -> bool {
    let found = found.trim();
    !found.is_empty() && dismissed.map(str::trim) == Some(found)
}

/// The line the menu bar carries while an update is waiting.
pub fn tray_line(version: &str) -> String {
    format!("Tiiny App Farm {version} is ready")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_look_is_at_once_and_the_rest_are_four_hours_apart() {
        assert_eq!(
            next_wait(0),
            Duration::ZERO,
            "a launcher just opened looks now"
        );
        assert_eq!(next_wait(1), EVERY);
        assert_eq!(next_wait(2), EVERY);
        assert_eq!(next_wait(1_000), EVERY);
        assert_eq!(EVERY.as_secs(), 4 * 60 * 60);
    }

    #[test]
    fn a_version_somebody_put_away_stays_away_and_the_next_one_does_not() {
        assert!(
            worth_showing("0.1.3", None),
            "nothing dismissed, so it shows"
        );
        assert!(
            !worth_showing("0.1.3", Some("0.1.3")),
            "that one was put away"
        );
        assert!(
            worth_showing("0.1.4", Some("0.1.3")),
            "this is a different one"
        );
        // Whitespace either side is the same answer, because this is read out
        // of a file somebody could have edited.
        assert!(!worth_showing(" 0.1.3 ", Some("0.1.3")));
        assert!(!worth_showing("0.1.3", Some(" 0.1.3 ")));
    }

    #[test]
    fn going_looking_takes_back_a_not_now_about_that_same_version() {
        assert!(
            asking_undismisses("0.2.0", Some("0.2.0")),
            "this is the version somebody put away and has now gone looking for"
        );
        assert!(asking_undismisses(" 0.2.0 ", Some("0.2.0")));
        // A different version was never put away, so there is nothing to take
        // back, and a launcher with no answer on file has nothing either.
        assert!(!asking_undismisses("0.2.1", Some("0.2.0")));
        assert!(!asking_undismisses("0.2.0", None));
        assert!(!asking_undismisses("", Some("")));
        assert!(!asking_undismisses("   ", None));
        // The two rules agree: once the answer is taken back, the banner shows.
        assert!(!worth_showing("0.2.0", Some("0.2.0")));
        assert!(worth_showing("0.2.0", None));
    }

    #[test]
    fn nothing_without_a_version_ever_interrupts_anybody() {
        assert!(!worth_showing("", None));
        assert!(!worth_showing("   ", Some("0.1.3")));
    }

    #[test]
    fn the_window_is_handed_the_three_names_it_reads() {
        let ready = Ready {
            version: "0.1.3".into(),
            notes: Some("Tiiny App Farm 0.1.3".into()),
            date: Some("2026-09-18T14:00:00Z".into()),
        };
        let value = serde_json::to_value(&ready).unwrap();
        assert_eq!(value["version"], "0.1.3");
        assert_eq!(value["notes"], "Tiiny App Farm 0.1.3");
        assert_eq!(value["date"], "2026-09-18T14:00:00Z");
        // A feed that carries neither is still a banner, so both are allowed to
        // be missing rather than being made up.
        let bare = Ready {
            version: "0.1.3".into(),
            notes: None,
            date: None,
        };
        let value = serde_json::to_value(&bare).unwrap();
        assert!(value["notes"].is_null());
        assert!(value["date"].is_null());
    }

    #[test]
    fn the_words_name_the_version_and_nothing_else() {
        // The banner's own sentence belongs to the page and is held by
        // scripts/check-ui.mjs. This is the menu bar's, which is the only one
        // Rust writes.
        assert_eq!(tray_line("0.1.3"), "Tiiny App Farm 0.1.3 is ready");
        // The event names are read by the page. A rename is a silent failure,
        // so they are pinned here as well as declared once.
        assert_eq!(READY_EVENT, "update:ready");
        assert_eq!(PROGRESS_EVENT, "update:progress");
    }
}
