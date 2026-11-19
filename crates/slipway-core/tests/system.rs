//! Tests for detecting the NixOS release, system configuration and channel,
//! run against folders laid out like a real system.

use std::{fs, os::unix::fs::symlink, path::Path};

use slipway_core::{
    config::{Channel, OutputMode},
    system::{ChannelSource, ConfigKind, NixFeatures, NotNixos, Probe},
};
use tempfile::TempDir;

const STABLE: &str = r#"ANSI_COLOR="0;38;2;126;186;228"
BUILD_ID="24.11.20241010.a1b2c3d"
DEFAULT_HOSTNAME=nixos
ID=nixos
NAME=NixOS
PRETTY_NAME="NixOS 24.11 (Vicuna)"
VERSION="24.11 (Vicuna)"
VERSION_CODENAME=vicuna
VERSION_ID="24.11"
"#;

const UNSTABLE: &str = r#"BUILD_ID="25.05pre712345.e4f5a6b"
ID=nixos
NAME=NixOS
PRETTY_NAME="NixOS 25.05 (Warbler)"
VERSION_CODENAME=warbler
VERSION_ID="25.05"
"#;

/// A fake filesystem root with the given `/etc/os-release`.
fn root_with(os_release: &str) -> TempDir {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("etc/nixos")).unwrap();
    fs::write(root.path().join("etc/os-release"), os_release).unwrap();
    root
}

fn probe(root: &TempDir) -> Probe {
    Probe {
        root: root.path().to_owned(),
        config: None,
    }
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "{ }\n").unwrap();
}

fn flake_lock(original: &str) -> String {
    format!(
        r#"{{
  "nodes": {{
    "nixpkgs": {{
      "locked": {{ "lastModified": 1728538411, "narHash": "sha256-x", "rev": "a1b2c3d", "type": "github", "owner": "NixOS", "repo": "nixpkgs" }},
      "original": {original}
    }},
    "root": {{ "inputs": {{ "nixpkgs": "nixpkgs" }} }}
  }},
  "root": "root",
  "version": 7
}}"#
    )
}

fn not_nixos(probe: &Probe) -> NotNixos {
    probe
        .nixos()
        .expect_err("should not be detected as NixOS")
        .downcast::<NotNixos>()
        .expect("error should be NotNixos")
}

#[test]
fn reads_the_nixos_release() {
    let root = root_with(STABLE);

    let nixos = probe(&root).nixos().unwrap();

    assert_eq!(nixos.pretty_name, "NixOS 24.11 (Vicuna)");
    assert_eq!(nixos.version, "24.11");
    assert_eq!(nixos.build.as_deref(), Some("24.11.20241010.a1b2c3d"));
    assert_eq!(nixos.codename.as_deref(), Some("vicuna"));
}

#[test]
fn accepts_systems_based_on_nixos() {
    let root = root_with("ID=mydistro\nID_LIKE=nixos\nVERSION_ID=1.0\n");

    assert_eq!(probe(&root).nixos().unwrap().version, "1.0");
}

#[test]
fn unquotes_os_release_values() {
    let root =
        root_with("ID='nixos'\nVERSION_ID=\"24.11\"\nPRETTY_NAME=\"NixOS \\\"Custom\\\"\"\n");

    let nixos = probe(&root).nixos().unwrap();

    assert_eq!(nixos.pretty_name, "NixOS \"Custom\"");
}

#[test]
fn names_the_other_system_when_not_nixos() {
    let root = root_with("ID=ubuntu\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\nVERSION_ID=\"24.04\"\n");

    let err = not_nixos(&probe(&root));

    assert_eq!(err.running.as_deref(), Some("Ubuntu 24.04 LTS"));
    assert_eq!(
        err.to_string(),
        "Slipway only works on NixOS, but this computer is running Ubuntu 24.04 LTS"
    );
}

#[test]
fn a_missing_os_release_is_not_nixos() {
    let root = TempDir::new().unwrap();

    let err = not_nixos(&probe(&root));

    assert!(
        err.to_string().starts_with("Slipway only works on NixOS"),
        "got: {err}"
    );
}

#[test]
fn not_nixos_is_reported_before_anything_else() {
    let root = TempDir::new().unwrap();

    let err = probe(&root).system().unwrap_err();

    assert!(err.downcast_ref::<NotNixos>().is_some(), "got: {err:#}");
}

#[test]
fn finds_configuration_nix_in_etc_nixos() {
    let root = root_with(STABLE);
    touch(&root.path().join("etc/nixos/configuration.nix"));

    let config = probe(&root).system_config().unwrap();

    assert_eq!(config.kind, ConfigKind::ConfigurationNix);
    assert_eq!(config.kind.output_mode(), OutputMode::Channels);
    assert!(config.entry.ends_with("etc/nixos/configuration.nix"));
    assert!(config.dir.ends_with("etc/nixos"));
}

#[test]
fn prefers_flake_nix_over_configuration_nix() {
    let root = root_with(STABLE);
    touch(&root.path().join("etc/nixos/configuration.nix"));
    touch(&root.path().join("etc/nixos/flake.nix"));

    let config = probe(&root).system_config().unwrap();

    assert_eq!(config.kind, ConfigKind::Flake);
    assert_eq!(config.kind.output_mode(), OutputMode::Flake);
    assert!(config.entry.ends_with("etc/nixos/flake.nix"));
}

#[test]
fn follows_a_linked_flake_to_the_users_folder() {
    let root = root_with(STABLE);
    let users_flake = root.path().join("home/me/nixos-config/flake.nix");
    touch(&users_flake);
    symlink(&users_flake, root.path().join("etc/nixos/flake.nix")).unwrap();

    let config = probe(&root).system_config().unwrap();

    assert_eq!(config.kind, ConfigKind::Flake);
    assert!(
        config.dir.ends_with("home/me/nixos-config"),
        "got {:?}",
        config.dir
    );
}

