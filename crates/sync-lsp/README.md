# sync-ls

Sync LSP server inspired by async-lsp, primarily for tinymist. The author of this crate thinks that async-lsp is better than sync-ls, so please use async-lsp whenever possible unless you have a good reason to use sync-ls. Some random points:

- The `req_queue` and `transport` are extracted from the rust-analyzer project.
- The sync-ls should have better performance on stdio transport than async-lsp, especially on windows, but the author have forgotten the idea.
- The sync-ls handlers can get a mutable reference to the state, which is not possible in `tower-lsp`.
- The sync-ls supports both LSP and DAP with a common codebase.

## Debugging with input mirroring

You can record the input during running the editors with binary. You can then replay the input to debug the language server.

```sh
# Record the input
your-ls --mirror input.txt
# Replay the input
your-ls --replay input.txt
```

This is much more useful when devloping a dap server.

## Usage

Starts a LSP server with stdio transport:

```rust
with_stdio_transport::<LspMessage>(args.mirror.clone(), |conn| {
    let client = LspClientRoot::new(tokio_handle, conn.sender);
    LspBuilder::new(args, client.weak())
        // Adds request handlers
        .with_request::<Shutdown>(State::shutdown)
        // Adds event handlers
        .with_event(&LspInterrupt::Settle, State::interrupt)
        .build()
        .start(conn.receiver, is_replay)
})?;
```
