//! Folders laid out like a real system, for tests that need Slipway to find
//! one. Each test binary uses the parts of this it needs.
#![allow(dead_code)]

use std::{fs, path::PathBuf};

use slipway_core::system::{Probe, System};
use tempfile::TempDir;

/// What `/etc/os-release` says on a stable NixOS.
const OS_RELEASE: &str = r#"BUILD_ID="24.11.20241010.a1b2c3d"
ID=nixos
NAME=NixOS
PRETTY_NAME="NixOS 24.11 (Vicuna)"
VERSION_ID="24.11"
"#;

/// A `configuration.nix` that brings Slipway's folder in, as `slipway
/// connect` leaves one.
pub const CHANNELS: &str = r#"{ config, pkgs, ... }:

{
  imports = [
    ./slipway
    ./hardware-configuration.nix
  ];

  environment.systemPackages = [ pkgs.vim ];

  system.stateVersion = "24.11";
}
"#;

/// The same configuration before connecting.
pub const NOT_CONNECTED: &str = r#"{ config, pkgs, ... }:

{
  imports = [ ./hardware-configuration.nix ];

  system.stateVersion = "24.11";
}
"#;

/// A `flake.nix` naming one machine, as `slipway connect` leaves one.
pub const FLAKE: &str = r#"{
  inputs.slipway.url = "path:./slipway";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";

  outputs = { self, nixpkgs, slipway }: {
    nixosConfigurations.nixos = nixpkgs.lib.nixosSystem {
      modules = [ slipway.nixosModules.default ./configuration.nix ];
    };
  };
}
"#;

/// A folder laid out like a real system built from `configuration.nix`, and
/// what Slipway makes of it.
pub fn system(configuration: &str) -> (TempDir, System) {
    laid_out(configuration, None)
}

/// The same for a system built from a flake, which Slipway reads in its own
/// way.
pub fn flake_system(configuration: &str) -> (TempDir, System) {
    laid_out(configuration, Some(FLAKE))
}

fn laid_out(configuration: &str, flake: Option<&str>) -> (TempDir, System) {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("etc/nixos");
    fs::create_dir_all(&dir).unwrap();
    fs::write(root.path().join("etc/os-release"), OS_RELEASE).unwrap();
    fs::write(dir.join("configuration.nix"), configuration).unwrap();
    fs::write(dir.join("hardware-configuration.nix"), "{ }\n").unwrap();
    if let Some(flake) = flake {
        fs::write(dir.join("flake.nix"), flake).unwrap();
    }

    let system = Probe {
        root: root.path().to_owned(),
        config: None,
    }
    .system()
    .expect("the fixture looks like NixOS");
    (root, system)
}

/// `NIX_CONFIG` outranks Nix's configuration files, so a shell that sets it
/// speaks for the fixture's features instead of the fixture.
pub fn host_sets_nix_config() -> bool {
    std::env::var_os("NIX_CONFIG").is_some_and(|value| !value.is_empty())
}

/// Slipway's folder in that system, whether or not it's there yet.
pub fn managed(system: &System) -> PathBuf {
    slipway_core::config::managed_dir(&system.config.dir)
}
