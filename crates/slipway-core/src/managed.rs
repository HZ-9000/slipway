//! Slipway's managed folder: the packages it has been asked to look after,
//! the Nix it writes to install them, and what can go wrong with either.
//!
//! Nothing here prints or asks. Every change is the same two steps — write
//! the Nix, then save the list — and every message says what the user can
//! do about it.

use std::{fs, path::Path};

use anyhow::{Context as _, Result, bail};

use crate::{
    config,
    nix::eval::Target,
    package::{Holds, PackageName, PackageSet},
    state::State,
    system::System,
    wire,
};

/// The file NixOS reads the managed folder through.
const MODULE_FILE: &str = "default.nix";

/// What a change came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The folder was rewritten.
    Made,
    /// It already said that, so nothing was written.
    AlreadyDone,
}

/// The packages Slipway looks after in `dir`, in the order it writes them.
///
/// Fails when Slipway isn't set up on this computer: an empty list would
/// otherwise read as "you have no packages", when the truth is that Slipway
/// isn't installing any of them yet.
pub fn packages(dir: &Path, system: &System) -> Result<Vec<PackageName>> {
    require_in_use(dir, system)?;
    Ok(State::load(dir)?.packages.into_iter().collect())
}

/// Adds `package` to the packages Slipway looks after, then rewrites the
/// Nix that installs them.
///
/// Checks the name against `available` first, so a typo never reaches the
/// user's files.
pub fn add(
    dir: &Path,
    system: &System,
    available: &dyn PackageSet,
    package: &PackageName,
) -> Result<Change> {
    require_in_use(dir, system)?;

    let mut state = State::load(dir)?;
    if state.packages.contains(package) {
        return Ok(Change::AlreadyDone);
    }

    match available
        .holds(package)
        .with_context(|| format!("couldn't check whether `{package}` is a package"))?
    {
        Holds::Package => {}
        Holds::SomethingElse => bail!(
            "`{package}` is part of the {} channel, but it isn't a package you can install; https://search.nixos.org/packages lists the ones you can",
            system.channel
        ),
        Holds::Nothing => bail!(
            "there's no package called `{package}` in the {} channel; check the spelling, or look the name up at https://search.nixos.org/packages",
            system.channel
        ),
    }

    state.packages.insert(package.clone());
    write(dir, system, &state)?;
    Ok(Change::Made)
}

/// Takes `package` off the list and rewrites the Nix without it.
///
/// Fails when Slipway isn't looking after it, since a package the user's own
/// configuration installs has to come out of that configuration instead.
pub fn remove(dir: &Path, system: &System, package: &PackageName) -> Result<Change> {
    require_in_use(dir, system)?;

    let mut state = State::load(dir)?;
    if !state.packages.remove(package) {
        bail!(
            "Slipway isn't looking after `{package}`, so there's nothing for it to remove; `slipway list` shows the packages it does look after, and anything else on this computer comes from your own settings"
        );
    }

    write(dir, system, &state)?;
    Ok(Change::Made)
}

/// Writes the Nix first and saves the list second, so a change that is
/// interrupted leaves the two disagreeing rather than a half-written file.
/// [`check`] says so when they disagree, and adding or removing anything
/// puts them back in step.
fn write(dir: &Path, system: &System, state: &State) -> Result<()> {
    state.config(system).write_to(dir)?;
    state.save(dir)
}

/// What [`check`] looks at, in the order it looks.
const EVERYTHING: [&str; 6] = [IN_PLACE, READABLE, UP_TO_DATE, IN_USE, READS, BUILDS];

const IN_PLACE: &str = "Slipway's folder is where it should be";
const READABLE: &str = "Slipway can read its list of packages";
const UP_TO_DATE: &str = "Slipway's settings match that list";
const IN_USE: &str = "your system setup uses Slipway";
const READS: &str = "Nix can read your system setup";
const BUILDS: &str = "everything your system needs is ready to build";

/// One thing [`check`] looked at.
#[derive(Debug)]
pub struct Check {
    /// What was looked at, as something to show the user.
    pub what: &'static str,
    pub outcome: Outcome,
    /// Something more that was learned on the way, such as what a dry build
    /// would build. Worth showing beside a check that passed.
    pub note: Option<String>,
}

#[derive(Debug)]
pub enum Outcome {
    Passed,
    /// What is wrong, and what the user can do about it. A problem Nix
    /// reported is a [`crate::nix::eval::Failure`] underneath, so a
    /// front-end that wants the parts of it — which step, why, what Nix
    /// said — can ask for them with `downcast_ref`.
    Failed(anyhow::Error),
    /// Not looked at, because something earlier went wrong.
    NotChecked,
}

