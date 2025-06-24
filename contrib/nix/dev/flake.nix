{
  inputs = {
    # nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.12";
    # flake-utils.follows = "cargo2nix/flake-utils";
    nixpkgs.follows = "cargo2nix/nixpkgs";
  };

  outputs = inputs @ { self, flake-parts, cargo2nix, nixpkgs, }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = ["x86_64-linux"];
      perSystem = {config, system, lib, pkgs, ...}: 
      let 
        pkgs = import nixpkgs {
          inherit system;
          overlays = [cargo2nix.overlays.default];
        };
        rustPkgs = pkgs.rustBuilder.makePackageSet {
          rustVersion = "1.85.0";
          packageFun = import ../../../Cargo.nix;
          target = "x86_64-unknown-linux-musl";
          workspaceSrc = ../../..;
        };
        # replace hello-world with your package name
        typlite = (rustPkgs.workspace.typlite {});
      in {
        # export the project devshell as the default devshell
        devShells.default = pkgs.mkShell {
          buildInputs = [
            typlite
          ];
          shellHook = ''
            echo "Docs: docs/tinymist/nix.typ."
          '';
        };
      };
    };
}