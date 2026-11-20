# Builds one VM test: boot a NixOS machine with Slipway installed, put a
# fixture configuration in /etc/nixos, then run `testScript` against it.
{
  pkgs,
  lib ? pkgs.lib,
  # The nixpkgs flake input, recorded in fixture flake.lock files.
  nixpkgs,
  # The channel nixpkgs follows, which fixture flakes name as their input.
  channel,
  slipway,
}:

let
  # A fresh flake install pins nixpkgs in flake.lock on its first rebuild.
  # Pinning it to the very nixpkgs the VM is built from keeps the fixture
  # buildable offline, since the source is already in the store.
  flakeLock = builtins.toJSON {
    nodes = {
      nixpkgs = {
        locked = {
          inherit (nixpkgs) lastModified narHash rev;
          owner = "NixOS";
          repo = "nixpkgs";
          type = "github";
        };
        original = {
          owner = "NixOS";
          ref = channel;
          repo = "nixpkgs";
          type = "github";
        };
      };
      root.inputs.nixpkgs = "nixpkgs";
    };
    root = "root";
    version = 7;
  };

  # Fixture flakes say `@channel@` where they name their nixpkgs branch.
  prepare =
    name: src:
    pkgs.runCommand "slipway-fixture-${name}" { } ''
      cp -r ${src} $out
      chmod -R u+w $out
      if [ -e $out/flake.nix ]; then
        substituteInPlace $out/flake.nix --replace-fail @channel@ ${channel}
        cp ${pkgs.writeText "flake.lock" flakeLock} $out/flake.lock
      fi
    '';
in

{
  name,
  # A folder laid out like /etc/nixos.
  fixture,
  # Whether the machine has `nix-command` and `flakes` turned on, as anyone
  # building from a flake must.
  flakes ? builtins.pathExists (fixture + "/flake.nix"),
  # Python run after the fixture is in place. `detect()` runs
  # `slipway detect` and returns its report as {label: value}.
  testScript,
}:

let
  etcNixos = prepare name fixture;
in

pkgs.testers.runNixOSTest {
  name = "slipway-${name}";

  nodes.machine = {
    environment.systemPackages = [ slipway ];
    virtualisation.additionalPaths = [ etcNixos ];
    nix.settings.experimental-features = lib.mkForce (
      lib.optionals flakes [
        "nix-command"
        "flakes"
      ]
    );
  };

  testScript =
    { nodes, ... }@args:
    ''
      start_all()
      machine.wait_for_unit("multi-user.target")
      machine.succeed("rm -rf /etc/nixos && cp -rT ${etcNixos} /etc/nixos && chmod -R u+w /etc/nixos")


      def detect(args: str = "") -> dict[str, str]:
          report: dict[str, str] = {}
          for line in machine.succeed(f"slipway detect {args}").splitlines():
              label, _, value = line.partition(":")
              report[label.strip()] = value.strip()
          return report


      ${if builtins.isFunction testScript then testScript args else testScript}
    '';
}
