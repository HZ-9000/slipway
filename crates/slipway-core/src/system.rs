//! Finds out what Slipway is working with: which NixOS this is, where its
//! system configuration lives, and which Nix features are turned on.

use std::{
    collections::HashMap,
    env, fmt, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};

use crate::{
    config::{Channel, OutputMode},
    nix,
};

/// Everything `slipway detect` reports.
#[derive(Debug)]
pub struct System {
    pub nixos: NixosRelease,
    /// The channel this system follows, such as `nixos-24.11`.
    pub channel: Channel,
    /// How [`System::channel`] was worked out.
    pub channel_source: ChannelSource,
    pub config: SystemConfig,
    /// Nix's experimental features, or why they couldn't be read.
    pub features: Result<NixFeatures>,
}

impl System {
    /// How Slipway should write its managed files for this system.
    #[must_use]
    pub fn output_mode(&self) -> OutputMode {
        self.config.kind.output_mode()
    }
}

/// The running NixOS, as `/etc/os-release` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixosRelease {
    /// A name for people, such as `NixOS 24.11 (Vicuna)`.
    pub pretty_name: String,
    /// The release number, such as `24.11`.
    pub version: String,
    /// The exact build, such as `24.11.20241010.a1b2c3d`.
    pub build: Option<String>,
    /// The release's code name, such as `vicuna`.
    pub codename: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelSource {
    /// The `nixpkgs` input recorded in the configuration's `flake.lock`.
    FlakeLock,
    /// The NixOS build version: development builds carry `pre` in it, and
    /// anything else is taken to be the numbered release.
    BuildVersion,
}

/// Where the system configuration lives and how it is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemConfig {
    /// The folder holding it, with symlinks resolved, so a `/etc/nixos` that
    /// links to a flake in the user's home is reported where it really is.
    pub dir: PathBuf,
    /// The file NixOS builds from: `flake.nix` or `configuration.nix`.
    pub entry: PathBuf,
    pub kind: ConfigKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKind {
    /// Built from a `flake.nix`.
    Flake,
    /// Built from a `configuration.nix` and channels.
    ConfigurationNix,
}

impl ConfigKind {
    #[must_use]
    pub fn output_mode(self) -> OutputMode {
        match self {
            Self::Flake => OutputMode::Flake,
            Self::ConfigurationNix => OutputMode::Channels,
        }
    }
}

/// The Nix features Slipway cares about, as turned on in Nix's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NixFeatures {
    pub nix_command: bool,
    pub flakes: bool,
}

impl NixFeatures {
    /// Picks the features out of an `experimental-features` list.
    #[must_use]
    pub fn from_enabled<S: AsRef<str>>(enabled: &[S]) -> Self {
        let has = |name: &str| enabled.iter().any(|feature| feature.as_ref() == name);
        Self {
            nix_command: has("nix-command"),
            flakes: has("flakes"),
        }
    }
}

/// Returned when Slipway isn't running on NixOS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotNixos {
    /// What the computer is running instead, if known.
    pub running: Option<String>,
}

impl fmt::Display for NotNixos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.running {
            Some(running) => write!(
                f,
                "Slipway only works on NixOS, but this computer is running {running}"
            ),
            None => f.write_str(
                "Slipway only works on NixOS, and this computer doesn't look like NixOS (it has no /etc/os-release)",
            ),
        }
    }
}

impl std::error::Error for NotNixos {}

/// Where to look. [`Probe::new`] looks at this machine; tests point a probe
/// at a folder laid out like one.
#[derive(Debug, Clone)]
pub struct Probe {
    /// The folder `etc/os-release` and `etc/nixos` are read from.
    pub root: PathBuf,
    /// The system configuration the user pointed at, if any: a folder, a
    /// `flake.nix` or a `configuration.nix`. Without one, `etc/nixos`.
    pub config: Option<PathBuf>,
}

impl Probe {
    /// Looks at this machine. Without `config`, honours `NIXOS_CONFIG` as
    /// `nixos-rebuild` does, then falls back to `/etc/nixos`.
    #[must_use]
    pub fn new(config: Option<PathBuf>) -> Self {
        let config = config.or_else(|| {
            env::var_os("NIXOS_CONFIG")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        });
        Self {
            root: PathBuf::from("/"),
            config,
        }
    }

    /// Everything at once. Fails early with [`NotNixos`] when this isn't
    /// NixOS.
    pub fn system(&self) -> Result<System> {
        let nixos = self.nixos()?;
        let config = self.system_config()?;
        let (channel, channel_source) = channel(&nixos, &config)?;
        let features =
            nix::eval::experimental_features().map(|enabled| NixFeatures::from_enabled(&enabled));

        Ok(System {
            nixos,
            channel,
            channel_source,
            config,
            features,
        })
    }

