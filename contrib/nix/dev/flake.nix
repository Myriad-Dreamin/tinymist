# https://wiki.nixos.org/wiki/Flakes
{
  description = "A flake configuration to run tinymist from current repository";
  
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.12";
    systems.url = "github:nix-systems/default";
    nixpkgs.follows = "cargo2nix/nixpkgs";

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

  outputs = { flake-parts, cargo2nix, nixpkgs, self, ... }@inputs: flake-parts.lib.mkFlake { inherit inputs; } {
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

        # rust-toolchain = (fenix.packages.${system}.fromManifestFile rust-manifest).defaultToolchain;

        # Crane-based Nix flake configuration.
        # Based on https://github.com/ipetkov/crane/blob/master/examples/trunk-workspace/flake.nix
        # craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;

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

        # create nixpkgs that contains rustBuilder from cargo2nix overlay
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ cargo2nix.overlays.default ];
        };

        # create the workspace & dependencies package set
        rustPkgs = pkgs.rustBuilder.makePackageSet {
          rustVersion = "1.85.0";
          packageFun = import ../../../Cargo.nix;
        };

        
        tinymist = (rustPkgs.workspace.tinymist {});

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
        # commonCraneArgs = {
        #   inherit src pname version;

        #   buildInputs = (lib.optionals pkgs.stdenv.isDarwin [
        #     pkgs.darwin.apple_sdk.frameworks.CoreServices
        #     pkgs.libiconv
        #   ]);
        #   nativeBuildInputs = [
        #     pkgs.pkg-config
        #   ];
        # };

        # Derivation with just the dependencies, so we don't have to keep
        # re-building them.
        # cargoArtifacts = craneLib.buildDepsOnly commonCraneArgs;
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

        # tinymist = craneLib.buildPackage (commonCraneArgs // {
        #   inherit cargoArtifacts;

        #   nativeBuildInputs = commonCraneArgs.nativeBuildInputs ++ [
        #     pkgs.installShellFiles
        #   ];

        #   # postInstall = ''
        #   #   installManPage crates/typst-cli/artifacts/*.1
        #   #   installShellCompletion \
        #   #     crates/typst-cli/artifacts/typst.{bash,fish} \
        #   #     --zsh crates/typst-cli/artifacts/_typst
        #   # '';

        #   # TYPST_VERSION =
        #   #   let
        #   #     rev = self.shortRev or "dirty";
        #   #     version = cargoToml.workspace.package.version;
        #   #   in
        #   #   "${version} (${rev})";

        #   meta.mainProgram = "tinymist-l10n";
        # });

        
          # rustPlatform.buildRustPackage (finalAttrs: {
          #   pname = "tinymist";
          #   # Please update the corresponding vscode extension when updating
          #   # this derivation.
          #   version = "0.13.14";

          #   src = fetchFromGitHub {
          #     owner = "Myriad-Dreamin";
          #     repo = "tinymist";
          #     tag = "v${finalAttrs.version}";
          #     hash = "sha256-CTZhMbXLL13ybKFC34LArE/OXGfrAnXKXM79DP8ct60=";
          #   };

          #   useFetchCargoVendor = true;
          #   cargoHash = "sha256-aD50+awwVds9zwW5hM0Hgxv8NGV7J63BOSpU9907O+k=";

          #   nativeBuildInputs = [
          #     installShellFiles
          #     pkg-config
          #   ];

          #   checkFlags = [
          #     "--skip=e2e"

          #     # Require internet access
          #     "--skip=docs::package::tests::cetz"
          #     "--skip=docs::package::tests::fletcher"
          #     "--skip=docs::package::tests::tidy"
          #     "--skip=docs::package::tests::touying"

          #     # Tests are flaky for unclear reasons since the 0.12.3 release
          #     # Reported upstream: https://github.com/Myriad-Dreamin/tinymist/issues/868
          #     "--skip=analysis::expr_tests::scope"
          #     "--skip=analysis::post_type_check_tests::test"
          #     "--skip=analysis::type_check_tests::test"
          #     "--skip=completion::tests::test_pkgs"
          #     "--skip=folding_range::tests::test"
          #     "--skip=goto_definition::tests::test"
          #     "--skip=hover::tests::test"
          #     "--skip=inlay_hint::tests::smart"
          #     "--skip=prepare_rename::tests::prepare"
          #     "--skip=references::tests::test"
          #     "--skip=rename::tests::test"
          #     "--skip=semantic_tokens_full::tests::test"
          #   ];

          #   postInstall = lib.optionalString (stdenv.hostPlatform.emulatorAvailable buildPackages) (
          #     let
          #       emulator = stdenv.hostPlatform.emulator buildPackages;
          #     in
          #     ''
          #       installShellCompletion --cmd tinymist \
          #         --bash <(${emulator} $out/bin/tinymist completion bash) \
          #         --fish <(${emulator} $out/bin/tinymist completion fish) \
          #         --zsh <(${emulator} $out/bin/tinymist completion zsh)
          #     ''
          #   );

          #   nativeInstallCheckInputs = [
          #     versionCheckHook
          #   ];
          #   versionCheckProgramArg = "-V";
          #   doInstallCheck = true;

          #   passthru = {
          #     updateScript = nix-update-script { };
          #     tests = {
          #       vscode-extension = vscode-extensions.myriad-dreamin.tinymist;
          #     };
          #   };

          #   meta = {
          #     description = "Tinymist is an integrated language service for Typst";
          #     homepage = "https://github.com/Myriad-Dreamin/tinymist";
          #     changelog = "https://github.com/Myriad-Dreamin/tinymist/blob/v${finalAttrs.version}/editors/vscode/CHANGELOG.md";
          #     license = lib.licenses.asl20;
          #     mainProgram = "tinymist";
          #     maintainers = with lib.maintainers; [
          #       GaetanLepage
          #       lampros
          #     ];
          #   };
          # })
      in rec
      {
        formatter = pkgs.nixpkgs-fmt;

        # packages = {
        #   default = tinymist;
        #   tinymist-dev = self'.packages.default;
        # };

        # overlayAttrs = builtins.removeAttrs self'.packages [ "default" ];

        packages = rec {
          default = tinymist;
          # for legacy users
          shell = devShells.default;
        };

        apps.default = {
          type = "app";
          program = "${packages.default}/bin/tinymist";
        };

        # checks = {
        #   tinymist-fmt = craneLib.cargoFmt commonCraneArgs;
        #   tinymist-clippy = craneLib.cargoClippy (commonCraneArgs // {
        #     inherit cargoArtifacts;
        #     cargoClippyExtraArgs = "--workspace -- --deny warnings";
        #   });
        #   tinymist-test = craneLib.cargoTest (commonCraneArgs // {
        #     inherit cargoArtifacts;
        #     cargoTestExtraArgs = "--workspace";
        #   });
        # };

        devShells.default = pkgs.mkShell {
          # inherit (commonCraneArgs) nativeBuildInputs buildInputs;
          # checks = self'.checks;
          # inputsFrom = [ tinymist ];

          packages = [
            tinymist
          ];

          shellHook = ''
            echo "Docs: ./docs/tinymist/nix.typ"
            echo "Got tinymist from current repository."
          '';
        };
      };
  };
}