use std::{fmt, str::FromStr};

use anyhow::Result;

/// A validated package name, such as `firefox` or `python3Packages.requests`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(String);

impl PackageName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PackageName {
    type Err = InvalidPackageName;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let invalid = |reason| Err(InvalidPackageName { reason });

        if name.is_empty() {
            return invalid("a package name can't be empty".to_owned());
        }
        if name.chars().any(char::is_whitespace) {
            return invalid("package names can't contain spaces".to_owned());
        }
        if let Some(c) = name
            .chars()
            .find(|&c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        {
            return invalid(format!(
                "`{c}` isn't allowed in a package name; use letters, numbers, `-`, `_` and `.`"
            ));
        }
        if name.split('.').any(str::is_empty) {
            return invalid(
                "package names can't start or end with `.`, or have two dots in a row".to_owned(),
            );
        }
        if name.split('.').any(|part| part.starts_with('-')) {
            return invalid("package names can't start with `-`".to_owned());
        }

        Ok(Self(name.to_owned()))
    }
}

/// What a package set holds under a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    /// A package, which Slipway can install.
    Package,
    /// Something that isn't a package, such as a part of one
    /// (`firefox.meta`) or a collection of them (`python3Packages`).
    SomethingElse,
    /// Nothing at all: the name is a typo, or names a package that only
    /// another channel has.
    Nothing,
}

/// Somewhere Slipway can check a package name, such as the set of packages
/// the user's own system is built from.
pub trait PackageSet {
    /// What this set holds under `package`.
    ///
    /// A name the set has but can't work out — an unfree package, say —
    /// counts as [`Holds::Package`]: the name is plainly one the set knows,
    /// and whatever is wrong with it is explained by the rebuild itself.
    fn holds(&self, package: &PackageName) -> Result<Holds>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPackageName {
    reason: String,
}

impl fmt::Display for InvalidPackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for InvalidPackageName {}
