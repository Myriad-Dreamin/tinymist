# sync-lsp

Sync LSP server inspired by async-lsp, primarily for tinymist. The author of this crate thinks that async-lsp is better than sync-lsp, so please use async-lsp whenever possible unless you have a good reason to use sync-lsp. Some random points:

- The `req_queue` and `transport` are extracted from the rust-analyzer project.
- The sync-lsp should have better performance on stdio transport than async-lsp, especially on windows, but the author have forgotten the idea.
