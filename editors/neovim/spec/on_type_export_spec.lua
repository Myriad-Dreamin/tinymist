---@brief [[
--- Tests for export functionalities.
---@brief ]]

local fixtures = require 'spec.fixtures'
local helpers = require 'spec.helpers'

-- async async
local util = require "plenary.async.util"
local async = require('plenary.async')

local async_tests = require "plenary.async.tests"

require('tinymist').setup {
  lsp = {
    init_options = {
        exportPdf = 'onType',
        outputPath = '/home/runner/test/$name',
        development = true,
        systemFonts = false,
    },
  }
}

local defer_swapped = function(timeout, callback)
  vim.defer_fn(callback, timeout)
end

async_tests.describe('Export', function()
  assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })

  async_tests.it('pdf is created onType', function()
    local pdf_path = '/home/runner/test/main.pdf'
    assert.is.same(nil, vim.uv.fs_stat(pdf_path), 'PDF file should not be created before testing')
    print(vim.inspect(async))

    local pdf_exported = async.wrap(function(cb)
      require('tinymist').subscribeDevEvent(
        function(result)
          if result.type == 'export' and result.needExport
          then
            cb(result) -- resolve the promise when the export event is received
            return true -- unregister the callback after receiving the event
          end
        end)

        -- defer 2000ms and resolve a nil
        defer_swapped(2000, function()
          cb(nil) -- resolve the promise after 2 seconds
        end)

        vim.cmd.edit(fixtures.project.some_existing_file)
        assert.is.same(1, #vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })
        --- append a text to current buffer
        helpers.insert('This is a test export.\n')
        vim.cmd.sleep('30m')
        --- append a text to current buffer
        helpers.insert('This is a test export.\n')
        vim.cmd.sleep('30m')

    end, 1)()

    assert.is_not.same(nil, pdf_exported, 'PDF export should be triggered on type')
    assert.is.same('onType', pdf_exported.when, 'Export is when = onType')
    assert.is_not.same(nil, vim.uv.fs_stat(pdf_path), 'PDF file should be created after typing')
  end)
end)
