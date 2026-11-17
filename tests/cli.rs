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
fn help_lists_the_configure_subcommand() {
    Command::cargo_bin("slipway")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("configure"));
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
fn configure_without_init_does_nothing() {
    let home = TempDir::new().unwrap();

    slipway_with_home(&home)
        .arg("configure")
        .assert()
        .success()
        .stdout(contains("Unknown Command"));

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
fn configure_init_probes_all_three_nix_sources() {
    let home = TempDir::new().unwrap();

    slipway_with_home(&home)
        .args(["configure", "--init"])
        .assert()
        .success()
        .stdout(contains("Nix (linked library):"))
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
