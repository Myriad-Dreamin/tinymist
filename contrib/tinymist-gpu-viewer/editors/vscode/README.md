# Tinymist GPU Viewer

Tinymist GPU Viewer is a previewer-provider extension for Tinymist. It launches the native GPU previewer from `crates/tinymist-viewer` through Tinymist's `tinymist.previewer` extension-provider contract.

## Usage

Install both extensions:

- `myriad-dreamin.tinymist`
- `myriad-dreamin.tinymist-gpu-viewer`

Then configure Tinymist:

```json
{
  "tinymist.previewer": "myriad-dreamin.tinymist-gpu-viewer"
}
```

Tinymist will activate this extension, start the regular preview server, and pass the preview data-plane websocket address to the native `tinymist-viewer` process. No VS Code webview preview panel is created for this provider.

In VS Code Remote windows, Tinymist may run in the remote extension host while Tinymist GPU Viewer runs in the local UI extension host. The provider uses a command bridge in that case, and Tinymist passes a VS Code-forwarded websocket URL to the local native viewer. Install Tinymist in the remote workspace and Tinymist GPU Viewer in the local/UI side.

For local development, the repository debug task builds the native viewer and copies the debug binary into this extension's `bin/` directory before launching the Extension Development Host:

```sh
cargo build --bin tinymist-viewer
```

The provider version is expected to match the Tinymist extension version because the preview websocket protocol is versioned with Tinymist.

If the executable is not bundled and cannot be found on PATH, configure:

```json
{
  "tinymist.gpuViewer.executable": "/path/to/tinymist-viewer"
}
```

## Window layout

By default the provider tries to place VS Code on the left and the native viewer on the right after starting preview. To disable this desktop window automation, configure:

```json
{
  "tinymist.gpuViewer.windowLayout": "disabled"
}
```

Window layout is best-effort and does not affect the preview task if the operating system blocks it. Check the `Tinymist GPU Viewer` output channel for layout diagnostics.

- Windows uses PowerShell with Win32 window APIs.
- macOS uses AppleScript through `osascript` and may require Accessibility permission.
- Linux uses `wmctrl`; it works on many X11/EWMH window managers and is commonly blocked or unsupported on Wayland.
