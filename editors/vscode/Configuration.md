# Tinymist Server Configuration

## `tinymist.projectResolution`

This configuration specifies the way to resolved projects.

- **Type**: `string`
- **Enum**:
  - `singleFile`: Manage typst documents like what we did in Markdown. Each single file is an individual document and no project resolution is needed.
  - `lockDatabase`: Manage typst documents like what we did in Rust. For each workspace, tinymist tracks your preview and compilation history, and stores the information in a lock file. Tinymist will automatically selects the main file to use according to the lock file. This also allows other tools push preview and export tasks to language server by updating the lock file.
- **Default**: `"singleFile"`

## `tinymist.outputPath`

The path pattern to store Typst artifacts, you can use `$root` or `$dir` or `$name` to do magic configuration, e.g. `$dir/$name` (default) and `$root/target/$dir/$name`.

- **Type**: `string`

## `tinymist.exportPdf`

The extension can export PDFs of your Typst files. This setting controls whether this feature is enabled and how often it runs.

- **Type**: `string`
- **Enum**:
  - `never`: Never export PDFs, you will manually run typst.
  - `onSave`: Export PDFs when you save a file.
  - `onType`: Export PDFs as you type in a file.
  - `onDocumentHasTitle`: Export PDFs when a document has a title (and save a file), which is useful to filter out template files.
- **Default**: `"never"`

## `tinymist.rootPath`

