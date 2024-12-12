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
      "williamboman/mason-lspconfig.nvim",
    },
    ---@class PluginLspOpts
    opts = {
      ---@type lspconfig.options
      servers = {
        tinymist = {
          --- todo: these configuration from lspconfig maybe broken
          single_file_support = true,
          root_dir = function()
            return vim.fn.getcwd()
          end,
          --- See [Tinymist Server Configuration](https://github.com/Myriad-Dreamin/tinymist/blob/main/Configuration.md) for references.
          settings = {
            
            -- Please don't edit following internal settings if you don't know what you are doing.
            -- Neovim 0.9.1 supported these builitin commands
            -- editor.action.triggerSuggest
            triggerSuggest = vim.fn.has("nvim-0.9.1"),
            -- editor.action.triggerParameterHints
            triggerParameterHints = vim.fn.has("nvim-0.9.1"),
            -- tinymist.triggerSuggestAndParameterHints which combines the above two commands.
            triggerSuggestAndParameterHints = vim.fn.has("nvim-0.9.1"),
          },
          -- todo: this is not a correct implementation
          commands = {
            "tinymist.triggerSuggestAndParameterHints" = {
              function()
                vscode_neovim.action("editor.action.triggerSuggest")
                vscode_neovim.action("editor.action.triggerParameterHints")
              end,
              desc = "Trigger Suggest and Parameter Hints",
            },
          },
        },
      },
    },
  },
}
