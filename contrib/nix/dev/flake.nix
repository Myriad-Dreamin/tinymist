{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.88.0.toml";
      flake = false;
    };
  };

  outputs = inputs @ { self, flake-parts, nixpkgs, crane, fenix, rust-manifest, }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];
    
      perSystem = {self', config, lib, pkgs, system, ...}: 
      let
        cargoToml = lib.importTOML ../../../Cargo.toml;
        pname = "tinymist";
        version = cargoToml.workspace.package.version;
        rust-toolchain = (fenix.packages.${system}.fromManifestFile rust-manifest).defaultToolchain;
        # Crane-based Nix flake configuration.
        # Based on https://github.com/ipetkov/crane/blob/master/examples/trunk-workspace/flake.nix
        craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;
        # Typst files to include in the derivation.
        # Here we include Rust files, docs and tests.
        src = lib.fileset.toSource {
          root = ../../..;
          fileset = lib.fileset.unions [
            ../../../Cargo.toml
            ../../../Cargo.lock
            ../../../rust-toolchain.toml
            ../../../crates
            ../../../tests
          ];
        };

        # Typst derivation's args, used within crane's derivation generation
        # functions.
        commonCraneArgs = {
          inherit src pname version;

          buildInputs = [
          ] ++ (lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.CoreServices
            pkgs.libiconv
          ]);

          nativeBuildInputs = [
            pkgs.pkg-config
          ];
        };

        # Derivation with just the dependencies, so we don't have to keep
        # re-building them.
        cargoArtifacts = craneLib.buildDepsOnly commonCraneArgs;

        tinymist = craneLib.buildPackage (commonCraneArgs // {
          inherit cargoArtifacts;

          nativeBuildInputs = commonCraneArgs.nativeBuildInputs ++ [
            pkgs.installShellFiles
          ];

          # postInstall = ''
          #   installManPage crates/typst-cli/artifacts/*.1
          #   installShellCompletion \
          #     crates/typst-cli/artifacts/typst.{bash,fish} \
          #     --zsh crates/typst-cli/artifacts/_typst
          # '';

          # postInstall = lib.optionalString (pkgs.stdenv.hostPlatform.emulatorAvailable pkgs.buildPackages) (
          #   let
          #     emulator = pkgs.stdenv.hostPlatform.emulator pkgs.buildPackages;
          #   in
          #   ''
          #     installShellCompletion --cmd tinymist \
          #       --bash <(${emulator} $out/bin/tinymist completion bash) \
          #       --fish <(${emulator} $out/bin/tinymist completion fish) \
          #       --zsh <(${emulator} $out/bin/tinymist completion zsh)
          #   ''
          # );

          GEN_ARTIFACTS = "artifacts";
          meta.mainProgram = "tinymist";
        });

      in {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
          default = tinymist;
          tinymist-dev = self'.packages.default;
        };

        # overlayAttrs = builtins.removeAttrs self'.packages [ "default" ];

        apps.default = {
          type = "app";
          program = lib.getExe tinymist;
        };

        # export the project devshell as the default devshell
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-analyzer
            nodejs_24
            (yarn.override { nodejs = nodejs_24; })
          ];

          shellHook = ''
            echo "Docs: docs/tinymist/nix.typ."
          '';
        };
        # Developing neovim integration requires a fresh tinymist binary
        devShells.neovim = pkgs.mkShell {
          buildInputs = [ tinymist ];
          shellHook = ''
            echo "binary installed."
            echo "Docs: docs/tinymist/nix.typ."
          '';
        };
      };
    };
}