//! What the two corners of a grid card say, and the little pile of seeds in
//! its footer.
//!
//! All three rules live here rather than in the page, for the same reason the
//! NPU arithmetic does: a rule written twice is a rule that will disagree with
//! itself. The window is handed a decision per app and draws it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One piece of a version, so that 0.1.10 is newer than 0.1.9 rather than
/// earlier in the alphabet.
#[derive(Debug, PartialEq, Eq)]
enum Part {
    Num(u64),
    Text(String),
}

fn parts(version: &str) -> Vec<Part> {
    version
        .split(['.', '-', '+'])
        .filter(|piece| !piece.is_empty())
        .map(|piece| match piece.parse::<u64>() {
            Ok(number) => Part::Num(number),
            Err(_) => Part::Text(piece.to_ascii_lowercase()),
        })
        .collect()
}

/// Whether `want` is a later version than `have`.
///
/// Numbers are compared as numbers, so 0.1.10 comes after 0.1.9. A version
/// that carries a tag is earlier than the same version without one, so 0.2.0
/// is newer than 0.2.0-rc1 and both are newer than 0.1.9. Anything that cannot
/// be read at all is never called newer: a badge that is wrong is worse than a
/// badge that is missing.
pub fn is_newer(have: &str, want: &str) -> bool {
    let have = have.trim();
    let want = want.trim();
    if have.is_empty() || want.is_empty() || have == want {
        return false;
    }
    let (mine, theirs) = (parts(have), parts(want));
    if mine.is_empty() || theirs.is_empty() {
        return false;
    }
    let mut at = 0;
    loop {
        match (mine.get(at), theirs.get(at)) {
            (None, None) => return false,
            // One of them ran out. A number after the end is a later release
            // (0.1 then 0.1.1); a word after the end is a pre-release of it
            // (0.2.0 then 0.2.0-rc1), so the shorter one is the later.
            (None, Some(Part::Num(_))) => return true,
            (None, Some(Part::Text(_))) => return false,
            (Some(Part::Num(_)), None) => return false,
            (Some(Part::Text(_)), None) => return true,
            (Some(a), Some(b)) => match (a, b) {
                (Part::Num(a), Part::Num(b)) => {
                    if a != b {
                        return b > a;
                    }
                }
                (Part::Text(a), Part::Text(b)) => {
                    if a != b {
                        return b > a;
                    }
                }
                // A release beats its own pre-release either way round.
                (Part::Num(_), Part::Text(_)) => return false,
                (Part::Text(_), Part::Num(_)) => return true,
            },
        }
        at += 1;
    }
}

/// One row of the catalog, as the window already has it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRow {
    pub id: String,
    /// What the catalog publishes.
    #[serde(default)]
    pub version: Option<String>,
    /// What is planted on this computer, when anything is.
    #[serde(default)]
    pub installed: Option<String>,
    /// What the engine already worked out, which is the same comparison the
    /// detail card's Update button uses. Trusted first; the version numbers
    /// are only read when it says nothing.
    #[serde(default)]
    pub update_available: Option<String>,
}

/// What one grid card puts in its corners.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    /// The version waiting, when something older than it is planted here.
    pub update: Option<String>,
    /// This launcher has never put this app on a screen before.
    pub fresh: bool,
}

/// What a whole screen of cards says, and the seen map it leaves behind.
///
/// The second half of the answer is `Some` only when something has to be
/// written: the very first run, where every app on the first screen is marked
/// seen without a word, because a wall of New says nothing to somebody who has
/// never opened the app before.
pub fn marks(
    rows: &[CardRow],
    seen: Option<&BTreeMap<String, String>>,
) -> (BTreeMap<String, Mark>, Option<BTreeMap<String, String>>) {
    let first_run = seen.is_none();
    let known = seen.cloned().unwrap_or_default();
    let mut out = BTreeMap::new();
    let mut next = known.clone();

    for row in rows {
        let update = match (&row.update_available, &row.installed, &row.version) {
            (Some(version), _, _) if !version.trim().is_empty() => Some(version.clone()),
            // No word from the engine, so read the two numbers. An app that is
            // not planted here can never have an update waiting.
            (_, Some(have), Some(want)) if is_newer(have, want) => Some(want.clone()),
            _ => None,
        };
        let fresh = !first_run && !known.contains_key(&row.id);
        if first_run {
            next.insert(
                row.id.clone(),
                row.version.clone().unwrap_or_default().trim().to_string(),
            );
        }
        out.insert(row.id.clone(), Mark { update, fresh });
    }

    (out, if first_run { Some(next) } else { None })
}

/// The seen map after somebody opened one card. A new version of an app that
/// was already seen is not New again: it is an update, or it is nothing.
pub fn opened(
    seen: Option<&BTreeMap<String, String>>,
    id: &str,
    version: Option<&str>,
) -> BTreeMap<String, String> {
    let mut next = seen.cloned().unwrap_or_default();
    next.entry(id.to_string())
        .or_insert_with(|| version.unwrap_or_default().trim().to_string());
    next
}

