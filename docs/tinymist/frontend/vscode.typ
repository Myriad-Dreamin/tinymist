#import "/docs/tinymist/frontend/mod.typ": *

#show: book-page.with(title: [VS Cod(e,ium)])

A VS Code or VS Codium extension for Typst. You can find the extension on:

- Night versions available at #link("https://github.com/Myriad-Dreamin/tinymist/actions")[GitHub Actions];.
- Stable versions available at #link("https://marketplace.visualstudio.com/items?itemName=myriad-dreamin.tinymist")[Visual Studio Marketplace];.
- Stable versions available at #link("https://open-vsx.org/extension/myriad-dreamin/tinymist")[Open VSX];.

== Features
<features>
See #link("https://github.com/Myriad-Dreamin/tinymist#features")[Tinymist Features] for a list of features.

== Usage Tips
<usage-tips>
=== Initializing with a Template
<initializing-with-a-template>
To initialize a Typst project:
- Use command `Typst init template` (tinymist.initTemplate) to initialize a new Typst project based on a template.
- Use command `Typst show template` (tinymist.showTemplateGallery) to show available Typst templates for picking up a template to initialize.


🎉 If your template contains only a single file, you can also insert the template content in place with command:
- Use command `Typst template in place` (tinymist.initTemplateInPlace) and input a template specifier for initialization.

=== Configuring LSP-Enhanced Formatters
<configuring-lsp-enhanced-formatters>
+ Open settings.
+ Search for "Tinymist Formatter" and modify the value.
  - Use `"formatterMode": "typstyle"` for #link("https://github.com/Enter-tainer/typstyle")[typstyle];.
  - Use `"formatterMode": "typstfmt"` for #link("https://github.com/astrale-sharp/typstfmt")[typstfmt];.

Tips: to enable formatting on save, you should add extra settings for typst language:

```json
{
  "[typst]": {
    "editor.formatOnSave": true
  }
}
```

=== Configuring/Using Tinymist’s Activity Bar (Sidebar)
<configuringusing-tinymists-activity-bar-sidebar>
If you don’t like the activity bar, you can right-click on the activity bar and uncheck "Tinymist" to hide it.

==== Symbol View
<symbol-view>
- Search symbols by keywords, descriptions, or handwriting.
- See symbols grouped by categories.
- Click on a symbol, then it will be inserted into the editor.

==== Tool View

- Template Gallery: Show available Typst templates for picking up a template to initialize.
- Document Summary: Show a summary of the current document.
- Symbols: Show symbols in the current document.
- Fonts: Show fonts in the current document.
- Profiling: Profile the current document.

==== Package View

- Create or open some local typst packages.
- Show a list of available typst packages and invoke associated commands.

==== Content View

- Show thumbnail content of the current document, which is useful for creating slides.

==== Label View

- Show labels in the current workspace.

==== Outline View

- Show outline of exported document, viewing typst as a markup language.
  - This is slightly different from the LSP-provided document outline, which shows the syntax structure of the document, viewing typst as a programming language.

=== Preview Command
<preview-command>
Open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

You can also use the shortcut (Ctrl+K V).

=== Theme-aware template (previewing)
<theme-aware-template-previewing>
In short, there is a `sys.inputs` item added to the compiler when your document is under the context of _user editing or previewing task_. You can use it to configure your template:

```typ
#let preview-args = json.decode(sys.inputs.at("x-preview", default: "{}"))
// One is previewing the document.
#let is-preview = sys.inputs.has("x-preview")
// `dark` or `light`
#let preview-theme = preview-args.at("theme", default: "light")
```

For details, please check #link("https://myriad-dreamin.github.io/tinymist/feature/preview.html#label-sys.inputs")[Preview’s sys.inputs];.

=== Configuring path to search fonts
<configuring-path-to-search-fonts>
To configure path to search fonts:
+ Open settings.
  - File -\> Preferences -\> Settings (Linux, Windows).
  - Code -\> Preferences -\> Settings (Mac).
+ Search for "Tinymist Font Paths" for providing paths to search fonts order-by-order.
+ Search for "Tinymist System Fonts" for disabling system fonts to be searched, which is useful for reproducible rendering your PDF documents.
+ Reload the window or restart the vscode editor to make the settings take effect.


*Note:* you must provide absolute paths.
*Note:* you can use vscode variables in the settings, see #link("https://www.npmjs.com/package/vscode-variables")[vscode-variables] for more information.

