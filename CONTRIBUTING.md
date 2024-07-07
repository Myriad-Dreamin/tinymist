
# Contributing

Tinymist provides a single integrated language service for Typst.

**Multiple Actors** – The main component, [tinymist](./crates/tinymist/), starts as a thread or process, obeying the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/). tinymist will bootstrap multiple actors, each of which provides some typst feature.

**Multi-level Analysis** – The most critical features are lsp functions, built on the [tinymist-query](./crates/tinymist-query/) crate. To achieve low latency, functions are classified into different levels of analysis.
+ `query_token_cache` – `TokenRequest` – locks and accesses token cache.
+ `query_source` – `SyntaxRequest` – locks and accesses a single source unit.
+ `query_world` – `SemanticRequest` – locks and accesses multiple source units.
+ `query_state` – `StatefulRequest` – acquires to accesses a specific version of compile results.

**Optional Features** – All rest features in tinymist are optional. The significant features are enabled by default, but you can disable them with feature flags. For example, `tinymist` provides preview server features powered by `typst-preview`.

**Editor Frontends** – Leveraging the interface of LSP, tinymist provides frontends to each editor, located in the [editor folder](./editors).

## Building and Running

To build tinymist LSP:

```bash
git clone https://github.com/Myriad-Dreamin/tinymist.git
# Debug
cargo build
# Release
cargo build --release
# RelWithDebInfo (GitHub Release)
cargo build --profile=gh-release
```

To run VS Code extension locally, open the repository in VS Code and press `F5` to start a debug session to extension.

## Server Entries

- `tinymist probe` – do nothing, which just probes that the binary is working.
- `tinymist lsp` – starts the language server.
- `tinymist preview` – starts a standalone preview server.

## Running Analyzer Tests

This is required if you have changed any code in `crates/tinymist-query`.

To run analyzer tests for tinymist:

```bash
cargo insta test -p tinymist-query --accept
```

## Running E2E Tests

This is required if you have changed any code in `crates/tinymist` or `crates/tinymist-query`.

To run e2e tests for tinymist on Unix systems:

```bash
./scripts/e2e.sh
```

To run e2e tests for tinymist on Windows:

```bash
./scripts/e2e.ps1
```
