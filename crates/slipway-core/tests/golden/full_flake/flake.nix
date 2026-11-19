# This file is managed by Slipway.
#
# Slipway rewrites it whenever you change your setup, so edits made here
# will be lost. Put your own changes in your own flake.nix (outside this
# folder) or a module it lists; NixOS combines them with the settings here.
#
# Packages and options here are chosen from the nixos-unstable channel.

{
  description = "Packages and settings managed by Slipway";

  # This flake has no inputs: its module uses the nixpkgs of the flake that
  # imports it, so your system never ends up with two copies of nixpkgs.
  outputs =
    { self }:
    {
      # The NixOS module your flake adds to its `modules` list, for example
      # `modules = [ slipway.nixosModules.default ./configuration.nix ];`
      nixosModules.default = import ./default.nix;
    };
}
