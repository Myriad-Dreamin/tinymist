# https://wiki.nixos.org/wiki/Flakes
{
  description = "A flake configuration to use tinymist CLI from unstable nixpkgs";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs = { self, nixpkgs }: 
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      devShells.x86_64-linux.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          tinymist
        ];
        shellHook = ''
          echo "Got tinymist from nixpkgs unstable channel"
        '';
      };
    };
}