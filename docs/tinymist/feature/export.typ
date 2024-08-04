#import "mod.typ": *

#show: book-page.with(title: "Exporting Documents")

You can export your documents to various formats using the `export` feature.

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

The first argument is the path to the file you want to export and the second argument is an object containing additional options.
