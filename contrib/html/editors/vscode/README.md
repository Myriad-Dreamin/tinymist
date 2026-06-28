# Tinymist Typst HTML

Tinymist Typst HTML is a companion VS Code extension for Tinymist. It adds HTML
and CSS language features for Typst documents that embed HTML or CSS in raw
blocks.

The extension activates for Typst files, asks Tinymist for the code context at
the cursor, and forwards completion requests in embedded HTML/CSS regions to
VS Code language services. It also provides CSS class completions for Typst
string attributes that look like HTML `class` attributes.

## Usage

Install both extensions:

- `myriad-dreamin.tinymist`
- `myriad-dreamin.tinymist-vscode-html`

Open a Typst document and edit embedded HTML or CSS raw blocks. CSS class
completion data is refreshed when CSS files are saved.

## Development

From the repository root:

```sh
yarn workspace tinymist-vscode-html compile
yarn workspace tinymist-vscode-html check
yarn workspace tinymist-vscode-html test
```

## License

This extension is licensed under Apache-2.0. Additional notices for derived CSS
support code live in `src/css/LICENSE`.
