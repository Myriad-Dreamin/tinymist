# Docs: ./docs/tinymist/nix.typ
{
  description = "Collecting nix configurations in tinymist repository.";
  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    
    tinymist-unstable.url = "path:contrib/nix/unstable";
    tinymist-dev.url = "path:contrib/nix/dev";
    tinymist-nixvim.url = "path:editors/neovim/samples/nixvim";
  };

  outputs = inputs@{
    flake-parts, tinymist-unstable, tinymist-dev, tinymist-nixvim, self, ... }: flake-parts.lib.mkFlake { inherit inputs; } {
    systems = [ "x86_64-linux" ];
    perSystem =
      { system, ... }:
      {
        # apps.default = tinymist-dev.apps.${system}.default;
        
        devShells = {
          # nix develop
          default = tinymist-dev.devShells.${system}.default;
          # nix develop #neovim
          neovim = tinymist-dev.devShells.${system}.neovim;
          # nix develop .#unstable
          unstable = tinymist-unstable.devShells.${system}.default;
          # nix develop .#nixvim
          nixvim = tinymist-nixvim.devShells.${system}.default;
        };
      };
  };
}