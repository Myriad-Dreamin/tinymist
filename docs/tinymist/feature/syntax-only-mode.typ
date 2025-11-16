#import "mod.typ": *

#show: book-page.with(title: [Syntax-Only Mode])

The syntax-only mode is available since `tinymist` v0.14.2.

When working under power-saving mode or with resource-consumed projects, typst compilations costs too much CPU and memory resources. From a simple test on a typst document with 200 pages, containing complex figures and WASM plugin calls, editing a large `.typ` file on a windows laptop (i9-12900H), the CPU and memory usage are as follows:

#align(center, table(
  columns: 4,
  [Mode], [CPU Usage], [Memory Usage (Code Compilation)], [Memory Usage (Incremental Compilation)],
  [Normal Mode], [5% \~ 12%], [2.72 GB], [6.62\~8.73GB],
  [Syntax-Only Mode], [0% \~ 0.6%], [15.0 MB], [15.1\~16.0 MB],
))

You can configure the extension to run in syntax only mode, i.e. only performing elementary tasks, like syntax checking, syntax-only code analysis and formatting by setting the `tinymist.syntaxOnly` to `enable` or `onPowerSaving` in the configuration.

The syntax-only mode is known to disable or limit the functionality of the following features:
- typst preview feature.
- compilation diagnostics.
- label completion.

The syntax-only mode will be able to work with following features:
- export PDF or other formats.
- label completion.

If there are any other features that you find it work abnormally, please report issues to the #link("https://github.com/Myriad-Dreamin/tinymist/issues")[GitHub Issues].
