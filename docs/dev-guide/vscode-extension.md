# VS Code Extension

To run VS Code extension locally, open the repository in VS Code and press `F5` to start a debug session to extension. The VS Code extension also shows how we build and run the language server and the editor tools.

## Dev-Kit View

When the extension is run in development mode, a Dev-Kit View will be shown in the sidebar. It contains some useful commands to help you develop the extension.

- Runs Preview Dev: Runs Preview in Developing Mode. It sets data plane port to the fix default value (23625).
  - This is helpful when you are developing the preview feature. Goto `tools/typst-preview-frontend` and start a preview frontend with `yarn dev`.
- Runs Default Preview: Runs Default Preview, which is not enabled in VS Code but used in other editors.

## APPENDIX: @Myriad-Dreamin's VS Code Settings

Applies the workspace settings template:

```
cp .vscode/tinymist.code-workspace.tmpl.json .vscode/tinymist.code-workspace.json
```

And then open the workspace in VS Code.

Rust Settings explained:

This configuration enables clippy on save:

```json
{
		"rust-analyzer.check.command": "clippy",
}
```

This configuration wraps comments automatically:

```json
{
		"rust-analyzer.rustfmt.extraArgs": ["--config=wrap_comments=true"],
}
```

This configuration excludes the `target` folder from the file watcher:

```json
{
  "files.watcherExclude": {
    "**/target": true
  },
}
```

Typst Settings explained:

This configuration help use the same fonts as the CI building tinymist docs:

```json
{
  "tinymist.fontPaths": [
    "assets/fonts"
  ],
}
```

Formatter Settings explained:

This configuration runs formatters on save and using the `prettier` formatter:

```json
{
  "[javascript]":{
    "editor.formatOnSave": true,
    "editor.defaultFormatter": "esbenp.prettier-vscode",
  },
  "[json]": {
    "editor.formatOnSave": true,
    "editor.defaultFormatter": "esbenp.prettier-vscode"
  },
}
```
