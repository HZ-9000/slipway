//! Asking Nix about the user's own system: whether a name is a package it
//! has, whether the configuration still reads, and whether it would build.
//!
//! Every question goes to the same configuration the next rebuild will use,
//! so Slipway sees the packages the user would really get and reports the
//! problems they would really hit.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};

use crate::{
    nix::cli,
    package::{Holds, PackageName, PackageSet},
    system::{ConfigKind, System},
};

/// How Slipway names the user's system configuration to Nix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A flake: everything hangs off `<dir>#nixosConfigurations.<machine>`,
    /// asked for with Nix's new command line.
    Flake { dir: PathBuf, machine: String },
    /// Channels: NixOS is built from `<nixpkgs/nixos>` with `nixos-config`
    /// pointed at the configuration file. Asked for with Nix's older
    /// commands, which are all a machine without `nix-command` has.
    Channels { entry: PathBuf },
}

impl Target {
    /// Works out how to ask Nix about `system`. For a flake this costs one
    /// question of its own, to find out which machine in it is this one.
    pub fn for_system(system: &System) -> Result<Self> {
        match system.config.kind {
            ConfigKind::ConfigurationNix => Ok(Self::Channels {
                entry: system.config.entry.clone(),
            }),
            ConfigKind::Flake => {
                let dir = system.config.dir.clone();
                let machine = machine_in(&dir)?;
                Ok(Self::Flake { dir, machine })
            }
        }
    }

    /// Checks that Nix can read the whole configuration, building nothing.
    pub fn evaluate(&self) -> Result<()> {
        match self {
            Self::Flake { dir, machine } => cli::verbatim(
                "nix",
                &["eval", "--raw", &toplevel(dir, machine, ".drvPath")],
            )
            .map(drop),
            Self::Channels { entry } => {
                let config = nixos_config(entry);
                cli::verbatim(
                    "nix-instantiate",
                    &["-I", &config, "<nixpkgs/nixos>", "-A", "system"],
                )
                .map(drop)
            }
        }
    }

    /// Works out everything the system would build, without building any of
    /// it, so a configuration that reads but can't be built is caught too.
    pub fn dry_build(&self) -> Result<()> {
        match self {
            Self::Flake { dir, machine } => cli::verbatim(
                "nix",
                &[
                    "build",
                    "--dry-run",
                    "--no-link",
                    &toplevel(dir, machine, ""),
                ],
            )
            .map(drop),
            Self::Channels { entry } => {
                let config = nixos_config(entry);
                cli::verbatim(
                    "nix-build",
                    &[
                        "--dry-run",
                        "--no-out-link",
                        "-I",
                        &config,
                        "<nixpkgs/nixos>",
                        "-A",
                        "system",
                    ],
                )
                .map(drop)
            }
        }
    }
}

impl PackageSet for Target {
    fn holds(&self, package: &PackageName) -> Result<Holds> {
        let question = looks_up(package);
        let answer = match self {
            Self::Flake { dir, machine } => cli::nix(&[
                "eval",
                "--json",
                &format!("{}#nixosConfigurations.{machine}.pkgs", dir.display()),
                "--apply",
                &question,
            ])?,
            // A channels system has no flake to ask, so this is the package
            // set `<nixpkgs>` on its search path gives, which is the channel
            // it was built from.
            Self::Channels { .. } => cli::older(
                "nix-instantiate",
                &[
                    "--eval",
                    "--json",
                    "--expr",
                    &format!("({question}) (import <nixpkgs> {{ }})"),
                ],
            )?,
        };

        match serde_json::from_str::<String>(&answer)
            .with_context(|| format!("Nix answered `{answer}`, which Slipway didn't expect"))?
            .as_str()
        {
            "package" => Ok(Holds::Package),
            "something else" => Ok(Holds::SomethingElse),
            "nothing" => Ok(Holds::Nothing),
            other => bail!("Nix answered `{other}`, which Slipway didn't expect"),
        }
    }
}

/// The flake attribute that is the whole built system, plus `suffix`.
fn toplevel(dir: &Path, machine: &str, suffix: &str) -> String {
    format!(
        "{}#nixosConfigurations.{machine}.config.system.build.toplevel{suffix}",
        dir.display()
    )
}

/// How Nix's older commands are told which configuration to build.
fn nixos_config(entry: &Path) -> String {
    format!("nixos-config={}", entry.display())
}

/// The machine in the flake at `dir` that describes this computer: the one
/// named after it, or the only one there is.
fn machine_in(dir: &Path) -> Result<String> {
    let json = cli::nix(&[
        "eval",
        "--json",
        &format!("{}#nixosConfigurations", dir.display()),
        "--apply",
        "builtins.attrNames",
    ])
    .with_context(|| {
        format!(
            "couldn't read the list of computers in {}",
            dir.join("flake.nix").display()
        )
    })?;
    let machines: Vec<String> = serde_json::from_str(&json)
        .with_context(|| format!("`nix eval` answered `{json}`, which isn't a list of names"))?;

    let this = hostname();
    if let Some(this) = &this
        && machines.iter().any(|machine| machine == this)
    {
        return Ok(this.clone());
    }

    match machines.as_slice() {
        [] => bail!(
            "{} doesn't describe any computer, so there's nothing for Slipway to check",
            dir.join("flake.nix").display()
        ),
        [only] => Ok(only.clone()),
        several => bail!(
            "{} describes several computers ({}), and none of them is named `{}` like this one, so Slipway can't tell which is this computer",
            dir.join("flake.nix").display(),
            several.join(", "),
            this.unwrap_or_else(|| "unknown".to_owned())
        ),
    }
}

/// This computer's name, as NixOS writes it.
fn hostname() -> Option<String> {
    let name = fs::read_to_string("/etc/hostname").ok()?.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// A Nix function that says what a package set holds at `package`:
/// `"package"`, `"something else"`, or `"nothing"`.
///
/// It never looks at the value it finds the whole way down, so a package
/// Nix refuses to work out — an unfree one, say — still answers `"package"`
/// rather than failing the question.
fn looks_up(package: &PackageName) -> String {
    // A PackageName is letters, numbers, `-`, `_` and `.`, so its parts
    // need no escaping as Nix strings.
    let path = package
        .as_str()
        .split('.')
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        r#"packages:
let
  found = builtins.foldl' (
    step: name:
    if step.found && builtins.isAttrs step.value && builtins.hasAttr name step.value then
      {{
        found = true;
        value = step.value.${{name}};
      }}
    else
      {{
        found = false;
        value = null;
      }}
  ) {{ found = true; value = packages; }} [ {path} ];
  kind = builtins.tryEval (
    if builtins.isAttrs found.value && (found.value.type or null) == "derivation" then
      "package"
    else
      "something else"
  );
in
if !found.found then
  "nothing"
else if kind.success then
  kind.value
else
  "package"
"#
    )
}
