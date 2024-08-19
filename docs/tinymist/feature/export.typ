#import "mod.typ": *

#show: book-page.with(title: "Exporting Documents")

You can export your documents to various formats using the `export` feature.

== Export from Query Result

=== Hello World Example (VSCode Tasks)

You can export the result of a query as text using the `export` command.

Given a code:

```typ
#println("Hello World!")
#println("Hello World! Again...")
```

LSP should export the result of the query as text with the following content:

```txt
Hello World!
Hello World! Again...
```

This requires the following configuration in your `tasks.json` file:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "Query as Text",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "query",
        "query.format": "txt",
        "query.outputExtension": "out",
        "query.field": "value",
        "query.selector": "<print-effect>",
        "query.one": true
      }
    },
  ]
}
```

See the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/e2e-workspaces/print-state")[Sample Workspace: print-state] for more details.

=== Pdfpc Example (VSCode Tasks)

A more practical example is exporting the result of a query as a pdfpc file. You can use the following configuration in your `tasks.json` file to export the result of a query as a pdfpc file, which is adapted by #link("https://touying-typ.github.io/touying/")[Touying Slides].

```json
{
  "label": "Query as Pdfpc",
  "type": "typst",
  "command": "export",
  "export": {
    "format": "query",
    "query.format": "json",
    "query.outputExtension": "pdfpc",
    "query.selector": "<pdfpc-file>",
    "query.field": "value",
    "query.one": true
  }
}
```

To simplify configuration,

```json
{
  "label": "Query as Pdfpc",
  "type": "typst",
  "command": "export",
  "export": {
    "format": "pdfpc"
  }
}
```

== VSCode: Task Configuration

You can configure tasks in your `tasks.json` file to "persist" the arguments for exporting documents.

Example:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "Export as Html",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "html"
      }
    },
    {
      "label": "Export as Markdown",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "markdown"
      }
    },
    {
      "label": "Export as Plain Text",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "html"
      }
    },
    {
      "label": "Export as SVG",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "svg",
        "merged": true
      }
    },
    {
      "label": "Export as PNG",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "png",
        // Default fill is white, but you can set it to transparent.
        "fill": "#00000000",
        "merged": true
      }
    },
    {
      "label": "Query as Pdfpc",
      "type": "typst",
      "command": "export",
      "export": {
        "format": "pdfpc"
      }
    },
    {
      "label": "Export as PNG and SVG",
      "type": "typst",
      "command": "export",
      "export": {
        // You can export multiple formats at once.
        "format": ["png", "svg"],
        // To make a visual effect, we set an obvious low resolution.
        // For a nice result, you should set a higher resolution like 288.
        "png.ppi": 24,
        "merged": true,
        // To make a visual effect, we set an obvious huge gap.
        // For a nice result, you should set a smaller gap like 10pt.
        "merged.gap": "100pt"
      }
    }
  ]
}
```

#let packages = json("/editors/vscode/package.json")

*todo: documenting export options.*

#raw(lang: "json", json.encode(packages.contributes.taskDefinitions, pretty: true), block: true)

After configuring the tasks, you can run them using the command palette.
+ Press `Ctrl+Shift+P` to open the command palette.
+ Type `Run Task` and select the task you want to run.
+ Select the task you want to run.

== Neovim: Export Commands

You can call the following export commands.
- `tinymist.exportSvg`
- `tinymist.exportPng`
- `tinymist.exportPdf`
- `tinymist.exportHtml`
- `tinymist.exportMarkdown`
- `tinymist.exportText`
- `tinymist.exportQuery`

The first argument is the path to the file you want to export and the second argument is an object containing additional options.
