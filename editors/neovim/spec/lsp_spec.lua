---@brief [[
--- Tests for basic (auto-)attaching of LSP clients.
---@brief ]]

local fixtures = require 'spec.fixtures'
local helpers = require 'spec.helpers'

require('tinymist').setup {}

describe('LSP', function()
  assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })

  it('is attached to .typ files within projects', function()
    vim.cmd.edit(fixtures.project.some_existing_file)
    assert.is.same(1, #vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })
  end)

  it(
    'is attached to single .typ files',
    helpers.clean_buffer(function()
      assert.is.same(1, #vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })
    end)
  )

  it('is not attached to non-typst files', function()
    vim.cmd.split 'some_non_typst_file.tmp'
    assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })
    vim.cmd.close { bang = true }
  end)
end)
