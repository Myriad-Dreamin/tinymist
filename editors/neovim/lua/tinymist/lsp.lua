local lsp = {}

local subscribers = {}

function lsp.subscribeDevEvent(callback)
    if type(callback) ~= 'function' then
        error('callback must be a function')
    end
    table.insert(subscribers, callback)
end


---@param opts LeanClientConfig
function lsp.enable(opts)
    opts.capabilities = opts.capabilities or vim.lsp.protocol.make_client_capabilities()
    opts = vim.tbl_deep_extend('keep', opts, {
        handlers = {
            ['tinymist/devEvent'] = function(_, result, ctx)
                -- unregister if callback return true
                for i = #subscribers, 1, -1 do
                    local callback = subscribers[i]
                    if callback(result) then
                        table.remove(subscribers, i)
                    end
                end
            end,
        },

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
