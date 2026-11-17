# Slipway
a GUI/TUI for building and configuring NixOS flakes for installation.

## Usage 

List Something!
```
slipway test --list
```

## Development

Slipway links against Nix's C API, so `cargo build` needs `pkg-config`,
Nix's `dev` output and libclang. The flake provides them:

```
nix develop
cargo build
```

Building outside the dev shell fails in `nix-bindings-sys`'s build script
with `Unable to find .pc file for nix-store-c`.

The Nix pinned in `flake.nix` is what the binary links against, which need
not match the Nix on a user's machine — `slipway configure --init` prints
all three versions it can see.
