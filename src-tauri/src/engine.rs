//! The engine: the farm command line tool, carried inside the app.
//!
//! Every action the launcher takes is the same action the CLI takes, because
//! it is the CLI. The shell runs `<bundled python> -m farm.farm <command>
//! --json` as a child process, reads the one JSON object off stdout and the
//! running commentary off stderr, and draws it. One install directory, one
//! device file, one lock, one set of failure messages, whichever face somebody
//! came in through.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

/// Where the engine lives and what it is allowed to touch.
#[derive(Debug, Clone)]
pub struct Engine {
    /// The interpreter inside the bundle.
    pub python: PathBuf,
    /// The staging stamp written by scripts/stage-runtime.mjs.
    pub stamp: Stamp,
    /// The home the engine reads and writes under. `None` means the person's
    /// own home, which is the whole point: the launcher and the CLI share one
    /// set of apps. A value here is for a test run against a scratch home.
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub python: String,
    #[serde(default)]
    pub farm: String,
    #[serde(default)]
    pub farm_from: Option<String>,
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub bytes: u64,
}

/// What went wrong, in the words a person reads.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineError {
    pub message: String,
    /// The engine's commentary up to the point it stopped, newest last. This is
    /// what turns "it failed" into a sentence somebody can act on.
    pub commentary: Vec<String>,
    /// What the window should offer next, read off the message. Computed here
    /// so the page never has to guess at a failure it was handed.
    pub trouble: crate::trouble::Trouble,
}

impl EngineError {
    pub fn plain(message: impl Into<String>) -> Self {
        Self::new(message, Vec::new())
    }

    pub fn new(message: impl Into<String>, commentary: Vec<String>) -> Self {
        let message = message.into();
        let trouble = crate::trouble::classify(&message);
        Self {
            message,
            commentary,
            trouble,
        }
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EngineError {}

/// One finished run of the engine.
pub struct Run {
    pub value: Value,
    pub commentary: Vec<String>,
}

/// How long a command is given before it is killed. An install pulls an
/// archive the manifest allows to be 512 MiB, so it gets the long one; nothing
/// else should ever take a minute.
#[derive(Debug, Clone, Copy)]
pub struct Budget(pub Duration);

impl Budget {
    pub const QUICK: Budget = Budget(Duration::from_secs(60));
    pub const PATIENT: Budget = Budget(Duration::from_secs(180));
    pub const DOWNLOAD: Budget = Budget(Duration::from_secs(900));
}

impl Engine {
    /// Find the staged runtime under the app's resource directory.
    pub fn locate(resources: &Path) -> Result<Self, EngineError> {
        let runtime = resources.join("runtime");
        let python = if cfg!(windows) {
            runtime.join("python.exe")
        } else {
            runtime.join("bin").join("python3.11")
        };
        if !python.exists() {
            return Err(EngineError::plain(format!(
                "The Python this app carries is missing from {}. The download is damaged; install the launcher again.",
                runtime.display()
            )));
        }
        let stamp = std::fs::read_to_string(runtime.join("runtime.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<Stamp>(&raw).ok())
            .unwrap_or_default();
        Ok(Self {
            python,
            stamp,
            home: None,
        })
    }

    /// The directory the engine keeps installed apps in.
    pub fn apps_dir(&self) -> PathBuf {
        match &self.home {
            Some(home) => home.join("tiinyapps"),
            None => home_dir().join("tiinyapps"),
        }
    }

    /// The directory the engine keeps the device file and the token in.
    pub fn config_dir(&self) -> PathBuf {
        match &self.home {
            Some(home) => home.join(".tiinyapps"),
            None => home_dir().join(".tiinyapps"),
        }
    }

    /// The launcher owns one interpreter, and it is the one inside the bundle.
    ///
    /// The CLI moves off a Python that macOS refused the local network, and
    /// saves the one it moved to. That is right for somebody at a terminal and
    /// wrong here: local network access is granted per application, so every
    /// Python the launcher starts is attributed to the launcher, and one grant
    /// covers finding the Tiiny, the doctor and every app. A second interpreter
    /// would need a second grant nobody will ever be asked for, and an app
    /// started under it would fail to reach the Tiiny for a reason the person
    /// cannot see.
    ///
    /// So before and after every engine call the saved setting is put back to
    /// the bundled interpreter. Returns the path it displaced, when it
    /// displaced one.
    ///
    /// A path inside the bundle is stable while the app is installed, and
    /// harmless once it is not: `Farm.app_python` only takes a saved
    /// interpreter that is still runnable, and falls back to its own otherwise.
    pub fn pin_interpreter(&self) -> Option<String> {
        let ours = self.python.to_string_lossy().to_string();
        let file = self.config_dir().join("settings.json");
        let raw = std::fs::read_to_string(&file).unwrap_or_default();
        let (next, displaced) = pinned_settings(&raw, &ours)?;
        if std::fs::create_dir_all(self.config_dir()).is_err() {
            return None;
        }
        std::fs::write(&file, next).ok()?;
        displaced
    }

    /// Whether the saved interpreter is already the bundled one, which is what
    /// the report shows rather than a claim.
    pub fn interpreter_on_file(&self) -> Option<String> {
        let raw = std::fs::read_to_string(self.config_dir().join("settings.json")).ok()?;
        let value: Value = serde_json::from_str(&raw).ok()?;
        value
            .get("python")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.python);
        command.arg("-m").arg("farm.farm").arg("--no-update-check");
        command.args(args);
        // The bundle is read only once it is in /Applications, and a farm app
        // inherits this environment, so neither writes bytecode beside itself.
        command.env("PYTHONDONTWRITEBYTECODE", "1");
        // Whatever else is installed on this machine's Python is not ours.
        command.env("PYTHONNOUSERSITE", "1");
        command.env("PYTHONUNBUFFERED", "1");
        if let Some(home) = &self.home {
            command.env("HOME", home);
            command.env("USERPROFILE", home);
        }
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window when the launcher shells out.
            command.creation_flags(0x0800_0000);
        }
        command
    }

    /// Run a `--json` command and hand back the object it answered with.
    pub fn json(&self, args: &[&str], budget: Budget) -> Result<Value, EngineError> {
        self.json_watching(args, budget, |_| {})
            .map(|run| run.value)
    }

    /// Run a `--json` command, calling `on_line` with each line of the
    /// engine's commentary as it arrives.
    pub fn json_watching<F>(
        &self,
        args: &[&str],
        budget: Budget,
        on_line: F,
    ) -> Result<Run, EngineError>
    where
        F: FnMut(&str) + Send + 'static,
    {
        let mut with_json: Vec<&str> = args.to_vec();
        with_json.push("--json");
        let (stdout, commentary) = self.run(&with_json, budget, None, on_line)?;
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Err(EngineError::new(
                "The farm answered with nothing at all, which means it stopped before it could.",
                commentary,
            ));
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|_| {
            EngineError::new(
                "The farm answered with something that is not JSON.",
                commentary.clone(),
            )
        })?;
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("The farm refused, and did not say why.")
                .to_string();
            return Err(EngineError::new(message, commentary));
        }
        Ok(Run { value, commentary })
    }

