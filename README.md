# Slipway
a GUI/TUI for building and configuring NixOS flakes for installation.

## Usage 

```
slipway detect             # show which versions of Nix slipway can find
slipway add <package>      # add a package (not built yet)
slipway remove <package>   # remove a package (not built yet)
slipway list               # list your packages (not built yet)
slipway check              # check your setup for problems (not built yet)
slipway configure --init   # create ~/.config/slipway
```

All logic lives in the `slipway-core` crate (`crates/slipway-core`); the
`slipway` binary is a thin CLI over it so the TUI and GUI can share it.

## Development

Slipway links against Nix's C API, so `cargo build` needs `pkg-config`,
Nix's `dev` output and libclang. The flake provides them:

```
nix develop
cargo build --workspace
```

Building outside the dev shell fails in `nix-bindings-sys`'s build script
with `Unable to find .pc file for nix-store-c`.

The Nix pinned in `flake.nix` is what the binary links against, which need
not match the Nix on a user's machine — `slipway configure --init` prints
all three versions it can see.
