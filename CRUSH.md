## Tinymist Development Guide

This document provides a summary of conventions and commands for working in the tinymist repository.

### Commands

- **Format all code:** `yarn fmt`
- **Build server:** `cargo build`
- **Build VS Code Extension:** `cd editors/vscode && yarn build`
- **Run server tests:** `cargo test --workspace -- --skip=e2e`
- **Run a single test:** `cargo test --workspace -- <TEST_NAME> --skip=e2e`
- **Lint server:** `cargo clippy --workspace --all-targets`
- **Lint VS Code extension:** `yarn lint`

### Code Style

- **General**: Follow Rust and JavaScript best practices. Maintain existing code structure.
- **Commits**: Run `yarn fmt` before each commit.
- **PR Titles**: Use conventional commit prefixes (e.g., `feat:`, `fix:`, `docs:`).
- **Testing**: Write unit tests for new functionality, preferably snapshot-based.
- **Documentation**: Document public APIs and complex logic.
- **Error Handling**: Follow existing patterns for error handling in Rust (`anyhow`) and TypeScript.
- **Imports**: Keep imports organized, following existing conventions.
- **Naming**: Use `snake_case` for Rust variables/functions and `camelCase` for TypeScript.

### Repository Structure

- `crates/`: Rust crates for the language server.
- `editors/vscode/`: VS Code extension.
- `editors/neovim/`: Neovim plugin.
- `docs/`: Project documentation.
- `tests/`: Integration tests.
