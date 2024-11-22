#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: "Tinymist Emacs Extension")

Run and configure tinymist in Emacs for Typst.

== Features
<features>
See #link("https://github.com/Myriad-Dreamin/tinymist#features")[Tinymist Features] for a list of features.

#include "common-finding-executable.typ"

== Setup Server
<setup-server>

```el
(with-eval-after-load 'eglot
  (with-eval-after-load 'typst-ts-mode
    (add-to-list 'eglot-server-programs
                 `((typst-ts-mode) .
                   ,(eglot-alternatives `(,typst-ts-lsp-download-path
                                          "tinymist"
                                          "typst-lsp"))))))
```

Above code adds `tinymist` downloaded by `typst-ts-lsp-download-binary`, `tinymist` in
your PATH and `typst-lsp` in your `PATH` to the `typst-ts-mode` entry of `eglot-server-programs`.


== Extra Settings
<extra-settings>
=== Configuring Language Server
<configuring-language-server>

You can either use `eglot-workspace-configuration` or specifying launch
arguments for `tinymist`.

==== eglot-workspace-configuration
<eglot-workspace-configuration>

For example, if you want to export PDF on save:

```el
  (setq-default eglot-workspace-configuration
                '(:tinymist (:exportPdf "onSave")))
```

You can also have configuration per directory. Be sure to look at the
documentation of `eglot-workspace-configuration` by #link("https://www.gnu.org/software/emacs/manual/html_node/emacs/Name-Help.html")[`describe-symbol`]..

See #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/Configuration.md")[Tinymist Server Configuration]
for references.


==== Launch Arguments

For example:

```el
(with-eval-after-load 'eglot
  (with-eval-after-load 'typst-ts-mode
    (add-to-list 'eglot-server-programs
                 `((typst-ts-mode) .
                   ,(eglot-alternatives `((,typst-ts-lsp-download-path "--font-path" "<your-font-path>")
                                          ("tinymist" "--font-path" "<your-font-path>")
                                          "typst-lsp"))))))
```

You can run command `tinymist help lsp` to view all available launch arguments for
configuration.

