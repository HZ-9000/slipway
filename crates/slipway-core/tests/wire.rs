//! Tests for connecting a user's configuration to Slipway's managed folder.
//!
//! The fixture configurations are the ones the NixOS VM tests boot, in
//! `tests/vm/fixtures`. Their connected versions are compared against
//! `tests/golden/wire/<fixture>/`; after an intended change, regenerate them
//! with `UPDATE_GOLDEN=1 cargo test -p slipway-core --test wire`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use slipway_core::{
    system::{ConfigKind, SystemConfig},
    wire::{self, CantWire, Link},
};
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/vm/fixtures")
        .join(name)
}

fn link() -> Link {
    Link {
        relative: "./slipway".to_owned(),
        absolute: PathBuf::from("/etc/nixos/slipway"),
    }
}

/// Connects one kind of configuration file.
type Wire = fn(&str) -> Result<Option<String>, CantWire>;

fn wire_channels(source: &str) -> Result<Option<String>, CantWire> {
    wire::wire_configuration_nix(source, &link())
}

fn wire_flake(source: &str) -> Result<Option<String>, CantWire> {
    wire::wire_flake(source, "./slipway")
}

/// Connects `source`, checking what must hold for every edit: the original
/// text survives untouched, and connecting again changes nothing.
fn connected(source: &str, wire: Wire) -> String {
    let after = wire(source)
        .unwrap_or_else(|err| panic!("refused: {err}"))
        .expect("should need connecting");
    assert_only_adds(source, &after);
    assert_eq!(
        wire(&after).unwrap(),
        None,
        "connecting twice changed it again:\n{after}"
    );
    after
}

/// Fails unless `after` is `before` with text inserted, so nothing the user
/// wrote was moved, reformatted or removed.
fn assert_only_adds(before: &str, after: &str) {
    let mut rest = after.chars();
    for c in before.chars() {
        assert!(
            rest.any(|d| d == c),
            "the edit changed the user's text rather than only adding to it:\n--- before\n{before}\n--- after\n{after}"
        );
    }
}

fn refusal(result: Result<Option<String>, CantWire>) -> CantWire {
    result.expect_err("should refuse to edit")
}

#[test]
fn connected_fixtures_match_golden_files() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let cases: [(&str, &str, Wire); 4] = [
        ("fresh-channels", "configuration.nix", wire_channels),
        ("hand-edited", "configuration.nix", wire_channels),
        ("fresh-flake", "flake.nix", wire_flake),
        ("existing-imports", "flake.nix", wire_flake),
    ];

    for (name, file, wire) in cases {
        let source = fs::read_to_string(fixture(name).join(file)).unwrap();
        let after = connected(&source, wire);

        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/wire")
            .join(name)
            .join(file);
        if update {
            fs::create_dir_all(golden.parent().unwrap()).unwrap();
            fs::write(&golden, &after).unwrap();
        } else {
            let expected = fs::read_to_string(&golden).unwrap_or_else(|_| {
                panic!("missing {}; run with UPDATE_GOLDEN=1", golden.display())
            });
            assert_eq!(
                after, expected,
                "{name}/{file} differs from its golden file"
            );
        }
    }
}

#[test]
fn an_already_connected_fixture_is_left_alone() {
    let source = fs::read_to_string(fixture("already-connected/configuration.nix")).unwrap();

    assert_eq!(wire_channels(&source).unwrap(), None);
}

#[test]
fn recognises_every_spelling_of_the_import() {
    for import in ["./slipway", "./slipway/default.nix", "/etc/nixos/slipway"] {
        let source = format!("{{ ... }}:\n{{\n  imports = [ ./hardware.nix {import} ];\n}}\n");
        assert_eq!(wire_channels(&source).unwrap(), None, "{import}");
    }
}

#[test]
fn adds_imports_when_there_are_none() {
    let after = connected(
        "{ pkgs, ... }:\n\n{\n  # Bootloader.\n  boot.loader.grub.enable = true;\n}\n",
        wire_channels,
    );

    assert_eq!(
        after,
        "{ pkgs, ... }:\n\n{\n  imports = [ ./slipway ];\n  # Bootloader.\n  boot.loader.grub.enable = true;\n}\n"
    );
}

