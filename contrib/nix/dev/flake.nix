# https://wiki.nixos.org/wiki/Flakes
{
  description = "A flake configuration to run tinymist from current repository";
  
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.87.0.toml";
      flake = false;
    };
  };

  outputs = { flake-parts, crane, nixpkgs, fenix, rust-manifest, self, ... }@inputs: flake-parts.lib.mkFlake { inherit inputs; } {
    # systems = import inputs.systems;
    systems = [ "x86_64-linux" ];

    imports = [
      inputs.flake-parts.flakeModules.easyOverlay
    ];

  # self', pkgs, lib, 
    perSystem = { self', system, pkgs, lib, ... }:
      let
        PROJECT_ROOT = ../../..;
        cargoToml = lib.importTOML "${PROJECT_ROOT}/Cargo.toml";

        pname = "tinymist-l10n";
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
            ../../../crates/tinymist-l10n
            ../../../docs
            ../../../tests
          ];
        };

        # src = craneLib.cleanCargoSource ../../..;

        # cargoLock = ../../../Cargo.lock;

        internal-crates = {
          tinymist-derive = { path = "${PROJECT_ROOT}/crates/tinymist-derive"; version = "0.13.12"; };
          tinymist-l10n = { path = "${PROJECT_ROOT}/crates/tinymist-l10n"; version = "0.13.12"; };
          tinymist-package = { path = "${PROJECT_ROOT}/crates/tinymist-package"; version = "0.13.12"; };
          tinymist-std = { path = "${PROJECT_ROOT}/crates/tinymist-std"; version = "0.13.12"; default-features = false; };
          tinymist-vfs = { path = "${PROJECT_ROOT}/crates/tinymist-vfs"; version = "0.13.12"; default-features = false; };
          tinymist-world = { path = "${PROJECT_ROOT}/crates/tinymist-world"; version = "0.13.12"; default-features = false; };
          tinymist-project = { path = "${PROJECT_ROOT}/crates/tinymist-project"; version = "0.13.12"; };
          tinymist-task = { path = "${PROJECT_ROOT}/crates/tinymist-task"; version = "0.13.12"; };
          typst-shim = { path = "${PROJECT_ROOT}/crates/typst-shim"; version = "0.13.12"; };
        };

        # Typst derivation's args, used within crane's derivation generation
        # functions.
        commonCraneArgs = {
          inherit src pname version;

          buildInputs = (lib.optionals pkgs.stdenv.isDarwin [
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
        # # This is need
        # cargoVendorDir = craneLib.vendorCargoDeps (commonCraneArgs // {
          
        #   # Use this function to override crates coming from any registry checkout
        #   overrideVendorCargoPackage = p: drv:
        #     # a random package that we depend
        #     if p.name == "typst" then
        #       # adds the internal crate to the vendor directory
        #       drv.overrideAttrs (oldAttrs: {
        #         src = self'.src;
        #         cargoLock = self'.cargoLock;
        #         cargoVendorDir = self'.cargoVendorDir;
        #         cargoVendorCargoPackages = self'.internal-crates;
        #       }) // oldAttrs
        #       drv
        #     else drv;
        # });

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

          # TYPST_VERSION =
          #   let
          #     rev = self.shortRev or "dirty";
          #     version = cargoToml.workspace.package.version;
          #   in
          #   "${version} (${rev})";

          meta.mainProgram = "tinymist-l10n";
        });
      in
      {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
          default = tinymist;
          tinymist-dev = self'.packages.default;
        };

        overlayAttrs = builtins.removeAttrs self'.packages [ "default" ];

        apps.default = {
          type = "app";
          program = lib.getExe tinymist;
        };

        checks = {
          tinymist-fmt = craneLib.cargoFmt commonCraneArgs;
          tinymist-clippy = craneLib.cargoClippy (commonCraneArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--workspace -- --deny warnings";
          });
          tinymist-test = craneLib.cargoTest (commonCraneArgs // {
            inherit cargoArtifacts;
            cargoTestExtraArgs = "--workspace";
          });
        };

        devShells.default = craneLib.devShell {
          inherit (commonCraneArgs) nativeBuildInputs buildInputs;
          checks = self'.checks;
          inputsFrom = [ tinymist ];

          packages = [
            # A script for quickly running tests.
            # See https://github.com/typst/typst/blob/main/tests/README.md#making-an-alias
            # (pkgs.writeShellScriptBin "testit" ''
            #   cargo test --workspace --test tests -- "$@"
            # '')
          ];

          shellHook = ''
            echo "Docs: ./docs/tinymist/nix.typ"
            echo "Got tinymist from current repository."
          '';
        };
      };
  };
}