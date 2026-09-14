//! What the Tiiny has loaded, and whether an app can run on it.
//!
//! The device is the engine's business: `farm models --json` says what is
//! loaded and what is on disk, `farm models --watch --json` says when that
//! changes, and `farm start --json` refuses with the kinds it is missing. What
//! is here is the reading of those answers, and the one decision the window
//! makes out of them: whether an app's Start button can be pressed, and what to
//! say when it cannot.
//!
//! A need is a kind of model, like `chat` or `tts`, or a particular model by
//! id. That is the engine's rule (`met_by` in `farm/farm.py`) and it is matched
//! here the same way, because two answers to the same question is how a window
//! ends up disagreeing with the thing it is a face on.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

/// One model, loaded or only downloaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    /// The farm's word for what it is for: chat, tts, image, asr. `None` when
    /// the device did not say.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    /// NPU units it costs while resident. `None` when the device does not
    /// report a cost for it.
    #[serde(default)]
    pub units: Option<i64>,
    #[serde(default)]
    pub state: Option<String>,
}

impl Model {
    /// Whether this model answers one need, by kind or by name.
    pub fn meets(&self, need: &str) -> bool {
        self.id == need || self.kind.as_deref() == Some(need)
    }
}

/// What is left of the NPU budget. Units are residency rather than a compute
/// reservation, so several models sit there at once and this is what says
/// whether another one fits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Npu {
    #[serde(default)]
    pub total: Option<i64>,
    #[serde(default)]
    pub used: Option<i64>,
    #[serde(default)]
    pub available: Option<i64>,
}

/// One look at the device.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Snapshot {
    pub npu: Npu,
    pub loaded: Vec<Model>,
    pub downloaded: Vec<Model>,
}

impl Snapshot {
    pub fn read(value: &Value) -> Option<Snapshot> {
        serde_json::from_value(value.clone()).ok()
    }

    /// The loaded model answering this need, if any.
    pub fn meeting(&self, need: &str) -> Option<&Model> {
        self.loaded.iter().find(|row| row.meets(need))
    }

    /// Every need in the list that nothing loaded answers.
    pub fn unmet<'a>(&self, needs: &'a [String]) -> Vec<&'a str> {
        needs
            .iter()
            .filter(|need| self.meeting(need).is_none())
            .map(String::as_str)
            .collect()
    }

    /// The model the farm would load for a kind: the cheapest that fits what
    /// the NPU has left, because the person asked for a kind rather than for a
    /// particular model, and the cheapest is the one least likely to push
    /// something else out. A model whose cost the device does not report is
    /// treated as fitting, since refusing to offer it would be worse than
    /// trying. This is `pick_model` in the engine, and it has to agree.
    pub fn would_load(&self, need: &str) -> Option<&Model> {
        self.would_load_within(need, self.npu.available)
    }

    /// The same pick against a budget that is not the whole of what is free.
    ///
    /// An app can be missing two kinds at once, and two models that each fit on
    /// their own do not both fit. Offering Load and start for a pair that
    /// cannot both be resident would be offering something that fails.
    pub fn would_load_within(&self, need: &str, free: Option<i64>) -> Option<&Model> {
        self.downloaded
            .iter()
            .filter(|row| row.meets(need))
            .filter(|row| match (row.units, free) {
                (Some(units), Some(free)) => units <= free,
                _ => true,
            })
            .min_by_key(|row| (row.units.unwrap_or(0), row.id.clone()))
    }

    /// Everything of this kind on disk, whether or not it fits, so the window
    /// can tell "nothing downloaded" apart from "nothing that fits".
    pub fn on_disk<'a>(&'a self, need: &str) -> Vec<&'a Model> {
        self.downloaded
            .iter()
            .filter(|row| row.meets(need))
            .collect()
    }
}

/// What the window does about one app, given what it needs and what is loaded.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Needs {
    /// Nothing declared. The Start button is a Start button.
    NoneDeclared,
    /// Everything it needs is loaded.
    Met,
    /// The device could not be asked. Start is still offered, because refusing
    /// on a question nobody could answer would be worse than letting the engine
    /// try and say what happened.
    #[serde(rename_all = "camelCase")]
    Unknown { needs: Vec<String> },
    /// Something is missing. Start is refused, with the reason, and Load and
    /// start is offered for each kind the farm could load.
    #[serde(rename_all = "camelCase")]
    Unmet {
        missing: Vec<Missing>,
        /// Whether every missing kind has something the farm could load, which
        /// is the difference between offering Load and start and saying the
        /// model has to come from TiinyOS first.
        loadable: bool,
    },
}

