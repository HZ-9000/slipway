//! What Slipway has been asked to look after on this computer.
//!
//! The list lives beside the Nix it generates, in the managed folder, so a
//! configuration kept in git carries it along and a reinstall finds it
//! again. It is written by Slipway and read by nothing else, so it is the
//! one file here that isn't Nix.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use serde_json::json;

use crate::{config::Config, package::PackageName, system::System};

/// The file Slipway keeps its choices in, inside the managed folder.
pub const STATE_FILE: &str = "state.json";

/// The layout of [`STATE_FILE`]. Slipway refuses a file written by a newer
/// version rather than guessing what it means.
const VERSION: u64 = 1;

/// The choices Slipway is keeping for one computer.
///
/// This is only what the user asked for. Everything else a config needs —
/// which channel, which kind of files to write — comes from the machine, so
/// moving the state to another computer keeps the packages and picks up
/// that computer's channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// Packages the user asked Slipway for, by name.
    pub packages: BTreeSet<PackageName>,
}

impl State {
    /// Where the state file sits inside the managed folder `dir`.
    #[must_use]
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(STATE_FILE)
    }

    /// Reads the state kept in `dir`, treating a missing file as a fresh
    /// start: a folder Slipway has only just written has no choices in it
    /// yet.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = Self::path(dir);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => {
                return Err(err).with_context(|| format!("couldn't read {}", path.display()));
            }
        };

        let saved: serde_json::Value = serde_json::from_str(&text).with_context(|| {
            format!(
                "{} isn't readable any more; Slipway writes it, so something else has changed it",
                path.display()
            )
        })?;

        let version = saved["version"].as_u64().with_context(|| {
            format!(
                "{} doesn't say which version of Slipway wrote it",
                path.display()
            )
        })?;
        if version > VERSION {
            bail!(
                "{} was written by a newer version of Slipway; update Slipway to read it",
                path.display()
            );
        }

        let listed = saved["packages"]
            .as_array()
            .with_context(|| format!("{} has no list of packages in it", path.display()))?;
        let mut packages = BTreeSet::new();
        for entry in listed {
            let name = entry
                .as_str()
                .with_context(|| format!("{} lists a package that isn't a name", path.display()))?;
            let package = name.parse::<PackageName>().with_context(|| {
                format!(
                    "{} lists `{name}`, which isn't a package name",
                    path.display()
                )
            })?;
            packages.insert(package);
        }

        Ok(Self { packages })
    }

    /// Writes the state into `dir`, replacing the file in one step so it is
    /// never left half-written.
    pub fn save(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir).with_context(|| format!("couldn't create {}", dir.display()))?;

        let path = Self::path(dir);
        let contents = format!(
            "{:#}\n",
            json!({
                "version": VERSION,
                "packages": self
                    .packages
                    .iter()
                    .map(PackageName::as_str)
                    .collect::<Vec<_>>(),
            })
        );

        let temp = dir.join(format!(".{STATE_FILE}.tmp"));
        fs::write(&temp, &contents)
            .with_context(|| format!("couldn't write {}", temp.display()))?;
        fs::rename(&temp, &path).with_context(|| format!("couldn't replace {}", path.display()))?;
        Ok(())
    }

    /// The config this state describes on `system`: the state says what the
    /// user chose, the system says which channel it comes from and which
    /// files to write.
    #[must_use]
    pub fn config(&self, system: &System) -> Config {
        let mut config = Config::new(system.channel.clone(), system.output_mode());
        config.packages = self.packages.clone();
        config
    }
}
