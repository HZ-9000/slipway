//! Tests for the config model and the Nix it generates.
//!
//! Generated files are compared against `tests/golden/<case>/`. After an
//! intended change to the output, regenerate them with
//! `UPDATE_GOLDEN=1 cargo test -p slipway-core --test config`.

use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

use slipway_core::config::{Config, MANAGED_MARKER, NixValue, OutputMode};

fn golden_dir(case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(case)
}

fn empty(output_mode: OutputMode) -> Config {
    Config::new("nixos-24.11".parse().unwrap(), output_mode)
}

fn full(output_mode: OutputMode) -> Config {
    let mut config = Config::new("nixos-unstable".parse().unwrap(), output_mode);
    for package in ["git", "firefox", "python3Packages.requests", "7zz"] {
        config.packages.insert(package.parse().unwrap());
    }

    config
        .enable_module("services.openssh".parse().unwrap())
        .set("ports", vec![22_i64, 2222])
        .unwrap()
        .set(
            "settings",
            NixValue::Attrs(BTreeMap::from([
                ("PasswordAuthentication".to_owned(), false.into()),
                ("PermitRootLogin".to_owned(), "no".into()),
            ])),
        )
        .unwrap();

    config.enable_module("services.printing".parse().unwrap());

    config
        .enable_module("programs.bash".parse().unwrap())
        .set("promptInit", "PS1=\"${USER} \\$ \"\n")
        .unwrap()
        .set("shellAliases", NixValue::Attrs(BTreeMap::new()))
        .unwrap()
        .set("interactiveShellInit", Vec::<NixValue>::new())
        .unwrap()
        .set(
            "edge-cases",
            vec![
                NixValue::Int(-1),
                NixValue::Attrs(BTreeMap::from([("in".to_owned(), true.into())])),
            ],
        )
        .unwrap();

    config
}

fn cases() -> [(&'static str, Config); 3] {
    [
        ("empty_channels", empty(OutputMode::Channels)),
        ("full_channels", full(OutputMode::Channels)),
        ("full_flake", full(OutputMode::Flake)),
    ]
}

#[test]
fn generated_files_match_golden_files() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();

    for (case, config) in cases() {
        let dir = golden_dir(case);
        let files = config.render();

        if update {
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            for file in &files {
                fs::write(dir.join(file.name), &file.contents).unwrap();
            }
            continue;
        }

        let mut expected_names: Vec<_> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{case}: {e}; run with UPDATE_GOLDEN=1"))
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        expected_names.sort();
        let mut names: Vec<_> = files.iter().map(|file| file.name.to_owned()).collect();
        names.sort();
        assert_eq!(names, expected_names, "{case}: different set of files");

        for file in &files {
            let expected = fs::read_to_string(dir.join(file.name)).unwrap();
            assert_eq!(
                file.contents, expected,
                "{case}/{} differs from its golden file; if the change is intended, rerun with UPDATE_GOLDEN=1",
                file.name
            );
        }
    }
}

#[test]
fn golden_files_are_nixfmt_formatted() {
    let files: Vec<_> = cases()
        .iter()
        .flat_map(|(case, config)| {
            config
                .render()
                .into_iter()
                .map(|file| golden_dir(case).join(file.name))
        })
        .collect();

    let output = Command::new("nixfmt")
        .arg("--check")
        .args(&files)
        .output()
        .expect("`nixfmt` should be on PATH (it is in the dev shell)");

    assert!(
        output.status.success(),
        "generated Nix isn't nixfmt-formatted:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn every_file_starts_with_the_managed_header() {
    for (case, config) in cases() {
        for file in config.render() {
            assert!(
                file.contents.starts_with(&format!("{MANAGED_MARKER}\n")),
                "{case}/{} should start with the managed header",
                file.name
            );
            assert!(
                file.contents.contains("Put your own changes in"),
                "{case}/{} should say where hand edits belong",
                file.name
            );
        }
    }
}

#[test]
fn flake_mode_adds_a_flake_and_channels_mode_does_not() {
    let names = |mode| {
        empty(mode)
            .render()
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>()
    };

    assert_eq!(names(OutputMode::Channels), ["default.nix"]);
    assert_eq!(names(OutputMode::Flake), ["default.nix", "flake.nix"]);
}

#[test]
fn enable_cannot_be_set_as_an_option() {
    let mut config = empty(OutputMode::Channels);
    let err = config
        .enable_module("services.openssh".parse().unwrap())
        .set("enable", false)
        .unwrap_err();

    assert!(err.to_string().contains("`enable`"), "got: {err}");
}

#[test]
fn enabling_twice_keeps_options_and_disabling_forgets_them() {
    let mut config = empty(OutputMode::Channels);
    let path = "services.openssh".parse().unwrap();

    config
        .enable_module("services.openssh".parse().unwrap())
        .set("ports", vec![22_i64])
        .unwrap();
    config.enable_module("services.openssh".parse().unwrap());
    assert_eq!(config.modules().next().unwrap().1.options().count(), 1);

    assert!(config.disable_module(&path));
    assert!(!config.disable_module(&path));
    assert_eq!(config.modules().count(), 0);
}

#[test]
fn rejects_bad_channels_and_option_paths() {
    for channel in ["", "NixOS-24.11", "nixos 24.11", "24.11"] {
        assert!(
            channel.parse::<slipway_core::config::Channel>().is_err(),
            "{channel:?} should be rejected"
        );
    }
    for path in [
        "",
        ".services",
        "services.",
        "services..openssh",
        "services.open ssh",
    ] {
        assert!(
            path.parse::<slipway_core::config::AttrPath>().is_err(),
            "{path:?} should be rejected"
        );
    }
}

#[test]
fn write_to_creates_the_managed_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = full(OutputMode::Flake);

    config.write_to(dir.path()).unwrap();

    for file in config.render() {
        assert_eq!(
            fs::read_to_string(dir.path().join(file.name)).unwrap(),
            file.contents
        );
    }
}

#[test]
fn write_to_removes_files_it_no_longer_needs() {
    let dir = tempfile::tempdir().unwrap();

    full(OutputMode::Flake).write_to(dir.path()).unwrap();
    full(OutputMode::Channels).write_to(dir.path()).unwrap();

    assert!(dir.path().join("default.nix").exists());
    assert!(!dir.path().join("flake.nix").exists());
}

#[test]
fn write_to_refuses_to_replace_files_it_did_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let hand_written = "{ ... }: { }\n";
    fs::write(dir.path().join("flake.nix"), hand_written).unwrap();

    let err = full(OutputMode::Channels).write_to(dir.path()).unwrap_err();

    assert!(
        format!("{err:#}").contains("wasn't written by Slipway"),
        "got: {err:#}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("flake.nix")).unwrap(),
        hand_written
    );
    assert!(!dir.path().join("default.nix").exists());
}
