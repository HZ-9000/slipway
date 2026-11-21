//! Tests for adding, removing and listing the packages Slipway looks after,
//! and for what `slipway check` makes of a folder.
//!
//! Nothing here runs Nix: every `check` case is arranged to go wrong before
//! the questions Nix answers, and the package set is pretended. The NixOS
//! VM tests in `tests/vm` are what run the real thing.

mod fixture;

use std::fs;

use anyhow::Result;
use slipway_core::{
    config::MANAGED_MARKER,
    managed::{self, Change, Outcome},
    package::{Holds, PackageName, PackageSet},
    state::State,
    system::System,
};

fn package(name: &str) -> PackageName {
    name.parse().unwrap()
}

/// A package set that answers from a list, as Nix would from a channel.
struct Channel;

impl PackageSet for Channel {
    fn holds(&self, package: &PackageName) -> Result<Holds> {
        Ok(match package.as_str() {
            "git" | "firefox" | "python3Packages.requests" => Holds::Package,
            "firefox.meta" | "python3Packages" => Holds::SomethingElse,
            _ => Holds::Nothing,
        })
    }
}

/// A system that Slipway has been connected to, with its folder written.
fn connected() -> (tempfile::TempDir, System) {
    let (root, system) = fixture::system(fixture::CHANNELS);
    let dir = fixture::managed(&system);
    State::default().config(&system).write_to(&dir).unwrap();
    (root, system)
}

fn module(system: &System) -> String {
    fs::read_to_string(fixture::managed(system).join("default.nix")).unwrap()
}

fn reason(result: Result<impl std::fmt::Debug>) -> String {
    format!("{:#}", result.expect_err("should have been refused"))
}

#[test]
fn adding_a_package_writes_the_list_and_the_nix() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);

    let change = managed::add(&dir, &system, &Channel, &package("git")).unwrap();

    assert_eq!(change, Change::Made);
    assert_eq!(
        State::load(&dir).unwrap().packages,
        [package("git")].into(),
        "the list should have been saved"
    );
    assert!(
        module(&system).contains("pkgs.git"),
        "the Nix should install it:\n{}",
        module(&system)
    );
}

#[test]
fn asking_for_a_package_twice_changes_nothing() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);
    managed::add(&dir, &system, &Channel, &package("git")).unwrap();
    let before = module(&system);

    let change = managed::add(&dir, &system, &Channel, &package("git")).unwrap();

    assert_eq!(change, Change::AlreadyDone);
    assert_eq!(module(&system), before);
    assert_eq!(State::load(&dir).unwrap().packages, [package("git")].into());
}

#[test]
fn a_name_the_channel_doesnt_have_is_refused_in_plain_language() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);

    let reason = reason(managed::add(&dir, &system, &Channel, &package("firefix")));

    assert!(reason.contains("no package called `firefix`"), "{reason}");
    assert!(reason.contains("nixos-24.11"), "{reason}");
    assert!(reason.contains("search.nixos.org"), "{reason}");
    assert!(
        State::load(&dir).unwrap().packages.is_empty(),
        "a name that was refused shouldn't reach the list"
    );
    assert!(
        !module(&system).contains("firefix"),
        "a name that was refused shouldn't reach the Nix"
    );
}

#[test]
fn a_name_that_isnt_a_package_is_refused_too() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);

    let reason = reason(managed::add(
        &dir,
        &system,
        &Channel,
        &package("firefox.meta"),
    ));

    assert!(
        reason.contains("isn't a package you can install"),
        "{reason}"
    );
}

#[test]
fn removing_a_package_takes_it_out_of_both() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);
    managed::add(&dir, &system, &Channel, &package("git")).unwrap();
    managed::add(&dir, &system, &Channel, &package("firefox")).unwrap();

    let change = managed::remove(&dir, &system, &package("git")).unwrap();

    assert_eq!(change, Change::Made);
    assert_eq!(
        State::load(&dir).unwrap().packages,
        [package("firefox")].into()
    );
    let module = module(&system);
    assert!(module.contains("pkgs.firefox"), "{module}");
    assert!(!module.contains("pkgs.git"), "{module}");
}

#[test]
fn removing_a_package_slipway_doesnt_look_after_says_where_to_look() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);

    let reason = reason(managed::remove(&dir, &system, &package("vim")));

    assert!(reason.contains("isn't looking after `vim`"), "{reason}");
    assert!(reason.contains("slipway list"), "{reason}");
    assert!(reason.contains("your own settings"), "{reason}");
}

/// The user's own `environment.systemPackages` is theirs to change, so it
/// is no part of what Slipway lists.
#[test]
fn listing_shows_only_the_packages_slipway_looks_after() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);
    managed::add(&dir, &system, &Channel, &package("git")).unwrap();

    let packages = managed::packages(&dir, &system).unwrap();

    assert_eq!(packages, [package("git")]);
}

