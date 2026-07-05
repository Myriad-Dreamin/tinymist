# GPU Viewer Debug Workspace

This workspace is configured to use the Tinymist GPU Viewer previewer provider:

```json
{
  "tinymist.previewer": "myriad-dreamin.tinymist-gpu-viewer"
}
```

From the repository root, start the `Run Extension [GPU Viewer]` debug configuration. It opens this folder in the Extension Development Host and loads both development extensions:

- `editors/vscode`
- `contrib/tinymist-gpu-viewer/editors/vscode`

Open `main.typ`, then run the `typst-preview.preview` command or use the preview editor title action. The preview should open in a native Tinymist GPU Viewer window, not in a VS Code webview panel.
