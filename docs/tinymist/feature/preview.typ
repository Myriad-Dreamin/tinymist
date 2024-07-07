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