    /// Run a command that has no `--json` mode. `farm remove` is the only one
    /// the launcher needs: it prints a sentence and sets an exit code, so that
    /// sentence is what the window shows.
    pub fn prose(&self, args: &[&str], budget: Budget) -> Result<String, EngineError> {
        let (stdout, commentary) = self.run(args, budget, None, |_| {})?;
        let said = stdout.trim();
        if !said.is_empty() {
            return Ok(said.to_string());
        }
        Ok(commentary.last().cloned().unwrap_or_default())
    }

    /// Run `farm device`, with the key going down the child's standard input
    /// and nowhere else. It is never an argument, never an environment
    /// variable, and never printed.
    pub fn device(&self, base: &str, key: &str) -> Result<(), EngineError> {
        let (_, commentary) = self.run(
            &["device", "--base", base, "--key-stdin"],
            Budget::PATIENT,
            Some(key),
            |_| {},
        )?;
        let _ = commentary;
        Ok(())
    }

    fn run<F>(
        &self,
        args: &[&str],
        budget: Budget,
        stdin_text: Option<&str>,
        mut on_line: F,
    ) -> Result<(String, Vec<String>), EngineError>
    where
        F: FnMut(&str) + Send + 'static,
    {
        // Before, so the engine starts from the bundled interpreter, and after,
        // because a command that met a refused local network will have moved
        // the setting somewhere else while it ran.
        self.pin_interpreter();
        let mut command = self.command(args);
        if stdin_text.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command.spawn().map_err(|error| {
            EngineError::plain(format!("The farm could not be started: {error}."))
        })?;

        if let Some(text) = stdin_text {
            let mut pipe = child.stdin.take().expect("stdin was asked for");
            // Written once and dropped. Nothing keeps a copy.
            let write = pipe.write_all(text.as_bytes()).and_then(|_| pipe.flush());
            drop(pipe);
            if let Err(error) = write {
                let _ = child.kill();
                return Err(EngineError::plain(format!(
                    "The key could not be handed to the farm: {error}."
                )));
            }
        }

        let out = child.stdout.take().expect("stdout is piped");
        let err = child.stderr.take().expect("stderr is piped");

        let (out_tx, out_rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut buffer = String::new();
            let mut reader = BufReader::new(out);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                buffer.push_str(&line);
                line.clear();
            }
            let _ = out_tx.send(buffer);
        });

        let (err_tx, err_rx) = mpsc::channel::<Vec<String>>();
        std::thread::spawn(move || {
            let mut lines = Vec::new();
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                on_line(&line);
                lines.push(line);
            }
            let _ = err_tx.send(lines);
        });

        let status = wait_with_budget(&mut child, budget);
        self.pin_interpreter();
        let status = status?;
        let stdout = out_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default();
        let commentary = err_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default();

        if !status && stdout.trim().is_empty() {
            let last = commentary
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "The farm stopped without saying why.".to_string());
            return Err(EngineError::new(last, commentary));
        }
        Ok((stdout, commentary))
    }
}

