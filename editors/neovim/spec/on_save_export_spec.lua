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
        exportPdf = 'onSave',
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
  
  async_tests.it('pdf is created onSave', function()
    --- keep edit the same file, and save it multiple times, and we should get a sequence of distinct pdf files.
    --- If not, either export is not triggered, or the save events are not emitted.

    local pdf_path = '/home/runner/test/main.pdf'
    assert.is.same(nil, vim.uv.fs_stat(pdf_path), 'PDF file should not be created before testing')

    local pdf_hashes = {}

  
    function export_pdf(index)
        local pdf_exported = async.wrap(function(cb)
        require('tinymist').subscribeDevEvent(
            function(result)
            if result.type == 'export' and result.needExport and result.when == 'onSave'
            then
                -- read the pdf file and calculate the sha256 hash
                local pdf_content = vim.uv.fs_read_file(pdf_path)
                local hash = vim.fn.sha256(pdf_content)
                result.pdf_hash = hash
                pdf_hashes[index] = hash
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
            -- save the file
            vim.cmd.write()
            vim.cmd.sleep('30m')

        end, 1)()

        assert.is_not.same(nil, pdf_exported, 'PDF export should be triggered on save')
        assert.is.same('onSave', pdf_exported.when, 'Export is when = onSave')
        assert.is_not.same(nil, vim.uv.fs_stat(pdf_path), 'PDF file should be created after saving')

    end
    
    for i = 1, 10 do
        export_pdf(i)
    end
   
    assert.is.same(10, #pdf_hashes, 'PDF hashes should be calculated')
    for i = 1, 10 do
        assert.is_not.same(nil, pdf_hashes[i], 'PDF hash should be calculated')
    end

    for i = 1, 9 do
        assert.is_not.same(pdf_hashes[i], pdf_hashes[i + 1], 'PDF hashes should be different')
    end
  end)
end)