Configure the root for absolute paths in typst. Hint: you can set the rootPath to `-`, so that tinymist will always use parent directory of the file as the root path. Note: for neovim users, if it complains root not found, you must set `require("lspconfig")["tinymist"].setup { root_dir }` as well, see [tinymist#528](https://github.com/Myriad-Dreamin/tinymist/issues/528).

- **Type**: `string` or `null`

## `tinymist.configureDefaultWordSeparator`

Whether to configure default word separators on startup

- **Type**: `string`
- **Enum**:
  - `enable`: Override the default word separators on startup
  - `disable`: Do not override the default word separators on startup
- **Default**: `"enable"`

## `tinymist.semanticTokens`

Enable or disable semantic tokens (LSP syntax highlighting)

- **Type**: `string`
- **Enum**:
  - `enable`: Use semantic tokens for syntax highlighting
  - `disable`: Do not use semantic tokens for syntax highlighting
- **Default**: `"enable"`

## `tinymist.typingContinueCommentsOnNewline`

Whether to prefix newlines after comments with the corresponding comment prefix.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.onEnterEvent`

Enable or disable [experimental/onEnter](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter) (LSP onEnter feature) to allow automatic insertion of characters on enter, such as `///` for comments. Note: restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.systemFonts`

A flag that determines whether to load system fonts for Typst compiler, which is useful for ensuring reproducible compilation. If set to null or not set, the extension will use the default behavior of the Typst compiler. Note: You need to restart LSP to change this options. 

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.fontPaths`

A list of file or directory path to fonts. Note: The configuration source in higher priority will **override** the configuration source in lower priority. The order of precedence is: Configuration `tinymist.fontPaths` > Configuration `tinymist.typstExtraArgs.fontPaths` > LSP's CLI Argument `--font-path` > The environment variable `TYPST_FONT_PATHS` (a path list separated by `;` (on Windows) or `:` (Otherwise)). Note: If the path to fonts is a relative path, it will be resolved based on the root directory. Note: In VSCode, you can use VSCode variables in the path, e.g. `${workspaceFolder}/fonts`.

- **Type**: `array` or `null`

## `tinymist.compileStatus`

In VSCode, enable compile status meaning that the extension will show the compilation status in the status bar. Since Neovim and Helix don't have a such feature, it is disabled by default at the language server label.

- **Type**: `string`
- **Enum**:
  - `enable`
  - `disable`
- **Default**: `"enable"`

## `tinymist.statusBarFormat`

Set format string of the server status. For example, `{compileStatusIcon}{wordCount} [{fileName}]` will format the status as `$(check) 123 words [main]`. Valid placeholders are:

- `{compileStatusIcon}`: Icon indicating the compile status
- `{wordCount}`: Number of words in the document
- `{fileName}`: Name of the file being compiled

Note: The status bar will be hidden if the format string is empty.

- **Type**: `string`
- **Default**: `"{compileStatusIcon} {wordCount} [{fileName}]"`

## `tinymist.typstExtraArgs`

You can pass any arguments as you like, and we will try to follow behaviors of the **same version** of typst-cli. Note: the arguments may be overridden by other settings. For example, `--font-path` will be overridden by `tinymist.fontPaths`.

- **Type**: `array`
- **Default**: `[]`

## `tinymist.serverPath`

The extension can use a local tinymist executable instead of the one bundled with the extension. This setting controls the path to the executable. The string "tinymist" means look up Tinymist in PATH.

- **Type**: `string` or `null`

## `tinymist.trace.server`

Traces the communication between VS Code and the language server.

- **Type**: `string`
- **Enum**:
  - `off`
  - `messages`
  - `verbose`
- **Default**: `"off"`

## `tinymist.formatterMode`

The extension can format Typst files using typstfmt or typstyle.

- **Type**: `string`
- **Enum**:
  - `disable`: Formatter is not activated.
  - `typstyle`: Use typstyle formatter.
  - `typstfmt`: Use typstfmt formatter.
- **Default**: `"disable"`

## `tinymist.formatterPrintWidth`

Set the print width for the formatter, which is a **soft limit** of characters per line. See [the definition of *Print Width*](https://prettier.io/docs/en/options.html#print-width). Note: this has lower priority than the formatter's specific configurations.

- **Type**: `number`
- **Default**: `120`

## `formatterTabSpaces`

Set the tab spaces for the formatter.

- **Type**: `number`
- **Default**: `2`

## `tinymist.showExportFileIn`

Configures way of opening exported files, e.g. inside of editor tabs or using system application.


## `tinymist.dragAndDrop`

Whether to handle drag-and-drop of resources into the editing typst document. Note: restarting the editor is required to change this setting.

- **Type**: `string`
- **Enum**:
  - `enable`
  - `disable`
- **Default**: `"enable"`

## `tinymist.copyAndPaste`

Whether to handle paste of resources into the editing typst document. Note: restarting the editor is required to change this setting.

- **Type**: `string`
- **Enum**:
  - `enable`
  - `disable`
- **Default**: `"enable"`

## `tinymist.renderDocs`

(Experimental) Whether to render typst elements in (hover) docs. In VS Code, when this feature is enabled, tinymist will store rendered results in the filesystem's temporary storage to show them in the hover content. Note: Please disable this feature if the editor doesn't support/handle image previewing in docs.

- **Type**: `string`
- **Enum**:
  - `enable`
  - `disable`
- **Default**: `"enable"`

## `tinymist.completion.triggerOnSnippetPlaceholders`

Whether to trigger completions on arguments (placeholders) of snippets. For example, `box` will be completed to `box(|)`, and server will request the editor (lsp client) to request completion after moving cursor to the placeholder in the snippet. Note: this has no effect if the editor doesn't support `editor.action.triggerSuggest` or `tinymist.triggerSuggestAndParameterHints` command. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`

## `tinymist.completion.postfix`

Whether to enable postfix code completion. For example, `[A].box|` will be completed to `box[A]|`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.completion.postfixUfcs`

Whether to enable UFCS-style completion. For example, `[A].box|` will be completed to `box[A]|`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.completion.postfixUfcsLeft`

Whether to enable left-variant UFCS-style completion. For example, `[A].table|` will be completed to `table(|)[A]`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.completion.postfixUfcsRight`

Whether to enable right-variant UFCS-style completion. For example, `[A].table|` will be completed to `table([A], |)`. Hint: Restarting the editor is required to change this setting.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.previewFeature`

Enable or disable preview features of Typst. Note: restarting the editor is required to change this setting.

- **Type**: `string`
- **Enum**:
  - `enable`
  - `disable`
- **Default**: `"enable"`

## `tinymist.preview.refresh`

Refresh preview when the document is saved or when the document is changed

- **Type**: `string`
- **Enum**:
  - `onSave`: Refresh preview on save
  - `onType`: Refresh preview on type
- **Default**: `"onType"`

## `tinymist.preview.scrollSync`

Configure scroll sync mode.

- **Type**: `string`
- **Enum**:
  - `never`: Disable automatic scroll sync
  - `onSelectionChangeByMouse`: Scroll preview to current cursor position when selection changes by mouse
  - `onSelectionChange`: Scroll preview to current cursor position when selection changes by mouse or keyboard (any source)
- **Default**: `"onSelectionChangeByMouse"`

## `tinymist.preview.partialRendering`

Only render visible part of the document. This can improve performance but still being experimental.

- **Type**: `boolean`
- **Default**: `true`

## `tinymist.preview.invertColors`

Invert colors of the preview (useful for dark themes without cost). Please note you could see the origin colors when you hover elements in the preview. It is also possible to specify strategy to each element kind by an object map in JSON format.


## `tinymist.preview.cursorIndicator`

(Experimental) Show typst cursor indicator in preview.

- **Type**: `boolean`
