---@type LazySpec[]
return {
  -- requires tinymist
  {
    "williamboman/mason.nvim",
    opts = {
      ensure_installed = {
        "tinymist",
      },
    },
  },
  -- add tinymist to lspconfig
  {
    "neovim/nvim-lspconfig",
    dependencies = {
      "mason.nvim",
      "nvim-lua/plenary.nvim",
      "williamboman/mason-lspconfig.nvim",
    },
    config = function()
      local lspconfig = require "lspconfig"
      local Path = require "plenary.path"

      lspconfig.tinymist.setup {
        --- todo: these configuration from lspconfig maybe broken
        single_file_support = true,
        root_dir = function()
          if vim.env.TYPST_ROOT ~= nil then
            local typst_root = Path:new(vim.env.TYPST_ROOT)
            if typst_root:exists() then return typst_root:absolute() end
          end
          return vim.fn.getcwd()
        end,
        --- See [Tinymist Server Configuration](https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/neovim/Configuration.md) for references.
        settings = {},
      }
    end,
  },
}
