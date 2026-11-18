use std::process::Command;

use anyhow::{Context as _, Result, bail};

pub fn nix_cli_version() -> Result<String> {
    let output = Command::new("nix")
        .arg("--version")
        .output()
        .context("running `nix --version` (is nix on PATH?)")?;

    if !output.status.success() {
        bail!(
            "`nix --version` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8(output.stdout)
        .context("`nix --version` emitted invalid UTF-8")?
        .trim()
        .to_owned())
}
