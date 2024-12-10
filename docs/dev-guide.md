# Development Guide

Tinymist provides a single integrated language service for Typst. To get a quick overview of the crates and structure of the project take a look at [Overview of Service](https://myriad-dreamin.github.io/tinymist/overview.html).

**The Language Server** – The main component, [tinymist](./crates/tinymist/), starts as a thread or process, obeying the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

**The Analyzers** – The most critical features are lsp functions, built upon the [tinymist-query](../crates/tinymist-query/) crate.

**The Editor Frontend** – Leveraging the interface of LSP, tinymist provides frontend to each editor, located in the [editor folder](../editors).

**The builtin essential but optional features** – All rest features in tinymist are optional. The significant features are enabled by default, but you can disable them with feature flags. 
- The syntax highlighting feature powered by [textmate](../syntaxes/textmate/).
- The document formatting feature powered by [typstfmt](https://github.com/astrale-sharp/typstfmt) or [typstyle](https://github.com/Enter-tainer/typstyle).
- The document previewing feature powered by [`typst-preview`](../crates/typst-preview/).
- The handwritten-stroke recognizer powered by [Detypify](https://detypify.quarticcat.com/).

## Installing Toolchain

To contribute to tinymist, you need to install the following tools:

- [Cargo](https://doc.rust-lang.org/cargo/) to develop Rust [crates](../crates/).
- [Yarn](https://yarnpkg.com/) to develop [VS Code extension](../editors/vscode/) or [tools](../tools/).

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

To run VS Code extension locally, open the repository in VS Code and press `F5` to start a debug session to extension. The VS Code extension also shows how we build and run the language server and the editor tools.

## Local Documentation

To serve the documentation locally, run:

```bash
yarn docs
```

To generate and open crate documentation, run:

```bash
yarn docs:rs --open
```

> [!Tip]  
> Check [Shiroa](https://myriad-dreamin.github.io/shiroa/guide/installation.html) to install the `shiroa` command for documentation generation.

If you find something are not documented, please feel free to open an issue or pull request. There is also a tracking issue to improve the documentation, see [Request for Documentation (RFD)](https://github.com/Myriad-Dreamin/tinymist/issues/931).

## Running Analyzer Tests

This is required if you have changed any code in `crates/tinymist-query`.

To run analyzer tests for tinymist:

```bash
cargo insta test -p tinymist-query --accept
```

> [!Tip]  
> Check [Cargo Insta](https://insta.rs/docs/cli/) to learn and install the `insta` command.

## Running Syntax Grammar Tests

This is required if you are going to change the textmate grammar in `syntaxes/textmate`.

```bash
# in root
yarn test:grammar
# Or in syntaxes/textmate
cd syntaxes/textmate && yarn test
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