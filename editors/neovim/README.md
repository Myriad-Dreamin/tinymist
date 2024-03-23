# Tinymist Neovim Support for Typst

Run and configure tinymist in neovim for Typst.

## Features

See [Tinymist Features](https://github.com/Myriad-Dreamin/tinymist#features) for a list of features.

## Finding Executable

To enable LSP, you must install `tinymist`. You can find `tinymist` on:

- Night versions available at [GitHub Actions](https://github.com/Myriad-Dreamin/tinymist/actions).
- Stable versions available at [GitHub Releases](https://github.com/Myriad-Dreamin/tinymist/releases).

You can also compile and install **latest** `tinymist` by [Cargo](https://www.rust-lang.org/tools/install).

```bash
cargo install --git https://github.com/Myriad-Dreamin/tinymist --locked
```

## Minimal Setup

This section shows you a minimal way to setup tinymist in Neovim.

1. Copy or merge the [Autocmds file](./config/autocmds.lua) and [Plugin file](./plugins/tinymist.lua) to corresponding paths into `~/.config/nvim/`.

2. Check and restart Neovim.

Please see [Extra Settings](#extra-settings) for more configuration.

## Troubleshooting

### tinymist does not start on creating/opening files

First, please check that tinymist can start with manual file association.

```
:setfiletype typst
```

If tinymist starts, that means you have not made correct file association. If not, there should be some errors in your lspconfig.

Please associate `.typ` file extension to `typst` filetype to start tinymist on creating/opening file events.

```
autocmd BufNewFile,BufRead *.typ setfiletype typst
```

## Extra settings

### Configuring LSP Server

To configure LSP server, you can edit the `opts.servers.tinymist.settings`. For example, if you want to export PDF onType and output files in `$root_dir/target` directory:

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

See [Tinymist Server Configuration](https://github.com/Myriad-Dreamin/tinymist/blob/main/Configuration.md) for references.

### Configuring Folding Range for Neovim Client

Enable LSP-based folding range with `kevinhwang91/nvim-ufo`:

```lua
return {
  { -- configure LSP servers
    "neovim/nvim-lspconfig",
    dependencies = "kevinhwang91/nvim-ufo", -- enable LSP-based folds
  },
}
```

You can copy or merge [lsp-folding.lua](./config/lsp-folding.lua) to corresponding paths into `~/.config/nvim/` and restart Neovim.

## Contributing

You can submit issues or make PRs to [GitHub](https://github.com/Myriad-Dreamin/tinymist).
