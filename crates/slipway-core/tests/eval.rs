//! Tests for the layer that asks Nix about the user's system: what it makes
//! of Nix's answers, and what it refuses to ask in the first place.
//!
//! Nothing here runs Nix. The cases that need a real one are the NixOS VM
//! tests in `tests/vm`, which evaluate and dry-build fixture configurations
//! on a machine with and without Nix's new command line.

mod fixture;

use slipway_core::{
    managed::Outcome,
    nix::{
        cli::Why,
        eval::{Failure, Planned, Step, Target},
    },
};

/// What Nix says on a machine that hasn't turned the feature on.
const NIX_COMMAND_OFF: &str = "error: experimental Nix feature 'nix-command' is disabled; add '--extra-experimental-features nix-command' to enable it";
const FLAKES_OFF: &str = "error: experimental Nix feature 'flakes' is disabled; add '--extra-experimental-features flakes' to enable it";

#[test]
fn a_feature_that_is_off_is_told_apart_from_a_refusal() {
    assert_eq!(
        Why::from_output(NIX_COMMAND_OFF),
        Why::FeatureOff {
            feature: "nix-command".to_owned()
        }
    );
    assert_eq!(
        Why::from_output(FLAKES_OFF),
        Why::FeatureOff {
            feature: "flakes".to_owned()
        }
    );
}

#[test]
fn anything_else_nix_says_counts_as_a_refusal() {
    for output in [
        "error:\n       … while evaluating the option `fileSystems.\"/\".fsType'",
        "error: attribute 'firefix' missing",
        "",
    ] {
        assert_eq!(Why::from_output(output), Why::Refused, "{output:?}");
    }
}

#[test]
fn a_dry_build_says_how_much_there_is_to_do() {
    let planned = Planned::read(
        "these 61 derivations will be built:\n  \
           /nix/store/aaa.drv\n  \
           /nix/store/bbb.drv\n\
         these 23 paths will be fetched (174.80 MiB download, 1234.00 MiB unpacked):\n  \
           /nix/store/ccc\n",
    );

    assert_eq!(planned.to_build, Some(61));
    assert_eq!(planned.to_fetch, Some(23));
    assert_eq!(planned.download.as_deref(), Some("174.80 MiB"));
    assert!(!planned.nothing_to_do());
    assert_eq!(
        planned.to_string(),
        "61 to build; 23 to download, 174.80 MiB"
    );
}

#[test]
fn a_dry_build_with_one_thing_to_do_still_counts_it() {
    let planned = Planned::read("this derivation will be built:\n  /nix/store/aaa.drv\n");

    assert_eq!(planned.to_build, Some(1));
    assert_eq!(planned.to_fetch, None);
    assert_eq!(planned.to_string(), "1 to build");
}

/// Nix's older command line leaves the count out of some of its wordings,
/// which is no reason to lose the rest of what it said.
#[test]
fn a_dry_build_that_names_no_count_keeps_what_it_does_say() {
    let planned = Planned::read(
        "these paths will be fetched (0.12 MiB download, 0.50 MiB unpacked):\n  /nix/store/ccc\n",
    );

    assert_eq!(planned.to_fetch, None);
    assert_eq!(planned.download.as_deref(), Some("0.12 MiB"));
    assert!(!planned.nothing_to_do());
    assert_eq!(planned.to_string(), "0.12 MiB to download");
}

/// Nix says nothing at all when the system is already built, which is worth
/// saying plainly rather than reading as a failure.
#[test]
fn a_system_that_is_already_built_has_nothing_to_do() {
    let planned = Planned::read("");

    assert!(planned.nothing_to_do());
    assert_eq!(planned.to_string(), "nothing left to build");
    assert_eq!(planned.to_build, None);
}

#[test]
fn everything_nix_said_is_kept_for_a_front_end_to_show() {
    let output = "these 2 derivations will be built:\n  /nix/store/aaa.drv\n";

    assert_eq!(Planned::read(output).output, output);
}

