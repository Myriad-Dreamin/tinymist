# WebAssembly Development Rules

## WASM Implementation Guidelines

1. When implementing the WASM language server, use `todo!()` for methods that are not yet implemented
2. Add all available LSP methods defined in `query.rs` to the WASM language server implementation
3. Ensure TypeScript wrapper code (worker.ts) properly handles errors from WASM method calls

## WASM Platform Compatibility

1. WASM doesn't support file locking - provide no-op implementations that return `Ok(())`
2. WASM doesn't support symlinks - fall back to copying files/directories
3. WASM doesn't support hard links - fall back to copying files  
4. WASM doesn't support blocking network operations - use async APIs or feature-gate blocking code
5. Always provide WASM-specific implementations in platform-conditional compilation blocks

## Monaco Integration Guidelines 

1. Focus on Monaco editor implementation rather than VS Code specific code
2. Use LSP protocol standards for communication between the language client and server
3. Follow the `monaco-languageclient` API patterns for integration

## Build Process

1. Build the WASM module using `wasm-pack build --target bundler crates/tinymist-wasm`
2. Always update TypeScript definitions after modifying the Rust implementation

## Error Handling

1. Always wrap WASM method calls in try/catch blocks in the worker.ts file
2. Log errors to console when they occur for easier debugging
3. Return sensible defaults when errors occur to prevent breaking the UI
