#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: "Tinymist Neovim Extension")

Run and configure tinymist in neovim for Typst.

== Features
<features>
See #link("https://github.com/Myriad-Dreamin/tinymist#features")[Tinymist Features] for a list of features.

#include "common-finding-executable.typ"

- (Recommended) Stable versions available with #link("https://github.com/williamboman/mason.nvim")[mason.nvim];.

  ```lua
  {
    "williamboman/mason.nvim",
    opts = {
      ensure_installed = {
        "tinymist",
      },
    },
  }
  ```

== Setup

This section shows you a minimal way to setup tinymist in #link("https://www.lazyvim.org/")[LazyVim]. For other distros, we also bring some tips for you.

// todo: heading link support
Please see #md-alter(link(<neovim-extra-settings>)[Extra Settings], () => link("#extra-settings")[Extra Settings]) for more configuration.

=== Setup for #link("https://www.lazyvim.org/")[LazyVim]

Copy or merge the two files to corresponding paths into `~/.config/nvim/`.

- #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/config/autocmds.lua")[Autocmds file] will help associate `.typ` file extension to `typst` filetype.
- #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/plugins/tinymist.lua")[Plugin file] will help start tinymist for buffers with the `typst` filetype.

Then, check and restart Neovim.

=== Setup for #link("https://github.com/AstroNvim")[AstroVim]

AstroNvim now uses tinymist by default. Please check the #link("https://github.com/AstroNvim/astrocommunity/tree/main/lua/astrocommunity/pack/typst")[setup script].

=== Setup for #link("https://github.com/neoclide/coc.nvim")[coc.nvim]

You can edit the `coc-settings.json` by executing `:CocConfig`:

```json
{
  "languageserver": {
    "tinymist": {
      "command": "tinymist",
      "filetypes": ["typst"],
      "settings": { ... }
    }
  }
}
```

== Tips
<tips>
=== Working with Multiple-File Projects
<working-with-multiple-file-projects>
The solution is a bit internal, which should get further improvement, but you can pin a main file by command.

```lua
-- pin the main file
vim.lsp.buf.execute_command({ command = 'tinymist.pinMain', arguments = { vim.api.nvim_buf_get_name(0) } })
-- unpin the main file
vim.lsp.buf.execute_command({ command = 'tinymist.pinMain', arguments = { nil } })
```

There is also a plan to support multiple-file project by workspace configuration, but I don’t know whether it is neovim’s way, so it needs further discussion.

#note-box[
  `tinymist.pinMain` is a stateful command, and tinymist doesn't remember it between sessions (closing and opening the editor).
]

== Troubleshooting
<troubleshooting>
=== tinymist does not start on creating/opening files
<tinymist-does-not-start-on-creatingopening-files>
First, please check that tinymist can start with manual file association.

```
:set filetype=typst
```

If tinymist starts, that means you have not made correct file association. If not, there should be some errors in your lspconfig.

Please associate `.typ` file extension to `typst` filetype to start tinymist on creating/opening file events.

```shell
autocmd BufNewFile,BufRead *.typ setfiletype typst
```

== Extra Settings
<neovim-extra-settings>
=== Configuring Language Server
<neovim-configuring-language-server>
To configure language server, you can edit the `opts.servers.tinymist.settings`. For example, if you want to export PDF on typing and output files in `$root_dir/target` directory:

```lua
return {
  -- add tinymist to lspconfig
  {
    "neovim/nvim-lspconfig",
    opts = {
      servers = {
        tinymist = {
          settings = {
            exportPdf = "onType",
            outputPath = "$root/target/$dir/$name",
          }
        },
      },
    },
  },
}
```

See #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/Configuration.md")[Tinymist Server Configuration] for references.

=== Configuring Folding Range for Neovim Client
<configuring-folding-range-for-neovim-client>
Enable LSP-based folding range with `kevinhwang91/nvim-ufo`:

```lua
return {
  { -- configure language servers
    "neovim/nvim-lspconfig",
    dependencies = "kevinhwang91/nvim-ufo", -- enable LSP-based folds
  },
}
```

You can copy or merge #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/plugins/lsp-folding.lua")[lsp-folding.lua] to corresponding paths into `~/.config/nvim/` and restart Neovim.

== Contributing
<contributing>
You can submit issues or make PRs to #link("https://github.com/Myriad-Dreamin/tinymist")[GitHub];.