/// One kind an app needs and has not got.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Missing {
    pub kind: String,
    /// What the farm would load for it, if anything.
    pub would_load: Option<String>,
    /// What it would cost, so the window can say so before a click.
    pub units: Option<i64>,
    /// Of this kind on disk, whatever it costs. Empty means nothing to load.
    pub on_disk: usize,
}

/// Work out what to do about one app.
pub fn needs_of(needs: &[String], snapshot: Option<&Snapshot>) -> Needs {
    if needs.is_empty() {
        return Needs::NoneDeclared;
    }
    let Some(snapshot) = snapshot else {
        return Needs::Unknown {
            needs: needs.to_vec(),
        };
    };
    let unmet = snapshot.unmet(needs);
    if unmet.is_empty() {
        return Needs::Met;
    }
    // The picks are made against a budget that shrinks as each one is taken,
    // because the app needs all of them at once and the Tiiny has one budget.
    let mut free = snapshot.npu.available;
    let missing: Vec<Missing> = unmet
        .iter()
        .map(|kind| {
            let pick = snapshot.would_load_within(kind, free);
            if let (Some(left), Some(cost)) = (free, pick.and_then(|row| row.units)) {
                free = Some(left - cost);
            }
            Missing {
                kind: (*kind).to_string(),
                would_load: pick.map(|row| row.id.clone()),
                units: pick.and_then(|row| row.units),
                on_disk: snapshot.on_disk(kind).len(),
            }
        })
        .collect();
    let loadable = missing.iter().all(|row| row.would_load.is_some());
    Needs::Unmet { missing, loadable }
}

impl Needs {
    /// Whether the window offers a plain Start.
    pub fn can_start(&self) -> bool {
        !matches!(self, Needs::Unmet { .. })
    }

    /// Whether the window offers Load and start.
    pub fn can_load_and_start(&self) -> bool {
        matches!(self, Needs::Unmet { loadable: true, .. })
    }

    /// The sentence under the button. Never a list of field names.
    pub fn sentence(&self, name: &str) -> String {
        match self {
            Needs::NoneDeclared | Needs::Met => String::new(),
            Needs::Unknown { .. } => {
                format!("{name} needs your Tiiny, and the farm could not ask it what is loaded.")
            }
            Needs::Unmet { missing, .. } => {
                let kinds: Vec<&str> = missing.iter().map(|row| row.kind.as_str()).collect();
                let listed = join_words(&kinds);
                let mut said = if kinds.len() == 1 {
                    format!("{name} needs a {listed} model, and your Tiiny has not got one loaded.")
                } else {
                    format!("{name} needs {listed} models, and your Tiiny has not got them loaded.")
                };
                let stuck: Vec<&str> = missing
                    .iter()
                    .filter(|row| row.would_load.is_none())
                    .map(|row| row.kind.as_str())
                    .collect();
                if !stuck.is_empty() {
                    // Two different reasons for being stuck, and unloading
                    // something cannot conjure up a model that was never
                    // downloaded, so each kind gets the sentence that is true
                    // of it.
                    let nothing: Vec<&str> = missing
                        .iter()
                        .filter(|row| row.would_load.is_none() && row.on_disk == 0)
                        .map(|row| row.kind.as_str())
                        .collect();
                    let oversized: Vec<&str> = missing
                        .iter()
                        .filter(|row| row.would_load.is_none() && row.on_disk > 0)
                        .map(|row| row.kind.as_str())
                        .collect();
                    if !nothing.is_empty() {
                        let listed = join_words(&nothing);
                        said.push(' ');
                        said.push_str(&format!(
                            "There is no {listed} model on it to load; download one in TiinyOS first."
                        ));
                    }
                    if !oversized.is_empty() {
                        let listed = join_words(&oversized);
                        said.push(' ');
                        said.push_str(&format!(
                            "Nothing it has for {listed} fits in the NPU units that are free; unload something first."
                        ));
                    }
                }
                said
            }
        }
    }
}

/// "a, b and c", the way the engine says it.
pub fn join_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// One line of `farm models --watch --json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// `loaded`, `unloaded` or `changed`.
    pub event: String,
    pub id: String,
    pub kind: Option<String>,
    pub units: Option<i64>,
    pub state: Option<String>,
    pub npu: Npu,
}

