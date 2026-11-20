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
      '';
  };

  vm-existing-imports = mkSlipwayTest {
    name = "existing-imports";
    fixture = ./fixtures/existing-imports;
    testScript = ''
      report = detect()
      assert report["System config"] == "/etc/nixos/flake.nix (flake)", report
      assert report["Channel"] == "${channel} (unstable, from flake.lock)", report
    '';
  };
}
