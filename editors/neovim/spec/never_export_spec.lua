---@brief [[
--- Tests for export functionalities.
---@brief ]]

local fixtures = require 'spec.fixtures'
local helpers = require 'spec.helpers'

require('tinymist').setup {
  lsp = {
    init_options = {
        exportPdf = 'onSave',
        systemFonts = false,
    },
  }
}

describe('Export', function()
  assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })

  it('no pdf is created never', function()
    vim.cmd.edit(fixtures.project.some_existing_file)
    assert.is.same(1, #vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })
    --- append a text to current buffer
    helpers.insert('This is a test export.\n')
    -- sleep 300ms
    vim.cmd.sleep('300m')
    -- there *must not be* a pdf file created, because we only export on save
    local pdf_path = fixtures.project.some_existing_file:gsub('%.typ$', '.pdf')
    assert.is_false(vim.loop.fs_stat(pdf_path), 'PDF file should not be created without saving because exportPdf = never')
  end)
end)
