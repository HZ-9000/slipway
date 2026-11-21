//! Asking Nix about the user's own system: whether a name is a package it
//! has, whether the configuration still reads, and whether it would build.
//!
//! Every question goes to the same configuration the next rebuild will use,
//! so Slipway sees the packages the user would really get and reports the
//! problems they would really hit.
//!
//! Both answers are made of parts rather than a line of text: [`Failure`]
//! says which question was being asked, what was run, what Nix said and as
//! much as Slipway can tell about why, and [`Planned`] says what a dry
//! build would build. A front-end can explain either without parsing a
//! message.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};

use crate::{
    nix::{
        cli::{self, Failed, Why},
        explain::{self, Explained},
    },
    package::{Holds, PackageName, PackageSet},
    system::{ConfigKind, System},
};

/// What Slipway was asking Nix for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Reading the whole system configuration, building nothing.
    Read,
    /// Working out everything the system would build, building none of it.
    DryBuild,
    /// Looking one package name up in the configuration's package set.
    LookUp,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "reading your system setup",
            Self::DryBuild => "working out what your system would build",
            Self::LookUp => "looking a package up",
        })
    }
}

/// Why Nix couldn't answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// What was being asked for when it went wrong.
    pub step: Step,
    /// As much as Slipway can tell about why.
    pub why: Why,
    /// The command Slipway ran, when it got as far as running one.
    pub command: Option<String>,
    /// Everything Nix said, as it said it. A front-end can offer to show
    /// this; turning it into advice needs more than Slipway knows here.
    pub output: String,
}

impl Failure {
    fn from(step: Step, failed: Failed) -> Self {
        Self {
            step,
            why: failed.why,
            command: Some(failed.command),
            output: failed.output,
        }
    }

    /// What went wrong in plain language: what it affects and what to try
    /// next, with Nix's own words left on [`Failure::output`] for a
    /// front-end that offers to show them.
    #[must_use]
    pub fn explain(&self) -> Explained {
        explain::of(self)
    }

    /// A machine that hasn't turned `feature` on, said without running
    /// anything: Slipway reads Nix's own settings, so it needn't ask.
    fn feature_off(step: Step, feature: &str) -> Self {
        Self {
            step,
            why: Why::FeatureOff {
                feature: feature.to_owned(),
            },
            command: None,
            output: String::new(),
        }
    }
}

impl fmt::Display for Failure {
    /// The one line of it that matters. [`Failure::explain`] says what to
    /// do about it, and `output` is everything Nix said.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.explain().fmt(f)
    }
}

impl std::error::Error for Failure {}

/// What came of reading the configuration: Nix worked out the whole system
/// and said what it would come to, without building any of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluated {
    /// The store path of the recipe for this system, as Nix named it.
    pub drv_path: String,
}

/// What came of a dry build. Nix says this in words and says nothing at all
/// when there is nothing to do, so each number is here only when it said so.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Planned {
    /// How many things Nix would build on this computer.
    pub to_build: Option<usize>,
    /// How many it would download rather than build.
    pub to_fetch: Option<usize>,
    /// How much it would download, in Nix's own words, such as `12.30 MiB`.
    pub download: Option<String>,
    /// Everything Nix said, for a front-end that offers to show it.
    pub output: String,
}

impl Planned {
    /// Reads what Nix said about a dry build, which it writes as lines like
    /// `these 61 derivations will be built:` and `these 3 paths will be
    /// fetched (12.30 MiB download, 50.10 MiB unpacked):`.
    #[must_use]
    pub fn read(output: &str) -> Self {
        let mut planned = Self {
            output: output.to_owned(),
            ..Self::default()
        };

        for line in output.lines().map(str::trim) {
            if line.contains("will be built") {
                planned.to_build = count(line);
            } else if line.contains("will be fetched") {
                planned.to_fetch = count(line);
                planned.download = download(line);
            }
        }
        planned
    }

    /// Whether there is nothing left to do, because everything this system
    /// needs is already on the computer.
    #[must_use]
    pub fn nothing_to_do(&self) -> bool {
        self.to_build.is_none() && self.to_fetch.is_none() && self.download.is_none()
    }
}

impl fmt::Display for Planned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.nothing_to_do() {
            return f.write_str("nothing left to build");
        }

        let mut parts = Vec::new();
        if let Some(to_build) = self.to_build {
            parts.push(format!("{to_build} to build"));
        }
        match (self.to_fetch, &self.download) {
            (Some(to_fetch), Some(download)) => {
                parts.push(format!("{to_fetch} to download, {download}"));
            }
            (Some(to_fetch), None) => parts.push(format!("{to_fetch} to download")),
            (None, Some(download)) => parts.push(format!("{download} to download")),
            (None, None) => {}
        }
        f.write_str(&parts.join("; "))
    }
}

/// The count in a line such as `these 61 derivations will be built:`, or one
/// for `this derivation will be built:`. Nix leaves the count out of some of
/// its wordings, which is why a line it recognises can still have none.
fn count(line: &str) -> Option<usize> {
    line.split_whitespace()
        .nth(1)
        .and_then(|word| word.parse().ok())
        .or_else(|| line.starts_with("this ").then_some(1))
}

/// The download size in a line such as `these 3 paths will be fetched
/// (12.30 MiB download, 50.10 MiB unpacked):`.
fn download(line: &str) -> Option<String> {
    let (_, rest) = line.split_once('(')?;
    let (size, _) = rest.split_once(" download")?;
    Some(size.trim().to_owned())
}

