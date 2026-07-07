# Tinymist Renderer Diff

Tinymist Renderer Diff is a Vite/React viewer for renderer comparison artifacts
produced by Tinymist renderer tests and CI.

The viewer loads `renderer-diff-*` GitHub Actions artifacts or local ZIP files
containing a `renderer-diff-manifest.json` file and the corresponding image
assets. It presents the available renderer groups, cases, status filters, and
side-by-side output so rendering changes can be inspected from a browser.

CI builds this package for GitHub Pages, where workflow summaries link to the
viewer with the current repository and action run prefilled.

## Development

From the repository root:

```sh
yarn workspace @tinymist/renderer-diff dev
yarn workspace @tinymist/renderer-diff build
```

## License

This tool is licensed under Apache-2.0.
