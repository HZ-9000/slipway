{
  inputs.slipway.url = "path:./slipway";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/@channel@";

  outputs = { self, nixpkgs, slipway }: {
    nixosConfigurations.nixos = nixpkgs.lib.nixosSystem {
      modules = [ slipway.nixosModules.default ./configuration.nix ];
    };
  };
}
