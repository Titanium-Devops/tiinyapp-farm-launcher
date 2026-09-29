//! A farm account and one app's conversation.
//!
//! The bearer token stops in this module. The page can paste one once, but no
//! command ever answers with it and no request or error is formatted with its
//! headers. Every farm request is made here in Rust.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::{HeaderValue, AUTHORIZATION, RETRY_AFTER};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::badges::{self, Stack};

const LIVE_SITE: &str = "https://tiinyapp.farm";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub configured: bool,
    pub label: Option<String>,
}

impl Account {
    fn signed_out() -> Account {
        Account {
            configured: false,
            label: None,
        }
    }

    fn saved() -> Account {
        Account {
            configured: true,
            // No bearer-authenticated farm route returns the account handle.
            label: Some("Token saved".into()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub handle: Option<String>,
    pub name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: Author,
    pub text: String,
    pub at: String,
    #[serde(default)]
    pub can_delete: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct FarmView {
    #[serde(default)]
    seeds: Option<u64>,
    #[serde(default)]
    thumbs: Option<u64>,
    #[serde(default)]
    mine: bool,
    #[serde(default)]
    comments: Vec<Comment>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub seeds: u64,
    pub mine: bool,
    pub comments: Vec<Comment>,
    pub stack: Stack,
}

impl From<FarmView> for View {
    fn from(value: FarmView) -> Self {
        let seeds = value.seeds.or(value.thumbs).unwrap_or(0);
        View {
            seeds,
            mine: value.mine,
            comments: value.comments,
            stack: badges::stack(seeds),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    /// `ok`, `signedOut`, `unverified`, `rateLimited`, or `error`.
    pub state: String,
    pub message: Option<String>,
    pub social: Option<View>,
}

impl Reply {
    fn ok(view: View) -> Reply {
        Reply {
            state: "ok".into(),
            message: None,
            social: Some(view),
        }
    }

    fn problem(state: &str, message: impl Into<String>) -> Reply {
        Reply {
            state: state.into(),
            message: Some(message.into()),
            social: None,
        }
    }
}

pub fn token_path(config_dir: &Path) -> PathBuf {
    config_dir.join("farm-token")
}

fn valid_token(token: &str) -> bool {
    token.len() == 45
        && token.starts_with("farm_")
        && token[5..]
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn read_token(config_dir: &Path) -> Option<String> {
    let token = std::fs::read_to_string(token_path(config_dir)).ok()?;
    let token = token.trim();
    valid_token(token).then(|| token.to_string())
}

fn save_token(config_dir: &Path, token: &str) -> Result<(), String> {
    std::fs::create_dir_all(config_dir)
        .map_err(|error| format!("The settings folder could not be made: {error}."))?;
    let path = token_path(config_dir);
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| format!("The farm token could not be saved: {error}."))?;
    file.write_all(token.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| format!("The farm token could not be saved: {error}."))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("The farm token permissions could not be set: {error}."))?;
    }
    Ok(())
}

pub fn account(config_dir: &Path) -> Account {
    if read_token(config_dir).is_some() {
        Account::saved()
    } else {
        Account::signed_out()
    }
}

pub fn sign_out(config_dir: &Path) -> Result<Account, String> {
    match std::fs::remove_file(token_path(config_dir)) {
        Ok(()) => Ok(Account::signed_out()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Account::signed_out()),
        Err(error) => Err(format!("The farm token could not be removed: {error}.")),
    }
}

fn site() -> String {
    std::env::var("FARM_LAUNCHER_SITE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| LIVE_SITE.into())
        .trim_end_matches('/')
        .to_string()
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!(
            "tiinyapp-farm-launcher/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| format!("The launcher could not open a connection: {error}."))
}

fn bearer(token: &str) -> Result<HeaderValue, String> {
    let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| "That farm token is not readable.".to_string())?;
    value.set_sensitive(true);
    Ok(value)
}

fn app_id(id: &str) -> bool {
    id.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn farm_message(body: &str, token: Option<&str>) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
        .map(|message| match token {
            Some(token) => message.replace(token, "[farm token]"),
            None => message,
        })
}

async fn answer(
    config_dir: &Path,
    response: reqwest::Response,
    token: Option<&str>,
) -> Result<Reply, String> {
    let status = response.status();
    let retry = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .text()
        .await
        .map_err(|_| "The farm answered, but its answer could not be read.".to_string())?;
    if status.is_success() {
        let view = serde_json::from_str::<FarmView>(&body)
            .map_err(|_| "The farm answered in a shape this launcher cannot read.".to_string())?;
        return Ok(Reply::ok(view.into()));
    }
    match status.as_u16() {
        401 => {
            let _ = sign_out(config_dir);
            Ok(Reply::problem(
                "signedOut",
                "Sign in with a farm token in Settings to give seeds and comment.",
            ))
        }
        403 => Ok(Reply::problem(
            "unverified",
            "Comments need a verified Tiiny on your farm account.",
        )),
        429 => {
            let when = retry
                .map(|value| match value.parse::<u64>() {
                    Ok(seconds) if seconds >= 120 => {
                        format!("Try again in {} minutes.", seconds.div_ceil(60))
                    }
                    Ok(seconds) => format!("Try again in {seconds} seconds."),
                    Err(_) => format!("Try again after {value}."),
                })
                .unwrap_or_else(|| "Try again in one hour.".into());
            Ok(Reply::problem("rateLimited", when))
        }
        _ => Ok(Reply::problem(
            "error",
            farm_message(&body, token).unwrap_or_else(|| format!("The farm answered {status}.")),
        )),
    }
}

async fn send(
    config_dir: &Path,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> Result<Reply, String> {
    let mut request = client()?.request(method, format!("{}{path}", site()));
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, bearer(token)?);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "The farm could not be reached. Check this computer's network.".to_string())?;
    answer(config_dir, response, token).await
}

pub async fn save(config_dir: &Path, token: String) -> Result<Account, String> {
    let token = token.trim().to_string();
    if !valid_token(&token) {
        return Err("Paste the farm_ token from your farm account.".into());
    }
    // This bearer route is also the farm's account validation route. Its body
    // deliberately has no account identity, so the UI uses the fallback label.
    let response = client()?
        .get(format!("{}/api/seeds/mine", site()))
        .header(AUTHORIZATION, bearer(&token)?)
        .send()
        .await
        .map_err(|_| "The farm could not be reached. Check this computer's network.".to_string())?;
    if response.status().as_u16() == 401 {
        return Err("That farm token was not accepted.".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "The farm could not check that token ({}).",
            response.status().as_u16()
        ));
    }
    save_token(config_dir, &token)?;
    Ok(Account::saved())
}

pub async fn detail(config_dir: &Path, id: &str) -> Result<Reply, String> {
    if !app_id(id) {
        return Err(format!("{id} is not an app id."));
    }
    let token = read_token(config_dir);
    send(
        config_dir,
        reqwest::Method::GET,
        &format!("/api/seeds/{id}/social"),
        None,
        token.as_deref(),
    )
    .await
}

pub async fn toggle(config_dir: &Path, id: &str) -> Result<Reply, String> {
    let Some(token) = read_token(config_dir) else {
        return Ok(Reply::problem(
            "signedOut",
            "Sign in with a farm token in Settings to give seeds and comment.",
        ));
    };
    if !app_id(id) {
        return Err(format!("{id} is not an app id."));
    }
    send(
        config_dir,
        reqwest::Method::POST,
        &format!("/api/seeds/{id}/seed"),
        None,
        Some(&token),
    )
    .await
}

pub async fn comment(config_dir: &Path, id: &str, text: String) -> Result<Reply, String> {
    let Some(token) = read_token(config_dir) else {
        return Ok(Reply::problem(
            "signedOut",
            "Sign in with a farm token in Settings to give seeds and comment.",
        ));
    };
    if !app_id(id) {
        return Err(format!("{id} is not an app id."));
    }
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 1000 {
        return Ok(Reply::problem(
            "error",
            "Write a comment of 1 to 1000 characters.",
        ));
    }
    send(
        config_dir,
        reqwest::Method::POST,
        &format!("/api/seeds/{id}/comments"),
        Some(serde_json::json!({ "text": text })),
        Some(&token),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    static SITE_LOCK: Mutex<()> = Mutex::new(());

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "farm-launcher-social-{name}-{}",
            std::process::id()
        ))
    }

    fn serve(status: u16, body: &'static str, seen: Arc<Mutex<String>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 8192];
            let read = stream.read(&mut bytes).unwrap();
            *seen.lock().unwrap() = String::from_utf8_lossy(&bytes[..read]).into_owned();
            let reason = if status == 200 { "OK" } else { "Error" };
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        format!("http://{address}")
    }

    #[test]
    fn token_is_mode_600_and_account_never_echoes_it() {
        let dir = temp("mode");
        std::fs::remove_dir_all(&dir).ok();
        let token = format!("farm_{}", "a".repeat(40));
        save_token(&dir, &token).unwrap();
        assert_eq!(account(&dir), Account::saved());
        assert!(!serde_json::to_string(&account(&dir))
            .unwrap()
            .contains(&token));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(token_path(&dir))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn social_seed_and_comment_calls_send_the_bearer_without_returning_it() {
        let _site = SITE_LOCK.lock().unwrap();
        let dir = temp("bearer");
        std::fs::remove_dir_all(&dir).ok();
        let token = format!("farm_{}", "b".repeat(40));
        save_token(&dir, &token).unwrap();
        for call in ["social", "seed", "comment"] {
            let seen = Arc::new(Mutex::new(String::new()));
            let body = r#"{"seeds":2,"thumbs":2,"mine":true,"comments":[]}"#;
            let root = serve(200, body, seen.clone());
            std::env::set_var("FARM_LAUNCHER_SITE", root);
            let reply = match call {
                "social" => tauri::async_runtime::block_on(detail(&dir, "story-lantern")).unwrap(),
                "seed" => tauri::async_runtime::block_on(toggle(&dir, "story-lantern")).unwrap(),
                _ => {
                    tauri::async_runtime::block_on(comment(&dir, "story-lantern", "Lovely".into()))
                        .unwrap()
                }
            };
            let request = seen.lock().unwrap().clone();
            assert!(
                request.contains(&format!("authorization: Bearer {token}"))
                    || request.contains(&format!("Authorization: Bearer {token}"))
            );
            assert!(!serde_json::to_string(&reply).unwrap().contains(&token));
        }
        std::env::remove_var("FARM_LAUNCHER_SITE");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_401_clears_signed_in() {
        let _site = SITE_LOCK.lock().unwrap();
        let dir = temp("unauthorized");
        std::fs::remove_dir_all(&dir).ok();
        save_token(&dir, &format!("farm_{}", "c".repeat(40))).unwrap();
        let seen = Arc::new(Mutex::new(String::new()));
        std::env::set_var(
            "FARM_LAUNCHER_SITE",
            serve(401, r#"{"error":"Sign in first."}"#, seen),
        );
        let reply = tauri::async_runtime::block_on(detail(&dir, "story-lantern")).unwrap();
        assert_eq!(reply.state, "signedOut");
        assert_eq!(account(&dir), Account::signed_out());
        std::env::remove_var("FARM_LAUNCHER_SITE");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn even_a_bad_server_cannot_echo_the_token_to_the_page() {
        let _site = SITE_LOCK.lock().unwrap();
        let dir = temp("redaction");
        std::fs::remove_dir_all(&dir).ok();
        let token = format!("farm_{}", "d".repeat(40));
        save_token(&dir, &token).unwrap();
        let seen = Arc::new(Mutex::new(String::new()));
        let leaked = Box::leak(format!(r#"{{"error":"refused {token}"}}"#).into_boxed_str());
        std::env::set_var("FARM_LAUNCHER_SITE", serve(500, leaked, seen));
        let reply = tauri::async_runtime::block_on(toggle(&dir, "story-lantern")).unwrap();
        assert!(!serde_json::to_string(&reply).unwrap().contains(&token));
        assert!(reply.message.unwrap().contains("[farm token]"));
        std::env::remove_var("FARM_LAUNCHER_SITE");
        std::fs::remove_dir_all(dir).ok();
    }
}
