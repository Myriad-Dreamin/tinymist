local tinymist = require("others.tinymist")[2]

-- set binary path
tinymist.opts.servers.tinymist.cmd = { "tinymist" }


return {
    { "mason-org/mason.nvim",           version = "^1.0.0" },
    { "mason-org/mason-lspconfig.nvim", version = "^1.0.0" },

    -- spread tinymist
    tinymist,

    -- change some telescope options and a keymap to browse plugin files
    {
        "nvim-telescope/telescope.nvim",
        keys = {
            -- add a keymap to browse plugin files
            -- stylua: ignore
            {
                "<leader>fp",
                function() require("telescope.builtin").find_files({ cwd = require("lazy.core.config").options.root }) end,
                desc = "Find Plugin File",
            },
        },
        -- change some options
        opts = {
            defaults = {
                layout_strategy = "horizontal",
                layout_config = { prompt_position = "top" },
                sorting_strategy = "ascending",
                winblend = 0,
            },
        },
    },

    -- add more treesitter parsers
    {
        "nvim-treesitter/nvim-treesitter",
        opts = {
            ensure_installed = {
                "bash",
                "json",
                "lua",
                "markdown",
                "markdown_inline",
                "vim",
            },
        },
    },

    -- add any tools you want to have installed below
    {
        "williamboman/mason.nvim",
        opts = {
            ensure_installed = {
                "stylua",
            },
        },
    },
}