/// Read one line the watch printed.
///
/// Returns the change, or the error the engine reported, or nothing at all for
/// a line that is neither. The watch prints one object per change and nothing
/// else, but a child process's stdout is not a promise, and a half line at the
/// end of a stream must not look like a model that went away.
pub enum Watched {
    Changed(Box<Change>),
    Failed(String),
}

pub fn read_watch_line(line: &str) -> Option<Watched> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The farm stopped watching your Tiiny, and did not say why.");
        return Some(Watched::Failed(message.to_string()));
    }
    if value.get("command").and_then(Value::as_str) != Some("models") {
        return None;
    }
    let event = value.get("event").and_then(Value::as_str)?.to_string();
    if !matches!(event.as_str(), "loaded" | "unloaded" | "changed") {
        return None;
    }
    let id = value.get("id").and_then(Value::as_str)?.to_string();
    Some(Watched::Changed(Box::new(Change {
        event,
        id,
        kind: value
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        units: value.get("units").and_then(Value::as_i64),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string),
        npu: value
            .get("npu")
            .and_then(|npu| serde_json::from_value(npu.clone()).ok())
            .unwrap_or_default(),
    })))
}

/// Fold one change into the snapshot the window is holding.
///
/// The watch's one job is this: a change that never reaches the held snapshot
/// is a change the window never sees, because every button asks that snapshot
/// and not the device.
pub fn fold(held: &Mutex<Option<Snapshot>>, change: &Change) {
    if let Ok(mut slot) = held.lock() {
        if let Some(snapshot) = slot.as_mut() {
            apply(snapshot, change);
        }
    }
}

