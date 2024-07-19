#import "mod.typ": *

#show: book-page.with(title: "Tinymist Preview Feature")

See #link("https://enter-tainer.github.io/typst-preview/arch.html")[Typst-Preview Developer Guide].

== CLI Integration

```bash
typst-preview /abs-path/to/main.typ --partial-rendering
```

is equivalent to

```bash
tinymist preview /abs-path/to/main.typ --partial-rendering
```

== `sys.inputs`

If the document is compiled by lsp, you can use `sys.inputs` to get the preview arguments:

```
#let preview-args = json.decode(sys.inputs.at("x-preview", default: "{}"))
#let preview-theme = preview-args.at("theme", default: "light")
```

== LSP Integration

The preview feature is also integrated into the LSP server. You can use the preview feature like when you were using it in `mgt19937.typst-preview` extension.

// The preview command accept a list of string arguments. And

// ```js
// vscode.executeCommand('tinymist.startPreview', ['/abs-path/to/main.typ', '--partial-rendering']);
// ```

// is equivalent to

// ```bash
// tinymist preview /abs-path/to/main.typ --partial-rendering
// ```
