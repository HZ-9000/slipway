//! Tests for the `nix` module and the config-path helper.

use slipway_core::{
    config_dir,
    nix::{
        eval::{Nix, linked_version},
        install::nix_cli_version,
    },
};

#[test]
fn config_dir_is_under_home() {
    let home = std::env::var("HOME").expect("tests run with HOME set");
    let dir = config_dir().expect("HOME is set, so this resolves");

    assert!(dir.starts_with(&home), "{dir:?} should live under {home:?}");
    assert!(dir.ends_with(".config/slipway"), "unexpected path: {dir:?}");
}

#[test]
fn linked_version_looks_like_a_version() {
    let version = linked_version();

    assert!(
        !version.is_empty(),
        "linked Nix version should not be empty"
    );
    assert!(
        version.starts_with(|c: char| c.is_ascii_digit()),
        "expected a leading digit, got {version:?}"
    );
    assert!(
        version.contains('.'),
        "expected a dotted version, got {version:?}"
    );
}

#[test]
fn dummy_store_opens_and_reports_a_version() {
    let nix = Nix::open_uri(Some("dummy://")).expect("the dummy store always opens");

    nix.store_version().expect("querying the dummy store");
}

#[test]
fn store_accessor_exposes_the_same_store() {
    let nix = Nix::open_uri(Some("dummy://")).unwrap();

    assert_eq!(
        nix.store().version().unwrap(),
        nix.store_version().unwrap(),
        "store() and store_version() should agree"
    );

    assert!(
        std::sync::Arc::strong_count(nix.context()) >= 2,
        "the opened store should share ownership of the context"
    );
}

#[test]
fn unknown_store_scheme_is_an_error() {
    let err = Nix::open_uri(Some("not-a-real-scheme://"))
        .err()
        .expect("an unknown scheme cannot be opened");

    let chain = format!("{err:#}");
    assert!(
        chain.contains("opening Nix store"),
        "error should carry our context, got: {chain}"
    );
    assert!(
        chain.contains("not-a-real-scheme"),
        "error should name the offending scheme, got: {chain}"
    );
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
