//! Turning what Nix said into something a person can act on.
//!
//! Nix explains itself to someone who already knows Nix: the outermost
//! complaint first, a stack trace through its own source, and the cause on
//! the last line. What follows picks out the handful of problems a person
//! setting up a computer actually runs into — a name that isn't defined, a
//! setting that doesn't exist, the same setting given twice, a package that
//! isn't free software, a typo — and says what went wrong, what it affects
//! and what to try next.
//!
//! Anything else keeps Nix's own words. A wrong guess about an unfamiliar
//! problem is worse than the real message.

use std::fmt;

use crate::nix::{
    cli::Why,
    eval::{Failure, Step},
};

/// A problem Nix reported, in plain language.
///
/// Everything Nix said is still on [`Failure::output`], for a front-end
/// that offers to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explained {
    /// What went wrong, in one sentence.
    pub problem: String,
    /// What it affects: the file and line, the setting, or the package.
    pub affects: Option<String>,
    /// What to try next.
    pub next_step: String,
}

impl fmt::Display for Explained {
    /// What went wrong and where, for somewhere there's only room for a
    /// line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.affects {
            Some(affects) => write!(f, "{} ({affects})", self.problem),
            None => f.write_str(&self.problem),
        }
    }
}

pub(super) fn of(failure: &Failure) -> Explained {
    match &failure.why {
        Why::CantRun => Explained {
            problem: format!("Slipway couldn't run Nix: {}", failure.output),
            affects: None,
            next_step: "check that `nix` is installed and on your PATH.".to_owned(),
        },
        Why::FeatureOff { feature } => Explained {
            problem: format!(
                "{} needs Nix's `{feature}` feature, which this computer hasn't turned on",
                failure.step
            ),
            affects: Some("every question Slipway asks Nix".to_owned()),
            next_step: format!(
                "add `nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];` to your own settings and rebuild, or put `extra-experimental-features = {feature}` in /etc/nix/nix.conf."
            ),
        },
        Why::Refused => refused(failure),
    }
}

fn refused(failure: &Failure) -> Explained {
    let output = &failure.output;
    let cause = cause(output);
    let at = location(output);
    // Whatever Nix quoted, which for most of these is the name of the thing
    // it couldn't make sense of. A rule that needs one and hasn't got one
    // falls through, rather than explaining something it can't name.
    let named = quoted(cause);

    // The wordings matched below are Nix's own, taken from what it prints.
    if cause.contains("undefined variable")
        && let Some(name) = named
    {
        return Explained {
            problem: format!("your settings use `{name}`, and nothing defines it"),
            affects: at,
            next_step: format!(
                "check the spelling where it's used. A package is written `pkgs.{name}`, and a setting another module adds needs that module imported first."
            ),
        };
    }

    if cause.contains("syntax error") {
        return Explained {
            problem: format!("Nix couldn't read your settings: {cause}"),
            affects: at,
            next_step: "open that line and look for a missing `;`, `}` or quote.".to_owned(),
        };
    }

    if cause.contains("does not exist")
        && cause.contains("option")
        && let Some(option) = named
    {
        return Explained {
            problem: format!("nothing on this computer has a setting called `{option}`"),
            affects: Some(match at {
                Some(at) => format!("the setting `{option}`, set in {at}"),
                None => format!("the setting `{option}`"),
            }),
            next_step: format!(
                "check the name at https://search.nixos.org/options?query={option}, or take the setting out. If another module adds it, that module has to be imported first."
            ),
        };
    }

    if cause.contains("conflicting definition")
        && let Some(option) = named
    {
        let files = files_listed(output);
        return Explained {
            problem: format!(
                "two parts of your setup set `{option}` to different things, and Nix can't choose between them"
            ),
            affects: (!files.is_empty()).then(|| files.join(" and ")),
            next_step:
                "set it in one place only, or mark the one that should win with `lib.mkForce`."
                    .to_owned(),
        };
    }

    if cause.contains("already defined")
        && let Some(name) = named
    {
        return Explained {
            problem: format!("`{name}` is set twice in the same file"),
            affects: at.or_else(|| location(cause)),
            next_step: "take one of the two out; Nix won't choose between them.".to_owned(),
        };
    }

    if cause.contains("unfree license")
        && let Some(name) = named
    {
        let package = without_version(name);
        return Explained {
            problem: format!(
                "`{package}` isn't free software, and this computer hasn't been told to allow that"
            ),
            affects: Some(format!("the package `{package}`")),
            next_step: format!(
                "add `nixpkgs.config.allowUnfree = true;` to your own settings — not Slipway's folder, which gets rewritten — or take it out with `slipway remove {package}`."
            ),
        };
    }

    if cause.contains("missing")
        && cause.contains("attribute")
        && let Some(missing) = named
    {
        let mut next_step =
            format!("check the spelling of `{missing}` where your settings ask for it.");
        if let Some(suggestion) = suggestion(output) {
            next_step.push(' ');
            next_step.push_str(suggestion);
        }
        return Explained {
            problem: format!("your settings ask for `{missing}`, which Nix couldn't find"),
            affects: at,
            next_step,
        };
    }

    // Nothing Slipway knows by name, so Nix speaks for itself.
    Explained {
        problem: format!("Nix couldn't finish {}: {cause}", failure.step),
        affects: at,
        next_step: match failure.step {
            Step::DryBuild => "the rest of what Nix said names what it couldn't build.".to_owned(),
            _ => "the rest of what Nix said names the file and line it stopped at.".to_owned(),
        },
    }
}

