//! Tests for turning what Nix said into something a person can act on.
//!
//! Every sample here is what Nix really printed, from a NixOS
//! configuration broken on purpose, abridged only in the middle of its
//! stack traces.

use slipway_core::nix::{
    cli::Why,
    eval::{Failure, Step},
    explain::Explained,
};

/// What Nix prints before it gets to the point: the outermost complaint,
/// then its own source.
const TRACE: &str = "error:
       … while evaluating the attribute 'config.system.build.toplevel'
         at /nix/store/7sa2x9c9566lfxkaxs36n3z4yayl5cy4-source/lib/modules.nix:402:9:
          401|         options = checked options;
          402|         config = checked (removeAttrs config [ \"_module\" ]);
             |         ^

       (stack trace truncated; use '--show-trace' to show the full, detailed trace)
";

fn refused(step: Step, said: &str) -> Explained {
    Failure {
        step,
        why: Why::Refused,
        command: Some("nix-instantiate <nixpkgs/nixos> -A system".to_owned()),
        output: format!("{TRACE}\n{said}"),
    }
    .explain()
}

fn reading(said: &str) -> Explained {
    refused(Step::Read, said)
}

#[test]
fn a_name_nothing_defines() {
    let explained = reading(
        "       error: undefined variable 'someUndefinedThing'\n       at /etc/nixos/configuration.nix:12:30:",
    );

    assert_eq!(
        explained.problem,
        "your settings use `someUndefinedThing`, and nothing defines it"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("/etc/nixos/configuration.nix, line 12")
    );
    assert!(
        explained.next_step.contains("check the spelling"),
        "{explained:?}"
    );
}

/// Nix often knows what was meant, and its guess is better than anything
/// Slipway could add.
#[test]
fn a_name_nix_couldnt_find_passes_on_its_own_suggestion() {
    let explained = reading(
        "       error: attribute 'firefix' missing
       at /etc/nixos/configuration.nix:12:34:
           12|   environment.systemPackages = [ pkgs.firefix ];
             |                                  ^
       Did you mean one of firefox, firefly or wiremix?",
    );

    assert_eq!(
        explained.problem,
        "your settings ask for `firefix`, which Nix couldn't find"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("/etc/nixos/configuration.nix, line 12")
    );
    assert!(
        explained
            .next_step
            .contains("Did you mean one of firefox, firefly or wiremix?"),
        "{explained:?}"
    );
}

#[test]
fn a_setting_that_doesnt_exist() {
    let explained = reading(
        "       error: The option `services.notarealservice' does not exist. Definition values:",
    );

    assert_eq!(
        explained.problem,
        "nothing on this computer has a setting called `services.notarealservice`"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("the setting `services.notarealservice`")
    );
    assert!(
        explained.next_step.contains("search.nixos.org/options"),
        "{explained:?}"
    );
}

#[test]
fn a_setting_two_parts_of_the_setup_disagree_about() {
    let explained = reading(
        "       error: The option `networking.hostName' has conflicting definition values:
       - In `/etc/nixos/other.nix': \"other\"
       - In `/etc/nixos/configuration.nix': \"nixos\"
       Use `lib.mkForce value` or `lib.mkDefault value` to change the priority on any of these definitions.",
    );

    assert_eq!(
        explained.problem,
        "two parts of your setup set `networking.hostName` to different things, and Nix can't choose between them"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("/etc/nixos/other.nix and /etc/nixos/configuration.nix"),
        "it should name the files Nix named"
    );
    assert!(
        explained.next_step.contains("one place only"),
        "{explained:?}"
    );
}

#[test]
fn a_setting_given_twice_in_one_file() {
    let explained = reading(
        "       error: attribute 'networking.hostName' already defined at /etc/nixos/configuration.nix:11:3",
    );

    assert_eq!(
        explained.problem,
        "`networking.hostName` is set twice in the same file"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("/etc/nixos/configuration.nix, line 11")
    );
}

#[test]
fn a_package_that_isnt_free_software() {
    let explained = reading(
        "       error: Refusing to evaluate package 'google-chrome-157.0.8103.38' in /nix/store/7sa2x9c9566lfxkaxs36n3z4yayl5cy4-source/pkgs/by-name/go/google-chrome/package.nix:354 because it has an unfree license (‘unfree’)
       a) To temporarily allow unfree packages, you can use an environment variable",
    );

    assert_eq!(
        explained.problem,
        "`google-chrome` isn't free software, and this computer hasn't been told to allow that",
        "the version Nix tacks on isn't the package's name"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("the package `google-chrome`")
    );
    assert!(explained.next_step.contains("allowUnfree"), "{explained:?}");
    assert!(
        explained.next_step.contains("slipway remove google-chrome"),
        "{explained:?}"
    );
}

#[test]
fn a_typo_nix_cant_read_past() {
    let explained = reading(
        "       error: syntax error, unexpected '=', expecting ';'
       at /etc/nixos/configuration.nix:13:23:",
    );

    assert!(
        explained.problem.contains("unexpected '=', expecting ';'"),
        "Nix is exact about this, so its own words are worth keeping: {explained:?}"
    );
    assert_eq!(
        explained.affects.as_deref(),
        Some("/etc/nixos/configuration.nix, line 13")
    );
    assert!(explained.next_step.contains("missing `;`"), "{explained:?}");
}

/// A problem Slipway doesn't know by name keeps Nix's own words rather than
/// a guess, and still says where to look for the rest.
#[test]
fn anything_else_keeps_what_nix_said() {
    let explained =
        reading("       error: cannot connect to socket at '/nix/var/nix/daemon-socket/socket'");

    assert!(
        explained
            .problem
            .contains("cannot connect to socket at '/nix/var/nix/daemon-socket/socket'"),
        "{explained:?}"
    );
    assert!(
        explained.problem.contains("reading your system setup"),
        "{explained:?}"
    );
    assert!(
        explained.next_step.contains("the rest of what Nix said"),
        "{explained:?}"
    );
}

/// Nix complains from the outside in: the first `error:` is the attribute it
/// was asked for, and the last is the reason it couldn't give it.
#[test]
fn the_reason_is_taken_from_the_end_of_the_trace() {
    let explained = reading("       error: undefined variable 'wanted'");

    assert!(
        !explained.problem.contains("config.system.build.toplevel"),
        "the outermost complaint isn't the reason: {explained:?}"
    );
    assert!(explained.problem.contains("`wanted`"), "{explained:?}");
}

/// A system built from a flake is read from a copy of it in the Nix store,
/// so that is where Nix says the user's own files are.
#[test]
fn a_file_in_a_copy_of_the_setup_is_named_as_the_user_knows_it() {
    let explained = reading(
        "       error: undefined variable 'someTypo'
       at /nix/store/zb8l0lk9kg3zbw1i8crz5i5hfjzjxg1x-source/configuration.nix:10:25:",
    );

    assert_eq!(
        explained.affects.as_deref(),
        Some("configuration.nix, line 10")
    );
}

/// Nix's own files are no answer to what a problem affects, so a path deeper
/// in the store is left out rather than shown.
#[test]
fn nixs_own_files_are_not_offered_as_the_place_to_look() {
    let explained = reading(
        "       error: undefined variable 'someTypo'
       at /nix/store/7sa2x9c9566lfxkaxs36n3z4yayl5cy4-source/nixos/modules/tasks/filesystems/zfs.nix:12:3:",
    );

    assert_eq!(explained.affects, None);
}

#[test]
fn a_feature_that_isnt_turned_on_says_how_to_turn_it_on() {
    let explained = Failure {
        step: Step::Read,
        why: Why::FeatureOff {
            feature: "flakes".to_owned(),
        },
        command: None,
        output: String::new(),
    }
    .explain();

    assert!(explained.problem.contains("`flakes`"), "{explained:?}");
    assert!(
        explained.problem.contains("reading your system setup"),
        "{explained:?}"
    );
    assert!(
        explained.next_step.contains("experimental-features"),
        "{explained:?}"
    );
    assert!(
        explained.next_step.contains("/etc/nix/nix.conf"),
        "{explained:?}"
    );
}

#[test]
fn a_nix_that_cant_be_run_says_to_look_at_the_path() {
    let explained = Failure {
        step: Step::LookUp,
        why: Why::CantRun,
        command: Some("nix eval --json".to_owned()),
        output: "No such file or directory (os error 2)".to_owned(),
    }
    .explain();

    assert!(
        explained.problem.contains("couldn't run Nix"),
        "{explained:?}"
    );
    assert!(explained.next_step.contains("PATH"), "{explained:?}");
}

/// Where there's only room for a line — after `error:` on a command that
/// wasn't about checking — it says what went wrong and where.
#[test]
fn one_line_says_what_went_wrong_and_where() {
    let failure = Failure {
        step: Step::LookUp,
        why: Why::Refused,
        command: Some("nix eval".to_owned()),
        output:
            "error: undefined variable 'someTypo'\n       at /etc/nixos/configuration.nix:10:25:"
                .to_owned(),
    };

    assert_eq!(
        failure.to_string(),
        "your settings use `someTypo`, and nothing defines it (/etc/nixos/configuration.nix, line 10)"
    );
}

/// Whatever Slipway makes of it, Nix's own words are kept as they were.
#[test]
fn nixs_words_are_kept_whole() {
    let said = format!("{TRACE}\n       error: undefined variable 'someTypo'");
    let failure = Failure {
        step: Step::Read,
        why: Why::Refused,
        command: None,
        output: said.clone(),
    };

    assert_eq!(failure.output, said);
}

/// A message that looks like one Slipway knows but names nothing is left to
/// Nix, rather than explained with a blank where the name should be.
#[test]
fn a_message_naming_nothing_is_left_to_nix() {
    let explained = reading("       error: undefined variable");

    assert!(
        explained.problem.contains("undefined variable"),
        "{explained:?}"
    );
    assert!(!explained.problem.contains("``"), "{explained:?}");
    assert!(
        explained.next_step.contains("the rest of what Nix said"),
        "{explained:?}"
    );
}

/// A dry build that fails is about what couldn't be built, not where to look
/// in a file.
#[test]
fn a_dry_build_that_fails_points_at_what_it_couldnt_build() {
    let explained = refused(
        Step::DryBuild,
        "       error: a 'aarch64-linux' with features {} is required to build '/nix/store/x.drv', but I am a 'x86_64-linux' with features {kvm}",
    );

    assert!(explained.problem.contains("aarch64-linux"), "{explained:?}");
    assert!(
        explained.next_step.contains("couldn't build"),
        "{explained:?}"
    );
}
