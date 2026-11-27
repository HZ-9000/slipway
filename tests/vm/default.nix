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

      # Packages: what slipway keeps, what it writes, and what Nix makes of
      # the two together.
      assert packages() == []
      added = machine.succeed("slipway add hello")
      assert "Added hello" in added, added
      assert packages() == ["hello"]
      assert "pkgs.hello" in machine.succeed("cat /etc/nixos/slipway/default.nix")
      checks()

      again = machine.succeed("slipway add hello")
      assert "already one of your packages" in again, again
      assert packages() == ["hello"]

      # A name the channel doesn't have is refused, and nothing is written.
      before = machine.succeed("cat /etc/nixos/slipway/default.nix")
      refused = machine.fail("slipway add fierfox 2>&1")
      assert "no package called `fierfox`" in refused, refused
      assert packages() == ["hello"]
      assert machine.succeed("cat /etc/nixos/slipway/default.nix") == before

      removed = machine.succeed("slipway remove hello")
      assert "Removed hello" in removed, removed
      assert packages() == []
      assert "systemPackages = [ ];" in machine.succeed("cat /etc/nixos/slipway/default.nix")
      checks()

      # Slipway only removes what it put there; the rest is the user's own.
      theirs = machine.fail("slipway remove vim 2>&1")
      assert "isn't looking after `vim`" in theirs, theirs
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

        # Nix's older commands answer the same questions on a machine with
        # none of the new command line turned on.
        assert packages() == []
        machine.succeed("slipway add hello")
        assert packages() == ["hello"]
        assert "pkgs.hello" in machine.succeed("cat /etc/nixos/slipway/default.nix")
        checks()

        refused = machine.fail("slipway add fierfox 2>&1")
        assert "no package called `fierfox`" in refused, refused
        assert packages() == ["hello"]

        # A setting that doesn't exist is explained rather than dumped, and
        # Nix's own words are kept for whoever asks for them.
        machine.succeed("cp /etc/nixos/configuration.nix /tmp/configuration.nix")
        machine.succeed(
            "sed -i '0,/^}/s/^}/  services.notarealservice.enable = true;\\n}/' /etc/nixos/configuration.nix"
        )
        explained = machine.fail("slipway check 2>&1")
        assert "a setting called `services.notarealservice`" in explained, explained
        assert "Try:" in explained, explained
        assert "search.nixos.org/options" in explained, explained
        assert "while evaluating" not in explained, explained
        assert "slipway check --details" in explained, explained

        detailed = machine.fail("slipway check --details 2>&1")
        assert "Nix said:" in detailed, detailed
        assert "does not exist" in detailed, detailed

        machine.succeed("cp /tmp/configuration.nix /etc/nixos/configuration.nix")
        checks()
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

      # A folder Slipway wrote on an older channel is still its own to
      # change, and changing it brings the channel up to date.
      assert packages() == []
      machine.succeed("slipway add hello")
      assert packages() == ["hello"]
      written = machine.succeed("cat /etc/nixos/slipway/default.nix")
      assert "pkgs.hello" in written, written
      assert "nixos-24.11" not in written, written
    '';
  };
}
