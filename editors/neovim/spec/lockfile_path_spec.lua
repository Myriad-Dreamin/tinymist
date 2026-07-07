---@brief [[
--- Regression coverage for tinymist#2489.
---@brief ]]

local fixtures = require 'spec.fixtures'

local log_path = '/tmp/tinymist-lockfile-path.log'
local wrapper_path = '/tmp/tinymist-lockfile-path-wrapper.sh'

local function absolute_path(path)
  return vim.fs.normalize(vim.fn.fnamemodify(path, ':p'))
end

local book_root = absolute_path(fixtures.project.root)
local zed_like_root = absolute_path(fixtures.project.some_nested_existing_file)

local function file_uri_from_path(path)
  return 'file://' .. path:gsub('%%', '%%25'):gsub(' ', '%%20')
end

local function write_tinymist_wrapper()
  vim.fn.mkdir(vim.fs.dirname(log_path), 'p')
  vim.fn.delete(log_path)
  vim.fn.writefile({
    '#!/usr/bin/env sh',
    ('TINYMIST_LOG="${TINYMIST_LOG:-tinymist::route=info,tinymist::input=info}" exec tinymist "$@" 2>>%s'):format(vim.fn.shellescape(log_path)),
  }, wrapper_path)
  vim.fn.setfperm(wrapper_path, 'rwxr-xr-x')
end

local function read_log()
  if vim.uv.fs_stat(log_path) == nil then
    return ''
  end
  return table.concat(vim.fn.readfile(log_path), '\n')
end

write_tinymist_wrapper()

require('tinymist').setup {
  lsp = {
    cmd = { wrapper_path, 'lsp' },
    init_options = {
      projectResolution = 'lockDatabase',
      development = true,
      systemFonts = false,
    },
    before_init = function(params)
      local root_uri = file_uri_from_path(zed_like_root)
      params.rootUri = root_uri
      params.rootPath = zed_like_root
      params.workspaceFolders = {
        {
          uri = root_uri,
          name = 'tinymist-issue-2489',
        },
      }
    end,
    root_dir = function()
      return book_root
    end,
  },
}

describe('Lockfile path resolution', function()
  assert.is.empty(vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true })

  it('does not look for tinymist.lock under the opened Typst file', function()
    local opened_path = zed_like_root
    local wrong_lock_path = vim.fs.joinpath(opened_path, 'tinymist.lock')

    vim.cmd.edit(opened_path)
    local client_attached = vim.wait(15000, function()
      return #vim.lsp.get_clients { bufnr = 0, name = 'tinymist', _uninitialized = true } > 0
    end)
    assert.message('tinymist LSP client did not attach').True(client_attached)

    local resolved_task = vim.wait(15000, function()
      local log = read_log()
      return log:find('resolved task with state:', 1, true) ~= nil
        and log:find(opened_path, 1, true) ~= nil
    end)
    assert.message(('tinymist did not resolve the opened file through lockDatabase. log:\n%s'):format(read_log())).True(resolved_task)

    local log = read_log()
    assert.is_nil(
      log:find(wrong_lock_path, 1, true),
      ('tinymist tried to load a lockfile below a Typst file path: %s\nlog:\n%s'):format(wrong_lock_path, log)
    )
  end)
end)
