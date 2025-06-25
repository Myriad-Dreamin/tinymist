local Tab = require 'std.nvim.tab'
local Window = require 'std.nvim.window'
local assert = require 'luassert'
local text = require 'std.text'
local fixtures = require 'spec.fixtures'

-- local progress = require 'lean.progress'
-- local util = require 'lean._util'

---@class LeanClientCapabilities : lsp.ClientCapabilities
---@field silentDiagnosticSupport? boolean Whether the client supports `DiagnosticWith.isSilent = true`.

---@class LeanClientConfig : vim.lsp.ClientConfig
---@field lean? LeanClientCapabilities

---Find the `vim.lsp.Client` attached to the given buffer.
---@param bufnr? number
---@return vim.lsp.Client?
function client_for(bufnr)
  local clients = vim.lsp.get_clients { name = 'tinymist', bufnr = bufnr or 0 }
  return clients[1]
end

local helpers = { _clean_buffer_counter = 1 }

---Feed some keystrokes into the current buffer, replacing termcodes.
function helpers.feed(contents, feed_opts)
  feed_opts = feed_opts or 'mtx'
  local to_feed = vim.api.nvim_replace_termcodes(contents, true, false, true)
  vim.api.nvim_feedkeys(to_feed, feed_opts, true)
end

---Insert some text into the current buffer.
function helpers.insert(contents, feed_opts)
  feed_opts = feed_opts or 'x'
  helpers.feed('i' .. contents, feed_opts)
end

function helpers.all_lean_extmarks(buffer, start, end_)
  local extmarks = {}
  for namespace, ns_id in pairs(vim.api.nvim_get_namespaces()) do
    if namespace:match '^lean.' then
      vim.list_extend(
        extmarks,
        vim.api.nvim_buf_get_extmarks(buffer, ns_id, start, end_, { details = true })
      )
    end
  end
  return extmarks
end

---Move the cursor to a new location.
---
---Ideally this function wouldn't exist, and one would call `set_cursor`
---directly, but it does not fire `CursorMoved` autocmds.
---This function exists therefore to make tests which have slightly
---less implementation details in them (the manual firing of that autocmd).
---
---@param opts MoveCursorOpts
function helpers.move_cursor(opts)
  local window = opts.window or Window:current()
  window:move_cursor(opts.to)
end

---Search forward in the buffer for the given text.
---
---Fires `CursorMoved` if the cursor moves and fails if it does not.
function helpers.search(string)
  local cursor = vim.api.nvim_win_get_cursor(0)
  vim.fn.search(string)
  assert.are_not.same(cursor, vim.api.nvim_win_get_cursor(0), 'Cursor did not move!')
  vim.api.nvim_exec_autocmds('CursorMoved', {})
end

function helpers.wait_for_ready_lsp()
  local succeeded, _ = vim.wait(15000, function()
    local client = client_for(0)
    return client and client.initialized or false
  end)
  assert.message('LSP server was never ready.').True(succeeded)
end

---Wait until a window that isn't one of the known ones shows up.
---@param known table
function helpers.wait_for_new_window(known)
  local ids = vim
      .iter(known)
      :map(function(window)
        return window.id
      end)
      :totable()

  local new_window
  local succeeded = vim.wait(1000, function()
    new_window = vim.iter(vim.api.nvim_tabpage_list_wins(0)):find(function(window)
      return not vim.tbl_contains(ids, window)
    end)
    return new_window
  end)
  assert.message('Never found a new window').is_true(succeeded)
  return Window:from_id(new_window)
end

-- Even though we can delete a buffer, so should be able to reuse names,
-- we do this to ensure if a test fails, future ones still get new "files".
local function set_unique_name_so_we_always_have_a_separate_fake_file(bufnr)
  local counter = helpers._clean_buffer_counter
  helpers._clean_buffer_counter = helpers._clean_buffer_counter + 1
  local unique_name = fixtures.project.child(('unittest-%d.typ'):format(counter))
  vim.api.nvim_buf_set_name(bufnr, unique_name)
end

---Create a clean Lean buffer with the given contents.
---
---Waits for the LSP to be ready before proceeding with a given callback.
--
---Yes c(lean) may be a double entendre, and no I don't feel bad.
function helpers.clean_buffer(contents, callback)
  local lines

  -- Support a 1-arg version where we assume the contents is an empty buffer.
  if callback == nil then
    callback = contents
    lines = {}
  else
    lines = vim.split(text.dedent(contents:gsub('^\n', '')):gsub('\n$', ''), '\n')
  end

  return function()
    local bufnr = vim.api.nvim_create_buf(false, false)
    set_unique_name_so_we_always_have_a_separate_fake_file(bufnr)

    vim.bo[bufnr].swapfile = false
    vim.bo[bufnr].filetype = 'typst'
    vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)

    -- isn't it fun how fragile the order of the below lines is, and how
    -- BufWinEnter seems automatically called by `nvim_set_current_buf`, but
    -- `BufEnter` seems not automatically called by `nvim_buf_call` so we
    -- manually trigger it?
    vim.api.nvim_set_current_buf(bufnr)
    vim.api.nvim_exec_autocmds('BufEnter', { buffer = bufnr })
    vim.api.nvim_buf_call(bufnr, callback)

    -- FIXME: Deleting buffers seems good for keeping our tests clean, but is
    --        broken on 0.11 with impossible to diagnose invalid buffer errors.
    -- vim.api.nvim_buf_delete(bufnr, { force = true })
  end
