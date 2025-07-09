This is a Rust+JavaScript repository. It builds:
- A Rust binary serves language features:
  - `lsp`: Runs language server
  - `dap`: Runs debug adapter
  - `preview`: Runs preview server
- The JavaScript VS Code extension.
- The lua plugin for Neovim.

It is primarily responsible for providing integrated typst language service to various editors like VS Code, Neovim, Emacs, and Zed. Please follow these guidelines when contributing:

## Code Standards

### Required Before Each Commit
- Run `yarn fmt` to format Rust/JavaScript files
- This will run formatters on all necessary files to maintain consistent style

### Development Flow
- Build Server: `cargo build`
- Build VS Code Extension: `cd editors/vscode && yarn build`
- Test Server: `cargo test --workspace -- --skip=e2e`
- Full CI check: `cargo clippy --workspace --all-targets`

## Repository Structure
- `crates/`: rust crates for the server and related functionality
- `editors/vscode/`: VS Code extension code
- `editors/neovim/`: Lua plugin for Neovim
- `tools/editor-tools`: utility GUI tools for typst
- `tools/typst-preview-frontend`: Preview GUI for typst
- `docs/`: documentation for the project
- `locales/`: localization files for the entire project
- `tests/`: integration tests for the server and editors
- `syntaxes/`: textmate syntax definitions for typst

## Key Guidelines
1. Follow Rust and JavaScript best practices and idiomatic patterns
2. Maintain existing code structure and organization
4. Write unit tests for new functionality. Use snapshot-based unit tests when possible.
5. Document public APIs and complex logic in code comments
