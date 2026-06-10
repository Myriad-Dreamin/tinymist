## Overview

The viewer owns its title bar inside the Xilem application tree and runs the native window without operating-system decorations. The title bar is a small custom Masonry widget that paints the document title and window controls, handles pointer interaction, and emits app-level actions such as toggling help.

The Xilem app is driven through an explicit `masonry_winit` event-loop wrapper. This lets the viewer observe each winit window id and install platform native behavior before events are delegated back to Masonry.

## Title Bar

The title bar renders:

- document title text, truncated to available width
- help, minimize, maximize, and close buttons
- hover and active button feedback
- an accessible title-bar role and label

The title-bar widget keeps all drawing local to `tinymist-viewer`; it does not add protocol messages or editor-side state. The help button toggles a Typst-rendered overlay with preview shortcuts. Close, minimize, maximize, drag, double-click maximize, and context-menu interactions use existing Masonry event APIs where available.

## Native Windows Behavior

Windows requires non-client hit testing for a decorationless window to feel native. The viewer installs a window subclass for live winit windows and promotes the title-bar drag area, resize edges, and maximize button to the matching Windows hit-test values.

The maximize button remains visible as a custom title-bar button, but Windows receives the native maximize hit so Windows 11 snap-layout hover and maximize behavior keep working. Mouse messages are bridged back to the client area so the custom button hover and active visuals remain in sync with native non-client interaction.

Non-Windows platforms keep the same Xilem title bar but use no-op native hit-test installation.

## Provider Window Matching

The GPU viewer provider passes `--document-title` based on the preview document path. The viewer window title becomes `<document> - Tinymist View`; connection status suffixes may be appended while reconnecting.

Side-by-side layout helpers search using the computed viewer title instead of a fixed `Tinymist View` string so post-launch arrangement still finds the native viewer window.

## Viewport Chrome

The scrollable preview viewport paints the same dark background as the title bar and uses subtler overlay scrollbar colors. The body column gap is set to zero so the custom title bar and preview viewport meet without a visible seam.
