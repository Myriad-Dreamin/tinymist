#import "mod.typ": *

#show: book-page.with(title: [Hook Script])

The hook script feature is available since `tinymist` v0.14.2.

Hook Scripts allow you to hook and customize certain behaviors of tinymist by providing code snippets that will be executed at specific events.

The hook scripts are run as typst scripts with some predefined variables. Since typst is sandboxed, the hook scripts cannot access system directly. However, you can still bind lsp commands to perform complex operations.

The following example demonstrates how to customize the paste behavior when pasting resources into the editing typst document.
- First, the editor client (VS Code) invokes the `tinymist.onPaste` and gets the action $cal(A)$ to perform. the action $cal(A)$ contains two fields `dir`, and `onConflict`, where `dir` is the directory to store the pasted resources, and `onConflict` is the script to execute when a conflict occurs (i.e., the file already exists). Note that `import` statements are allowed in the paste script.
- Then, the editor client checks the physical file system. If it detects conflict when creating a resource file, it again runs the `onConflict` script to determine next behavior.
- After several iterations, the editor client finally creates the resource files and inserts the corresponding Typst code into the document.

#let hook-graph(theme) = {
  let (colors, node, edge) = fletcher-ctx(theme)
  diagram(
    edge-stroke: 0.85pt,
    node-corner-radius: 3pt,
    edge-corner-radius: 4pt,
    mark-scale: 80%,
    node((0, 0), [Paste Handler (Client)], fill: colors.at(0), shape: fletcher.shapes.hexagon),
    node((0, 2), align(center)[Run Paste Script (Server)], fill: colors.at(1)),
    node((2, 0), [Paste Callback (Client)], fill: colors.at(0), shape: fletcher.shapes.hexagon),
    edge((0, 0), "dd", box(width: 6em)[```typc{ join(root, "assets") }```], "-}>"),
    edge((0, 0), "rr", "--}>"),
    edge(
      (0, 2),
      "r,uu,r",
      box(width: 7em)[```typc
      {
        dir: "/root/assets",
        onConflict: "{ ..; on-conflict() }"
      }
      ```],
      "-}>",
      label-pos: 40%,
    ),
    edge(
      (2, 0),
      "dddd,ll,uu",
      box(width: 15em)[
        #set par(justify: false)
        on conflict, \
        #raw(
          lang: "typc",
          ```typc   import "@local/hooks:0.1.0";
            hooks.on-conflict()
          ```.text,
        )],
      "-}>",
    ),
    edge((2, 0), "r", align(center, box(width: 5em, inset: (left: 1em), [Editor Actions])), "-}>"),
    // edge((2, 0), (2, 1), align(center)[Rendering\ Requests], "-}>"),
    // edge((2, -1), (2, 0), align(center)[Analysis\ Requests], "-}>"),
  )
}

#figure(
  cond-image(hook-graph),
  caption: [The workflow of running `tinymist.onPaste`],
) <fig:script-hook-workflow>

Specifically, three script hooks will be supported:
- Hook on Paste: customize the paste behavior when pasting resources into the editing typst document.
- Hook on Ex[prt]: customize the export behavior when a file change is detected in the workspaces.
- Hook on Generating Code Actions and Lenses: adding additional code actions by typst scripting.

= Customizing Paste Behavior

You could configure `tinymist.onPaste` to customize the paste behavior. It will be executed when pasting resources into the editing typst document. Two kinds of script code are supported:
- If the script code starts with `{` and ends with `}`, it will be evaluated as a typst code expression.
- Otherwise, it will be evaluated as a path pattern.

== Path Pattern (Stable)

When evaluated as a path pattern, the path variables could be used:
- `root`: the root of the workspace.
- `dir`: the directory of the current file.
- `name`: the name of the current file.

For example, the following path pattern
```
$root/assets
```
is evaluated as `/path/to/root/assets` when pasting `main.typ` in `/path/to/root/dir/main.typ`.

== Code Expression (Experimental)

When evaluated as a typst code expression, the script could use following predefined definitions:
- `root`: the root of the workspace.
- `dir`: the directory of the current file.
- `name`: the name of the current file.
- `join`: a function to join path segments, e.g. `join("a", "b", "c")` returns `a/b/c` on Unix-like systems.

For example, the following paste script

```typc
{ join(root, "x", dir, if name.ends-with(".png") ("imgs"), name) }
```

is evaluated as `/path/to/root/dir/imgs/main` when pasting `main.png` in `/path/to/root/dir/main.typ`.

The result of the paste script could also be a dictionary with the following fields:
- `dir`: the directory to store the pasted resources.

If the result is a string, it will be treated as the `dir` field, i.e. `{ dir: <result> }` and the editor client will creates the pasted resource files in the specified directory.

More fields will be supported in the future. If you have any suggestions, please feel free to open an issue.

= Customizing Export Behavior (Experimental)

You could configure `tinymist.onExport` to customize the export behavior. It will be executed when a file change is detected in the workspace.

For example, debouncing time:

```typc
{ debounce("100ms") }
```

For example, debounce and
- export by a custom handler and postprocess using `ghostscript`,
- export cover (first page) as SVG.

```typc
{ if debounce("1000ms") { (
  ( command: "myExtension.pdfWithGhostScript" ),
  ( export: "svg", pages: "1" ),
) } }
```

And define a custom command at client side (VS Code):
```js
async function pdfWithGhostScript() {
  const pdfPath = await vscode.commands.execute("tinymist.exportPdf");
  const outputPath = pdfPath.replace(/\.pdf$/, "-gs.pdf");
  return new Promise((resolve, reject) => {
    exec(`gs ... -sOutputFile=${outputPath} ${pdfPath}`, (error, stdout, stderr) => {
      if (error) {
        reject(error);
      } else {
        resolve(outputPath);
      }
    });
  });
}
```

Hint: you could create your own vscode extension to define such custom commands.

`tinymist.exportPdf` will be ignored if this configuration item is set.

= Providing Package-Specific Code Actions (Experimental)

*Note: this is not implemented yet in current version.*

You could configure `tinymist.onCodeAction` to provide package-specific code actions. It will be executed when requesting code actions in the editing typst document.

For example, matching a table element and providing a code lens to open it in a Microsoft Excel:

````typc
{
  code-lens(
    if: ```typc is-func and func.name == "table"```,
    title: "Open in Excel",
    command: "myExtension.openTableInExcel",
    arguments: (sys.inputs，),
  )
}
````