    /// Reads `etc/os-release`, failing with [`NotNixos`] if it describes
    /// another system.
    pub fn nixos(&self) -> Result<NixosRelease> {
        let path = self.root.join("etc/os-release");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let running = (env::consts::OS == "macos").then(|| "macOS".to_owned());
                return Err(NotNixos { running }.into());
            }
            Err(err) => {
                return Err(err).with_context(|| format!("couldn't read {}", path.display()));
            }
        };
        let fields = parse_os_release(&text);
        let field = |key: &str| fields.get(key).cloned();

        let is_nixos = field("ID").as_deref() == Some("nixos")
            || field("ID_LIKE").is_some_and(|like| like.split_whitespace().any(|id| id == "nixos"));
        if !is_nixos {
            let running = field("PRETTY_NAME").or_else(|| field("NAME"));
            return Err(NotNixos { running }.into());
        }

        let Some(version) = field("VERSION_ID") else {
            bail!(
                "{} doesn't say which NixOS version this is (no VERSION_ID)",
                path.display()
            );
        };
        Ok(NixosRelease {
            pretty_name: field("PRETTY_NAME").unwrap_or_else(|| format!("NixOS {version}")),
            version,
            build: field("BUILD_ID"),
            codename: field("VERSION_CODENAME"),
        })
    }

    /// Finds the system configuration, preferring `flake.nix` over
    /// `configuration.nix` as `nixos-rebuild` does.
    pub fn system_config(&self) -> Result<SystemConfig> {
        let start = self
            .config
            .clone()
            .unwrap_or_else(|| self.root.join("etc/nixos"));

        let found = fs::canonicalize(&start)
            .with_context(|| format!("there's no system configuration at {}", start.display()))?;

        if found.is_file() {
            let kind = if found.file_name().is_some_and(|name| name == "flake.nix") {
                ConfigKind::Flake
            } else {
                ConfigKind::ConfigurationNix
            };
            return config_at(found, kind);
        }

        for (name, kind) in [
            ("flake.nix", ConfigKind::Flake),
            ("configuration.nix", ConfigKind::ConfigurationNix),
        ] {
            let entry = found.join(name);
            if entry.is_file() {
                let entry = fs::canonicalize(&entry)
                    .with_context(|| format!("couldn't follow {}", entry.display()))?;
                return config_at(entry, kind);
            }
        }

        bail!(
            "{} has neither a flake.nix nor a configuration.nix, so Slipway can't tell how this system is built",
            start.display()
        )
    }
}

fn config_at(entry: PathBuf, kind: ConfigKind) -> Result<SystemConfig> {
    let dir = entry
        .parent()
        .with_context(|| format!("{} has no parent folder", entry.display()))?
        .to_owned();
    Ok(SystemConfig { dir, entry, kind })
}

fn channel(nixos: &NixosRelease, config: &SystemConfig) -> Result<(Channel, ChannelSource)> {
    if config.kind == ConfigKind::Flake
        && let Some(channel) = flake_lock_channel(&config.dir.join("flake.lock"))
    {
        return Ok((channel, ChannelSource::FlakeLock));
    }

    let name = if nixos
        .build
        .as_deref()
        .is_some_and(|build| build.contains("pre"))
    {
        "nixos-unstable".to_owned()
    } else {
        format!("nixos-{}", nixos.version)
    };
    let channel = name
        .parse()
        .with_context(|| format!("NixOS reports an unexpected version, {}", nixos.version))?;
    Ok((channel, ChannelSource::BuildVersion))
}

/// The channel named by the `nixpkgs` input of a `flake.lock`, if it names
/// one (an input pinned to a bare commit doesn't).
fn flake_lock_channel(path: &Path) -> Option<Channel> {
    let lock: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    let nodes = &lock["nodes"];
    let root = lock["root"].as_str().unwrap_or("root");
    let input = nodes[root]["inputs"]["nixpkgs"].as_str()?;
    let original = &nodes[input]["original"];

    // `github:NixOS/nixpkgs/nixos-24.11` locks with `ref`; a channel tarball
    // such as https://channels.nixos.org/nixos-24.11/nixexprs.tar.xz only
    // names it in the URL.
    let from_url = original["url"]
        .as_str()
        .into_iter()
        .flat_map(|url| url.split(['/', '?', '&', '=']));
    original["ref"]
        .as_str()
        .into_iter()
        .chain(from_url)
        .filter(|name| name.starts_with("nixos-") || name.starts_with("nixpkgs-"))
        .find_map(|name| name.parse().ok())
}

/// Parses `KEY=value` lines as described in os-release(5).
fn parse_os_release(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), unquote(value)))
        .collect()
}

fn unquote(value: &str) -> String {
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return inner.to_owned();
    }
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
        return value.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}
