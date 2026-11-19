use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context as _, Result, bail};

use crate::package::PackageName;

mod generate;
mod value;

pub use generate::{MANAGED_MARKER, ManagedFile};
pub use value::{AttrPath, NixValue};
pub const MANAGED_DIR: &str = "/etc/nixos/slipway";

#[must_use]
pub fn managed_dir() -> PathBuf {
    PathBuf::from(MANAGED_DIR)
}

/// Everything Slipway has chosen for one system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The nixpkgs channel packages and options are chosen from.
    pub channel: Channel,
    /// Whether the user's system is built from a flake or from channels.
    pub output_mode: OutputMode,
    /// Packages installed for everyone, as in `environment.systemPackages`.
    pub packages: BTreeSet<PackageName>,
    modules: BTreeMap<AttrPath, Module>,
}

impl Config {
    #[must_use]
    pub fn new(channel: Channel, output_mode: OutputMode) -> Self {
        Self {
            channel,
            output_mode,
            packages: BTreeSet::new(),
            modules: BTreeMap::new(),
        }
    }

    /// The enabled modules, sorted by option path.
    pub fn modules(&self) -> impl Iterator<Item = (&AttrPath, &Module)> {
        self.modules.iter()
    }

    /// Turns on the module at `path` (setting its `enable` option) and
    /// returns it so options can be set. Enabling it again keeps its options.
    pub fn enable_module(&mut self, path: AttrPath) -> &mut Module {
        self.modules.entry(path).or_default()
    }

    /// Turns off the module at `path` and forgets its options. Returns
    /// whether it was enabled.
    pub fn disable_module(&mut self, path: &AttrPath) -> bool {
        self.modules.remove(path).is_some()
    }

    /// The files this config becomes, in the order they're written.
    #[must_use]
    pub fn render(&self) -> Vec<ManagedFile> {
        generate::render(self)
    }

    /// Writes the managed files into `dir`, creating it if needed.
    ///
    /// Refuses to overwrite a file that Slipway didn't write, and removes
    /// files Slipway wrote earlier but no longer needs (for example
    /// `flake.nix` after switching from a flake to channels).
    pub fn write_to(&self, dir: &Path) -> Result<()> {
        let files = self.render();

        fs::create_dir_all(dir).with_context(|| format!("couldn't create {}", dir.display()))?;

        for name in generate::FILE_NAMES {
            let path = dir.join(name);
            if !is_ours_or_missing(&path)? {
                bail!(
                    "{} wasn't written by Slipway, so Slipway won't replace it; move it out of {} and try again",
                    path.display(),
                    dir.display()
                );
            }
        }

        for file in &files {
            let path = dir.join(file.name);
            let temp = dir.join(format!(".{}.tmp", file.name));
            fs::write(&temp, &file.contents)
                .with_context(|| format!("couldn't write {}", temp.display()))?;
            fs::rename(&temp, &path)
                .with_context(|| format!("couldn't replace {}", path.display()))?;
        }

        for name in generate::FILE_NAMES {
            let path = dir.join(name);
            if !files.iter().any(|file| file.name == name) && path.exists() {
                fs::remove_file(&path)
                    .with_context(|| format!("couldn't remove {}", path.display()))?;
            }
        }

        Ok(())
    }
}

fn is_ours_or_missing(path: &Path) -> Result<bool> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(generate::is_managed(&contents)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(err) => Err(err).with_context(|| format!("couldn't read {}", path.display())),
    }
}

/// One enabled NixOS module, such as `services.openssh`, and the options
/// Slipway sets on it besides `enable`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Module {
    options: BTreeMap<String, NixValue>,
}

impl Module {
    /// Sets an option, such as `ports` on `services.openssh`. Nested options
    /// like `settings.PasswordAuthentication` are set with a
    /// [`NixValue::Attrs`] under `settings`.
    pub fn set(
        &mut self,
        name: impl Into<String>,
        value: impl Into<NixValue>,
    ) -> Result<&mut Self, InvalidConfig> {
        let name = name.into();
        if name.is_empty() {
            return Err(InvalidConfig::new("an option name can't be empty"));
        }
        if name == "enable" {
            return Err(InvalidConfig::new(
                "`enable` is set by enabling the module; it can't be set as an option",
            ));
        }
        self.options.insert(name, value.into());
        Ok(self)
    }

    /// Removes an option. Returns whether it was set.
    pub fn unset(&mut self, name: &str) -> bool {
        self.options.remove(name).is_some()
    }

    /// The options set on this module, sorted by name.
    pub fn options(&self) -> impl Iterator<Item = (&str, &NixValue)> {
        self.options
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
}

/// A nixpkgs channel, such as `nixos-24.11` or `nixos-unstable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel(String);

impl Channel {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this channel follows nixpkgs' development branch rather than
    /// a numbered release, as `nixos-unstable` and `nixpkgs-unstable` do.
    #[must_use]
    pub fn is_unstable(&self) -> bool {
        self.0.split('-').any(|part| part == "unstable")
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Channel {
    type Err = InvalidConfig;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let valid = name.starts_with(|c: char| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.'));
        if !valid {
            return Err(InvalidConfig::new(format!(
                "`{name}` isn't a channel name; channels look like `nixos-24.11` or `nixos-unstable`"
            )));
        }
        Ok(Self(name.to_owned()))
    }
}

/// How the user's system configuration is built, which decides how it
/// picks up the managed directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// A flake: Slipway's directory is a flake exposing `nixosModules.default`.
    Flake,
    /// Channels and `configuration.nix`, which imports `./slipway`.
    Channels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidConfig {
    reason: String,
}

impl InvalidConfig {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for InvalidConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for InvalidConfig {}
