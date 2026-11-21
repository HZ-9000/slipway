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

/// A folder laid out like a real system, built from `configuration`, and
/// what Slipway makes of it.
pub fn system(configuration: &str) -> (TempDir, System) {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("etc/nixos");
    fs::create_dir_all(&dir).unwrap();
    fs::write(root.path().join("etc/os-release"), OS_RELEASE).unwrap();
    fs::write(dir.join("configuration.nix"), configuration).unwrap();
    fs::write(dir.join("hardware-configuration.nix"), "{ }\n").unwrap();

    let system = Probe {
        root: root.path().to_owned(),
        config: None,
    }
    .system()
    .expect("the fixture looks like NixOS");
    (root, system)
}

/// Slipway's folder in that system, whether or not it's there yet.
pub fn managed(system: &System) -> PathBuf {
    slipway_core::config::managed_dir(&system.config.dir)
}
