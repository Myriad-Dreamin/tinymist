#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: "Tinymist Neovim Extension")

Run and configure `tinymist` in Neovim with support for all major distros and package managers.

== Feature Integration
- *Language service* (completion, definitions, etc.)
- *Code Formatting*
- *Live Web Preview* with #link("https://github.com/chomosuke/typst-preview.nvim")[typst-preview.]

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
- With `lspconfig`:
  ```lua
  require("lspconfig")["tinymist"].setup {
      settings = {
          formatterMode = "typstyle",
          exportPdf = "onType",
          semanticTokens = "disable"
      }
  }
  ```

- Or with `Coc.nvim`:

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
- Or finally with the builtin lsp protocol:

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

Either `typstyle` or `typstfmt`. Both are now included in `tinymist`, you can select the one you prefer with:

```lua
formatterMode = "typstyle"
```

== Live Preview
<live-preview>

Live preview can be achieved with either a web preview or a pdf reader that supports automatic reloading (#link("https://pwmt.org/projects/zathura/")[zathura] is good).

*Web Preview*

```lua
-- lazy.nvim
{
  'chomosuke/typst-preview.nvim',
  lazy = false, -- or ft = 'typst'
  version = '1.*',
  opts = {}, -- lazy.nvim will implicitly calls `setup {}`
}
```
See #link("https://github.com/chomosuke/typst-preview.nvim")[typst-preview] for more installation and configuration options.

*Pdf Preview*

This preview method is slower because of compilation delays, and additional delays in the pdf reader refreshing.

It is often useful to have a command that opens the current file in the reader.

```lua
vim.api.nvim_create_user_command("OpenPdf", function()
  local filepath = vim.api.nvim_buf_get_name(0)
  if filepath:match("%.typ$") then
    os.execute("open " .. vim.fn.shellescape(filepath:gsub("%.typ$", ".pdf")))
    -- replace open with your preferred pdf viewer
    -- os.execute("zathura " .. vim.fn.shellescape(filepath:gsub("%.typ$", ".pdf")))
  end
end, {})

```
Make sure to change `exportPdf` to "onType" or "onSave".

=== Working with Multiple-Files Projects
<working-with-multiple-file-projects>

Tinymist cannot know the main file of a multiple-files project if you don't tell it explicitly. This causes the well-known label error when editing the `/sub.typ` file in a project like that:

```typ
// in file: /sub.typ
// Error: unknown label 'label-in-main'
@label-in-main
// in file: /main.typ
#include "sub.typ"
= Heading <label-in-main>
```

The solution is a bit internal, which should get further improvement, but you can pin a main file by command.

```lua
-- pin the main file
vim.lsp.buf.execute_command({ command = 'tinymist.pinMain', arguments = { vim.api.nvim_buf_get_name(0) } })
-- unpin the main file
vim.lsp.buf.execute_command({ command = 'tinymist.pinMain', arguments = { nil } })
```

It also doesn't remember the pinned main file across sessions, so you may need to run the command again after restarting neovim.

This could be improved in the future.

== Troubleshooting
<troubleshooting>

Generally you can find in depth information via the `:mes` command. `:checkhealth` and `LspInfo` can also provide valuable information. Tinymist also creates a debug log that is usually at `~/.local/state/nvim/lsp.log`. Reporting bugs is welcome.

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
