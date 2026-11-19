//! End-to-end tests driving the `slipway` binary.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn slipway_with_home(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("slipway").expect("binary builds");
    cmd.env("HOME", home.path());
    cmd
}

#[test]
fn requires_a_subcommand() {
    Command::cargo_bin("slipway")
        .unwrap()
        .assert()
        .failure()
        .stderr(contains("Usage:"));
}

#[test]
fn help_lists_every_subcommand() {
    let output = Command::cargo_bin("slipway")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);

    for subcommand in ["detect", "add", "remove", "list", "check", "configure"] {
        assert!(
            help.contains(subcommand),
            "--help is missing `{subcommand}`:\n{help}"
        );
    }
}

#[test]
fn help_avoids_unexplained_nix_jargon() {
    let mut text = String::new();
    for args in [
        vec!["--help"],
        vec!["detect", "--help"],
        vec!["add", "--help"],
        vec!["remove", "--help"],
        vec!["list", "--help"],
        vec!["check", "--help"],
        vec!["configure", "--help"],
    ] {
        let output = Command::cargo_bin("slipway")
            .unwrap()
            .args(&args)
            .output()
            .unwrap();
        text.push_str(&String::from_utf8_lossy(&output.stdout).to_lowercase());
    }

    for jargon in [
        "flake",
        "derivation",
        "attribute",
        "nixpkgs",
        "store",
        "closure",
    ] {
        assert!(
            !text.contains(jargon),
            "--help mentions `{jargon}`:\n{text}"
        );
    }
}

#[test]
fn reports_the_crate_version() {
    Command::cargo_bin("slipway")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn rejects_an_unknown_subcommand() {
    Command::cargo_bin("slipway")
        .unwrap()
        .arg("teleport")
        .assert()
        .failure();
}

#[test]
fn configure_without_init_is_rejected() {
    let home = TempDir::new().unwrap();

    slipway_with_home(&home)
        .arg("configure")
        .assert()
        .failure()
        .stderr(contains("--init"));

    assert!(
        !home.path().join(".config/slipway").exists(),
        "`configure` without `--init` must not create the config directory"
    );
}

#[test]
fn configure_init_creates_the_config_directory() {
    let home = TempDir::new().unwrap();

    slipway_with_home(&home)
        .args(["configure", "--init"])
        .assert()
        .success();

    assert!(
        home.path().join(".config/slipway").is_dir(),
        "`configure --init` should create $HOME/.config/slipway"
    );
}

#[test]
fn configure_init_accepts_the_short_flag() {
    let home = TempDir::new().unwrap();

    slipway_with_home(&home)
        .args(["configure", "-i"])
        .assert()
        .success();

    assert!(home.path().join(".config/slipway").is_dir());
}

#[test]
fn configure_init_is_idempotent() {
    let home = TempDir::new().unwrap();

    for _ in 0..2 {
        slipway_with_home(&home)
            .args(["configure", "--init"])
            .assert()
            .success();
    }

    assert!(home.path().join(".config/slipway").is_dir());
}

#[test]
fn configure_init_probes_both_nix_sources() {
    let home = TempDir::new().unwrap();

    slipway_with_home(&home)
        .args(["configure", "--init"])
        .assert()
        .success()
        .stdout(contains("Nix (on PATH):"))
        .stdout(contains("Nix (store):"));
}

#[test]
fn configure_init_fails_when_home_is_unset() {
    Command::cargo_bin("slipway")
        .unwrap()
        .env_remove("HOME")
        .args(["configure", "--init"])
        .assert()
        .failure()
        .stderr(contains("HOME is not set"));
}

fn running_on_nixos() -> bool {
    std::fs::read_to_string("/etc/os-release")
        .is_ok_and(|text| text.lines().any(|line| line == "ID=nixos"))
}

#[test]
fn detect_reports_the_system_on_nixos_and_refuses_elsewhere() {
    let assert = Command::cargo_bin("slipway")
        .unwrap()
        .arg("detect")
        .assert();

    if running_on_nixos() {
        assert
            .success()
            .stdout(contains("NixOS:"))
            .stdout(contains("Channel:"))
            .stdout(contains("System config:"))
            .stdout(contains("flakes:"))
            .stdout(contains("Nix (on PATH):"));
    } else {
        assert
            .code(1)
            .stderr(contains("Slipway only works on NixOS"));
    }
}

#[test]
fn detect_accepts_a_config_path() {
    let help = Command::cargo_bin("slipway")
        .unwrap()
        .args(["detect", "--help"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&help.stdout).contains("--config <PATH>"));

    Command::cargo_bin("slipway")
        .unwrap()
        .args(["detect", "--config"])
        .assert()
        .code(2);
}

#[test]
fn add_and_remove_require_a_package_name() {
    for subcommand in ["add", "remove"] {
        Command::cargo_bin("slipway")
            .unwrap()
            .arg(subcommand)
            .assert()
            .code(2)
            .stderr(contains("<PACKAGE>"));
    }
}

#[test]
fn add_and_remove_reject_an_invalid_package_name_in_plain_language() {
    for subcommand in ["add", "remove"] {
        Command::cargo_bin("slipway")
            .unwrap()
            .args([subcommand, "fire fox"])
            .assert()
            .code(2)
            .stderr(contains("package names can't contain spaces"));
    }
}

#[test]
fn stubbed_commands_say_they_are_not_available_yet() {
    for args in [
        vec!["add", "firefox"],
        vec!["remove", "python3Packages.requests"],
        vec!["list"],
        vec!["check"],
    ] {
        Command::cargo_bin("slipway")
            .unwrap()
            .args(&args)
            .assert()
            .code(1)
            .stderr(contains("isn't available in this version of slipway yet"));
    }
}

#[test]
fn commands_without_arguments_reject_extras() {
    for subcommand in ["list", "check", "detect"] {
        Command::cargo_bin("slipway")
            .unwrap()
            .args([subcommand, "extra"])
            .assert()
            .code(2);
    }
}
