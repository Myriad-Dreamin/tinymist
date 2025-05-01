#import "mod.typ": *

#show: book-page.with(title: [Command Line Interface (CLI)])

The difference between typst-cli and tinymist-cli is that the latter one focuses on the features requiring code analysis or helping the language server. For example, `tinymist-cli` also provides a `compile` command, but it doesn't provide a `query` or `watch` command, which are provided by `typst-cli`. This is because `tinymist compile` also collects and saves the compilation commands needed by the language server.

== Servers

=== Starting a Language Server Following LSP Protocol

To start a language server following the #link("https://microsoft.github.io/language-server-protocol/")[Language Server Protocol], please use the following command:

```bash
tinymist lsp
```

Or simply runs the CLI without any arguments:

```bash
tinymist
```

=== Starting a Preview Server

To start a preview server, please use the following command:

```bash
tinymist preview path/to/main.typ
```

See #link("https://enter-tainer.github.io/typst-preview/standalone.html")[Arguments].

=== Starting a debug adapter Server Following DAP Protocol

To start a debug adapter following the #link("https://microsoft.github.io/debug-adapter-protocol//")[Debug Adapter Protocol], please use the following command:

```bash
tinymist dap
```

== Commands

=== Compiling a Document

The `tinymist compile` command is compatible with `typst compile`:

```
tinymist compile path/to/main.typ
```

To save the compilation command to the lock file:

```bash
tinymist compile --save-lock path/to/main.typ
```

To save the compilation command to the lock file at the path `some/tinymist.lock`:

```bash
tinymist compile --lockfile some/tinymist.lock path/to/main.typ
```

The lock file feature is in development. It is to help the language server to understand the structure of your projects. See #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/vscode/Configuration.md#tinymistprojectresolution")[Configuration: tinymist.projectResolution].

=== Running Tests

To run tests, you can use the `test` command, which is also compatible with `typst compile`:

```bash
tinymist test path/to/main.typ
```

The `test` command will defaultly run all the functions whose names are staring with `test-` related the the main file:

```typ
#let test-it() = []
```

See #cross-link("/feature/testing.typ")[Docs: Testing Features] for more information.

=== Generating shell completion script

To generate a bash-compatible completion script:

```bash
tinymist completion bash
```

Available values for the shell parameter are `bash`, `elvish`, `fig`, `fish`, `powershell`, `zsh`, and `nushell`.
