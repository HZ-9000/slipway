//! Tests for package-name validation.

use slipway_core::package::PackageName;

fn reason(name: &str) -> String {
    name.parse::<PackageName>()
        .expect_err("name should be rejected")
        .to_string()
}

#[test]
fn accepts_ordinary_package_names() {
    for name in [
        "firefox",
        "git",
        "python3Packages.requests",
        "_1password",
        "7zz",
        "gnome-shell",
    ] {
        let package: PackageName = name.parse().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(package.as_str(), name);
        assert_eq!(package.to_string(), name);
    }
}

#[test]
fn rejects_an_empty_name() {
    assert!(reason("").contains("can't be empty"));
}

#[test]
fn rejects_spaces() {
    assert!(reason("fire fox").contains("spaces"));
}

#[test]
fn rejects_odd_characters_and_names_them() {
    let reason = reason("firefox!");
    assert!(reason.contains("`!`"), "got: {reason}");
}

#[test]
fn rejects_misplaced_dots() {
    for name in [".firefox", "firefox.", "python3Packages..requests"] {
        assert!(reason(name).contains("dot"), "{name} should be rejected");
    }
}

#[test]
fn rejects_a_leading_dash() {
    assert!(reason("python3Packages.-requests").contains("`-`"));
}
