## Overview

The current viewer already knows the preview content area size through `resize_observer`, which is enough for page fit. The visible gap comes from fitting pages to `window_width - 0.5`; this was intended to avoid a stray scrollbar but now creates a persistent half-pixel inset in side-by-side layouts.

Window state reporting needs lower-level window events than the Xilem app-state callback exposes. Xilem supports an external event-loop integration, so `tinymist-viewer` can wrap the normal Masonry/Xilem driver in a small `ApplicationHandler` that observes winit `Moved` and `Resized` events before delegating them to Masonry.

## Window State

The viewer does not own durable storage. The launching client owns persistence and passes any restored geometry back to the viewer on the next launch.

Viewer startup accepts:

- `--initial-window-inner-size WIDTHxHEIGHT`
- `--initial-window-position X,Y`

The native viewer already connects to tinymist server's preview data-plane websocket. After accepted move/resize events, it sends one text command over that existing connection:

```text
viewer-window-state {"schema_version":1,"window":{"inner_width":1280,"inner_height":900,"outer_x":24,"outer_y":48}}
```

Event payload:

- physical inner size from `WindowEvent::Resized`
- physical outer position from `WindowEvent::Moved`
- `schema_version: 1`

Restore:

- initial inner size through `WindowOptions::with_initial_inner_size`
- initial position through `WindowOptions::with_initial_position`

Tinymist server receives the data-plane text command in `WebviewActor`, forwards it to the internal preview editor actor, and then sends a `tinymist/preview/windowState` LSP notification to the editor client. The VS Code preview integration stores schema-versioned records in `ExtensionContext.globalState` under a versioned key and passes the restored initial state to the configured previewer on the next launch. This avoids inventing cross-process file locking in the viewer and lets multiple same-version viewer processes converge through the client's storage semantics. Multiple viewer/client versions are isolated by `schema_version` and the versioned storage key; an incompatible future schema should use a new key or ignore mismatched records.

Neovim and other clients can implement their own persistence later by passing the same initial geometry arguments and consuming the server-forwarded window-state notification.

## Side-by-side Startup

When `tinymist.gpuViewer.windowLayout` is `sideBySide`, side-by-side layout is the launch strategy. The VS Code provider should try to compute the right-half work-area rectangle and move the VS Code window to the left half before spawning `tinymist-viewer`. The computed right-half rectangle is passed as the viewer's initial size and position so the operating system can create the viewer close to the final side-by-side layout.

Stored window state must not suppress the side-by-side script, because viewer window-state persistence will normally produce a stored record after the first launch. In side-by-side mode, stored state is only a fallback if pre-layout fails before spawn. When `tinymist.gpuViewer.windowLayout` is `disabled`, stored state is the primary initial geometry.

The existing post-spawn arrangement remains as a repair pass in side-by-side mode because native window managers can apply decorations, minimum sizes, DPI conversions, and placement policies differently from the requested initial geometry. If pre-layout fails, the provider falls back to stored state or default placement and still performs the post-spawn repair pass.

## Fit Width

`fitted_page_width` should return the finite available viewport width directly. This removes the fractional blank column while keeping invalid sizes clamped to at least one logical pixel.

If future floating-point rounding causes a stray scrollbar, that should be handled in the scroll container's overflow tolerance rather than by shrinking the rendered page.

## Lifecycle

Window state messages are sent immediately after accepted move/resize events so process termination from the preview provider still leaves a recent state in client storage. Very small resize values are ignored to avoid persisting minimized or transient zero-sized states.
