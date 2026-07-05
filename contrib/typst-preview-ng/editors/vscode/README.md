# Typst Preview NG

Experimental Tinymist previewer provider that hosts a rewritten client-side preview frontend.

The extension leaves the Tinymist preview server unchanged. Tinymist still owns preview task
creation and injects the data-plane URL into this provider's HTML, while this frontend moves the
WebSocket connection into a web worker.

Enable it from VS Code settings:

```json
{
  "tinymist.previewer": "myriad-dreamin.typst-preview-ng"
}
```

The client connects to the Tinymist preview data-plane from a worker and renders pages with
typst.ts on worker-owned canvas resources.
