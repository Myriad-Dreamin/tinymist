local lsp = {}

---@param opts LeanClientConfig
function lsp.enable(opts)
    opts.capabilities = opts.capabilities or vim.lsp.protocol.make_client_capabilities()
    opts = vim.tbl_deep_extend('keep', opts, {
        init_options = {
            -- editDelay = 10, -- see lean#289
            hasWidgets = true,
        },
        on_init = function(_, response)
        end,
    })
    require('lspconfig').tinymist.setup(opts)
end

return lsp
