# WASM and Browser Integration Rules

## WASM Build Process

- Always use `wasm-pack build --target bundler` for building WASM modules
- Check TypeScript definitions after building to ensure they match the Rust implementation
- Maintain minimal dependencies in WASM crates to ensure compatibility

## Monaco Editor Integration

- Follow the Monaco language client protocols for all editor integrations
- Use web workers for running language server to avoid blocking the main thread
- Keep LSP implementation consistent with server-side implementation where possible

## Implementation Best Practices

- Start with minimal implementations for browser/WASM targets and expand gradually
- Use js-sys for JavaScript interoperability in WASM modules
- Use explicit return types in TypeScript definitions for better type safety
- Maintain backward compatibility with Monaco Editor versions
