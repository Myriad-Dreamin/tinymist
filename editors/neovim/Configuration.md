# Tinymist Server Configuration

## `outputPath`

The path pattern to store Typst artifacts, you can use `$root` or `$dir` or `$name` to do magic configuration, e.g. `$dir/$name` (default) and `$root/target/$dir/$name`.

- **Type**: `string`

## `exportPdf`

The extension can export PDFs of your Typst files. This setting controls whether this feature is enabled and how often it runs.

- **Type**: `string`
- **Enum**:
  - `never`: Never export PDFs, you will manually run typst.
  - `onSave`: Export PDFs when you save a file.
  - `onType`: Export PDFs as you type in a file.
  - `onDocumentHasTitle`: Export PDFs when a document has a title (and save a file), which is useful to filter out template files.
- **Default**: `"never"`

## `rootPath`

Configure the root for absolute paths in typst. Hint: you can set the rootPath to `-`, so that tinymist will always use parent directory of the file as the root path. Note: for neovim users, if it complains root not found, you must set `require("lspconfig")["tinymist"].setup { root_dir }` as well, see [tinymist#528](https://github.com/Myriad-Dreamin/tinymist/issues/528).

- **Type**: `string` or `null`

## `semanticTokens`

Enable or disable semantic tokens (LSP syntax highlighting)

- **Type**: `string`
- **Enum**:
  - `enable`: Use semantic tokens for syntax highlighting
  - `disable`: Do not use semantic tokens for syntax highlighting
- **Default**: `"enable"`

## `systemFonts`

A flag that determines whether to load system fonts for Typst compiler, which is useful for ensuring reproducible compilation. If set to null or not set, the extension will use the default behavior of the Typst compiler. Note: You need to restart LSP to change this options. 

- **Type**: `boolean`
- **Default**: `true`

## `fontPaths`

A list of file or directory path to fonts. Note: The configuration source in higher priority will **override** the configuration source in lower priority. The order of precedence is: Configuration `tinymist.fontPaths` > Configuration `tinymist.typstExtraArgs.fontPaths` > LSP's CLI Argument `--font-path` > The environment variable `TYPST_FONT_PATHS` (a path list separated by `;` (on Windows) or `:` (Otherwise)). Note: If the path to fonts is a relative path, it will be resolved based on the root directory. Note: In VSCode, you can use VSCode variables in the path, e.g. `${workspaceFolder}/fonts`.

- **Type**: `array` or `null`

## `compileStatus`

In VSCode, enable compile status meaning that the extension will show the compilation status in the status bar. Since Neovim and Helix don't have a such feature, it is disabled by default at the language server label.

- **Type**: `string`
- **Enum**:
  - `enable`
  - `disable`
- **Default**: `"disable"`

## `typstExtraArgs`

You can pass any arguments as you like, and we will try to follow behaviors of the **same version** of typst-cli. Note: the arguments may be overridden by other settings. For example, `--font-path` will be overridden by `tinymist.fontPaths`.

- **Type**: `array`
- **Default**: `[]`

## `formatterMode`

The extension can format Typst files using typstfmt or typstyle.

- **Type**: `string`
- **Enum**:
  - `disable`: Formatter is not activated.
  - `typstyle`: Use typstyle formatter.
  - `typstfmt`: Use typstfmt formatter.
- **Default**: `"disable"`

## `formatterPrintWidth`

Set the print width for the formatter, which is a **soft limit** of characters per line. See [the definition of *Print Width*](https://prettier.io/docs/en/options.html#print-width). Note: this has lower priority than the formatter's specific configurations.

- **Type**: `number`
- **Default**: `120`

## `completion.triggerOnSnippetPlaceholders`

Whether to trigger completions on arguments (placeholders) of snippets. For example, `box` will be completed to `box(|)`, and server will request the editor (lsp client) to request completion after moving cursor to the placeholder in the snippet. Note: this has no effect if the editor doesn't support `editor.action.triggerSuggest` or `tinymist.triggerSuggestAndParameterHints` command. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`

## `completion.postfix`

Whether to enable postfix code completion. For example, `[A].box|` will be completed to `box[A]|`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `completion.postfixUfcs`

Whether to enable UFCS-style completion. For example, `[A].box|` will be completed to `box[A]|`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `completion.postfixUfcsLeft`

Whether to enable left-variant UFCS-style completion. For example, `[A].table|` will be completed to `table(|)[A]`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `completion.postfixUfcsRight`

Whether to enable right-variant UFCS-style completion. For example, `[A].table|` will be completed to `table([A], |)`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`
