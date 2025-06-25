return {
  {
    'Julian/lean.nvim',
    dependencies = {
      'nvim-lua/plenary.nvim',
      'neovim/nvim-lspconfig',
    },
    ---@module 'lean'
    ---@type lean.Config
    opts = {
      infoview = {
        horizontal_position = 'top',
        show_processing = false,
      },
      mappings = true,
    },
  },
}
