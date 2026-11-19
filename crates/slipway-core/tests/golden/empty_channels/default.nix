# This file is managed by Slipway.
#
# Slipway rewrites it whenever you change your setup, so edits made here
# will be lost. Put your own changes in /etc/nixos/configuration.nix
# instead; NixOS combines them with the settings here.
#
# Packages and options here are chosen from the nixos-24.11 channel.

# A NixOS module: NixOS calls this function with your package set (`pkgs`)
# and merges the settings it returns into the rest of your configuration.
{ pkgs, ... }:

{
  # Software installed for everyone on this computer. Each `pkgs.<name>`
  # is one package; add or remove them with `slipway add` and
  # `slipway remove`.
  environment.systemPackages = [ ];
}