/// The line that says what actually went wrong.
///
/// Nix prints the outermost complaint first and the cause last, each marked
/// `error:`, with its own stack trace in between.
pub(super) fn cause(output: &str) -> &str {
    let lines = || output.lines().map(str::trim);
    lines()
        .filter_map(|line| line.strip_prefix("error:"))
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .or_else(|| lines().find(|line| !line.is_empty()))
        .unwrap_or("it printed no explanation")
}

/// The first thing Nix quoted. It quotes with `'`, with `‘…’` and with a
/// backtick and a straight quote, depending on which part of it is talking.
fn quoted(text: &str) -> Option<&str> {
    let opens = ['\'', '‘', '`'];
    let start = text.find(opens)?;
    let open = text[start..].chars().next()?;
    let close = match open {
        '‘' => '’',
        // The module system writes an option as `services.openssh'.
        '`' => '\'',
        other => other,
    };
    let rest = &text[start + open.len_utf8()..];
    let end = rest.find(close)?;
    Some(&rest[..end])
}

/// The file and line Nix blamed, from a line such as
/// `at /etc/nixos/configuration.nix:12:3:`. The first one it names that
/// means anything to the user, which is the outermost frame of its trace
/// that isn't inside Nix itself.
fn location(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|token| {
        let token = token.trim_end_matches(':');
        if token.starts_with('«') {
            return None;
        }
        let (file, rest) = token.split_once(".nix:")?;
        let line = rest.split(':').next()?;
        line.parse::<u32>().ok()?;
        Some(format!("{}.nix, line {line}", worth_showing(file)?))
    })
}

/// The part of a path worth putting in front of someone.
///
/// A system built from a flake is read from a copy of it in the Nix store,
/// so that is where Nix says the user's own files are. A file at the root
/// of such a copy — `configuration.nix`, not `nixos/modules/something` —
/// is one of theirs, and its name tells them where to look. Anything deeper
/// in the store is Nix explaining its own workings, which is no answer to
/// what a problem affects.
fn worth_showing(file: &str) -> Option<&str> {
    let Some(in_store) = file.strip_prefix("/nix/store/") else {
        return Some(file);
    };
    let (_, inside) = in_store.split_once('/')?;
    (!inside.contains('/')).then_some(inside)
}

/// The files Nix listed under a conflict, which it writes as
/// ``- In `/etc/nixos/configuration.nix': "nixos"``.
fn files_listed(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("- In "))
        .filter_map(quoted)
        .map(str::to_owned)
        .collect()
}

/// Nix's own guess at what was meant, when it offers one.
fn suggestion(output: &str) -> Option<&str> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Did you mean"))
}

/// A package name without the version Nix tacks onto it, so
/// `google-chrome-157.0.8103.38` reads as `google-chrome`.
fn without_version(name: &str) -> &str {
    name.rfind('-')
        .filter(|&dash| {
            name[dash + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        })
        .map_or(name, |dash| &name[..dash])
}
