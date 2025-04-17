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
          root_dir = function()
            return vim.fn.getcwd()
          end,
          --- See [Tinymist Server Configuration](https://github.com/Myriad-Dreamin/tinymist/blob/main/Configuration.md) for references.
          settings = {
            --- You could set the formatter mode to use lsp-enhanced formatters.
            -- formatterMode = "typstyle",
            
            --- If you love to edit the documents and preview exported pdfs in the same time,
            --- you could set this to `onType`.
            -- exportPdf = "onType",
          },
          on_attach = function(client, bufnr)
              vim.keymap.set("n", "<leader>tp", function()
                  client:exec_cmd({
                      title = "pin",
                      command = "tinymist.pinMain",
                      arguments = { vim.api.nvim_buf_get_name(0) },
                  }, { bufnr = bufnr })
              end, { desc = "[T]inymist [P]in", noremap = true })
      
              vim.keymap.set("n", "<leader>tu", function()
                  client:exec_cmd({
                      title = "unpin",
                      command = "tinymist.pinMain",
                      arguments = { vim.v.null },
                  }, { bufnr = bufnr })
              end, { desc = "[T]inymist [U]npin", noremap = true })
          end,
        },
      },
    },
  },
}