/// The shape of the little pile beside the Planted chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pile {
    /// Nobody has given this one a seed yet: an empty outline.
    None,
    /// Few enough to count by looking.
    Seeds,
    /// Too many to count, so the pile stops growing and the number says it.
    Heap,
}

/// A pile of seeds, bottom row first, and the words a screen reader hears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stack {
    pub kind: Pile,
    /// How many seeds sit on each row, the bottom of the pile first.
    pub rows: Vec<u32>,
    pub count: u64,
    /// Written beside the pile, and only once the pile has stopped growing.
    pub number: Option<u64>,
    pub words: String,
}

/// Zero is an outline dot, one to three is that many seeds in a row, four to
/// nine is two rows with the wider one underneath, and ten or more is the full
/// pile with the number beside it. The same rules the site draws, so a card in
/// the launcher and a card in a browser are the same card.
pub fn stack(count: u64) -> Stack {
    let words = match count {
        0 => "No seeds yet".to_string(),
        1 => "1 seed".to_string(),
        many => format!("{many} seeds"),
    };
    match count {
        0 => Stack {
            kind: Pile::None,
            rows: Vec::new(),
            count,
            number: None,
            words,
        },
        1..=3 => Stack {
            kind: Pile::Seeds,
            rows: vec![count as u32],
            count,
            number: None,
            words,
        },
        4..=9 => {
            let bottom = count.div_ceil(2) as u32;
            let top = (count / 2) as u32;
            Stack {
                kind: Pile::Seeds,
                rows: vec![bottom, top],
                count,
                number: None,
                words,
            }
        }
        _ => Stack {
            kind: Pile::Heap,
            rows: vec![5, 4],
            count,
            number: Some(count),
            words,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, version: &str, installed: Option<&str>, update: Option<&str>) -> CardRow {
        CardRow {
            id: id.to_string(),
            version: Some(version.to_string()),
            installed: installed.map(str::to_string),
            update_available: update.map(str::to_string),
        }
    }

    #[test]
    fn ten_comes_after_nine_rather_than_before_it() {
        assert!(is_newer("0.1.9", "0.1.10"));
        assert!(!is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("1.2.3", "2.0.0"));
    }

    #[test]
    fn the_same_version_is_never_an_update() {
        assert!(!is_newer("0.1.3", "0.1.3"));
        assert!(!is_newer(" 0.1.3 ", "0.1.3"));
        assert!(!is_newer("0.1.3", "0.1.2"));
    }

    #[test]
    fn a_longer_number_is_later_and_a_tag_on_the_end_is_earlier() {
        assert!(is_newer("0.1", "0.1.1"));
        assert!(!is_newer("0.1.1", "0.1"));
        assert!(is_newer("0.2.0-rc1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0-rc1"));
        assert!(is_newer("0.2.0-rc1", "0.2.0-rc2"));
    }

    #[test]
    fn a_version_nobody_can_read_is_never_an_update() {
        assert!(!is_newer("", "0.1.2"));
        assert!(!is_newer("0.1.2", ""));
        assert!(!is_newer("...", "0.1.2"));
        assert!(!is_newer("0.1.2", "..."));
    }

    #[test]
    fn the_first_run_marks_everything_seen_without_saying_a_word() {
        let rows = vec![
            row("story-lantern", "0.1.3", None, None),
            row("tiiny-bench", "0.1.4", None, None),
        ];
        let (marks, write) = marks(&rows, None);
        assert!(!marks["story-lantern"].fresh);
        assert!(!marks["tiiny-bench"].fresh);
        let write = write.expect("the first run has a seen map to save");
        assert_eq!(write.len(), 2);
        assert_eq!(write["tiiny-bench"], "0.1.4");
    }

    #[test]
    fn an_id_that_arrives_after_the_first_run_is_new() {
        let seen = BTreeMap::from([("story-lantern".to_string(), "0.1.3".to_string())]);
        let rows = vec![
            row("story-lantern", "0.1.3", None, None),
            row("daybreak", "0.1.0", None, None),
        ];
        let (marks, write) = marks(&rows, Some(&seen));
        assert!(!marks["story-lantern"].fresh);
        assert!(marks["daybreak"].fresh);
        // Nothing is written by looking. New goes away when somebody opens it.
        assert!(write.is_none());
    }

    #[test]
    fn opening_a_card_clears_new_and_nothing_else() {
        let seen = BTreeMap::from([("story-lantern".to_string(), "0.1.3".to_string())]);
        let next = opened(Some(&seen), "daybreak", Some("0.1.0"));
        assert_eq!(next["daybreak"], "0.1.0");
        assert_eq!(next["story-lantern"], "0.1.3");
        let rows = vec![row("daybreak", "0.1.0", None, None)];
        assert!(!marks(&rows, Some(&next)).0["daybreak"].fresh);
    }

    #[test]
    fn opening_a_card_twice_keeps_the_version_it_was_first_seen_at() {
        let first = opened(None, "daybreak", Some("0.1.0"));
        let again = opened(Some(&first), "daybreak", Some("0.2.0"));
        assert_eq!(again["daybreak"], "0.1.0");
    }

    #[test]
    fn a_newer_version_of_a_seen_app_is_an_update_and_never_new_again() {
        let seen = BTreeMap::from([("story-lantern".to_string(), "0.1.3".to_string())]);
        let planted = vec![row("story-lantern", "0.1.4", Some("0.1.3"), None)];
        let marked = marks(&planted, Some(&seen)).0;
        assert!(!marked["story-lantern"].fresh);
        assert_eq!(marked["story-lantern"].update.as_deref(), Some("0.1.4"));

        // The same app newer in the catalog but not planted here says nothing.
        let not_planted = vec![row("story-lantern", "0.1.4", None, None)];
        let marked = marks(&not_planted, Some(&seen)).0;
        assert!(!marked["story-lantern"].fresh);
        assert_eq!(marked["story-lantern"].update, None);
    }

    #[test]
    fn the_engine_is_believed_before_the_version_numbers_are_read() {
        let rows = vec![row("onelane", "0.1.2", Some("0.1.1"), Some("0.1.3"))];
        let marked = marks(&rows, Some(&BTreeMap::new())).0;
        assert_eq!(marked["onelane"].update.as_deref(), Some("0.1.3"));
    }

    #[test]
    fn what_is_planted_and_current_has_no_update_waiting() {
        let rows = vec![row("onelane", "0.1.2", Some("0.1.2"), None)];
        assert_eq!(
            marks(&rows, Some(&BTreeMap::new())).0["onelane"].update,
            None
        );
    }

    #[test]
    fn nobody_has_given_it_a_seed_yet() {
        let pile = stack(0);
        assert_eq!(pile.kind, Pile::None);
        assert!(pile.rows.is_empty());
        assert_eq!(pile.number, None);
        assert_eq!(pile.words, "No seeds yet");
    }

    #[test]
    fn one_to_three_seeds_sit_in_a_single_row() {
        for count in 1..=3u64 {
            let pile = stack(count);
            assert_eq!(pile.kind, Pile::Seeds);
            assert_eq!(pile.rows, vec![count as u32]);
            assert_eq!(pile.number, None);
        }
        assert_eq!(stack(1).words, "1 seed");
        assert_eq!(stack(3).words, "3 seeds");
    }

    #[test]
    fn four_to_nine_seeds_pile_into_two_rows_with_the_wider_one_underneath() {
        assert_eq!(stack(4).rows, vec![2, 2]);
        assert_eq!(stack(5).rows, vec![3, 2]);
        assert_eq!(stack(9).rows, vec![5, 4]);
        for count in 4..=9u64 {
            let pile = stack(count);
            assert_eq!(pile.kind, Pile::Seeds);
            assert_eq!(pile.rows.len(), 2);
            assert_eq!(u64::from(pile.rows[0] + pile.rows[1]), count);
            assert!(pile.rows[0] >= pile.rows[1], "the bottom row is the wider");
            assert_eq!(pile.number, None);
        }
    }

    #[test]
    fn ten_and_up_stops_growing_and_says_the_number() {
        for count in [10u64, 11, 250, 9_999] {
            let pile = stack(count);
            assert_eq!(pile.kind, Pile::Heap);
            assert_eq!(pile.rows, vec![5, 4]);
            assert_eq!(pile.number, Some(count));
        }
        assert_eq!(stack(10).words, "10 seeds");
    }

    /// The page reads these field names off the answer, so a rename here is a
    /// pile that stops drawing. Printed as well, because the strip of shapes in
    /// the report is drawn from exactly this.
    #[test]
    fn the_window_is_handed_the_shape_in_the_names_it_reads() {
        let one = serde_json::to_value(stack(1)).unwrap();
        assert_eq!(one["kind"], "seeds");
        assert_eq!(one["rows"], serde_json::json!([1]));
        assert_eq!(one["count"], 1);
        assert_eq!(one["number"], serde_json::Value::Null);
        assert_eq!(one["words"], "1 seed");
        assert_eq!(serde_json::to_value(stack(0)).unwrap()["kind"], "none");
        assert_eq!(serde_json::to_value(stack(10)).unwrap()["kind"], "heap");

        let strip: Vec<_> = [0u64, 1, 2, 3, 4, 5, 7, 9, 12, 148]
            .iter()
            .map(|count| stack(*count))
            .collect();
        println!("SHAPES {}", serde_json::to_string(&strip).unwrap());
    }

    #[test]
    fn the_pile_never_shrinks_as_the_count_goes_up() {
        let mut drawn = 0;
        for count in 0..=12u64 {
            let pile = stack(count);
            let seeds: u32 = pile.rows.iter().sum();
            assert!(
                seeds >= drawn,
                "{count} seeds drew fewer than {} did",
                count - 1
            );
            drawn = seeds;
        }
    }
}
