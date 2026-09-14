//! The two things the launcher reads over the network itself: the catalog, and
//! whether a Tiiny is where somebody said it is.
//!
//! Both are read from Rust rather than from the page. A web view asking for
//! them would be a second HTTP client with a different idea of timeouts, and
//! the device probe has to accept a 401 as a yes, which is easier to say here.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

pub const CATALOG: &str = "https://tiinyapp.farm/manifests/";
pub const SITE: &str = "https://tiinyapp.farm";

/// The addresses a Tiiny answers on, in the order the install page documents
/// them. The first is what the TiinyOS client publishes on a Mac.
pub const WELL_KNOWN_BASE: &str = "http://openai.api.tiiny/v1";

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
}
