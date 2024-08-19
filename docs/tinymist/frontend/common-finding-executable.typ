
== Finding Executable
<finding-executable>
To enable LSP, you must install `tinymist`. You can find `tinymist` by:

- Night versions available at #link("https://github.com/Myriad-Dreamin/tinymist/actions")[GitHub Actions].

- Stable versions available at #link("https://github.com/Myriad-Dreamin/tinymist/releases")[GitHub Releases]. \
  If you are using the latest version of
  #link("https://codeberg.org/meow_king/typst-ts-mode")[typst-ts-mode], then
  you can use command `typst-ts-lsp-download-binary` to download the latest
  stable binary of `tinymist` at `typst-ts-lsp-download-path`.

- Build from source by cargo.
  You can also compile and install *latest* `tinymist` by #link("https://www.rust-lang.org/tools/install")[Cargo];.

  ```bash
  cargo install --git https://github.com/Myriad-Dreamin/tinymist --locked tinymist
  ```