/// A flake can only be read with Nix's new command line, and Slipway has
/// already read Nix's settings, so it says so itself instead of running a
/// command it knows will fail.
#[test]
fn a_flake_system_without_the_features_is_refused_before_nix_is_run() {
    if fixture::host_sets_nix_config() {
        return;
    }
    let (_root, system) = fixture::flake_system(fixture::CHANNELS);

    let failure = Target::for_system(&system).expect_err("the features are off in the fixture");

    assert_eq!(failure.step, Step::Read);
    assert_eq!(
        failure.why,
        Why::FeatureOff {
            feature: "nix-command".to_owned()
        }
    );
    assert_eq!(failure.command, None, "nothing should have been run");
    assert!(failure.output.is_empty());
    let said = failure.to_string();
    assert!(said.contains("reading your system setup"), "{said}");
    assert!(said.contains("`nix-command`"), "{said}");
    assert!(said.contains("isn't turned on"), "{said}");
}

/// A configuration.nix system needs none of those features: Nix's older
/// commands are always there, which is the whole reason Slipway uses them.
#[test]
fn a_configuration_nix_system_needs_no_features() {
    if fixture::host_sets_nix_config() {
        return;
    }
    let (_root, system) = fixture::system(fixture::CHANNELS);

    let target = Target::for_system(&system).expect("no features are needed");

    assert_eq!(
        target,
        Target::Channels {
            entry: system.config.entry.clone()
        }
    );
}

#[test]
fn a_refusal_shows_the_command_and_everything_nix_said() {
    let failure = Failure {
        step: Step::DryBuild,
        why: Why::Refused,
        command: Some("nix build --dry-run /etc/nixos#x".to_owned()),
        output: "error: attribute 'x' missing".to_owned(),
    };

    let said = failure.to_string();
    assert!(
        said.contains("working out what your system would build"),
        "{said}"
    );
    assert!(said.contains("nix build --dry-run /etc/nixos#x"), "{said}");
    assert!(said.contains("attribute 'x' missing"), "{said}");
}

#[test]
fn the_steps_are_named_in_plain_language() {
    for step in [Step::Read, Step::DryBuild, Step::LookUp] {
        let said = step.to_string();
        for jargon in ["derivation", "nixpkgs", "closure", "attribute"] {
            assert!(!said.contains(jargon), "`{said}` mentions `{jargon}`");
        }
    }
}

/// The front-end entry points hand the same parts back, so a TUI or GUI
/// calling them needn't read a message to know what happened.
#[test]
fn the_entry_points_keep_the_parts_of_a_failure() {
    if fixture::host_sets_nix_config() {
        return;
    }
    let (_root, system) = fixture::flake_system(fixture::CHANNELS);

    for failure in [
        slipway_core::evaluate(&system).map(|_| ()).unwrap_err(),
        slipway_core::dry_build(&system).map(|_| ()).unwrap_err(),
    ] {
        assert_eq!(
            failure.why,
            Why::FeatureOff {
                feature: "nix-command".to_owned()
            }
        );
        assert_eq!(failure.step, Step::Read);
    }
}

/// `slipway check` reports a Nix problem as one of these, so a front-end
/// reading a report can get at the parts rather than the sentence.
#[test]
fn a_problem_in_a_report_still_has_its_parts() {
    if fixture::host_sets_nix_config() {
        return;
    }
    let (_root, system) = fixture::flake_system(fixture::CHANNELS);
    let dir = slipway_core::config::managed_dir(&system.config.dir);
    slipway_core::state::State::default()
        .config(&system)
        .write_to(&dir)
        .unwrap();

    let report = slipway_core::check(&system);

    let problem = report
        .checks()
        .iter()
        .find_map(|check| match &check.outcome {
            Outcome::Failed(problem) => Some(problem),
            _ => None,
        })
        .expect("the features are off, so Nix can't read a flake");
    let failure: &Failure = problem
        .downcast_ref()
        .expect("a problem Nix reported should keep its parts");
    assert_eq!(failure.step, Step::Read);
    assert!(matches!(failure.why, Why::FeatureOff { .. }));
}
