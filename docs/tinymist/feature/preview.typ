#import "mod.typ": *

#show: book-page.with(title: "Tinymist Preview Feature")

Two ways of previewing a Typst document are provided:
- PDF Preview: let lsp export your PDF on typed, and open related PDF by your favorite PDF viewer.
- Web (SVG) Preview: use builtin preview feature.

Whenever you can get a web preview feature, it is recommended since it is much faster than PDF preview and provides bidirectional navigation feature, allowing jumping between the source code and the preview by clicking or lsp commands.

== PDF Preview

For non-vscode clients, neovim client as an example. One who uses `nvim-lspconfig` can place their configuration in the `servers.tinymist.settings` section. If you want to export PDF on typing and output files in `$root_dir/target` directory, please configure it like that:

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

#pro-tip[
  === VSCode:

  The corresponding configuration should be placed in the `settings.json` file. For example:

  ```json
  {
    "tinymist.exportPdf": "onType",
    "tinymist.outputPath": "$root/target/$dir/$name"
  }
  ```
]

Also see:

- #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/vscode/Configuration.md")[VS Cod(e,ium): Tinymist Server Configuration]
- #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/neovim/Configuration.md")[Neovim: Tinymist Server Configuration]

== Builtin Preview Feature

See #link("https://enter-tainer.github.io/typst-preview/arch.html")[Typst-Preview Developer Guide].

=== CLI Integration

```bash
typst-preview /abs-path/to/main.typ --partial-rendering
```

is equivalent to

```bash
tinymist preview /abs-path/to/main.typ --partial-rendering
```

=== Editor Integration

#pro-tip[
  === VSCode:

  The preview feature is also integrated into the language server. You can use the preview feature like when you were using it in `mgt19937.typst-preview` extension.
]

#pro-tip[
  === Neovim:

  You may seek #link("https://github.com/chomosuke/typst-preview.nvim")[typst-preview.nvim] for the preview feature.
]

#pro-tip[
  === Emacs:

  You may seek #link("https://github.com/havarddj/typst-preview.el")[typst-preview.el] for the preview feature.
]

=== `sys.inputs`

If the document is compiled by lsp, you can use `sys.inputs` to get the preview arguments:

```typ
#let preview-args = json.decode(sys.inputs.at("x-preview", default: "{}"))
```

There is a `version` field in the `preview-args` object, which will increase when the scheme of the preview arguments is changed.

```typ
#let version = preview-args.at("version", default: 0)
#if version <= 1 {
  assert(preview-args.at("theme", default: "light") in ("light", "dark"))
}
```

==== Theme-aware template

The only two abstracted theme kinds are supported: `light` and `dark`. You can use the following code to get the theme:

```typ
#let preview-theme = preview-args.at("theme", default: "light")
```