#[test]
fn uses_a_config_folder_the_user_points_at() {
    let root = root_with(STABLE);
    let elsewhere = TempDir::new().unwrap();
    touch(&elsewhere.path().join("flake.nix"));

    let probe = Probe {
        config: Some(elsewhere.path().to_owned()),
        ..probe(&root)
    };
    let config = probe.system_config().unwrap();

    assert_eq!(config.kind, ConfigKind::Flake);
    assert_eq!(config.dir, fs::canonicalize(elsewhere.path()).unwrap());
}

#[test]
fn uses_a_config_file_the_user_points_at() {
    let root = root_with(STABLE);
    let file = root.path().join("srv/machines/laptop.nix");
    touch(&file);

    let probe = Probe {
        config: Some(file.clone()),
        ..probe(&root)
    };
    let config = probe.system_config().unwrap();

    assert_eq!(config.kind, ConfigKind::ConfigurationNix);
    assert_eq!(config.entry, fs::canonicalize(&file).unwrap());
}

#[test]
fn explains_when_there_is_no_config() {
    let root = root_with(STABLE);

    let err = probe(&root).system_config().unwrap_err();
    assert!(
        format!("{err:#}").contains("neither a flake.nix nor a configuration.nix"),
        "got: {err:#}"
    );

    let probe = Probe {
        config: Some(root.path().join("nowhere")),
        ..probe(&root)
    };
    let err = probe.system_config().unwrap_err();
    assert!(
        format!("{err:#}").contains("there's no system configuration at"),
        "got: {err:#}"
    );
}

#[test]
fn stable_channel_comes_from_the_release_number() {
    let root = root_with(STABLE);
    touch(&root.path().join("etc/nixos/configuration.nix"));

    let system = probe(&root).system().unwrap();

    assert_eq!(system.channel.as_str(), "nixos-24.11");
    assert!(!system.channel.is_unstable());
    assert_eq!(system.channel_source, ChannelSource::BuildVersion);
    assert_eq!(system.output_mode(), OutputMode::Channels);
}

#[test]
fn development_builds_are_unstable() {
    let root = root_with(UNSTABLE);
    touch(&root.path().join("etc/nixos/configuration.nix"));

    let system = probe(&root).system().unwrap();

    assert_eq!(system.channel.as_str(), "nixos-unstable");
    assert!(system.channel.is_unstable());
}

#[test]
fn flake_channel_comes_from_flake_lock() {
    let root = root_with(STABLE);
    touch(&root.path().join("etc/nixos/flake.nix"));
    fs::write(
        root.path().join("etc/nixos/flake.lock"),
        flake_lock(
            r#"{ "owner": "NixOS", "ref": "nixos-unstable", "repo": "nixpkgs", "type": "github" }"#,
        ),
    )
    .unwrap();

    let system = probe(&root).system().unwrap();

    assert_eq!(system.channel.as_str(), "nixos-unstable");
    assert_eq!(system.channel_source, ChannelSource::FlakeLock);
    assert_eq!(system.output_mode(), OutputMode::Flake);
}

#[test]
fn flake_channel_can_come_from_a_channel_url() {
    let root = root_with(UNSTABLE);
    touch(&root.path().join("etc/nixos/flake.nix"));
    fs::write(
        root.path().join("etc/nixos/flake.lock"),
        flake_lock(
            r#"{ "type": "tarball", "url": "https://channels.nixos.org/nixos-24.11/nixexprs.tar.xz" }"#,
        ),
    )
    .unwrap();

    let system = probe(&root).system().unwrap();

    assert_eq!(system.channel.as_str(), "nixos-24.11");
    assert_eq!(system.channel_source, ChannelSource::FlakeLock);
}

#[test]
fn a_flake_pinned_to_a_commit_falls_back_to_the_release() {
    let root = root_with(STABLE);
    touch(&root.path().join("etc/nixos/flake.nix"));
    fs::write(
        root.path().join("etc/nixos/flake.lock"),
        flake_lock(
            r#"{ "owner": "NixOS", "repo": "nixpkgs", "rev": "a1b2c3d", "type": "github" }"#,
        ),
    )
    .unwrap();

    let system = probe(&root).system().unwrap();

    assert_eq!(system.channel.as_str(), "nixos-24.11");
    assert_eq!(system.channel_source, ChannelSource::BuildVersion);
}

#[test]
fn picks_nix_command_and_flakes_out_of_the_feature_list() {
    assert_eq!(
        NixFeatures::from_enabled(&["ca-derivations", "flakes", "nix-command"]),
        NixFeatures {
            nix_command: true,
            flakes: true
        }
    );
    assert_eq!(
        NixFeatures::from_enabled(&["nix-command"]),
        NixFeatures {
            nix_command: true,
            flakes: false
        }
    );
    assert_eq!(
        NixFeatures::from_enabled::<&str>(&[]),
        NixFeatures {
            nix_command: false,
            flakes: false
        }
    );
}

#[test]
fn reads_this_machines_nix_features() {
    let root = root_with(STABLE);
    touch(&root.path().join("etc/nixos/configuration.nix"));

    let system = probe(&root).system().unwrap();

    system
        .features
        .expect("Nix's settings should be readable wherever the tests run");
}

#[test]
fn recognises_unstable_channel_names() {
    for (name, unstable) in [
        ("nixos-unstable", true),
        ("nixos-unstable-small", true),
        ("nixpkgs-unstable", true),
        ("nixos-24.11", false),
        ("nixos-24.11-small", false),
    ] {
        let channel: Channel = name.parse().unwrap();
        assert_eq!(channel.is_unstable(), unstable, "{name}");
    }
}
