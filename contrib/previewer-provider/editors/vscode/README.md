# Tinymist Previewer Provider Fixture

Tinymist Previewer Provider Fixture is a minimal VS Code extension used by
Tinymist integration tests. It exercises the `tinymist.previewer`
extension-provider contract without depending on a full preview frontend.

When activated, the extension exports `providePreviewer()` and returns
`previewer/index.html` as the preview HTML. The fixture reports compatibility
with the installed Tinymist extension version so tests can verify provider
resolution and preview HTML loading.

This package is a test fixture, not an end-user extension.

## Development

The fixture is compiled as part of the Tinymist VS Code integration test suite:

```sh
yarn workspace tinymist test:vsc
```

To compile only the fixture:

```sh
yarn tsc -p contrib/previewer-provider/editors/vscode/tsconfig.json
```

## License

This fixture is licensed under Apache-2.0.
