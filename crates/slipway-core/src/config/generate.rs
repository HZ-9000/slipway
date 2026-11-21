use super::{
    Config, OutputMode,
    value::{NixValue, attr_name, attr_path, string},
};

pub const MANAGED_MARKER: &str = "# This file is managed by Slipway.";
pub(super) const FILE_NAMES: [&str; 2] = ["default.nix", "flake.nix"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFile {
    pub name: &'static str,
    pub contents: String,
}

/// Whether Slipway wrote this file, which it says in its first line.
pub fn is_managed(contents: &str) -> bool {
    contents.lines().next() == Some(MANAGED_MARKER)
}

pub(super) fn render(config: &Config) -> Vec<ManagedFile> {
    let mut files = vec![ManagedFile {
        name: "default.nix",
        contents: module(config),
    }];
    if config.output_mode == OutputMode::Flake {
        files.push(ManagedFile {
            name: "flake.nix",
            contents: flake(config),
        });
    }
    files
}

fn header(config: &Config) -> String {
    let own_changes = match config.output_mode {
        OutputMode::Flake => {
            "# will be lost. Put your own changes in your own flake.nix (outside this\n\
                              # folder) or a module it lists; NixOS combines them with the settings here.\n"
        }
        OutputMode::Channels => {
            "# will be lost. Put your own changes in /etc/nixos/configuration.nix\n\
                                 # instead; NixOS combines them with the settings here.\n"
        }
    };
    format!(
        "{MANAGED_MARKER}\n\
         #\n\
         # Slipway rewrites it whenever you change your setup, so edits made here\n\
         {own_changes}\
         #\n\
         # Packages and options here are chosen from the {} channel.\n",
        config.channel
    )
}

fn module(config: &Config) -> String {
    let mut out = header(config);
    out.push_str(
        "\n\
         # A NixOS module: NixOS calls this function with your package set (`pkgs`)\n\
         # and merges the settings it returns into the rest of your configuration.\n\
         { pkgs, ... }:\n\
         \n\
         {\n  \
           # Software installed for everyone on this computer. Each `pkgs.<name>`\n  \
           # is one package; add or remove them with `slipway add` and\n  \
           # `slipway remove`.\n",
    );

    if config.packages.is_empty() {
        out.push_str("  environment.systemPackages = [ ];\n");
    } else {
        out.push_str("  environment.systemPackages = [\n");
        for package in &config.packages {
            let path = attr_path(std::iter::once("pkgs").chain(package.as_str().split('.')));
            out.push_str(&format!("    {path}\n"));
        }
        out.push_str("  ];\n");
    }

    for (path, module) in config.modules() {
        let what = if module.options().next().is_some() {
            " and sets the options below"
        } else {
            ""
        };
        out.push_str(&format!(
            "\n  \
             # Turns on `{path}`{what}.\n  \
             # What it does, and every option it has: https://search.nixos.org/options?query={path}\n  \
             {} = {{\n    \
             enable = true;\n",
            attr_path(path.segments().iter().map(String::as_str))
        ));
        for (name, value) in module.options() {
            out.push_str(&format!(
                "    {} = {};\n",
                attr_name(name),
                nix_value(value, 2)
            ));
        }
        out.push_str("  };\n");
    }

    out.push_str("}\n");
    out
}

fn flake(config: &Config) -> String {
    let mut out = header(config);
    out.push_str(
        "\n\
         {\n  \
           description = \"Packages and settings managed by Slipway\";\n\
         \n  \
           # This flake has no inputs: its module uses the nixpkgs of the flake that\n  \
           # imports it, so your system never ends up with two copies of nixpkgs.\n  \
           outputs =\n    \
             { self }:\n    \
             {\n      \
               # The NixOS module your flake adds to its `modules` list, for example\n      \
               # `modules = [ slipway.nixosModules.default ./configuration.nix ];`\n      \
               nixosModules.default = import ./default.nix;\n    \
             };\n\
         }\n",
    );
    out
}

/// Writes a value whose first line continues the current line; any further
/// lines are indented `depth` levels, as nixfmt would place them.
fn nix_value(value: &NixValue, depth: usize) -> String {
    let pad = |depth: usize| "  ".repeat(depth);
    match value {
        NixValue::Bool(value) => value.to_string(),
        NixValue::Int(value) => value.to_string(),
        NixValue::String(value) => string(value),
        NixValue::List(items) if items.is_empty() => "[ ]".to_owned(),
        NixValue::List(items) => {
            let mut out = "[\n".to_owned();
            for item in items {
                let item_text = nix_value(item, depth + 1);
                // `[ 1 -1 ]` would read as subtraction, so negative numbers
                // in a list need brackets.
                let item_text = match item {
                    NixValue::Int(n) if *n < 0 => format!("({item_text})"),
                    _ => item_text,
                };
                out.push_str(&format!("{}{item_text}\n", pad(depth + 1)));
            }
            out.push_str(&format!("{}]", pad(depth)));
            out
        }
        NixValue::Attrs(attrs) if attrs.is_empty() => "{ }".to_owned(),
        NixValue::Attrs(attrs) => {
            let mut out = "{\n".to_owned();
            for (name, value) in attrs {
                out.push_str(&format!(
                    "{}{} = {};\n",
                    pad(depth + 1),
                    attr_name(name),
                    nix_value(value, depth + 1)
                ));
            }
            out.push_str(&format!("{}}}", pad(depth)));
            out
        }
    }
}
