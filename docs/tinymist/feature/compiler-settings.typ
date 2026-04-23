#import "mod.typ": *

#show: book-page.with(title: [Compiler Settings])

#let font-inventory = json("/docs/tinymist/generated/compiler-settings-fonts.json")

Tinymist uses an embedded Typst compiler for editing, previewing, and exporting. This document guides you on how to configure fonts, packages, roots, or other supported compiler arguments.

For a guided VS Code walkthrough, see #cross-link("/frontend/vscode.typ", [VS Code frontend]). For the generated settings reference, see #cross-link("/config/vscode.typ", [Configuration Reference]).

= Reproducible Font Setup

If you want preview and export output to stay stable across machines, start by turning off host font discovery and pointing Tinymist at explicit font paths:

```json
{
  "tinymist.systemFonts": false,
  "tinymist.fontPaths": [
    "${workspaceFolder}/fonts"
  ]
}
```

With `tinymist.systemFonts` set to `false`, Tinymist stops scanning the operating system's font directories. That removes host-specific surprises and makes the compiler rely on the fonts you intentionally provide.

`tinymist.fontPaths` accepts files or directories. Relative paths are resolved from the Typst root directory, while VS Code also lets you use variables such as `${workspaceFolder}` before the LSP sees the final path.

#note-box[
  Dedicated settings take precedence over overlapping `tinymist.typstExtraArgs` entries.
  `tinymist.systemFonts` overrides `--ignore-system-fonts`.
  `tinymist.fontPaths` overrides `--font-path`.
  `tinymist.rootPath` overrides `--root`.
]

= Embedded Fonts

Tinymist always keeps the embedded `typst-assets` font bundle available, even when `tinymist.systemFonts` is `false`. The current docs inventory is generated from `typst-assets` v#font-inventory.typstAssets.version, so it tracks the same source Tinymist resolves at runtime.

The bundled fonts are:

#for font in font-inventory.fonts [
  - *#font.displayName* (#raw(font.fileName))
]

== Emoji Fonts

The official Typst app experience additionally bundles the extra Twitter emoji font. Tinymist's binary does not embed that extra emoji font, so emoji rendering can differ unless you add one yourself.

To get similar emoji coverage, place an emoji font in a tracked directory and add it through `tinymist.fontPaths` or `--font-path`. For example:

```jsonc
{
  "tinymist.systemFonts": false,
  "tinymist.fontPaths": [
    // For example, place a noto color emoji font in this directory.
    "${workspaceFolder}/fonts/NotoColorEmoji.ttf",
    "${workspaceFolder}/fonts"
  ]
}
```

= Packages, Roots, and Certificates

Preview and export use the same package search paths, package cache, root directory, and certificate settings as the editor.

Use the dedicated `tinymist.rootPath` setting when you want the editor to own the Typst root, and use `tinymist.typstExtraArgs` for the CLI-shaped package and certificate options that Tinymist parses today:

```json
{
  "tinymist.rootPath": "/abs/path/to/workspace",
  "tinymist.typstExtraArgs": [
    "--package-path", "/abs/path/to/workspace/.typst/packages",
    "--package-cache-path", "/abs/path/to/workspace/.cache/typst/packages",
    "--cert", "/abs/path/to/workspace/certs/internal-ca.pem"
  ]
}
```

If you commit local packages or private certificates alongside your project, pairing them with an explicit root and explicit font paths makes preview and export behavior much easier to reproduce on CI and on teammates' machines.

= Supported `tinymist.typstExtraArgs`

There is a *global* configuration `tinymist.typstExtraArgs` to pass extra arguments to tinymist LSP, like what you usually do with `typst-cli` CLI. For example, you can set it to `["--input=awa=1", "--input=abaaba=2", "main.typ"]` to configure `sys.inputs` and entry for compiler, which is equivalent to make LSP run like a `typst-cli` with such arguments:

```
typst watch --input=awa=1 --input=abaaba=2 main.typ
```

Tinymist parses a supported subset of Typst CLI arguments instead of acting as an unrestricted passthrough. Use the CLI spelling in `tinymist.typstExtraArgs`, but keep the list to the options Tinymist understands today:

Supported arguments:
- entry file: The last positional argument string in the array will be treated as the entry file.
  - This is used to specify the *default* entry file for the compiler, which may be overridden by other settings.
- `--input=key=value` adds values to `sys.inputs`.
- `--features=<feature>` (or `--features`, `<feature>`) adds Typst features to the compiler, for example `--features=html`.
- `--root` sets the Typst project root.
- `--font-path` adds explicit font paths.
- `--ignore-system-fonts` disables system-font discovery.
- `--package-path` and `--package-cache-path` control where Typst looks for packages.
- `--creation-timestamp` sets a reproducible document timestamp.
- `--cert` points Typst at a CA certificate file for network package access.

Example:

```json
{
  "tinymist.typstExtraArgs": [
    "--input=mode=print",
    "--features=html",
    "--root", "/abs/path/to/workspace",
    "--font-path", "/abs/path/to/workspace/fonts",
    "--ignore-system-fonts",
    "--package-path", "/abs/path/to/workspace/.typst/packages",
    "--package-cache-path", "/abs/path/to/workspace/.cache/typst/packages",
    "--creation-timestamp", "1735689600",
    "--cert", "/abs/path/to/workspace/certs/internal-ca.pem",
    "main.typ"
  ]
}
```

*Note:* Fix entry to `main.typ` may help multiple-file projects
but you may lose diagnostics and autocompletions in unrelated files.

*Note:* Please use `tinymist.typstExtraArgs` for the remaining CLI-shaped inputs only if you don't find a dedicated tinymist setting. For example, `--root` has a corresponding dedicated `tinymist.rootPath` setting.
