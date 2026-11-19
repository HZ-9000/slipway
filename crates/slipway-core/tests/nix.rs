//! Tests for the `nix` module and the config-path helper.

use std::fs;

use slipway_core::{
    config_dir,
    nix::{
        cli::{nix_cli_version, store_version},
        settings::Settings,
    },
    system::NixFeatures,
};
use tempfile::TempDir;

#[test]
fn config_dir_is_under_home() {
    let home = std::env::var("HOME").expect("tests run with HOME set");
    let dir = config_dir().expect("HOME is set, so this resolves");

    assert!(dir.starts_with(&home), "{dir:?} should live under {home:?}");
    assert!(dir.ends_with(".config/slipway"), "unexpected path: {dir:?}");
}

#[test]
fn nix_cli_version_is_reported() {
    let version = nix_cli_version().expect("`nix --version` should run");

    assert!(
        version.contains("nix"),
        "expected `nix --version` output, got {version:?}"
    );
    assert_eq!(version.trim(), version, "output should be trimmed");
}

/// The store version needs the `nix-command` feature, so a machine without
/// it turned on can only be told why — not given a version.
#[test]
fn store_version_is_a_version_or_says_why_not() {
    match store_version() {
        Ok(version) => {
            assert!(
                version.starts_with(|c: char| c.is_ascii_digit()) && version.contains('.'),
                "expected a dotted version, got {version:?}"
            );
        }
        Err(err) => {
            let chain = format!("{err:#}");
            assert!(
                chain.contains("nix-command") || chain.contains("is nix on PATH?"),
                "an unreachable store should say why, got: {chain}"
            );
        }
    }
}

#[test]
fn reads_the_experimental_features_setting() {
    let settings = Settings::parse("experimental-features = nix-command flakes\n").unwrap();

    assert_eq!(settings.experimental_features(), ["nix-command", "flakes"]);
    assert_eq!(
        NixFeatures::from_enabled(&settings.experimental_features()),
        NixFeatures {
            nix_command: true,
            flakes: true
        }
    );
}

/// What a stock NixOS looks like: the setting is there but empty, or absent
/// altogether. Slipway has to report "off" rather than fail.
#[test]
fn no_features_turned_on_is_not_a_failure() {
    for text in ["experimental-features =\n", "", "# nothing but a comment\n"] {
        let settings = Settings::parse(text).unwrap();

        assert!(
            settings.experimental_features().is_empty(),
            "expected no features from {text:?}"
        );
        assert_eq!(
            NixFeatures::from_enabled(&settings.experimental_features()),
            NixFeatures {
                nix_command: false,
                flakes: false
            }
        );
    }
}

#[test]
fn extra_appends_and_plain_replaces() {
    let appended = Settings::parse(
        "experimental-features = nix-command\nextra-experimental-features = flakes\n",
    )
    .unwrap();
    assert_eq!(appended.experimental_features(), ["nix-command", "flakes"]);

    let replaced = Settings::parse(
        "experimental-features = nix-command flakes\nexperimental-features = nix-command\n",
    )
    .unwrap();
    assert_eq!(replaced.experimental_features(), ["nix-command"]);

    // `extra-` with nothing set yet still turns the feature on.
    let only_extra = Settings::parse("extra-experimental-features = flakes\n").unwrap();
    assert_eq!(only_extra.experimental_features(), ["flakes"]);
}

#[test]
fn comments_and_other_settings_are_ignored() {
    let settings = Settings::parse(
        "# WARNING: this file is generated\n\
         trusted-public-keys = cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=\n\
         experimental-features = nix-command flakes # and fetch-tree one day\n\
         builders =\n",
    )
    .unwrap();

    assert_eq!(settings.experimental_features(), ["nix-command", "flakes"]);
    assert_eq!(
        settings.get("trusted-public-keys"),
        Some("cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="),
        "a value containing `=` should survive intact"
    );
}

/// A fake filesystem root holding `/etc/nix/nix.conf`.
fn root_with_nix_conf(contents: &str) -> TempDir {
    let root = TempDir::new().unwrap();
    let conf = root.path().join("etc/nix/nix.conf");
    fs::create_dir_all(conf.parent().unwrap()).unwrap();
    fs::write(&conf, contents).unwrap();
    root
}

#[test]
fn reads_the_system_config_file_under_a_root() {
    let root = root_with_nix_conf("experimental-features = nix-command flakes\n");

    let settings = Settings::read(root.path()).unwrap();

    assert_eq!(settings.experimental_features(), ["nix-command", "flakes"]);
}

#[test]
fn a_root_with_no_config_files_has_no_features() {
    if host_sets_nix_config() {
        return;
    }
    let root = TempDir::new().unwrap();

    let settings = Settings::read(root.path()).unwrap();

    assert!(settings.experimental_features().is_empty());
}

/// Reading under a fixture root must not reach the host's own
/// `/etc/nix/nix.conf`, or these tests would report whatever the machine
/// running them happens to have configured.
#[test]
fn the_hosts_own_config_does_not_leak_in() {
    if host_sets_nix_config() {
        return;
    }
    let root = root_with_nix_conf("experimental-features = flakes\n");

    let settings = Settings::read(root.path()).unwrap();

    assert_eq!(settings.experimental_features(), ["flakes"]);
    for key in ["substituters", "trusted-users", "sandbox"] {
        assert_eq!(
            settings.get(key),
            None,
            "{key} came from somewhere outside the fixture root"
        );
    }
}

#[test]
fn follows_include_directives() {
    let root = root_with_nix_conf("include extra.conf\n");
    fs::write(
        root.path().join("etc/nix/extra.conf"),
        "experimental-features = nix-command flakes\n",
    )
    .unwrap();

    let settings = Settings::read(root.path()).unwrap();

    assert_eq!(settings.experimental_features(), ["nix-command", "flakes"]);
}

#[test]
fn a_missing_include_is_an_error_but_a_missing_bang_include_is_not() {
    let root = root_with_nix_conf("include not-there.conf\n");
    let err =
        Settings::read(root.path()).expect_err("a required include that isn't there should fail");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("not-there.conf") && chain.contains("isn't there"),
        "the error should name the missing file, got: {chain}"
    );

    let root = root_with_nix_conf("!include not-there.conf\nexperimental-features = flakes\n");
    let settings = Settings::read(root.path()).expect("`!include` may skip a missing file");
    assert_eq!(settings.experimental_features(), ["flakes"]);
}

#[test]
fn include_loops_give_up_rather_than_hang() {
    let root = root_with_nix_conf("include nix.conf\n");

    let err = Settings::read(root.path())
        .expect_err("a file including itself should not recurse for ever");

    assert!(format!("{err:#}").contains("nest"), "got: {err:#}");
}

#[test]
fn unreadable_config_files_are_reported() {
    let root = TempDir::new().unwrap();
    // A directory where a file belongs: not "missing", so not ignorable.
    fs::create_dir_all(root.path().join("etc/nix/nix.conf")).unwrap();

    let err =
        Settings::read(root.path()).expect_err("a nix.conf that isn't a file should be reported");

    assert!(
        format!("{err:#}").contains("nix.conf"),
        "the error should name the file, got: {err:#}"
    );
}

/// `NIX_CONFIG` overrides the files, so the few tests that assert a setting
/// is *absent* can't speak for a shell that sets it.
fn host_sets_nix_config() -> bool {
    std::env::var_os("NIX_CONFIG").is_some_and(|value| !value.is_empty())
}
