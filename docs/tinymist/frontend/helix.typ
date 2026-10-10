#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: [Tinymist Helix Support for Typst])

Run and configure tinymist in helix for Typst.

= Features

See #link("https://github.com/Myriad-Dreamin/tinymist#features")[Tinymist Features] for a list of features.

= Finding Executable
<finding-executable>

#include "common-finding-executable.typ"

To use it in helix, install the helix editor itself from the #link("https://github.com/helix-editor/helix/releases")[official releases];, and keep the `hx` executable and the `runtime/` directory from the release archive together (`hx` locates its runtime relative to its own path).

To verify the setup, open a `.typ` file in helix: the status bar will show a diagnostics count (e.g. `* 1`) once the language server is up. You can also start helix with `hx --log <file>` and check the log for the initialization of the language server.

= Setup Server

Update the helix configuration file to use tinymist. On Unix-like systems this is `~/.config/helix/languages.toml`; on Windows it is `%APPDATA%\helix\languages.toml`. Alternatively, you can use a project-level `.helix/languages.toml` (see the multiple-file projects tip below).

```toml
[language-server.tinymist]
command = "tinymist"

[[language]]
name = "typst"
language-servers = ["tinymist"]
```

= Tips

== Getting Preview Feature

#cross-link("/feature/preview.typ", reference: <default-preview>)[Default Preview Feature] and #cross-link("/feature/preview.typ", reference: <background-preview>)[Background Preview Feature] are suitable in helix.

To get a live preview in helix, enable the background preview in the server configuration:

```toml
[language-server.tinymist.config]
preview.background.enabled = true
```

See the #github-link("/editors/helix/README.md#extra-settings")[Extra Settings] section below for the full example with preview arguments.

== Working under Power-Saving Mode or with Resource-consumed Projects
<working-with-resource-consumed-projects>
#include "syntax-only.typ"

== Working with Multiple-File Projects

There is a way in #github-link("/editors/neovim/README.md#working-with-multiple-files-projects")[Neovim];, but you cannot invoke related commands with arguments by #link("https://docs.helix-editor.com/commands.html")[:lsp-workspace-command] in helix. As a candidate solution, assuming you have the following directory layout:

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

Note that with the current tinymist releases, the entry file may not be compiled on the first open, showing no diagnostics at all; it starts working after the first edit or save in that file.

Also note that with that configuration, if you are seeing a file that is not reachable by `main.typ`, you will not get diagnostics for that file, though completions of globally defined symbols (e.g. `#let` definitions in the entry file) still work.

= Extra Settings

To configure the language server, edit the `language-server.tinymist` section. For example, if you want to export PDF on typing and output files in the `$root/target` directory. The `outputPath` template variables are `$root` (the project root), `$dir` (the directory of the input file), and `$name` (the input file name without extension):

```toml
[language-server.tinymist]
command = "tinymist"
config = { exportPdf = "onType", outputPath = "$root/target/$dir/$name" }
```

To enable a live preview you can use the `preview.background`:

```toml
[language-server.tinymist]
command = "tinymist"
config = { preview.background.enabled = true, preview.background.args = ["--data-plane-host=127.0.0.1:23635", "--invert-colors=never", "--open"] }
```

Diagnostics messages are not shown inline by default: the status bar only displays a count (e.g. `* 3`), and you need to hover over a position to read the message. To display messages directly, enable the `inline-diagnostics` (or `end-of-line-diagnostics`) option in your helix `config.toml`:

```toml
[editor.inline-diagnostics]
cursor-line = "hint"
```

See #github-link("/editors/neovim/Configuration.md")[Tinymist Server Configuration]
for references.
