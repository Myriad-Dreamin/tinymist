# Typlite

Converts a subset of typst to markdown.

## Usage

```shell
# default output is main.md
typlite main.typ
# specify output
typlite main.typ output.md
# specify --assets-path to make contexual block exported to external directory
typlite README.typ --assets-path assets
# specify --assets-src-path to make contexual block's source code exported to external directory
typlite README.typ --assets-path assets --assets-src-path assets
```

## Feature

- **Raw Output**: Raw codes with `typlite` language will be directly output into the Markdown result.
