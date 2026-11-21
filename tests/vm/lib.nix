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
  # `slipway detect` and returns its report as {label: value};
  # `connects()` runs `slipway connect` twice, checking the second run finds
  # nothing to do; `evaluates()` checks NixOS can evaluate /etc/nixos
  # (without building it); `packages()` returns what `slipway list` shows;
  # `checks()` runs `slipway check`, which evaluates and dry-builds the whole
  # configuration.
  testScript,
}:

let
  etcNixos = prepare name fixture;

  # Evaluating, unlike building, needs nothing from the network: only the
  # nixpkgs source, which the VM is given below.
  evaluate =
    if flakes then
      "nix eval --offline --raw /etc/nixos#nixosConfigurations.nixos.config.system.build.toplevel.drvPath"
    else
      "nix-instantiate '<nixpkgs/nixos>' -A system -I nixpkgs=${nixpkgs} -I nixos-config=/etc/nixos/configuration.nix";
in

pkgs.testers.runNixOSTest {
  name = "slipway-${name}";

  nodes.machine = {
    environment.systemPackages = [ slipway ];
    virtualisation.additionalPaths = [
      etcNixos
      nixpkgs.outPath
    ];
    # Evaluating a whole NixOS system needs more than the default 1 GiB.
    virtualisation.memorySize = 3072;
    nix.settings = {
      experimental-features = lib.mkForce (
        lib.optionals flakes [
          "nix-command"
          "flakes"
        ]
      );

      # `nix --offline` written out as settings, since Slipway passes no
      # flags of its own: the machine has no network, so every question has
      # to be answered from its store, which holds the nixpkgs the fixtures
      # are pinned to.
      substitute = false;
      connect-timeout = 1;
      download-attempts = 1;
      tarball-ttl = 31536000;
    };
    # A configuration.nix system finds nixpkgs on Nix's search path, as a
    # real one does after `nix-channel --update`.
    nix.nixPath = [ "nixpkgs=${nixpkgs}" ];
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


      def connects() -> None:
          first = machine.succeed("slipway connect --yes")
          assert "Connected." in first, first
          machine.succeed("test -f /etc/nixos/slipway/default.nix")
          again = machine.succeed("slipway connect --yes")
          assert "doesn't need changing" in again, again


      def evaluates() -> None:
          machine.succeed("${evaluate}")


      def packages() -> list[str]:
          return machine.succeed("slipway list").split()


      def checks() -> None:
          report = machine.succeed("slipway check")
          assert "Everything checks out." in report, report
          # The dry build says what a rebuild would do, and the report
          # repeats it in brackets after the check it came from.
          assert "ready to build (" in report, report


      ${if builtins.isFunction testScript then testScript args else testScript}
    '';
}
