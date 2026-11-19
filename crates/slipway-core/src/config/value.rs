use std::{collections::BTreeMap, fmt, str::FromStr};

use super::InvalidConfig;

/// A value Slipway can write into a Nix file, such as an option's setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NixValue {
    Bool(bool),
    Int(i64),
    String(String),
    List(Vec<NixValue>),
    Attrs(BTreeMap<String, NixValue>),
}

impl From<bool> for NixValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for NixValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<&str> for NixValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for NixValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl<T: Into<NixValue>> From<Vec<T>> for NixValue {
    fn from(values: Vec<T>) -> Self {
        Self::List(values.into_iter().map(Into::into).collect())
    }
}

/// A dotted path to a NixOS option, such as `services.openssh`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttrPath(Vec<String>);

impl AttrPath {
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Display for AttrPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("."))
    }
}

impl FromStr for AttrPath {
    type Err = InvalidConfig;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        if path.is_empty() {
            return Err(InvalidConfig::new("an option path can't be empty"));
        }
        if path.split('.').any(str::is_empty) {
            return Err(InvalidConfig::new(format!(
                "`{path}` isn't a valid option path; it can't start or end with `.`, or have two dots in a row"
            )));
        }
        if path.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(InvalidConfig::new(format!(
                "`{path}` isn't a valid option path; it can't contain spaces"
            )));
        }
        Ok(Self(path.split('.').map(str::to_owned).collect()))
    }
}

const KEYWORDS: [&str; 10] = [
    "assert", "else", "if", "in", "inherit", "let", "or", "rec", "then", "with",
];

/// Writes one attribute name, quoting it when Nix wouldn't accept it bare
/// (for example `"7zz"`, which starts with a digit).
pub(super) fn attr_name(name: &str) -> String {
    let mut chars = name.chars();
    let bare = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '\'' | '-'))
        && !KEYWORDS.contains(&name);

    if bare { name.to_owned() } else { string(name) }
}

/// Writes a dotted attribute path, quoting each part as needed.
pub(super) fn attr_path<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments
        .into_iter()
        .map(attr_name)
        .collect::<Vec<_>>()
        .join(".")
}

/// Writes a double-quoted Nix string, escaping anything Nix would interpret.
pub(super) fn string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
