
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

## Build and run

To build tinymist LSP:

```bash
git clone https://github.com/Myriad-Dreamin/tinymist.git
cargo build
```

To run VS Code extension locally, open the repository in VS Code and press `F5` to start a debug session to extension.