#[test]
fn adds_to_imports_in_whatever_layout_they_have() {
    for (before, after) in [
        (
            "{ imports = [ ./a.nix ]; }",
            "{ imports = [ ./slipway ./a.nix ]; }",
        ),
        ("{ imports = [ ]; }", "{ imports = [ ./slipway ]; }"),
        ("{ imports = []; }", "{ imports = [ ./slipway ]; }"),
        (
            "{\n  imports = [\n  ];\n}",
            "{\n  imports = [\n    ./slipway\n  ];\n}",
        ),
        (
            "{\n  imports = [\n    ./a.nix\n    ./b.nix\n  ];\n}",
            "{\n  imports = [\n    ./slipway\n    ./a.nix\n    ./b.nix\n  ];\n}",
        ),
    ] {
        assert_eq!(connected(before, wire_channels), after);
    }
}

#[test]
fn looks_past_let_and_with_to_the_settings() {
    let after = connected(
        "{ pkgs, ... }:\nlet\n  x = 1;\nin\nwith pkgs; {\n  imports = [ ./a.nix ];\n}\n",
        wire_channels,
    );

    assert!(
        after.contains("imports = [ ./slipway ./a.nix ];"),
        "{after}"
    );
}

#[test]
fn refuses_configuration_nix_it_cant_edit_safely() {
    for (source, why) in [
        ("import ./machines/laptop.nix", "doesn't directly return"),
        (
            "{ lib, ... }: lib.mkMerge [ { } ]",
            "doesn't directly return",
        ),
        ("{ imports = [ ./a.nix ] ++ extra; }", "isn't a plain list"),
        ("{ inherit (x) imports; }", "inherited"),
        ("{ imports = [ ./a.nix ]; imports = [ ]; }", "in pieces"),
        ("{ imports = [ ./a.nix ];", "syntax error"),
    ] {
        let err = refusal(wire_channels(source));
        assert!(err.reason.contains(why), "{source}: {err}");
        assert!(err.by_hand.contains("imports = [ ./slipway ]"), "{err}");
    }
}

#[test]
fn adds_an_input_and_module_to_a_flake() {
    let after = connected(
        r#"{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs, ... }: {
    nixosConfigurations.laptop = nixpkgs.lib.nixosSystem {
      modules = [
        ./configuration.nix
      ];
    };
  };
}
"#,
        wire_flake,
    );

    assert_eq!(
        after,
        r#"{
  inputs = {
    slipway.url = "path:./slipway";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs, slipway, ... }: {
    nixosConfigurations.laptop = nixpkgs.lib.nixosSystem {
      modules = [
        slipway.nixosModules.default
        ./configuration.nix
      ];
    };
  };
}
"#
    );
}

#[test]
fn adds_an_argument_to_outputs_laid_out_one_per_line() {
    let after = connected(
        r#"{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    {
      self,
      nixpkgs,
      ...
    }@inputs:
    {
      nixosConfigurations.nixos = nixpkgs.lib.nixosSystem {
        specialArgs = { inherit inputs; };
        modules = [ ./configuration.nix ];
      };
    };
}
"#,
        wire_flake,
    );

    assert!(
        after.contains("      nixpkgs,\n      slipway,\n      ...\n"),
        "{after}"
    );
    assert!(
        after.contains("inputs.slipway.url = \"path:./slipway\";\n  inputs.nixpkgs.url"),
        "{after}"
    );
    assert!(
        after.contains("modules = [ slipway.nixosModules.default ./configuration.nix ];"),
        "{after}"
    );
}

#[test]
fn uses_the_inputs_argument_when_outputs_takes_them_whole() {
    let after = connected(
        "{\n  inputs.nixpkgs.url = \"x\";\n  outputs = inputs: {\n    nixosConfigurations.nixos = inputs.nixpkgs.lib.nixosSystem { modules = [ ./c.nix ]; };\n  };\n}\n",
        wire_flake,
    );

    assert!(
        after.contains("modules = [ inputs.slipway.nixosModules.default ./c.nix ];"),
        "{after}"
    );
}

#[test]
fn finishes_a_half_connected_flake() {
    let after = connected(
        "{\n  inputs.slipway.url = \"path:./slipway\";\n  outputs = { self, slipway }: {\n    nixosConfigurations.nixos = lib.nixosSystem { modules = [ ./c.nix ]; };\n  };\n}\n",
        wire_flake,
    );

    assert_eq!(
        after,
        "{\n  inputs.slipway.url = \"path:./slipway\";\n  outputs = { self, slipway }: {\n    nixosConfigurations.nixos = lib.nixosSystem { modules = [ slipway.nixosModules.default ./c.nix ]; };\n  };\n}\n"
    );
}