/// Everything [`check`] looked at, in order.
#[derive(Debug, Default)]
pub struct Report {
    checks: Vec<Check>,
}

impl Report {
    #[must_use]
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }

    /// Whether nothing went wrong.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self
            .checks
            .iter()
            .any(|check| matches!(check.outcome, Outcome::Failed(_)))
    }

    /// Records one outcome, and says whether to carry on.
    fn ran(&mut self, what: &'static str, result: Result<()>) -> bool {
        self.ran_with(what, result.map(|()| None))
    }

    /// The same, keeping whatever the check learned worth passing on.
    fn ran_with(&mut self, what: &'static str, result: Result<Option<String>>) -> bool {
        let passed = result.is_ok();
        let (outcome, note) = match result {
            Ok(note) => (Outcome::Passed, note),
            Err(err) => (Outcome::Failed(err), None),
        };
        self.checks.push(Check {
            what,
            outcome,
            note,
        });
        passed
    }

    /// Fills in the checks that weren't reached.
    fn rest(mut self) -> Self {
        for what in EVERYTHING.iter().skip(self.checks.len()) {
            self.checks.push(Check {
                what,
                outcome: Outcome::NotChecked,
                note: None,
            });
        }
        self
    }
}

/// Looks over the whole setup without changing any of it: Slipway's folder,
/// the list in it, the Nix it generated, the one line that brings it in, and
/// what Nix makes of the lot.
///
/// Each check takes the ones before it as given, so the first thing to go
/// wrong ends the run: everything after it would only report the same
/// problem again.
pub fn check(dir: &Path, system: &System) -> Report {
    let mut report = Report::default();

    if !report.ran(IN_PLACE, in_place(dir)) {
        return report.rest();
    }
    if !report.ran(READABLE, State::load(dir).map(drop)) {
        return report.rest();
    }
    if !report.ran(UP_TO_DATE, up_to_date(dir, system)) {
        return report.rest();
    }
    if !report.ran(IN_USE, require_in_use(dir, system)) {
        return report.rest();
    }

    // Both of the questions left are about the same configuration, and
    // working out how to ask them costs a question of its own.
    let target = match Target::for_system(system) {
        Ok(target) => target,
        Err(failure) => {
            report.ran(READS, Err(failure.into()));
            return report.rest();
        }
    };

    let read = target.evaluate().map(drop).map_err(anyhow::Error::from);
    if !report.ran(READS, read) {
        return report.rest();
    }

    // What a dry build would build is worth repeating: it is the nearest
    // thing to a preview of the rebuild the user is about to run.
    let built = target
        .dry_build()
        .map(|planned| Some(planned.to_string()))
        .map_err(anyhow::Error::from);
    report.ran_with(BUILDS, built);
    report
}

/// Checks the managed folder is there and is Slipway's own work.
fn in_place(dir: &Path) -> Result<()> {
    let module = dir.join(MODULE_FILE);
    let contents = match fs::read_to_string(&module) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => bail!(
            "{} isn't there, so Slipway isn't set up on this computer yet; run `slipway connect` to set it up",
            module.display()
        ),
        Err(err) => {
            return Err(err).with_context(|| format!("couldn't read {}", module.display()));
        }
    };
    if !config::is_managed(&contents) {
        bail!(
            "{} wasn't written by Slipway, so Slipway won't go near it; move it out of {} and run `slipway connect` again",
            module.display(),
            dir.display()
        );
    }
    Ok(())
}

/// Checks the Nix in the folder is what the list of packages comes to, so a
/// rebuild installs what `slipway list` says it will.
fn up_to_date(dir: &Path, system: &System) -> Result<()> {
    let wanted = State::load(dir)?.config(system);
    for file in wanted.render() {
        let path = dir.join(file.name);
        let found = fs::read_to_string(&path)
            .with_context(|| format!("couldn't read {}", path.display()))?;
        if found != file.contents {
            bail!(
                "{} isn't what Slipway would write for the packages it's looking after, so a rebuild wouldn't install what `slipway list` shows; adding or removing any package writes it again",
                path.display()
            );
        }
    }
    Ok(())
}

/// Fails unless the user's configuration brings the managed folder in,
/// since packages written somewhere nothing reads wouldn't be installed.
///
/// Every change checks this first. [`crate::add`] checks it too, before
/// asking Nix anything, so a computer that isn't set up hears about that
/// rather than about Nix.
pub fn require_in_use(dir: &Path, system: &System) -> Result<()> {
    if wire::plan(&system.config, dir)?.is_some() {
        bail!(
            "{} doesn't bring in {}, so nothing Slipway installs would reach this computer; run `slipway connect` first",
            system.config.entry.display(),
            dir.display()
        );
    }
    Ok(())
}
