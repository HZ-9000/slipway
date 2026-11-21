# Slipway
a GUI/TUI for building and configuring NixOS flakes for installation.

## Usage 

```
slipway detect             # show the NixOS version, channel, system config and Nix features
slipway connect            # connect your system config to Slipway (shows the change, asks first)
slipway add <package>      # add a package, then rewrite the Nix that installs it
slipway remove <package>   # remove a package slipway added
slipway list               # list the packages slipway looks after, one per line
slipway check              # check the setup, and that NixOS can still read and build it
slipway configure --init   # create ~/.config/slipway
```

Every command that reads the system takes `--config <path>` for a setup that
isn't in `/etc/nixos`.

## What slipway writes

Slipway keeps everything it manages in a `slipway` folder beside your own
system configuration, which is `/etc/nixos/slipway` on most computers:

- `default.nix`, and `flake.nix` on a system built from a flake, is the
  NixOS module it generates. Your configuration imports it (that one line is
  what `slipway connect` adds), and slipway rewrites it whenever you change
  something, so edits made there are lost.
- `state.json` is the list of packages you asked for. It is the only file
  there that isn't Nix, and the one `slipway list` reads. Keeping it beside
  the generated Nix means the list travels with the configuration — in git,
  in a backup, or on a copy of `/etc/nixos` — rather than living in one
  user's home.

`slipway add` asks Nix whether the name is a package before it writes
anything, using the same package set the computer is built from, so a typo
is refused there and then rather than left for the next rebuild to find.
`slipway check` goes over the lot: the folder, the list, the Nix generated
from it, the line that brings it in, and whether Nix can read and build the
whole configuration. It changes nothing, and says what a rebuild would
build. A problem it finds is reported as what went wrong, what it affects
and what to try next:

```
  PROBLEM  Nix can read your system setup
           `networking.hostName` is set twice in the same file
           Affects: /etc/nixos/configuration.nix, line 11
           Try:     take one of the two out; Nix won't choose between them.
```

Nix's own explanation is kept word for word behind `slipway check --details`,
for a problem Slipway doesn't recognise or a user who wants the stack trace.

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

`nix flake check` also boots NixOS VMs and runs Slipway against the fixture
configurations in `tests/vm/fixtures`. Those checks need a Linux builder
(with KVM, or they're very slow), so on macOS `nix flake check` skips them;
run one directly with `nix build .#checks.x86_64-linux.vm-fresh-flake -L`.
Fixtures are snapshots of a real `/etc/nixos`, so leave their formatting as
a person or `nixos-generate-config` would have written it.

Slipway drives Nix through its command line rather than linking its C API.
That keeps one binary working against whatever Nix a machine has: the C
libraries carry the full version in their SONAME (`libnixstorec.so.2.35.6`)
and wrap C++ with no stable ABI, so linking them would pin Slipway to a
single Nix patch release.
