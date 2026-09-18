//! The About window: who made this, what it is built for, and where it lives.
//!
//! The stock panel macOS puts up says the name, the version and the copyright
//! line out of the bundle, and nothing else. That is three facts about a thing
//! four different people and projects had a hand in, and none of them are
//! reachable from it. This window is the credit roll: every party named, with
//! its own mark, and a link that opens in the person's own browser.
//!
//! Two rules live here rather than in the page, because the page is drawing and
//! these are decisions:
//!
//! - **The allow list.** A window full of links is a window full of ways out of
//!   the app. Every one of them is written down here, every one is `https`, and
//!   anything not on the list is refused by name rather than opened. The page
//!   cannot add a link the launcher will follow.
//! - **The version and the date are read, never typed.** The version is the one
//!   cargo compiled, and the date is the one the changelog gives that version.
//!   A release whose About window and download page disagree about what it is
//!   would be a small lie nobody would catch.

use serde::Serialize;

/// The changelog, compiled in, so the release date comes from the same file the
/// download page is built from. `include_str!` also makes the changelog a build
/// dependency: editing it rebuilds this.
pub const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

/// The licence this launcher is published under. Held beside the file it names
/// by `the_licence_named_here_is_the_one_in_the_repository`.
pub const LICENSE: &str = "MIT";

/// Every address the About window is allowed to send somebody to.
///
/// Exact matches, in full, with the scheme. Not prefixes: a prefix list is a
/// list somebody can walk off the end of, and there are eight links here, not
/// eight thousand. `scripts/check-ui.mjs` reads this array out of this file and
/// fails if the page carries a link that is not in it, or if this carries one
/// the page does not use.
pub const ALLOWED: &[&str] = &[
    // Made by
    "https://titaniumcomputing.com",
    "https://github.com/webdevtodayjason",
    "https://jasonbrashear.com",
    // Built for
    "https://tiiny.ai",
    // Part of
    "https://tiinyapp.farm",
    "https://titanium.bot",
    // Source
    "https://github.com/Titanium-Devops/tiinyapp-farm-launcher",
    "https://github.com/Titanium-Devops/tiinyapp-farm-launcher/blob/main/LICENSE",
];

/// Whether the About window may open this address.
pub fn allows(url: &str) -> bool {
    ALLOWED.contains(&url)
}

/// What the About window is told about the app it is describing.
///
/// Everything here is read: nothing in this struct is a string somebody typed
/// into the window and has to remember to change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct About {
    /// The version cargo built, which is the version in the three files
    /// `scripts/check-release-notes.mjs` holds together.
    pub version: String,
    /// The day the changelog gives this version, in words. Missing rather than
    /// guessed at when the changelog has no date for it.
    pub released: Option<String>,
    pub license: String,
    /// The allow list, so the page can be tested against the same array the
    /// launcher enforces rather than against a copy of it.
    pub links: Vec<String>,
}

/// Read the day a version was published out of the changelog.
///
/// The headings are `## 0.1.4 - 2026-09-18`, which is the shape
/// `scripts/check-release-notes.mjs` already insists on, so this is reading a
/// field rather than hoping about a format.
pub fn released(changelog: &str, version: &str) -> Option<String> {
    for line in changelog.lines() {
        let Some(rest) = line.strip_prefix("## ") else {
            continue;
        };
        let Some((found, date)) = rest.split_once(" - ") else {
            continue;
        };
        if found.trim() != version {
            continue;
        }
        let date = date.trim();
        if date.is_empty() {
            return None;
        }
        return Some(in_words(date));
    }
    None
}

/// `2026-09-18` becomes `September 18, 2026`, and anything that is not a date
/// in that shape is handed back exactly as it was found.
///
/// A date nobody can read at a glance is a fact nobody reads at all, and the
/// alternative to this is a crate whose whole job is formatting one line.
pub fn in_words(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = date.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return date.to_string();
    };
    let (Ok(month), Ok(day)) = (month.parse::<usize>(), day.parse::<u32>()) else {
        return date.to_string();
    };
    if year.len() != 4 || year.parse::<u32>().is_err() {
        return date.to_string();
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return date.to_string();
    }
    format!("{} {day}, {year}", MONTHS[month - 1])
}

