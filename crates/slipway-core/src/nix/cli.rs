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
    run(&["--version"])
}

/// The version of the Nix store this machine talks to, which is the daemon's
/// when a daemon is in use.
///
/// Needs Nix's `nix-command` feature: `nix store info` belongs to the new
/// command line, which Nix still gates behind it.
pub fn store_version() -> Result<String> {
    let json = run(&["store", "info", "--json"])?;
    let info: serde_json::Value =
        serde_json::from_str(&json).context("`nix store info --json` printed invalid JSON")?;
    info["version"]
        .as_str()
        .map(str::to_owned)
        .context("`nix store info --json` didn't report a version")
}

/// Runs `nix` with `args` and hands back its trimmed standard output.
fn run(args: &[&str]) -> Result<String> {
    let shown = || format!("nix {}", args.join(" "));

    let output = Command::new("nix")
        .args(args)
        .output()
        .with_context(|| format!("running `{}` (is nix on PATH?)", shown()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .map(str::trim)
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
        bail!("`{}` failed: {reason}", shown());
    }

    Ok(String::from_utf8(output.stdout)
        .with_context(|| format!("`{}` printed invalid UTF-8", shown()))?
        .trim()
        .to_owned())
}
