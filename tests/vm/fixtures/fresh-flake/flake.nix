{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/@channel@";

  outputs = { self, nixpkgs }: {
    nixosConfigurations.nixos = nixpkgs.lib.nixosSystem {
      modules = [ ./configuration.nix ];
    };
  };
}
