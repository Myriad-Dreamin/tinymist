#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: [Helix])

Run and configure tinymist in helix for Typst.

== Features

See #link("https://github.com/Myriad-Dreamin/tinymist#features")[Tinymist Features] for a list of features.

#include "common-finding-executable.typ"

== Setup Server

Update `.config/helix/languages.toml` to use tinymist.

```toml
[language-server.tinymist]
command = "tinymist"

[[language]]
name = "typst"
language-servers = ["tinymist"]
```

== Tips

=== Getting Preview Feature

#cross-link("/feature/preview.typ", reference: <default-preview>)[Default Preview Feature] and #cross-link("/feature/preview.typ", reference: <background-preview>)[Background Preview Feature] are suitable in helix.

=== Working with Multiple-File Projects

There is a way in #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/README.md#multiple-file-project-support")[Neovim];, but you cannot invoke related commands with arguments by #link("https://docs.helix-editor.com/commands.html")[:lsp-workspace-command] in helix. As a candidate solution, assuming your having following directory layout:

```plain
├── .helix
│   └── languages.toml
└── main.typ
```

You could create .helix/languages.toml in the project folder with the following contents:

```toml
[language-server.tinymist.config]
typstExtraArgs = ["main.typ"]
```

Then all diagnostics and autocompletion will be computed according to the `main.typ`.

Note: With that configuration, if you’re seeing a file that is not reachable by `main.typ`, you will not get diagnostics and autocompletion correctly in that file.

== Extra Settings

To configure language server, you can edit the `language-server.tinymist` section. For example, if you want to export PDF on typing and output files in `$root_dir/target` directory:

```toml
[language-server.tinymist]
command = "tinymist"
config = { exportPdf = "onType", outputPath = "$root/target/$dir/$name" }
```

See #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/Configuration.md")[Tinymist Server Configuration]
for references.
