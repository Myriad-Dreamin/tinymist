# tinymist-monaco

This package integrates the Tinymist typst language server with Monaco Editor, providing a rich editing experience for Typst documents in the browser.

## Features

- Syntax highlighting for Typst files
- Autocompletion for Typst syntax and commands
- Hover information for Typst elements
- Document symbols
- More features coming soon!

## Installation

```bash
npm install tinymist-monaco monaco-editor
```

## Usage

```typescript
import { createMonacoLanguageClient } from 'tinymist-monaco';
import TinymistWorker from 'tinymist-monaco/worker?worker';

// Create a Monaco editor instance
const editor = monaco.editor.create(document.getElementById('editor'), {
    value: 'Hello #strong[world]!',
    language: 'typst'
});

// Create the worker and language client
const worker = new TinymistWorker();
const languageClient = createMonacoLanguageClient(worker);

// Start the language client
languageClient.start();
```

## Implementation Details

This package contains:
- A TypeScript wrapper around the Monaco editor integration
- A WebAssembly module with the Tinymist language server

The package provides a WebAssembly implementation of the Language Server Protocol (LSP) that runs directly in the browser. This enables features like code completion, hover information, and document symbols without requiring a backend server.

## Development

### Building

```bash
# Build the WASM module
wasm-pack build --target bundler crates/tinymist-wasm

# Build the TypeScript package
npm run build
```

### Extending

To add more LSP features:

1. Extend the Rust implementation in `src/lib.rs` with new methods
2. Update the worker.ts file to handle new LSP method requests
3. Rebuild using wasm-pack

## License

Apache-2.0
