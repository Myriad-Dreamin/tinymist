local tinymist = {}

---Setup function to be run in your init.lua.
---@param opts lean.Config Configuration options
function tinymist.setup(opts)
    opts = opts or {}

    opts.lsp = opts.lsp or {}
    if opts.lsp.enable ~= false then
        require('tinymist.lsp').enable(opts.lsp)
    end

    vim.g.tinymist_config = opts
end

return tinymist
