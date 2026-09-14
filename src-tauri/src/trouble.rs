//! A bad day, read into something the window can draw.
//!
//! The engine's failures are already sentences. What this adds is the one
//! thing a sentence cannot carry: what to offer the person next. A busy port
//! gets a port to try, an app that started and stopped gets its log, and a
//! checksum mismatch gets the reassurance that nothing on the computer
//! changed, because that is the question somebody actually has.
//!
//! It lives here rather than in the page so that the four cases the design
//! calls for are tested rather than looked at.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trouble {
    /// A short heading, so the panel says what kind of day this is.
    pub head: String,
    /// Anything worth adding to the engine's own sentence. Empty when the
    /// engine already said everything there is to say.
    pub also: String,
    /// A port to offer instead, when the engine said the app can be moved.
    pub offer_port: Option<u16>,
    /// Whether to offer the app's log, which is where a crash leaves its
    /// reason.
    pub offer_log: bool,
}

impl Default for Trouble {
    fn default() -> Self {
        Self {
            head: "That did not work".into(),
            also: String::new(),
            offer_port: None,
            offer_log: false,
        }
    }
}

/// Read one of the engine's failures.
pub fn classify(message: &str) -> Trouble {
    let lower = message.to_lowercase();

    if lower.contains("checksum mismatch") || lower.contains("size does not match") {
        return Trouble {
            head: "The download did not match the catalog".into(),
            also: "Nothing was unpacked and nothing on this computer changed. \
                   A download can arrive damaged, so it is worth trying once more."
                .into(),
            ..Trouble::default()
        };
    }

    if lower.contains("already in use") {
        // The engine moves a movable app off a busy port by itself. Reaching
        // this sentence at all means it could not, and it says which of the
        // two reasons applies in the same breath: an app that can be moved is
        // told to use --port, and one that cannot is told it cannot.
        let busy = busy_port(message);
        let movable = message.contains("--port");
        return Trouble {
            head: "That port is taken".into(),
            also: if movable {
                String::new()
            } else {
                "Something else on this computer is listening there, and this app \
                 declares one port and reads no setting for another. Stop whatever \
                 has the port, or ask its author to let the port be moved."
                    .into()
            },
            offer_port: if movable {
                busy.and_then(|p| p.checked_add(1))
            } else {
                None
            },
            offer_log: false,
        };
    }

    if lower.contains("timed out waiting for readiness")
        || lower.contains("exited")
        || lower.contains("farm.log")
    {
        return Trouble {
            head: "It started and then stopped".into(),
            also: "The last lines it wrote are below. They are the app's own words, \
                   not the farm's."
                .into(),
            offer_log: true,
            ..Trouble::default()
        };
    }

    if lower.contains("took longer than") {
        return Trouble {
            head: "That took too long".into(),
            also: "It was stopped rather than left running. An install stages \
                   everything before it touches what is already there, so nothing \
                   is half done."
                .into(),
            ..Trouble::default()
        };
    }

    if lower.contains("no device is configured") || lower.contains("no tiiny is on file") {
        return Trouble {
            head: "There is no Tiiny on file".into(),
            also: "Settings, Your Tiiny, and paste the key TiinyOS shows.".into(),
            ..Trouble::default()
        };
    }

    Trouble::default()
}

fn busy_port(message: &str) -> Option<u16> {
    let at = message.find("Port ")? + "Port ".len();
    let rest = &message[at..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every sentence below is one farm 0.1.11 raises, copied out of
    // farm/farm.py rather than written from memory.

    #[test]
    fn a_checksum_mismatch_says_nothing_was_installed() {
        let trouble = classify("Checksum mismatch; archive was not unpacked or run.");
        assert_eq!(trouble.head, "The download did not match the catalog");
        assert!(trouble.also.contains("nothing on this computer changed"));
        assert_eq!(trouble.offer_port, None);
        assert!(!trouble.offer_log);
    }

    #[test]
    fn a_size_that_does_not_match_reads_the_same_way() {
        let trouble = classify("Archive size does not match the manifest.");
        assert_eq!(trouble.head, "The download did not match the catalog");
    }

    #[test]
    fn a_busy_port_on_a_movable_app_offers_the_next_one() {
        let trouble =
            classify("Port 8420 is already in use; use farm start story-lantern --port N.");
        assert_eq!(trouble.head, "That port is taken");
        assert_eq!(trouble.offer_port, Some(8421));
        assert_eq!(trouble.also, "");
    }

    #[test]
    fn a_busy_port_on_an_app_that_cannot_move_offers_no_port() {
        let trouble =
            classify("Port 8500 is already in use, and tiiny-brain cannot be moved off it.");
        assert_eq!(trouble.head, "That port is taken");
        assert_eq!(trouble.offer_port, None);
        assert!(trouble.also.contains("declares one port"));
    }

    #[test]
    fn the_last_port_in_the_range_is_not_offered_as_the_next_one() {
        let trouble = classify("Port 65535 is already in use; use farm start x --port N.");
        assert_eq!(trouble.offer_port, None);
    }

    #[test]
    fn an_app_that_started_and_stopped_offers_its_log() {
        let trouble = classify(
            "story-lantern timed out waiting for readiness after 10 s. \
             The last lines of farm.log follow.",
        );
        assert_eq!(trouble.head, "It started and then stopped");
        assert!(trouble.offer_log);
    }

    #[test]
    fn a_command_that_ran_out_of_time_says_nothing_is_half_done() {
        let trouble = classify(
            "The farm took longer than 900 seconds and was stopped. Nothing was left half done.",
        );
        assert_eq!(trouble.head, "That took too long");
        assert!(trouble.also.contains("nothing"));
    }

    #[test]
    fn anything_the_launcher_does_not_recognise_keeps_the_engines_own_words() {
        let trouble = classify("This app has no release to install yet.");
        assert_eq!(trouble.head, "That did not work");
        assert_eq!(trouble.also, "");
        assert_eq!(trouble.offer_port, None);
        assert!(!trouble.offer_log);
    }

    #[test]
    fn a_port_is_read_out_of_the_sentence_and_not_guessed() {
        assert_eq!(
            busy_port("Port 8420 is already in use; use --port N."),
            Some(8420)
        );
        assert_eq!(busy_port("nothing about a port here"), None);
        assert_eq!(busy_port("Port zero is already in use"), None);
    }
}