/// Checking a configuration mustn't change it, and a flake whose lock file
/// has fallen behind is one Nix would otherwise bring up to date on the
/// spot — in a folder that usually belongs to root.
const READ_ONLY: &str = "--no-write-lock-file";

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
    /// Works out how to ask Nix about `system`.
    ///
    /// Fails before running anything when the configuration is a flake and
    /// the features it takes to read one are off, since Slipway has already
    /// read Nix's settings and needn't make Nix say so. For a flake that
    /// can be read, this costs one question of its own: which machine in it
    /// is this one.
    pub fn for_system(system: &System) -> Result<Self, Failure> {
        match system.config.kind {
            ConfigKind::ConfigurationNix => Ok(Self::Channels {
                entry: system.config.entry.clone(),
            }),
            ConfigKind::Flake => {
                features_for_flakes(system)?;
                let dir = system.config.dir.clone();
                let machine = machine_in(&dir)?;
                Ok(Self::Flake { dir, machine })
            }
        }
    }

    /// Checks that Nix can read the whole configuration, building nothing.
    pub fn evaluate(&self) -> Result<Evaluated, Failure> {
        let output = match self {
            Self::Flake { dir, machine } => cli::try_run(
                "nix",
                &[
                    "eval",
                    READ_ONLY,
                    "--raw",
                    &toplevel(dir, machine, ".drvPath"),
                ],
            ),
            Self::Channels { entry } => {
                let config = nixos_config(entry);
                cli::try_run(
                    "nix-instantiate",
                    &["-I", &config, "<nixpkgs/nixos>", "-A", "system"],
                )
            }
        }
        .map_err(|failed| Failure::from(Step::Read, failed))?;

        Ok(Evaluated {
            drv_path: output.stdout,
        })
    }

    /// Works out everything the system would build, without building any of
    /// it, so a configuration that reads but can't be built is caught too.
    pub fn dry_build(&self) -> Result<Planned, Failure> {
        let output = match self {
            Self::Flake { dir, machine } => cli::try_run(
                "nix",
                &[
                    "build",
                    "--dry-run",
                    "--no-link",
                    READ_ONLY,
                    &toplevel(dir, machine, ""),
                ],
            ),
            Self::Channels { entry } => {
                let config = nixos_config(entry);
                cli::try_run(
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
            }
        }
        .map_err(|failed| Failure::from(Step::DryBuild, failed))?;

        // Nix says what it would build as it goes, not in its answer.
        Ok(Planned::read(&output.stderr))
    }
}

impl PackageSet for Target {
    fn holds(&self, package: &PackageName) -> Result<Holds> {
        let question = looks_up(package);
        let output = match self {
            Self::Flake { dir, machine } => cli::try_run(
                "nix",
                &[
                    "eval",
                    "--json",
                    &format!("{}#nixosConfigurations.{machine}.pkgs", dir.display()),
                    "--apply",
                    &question,
                ],
            ),
            // A channels system has no flake to ask, so this is the package
            // set `<nixpkgs>` on its search path gives, which is the channel
            // it was built from.
            Self::Channels { .. } => cli::try_run(
                "nix-instantiate",
                &[
                    "--eval",
                    "--json",
                    "--expr",
                    &format!("({question}) (import <nixpkgs> {{ }})"),
                ],
            ),
        }
        .map_err(|failed| Failure::from(Step::LookUp, failed))?;

        let answer = output.stdout;
        match serde_json::from_str::<String>(&answer)
            .with_context(|| format!("Nix answered `{answer}`, which Slipway didn't expect"))?
            .as_str()
        {
            "package" => Ok(Holds::Package),
            "something else" => Ok(Holds::SomethingElse),
            "nothing" => Ok(Holds::Nothing),
            other => {
                anyhow::bail!("Nix answered `{other}`, which Slipway didn't expect")
            }
        }
    }
}

/// Fails unless Nix has the features it takes to read a flake.
///
/// Features that couldn't be read are left to Nix: it will say so itself,
/// and guessing here would stop a machine Slipway simply couldn't read the
/// settings of.
fn features_for_flakes(system: &System) -> Result<(), Failure> {
    let Ok(features) = &system.features else {
        return Ok(());
    };

    for (on, feature) in [
        (features.nix_command, "nix-command"),
        (features.flakes, "flakes"),
    ] {
        if !on {
            return Err(Failure::feature_off(Step::Read, feature));
        }
    }
    Ok(())
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
fn machine_in(dir: &Path) -> Result<String, Failure> {
    let listed = cli::try_run(
        "nix",
        &[
            "eval",
            "--json",
            &format!("{}#nixosConfigurations", dir.display()),
            "--apply",
            "builtins.attrNames",
        ],
    )
    .map_err(|failed| Failure::from(Step::Read, failed))?;

    let said = |output: String| Failure {
        step: Step::Read,
        why: Why::Refused,
        command: Some(listed.command.clone()),
        output,
    };

    let machines: Vec<String> = serde_json::from_str(&listed.stdout).map_err(|_| {
        said(format!(
            "it answered `{}`, which isn't a list of the computers in {}",
            listed.stdout,
            dir.join("flake.nix").display()
        ))
    })?;

    let this = hostname();
    if let Some(this) = &this
        && machines.iter().any(|machine| machine == this)
    {
        return Ok(this.clone());
    }

    match machines.as_slice() {
        [] => Err(said(format!(
            "{} doesn't describe any computer, so there's nothing for Slipway to check",
            dir.join("flake.nix").display()
        ))),
        [only] => Ok(only.clone()),
        several => Err(said(format!(
            "{} describes several computers ({}), and none of them is named `{}` like this one, so Slipway can't tell which is this computer",
            dir.join("flake.nix").display(),
            several.join(", "),
            this.unwrap_or_else(|| "unknown".to_owned())
        ))),
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
