# Tinymist Helix Support for Typst

Run and configure tinymist in helix for Typst.

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

## Setup Server

Update `.config/helix/languages.toml` to use tinymist.

```toml
[language-server.tinymist]
command = "tinymist"

[[language]]
name = "typst"
language-servers = ["tinymist"]
```

## Extra Settings

To configure LSP server, you can edit the `language-server.tinymist` section. For example, if you want to export PDF on typing and output files in `$root_dir/target` directory:

```toml
[language-server.tinymist]
command = "tinymist"
config = { exportPdf = "onType", outputPath = "$root/target/$dir/$name" }
```

See [Tinymist Server Configuration](../neovim/Configuration.md) for references.
