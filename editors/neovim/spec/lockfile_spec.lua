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

async_tests.describe('Lockfile', function()
  assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })

  async_tests.it('pdf of main is created onType', function()
  end)
end)
