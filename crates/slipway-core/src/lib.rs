//! Everything slipway does lives here, so the CLI, TUI and GUI are thin
//! front-ends over the same logic. Nothing in this crate prints.

use std::{env, fs, io, path::PathBuf};

use anyhow::{Context as _, Result};

pub mod config;
pub mod managed;
pub mod nix;
pub mod package;
pub mod state;
pub mod system;
pub mod wire;

use managed::{Change, Report};
use package::PackageName;
use system::System;

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
pub fn plan_connection(system: &System) -> Result<Option<wire::Wiring>> {
    wire::plan(&system.config, &managed_dir(system))
}

/// Writes Slipway's managed folder if it's missing, then makes the planned
/// edit.
pub fn connect(system: &System, wiring: Option<&wire::Wiring>) -> Result<()> {
    wire::connect(system, &managed_dir(system), wiring)
}

/// Adds a package to the ones Slipway looks after on this computer, then
/// writes the Nix that installs it.
///
/// Fails in plain language when the name isn't a package the computer's
/// channel has.
pub fn add(system: &System, package: &PackageName) -> Result<Change> {
    let dir = managed_dir(system);
    managed::require_in_use(&dir, system)?;
    let available = nix::eval::Target::for_system(system)?;
    managed::add(&dir, system, &available, package)
}

/// Takes a package off the ones Slipway looks after, then writes the Nix
/// without it.
pub fn remove(system: &System, package: &PackageName) -> Result<Change> {
    managed::remove(&managed_dir(system), system, package)
}

/// The packages Slipway looks after on this computer. Packages the user's
/// own configuration installs aren't Slipway's to list.
pub fn list(system: &System) -> Result<Vec<PackageName>> {
    managed::packages(&managed_dir(system), system)
}

/// Looks over the whole setup, changing none of it.
#[must_use]
pub fn check(system: &System) -> Report {
    managed::check(&managed_dir(system), system)
}

/// Slipway's folder on this computer, beside the system configuration it
/// found.
fn managed_dir(system: &System) -> PathBuf {
    config::managed_dir(&system.config.dir)
}
