## Overview

The GPU viewer provider already receives `handlePreview(task)` after Tinymist has started the preview server. The layout helper will run from the provider extension after spawning `tinymist-viewer`, using the spawned process id and the known viewer window title (`Tinymist View`) to find the native viewer window.

This keeps window automation outside Tinymist's core VS Code extension and outside the Rust viewer rendering loop. Tinymist continues to manage preview lifecycle and data-plane communication; the provider owns the optional desktop side effect.

## Configuration

`tinymist.gpuViewer.windowLayout`:

- `sideBySide`: move VS Code to the left half of the primary work area and the viewer to the right half.
- `disabled`: do not move any windows.

The default is `sideBySide` for this provider because the native viewer opens outside VS Code and the expected workflow is source on the left with preview on the right. Users can set `disabled` on tiling window managers, remote sessions, or multi-monitor setups where desktop window movement is unwanted.

## Platform Helpers

### Windows

The extension invokes PowerShell with embedded C# P/Invoke declarations for `user32.dll`. The helper enumerates top-level visible windows, finds a VS Code window by process name (`Code`, `Code - Insiders`, or `VSCodium`) and the viewer window by the spawned process id, then calls `ShowWindowAsync` and `MoveWindow`.

The helper uses the primary screen work area via `System.Windows.Forms.Screen.PrimaryScreen.WorkingArea` to avoid covering the taskbar.

### macOS

The extension invokes `osascript`. AppleScript uses System Events to find the Code process and `Tinymist View`, then sets the front window positions and sizes to the visible desktop bounds. This requires the usual macOS Accessibility permission for the controlling application. If permission is missing, the helper fails softly and logs the error.

### Linux

The extension uses `wmctrl` when available. It queries the desktop work area and visible windows, finds VS Code and the viewer process/window, then moves them with `wmctrl -ir ... -e`. This is expected to work on X11/EWMH window managers. Wayland compositors commonly block global window management; in that case the provider logs that layout was skipped.

## Error Handling

Window layout is best-effort. It must not fail preview launch or kill the viewer. The provider logs the helper command failure, stderr, or timeout to the GPU viewer output channel.

## Lifecycle

The layout helper runs once shortly after spawn. It does not continuously enforce layout and does not watch for later monitor changes. Disposal remains unchanged: Tinymist preview task disposal kills the viewer process.

## Alternatives Considered

- Embedding the native viewer into a VS Code webview: not viable because VS Code cannot host arbitrary native windows in editor tabs.
- Moving window layout into `tinymist-viewer`: the viewer can control its own window size, but it cannot reliably move VS Code's window across all platforms.
- Shipping separate native helper binaries: more robust long-term, but increases packaging complexity. Script-backed helpers are sufficient for an opt-in first implementation.
