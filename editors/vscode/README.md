# Tinymist Typst VS Code Extension

A VS Code extension for Typst.

## Features

See [Tinymist features](../../README.md#features) for a list of features.

## Usage Tips

- This extension compiles to PDF, but it doesn't have a PDF viewer yet. To view
  the output as you work, install a PDF viewer extension, such as
  `vscode-pdf`.
- To configure when PDFs are compiled:
  1. Open settings.
    - File -> Preferences -> Settings (Linux, Windows)
    - Code -> Preferences -> Settings (Mac)
  2. Search for "Tinymist Export PDF".
  3. Change the Export PDF setting.
    - `onSave` makes a PDF after saving the Typst file.
    - `onType` makes PDF files live, as you type.
    - `never` disables PDF compilation.
    - "onDocumentHasTitle" makes a PDF when the document has a title and, as you save.
- To configure where PDFs are saved:
  1. Open settings.
  2. Search for "Tinymist Output Path".
  3. Change the Export PDF setting. This is the path pattern to store artifacts, you can use `$root` or `$dir` or `$name` to do magic configuration
    - e.g. `$root/$dir/$name` (default) for `$root/path/to/main.pdf`.
    - e.g. `$root/target/$dir/$name` for `$root/target/path/to/main.pdf`.
    - e.g. `$root/target/foo` for `$root/target/foo.pdf`. This will ensure that the output is always output to `target/foo.pdf`.
  4. Note: the output path should be substituted as an absolute path.

## Technical

The extension uses [Tinymist](https://github.com/Myriad-Dreamin/tinymist) on the
backend.