#[test]
fn refuses_flakes_it_cant_edit_safely() {
    for (source, why) in [
        (
            "{ outputs = { nixpkgs, ... }: { a = nixpkgs.lib.nixosSystem { modules = [ ]; }; b = nixpkgs.lib.nixosSystem { modules = [ ]; }; }; }",
            "more than one system",
        ),
        (
            "{ outputs = { flake-parts, ... }@inputs: flake-parts.lib.mkFlake { inherit inputs; } { }; }",
            "couldn't find a `nixosSystem`",
        ),
        (
            "{ outputs = { nixpkgs, ... }: { a = nixpkgs.lib.nixosSystem { modules = common ++ [ ./c.nix ]; }; }; }",
            "`modules` isn't a plain list",
        ),
        (
            "{ inputs = import ./inputs.nix; outputs = _: { }; }",
            "isn't a plain set",
        ),
        ("{ description = \"no outputs\"; }", "no `outputs`"),
        (
            "let x = 1; in { outputs = _: { }; }",
            "isn't a plain set of inputs",
        ),
    ] {
        let err = refusal(wire_flake(source));
        assert!(err.reason.contains(why), "{source}: {err}");
        assert!(err.by_hand.contains("inputs.slipway.url"), "{err}");
    }
}

fn config_in(dir: &Path, file: &str, contents: &str) -> SystemConfig {
    fs::write(dir.join(file), contents).unwrap();
    let dir = fs::canonicalize(dir).unwrap();
    SystemConfig {
        entry: dir.join(file),
        dir,
        kind: if file == "flake.nix" {
            ConfigKind::Flake
        } else {
            ConfigKind::ConfigurationNix
        },
    }
}

#[test]
fn plans_and_applies_the_edit_to_the_file() {
    let etc_nixos = TempDir::new().unwrap();
    let config = config_in(
        etc_nixos.path(),
        "configuration.nix",
        "{ ... }:\n{\n  imports = [ ./hardware.nix ];\n}\n",
    );

    let wiring = wire::plan(&config, &etc_nixos.path().join("slipway"))
        .unwrap()
        .expect("should need connecting");
    assert!(
        wiring
            .diff()
            .contains("+  imports = [ ./slipway ./hardware.nix ];")
    );
    wiring.apply().unwrap();

    assert_eq!(fs::read_to_string(&config.entry).unwrap(), wiring.after);
    assert_eq!(
        wire::plan(&config, &etc_nixos.path().join("slipway")).unwrap(),
        None,
        "planning again after applying should find nothing to do"
    );
}

#[test]
fn wont_apply_over_a_file_that_changed_since_planning() {
    let etc_nixos = TempDir::new().unwrap();
    let config = config_in(etc_nixos.path(), "configuration.nix", "{ }\n");
    let wiring = wire::plan(&config, &etc_nixos.path().join("slipway"))
        .unwrap()
        .unwrap();

    fs::write(&config.entry, "{ networking.hostName = \"new\"; }\n").unwrap();

    let err = wiring.apply().unwrap_err();
    assert!(
        format!("{err:#}").contains("changed while Slipway"),
        "{err:#}"
    );
    assert_eq!(
        fs::read_to_string(&config.entry).unwrap(),
        "{ networking.hostName = \"new\"; }\n"
    );
}

#[test]
fn refuses_a_managed_folder_outside_the_config_folder() {
    let etc_nixos = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let config = config_in(etc_nixos.path(), "configuration.nix", "{ }\n");

    let err = wire::plan(&config, &elsewhere.path().join("slipway")).unwrap_err();

    let cant = err.downcast_ref::<CantWire>().expect("should be CantWire");
    assert!(cant.reason.contains("isn't inside"), "{cant}");
}

#[test]
fn a_refusal_names_the_file_and_says_what_to_add() {
    let etc_nixos = TempDir::new().unwrap();
    let config = config_in(
        etc_nixos.path(),
        "configuration.nix",
        "import ./elsewhere.nix\n",
    );

    let err = wire::plan(&config, &etc_nixos.path().join("slipway")).unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("configuration.nix"), "{message}");
    assert!(message.contains("won't edit it"), "{message}");
    assert!(message.contains("imports = [ ./slipway ]"), "{message}");
}
