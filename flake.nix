{
  description = "Lific";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane.url = "github:ipetkov/crane";

    bun2nix = {
      url = "github:nix-community/bun2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    bun2nix,
    crane,
    nixpkgs,
  }: let
    systems = [
      "aarch64-darwin"
      "aarch64-linux"
      "x86_64-linux"
    ];
    forAllSystems = nixpkgs.lib.genAttrs systems;
    overlay = final: _prev: {
      lific = final.callPackage ./nix/package.nix {
        src = ./.;
        supportedSystems = systems;
        bun2nix = bun2nix.packages.${final.stdenv.buildPlatform.system}.default;
        craneLib = crane.mkLib final;
      };
    };
    pkgsFor = system:
      import nixpkgs {
        inherit system;
        overlays = [overlay];
      };
  in {
    overlays.default = overlay;

    packages = forAllSystems (system: {
      inherit (pkgsFor system) lific;
      default = (pkgsFor system).lific;
    });

    checks = forAllSystems (
      system: let
        pkgs = pkgsFor system;
      in
        {
          inherit (self.packages.${system}) lific;
        }
        // nixpkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          nixos-module = import ./nix/tests/module.nix {
            inherit
              nixpkgs
              pkgs
              self
              system
              ;
          };
        }
    );

    devShells = forAllSystems (
      system: let
        pkgs = pkgsFor system;
      in {
        default = (crane.mkLib pkgs).devShell {
          packages = [
            pkgs.bun
            bun2nix.packages.${system}.default
            pkgs.cargo-nextest
          ];
        };
      }
    );

    formatter = forAllSystems (system: (pkgsFor system).alejandra);

    nixosModules = {
      default = import ./nix/module.nix {inherit self;};
      lific = self.nixosModules.default;
    };
  };
}
