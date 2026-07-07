---@brief [[
--- Tests for document summary data.
---@brief ]]

local fixtures = require 'spec.fixtures'
local helpers = require 'spec.helpers'

require('tinymist').setup {
  lsp = {
    init_options = {
      systemFonts = false,
    },
  },
}

local function tinymist_client()
  return vim.lsp.get_clients { bufnr = 0, name = 'tinymist' }[1]
end

local function execute_command(client, command, arguments, timeout)
  local responses = vim.lsp.buf_request_sync(0, 'workspace/executeCommand', {
    command = command,
    arguments = arguments or {},
  }, timeout or 2000)

  if not responses then
    return nil, 'timed out waiting for response'
  end

  local response = responses[client.id]
  if not response then
    return nil, 'tinymist did not respond'
  end

  assert.is_nil(response.err, vim.inspect(response.err))
  return response.result, 'nil result'
end

local function wait_for_command_result(client, command, arguments)
  local result
  local last_status = 'request was not sent'
  local succeeded = vim.wait(15000, function()
    result, last_status = execute_command(client, command, arguments)
    return result ~= nil
  end, 100)

  assert.message(('%s never returned a result: %s'):format(command, last_status)).True(succeeded)
  return result
end

local function has_used_font(font_info)
  for _, font in ipairs(font_info) do
    if (font.usesScale or 0) > 0 then
      return true
    end
  end

  return false
end

local function has_project_server_info(server_info)
  for _, info in pairs(server_info) do
    if info.root ~= nil and type(info.stats) == 'table' then
      return true
    end
  end

  return false
end

describe('Document summary', function()
  assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })

  it('provides current document metrics and server info', function()
    vim.cmd.edit(fixtures.project.some_existing_file)
    helpers.wait_for_ready_lsp()

    local client = tinymist_client()
    assert.is_not_nil(client)

    local path = vim.api.nvim_buf_get_name(0)
    local metrics = wait_for_command_result(client, 'tinymist.getDocumentMetrics', { path })
    local server_info = wait_for_command_result(client, 'tinymist.getServerInfo')

    assert.is.same('table', type(metrics.fontInfo))
    assert.is_true(has_used_font(metrics.fontInfo), 'summary metrics should include used font data')
    assert.is.same('table', type(server_info))
    assert.is_true(has_project_server_info(server_info), 'summary should include project server info')
  end)
end)
