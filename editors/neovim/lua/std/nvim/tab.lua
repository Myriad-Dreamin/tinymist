local Window = require 'std.nvim.window'

---A Neovim tab.
---@class Tab
---@field id integer The tab number
local Tab = {}
Tab.__index = Tab

---Bind to a Neovim tab.
---@param id? integer tab ID, defaulting to the current one
---@return Tab
function Tab:from_id(id)
  return setmetatable({ id = id or vim.api.nvim_get_current_tabpage() }, self)
end

---Bind to the current tab.
function Tab:current()
  return self:from_id(vim.api.nvim_get_current_tabpage())
end

---All current tabs.
---@return Tab[] tabs
function Tab:all()
  return vim
    .iter(vim.api.nvim_list_tabpages())
    :map(function(tab_id)
      return self:from_id(tab_id)
    end)
    :totable()
end

---Open a new tab page.
function Tab:new()
  -- See https://github.com/neovim/neovim/pull/27223
  vim.cmd.tabnew()
  return self:current()
end

---Close the tab page.
function Tab:close()
  vim.cmd.tabclose(vim.api.nvim_tabpage_get_number(self.id))
end

---Return the windows present in the tab.
---@return Window[] windows
function Tab:windows()
  return vim
    .iter(vim.api.nvim_tabpage_list_wins(self.id))
    :map(function(win_id)
      return Window:from_id(win_id)
    end)
    :totable()
end

return Tab
