//! Nix's own settings, read from its configuration files.
//!
//! Slipway needs to know which experimental features are turned on — above
//! all `nix-command` and `flakes` — and it needs that answer most when they
//! are *off*, so it can tell the user what to enable. Nix can report its own
//! settings with `nix config show`, but that command belongs to the new
//! command line and is gated behind `nix-command`, so it cannot answer the
//! question that matters: on a machine without the feature it only says the
//! feature is missing. Passing `--extra-experimental-features` to get past
//! that changes the very value being read.
//!
//! So these settings come from Nix's configuration files instead, in the
//! order nix.conf(5) gives them.

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};

/// How far `include` directives may nest before we assume a loop.
const MAX_INCLUDE_DEPTH: u8 = 10;

/// Nix's settings, with later sources overriding earlier ones.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    values: HashMap<String, String>,
}

impl Settings {
    /// Reads Nix's configuration files. `root` is the filesystem root they
    /// are looked up under: `/` on a real machine, a fixture folder in
    /// tests.
    pub fn read(root: &Path) -> Result<Self> {
        let mut settings = Self::default();

        for path in config_files(root) {
            settings.merge_file(&path, 0)?;
        }

        // Highest precedence: a config file's worth of text in the
        // environment.
        if let Some(config) = env::var_os("NIX_CONFIG") {
            settings
                .merge(&config.to_string_lossy(), Path::new("."), 0)
                .context("couldn't read the NIX_CONFIG environment variable")?;
        }

        Ok(settings)
    }

    /// Parses the contents of one configuration file.
    pub fn parse(text: &str) -> Result<Self> {
        let mut settings = Self::default();
        settings.merge(text, Path::new("."), 0)?;
        Ok(settings)
    }

    /// The value Nix's settings give `key`, if they set it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The experimental features these settings turn on, such as
    /// `nix-command` and `flakes`.
    #[must_use]
    pub fn experimental_features(&self) -> Vec<String> {
        self.get("experimental-features")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }

    /// Merges a file into these settings. A file that isn't there is the
    /// normal case, not a problem.
    fn merge_file(&mut self, path: &Path, depth: u8) -> Result<()> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(err).with_context(|| format!("couldn't read {}", path.display()));
            }
        };
        let dir = path.parent().unwrap_or(Path::new("."));
        self.merge(&text, dir, depth)
            .with_context(|| format!("couldn't read {}", path.display()))
    }

    /// Merges configuration text. `dir` is what relative `include` paths are
    /// resolved against.
    fn merge(&mut self, text: &str, dir: &Path, depth: u8) -> Result<()> {
        for line in text.lines() {
            // Nix treats the rest of a line after `#` as a comment.
            let line = line.split('#').next().unwrap_or(line).trim();
            if line.is_empty() {
                continue;
            }

            // `include` must find its file; `!include` may skip a missing
            // one.
            if let Some((rest, required)) = line
                .strip_prefix("include ")
                .map(|rest| (rest, true))
                .or_else(|| line.strip_prefix("!include ").map(|rest| (rest, false)))
            {
                if depth >= MAX_INCLUDE_DEPTH {
                    bail!("its `include` directives nest more than {MAX_INCLUDE_DEPTH} deep");
                }
                let included = dir.join(rest.trim());
                if included.exists() {
                    self.merge_file(&included, depth + 1)?;
                } else if required {
                    bail!("it includes {}, which isn't there", included.display());
                }
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());

            // `extra-foo` appends to `foo` instead of replacing it.
            if let Some(key) = key.strip_prefix("extra-") {
                let current = self.values.entry(key.to_owned()).or_default();
                if !current.is_empty() && !value.is_empty() {
                    current.push(' ');
                }
                current.push_str(value);
            } else {
                self.values.insert(key.to_owned(), value.to_owned());
            }
        }

        Ok(())
    }
}

/// The experimental features this machine's Nix turns on, looked up under
/// `root`.
pub fn experimental_features(root: &Path) -> Result<Vec<String>> {
    Ok(Settings::read(root)?.experimental_features())
}

/// Nix's configuration files, lowest precedence first.
///
/// Only the system file and the user's own are consulted; Nix also walks
/// `XDG_CONFIG_DIRS`, which practically never holds a `nix.conf`.
fn config_files(root: &Path) -> Vec<PathBuf> {
    let conf_dir =
        env::var_os("NIX_CONF_DIR").map_or_else(|| PathBuf::from("/etc/nix"), PathBuf::from);
    let mut files = vec![under(root, &conf_dir).join("nix.conf")];

    match env::var_os("NIX_USER_CONF_FILES") {
        Some(listed) => files.extend(
            env::split_paths(&listed)
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| under(root, &path)),
        ),
        None => {
            if let Some(dir) = user_config_dir() {
                files.push(under(root, &dir).join("nix/nix.conf"));
            }
        }
    }

    files
}

/// `$XDG_CONFIG_HOME`, or `~/.config` as it defaults to.
fn user_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg));
    }
    let home = env::var_os("HOME").filter(|value| !value.is_empty())?;
    Some(PathBuf::from(home).join(".config"))
}

/// Resolves an absolute path inside `root`, so tests can point at a folder
/// laid out like a real system.
fn under(root: &Path, path: &Path) -> PathBuf {
    root.join(path.strip_prefix("/").unwrap_or(path))
}