/// Everything the window draws about the app, gathered in one place.
pub fn about(version: &str) -> About {
    About {
        version: version.to_string(),
        released: released(CHANGELOG, version),
        license: LICENSE.to_string(),
        links: ALLOWED.iter().map(|url| (*url).to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_address_the_window_can_open_is_https_and_written_down_once() {
        for url in ALLOWED {
            assert!(
                url.starts_with("https://"),
                "{url} is not https, and nothing in this window is worth a plaintext hop"
            );
            assert!(!url.ends_with('/'), "{url} has a trailing slash; the page's link and this list are compared exactly, so they have to be spelled the same way");
            assert_eq!(
                ALLOWED.iter().filter(|other| *other == url).count(),
                1,
                "{url} is in the list twice"
            );
        }
        assert_eq!(ALLOWED.len(), 8, "a link added or taken away is a change to what this window says about who made the launcher, so it is said out loud here");
    }

    #[test]
    fn anything_not_on_the_list_is_refused_however_close_it_looks() {
        assert!(allows("https://tiiny.ai"));
        assert!(allows(
            "https://github.com/Titanium-Devops/tiinyapp-farm-launcher"
        ));
        // A prefix of something allowed, and something allowed with anything
        // added, are both somewhere else.
        assert!(!allows("https://tiiny.ai/pricing"));
        assert!(!allows("https://github.com"));
        assert!(!allows("https://tiiny.ai.example.com"));
        // The same address without the lock on it is a different address.
        assert!(!allows("http://tiiny.ai"));
        assert!(!allows("HTTPS://TIINY.AI"));
        assert!(!allows(""));
        assert!(!allows("javascript:alert(1)"));
        assert!(!allows("file:///etc/passwd"));
    }

    #[test]
    fn the_release_date_comes_out_of_the_changelog() {
        let changelog = "# Changelog\n\n## 0.1.4 - 2026-09-18\n\n### Added\n\n- A thing.\n\n## 0.1.3 - 2026-09-17\n\n- Another.\n";
        assert_eq!(
            released(changelog, "0.1.4").as_deref(),
            Some("September 18, 2026")
        );
        assert_eq!(
            released(changelog, "0.1.3").as_deref(),
            Some("September 17, 2026")
        );
        // A version the changelog has never heard of has no date, rather than
        // borrowing the newest one it can find.
        assert_eq!(released(changelog, "0.9.9"), None);
        assert_eq!(released("", "0.1.4"), None);
        assert_eq!(released("## 0.1.4\n", "0.1.4"), None);
        assert_eq!(released("## 0.1.4 - \n", "0.1.4"), None);
    }

    #[test]
    fn this_launchers_own_version_has_a_date_in_the_real_changelog() {
        // The changelog compiled into this build is the one the download page
        // is made from, so a release with no date here is a release whose About
        // window would be missing a line.
        let version = env!("CARGO_PKG_VERSION");
        assert!(
            released(CHANGELOG, version).is_some(),
            "CHANGELOG.md has no dated entry for {version}"
        );
    }

    #[test]
    fn a_date_is_put_in_words_and_anything_that_is_not_a_date_is_left_alone() {
        assert_eq!(in_words("2026-09-18"), "September 18, 2026");
        assert_eq!(in_words("2026-01-01"), "January 1, 2026");
        assert_eq!(in_words("2026-12-31"), "December 31, 2026");
        // Leading zeros go, because nobody says "September 08".
        assert_eq!(in_words("2026-09-08"), "September 8, 2026");
        // Anything else is handed back rather than mangled or dropped.
        assert_eq!(in_words("soon"), "soon");
        assert_eq!(in_words("2026-13-01"), "2026-13-01");
        assert_eq!(in_words("2026-00-01"), "2026-00-01");
        assert_eq!(in_words("2026-09-32"), "2026-09-32");
        assert_eq!(in_words("26-09-18"), "26-09-18");
        assert_eq!(in_words("2026-09"), "2026-09");
        assert_eq!(in_words("2026-09-18-01"), "2026-09-18-01");
        assert_eq!(in_words(""), "");
    }

    #[test]
    fn the_licence_named_here_is_the_one_in_the_repository() {
        let text = include_str!("../../LICENSE");
        assert!(
            text.starts_with("MIT License"),
            "the About window says {LICENSE} and LICENSE says something else"
        );
        assert_eq!(LICENSE, "MIT");
    }

    #[test]
    fn the_window_is_handed_the_version_it_was_built_as() {
        let drawn = about("0.1.4");
        assert_eq!(drawn.version, "0.1.4");
        assert_eq!(drawn.license, "MIT");
        assert_eq!(drawn.links.len(), ALLOWED.len());
        assert_eq!(drawn.links[0], "https://titaniumcomputing.com");
        // Built from the real one, the version is the one cargo compiled and
        // never a string typed into a page.
        let real = about(env!("CARGO_PKG_VERSION"));
        assert_eq!(real.version, env!("CARGO_PKG_VERSION"));
        assert!(real.released.is_some());
    }
}
