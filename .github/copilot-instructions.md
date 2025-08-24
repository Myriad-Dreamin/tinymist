This is a Rust+JavaScript repository. It builds:
- A Rust binary serves language features:
  - `lsp`: Runs language server
  - `dap`: Runs debug adapter
  - `preview`: Runs preview server
- The JavaScript VS Code extension.
- The lua plugin for Neovim.

It is primarily responsible for providing integrated typst language service to various editors like VS Code, Neovim, Emacs, and Zed. Please follow these guidelines when contributing:

## Specialized Instructions

- **Localization**: See [copilot-instructions-l10n.md](./copilot-instructions-l10n.md) for comprehensive guidance on adding, updating, and maintaining localization in the project.

## Code Standards

### Keep Good PR Title

Determine a good PR prefix **only** by the PR description before work. Add a prefix to indicate what kind of release this pull request corresponds to. For reference, see https://www.conventionalcommits.org/

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

## Development Guidelines

### `tools/editor-tools`

The frontend-side and backend-side can be developed independently. For example, a data object passed from backend to frontend can be coded as `van.state<T>` as follows:

- Intermediate arguments:

  ```ts
  const documentMetricsData = `:[[preview:DocumentMetrics]]:`;
  const docMetrics = van.state<DocumentMetrics>(
    documentMetricsData.startsWith(":") ? DOC_MOCK : JSON.parse(base64Decode(documentMetricsData)),
  );
  ```

- Server-pushing arguments (e.g. `programTrace` in `tools/editor-tools/src/vscode.ts`):

  ```ts
  export const programTrace = van.state<TraceReport | undefined>(undefined /* init value */);

  export function setupVscodeChannel() {
    if (vscodeAPI?.postMessage) {
      // Handle messages sent from the extension to the webview
      window.addEventListener("message", (event: any) => {
        switch (event.data.type) {
          case "traceData": {
            programTrace.val = event.data.data;
            break;
          }
          // other cases
        }
      });
    }
  }
  ```

- Tool request arguments (e.g. `requestSaveFontsExportConfigure` in `tools/editor-tools/src/vscode.ts`):

  ```ts
  export function requestSaveFontsExportConfigure(data: fontsExportConfigure) {
    if (vscodeAPI?.postMessage) {
      vscodeAPI.postMessage({ type: "saveFontsExportConfigure", data });
    }
  }
  ```

`DOC_MOCK` is a mock data object for the frontend to display so that the frontend can be developed directly with `yarn dev`.
