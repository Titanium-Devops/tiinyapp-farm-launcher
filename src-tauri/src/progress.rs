//! What an install is doing right now, read off the engine's own sentences.
//!
//! `farm install --json` prints one JSON object on stdout and puts everything a
//! person would have read on stderr, in order. Those lines are the only honest
//! progress there is: they come from the code that does the work, so they
//! cannot drift away from it. This module turns each line into a phase the
//! window can draw, and refuses to invent one for a line it does not know.

use serde::Serialize;

/// The three real steps of an install, plus the two ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Reading the app's entry in the catalog.
    Resolving,
    /// Pulling the archive down.
    Downloading,
    /// The archive is here and its checksum matched.
    Verified,
    /// Writing the files out.
    Unpacking,
    /// Installed, with nothing left to do.
    Ready,
}

impl Phase {
    /// The words the window shows for this step.
    pub fn sentence(self) -> &'static str {
        match self {
            Phase::Resolving => "Looking it up in the catalog",
            Phase::Downloading => "Downloading",
            Phase::Verified => "The download matches its checksum",
            Phase::Unpacking => "Unpacking",
            Phase::Ready => "Ready",
        }
    }

    /// How far along the bar sits when this step starts, nought to one.
    pub fn fraction(self) -> f32 {
        match self {
            Phase::Resolving => 0.05,
            Phase::Downloading => 0.15,
            Phase::Verified => 0.80,
            Phase::Unpacking => 0.85,
            Phase::Ready => 1.0,
        }
    }
}

/// A line the engine printed, and what it means.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub phase: Phase,
    /// The engine's own words, kept whole. When the line carries a detail the
    /// person wants (where it is downloading from, where it is unpacking to)
    /// it is in here and nowhere else.
    pub line: String,
}

/// Read one line of the engine's commentary.
///
/// Returns `None` for a line that is not a step: a blank line, the app's name
/// and pitch, the permissions list, the update notice. Those are printed too
/// and none of them move the bar.
pub fn read_line(line: &str) -> Option<Step> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let phase = if trimmed.starts_with("Looking up ") {
        Phase::Resolving
    } else if trimmed.starts_with("Downloading ") {
        Phase::Downloading
    } else if trimmed.starts_with("The download matches the checksum") {
        Phase::Verified
    } else if trimmed.starts_with("Unpacking ") {
        Phase::Unpacking
    } else if trimmed.starts_with("Ready.") || trimmed == "Ready" {
        Phase::Ready
    } else {
        return None;
    };
    Some(Step {
        phase,
        line: trimmed.to_string(),
    })
}

/// How many bytes the archive is expected to be, read out of a Downloading
/// line when the catalog knew the size. `Downloading it from ...` means nobody
/// has measured it, and nothing is guessed in that case.
pub fn expected_bytes(line: &str) -> Option<u64> {
    let rest = line.trim().strip_prefix("Downloading ")?;
    let mut words = rest.split_whitespace();
    let number: f64 = words.next()?.parse().ok()?;
    let unit = words.next()?.trim_end_matches(['.', ',']);
    let scale = match unit {
        "bytes" | "byte" => 1.0,
        "KB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        _ => return None,
    };
    Some((number * scale) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact sentences farm 0.1.11 prints during an install, in the order
    // it prints them. Taken from Farm.install in farm/farm.py; if the engine
    // ever changes its words these tests are what says so.
    const INSTALL_LINES: &[&str] = &[
        "Looking up story-lantern in the catalog.",
        "Story Lantern 0.1.2",
        "Write, illustrate and narrate bedtime stories on your Tiiny.",
        "By Jason Brashear.",
        "Needs: Python 3.9 or newer, port 8420, a chat, image and tts model.",
        "It asks for: files, network, device.",
        "Downloading 3.5 MB from github.com.",
        "The download matches the checksum the catalog lists.",
        "Unpacking it into /Users/somebody/tiinyapps/story-lantern/0.1.2.",
        "Ready. Run: farm start story-lantern",
    ];

    #[test]
    fn the_three_real_steps_and_both_ends_are_read_in_order() {
        let phases: Vec<Phase> = INSTALL_LINES
            .iter()
            .filter_map(|l| read_line(l))
            .map(|s| s.phase)
            .collect();
        assert_eq!(
            phases,
            vec![
                Phase::Resolving,
                Phase::Downloading,
                Phase::Verified,
                Phase::Unpacking,
                Phase::Ready
            ]
        );
    }

    #[test]
    fn the_lines_that_are_not_steps_move_nothing() {
        assert!(read_line("Story Lantern 0.1.2").is_none());
        assert!(read_line("It asks for: files, network, device.").is_none());
        assert!(read_line("").is_none());
        assert!(read_line("   ").is_none());
        assert!(read_line("farm 0.1.12 is out and you are on 0.1.11.").is_none());
    }

    #[test]
    fn a_step_keeps_the_engines_own_words() {
        let step = read_line("Downloading 3.5 MB from github.com.").unwrap();
        assert_eq!(step.line, "Downloading 3.5 MB from github.com.");
        assert_eq!(step.phase, Phase::Downloading);
    }

    #[test]
    fn the_bar_only_moves_forward() {
        let mut last = 0.0;
        for line in INSTALL_LINES {
            if let Some(step) = read_line(line) {
                assert!(step.phase.fraction() >= last, "{} went backwards", line);
                last = step.phase.fraction();
            }
        }
        assert_eq!(last, 1.0);
    }

    #[test]
    fn a_size_is_read_when_the_catalog_measured_one() {
        assert_eq!(
            expected_bytes("Downloading 3.5 MB from github.com."),
            Some(3_500_000)
        );
        assert_eq!(
            expected_bytes("Downloading 512 KB from github.com."),
            Some(512_000)
        );
        assert_eq!(
            expected_bytes("Downloading 900 bytes from github.com."),
            Some(900)
        );
    }

    #[test]
    fn an_unmeasured_size_is_not_guessed() {
        // describe_size answers "it" for a catalog entry carrying size 0.
        assert_eq!(expected_bytes("Downloading it from github.com."), None);
        assert_eq!(expected_bytes("Unpacking it into /tmp/x."), None);
    }

    #[test]
    fn an_update_prints_the_same_steps_plus_one_that_is_not_a_step() {
        let lines = [
            "Looking up story-lantern in the catalog.",
            "Downloading 3.5 MB from github.com.",
            "The download matches the checksum the catalog lists.",
            "Unpacking it into /Users/somebody/tiinyapps/story-lantern/0.1.3.",
            "Your data in /Users/somebody/tiinyapps/story-lantern/data is kept.",
            "Ready.",
        ];
        let phases: Vec<Phase> = lines
            .iter()
            .filter_map(|l| read_line(l))
            .map(|s| s.phase)
            .collect();
        assert_eq!(
            phases,
            vec![
                Phase::Resolving,
                Phase::Downloading,
                Phase::Verified,
                Phase::Unpacking,
                Phase::Ready
            ]
        );
    }
}
