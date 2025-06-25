vim.o.display = 'lastline' -- Avoid neovim/neovim#11362
vim.o.directory = ''
vim.o.shada = ''

local this_dir = vim.fs.dirname(debug.getinfo(1, 'S').source:sub(2))
local lean_nvim_dir = vim.fs.dirname(this_dir)
local packpath = vim.fs.joinpath('/home/runner/packpath/*')
vim.opt.runtimepath:append(packpath)

-- Doing this unconditionally seems to fail a random indent test?!?!
-- Inanis/Plenary will automatically set rtp+. (which seems wrong, but OK)
-- so really we need this just for `just nvim`...
if #vim.api.nvim_list_uis() ~= 0 then
    vim.opt.runtimepath:append(lean_nvim_dir)
end

vim.cmd [[
  runtime! plugin/lspconfig.vim
  runtime! plugin/matchit.vim
  runtime! plugin/plenary.vim
  runtime! plugin/switch.vim
  runtime! plugin/tcomment.vim
]]

-- plenary forks subprocesses, so enable coverage here when appropriate
if vim.env.LEAN_NVIM_COVERAGE then
    local luapath = vim.fs.joinpath(lean_nvim_dir, 'luapath')
    package.path = package.path
        .. ';'
        .. luapath
        .. '/share/lua/5.1/?.lua;'
        .. luapath
        .. '/share/lua/5.1/?/init.lua;;'
    package.cpath = package.cpath .. ';' .. luapath .. '/lib/lua/5.1/?.so;'
    require 'luacov'
end

-- if vim.env.LEAN_NVIM_DEBUG then
--     local port = 8088
--     if vim.env.LEAN_NVIM_DEBUG ~= '' and vim.env.LEAN_NVIM_DEBUG ~= '1' then
--         port = tonumber(vim.env.LEAN_NVIM_DEBUG)
--     end
--     require('osv').launch { host = '127.0.0.1', port = port }
--     vim.wait(5000)
-- end
