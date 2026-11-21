//! What Slipway asks the `nix` command on `PATH`.
//!
//! Slipway drives Nix through its command line rather than linking its C
//! API, so one Slipway binary works with whatever Nix a machine happens to
//! have. Linking the C API would pin Slipway to a single Nix patch release:
//! those libraries carry the full version in their SONAME
//! (`libnixstorec.so.2.35.6`) and wrap C++ with no stable ABI.

use std::process::Command;

use anyhow::{Context as _, Result, bail};

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

/// Runs `nix` with `args` and hands back its trimmed standard output.
pub fn nix(args: &[&str]) -> Result<String> {
    run("nix", args, OnFailure::Summarise)
}

/// The same for one of Nix's older commands, such as `nix-instantiate`.
/// Those are all a machine without the new command line turned on has.
pub fn older(command: &str, args: &[&str]) -> Result<String> {
    run(command, args, OnFailure::Summarise)
}

/// The same for a command that reads the user's whole configuration, where
/// a failure is something about that configuration rather than about
/// Slipway's question.
pub fn verbatim(command: &str, args: &[&str]) -> Result<String> {
    run(command, args, OnFailure::PassOn)
}

/// How much of what a failed command said to hand back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnFailure {
    /// The line that says what went wrong, which is all a short question
    /// needs.
    Summarise,
    /// Everything it said: when Nix has read a whole configuration, its own
    /// explanation — which file, which line, which option — is the only one
    /// worth having.
    PassOn,
}

/// Runs `command` with `args` and hands back its trimmed standard output.
fn run(command: &str, args: &[&str], on_failure: OnFailure) -> Result<String> {
    let shown = || format!("{command} {}", args.join(" "));

    let output = Command::new(command)
        .args(args)
        .output()
        .with_context(|| format!("running `{}` (is {command} on PATH?)", shown()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Nix puts `error:` on a line of its own before a long explanation,
        // so the first line that says anything is the first one past it.
        let reason = stderr
            .lines()
            .map(|line| line.trim().trim_start_matches("error:").trim())
            .find(|line| !line.is_empty())
            .unwrap_or("it printed no explanation");

        // The new command line is itself experimental, so this is what a
        // machine that hasn't turned `nix-command` on says to everything.
        if reason.contains("nix-command") && reason.contains("disabled") {
            bail!(
                "`{}` needs Nix's `nix-command` feature, which isn't turned on",
                shown()
            );
        }

        match on_failure {
            OnFailure::Summarise => bail!("`{}` failed: {reason}", shown()),
            OnFailure::PassOn => bail!(
                "Nix couldn't do it. This is what `{}` said:\n{}",
                shown(),
                stderr.trim_end()
            ),
        }
    }

    Ok(String::from_utf8(output.stdout)
        .with_context(|| format!("`{}` printed invalid UTF-8", shown()))?
        .trim()
        .to_owned())
}