/// Put `python` in the settings text, keeping every other key.
///
/// Returns the text to write and the interpreter it displaced, or `None` when
/// the setting already says what it should and nothing needs writing. A
/// settings file that is not readable JSON is replaced rather than parsed
/// around: the farm treats it as absent too.
pub fn pinned_settings(raw: &str, python: &str) -> Option<(String, Option<String>)> {
    let mut held: serde_json::Map<String, Value> = serde_json::from_str(raw)
        .ok()
        .and_then(|value: Value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    let displaced = match held.get("python") {
        Some(Value::String(saved)) if saved == python => return None,
        Some(Value::String(saved)) => Some(saved.clone()),
        _ => None,
    };
    held.insert("python".to_string(), Value::String(python.to_string()));
    let text = serde_json::to_string_pretty(&Value::Object(held)).ok()? + "\n";
    Some((text, displaced))
}

/// Wait for the child, killing it when the budget runs out. Returns whether it
/// exited cleanly.
fn wait_with_budget(child: &mut Child, budget: Budget) -> Result<bool, EngineError> {
    let deadline = Instant::now() + budget.0;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {}
            Err(error) => {
                return Err(EngineError::plain(format!(
                    "The farm could not be waited for: {error}."
                )))
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EngineError::plain(format!(
                "The farm took longer than {} seconds and was stopped. Nothing was left half done: an install stages everything before it touches what is already there.",
                budget.0.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_runtime_says_what_to_do_rather_than_a_path_error() {
        let error = Engine::locate(Path::new("/nowhere/at/all")).unwrap_err();
        assert!(
            error.message.contains("install the launcher again"),
            "{}",
            error.message
        );
    }

    const OURS: &str = "/Applications/Tiiny App Farm.app/Contents/Resources/runtime/bin/python3.11";

    #[test]
    fn an_empty_settings_file_gains_the_bundled_interpreter() {
        let (text, displaced) = pinned_settings("", OURS).unwrap();
        assert_eq!(displaced, None);
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["python"], OURS);
    }

    #[test]
    fn a_python_the_farm_moved_to_is_put_back_and_named() {
        // What farm 0.1.11 saves when macOS refuses the local network: it walks
        // the other Pythons on the machine and keeps the first that gets
        // through. Inside the launcher that is the wrong answer, because the
        // grant belongs to the app.
        let raw = r#"{"python": "/opt/homebrew/bin/python3"}"#;
        let (text, displaced) = pinned_settings(raw, OURS).unwrap();
        assert_eq!(displaced.as_deref(), Some("/opt/homebrew/bin/python3"));
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["python"], OURS);
    }

    #[test]
    fn a_setting_that_is_already_ours_is_not_rewritten() {
        let raw = format!("{{\"python\": \"{OURS}\"}}");
        assert!(pinned_settings(&raw, OURS).is_none());
    }

    #[test]
    fn everything_else_in_the_settings_file_survives() {
        let raw = r#"{"python": "/usr/bin/python3", "somethingElse": 7, "kept": ["a"]}"#;
        let (text, _) = pinned_settings(raw, OURS).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["python"], OURS);
        assert_eq!(back["somethingElse"], 7);
        assert_eq!(back["kept"][0], "a");
    }

    #[test]
    fn a_settings_file_that_is_not_json_is_replaced_rather_than_parsed_around() {
        let (text, displaced) = pinned_settings("this is not json", OURS).unwrap();
        assert_eq!(displaced, None);
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["python"], OURS);
    }

    #[test]
    fn the_budgets_are_in_the_order_the_work_takes() {
        assert!(Budget::QUICK.0 < Budget::PATIENT.0);
        assert!(Budget::PATIENT.0 < Budget::DOWNLOAD.0);
    }

    #[test]
    fn an_error_carries_the_commentary_that_led_to_it() {
        let error = EngineError::new(
            "Checksum mismatch; archive was not unpacked or run.",
            vec!["Downloading 3.7 MB from github.com.".into()],
        );
        assert_eq!(
            error.to_string(),
            "Checksum mismatch; archive was not unpacked or run."
        );
        assert_eq!(error.commentary.len(), 1);
        // The window is handed what to offer next along with the sentence.
        assert_eq!(error.trouble.head, "The download did not match the catalog");
    }
}
