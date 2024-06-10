# [Typst Preview VSCode](https://github.com/Enter-tainer/typst-preview)

Preview your Typst files in vscode instantly!

## Features

- Low latency preview: preview your document instantly on type. The incremental rendering technique makes the preview latency as low as possible.
- Open in browser: open the preview in browser, so you put it in another monitor. https://github.com/typst/typst/issues/1344
- Cross jump between code and preview: We implement SyncTeX-like feature for typst-preview. You can now click on the preview panel to jump to the corresponding code location, and vice versa.

Install this extension from [marketplace](https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview), open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

You can also use the shortcut (Ctrl+K V).

![demo](demo.png)

For more information, please visit documentation at [Typst Preview Book](https://enter-tainer.github.io/typst-preview/).

## Extension Settings

See https://enter-tainer.github.io/typst-preview/config.html

## Bug report

To achieve high performance instant preview, we use a **different rendering backend** from official typst. We are making our best effort to keep the rendering result consistent with official typst. We have set up comprehensive tests to ensure the consistency of the rendering result. But we cannot guarantee that the rendering result is the same in all cases. There can be unknown corner cases that we haven't covered.

**Therefore, if you encounter any rendering issue, please report it to this repo other than official typst repo.**
## Known Issues

See [issues](https://github.com/Enter-tainer/typst-preview/issues?q=is%3Aissue+is%3Aopen+sort%3Aupdated-desc) on GitHub.

## Legal

This project is not affiliated with, created by, or endorsed by Typst the brand.

## Change Log

See [CHANGELOG.md](CHANGELOG.md)
