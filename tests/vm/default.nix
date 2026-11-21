# NixOS VM tests: each boots a machine with a fixture configuration in
# /etc/nixos and checks what Slipway makes of it. Run one with
# `nix build .#checks.x86_64-linux.vm-fresh-flake -L`.
{
  pkgs,
  nixpkgs,
  slipway,
  # Must match the branch the flake's nixpkgs input follows.
  channel ? "nixos-unstable",
}:

let
  mkSlipwayTest = import ./lib.nix {
    inherit
      pkgs
      nixpkgs
      channel
      slipway
      ;
  };

  # A system built from configuration.nix gets its channel from the NixOS
  # version. Real unstable channel builds carry `pre` in that version, but
  # these VMs are built from the flake, so they read as the numbered release.
  releaseChannel = nodes: "nixos-${nodes.machine.system.nixos.release}";
in

{
  vm-fresh-flake = mkSlipwayTest {
    name = "fresh-flake";
    fixture = ./fixtures/fresh-flake;
    testScript = ''
      report = detect()
      assert report["System config"] == "/etc/nixos/flake.nix (flake)", report
      assert report["Channel"] == "${channel} (unstable, from flake.lock)", report
      assert report["nix-command"] == "enabled", report
      assert report["flakes"] == "enabled", report

      connects()
      machine.succeed("test -f /etc/nixos/slipway/flake.nix")
      evaluates()
    '';
  };

  vm-fresh-channels = mkSlipwayTest {
    name = "fresh-channels";
    fixture = ./fixtures/fresh-channels;
    testScript =
      { nodes, ... }:
      ''
        report = detect()
        assert report["System config"] == "/etc/nixos/configuration.nix (configuration.nix, no flake)", report
        assert report["Channel"] == "${releaseChannel nodes} (stable, from the NixOS version)", report
        assert report["nix-command"] == "not enabled", report
        assert report["flakes"] == "not enabled", report

        # Answering no leaves everything as it was.
        before = machine.succeed("cat /etc/nixos/configuration.nix")
        declined = machine.succeed("echo n | slipway connect")
        assert "+      ./slipway" in declined, declined
        assert "Nothing was changed." in declined, declined
        assert machine.succeed("cat /etc/nixos/configuration.nix") == before
        machine.fail("test -e /etc/nixos/slipway")

        connects()
        machine.fail("test -e /etc/nixos/slipway/flake.nix")
        evaluates()
      '';
  };

  vm-hand-edited = mkSlipwayTest {
    name = "hand-edited";
    fixture = ./fixtures/hand-edited;
    testScript =
      { nodes, ... }:
      ''
        report = detect()
        assert report["System config"] == "/etc/nixos/configuration.nix (configuration.nix, no flake)", report
        assert report["Channel"] == "${releaseChannel nodes} (stable, from the NixOS version)", report

        connects()
        evaluates()
      '';
  };

  vm-existing-imports = mkSlipwayTest {
    name = "existing-imports";
    fixture = ./fixtures/existing-imports;
    testScript = ''
      report = detect()
      assert report["System config"] == "/etc/nixos/flake.nix (flake)", report
      assert report["Channel"] == "${channel} (unstable, from flake.lock)", report

      connects()
      evaluates()
    '';
  };

  vm-already-connected = mkSlipwayTest {
    name = "already-connected";
    fixture = ./fixtures/already-connected;
    testScript = ''
      before = machine.succeed("cat /etc/nixos/configuration.nix /etc/nixos/slipway/default.nix")
      output = machine.succeed("slipway connect --yes")
      assert "doesn't need changing" in output, output
      assert machine.succeed("cat /etc/nixos/configuration.nix /etc/nixos/slipway/default.nix") == before
      evaluates()
    '';
  };
}
