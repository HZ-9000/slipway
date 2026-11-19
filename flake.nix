{
  description = "Slipway — a GUI/TUI for building and configuring NixOS flakes";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      cargoToml = nixpkgs.lib.importTOML ./Cargo.toml;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "slipway";
            inherit (cargoToml.workspace.package) version;

            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            # Slipway shells out to whatever `nix` is on PATH, so it needs
            # no Nix libraries to build and none at runtime either.

            # Tests run in CI via `cargo test`; the sandbox has no usable Nix store or $HOME.
            doCheck = false;
          };
        }
      );

      checks = forAllSystems (system: {
        package = self.packages.${system}.default;
      });

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = [
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt

              pkgs.nixfmt

              # Only so the tests have a `nix` to talk to; Slipway builds
              # without it.
              pkgs.nix
            ];
          };
        }
      );
    };
}
