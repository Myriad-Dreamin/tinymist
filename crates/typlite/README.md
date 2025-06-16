# Typlite

Converts a subset of typst to markdown.

## Installation

```
cargo install typlite
```

## Usage

```shell
# default output is main.md
typlite main.typ
# specify output
typlite main.typ output.md
```

Supported format:

- `output.txt`: Plain text
- `output.md`: Markdown
- `output.tex`: LaTeX
- `output.docx`: Word

Todo: We may support custom format by typst scripting in future, like:

```shell
# specify output
typlite main.typ --post-process @preview/typlite-mdx output.mdx
```

## Feature

- **Contexual Content Rendering**: Contents begin with `context` keyword will be rendered as svg output. The svg output will be embedded inline in the output file as **base64** by default, if the `--assets-path` parameter is not specified. Otherwise, the svg output will be saved in the specified folder and the path will be embedded in the output file.

## Typlite-Specific `sys.inputs`

The `sys.input.x-target` can be used distinguish with normal HTML export.

```typ
#let x-target = sys.inputs.at("x-target", default: "pdf")

#let my-function = if x-target == "md" {
  md-impl
} else {
  pdf-impl or html-impl
}
```
