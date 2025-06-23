# Docs: ./docs/tinymist/nix.typ
{
  description = "Collecting nix configurations in tinymist repository.";
  inputs = {
    tinymist-unstable.url = "path:./contrib/nix/unstable";
    tinymist-dev.url = "path:./contrib/nix/dev";
    tinymist-nixvim.url = "path:./editors/nixvim";
  };

  outputs =
    { self, tinymist-unstable, tinymist-dev, tinymist-nixvim, ... }@inputs:
    {
      devShells = {
        # nix develop
        x86_64-linux.default = tinymist-dev.devShells.x86_64-linux.default;
        # nix develop .#unstable
        x86_64-linux.unstable = tinymist-unstable.devShells.x86_64-linux.default;
        # nix develop .#nixvim
        x86_64-linux.nixvim = tinymist-nixvim.devShells.x86_64-linux.default;
      };
    };
}