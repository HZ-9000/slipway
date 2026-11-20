# alice's laptop -- started from the installer's config, been
# tweaking it ever since.

{ config, pkgs, lib, ... }:

let
  # TODO: move these into their own file someday
  devTools = with pkgs; [ git ripgrep fd jq ];
  isLaptop = true;
in
{
  imports = [ ./hardware-configuration.nix ];

  boot.loader = {
    systemd-boot.enable = true;
    systemd-boot.configurationLimit = 10;
    efi.canTouchEfiVariables = true;
  };

  networking = { hostName = "thinkpad"; networkmanager.enable = true; };

  time.timeZone = "Europe/Berlin";
  i18n.defaultLocale="en_GB.UTF-8";

  services.xserver.enable = true;
  services.displayManager.gdm.enable = true;
  services.desktopManager.gnome.enable = true;

  services.tlp.enable = isLaptop;   # battery life
  services.power-profiles-daemon.enable = false; # gnome turns this on, fights with tlp
  powerManagement.enable = lib.mkIf isLaptop true;

  users.users.alice = {
    isNormalUser = true;
    extraGroups = [ "wheel" "networkmanager" "video" ];
    shell = pkgs.zsh;
  };
  programs.zsh.enable = true;

  environment.systemPackages = with pkgs; [
    firefox
    vim   # editor
    # vscode -- unfree, enable later
  ] ++ devTools;

  # services.printing.enable = true;

  nix.gc = { automatic = true; dates = "weekly"; options = "--delete-older-than 14d"; };

  system.stateVersion = "24.11"; # first installed on 24.11, do not bump
}
