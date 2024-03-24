# Tinymist Server Configuration

## `tinymist.outputPath`

The path pattern to store Typst artifacts, you can use `$root` or `$dir` or `$name` to do magic configuration, e.g. `$dir/$name` (default) and `$root/target/$dir/$name`.

- **Type**: `string`

## `tinymist.exportPdf`

The extension can export PDFs of your Typst files. This setting controls whether this feature is enabled and how often it runs.

- **Type**: `string`
- **Enum**:
  - `auto`: Select best solution automatically. (Recommended)
  - `never`: Never export PDFs, you will manually run typst.
  - `onSave`: Export PDFs when you save a file.
  - `onType`: Export PDFs as you type in a file.
  - `onDocumentHasTitle`: Export PDFs when a document has a title (and save a file), which is useful to filter out template files.
- **Default**: `"auto"`

## `tinymist.rootPath`

Configure the root for absolute paths in typst

- **Type**: `string` or `null`

## `tinymist.semanticTokens`

Enable or disable semantic tokens (LSP syntax highlighting)

- **Type**: `string`
- **Enum**:
  - `enable`: Use semantic tokens for syntax highlighting
  - `disable`: Do not use semantic tokens for syntax highlighting
- **Default**: `"enable"`

## `tinymist.systemFonts`

A flag that determines whether to load system fonts for Typst compiler, which is useful for ensuring reproducible compilation. If set to null or not set, the extension will use the default behavior of the Typst compiler.

- **Type**: `boolean` or `null`

## `tinymist.fontPaths`

Font paths, which doesn't allow for dynamic configuration. Note: you can use vscode variables in the path, e.g. `${workspaceFolder}/fonts`.

- **Type**: `array` or `null`

## `tinymist.typstExtraArgs`

You can pass any arguments as you like, and we will try to follow behaviors of the **same version** of typst-cli. Note: the arguments may be overridden by other settings. For example, `--font-path` will be overridden by `tinymist.fontPaths`.

- **Type**: `array`
- **Default**: `[]`

## `tinymist.serverPath`

The extension can use a local tinymist executable instead of the one bundled with the extension. This setting controls the path to the executable.

- **Type**: `string` or `null`

## `tinymist.trace.server`

Traces the communication between VS Code and the language server.

- **Type**: `string`
- **Enum**:
  - `off`
  - `messages`
  - `verbose`
- **Default**: `"off"`

## `tinymist.experimentalFormatterMode`

The extension can format Typst files using typstfmt (experimental).

- **Type**: `string`
- **Enum**:
  - `disable`: Formatter is not activated.
  - `enable`: Experimental formatter is activated.
- **Default**: `"disable"`