/// Apply one change to what the window believes is loaded.
///
/// The watch says what changed, not what everything is, so the window keeps its
/// own set. Doing it this way rather than asking for the whole picture on every
/// change is what makes an unload show up in the three seconds the engine
/// promises rather than whenever the next full answer happens to arrive.
pub fn apply(snapshot: &mut Snapshot, change: &Change) {
    snapshot.npu = change.npu;
    match change.event.as_str() {
        "unloaded" => {
            if let Some(at) = snapshot.loaded.iter().position(|row| row.id == change.id) {
                let gone = snapshot.loaded.remove(at);
                // It is still on the device's disk, so it moves rather than
                // disappearing: the window can offer to load it again.
                if !snapshot.downloaded.iter().any(|row| row.id == gone.id) {
                    snapshot.downloaded.push(Model {
                        state: None,
                        ..gone
                    });
                    snapshot.downloaded.sort_by(|a, b| a.id.cmp(&b.id));
                }
            }
        }
        _ => {
            snapshot.downloaded.retain(|held| held.id != change.id);
            match snapshot.loaded.iter_mut().find(|held| held.id == change.id) {
                // A change says what changed. A field it leaves out is a field
                // that did not, so writing None over the kind would make an
                // app that needs that kind look unmet because a model changed
                // state.
                Some(held) => {
                    if change.kind.is_some() {
                        held.kind = change.kind.clone();
                    }
                    if change.units.is_some() {
                        held.units = change.units;
                    }
                    if change.state.is_some() {
                        held.state = change.state.clone();
                    }
                }
                None => snapshot.loaded.push(Model {
                    id: change.id.clone(),
                    kind: change.kind.clone(),
                    capability: None,
                    units: change.units,
                    state: change.state.clone(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // The four models loaded on Jason's Tiiny at 21:57 on 2026-09-14, and some
    // of what was on its disk, copied out of `farm models --json` rather than
    // invented.
    fn tiiny() -> Snapshot {
        Snapshot::read(&json!({
            "npu": {"total": 100, "used": 68, "available": 32},
            "loaded": [
                {"id": "Qwen/Qwen3-8B", "kind": "chat", "units": 28, "state": "running"},
                {"id": "Qwen/Qwen3-Embedding-0.6B", "kind": "embedding", "units": 1, "state": "running"},
                {"id": "Tongyi-MAI/Z-Image-Turbo", "kind": "image", "units": 32, "state": "running"},
                {"id": "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice", "kind": "tts", "units": 7, "state": "running"}
            ],
            "downloaded": [
                {"id": "FireRedTeam/Firered-ASR2-LLM", "kind": "asr", "units": 6},
                {"id": "Qwen/Qwen3-ASR-1.7B", "kind": "asr", "units": 7},
                {"id": "Qwen/Qwen3-30B-A3B-Instruct", "kind": "chat", "units": 55},
                {"id": "openai/gpt-oss-20b", "kind": "chat", "units": 32},
                {"id": "RoyalCities/Foundation-1", "kind": "music", "units": 5}
            ]
        }))
        .unwrap()
    }

    fn kinds(of: &[&str]) -> Vec<String> {
        of.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn an_app_whose_models_are_all_loaded_can_start() {
        // Story Lantern: chat, image and tts, all four loaded.
        let needs = kinds(&["chat", "image", "tts"]);
        let state = needs_of(&needs, Some(&tiiny()));
        assert_eq!(state, Needs::Met);
        assert!(state.can_start());
        assert_eq!(state.sentence("Story Lantern"), "");
    }

    #[test]
    fn an_app_that_declares_nothing_is_never_in_the_way() {
        let state = needs_of(&[], Some(&tiiny()));
        assert_eq!(state, Needs::NoneDeclared);
        assert!(state.can_start());
        assert!(!state.can_load_and_start());
    }

    #[test]
    fn a_missing_kind_stops_start_and_offers_the_cheapest_that_fits() {
        // asr is on the disk twice and neither is loaded. 32 units are free, so
        // both fit, and the cheaper one is the offer.
        let state = needs_of(&kinds(&["asr"]), Some(&tiiny()));
        match &state {
            Needs::Unmet { missing, loadable } => {
                assert!(*loadable);
                assert_eq!(missing.len(), 1);
                assert_eq!(missing[0].kind, "asr");
                assert_eq!(
                    missing[0].would_load.as_deref(),
                    Some("FireRedTeam/Firered-ASR2-LLM")
                );
                assert_eq!(missing[0].units, Some(6));
                assert_eq!(missing[0].on_disk, 2);
            }
            other => panic!("expected an unmet need, got {other:?}"),
        }
        assert!(!state.can_start());
        assert!(state.can_load_and_start());
        assert_eq!(
            state.sentence("Titanium Tiiny Bot"),
            "Titanium Tiiny Bot needs a asr model, and your Tiiny has not got one loaded."
        );
    }

    #[test]
    fn a_kind_with_nothing_on_disk_says_to_download_one_first() {
        let state = needs_of(&kinds(&["video"]), Some(&tiiny()));
        assert!(!state.can_start());
        assert!(!state.can_load_and_start());
        assert!(state
            .sentence("Something")
            .contains("download one in TiinyOS first"));
    }

    #[test]
    fn a_kind_whose_models_do_not_fit_says_to_unload_something() {
        // One chat model on disk at 55 units, and only 32 free.
        let tight = Snapshot::read(&json!({
            "npu": {"total": 100, "used": 68, "available": 32},
            "loaded": [],
            "downloaded": [{"id": "Qwen/Qwen3-30B-A3B-Instruct", "kind": "chat", "units": 55}]
        }))
        .unwrap();
        let state = needs_of(&kinds(&["chat"]), Some(&tight));
        assert!(!state.can_start());
        assert!(!state.can_load_and_start());
        assert!(state
            .sentence("Daybreak")
            .contains("unload something first"));
    }

    #[test]
    fn a_model_whose_cost_the_device_does_not_report_is_still_offered() {
        let unknown = Snapshot::read(&json!({
            "npu": {"total": 100, "used": 99, "available": 1},
            "loaded": [],
            "downloaded": [{"id": "mystery/model", "kind": "chat"}]
        }))
        .unwrap();
        let state = needs_of(&kinds(&["chat"]), Some(&unknown));
        assert!(state.can_load_and_start());
    }

    #[test]
    fn a_need_can_name_a_model_rather_than_a_kind() {
        let state = needs_of(&kinds(&["Qwen/Qwen3-8B"]), Some(&tiiny()));
        assert_eq!(state, Needs::Met);
        let other = needs_of(&kinds(&["openai/gpt-oss-20b"]), Some(&tiiny()));
        assert!(other.can_load_and_start());
    }

    #[test]
    fn a_device_nobody_could_ask_does_not_stop_the_button() {
        let state = needs_of(&kinds(&["chat"]), None);
        assert!(state.can_start());
        assert!(!state.can_load_and_start());
        assert!(state.sentence("Daybreak").contains("could not ask"));
    }

    #[test]
    fn two_missing_kinds_read_as_a_sentence() {
        let bare = Snapshot::read(&json!({
            "npu": {"total": 100, "used": 0, "available": 100},
            "loaded": [],
            "downloaded": [
                {"id": "a/chat", "kind": "chat", "units": 10},
                {"id": "a/tts", "kind": "tts", "units": 5}
            ]
        }))
        .unwrap();
        let state = needs_of(&kinds(&["chat", "tts"]), Some(&bare));
        assert_eq!(
            state.sentence("Story Lantern"),
            "Story Lantern needs chat and tts models, and your Tiiny has not got them loaded."
        );
    }

    // ---- the watch ------------------------------------------------------

    #[test]
    fn a_watch_line_is_read_into_a_change() {
        let line = r#"{"command":"models","event":"unloaded","id":"Qwen/Qwen3-8B","kind":"chat","capability":"chat","units":28,"state":"running","npu":{"total":100,"used":40,"available":60},"at":1789431000}"#;
        match read_watch_line(line) {
            Some(Watched::Changed(change)) => {
                assert_eq!(change.event, "unloaded");
                assert_eq!(change.id, "Qwen/Qwen3-8B");
                assert_eq!(change.kind.as_deref(), Some("chat"));
                assert_eq!(change.npu.available, Some(60));
            }
            _ => panic!("expected a change"),
        }
    }

    #[test]
    fn the_watch_saying_it_failed_is_not_a_model_going_away() {
        let line = r#"{"error":{"command":"models","message":"No Tiiny is on file."}}"#;
        match read_watch_line(line) {
            Some(Watched::Failed(said)) => assert_eq!(said, "No Tiiny is on file."),
            _ => panic!("expected a failure"),
        }
    }

    #[test]
    fn anything_that_is_not_a_change_moves_nothing() {
        assert!(read_watch_line("").is_none());
        assert!(read_watch_line("Watching your Tiiny's models, asking every 3 seconds.").is_none());
        // A half line at the end of a stream.
        assert!(read_watch_line(r#"{"command":"models","event":"unloa"#).is_none());
        // The right shape, an event nobody defined.
        assert!(read_watch_line(r#"{"command":"models","event":"exploded","id":"x"}"#).is_none());
        // Another command's object on the same stdout.
        assert!(read_watch_line(r#"{"command":"status","running":[]}"#).is_none());
    }

    #[test]
    fn an_unload_moves_the_model_to_the_disk_list_and_frees_its_units() {
        let mut state = tiiny();
        let line = r#"{"command":"models","event":"unloaded","id":"Qwen/Qwen3-8B","kind":"chat","units":28,"state":"running","npu":{"total":100,"used":40,"available":60}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(line) else {
            panic!("expected a change")
        };
        apply(&mut state, &change);
        assert!(state.meeting("chat").is_none());
        assert!(state.downloaded.iter().any(|row| row.id == "Qwen/Qwen3-8B"));
        assert_eq!(state.npu.available, Some(60));
        // And the app that needed chat can now be offered a load.
        let needs = needs_of(&kinds(&["chat"]), Some(&state));
        assert!(!needs.can_start());
        assert!(needs.can_load_and_start());
    }

    #[test]
    fn a_load_takes_the_model_off_the_disk_list_and_meets_the_need() {
        let mut state = tiiny();
        // Take chat away first, the way the run does.
        let gone = r#"{"command":"models","event":"unloaded","id":"Qwen/Qwen3-8B","kind":"chat","units":28,"npu":{"total":100,"used":40,"available":60}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(gone) else {
            panic!()
        };
        apply(&mut state, &change);

        let back = r#"{"command":"models","event":"loaded","id":"Qwen/Qwen3-8B","kind":"chat","units":28,"state":"running","npu":{"total":100,"used":68,"available":32}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(back) else {
            panic!()
        };
        apply(&mut state, &change);

        assert_eq!(
            state.meeting("chat").map(|row| row.id.as_str()),
            Some("Qwen/Qwen3-8B")
        );
        assert!(!state.downloaded.iter().any(|row| row.id == "Qwen/Qwen3-8B"));
        assert_eq!(state.npu.available, Some(32));
        assert_eq!(needs_of(&kinds(&["chat"]), Some(&state)), Needs::Met);
    }

    #[test]
    fn a_model_that_only_changed_state_is_updated_in_place() {
        let mut state = tiiny();
        let line = r#"{"command":"models","event":"changed","id":"Qwen/Qwen3-8B","kind":"chat","units":28,"state":"loading","npu":{"total":100,"used":68,"available":32}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(line) else {
            panic!()
        };
        apply(&mut state, &change);
        assert_eq!(state.loaded.len(), 4);
        assert_eq!(
            state
                .meeting("chat")
                .and_then(|row| row.state.clone())
                .as_deref(),
            Some("loading")
        );
    }

    #[test]
    fn a_model_the_window_never_saw_arriving_is_still_added() {
        let mut state = tiiny();
        let line = r#"{"command":"models","event":"loaded","id":"Qwen/Qwen3-ASR-1.7B","kind":"asr","units":7,"state":"running","npu":{"total":100,"used":75,"available":25}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(line) else {
            panic!()
        };
        apply(&mut state, &change);
        assert_eq!(
            state.meeting("asr").map(|row| row.id.as_str()),
            Some("Qwen/Qwen3-ASR-1.7B")
        );
        assert!(!state
            .downloaded
            .iter()
            .any(|row| row.id == "Qwen/Qwen3-ASR-1.7B"));
    }

    #[test]
    fn the_watch_moves_the_snapshot_the_window_is_holding() {
        // The bug this guards: the watch read the line, told the window to
        // look again, and left the held snapshot where it was, so the window
        // looked at the same stale answer and the chips never changed.
        let held: Mutex<Option<Snapshot>> = Mutex::new(Some(tiiny()));
        let line = r#"{"command":"models","event":"unloaded","id":"Qwen/Qwen3-8B","kind":"chat","units":28,"state":null,"npu":{"total":100,"used":40,"available":60}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(line) else {
            panic!()
        };
        fold(&held, &change);
        let after = held.lock().unwrap().clone().unwrap();
        assert!(after.meeting("chat").is_none());
        assert_eq!(after.npu.available, Some(60));
    }

    #[test]
    fn a_change_with_no_snapshot_yet_is_dropped_rather_than_guessed_at() {
        let held: Mutex<Option<Snapshot>> = Mutex::new(None);
        let line = r#"{"command":"models","event":"loaded","id":"Qwen/Qwen3-8B","kind":"chat","units":28,"state":"running","npu":{"total":100,"used":68,"available":32}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(line) else {
            panic!()
        };
        fold(&held, &change);
        assert!(held.lock().unwrap().is_none());
    }

    #[test]
    fn two_missing_kinds_are_picked_against_one_budget() {
        // 32 units free, and two 28 unit models. Each fits on its own; both
        // together do not, so the second one is not offered.
        let mut state = tiiny();
        state.npu.available = Some(32);
        state
            .loaded
            .retain(|row| row.kind.as_deref() != Some("chat"));
        state
            .loaded
            .retain(|row| row.kind.as_deref() != Some("tts"));
        state.downloaded.push(Model {
            id: "someone/tts-28".into(),
            kind: Some("tts".into()),
            capability: None,
            units: Some(28),
            state: None,
        });
        state.downloaded.push(Model {
            id: "someone/chat-28".into(),
            kind: Some("chat".into()),
            capability: None,
            units: Some(28),
            state: None,
        });
        let needs = vec!["chat".to_string(), "tts".to_string()];
        let Needs::Unmet { missing, loadable } = needs_of(&needs, Some(&state)) else {
            panic!()
        };
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].would_load.as_deref(), Some("someone/chat-28"));
        assert_eq!(missing[1].would_load, None);
        assert!(!loadable);
    }

    #[test]
    fn a_kind_with_nothing_downloaded_and_one_that_does_not_fit_get_their_own_sentence() {
        let mut state = tiiny();
        state.npu.available = Some(1);
        state
            .loaded
            .retain(|row| row.kind.as_deref() != Some("chat"));
        state
            .loaded
            .retain(|row| row.kind.as_deref() != Some("image"));
        state
            .downloaded
            .retain(|row| row.kind.as_deref() != Some("image"));
        let needs = vec!["chat".to_string(), "image".to_string()];
        let said = needs_of(&needs, Some(&state)).sentence("Story Lantern");
        assert!(
            said.contains("There is no image model on it to load"),
            "{said}"
        );
        assert!(
            said.contains("Nothing it has for chat fits in the NPU units that are free"),
            "{said}"
        );
    }

    #[test]
    fn a_change_that_only_says_the_state_keeps_the_kind() {
        let mut state = tiiny();
        let line = r#"{"command":"models","event":"changed","id":"Qwen/Qwen3-8B","state":"loading","npu":{"total":100,"used":68,"available":32}}"#;
        let Some(Watched::Changed(change)) = read_watch_line(line) else {
            panic!()
        };
        apply(&mut state, &change);
        let row = state.meeting("chat").expect("the chat model is still chat");
        assert_eq!(row.id, "Qwen/Qwen3-8B");
        assert_eq!(row.units, Some(28));
        assert_eq!(row.state.as_deref(), Some("loading"));
    }
}
