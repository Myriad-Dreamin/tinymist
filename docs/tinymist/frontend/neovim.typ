#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: "Tinymist Neovim Extension")

Run and configure `tinymist` in Neovim with support for all major distros and package managers.

== Feature Integration
- *LSP* (Completion, Definitions, etc)
- *Folding Ranges* supported with #link("https://github.com/kevinhwang91/nvim-ufo")[ufo].
- *Code Formatting* with #link("https://github.com/Enter-tainer/typstyle/")[typestyle] or #link("https://github.com/astrale-sharp/typstfmt")[typstfmt] (depreciated)
- *Live Web Preview* with #link("https://github.com/chomosuke/typst-preview.nvim")[typst-preview]

#note-box[
  Work for full parity for all `tinymist` features is underway. This will include: exporting to different file types, template preview, and multifile support. Neovim integration is behind VS Code currently but should be caught up in the near future.
]

== Installation
- (Recommended) #link("https://github.com/williamboman/mason.nvim")[mason.nvim];.
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
- Or manually:

  #include "common-finding-executable.typ"

== Configuration
- With `lspconfig`
  ```lua
  require("lspconfig")["tinymist"].setup {
      settings = {
          tinymist = {
              settings = {
                  formatterMode = "typstyle",
                  exportPdf = "onType",
                  semanticTokens = "disable"
                  -- ...
              },
          },
      }
  }
  ```

- Or with `Coc.nvim`

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
- Or finally with the builtin lsp protocol

  ```lua
  vim.lsp.config['tinymist'] = {
      cmd = {'tinymist'},
      filetypes = {'typst'}
      settings = {
          -- ...
      }
  }
  ```

For a full list of available settings see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/Configuration.md")[Tinymist Server Configuration].

== Formatting

Either `typststyle` or `typstfmt`. Both are now included in `tinymist`, you can select the one you prefer with:

```lua
formatterMode = "typstyle"
```

== Preview

There is work in progress to integrate #link("https://github.com/chomosuke/typst-preview.nvim")[typst-preview.nvim] directly into `tinymist`. In the meantime you can find can find installation and configuration instructions on that repo. This should be a shortterm solution.

== Troubleshooting
<troubleshooting>

Generally you can find in depth information via the `:mes` command, `:checkhealth` and `LspInfo` can also provide valuable information. Tinymist also creates a debug log that is usually at `~/.local/state/nvim/lsp.log`. Reporting bugs is welcome.

=== tinymist not starting when creating/opening files
<tinymist-does-not-start-on-creatingopening-files>

This is most commonly due to nvim not recognizing the `.typ` file extension as a `typst` source file. In most cases is can be resolved with:

```typ
:set filetype=typst
```

In older versions of neovim an autocommand may be necessary.

```vim
autocmd BufNewFile,BufRead *.typ setfiletype typst
```

== Contributing
<contributing>
You can submit issues or make PRs to #link("https://github.com/Myriad-Dreamin/tinymist")[GitHub];.
