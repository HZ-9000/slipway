# Slipway
a GUI/TUI for building and configuring NixOS flakes for installation.

## Usage 

```
slipway detect             # show the NixOS version, channel, system config and Nix features
slipway add <package>      # add a package (not built yet)
slipway remove <package>   # remove a package (not built yet)
slipway list               # list your packages (not built yet)
slipway check              # check your setup for problems (not built yet)
slipway configure --init   # create ~/.config/slipway
```

All logic lives in the `slipway-core` crate (`crates/slipway-core`); the
`slipway` binary is a thin CLI over it so the TUI and GUI can share it.

## Development

Slipway is plain Rust with no system dependencies:

```
cargo build --workspace
cargo test --workspace
```

`nix develop` gives you the pinned toolchain plus a `nix` for the tests to
talk to, but it isn't required.

Slipway drives Nix through its command line rather than linking its C API.
That keeps one binary working against whatever Nix a machine has: the C
libraries carry the full version in their SONAME (`libnixstorec.so.2.35.6`)
and wrap C++ with no stable ABI, so linking them would pin Slipway to a
single Nix patch release.
