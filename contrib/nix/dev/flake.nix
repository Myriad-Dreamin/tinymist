{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-manifest = {
      url = "https://static.rust-lang.org/dist/channel-rust-1.85.1.toml";
      flake = false;
    };
  };

  outputs = inputs @ { self, flake-parts, nixpkgs, fenix, rust-manifest, }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];
    
      perSystem = {config, lib, pkgs, system, ...}: 
      let
        rust-toolchain = (fenix.packages.${system}.fromManifestFile rust-manifest).defaultToolchain;
        tinymist = pkgs.rustPlatform.buildRustPackage (finalAttrs: {
          pname = "tinymist";
          # Please update the corresponding vscode extension when updating
          # this derivation.
          version = "0.13.17-rc2";

          src = pkgs.lib.cleanSource ../../..;

          useFetchCargoVendor = true;
          cargoHash = "sha256-2cC3yPcywpPzahA+da9ZDCDnpCP6X9WqqVRrfEvHLNA=";

          nativeBuildInputs = [
            pkgs.installShellFiles
            pkgs.pkg-config
          ];

          checkFlags = [
            "--skip=e2e"

            # Require internet access
            "--skip=docs::package::tests::cetz"
            "--skip=docs::package::tests::fletcher"
            "--skip=docs::package::tests::tidy"
            "--skip=docs::package::tests::touying"

            # Tests are flaky for unclear reasons since the 0.12.3 release
            # Reported upstream: https://github.com/Myriad-Dreamin/tinymist/issues/868
            "--skip=analysis::expr_tests::scope"
            "--skip=analysis::post_type_check_tests::test"
            "--skip=analysis::type_check_tests::test"
            "--skip=completion::tests::test_pkgs"
            "--skip=folding_range::tests::test"
            "--skip=goto_definition::tests::test"
            "--skip=hover::tests::test"
            "--skip=inlay_hint::tests::smart"
            "--skip=prepare_rename::tests::prepare"
            "--skip=references::tests::test"
            "--skip=rename::tests::test"
            "--skip=semantic_tokens_full::tests::test"
          ];

          postInstall = lib.optionalString (pkgs.stdenv.hostPlatform.emulatorAvailable pkgs.buildPackages) (
            let
              emulator = pkgs.stdenv.hostPlatform.emulator pkgs.buildPackages;
            in
            ''
              installShellCompletion --cmd tinymist \
                --bash <(${emulator} $out/bin/tinymist completion bash) \
                --fish <(${emulator} $out/bin/tinymist completion fish) \
                --zsh <(${emulator} $out/bin/tinymist completion zsh)
            ''
          );

          nativeInstallCheckInputs = [
            pkgs.versionCheckHook
          ];
          versionCheckProgramArg = "-V";
          doInstallCheck = true;

          meta = {
            description = "Tinymist is an integrated language service for Typst";
            homepage = "https://github.com/Myriad-Dreamin/tinymist";
            changelog = "https://github.com/Myriad-Dreamin/tinymist/blob/v${finalAttrs.version}/editors/vscode/CHANGELOG.md";
            license = lib.licenses.asl20;
            mainProgram = "tinymist";
            maintainers = with lib.maintainers; [
              GaetanLepage
              lampros
            ];
          };
        });
      in {
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
          buildInputs = [
            tinymist
          ];
          shellHook = ''
            echo "Docs: docs/tinymist/nix.typ."
          '';
        };
      };
    };
}