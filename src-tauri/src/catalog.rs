//! The three things the launcher reads over the network itself: the catalog,
//! how many seeds each app has been given, and whether a Tiiny is where
//! somebody said it is.
//!
//! All three are read from Rust rather than from the page. A web view asking
//! for them would be a second HTTP client with a different idea of timeouts,
//! the device probe has to accept a 401 as a yes, which is easier to say here,
//! and the page has no network origin of its own to ask with: the content
//! policy on this app allows `ipc:` and nothing else.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::badges::{self, Stack};

pub const CATALOG: &str = "https://tiinyapp.farm/manifests/";
pub const SITE: &str = "https://tiinyapp.farm";

/// The addresses a Tiiny answers on, in the order the install page documents
/// them. The first is what the TiinyOS client publishes on a Mac.
pub const WELL_KNOWN_BASE: &str = "http://openai.api.tiiny/v1";

/// Every app's seed and comment count in one answer.
pub const COUNTS: &str = "https://tiinyapp.farm/api/social/counts";

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(concat!(
            "tiinyapp-farm-launcher/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| format!("The launcher could not open a connection: {error}."))
}

/// One app's whole catalog entry: the icon, the pitch, what it needs and what
/// access it asks for. The card shows all of it before anything downloads.
pub async fn manifest(id: &str) -> Result<Value, String> {
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || id.is_empty()
    {
        return Err(format!("{id} is not an app id."));
    }
    let url = format!("{CATALOG}{id}.json");
    let response = client(Duration::from_secs(20))?
        .get(&url)
        .send()
        .await
        .map_err(|_| {
            format!("The catalog at {SITE} could not be reached. Check this computer's network.")
        })?;
    if !response.status().is_success() {
        return Err(format!(
            "The catalog has no entry for {id} ({}).",
            response.status().as_u16()
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|_| format!("The catalog entry for {id} is not readable."))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub base: String,
    /// Something answered. A Tiiny with a key set answers 401 to a request
    /// with no key, and that is still a Tiiny.
    pub found: bool,
    /// What it said, in words, when it did not.
    pub detail: Option<String>,
    /// Whether it answered without needing a key, which means the key is not
    /// set on the device rather than that the launcher has one.
    pub open: bool,
}

/// Knock on one address and see whether a Tiiny is behind it.
pub async fn probe(base: &str) -> Probe {
    let trimmed = base.trim().trim_end_matches('/');
    let url = format!("{trimmed}/models");
    let built = match client(Duration::from_secs(4)) {
        Ok(client) => client,
        Err(error) => {
            return Probe {
                base: trimmed.to_string(),
                found: false,
                detail: Some(error),
                open: false,
            }
        }
    };
    match built.get(&url).send().await {
        Ok(response) => {
            let code = response.status().as_u16();
            Probe {
                base: trimmed.to_string(),
                found: code == 200 || code == 401 || code == 403,
                detail: match code {
                    200 | 401 | 403 => None,
                    404 => Some("Something answered there, but it is not a Tiiny.".into()),
                    other => Some(format!("It answered {other}.")),
                },
                open: code == 200,
            }
        }
        Err(_) => Probe {
            base: trimmed.to_string(),
            found: false,
            detail: Some("Nothing answered there.".into()),
            open: false,
        },
    }
}

/// One app's icon, as PNG bytes, for the window the launcher opens on it.
///
/// Best effort on purpose: a window with the wrong icon is a small thing and a
/// window that would not open because an icon did not download is not.
pub async fn icon_bytes(id: &str) -> Option<Vec<u8>> {
    let manifest = manifest(id).await.ok()?;
    let path = manifest.get("media")?.get("icon")?.as_str()?;
    let url = if path.starts_with("http") {
        path.to_string()
    } else {
        format!("{SITE}{path}")
    };
    let response = client(Duration::from_secs(15))
        .ok()?
        .get(&url)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    Some(response.bytes().await.ok()?.to_vec())
}

/// How many seeds one app has been given, and the pile that draws.
///
/// The count is read here and the shape is decided in `badges`, so the window
/// is handed a pile rather than a number to do arithmetic on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Social {
    pub seeds: u64,
    pub comments: u64,
    pub stack: Stack,
}

impl Social {
    fn new(seeds: u64, comments: u64) -> Social {
        Social {
            seeds,
            comments,
            stack: badges::stack(seeds),
        }
    }
}

fn count(value: Option<&Value>) -> u64 {
    match value {
        // The batch route counts the comments; the per-app route lists them.
        Some(Value::Array(list)) => list.len() as u64,
        Some(other) => other.as_u64().unwrap_or(0),
        None => 0,
    }
}

/// Read the whole screen's counts out of the batch answer. Public so that a
/// test can hold the shape of that answer without a network.
pub fn counts_from_batch(body: &Value, ids: &[String]) -> Option<BTreeMap<String, Social>> {
    let apps = body.get("apps")?.as_object()?;
    let mut out = BTreeMap::new();
    for id in ids {
        let row = match apps.get(id) {
            Some(row) => row,
            // An app the route has never heard of has had no seeds, which is a
            // real answer rather than a gap.
            None => {
                out.insert(id.clone(), Social::new(0, 0));
                continue;
            }
        };
        out.insert(
            id.clone(),
            Social::new(count(row.get("seeds")), count(row.get("comments"))),
        );
    }
    Some(out)
}

/// One app's counts out of the older per-app answer, where the seed count is
/// called `thumbs` and the comments arrive as a list.
pub fn count_from_one(body: &Value) -> Social {
    Social::new(count(body.get("thumbs")), count(body.get("comments")))
}

fn is_app_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Seeds and comments for a screen of apps.
///
/// One request for the lot where the farm answers it, and one request per app
/// where it does not, because the batch route is newer than some of the copies
/// of the launcher that will ask for it. Nothing here is allowed to fail
/// loudly: a card with no pile on it is a card, and a card that would not draw
/// because a count did not arrive is not.
pub async fn social_counts(ids: &[String]) -> BTreeMap<String, Social> {
    let wanted: Vec<String> = ids.iter().filter(|id| is_app_id(id)).cloned().collect();
    if wanted.is_empty() {
        return BTreeMap::new();
    }
    let client = match client(Duration::from_secs(12)) {
        Ok(client) => client,
        Err(_) => return BTreeMap::new(),
    };

    if let Ok(response) = client.get(COUNTS).send().await {
        if response.status().is_success() {
            if let Ok(body) = response.json::<Value>().await {
                if let Some(found) = counts_from_batch(&body, &wanted) {
                    return found;
                }
            }
        }
    }

    // The batch route is not there, or did not answer in a shape anybody
    // recognises. Ask about each app instead.
    let asking: Vec<_> = wanted
        .iter()
        .map(|id| {
            let client = client.clone();
            let id = id.clone();
            tauri::async_runtime::spawn(async move {
                let url = format!("{SITE}/api/seeds/{id}/social");
                let response = client.get(&url).send().await.ok()?;
                if !response.status().is_success() {
                    return None;
                }
                Some((id, count_from_one(&response.json::<Value>().await.ok()?)))
            })
        })
        .collect();
    let mut out = BTreeMap::new();
    for handle in asking {
        if let Ok(Some((id, social))) = handle.await {
            out.insert(id, social);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_that_is_not_an_id_never_becomes_a_url() {
        let answer = tauri::async_runtime::block_on(manifest("../../etc/passwd"));
        assert!(answer.unwrap_err().contains("is not an app id"));
        let answer = tauri::async_runtime::block_on(manifest(""));
        assert!(answer.unwrap_err().contains("is not an app id"));
        let answer = tauri::async_runtime::block_on(manifest("Story Lantern"));
        assert!(answer.unwrap_err().contains("is not an app id"));
    }

    #[test]
    fn the_well_known_address_is_the_one_the_install_page_documents() {
        assert_eq!(WELL_KNOWN_BASE, "http://openai.api.tiiny/v1");
    }

    #[test]
    fn the_batch_answer_is_read_for_every_app_that_was_asked_about() {
        let body = serde_json::json!({
            "apps": {
                "tiiny-bench": { "seeds": 1, "comments": 0 },
                "story-lantern": { "seeds": 12, "comments": 3 },
                "not-asked-about": { "seeds": 99, "comments": 0 }
            },
            "makers": { "webdevtodayjason": { "seeds": 13 } }
        });
        let ids = [
            "tiiny-bench".to_string(),
            "story-lantern".to_string(),
            "daybreak".to_string(),
        ];
        let found = counts_from_batch(&body, &ids).expect("that shape is the batch answer");
        assert_eq!(found.len(), 3, "one row per app asked about, and no more");
        assert_eq!(found["tiiny-bench"].seeds, 1);
        assert_eq!(found["story-lantern"].comments, 3);
        assert_eq!(found["story-lantern"].stack.number, Some(12));
        // An app the route said nothing about has had no seeds.
        assert_eq!(found["daybreak"].seeds, 0);
        assert_eq!(found["daybreak"].stack.kind, crate::badges::Pile::None);
    }

    #[test]
    fn an_answer_that_is_not_the_batch_shape_is_no_answer_at_all() {
        assert!(counts_from_batch(
            &serde_json::json!({"error": "This route does not exist."}),
            &[]
        )
        .is_none());
        assert!(counts_from_batch(&serde_json::json!([]), &[]).is_none());
    }

    #[test]
    fn the_older_per_app_answer_calls_the_seeds_thumbs_and_lists_the_comments() {
        let one = count_from_one(&serde_json::json!({
            "thumbs": 5,
            "mine": false,
            "comments": [{ "body": "nice" }, { "body": "planted" }]
        }));
        assert_eq!(one.seeds, 5);
        assert_eq!(one.comments, 2);
        assert_eq!(one.stack.rows, vec![3, 2]);
        // And an answer missing the fields entirely is a zero rather than a
        // panic, because this route is older than the fields it is read for.
        assert_eq!(count_from_one(&serde_json::json!({})).seeds, 0);
    }

    #[test]
    fn an_id_that_is_not_an_id_is_never_asked_about() {
        assert!(is_app_id("story-lantern"));
        assert!(!is_app_id("../../etc/passwd"));
        assert!(!is_app_id("Story Lantern"));
        assert!(!is_app_id(""));
        let none = tauri::async_runtime::block_on(social_counts(&["../secret".to_string()]));
        assert!(none.is_empty());
    }
}
