# Tinymist Typst VS Code Extension

A VS Code or VS Codium extension for Typst. You can find the extension on:

- Night versions available at [GitHub Actions](https://github.com/Myriad-Dreamin/tinymist/actions).
- Stable versions available at [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=myriad-dreamin.tinymist).
- Stable versions available at [Open VSX](https://open-vsx.org/extension/myriad-dreamin/tinymist).

## Features

See [Tinymist Features](https://github.com/Myriad-Dreamin/tinymist#features) for a list of features.

## Usage Tips

### Initializing with a Template

To initialize a Typst project:
- Use command `Typst Init Template` (tinymist.initTemplate) to initialize a new Typst project based on a template.
- Use command `Typst Show Template` (tinymist.showTemplateGallery) to show available Typst templates for picking up a template to initialize.

### Configuring path to search fonts

To configure path to search fonts:
1. Open settings.
  - File -> Preferences -> Settings (Linux, Windows).
  - Code -> Preferences -> Settings (Mac).
2. Search for "Tinymist Font Paths" for providing paths to search fonts order-by-order.
3. Search for "Tinymist No System Fonts" for disabling system fonts to be searched, which is useful for reproducible rendering your PDF documents.
4. Reload the window or restart the vscode editor to make the settings take effect.
**Note:** you must provide absolute paths.
**Note':** you can use vscode variables in the settings, see [vscode-variables](https://www.npmjs.com/package/vscode-variables) for more information.

### Configuring path to root directory

To configure the root path resolved for Typst compiler:
1. Open settings.
2. Search for "Tinymist Root Path" and modify the value.
3. Reload the window or restart the vscode editor to make the settings take effect.
**Note:** you must provide absolute paths.

### Compiling PDF

This extension compiles to PDF, but it doesn't have a PDF viewer yet. To view the output as you work, install a PDF viewer extension, such as `vscode-pdf`.

To find a way to compile PDF:
- Click the code len `Export PDF` at the top of document, or use command `Typst Show PDF ...`, to show the current document to PDF.
- Use command `Typst Export PDF` to export the current document to PDF.
- There are code lens buttons at the start of the document to export your document to PDF or other formats.

To configure when PDFs are compiled:
1. Open settings.
2. Search for "Tinymist Export PDF".
3. Change the "Export PDF" setting.
  - `onSave` makes a PDF after saving the Typst file.
  - `onType` makes PDF files live, as you type.
  - `never` disables PDF compilation.
  - "onDocumentHasTitle" makes a PDF when the document has a title and, as you save.

To configure where PDFs are saved:

1. Open settings.
2. Search for "Tinymist Output Path".
3. Change the "Output Path" setting. This is the path pattern to store artifacts, you can use `$root` or `$dir` or `$name` to do magic configuration
  - e.g. `$root/$dir/$name` (default) for `$root/path/to/main.pdf`.
  - e.g. `$root/target/$dir/$name` for `$root/target/path/to/main.pdf`.
  - e.g. `$root/target/foo` for `$root/target/foo.pdf`. This will ensure that the output is always output to `target/foo.pdf`.
4. Note: the output path should be substituted as an absolute path.

### Working with Multiple-File Projects

- The solution is a bit internal, but you can set a main file temporarily by command.
  - Use command `Typst Pin Main` (tinymist.pinMainToCurrent) to set the current file as the main file.
  - Use command `Typst Unpin Main` (tinymist.unpinMain) to unset the main file.

### Passing Extra CLI Arguments

There is a **global** configuration `tinymist.typstExtraArgs` to pass extra arguments to tinymist LSP, like what you usually do with `typst-cli` CLI. For example, you can set it to `["--input=awa=1", "--input=abaaba=2"]` to configure `sys.inputs`.

Note: the arguments has quite low priority, and that may be overridden by other settings.

## Contributing

You can submit issues or make PRs to [GitHub](https://github.com/Myriad-Dreamin/tinymist).
