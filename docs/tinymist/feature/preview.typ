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

=== Theme-aware template

The only two abstracted theme kinds are supported: `light` and `dark`. You can use the following code to get the theme:

```typ
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
