//! Tests for the list of packages Slipway keeps in its managed folder.

mod fixture;

use std::fs;

use slipway_core::{
    config::OutputMode,
    package::PackageName,
    state::{STATE_FILE, State},
};
use tempfile::TempDir;

fn package(name: &str) -> PackageName {
    name.parse().unwrap()
}

fn saved(packages: &[&str]) -> (TempDir, State) {
    let dir = TempDir::new().unwrap();
    let state = State {
        packages: packages.iter().map(|name| package(name)).collect(),
    };
    state.save(dir.path()).unwrap();
    (dir, state)
}

#[test]
fn a_folder_without_a_list_is_a_fresh_start() {
    let dir = TempDir::new().unwrap();

    let state = State::load(dir.path()).unwrap();

    assert_eq!(state, State::default());
    assert!(state.packages.is_empty());
}

#[test]
fn what_is_saved_is_what_is_read_back() {
    let (dir, state) = saved(&["git", "firefox", "python3Packages.requests"]);

    assert_eq!(State::load(dir.path()).unwrap(), state);
}

#[test]
fn the_file_is_named_and_laid_out_for_people_to_read() {
    let (dir, _) = saved(&["git", "firefox"]);

    let text = fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();

    assert!(text.ends_with('\n'), "should end with a newline: {text:?}");
    assert!(
        text.contains('\n'),
        "should be laid out over lines: {text:?}"
    );
    // Sorted, so two computers with the same packages have the same file.
    assert!(
        text.find("\"firefox\"") < text.find("\"git\""),
        "packages should be in order: {text}"
    );
}

#[test]
fn saving_replaces_the_file_and_leaves_no_leftovers() {
    let (dir, _) = saved(&["git"]);
    State {
        packages: [package("firefox")].into(),
    }
    .save(dir.path())
    .unwrap();

    assert_eq!(
        State::load(dir.path()).unwrap().packages,
        [package("firefox")].into()
    );
    let left: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, [STATE_FILE]);
}

#[test]
fn says_so_when_the_file_has_been_changed_by_something_else() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(STATE_FILE), "not json at all\n").unwrap();

    let reason = format!("{:#}", State::load(dir.path()).unwrap_err());

    assert!(reason.contains("something else has changed it"), "{reason}");
    assert!(reason.contains(STATE_FILE), "{reason}");
}

#[test]
fn refuses_a_list_from_a_newer_slipway_rather_than_guessing() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(STATE_FILE),
        r#"{ "version": 99, "packages": [] }"#,
    )
    .unwrap();

    let reason = format!("{:#}", State::load(dir.path()).unwrap_err());

    assert!(reason.contains("newer version of Slipway"), "{reason}");
}

#[test]
fn refuses_a_list_holding_something_that_isnt_a_package_name() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(STATE_FILE),
        r#"{ "version": 1, "packages": ["fire fox"] }"#,
    )
    .unwrap();

    let reason = format!("{:#}", State::load(dir.path()).unwrap_err());

    assert!(reason.contains("`fire fox`"), "{reason}");
    assert!(reason.contains("isn't a package name"), "{reason}");
}

#[test]
fn the_config_it_describes_takes_the_channel_from_the_computer() {
    let (_dir, state) = saved(&["git"]);
    let (_root, system) = fixture::system(fixture::CHANNELS);

    let config = state.config(&system);

    assert_eq!(config.channel.as_str(), "nixos-24.11");
    assert_eq!(config.output_mode, OutputMode::Channels);
    assert_eq!(config.packages, state.packages);
}
