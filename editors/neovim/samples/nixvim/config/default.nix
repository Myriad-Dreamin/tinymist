# https://github.com/GaetanLepage/nix-config/blob/81a6c06fa6fc04a0436a55be344609418f4c4fd9/modules/home/core/dev/typst.nix#L22
{
  # Import all your configuration modules here
  imports = [ 
    ./bufferline.nix
    ./tinymist.nix
    # ./typst-preview.nix
    # ./typst-vim.nix
  ];
}
