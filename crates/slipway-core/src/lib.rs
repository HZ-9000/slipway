//! Everything slipway does lives here, so the CLI, TUI and GUI are thin
//! front-ends over the same logic. Nothing in this crate prints.

use std::{env, fmt, fs, io, path::PathBuf};

use anyhow::{Context as _, Result};

pub mod config;
pub mod nix;
pub mod package;
pub mod system;
pub mod wire;

use package::PackageName;

pub fn config_dir() -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config/slipway"))
}

/// Creates slipway's config directory if needed and returns its path.
pub fn init_config_dir() -> Result<PathBuf> {
    let config_directory = config_dir().context("couldn't find your home folder")?;
    fs::create_dir_all(&config_directory)
        .with_context(|| format!("couldn't create {}", config_directory.display()))?;
    Ok(config_directory)
}

/// The Nix versions slipway can see; they need not agree.
pub struct NixVersions {
    /// The `nix` command on `PATH`.
    pub on_path: Result<String>,
    /// The Nix daemon/store this machine uses.
    pub store: Result<String>,
}

#[must_use]
pub fn detect() -> NixVersions {
    NixVersions {
        on_path: nix::cli::nix_cli_version(),
        store: nix::cli::store_version(),
    }
}

/// Works out which NixOS this is, where its system configuration lives and
/// which Nix features are on. `config` points at the configuration when it
/// isn't in the usual place.
///
/// Fails with [`system::NotNixos`] when this computer isn't running NixOS.
pub fn detect_system(config: Option<PathBuf>) -> Result<system::System> {
    system::Probe::new(config).system()
}

/// Plans the one edit that connects the system configuration to Slipway's
/// managed folder, or `None` if it's already connected.
///
/// Fails with [`wire::CantWire`] when the configuration is too unusual to
/// edit safely.
pub fn plan_connection(system: &system::System) -> Result<Option<wire::Wiring>> {
    wire::plan(&system.config, &config::managed_dir())
}

/// Writes Slipway's managed folder if it's missing, then makes the planned
/// edit.
pub fn connect(system: &system::System, wiring: Option<&wire::Wiring>) -> Result<()> {
    wire::connect(system, &config::managed_dir(), wiring)
}

pub fn add(_package: &PackageName) -> Result<()> {
    Err(NotImplemented("adding packages").into())
}

pub fn remove(_package: &PackageName) -> Result<()> {
    Err(NotImplemented("removing packages").into())
}

pub fn list() -> Result<Vec<PackageName>> {
    Err(NotImplemented("listing packages").into())
}

pub fn check() -> Result<()> {
    Err(NotImplemented("checking your setup").into())
}

/// Returned by commands that exist in the interface but aren't built yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotImplemented(pub &'static str);

impl fmt::Display for NotImplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} isn't available in this version of slipway yet",
            self.0
        )
    }
}

impl std::error::Error for NotImplemented {}
