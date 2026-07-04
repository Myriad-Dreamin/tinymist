local Buffer = require 'std.nvim.buffer'

---A Neovim window.
---@class Window
---@field id integer The window ID
local Window = {}
Window.__index = Window

---Bind to a Neovim window.
---@param id? integer Window ID, defaulting to the current window
---@return Window
function Window:from_id(id)
  return setmetatable({ id = id or vim.api.nvim_get_current_win() }, self)
end

---Bind to the current window.
function Window:current()
  return self:from_id(vim.api.nvim_get_current_win())
end

---Return the buffer shown in the window.
---@return Buffer buffer
function Window:buffer()
  return Buffer:from_bufnr(self:bufnr())
end

---Return the buffer number of the window.
---@return integer bufnr
function Window:bufnr()
  return vim.api.nvim_win_get_buf(self.id)
end

---Return the tab the window is on.
---@return Tab tab
function Window:tab()
  return require('std.nvim.tab'):from_id(vim.api.nvim_win_get_tabpage(self.id))
end

---@class SplitOpts
---@field buffer? Buffer the buffer to open in the new window (default current)
---@field enter? boolean whether to enter the window (default false)
---@field direction? 'left'|'right'|'above'|'below' the direction to split

---Split a new window relative to this window.
---@param opts? SplitOpts
---@return Window
function Window:split(opts)
  opts = vim.tbl_extend('keep', opts or {}, { enter = false })
  local direction = opts.direction or vim.o.splitright and 'right' or 'left'

  local config = { win = self.id, split = direction }
  local bufnr = opts.buffer and opts.buffer.bufnr or 0
  local id = vim.api.nvim_open_win(bufnr, opts.enter, config)
  return Window:from_id(id)
end

---Return the window's current cursor position.
---
---(1, 0)-indexed, like `nvim_win_get_cursor()`.
---@return { [1]: integer, [2]: integer } pos
function Window:cursor()
  return vim.api.nvim_win_get_cursor(self.id)
end

---Set the window's current cursor position.
---
---(1, 0)-indexed, like `nvim_win_set_cursor()`.
---@param pos { [1]: integer, [2]: integer } the new cursor position
function Window:set_cursor(pos)
  vim.api.nvim_win_set_cursor(self.id, pos)
end

---Is this the current window?
function Window:is_current()
  return vim.api.nvim_get_current_win() == self.id
end

---Make this window be the current one.
function Window:make_current()
  vim.api.nvim_set_current_win(self.id)
end

---Return the window's height.
---@return integer height
function Window:height()
  return vim.api.nvim_win_get_height(self.id)
end

---Set the window's height.
---@param height integer
function Window:set_height(height)
  vim.api.nvim_win_set_height(self.id, height)
end

---Return the window's width.
---@return integer width
function Window:width()
  return vim.api.nvim_win_get_width(self.id)
end

---Set the window's width.
---@param width integer
function Window:set_width(width)
  vim.api.nvim_win_set_width(self.id, width)
end

---Run a function with the window as temporary current window.
function Window:call(fn)
  vim.api.nvim_win_call(self.id, fn)
end

---Check if the window is valid.
---
---Do you wonder exactly what that corresponds to?
---Well keep wondering because the Neovim docstring doesn't elaborate.
---
---But for one, closed windows return `false`.
---@return boolean valid
function Window:is_valid()
  return vim.api.nvim_win_is_valid(self.id)
end

---Close the window.
function Window:close()
  vim.api.nvim_win_close(self.id, false)
end

-- Beyond the Neovim API...

---Move the cursor to a given position.
---
---(1, 0)-indexed, like `nvim_win_set_cursor()`.
---
---Fires `CursorMoved` if (and only if) the cursor is now at a new position.
---@param pos { [1]: integer, [2]: integer } the new cursor position
function Window:move_cursor(pos)
  local start = self:cursor()
  self:set_cursor(pos)
  if vim.deep_equal(self:cursor(), start) then
    return
  end
  vim.api.nvim_exec_autocmds('CursorMoved', { buffer = self:bufnr() })
end

---Get the contents of the remainder of the line with the window's cursor.
---@return string contents text from cursor position to the end of line
function Window:rest_of_cursor_line()
  local row, col = unpack(self:cursor())
  local line = vim.api.nvim_buf_get_lines(self:bufnr(), row - 1, row, true)[1]
  return line:sub(col + 1)
end

return Window
