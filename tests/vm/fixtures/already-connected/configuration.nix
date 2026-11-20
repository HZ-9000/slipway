# Connected to Slipway by hand, with the import spelled out in full and
# not at the top of the list.
{ config, pkgs, ... }:

{
  imports = [
    ./hardware-configuration.nix
    ./slipway/default.nix
  ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;

  networking.hostName = "nixos";

  users.users.alice = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
  };

  system.stateVersion = "26.05";
}
