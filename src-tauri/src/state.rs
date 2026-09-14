//! Where one app is in its life, and the only ways it can move.
//!
//! The window draws this and nothing else, so every button it offers is a
//! button that can be pressed. Keeping the rules here, away from the engine
//! and away from the page, is what makes them testable: the interesting cases
//! are the bad days, and a bad day is hard to arrange on purpose.

use serde::Serialize;

use crate::progress::Phase;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AppState {
    /// In the catalog, not on this machine.
    Absent,
    /// Being installed or updated right now.
    #[serde(rename_all = "camelCase")]
    Working {
        phase: Phase,
        line: String,
        fraction: f32,
    },
    /// Installed and not running.
    Stopped,
    /// Started, waiting to answer.
    Starting,
    /// Running, with the port it really took.
    #[serde(rename_all = "camelCase")]
    Running {
        port: Option<u16>,
        url: Option<String>,
        ready: bool,
    },
    /// Installed, and a library, so there is nothing to start.
    Library,
    /// The last thing that was asked of it did not work.
    #[serde(rename_all = "camelCase")]
    Failed { message: String, was: Box<AppState> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// An install or an update was asked for.
    WorkBegan,
    /// The engine printed a line that moves the bar.
    Stepped(Phase, String),
    /// The install finished and the app is a runnable app.
    Installed,
    /// The install finished and the app is a library.
    InstalledLibrary,
    /// Start was asked for.
    StartAsked,
    /// The engine says it is up.
    Started {
        port: Option<u16>,
        url: Option<String>,
        ready: bool,
    },
    /// Stop was asked for, and the engine says it is down.
    Stopped,
    /// It was removed from this machine.
    Removed,
    /// Anything the engine refused, in its own words.
    Refused(String),
    /// The person read the failure and put it away.
    Dismissed,
}

impl AppState {
    /// The one place a state changes.
    pub fn next(&self, event: Event) -> AppState {
        match (self, event) {
            // A failure is a hat worn over the state underneath, so dismissing
            // it puts the app back where it really is rather than guessing.
            (AppState::Failed { was, .. }, Event::Dismissed) => (**was).clone(),
            (_, Event::Refused(message)) => AppState::Failed {
                message,
                // A failure during work drops back to what it was before the
                // work: an install that failed installed nothing, because the
                // engine stages everything before it touches the app directory.
                was: Box::new(match self {
                    AppState::Working { .. } => AppState::Absent,
                    // A start that failed did not start anything, and Starting
                    // is a state nothing else would ever move it out of.
                    AppState::Starting => AppState::Stopped,
                    AppState::Failed { was, .. } => (**was).clone(),
                    other => other.clone(),
                }),
            },
            (_, Event::WorkBegan) => AppState::Working {
                phase: Phase::Resolving,
                line: Phase::Resolving.sentence().to_string(),
                fraction: Phase::Resolving.fraction(),
            },
            (
                AppState::Working {
                    phase,
                    line,
                    fraction,
                },
                Event::Stepped(next, said),
            ) => {
                // Lines arrive in order, but a slow reader and a fast engine can
                // still deliver one late. The bar never goes backwards.
                if next.fraction() < *fraction {
                    AppState::Working {
                        phase: *phase,
                        line: line.clone(),
                        fraction: *fraction,
                    }
                } else {
                    AppState::Working {
                        phase: next,
                        line: said,
                        fraction: next.fraction(),
                    }
                }
            }
            (_, Event::Stepped(..)) => self.clone(),
            (_, Event::Installed) => AppState::Stopped,
            (_, Event::InstalledLibrary) => AppState::Library,
            (_, Event::StartAsked) => AppState::Starting,
            (_, Event::Started { port, url, ready }) => AppState::Running { port, url, ready },
            (_, Event::Stopped) => AppState::Stopped,
            (_, Event::Removed) => AppState::Absent,
            (_, Event::Dismissed) => self.clone(),
        }
    }

    /// Whether the window should offer an Open button.
    pub fn can_open(&self) -> bool {
        matches!(self, AppState::Running { url: Some(_), .. })
    }

    /// Whether the window should offer a Stop button.
    pub fn can_stop(&self) -> bool {
        matches!(self, AppState::Running { .. } | AppState::Starting)
    }

    /// Whether anything is in flight, which is when every button is quiet.
    pub fn busy(&self) -> bool {
        matches!(self, AppState::Working { .. } | AppState::Starting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(start: AppState, events: &[Event]) -> AppState {
        events
            .iter()
            .fold(start, |state, event| state.next(event.clone()))
    }

    #[test]
    fn a_plain_install_walks_from_absent_to_stopped() {
        let end = run(
            AppState::Absent,
            &[
                Event::WorkBegan,
                Event::Stepped(
                    Phase::Downloading,
                    "Downloading 3.5 MB from github.com.".into(),
                ),
                Event::Stepped(
                    Phase::Verified,
                    "The download matches the checksum the catalog lists.".into(),
                ),
                Event::Stepped(
                    Phase::Unpacking,
                    "Unpacking it into /home/x/tiinyapps/story-lantern/0.1.2.".into(),
                ),
                Event::Installed,
            ],
        );
        assert_eq!(end, AppState::Stopped);
    }

    #[test]
    fn a_checksum_mismatch_installs_nothing() {
        let end = run(
            AppState::Absent,
            &[
                Event::WorkBegan,
                Event::Stepped(
                    Phase::Downloading,
                    "Downloading 3.5 MB from github.com.".into(),
                ),
                Event::Refused("Checksum mismatch; archive was not unpacked or run.".into()),
            ],
        );
        match &end {
            AppState::Failed { message, was } => {
                assert!(message.starts_with("Checksum mismatch"));
                // Not installed. The bad day must not leave a half app behind.
                assert_eq!(**was, AppState::Absent);
            }
            other => panic!("expected a failure, got {other:?}"),
        }
        assert_eq!(end.next(Event::Dismissed), AppState::Absent);
    }

    #[test]
    fn a_failed_start_leaves_the_app_installed() {
        let end = run(
            AppState::Stopped,
            &[
                Event::StartAsked,
                Event::Refused("Port 8420 is already in use.".into()),
            ],
        );
        assert_eq!(end.next(Event::Dismissed), AppState::Stopped);
    }

    #[test]
    fn a_running_app_stays_running_when_the_window_is_put_away() {
        // Closing the window sends no event at all, which is the point: the
        // apps are the engine's processes, not the window's.
        let running = AppState::Running {
            port: Some(8420),
            url: Some("http://localhost:8420".into()),
            ready: true,
        };
        assert_eq!(running.clone().next(Event::Dismissed), running);
        assert!(running.can_open());
        assert!(running.can_stop());
    }

    #[test]
    fn the_bar_never_goes_backwards_when_a_line_arrives_late() {
        let unpacking = run(
            AppState::Absent,
            &[
                Event::WorkBegan,
                Event::Stepped(Phase::Unpacking, "Unpacking it into /home/x.".into()),
                Event::Stepped(
                    Phase::Downloading,
                    "Downloading 3.5 MB from github.com.".into(),
                ),
            ],
        );
        match unpacking {
            AppState::Working {
                phase, fraction, ..
            } => {
                assert_eq!(phase, Phase::Unpacking);
                assert_eq!(fraction, Phase::Unpacking.fraction());
            }
            other => panic!("expected work in progress, got {other:?}"),
        }
    }

    #[test]
    fn a_library_has_nothing_to_start() {
        let end = run(
            AppState::Absent,
            &[Event::WorkBegan, Event::InstalledLibrary],
        );
        assert_eq!(end, AppState::Library);
        assert!(!end.can_open());
        assert!(!end.can_stop());
        assert!(!end.busy());
    }

    #[test]
    fn a_started_app_that_has_no_health_path_is_running_but_not_ready() {
        let end = AppState::Starting.next(Event::Started {
            port: Some(8421),
            url: Some("http://localhost:8421".into()),
            ready: false,
        });
        assert_eq!(
            end,
            AppState::Running {
                port: Some(8421),
                url: Some("http://localhost:8421".into()),
                ready: false
            }
        );
        assert!(end.can_open());
    }

    #[test]
    fn work_and_starting_are_the_only_busy_states() {
        assert!(AppState::Starting.busy());
        assert!(AppState::Absent.next(Event::WorkBegan).busy());
        assert!(!AppState::Stopped.busy());
        assert!(!AppState::Absent.busy());
        assert!(!AppState::Failed {
            message: "no".into(),
            was: Box::new(AppState::Stopped)
        }
        .busy());
    }

    #[test]
    fn removing_an_app_puts_it_back_in_the_catalog() {
        assert_eq!(AppState::Stopped.next(Event::Removed), AppState::Absent);
    }

    #[test]
    fn a_second_failure_keeps_the_first_states_footing() {
        let once = AppState::Stopped.next(Event::Refused("one".into()));
        let twice = once.next(Event::Refused("two".into()));
        assert_eq!(twice.next(Event::Dismissed), AppState::Stopped);
    }
}
