//! What Slipway asks the `nix` command on `PATH`.
//!
//! Slipway drives Nix through its command line rather than linking its C
//! API, so one Slipway binary works with whatever Nix a machine happens to
//! have. Linking the C API would pin Slipway to a single Nix patch release:
//! those libraries carry the full version in their SONAME
//! (`libnixstorec.so.2.35.6`) and wrap C++ with no stable ABI.
//!
//! [`try_run`] keeps everything a command said and says what it can about
//! why a command failed, so a front-end can explain it. The helpers above
//! it boil that down to one sentence, which is all a short question needs.

use std::{fmt, process::Command};

use anyhow::{Context as _, Result, anyhow};

/// The version of the `nix` command on `PATH`, as it prints it.
pub fn nix_cli_version() -> Result<String> {
    nix(&["--version"])
}

/// The version of the Nix store this machine talks to, which is the daemon's
/// when a daemon is in use.
///
/// Needs Nix's `nix-command` feature: `nix store info` belongs to the new
/// command line, which Nix still gates behind it.
pub fn store_version() -> Result<String> {
    let json = nix(&["store", "info", "--json"])?;
    let info: serde_json::Value =
        serde_json::from_str(&json).context("`nix store info --json` printed invalid JSON")?;
    info["version"]
        .as_str()
        .map(str::to_owned)
        .context("`nix store info --json` didn't report a version")
}

/// Runs `nix` with `args` and hands back its trimmed standard output,
/// summing a failure up in one sentence.
pub fn nix(args: &[&str]) -> Result<String> {
    summarised("nix", args)
}

/// The same for one of Nix's older commands, such as `nix-instantiate`.
/// Those are all a machine without the new command line turned on has.
pub fn older(command: &str, args: &[&str]) -> Result<String> {
    summarised(command, args)
}

fn summarised(command: &str, args: &[&str]) -> Result<String> {
    match try_run(command, args) {
        Ok(output) => Ok(output.stdout),
        Err(failed) => Err(match &failed.why {
            Why::CantRun => anyhow!(
                "running `{}` (is {command} on PATH?): {}",
                failed.command,
                failed.output
            ),
            Why::FeatureOff { feature } => anyhow!(
                "`{}` needs Nix's `{feature}` feature, which isn't turned on",
                failed.command
            ),
            Why::Refused => anyhow!("`{}` failed: {}", failed.command, failed.cause()),
        }),
    }
}

/// What a Nix command said when it worked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The command, as it would be typed.
    pub command: String,
    /// Its standard output, trimmed: the answer to the question.
    pub stdout: String,
    /// Everything it said as it went, trimmed. Nix reports what it is doing
    /// and what it would build here, not only what went wrong.
    pub stderr: String,
}

/// Why a Nix command didn't work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The command, as it would be typed, so it can be shown to the user or
    /// run by hand.
    pub command: String,
    /// Everything Nix said, as it said it: which file, which line, which
    /// option. A front-end can offer to show this.
    pub output: String,
    pub why: Why,
}

impl Failed {
    /// The line of Nix's output that says what actually went wrong.
    #[must_use]
    pub fn cause(&self) -> &str {
        super::explain::cause(&self.output)
    }
}

/// As much as Slipway can tell about why a command failed without reading
/// Nix's message closely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// Nix couldn't be started at all: it isn't on `PATH`, or the machine
    /// wouldn't run it.
    CantRun,
    /// The command needs one of Nix's experimental features, and this
    /// machine hasn't turned it on.
    FeatureOff { feature: String },
    /// Nix ran, read what it was given, and said no.
    Refused,
}

impl Why {
    /// What Nix's own output says about why it refused.
    #[must_use]
    pub fn from_output(stderr: &str) -> Self {
        // `error: experimental Nix feature 'flakes' is disabled; add
        // '--extra-experimental-features flakes' to enable it`
        let named = stderr
            .lines()
            .find(|line| line.contains("feature") && line.contains("is disabled"))
            .and_then(|line| line.split_once('\''))
            .and_then(|(_, rest)| rest.split_once('\''))
            .map(|(feature, _)| feature.to_owned());

        match named {
            Some(feature) => Self::FeatureOff { feature },
            None => Self::Refused,
        }
    }
}

impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CantRun => f.write_str("Nix couldn't be run"),
            Self::FeatureOff { feature } => {
                write!(f, "Nix's `{feature}` feature isn't turned on")
            }
            Self::Refused => f.write_str("Nix said no"),
        }
    }
}

/// Runs `command` with `args`, keeping everything it said either way.
pub fn try_run(command: &str, args: &[&str]) -> Result<Output, Failed> {
    let shown = format!("{command} {}", args.join(" "));

    let output = match Command::new(command).args(args).output() {
        Ok(output) => output,
        Err(err) => {
            return Err(Failed {
                command: shown,
                output: err.to_string(),
                why: Why::CantRun,
            });
        }
    };

    // Nix's messages are UTF-8, and a stray byte in one is no reason to
    // drop what it was saying.
    let stderr = String::from_utf8_lossy(&output.stderr)
        .trim_end()
        .to_owned();
    if !output.status.success() {
        return Err(Failed {
            command: shown,
            why: Why::from_output(&stderr),
            output: stderr,
        });
    }

    Ok(Output {
        command: shown,
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr,
    })
}
