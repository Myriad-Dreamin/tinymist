This is a Rust+JavaScript repository. It builds:
- A Rust binary serves language features:
  - `lsp`: Runs language server
  - `dap`: Runs debug adapter
  - `preview`: Runs preview server
- The JavaScript VS Code extension.
- The lua plugin for Neovim.

It is primarily responsible for providing integrated typst language service to various editors like VS Code, Neovim, Emacs, and Zed. Please follow these guidelines when contributing:

## Code Standards

### Keep Good PR Title

Add a prefix to indicate what kind of release this pull request corresponds to. For reference, see https://www.conventionalcommits.org/

Available types:
 - dev
 - feat
 - fix
 - docs
 - style
 - refactor
 - perf
 - test
 - build
 - ci
 - chore
 - revert

### Required Before Each Commit
- Run `yarn fmt` to format Rust/JavaScript files
- This will run formatters on all necessary files to maintain consistent style

### Development Flow
- Build Server: `cargo build`
- Build VS Code Extension: `cd editors/vscode && yarn build`
- Full CI check: `cargo clippy --workspace --all-targets`
- Test Server: `cargo test --workspace -- --skip=e2e`
    Note that, in the envoironment where network is not available (copilot or nix actions), we should also skip following tests:
    ```
    completion::tests::test_pkgs
    docs::package::tests::cetz
    docs::package::tests::fletcher
    docs::package::tests::tidy
    docs::package::tests::touying
    ```

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