end

---Wait a few seconds for line diagnostics, erroring if none arrive.
function helpers.wait_for_line_diagnostics()
  local params = vim.lsp.util.make_position_params(0, 'utf-16')
  local succeeded, _ = vim.wait(15000, function()
    if progress.at(params) == progress.Kind.processing then
      return false
    end
    local diagnostics = util.lean_lsp_diagnostics { lnum = params.position.line }
    return #diagnostics > 0
  end)
  assert.message('Waited for line diagnostics but none came.').True(succeeded)
end

function helpers.wait_for_filetype()
  local result, _ = vim.wait(15000, function()
    return vim.bo.filetype == 'typst'
  end)
  assert.message('filetype was never set').is_truthy(result)
end

---Assert about the current word.
local function has_current_word(_, arguments)
  assert.is.equal(arguments[1], vim.fn.expand '<cword>')
  return true
end
assert:register('assertion', 'current_word', has_current_word)

---Assert about the current line.
local function has_current_line(_, arguments)
  assert.is.equal(arguments[1], vim.api.nvim_get_current_line())
  return true
end
assert:register('assertion', 'current_line', has_current_line)

---Assert about the current cursor location.
local function has_current_cursor(_, arguments)
  local window = arguments[1].window
  if not window then
    window = Window:current()
  elseif type(window) == 'number' then
    window = Window:from_id(window)
  end
  local got = window:cursor()

  local column = arguments[1][2] or arguments[1].column or 0
  local expected = { arguments[1][1] or got[1], column }

  assert.are.same(expected, got)
  return true
end
assert:register('assertion', 'current_cursor', has_current_cursor)

---Assert about the current tabpage.
local function has_current_tabpage(_, arguments)
  assert.are.same(Tab:current(), arguments[1])
  return true
end
assert:register('assertion', 'current_tabpage', has_current_tabpage)

---Assert about the current window.
local function has_current_window(_, arguments)
  assert.are.same(Window:current(), arguments[1])
  return true
end
assert:register('assertion', 'current_window', has_current_window)

local function _expected(arguments)
  local expected = arguments[1][1] or arguments[1]
  -- Handle cases where we're indeed checking for a real trailing newline.
  local dedented = text.dedent(expected)
  if dedented ~= expected then
    expected = dedented:gsub('\n$', '')
  end
  return expected
end

---Assert about the entire buffer contents.
local function has_buf_contents(_, arguments)
  local bufnr = arguments[1].bufnr or 0
  local got = table.concat(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false), '\n')
  assert.is.equal(_expected(arguments), got)
  return true
end
assert:register('assertion', 'contents', has_buf_contents)

assert:register('assertion', 'diff_contents', has_diff_contents)

local function has_highlighted_text(_, arguments)
  local inspected = vim.inspect_pos(0)
  local highlight = vim.iter(inspected.extmarks):find(function(mark)
    return mark.opts.hl_group == 'widgetElementHighlight'
  end)

  assert.is_not_nil(highlight, ('No highlighted text found in %s'):format(vim.inspect(inspected)))

  local got = vim.api.nvim_buf_get_text(
    0,
    highlight.row,
    highlight.col,
    highlight.end_row,
    highlight.end_col,
    {}
  )[1]
  assert.are.same(arguments[1], got)
  return true
end

assert:register('assertion', 'highlighted_text', has_highlighted_text)

local function has_all(_, arguments)
  local contents = arguments[1]

  if type(contents) == 'table' then
    contents = table.concat(contents, '\n')
  end
  local expected = arguments[2]
  for _, string in pairs(expected) do
    assert.has_match(string, contents, nil, true)
  end
  return true
end

assert:register('assertion', 'has_all', has_all)

---Assert a tabpage has the given windows open in it.
local function has_open_windows(_, arguments)
  local expected
  if arguments.n == 1 and type(arguments[1]) == 'table' then
    expected = arguments[1]
    expected.n = #expected
  else
    expected = arguments
  end
  local got = vim.api.nvim_tabpage_list_wins(0)
  got.n = #got
  table.sort(expected)
  table.sort(got)
  assert.are.same(expected, got)
  return true
end

assert:register('assertion', 'windows', has_open_windows)

return helpers
