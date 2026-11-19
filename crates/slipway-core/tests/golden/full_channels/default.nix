# This file is managed by Slipway.
#
# Slipway rewrites it whenever you change your setup, so edits made here
# will be lost. Put your own changes in /etc/nixos/configuration.nix
# instead; NixOS combines them with the settings here.
#
# Packages and options here are chosen from the nixos-unstable channel.

# A NixOS module: NixOS calls this function with your package set (`pkgs`)
# and merges the settings it returns into the rest of your configuration.
{ pkgs, ... }:

{
  # Software installed for everyone on this computer. Each `pkgs.<name>`
  # is one package; add or remove them with `slipway add` and
  # `slipway remove`.
  environment.systemPackages = [
    pkgs."7zz"
    pkgs.firefox
    pkgs.git
    pkgs.python3Packages.requests
  ];

  # Turns on `programs.bash` and sets the options below.
  # What it does, and every option it has: https://search.nixos.org/options?query=programs.bash
  programs.bash = {
    enable = true;
    edge-cases = [
      (-1)
      {
        "in" = true;
      }
    ];
    interactiveShellInit = [ ];
    promptInit = "PS1=\"\${USER} \\$ \"\n";
    shellAliases = { };
  };

  # Turns on `services.openssh` and sets the options below.
  # What it does, and every option it has: https://search.nixos.org/options?query=services.openssh
  services.openssh = {
    enable = true;
    ports = [
      22
      2222
    ];
    settings = {
      PasswordAuthentication = false;
      PermitRootLogin = "no";
    };
  };

  # Turns on `services.printing`.
  # What it does, and every option it has: https://search.nixos.org/options?query=services.printing
  services.printing = {
    enable = true;
  };
}