#[test]
fn listing_is_in_the_order_it_is_written() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);
    for name in ["python3Packages.requests", "git", "firefox"] {
        managed::add(&dir, &system, &Channel, &package(name)).unwrap();
    }

    let packages = managed::packages(&dir, &system).unwrap();

    assert_eq!(
        packages,
        ["firefox", "git", "python3Packages.requests"].map(package)
    );
}

#[test]
fn a_computer_that_isnt_set_up_is_told_to_connect_first() {
    let (_root, system) = fixture::system(fixture::NOT_CONNECTED);
    let dir = fixture::managed(&system);

    for reason in [
        reason(managed::add(&dir, &system, &Channel, &package("git"))),
        reason(managed::remove(&dir, &system, &package("git"))),
        reason(managed::packages(&dir, &system)),
    ] {
        assert!(reason.contains("run `slipway connect` first"), "{reason}");
        assert!(reason.contains("configuration.nix"), "{reason}");
    }
    assert!(
        !dir.exists(),
        "nothing should have been written to {}",
        dir.display()
    );
}

fn outcomes(report: &slipway_core::managed::Report) -> Vec<&'static str> {
    report
        .checks()
        .iter()
        .map(|check| match check.outcome {
            Outcome::Passed => "ok",
            Outcome::Failed(_) => "problem",
            Outcome::NotChecked => "skipped",
        })
        .collect()
}

fn problem(report: &slipway_core::managed::Report) -> String {
    report
        .checks()
        .iter()
        .find_map(|check| match &check.outcome {
            Outcome::Failed(problem) => Some(format!("{problem:#}")),
            _ => None,
        })
        .expect("something should have gone wrong")
}

#[test]
fn checking_a_computer_that_isnt_set_up_stops_at_the_first_problem() {
    let (_root, system) = fixture::system(fixture::NOT_CONNECTED);

    let report = slipway_core::check(&system);

    assert_eq!(
        outcomes(&report),
        [
            "problem", "skipped", "skipped", "skipped", "skipped", "skipped"
        ]
    );
    assert!(!report.passed());
    let problem = problem(&report);
    assert!(
        problem.contains("isn't set up on this computer yet"),
        "{problem}"
    );
    assert!(problem.contains("slipway connect"), "{problem}");
}

#[test]
fn checking_notices_a_folder_slipway_didnt_write() {
    let (_root, system) = connected();
    fs::write(fixture::managed(&system).join("default.nix"), "{ }\n").unwrap();

    let report = slipway_core::check(&system);

    let problem = problem(&report);
    assert!(problem.contains("wasn't written by Slipway"), "{problem}");
}

#[test]
fn checking_notices_nix_that_no_longer_matches_the_list() {
    let (_root, system) = connected();
    let dir = fixture::managed(&system);
    managed::add(&dir, &system, &Channel, &package("git")).unwrap();
    // A hand-written line, kept under Slipway's own first line so the file
    // still looks like its work.
    let edited = format!("{}\n# mine\n", module(&system).trim_end());
    assert!(edited.starts_with(MANAGED_MARKER));
    fs::write(dir.join("default.nix"), edited).unwrap();

    let report = slipway_core::check(&system);

    assert_eq!(
        outcomes(&report),
        ["ok", "ok", "problem", "skipped", "skipped", "skipped"]
    );
    let problem = problem(&report);
    assert!(
        problem.contains("isn't what Slipway would write"),
        "{problem}"
    );
    assert!(problem.contains("slipway list"), "{problem}");
}

#[test]
fn checking_notices_a_folder_nothing_brings_in() {
    let (_root, system) = fixture::system(fixture::NOT_CONNECTED);
    let dir = fixture::managed(&system);
    State::default().config(&system).write_to(&dir).unwrap();

    let report = slipway_core::check(&system);

    assert_eq!(
        outcomes(&report),
        ["ok", "ok", "ok", "problem", "skipped", "skipped"]
    );
    let problem = problem(&report);
    assert!(problem.contains("doesn't bring in"), "{problem}");
    assert!(problem.contains("slipway connect"), "{problem}");
}

#[test]
fn checking_says_what_it_looked_at_in_plain_language() {
    let (_root, system) = fixture::system(fixture::NOT_CONNECTED);

    let report = slipway_core::check(&system);

    for check in report.checks() {
        assert!(!check.what.is_empty());
        for jargon in ["derivation", "attribute", "nixpkgs", "closure"] {
            assert!(
                !check.what.to_lowercase().contains(jargon),
                "`{}` mentions `{jargon}`",
                check.what
            );
        }
    }
}
