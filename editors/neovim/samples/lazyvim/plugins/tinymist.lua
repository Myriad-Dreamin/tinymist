-- https://github.com/neovim/neovim/blob/381806729db1016106b02d866c4f4f64f76a351f/src/nvim/highlight_group.c
local links = {
  -- Unable to pick a suitable highlight group for the following:
  -- ['@lsp.type.raw.typst'] = '@markup.link',
  -- ['@lsp.type.punct.typst'] = '@punctuation',
  -- ['@lsp.mod.math.typst'] = '@',
  -- "*.strong.emph": [
  --     "markup.bold.typst markup.italic.typst"
  -- ],

  ['@lsp.mod.strong.typst'] = '@markup.strong',
  ['@lsp.mod.emph.typst'] = '@markup.italic',

  ['@lsp.type.bool.typst'] = '@boolean',
  ['@lsp.type.escape.typst'] = '@string.escape',
  ['@lsp.type.link.typst'] = '@markup.link',
  ['@lsp.typemod.delim.math.typst'] = '@punctuation',
  ['@lsp.typemod.operator.math.typst'] = '@operator',
  ['@lsp.type.heading.typst'] = '@markup.heading',
  ['@lsp.type.pol.typst'] = '@variable',
  ['@lsp.type.error.typst'] = 'DiagnosticError',
  ['@lsp.type.term.typst'] = '@markup.bold',
  ['@lsp.type.marker.typst'] = '@punctuation',
  ['@lsp.type.ref.typst'] = '@label',
  ['@lsp.type.label.typst'] = '@label',
}

for newgroup, oldgroup in pairs(links) do
  vim.api.nvim_set_hl(0, newgroup, { link = oldgroup, default = true })
end

return {
  -- requires tinymist
  {
    "williamboman/mason.nvim",
    version = "^1.0.0",
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
          single_file_support = true, -- Fixes LSP attachment in non-Git directories
          --- See [Tinymist Server Configuration](https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/neovim/Configuration.md) for references.
          settings = {
            --- You could set the formatter mode to use lsp-enhanced formatters.
            -- formatterMode = "typstyle",

            --- If you would love to preview exported PDF files at the same time,
            --- you could set this to `onType` and open the file with your favorite PDF viewer.
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