=== Configuring path to root directory
<configuring-path-to-root-directory>
To configure the root path resolved for Typst compiler:
+ Open settings.
+ Search for "Tinymist Root Path" and modify the value.
+ Reload the window or restart the vscode editor to make the settings take effect. *Note:* you must provide absolute paths.

=== Managing Local Packages

+ Use `Typst: Create Typst Local Package` command to create a local package.
+ Use `Typst: Open Typst Local Package` command to open a local package.
+ View and manage a list of available local packages in the "PACKAGE" view in the activity bar.

=== Compiling PDF
<compiling-pdf>
This extension compiles to PDF, but it doesn’t have a PDF viewer yet. To view the output as you work, install a PDF viewer extension, such as `vscode-pdf`.

To find a way to compile PDF:
- Click the code len `Export PDF` at the top of document, or use command `Typst Show PDF ...`, to show the current document to PDF.
- Use command `Typst Export PDF` to export the current document to PDF.
- There are code lens buttons at the start of the document to export your document to PDF or other formats.

To configure when PDFs are compiled:
+ Open settings.
+ Search for "Tinymist Export PDF".
+ Change the "Export PDF" setting.
  - `onSave` makes a PDF after saving the Typst file.
  - `onType` makes PDF files live, as you type.
  - `never` disables PDF compilation.
  - `onDocumentHasTitle` makes a PDF when the document has a title and, as you save.

To configure where PDFs are saved:

+ Open settings.
+ Search for "Tinymist Output Path".
+ Change the "Output Path" setting. This is the path pattern to store
  artifacts, you can use `$root` or `$dir` or `$name` to do magic
  configuration
  - e.g. `$root/$dir/$name` (default) for `$root/path/to/main.pdf`.
  - e.g. `$root/target/$dir/$name` for `$root/target/path/to/main.pdf`.
  - e.g. `$root/target/foo` for `$root/target/foo.pdf`. This will ensure
    that the output is always output to `target/foo.pdf`.

*Note:* the output path should be substituted as an absolute path.

=== Exporting to Other Formats
<exporting-to-other-formats>
You can export your documents to various other formats by lsp as well.
Currently, the following formats are supported:
- Official svg, png, and pdf.
- Unofficial html, md (typlite), and txt
- Query Results (into json, yaml, or txt), and pdfpc (by `typst query --selector <pdfpc-file>`, for #link("https://touying-typ.github.io/touying/")[Touying];)

See
#link("https://myriad-dreamin.github.io/tinymist/feature/export.html")[Docs: Exporting Documents]
for more information.

=== Working with Multiple-File Projects
<working-with-multiple-file-projects>
You can pin a main file by command.
- Use command `Typst Pin Main` (tinymist.pinMainToCurrent) to set the current file as the main file.
- Use command `Typst Unpin Main` (tinymist.unpinMain) to unset the main file.

#note-box[
  `tinymist.pinMain` is a stateful command, and tinymist doesn't remember it between sessions (closing and opening the editor).
]

=== Passing Extra CLI Arguments
<passing-extra-cli-arguments>
There is a *global* configuration `tinymist.typstExtraArgs` to pass extra arguments to tinymist LSP, like what you usually do with `typst-cli` CLI. For example, you can set it to `["--input=awa=1", "--input=abaaba=2", "main.typ"]` to configure `sys.inputs` and entry for compiler, which is equivalent to make LSP run like a `typst-cli` with such arguments:

```
typst watch --input=awa=1 --input=abaaba=2 main.typ
```

Supported arguments:
- entry file: The last string in the array will be treated as the entry file.
  - This is used to specify the *default* entry file for the compiler, which may be overridden by other settings.
- `--input`: Add a string key-value pair visible through `sys.inputs`.
- `--font-path` (environment variable: `TYPST_FONT_PATHS`), Font paths, maybe overridden by `tinymist.fontPaths`.
- `--ignore-system-fonts`: Ensures system fonts won’t be searched, maybe overridden by `tinymist.systemFonts`.
- `--creation-timestamp` (environment variable: `SOURCE_DATE_EPOCH`): The document’s creation date formatted as a #link("https://reproducible-builds.org/specs/source-date-epoch/")[UNIX timestamp];.
- `--cert` (environment variable: `TYPST_CERT`): Path to CA certificate file for network access, especially for downloading typst packages.

*Note:* Fix entry to `main.typ` may help multiple-file projects
but you may loss diagnostics and autocompletions in unrelated files.

*Note:* the arguments has quite low priority, and that may be overridden
by other settings.

== Contributing
<contributing>
You can submit issues or make PRs to #link("https://github.com/Myriad-Dreamin/tinymist")[GitHub];.
