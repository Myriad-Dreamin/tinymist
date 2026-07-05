---A Neovim buffer.
---@class Buffer
---@field bufnr integer The buffer number
local Buffer = {}
Buffer.__index = Buffer

---Bind to a Neovim buffer.
---@param bufnr? integer buffer number, defaulting to the current one
---@return Buffer
function Buffer:from_bufnr(bufnr)
  return setmetatable({ bufnr = bufnr or vim.api.nvim_get_current_buf() }, self)
end

---Bind to the current buffer.
function Buffer:current()
  return self:from_bufnr(vim.api.nvim_get_current_buf())
end

---The buffer's name.
---@return string name
function Buffer:name()
  return vim.api.nvim_buf_get_name(self.bufnr)
end

return Buffer
