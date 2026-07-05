## 1. Bytecode Model

- [x] 1.1 Add internal bytecode data structures for programs, instructions, constants, and closure prototypes.
- [x] 1.2 Add semantic value and neutral residual data structures with stable debug formatting for snapshots.
- [x] 1.3 Add quote support from semantic values to existing `Ty`.

## 2. Compiler

- [x] 2.1 Compile supported expression nodes into type bytecode programs.
- [x] 2.2 Compile function definitions into closure prototypes with captured scope metadata and return metas.
- [x] 2.3 Add unit tests for bytecode generation on calls, selection, binary operations, conditionals, and recursive functions.

## 3. WebAssembly Contract

- [x] 3.1 Define the wasm host ABI for value, args, env, string, meta, and closure handles.
- [x] 3.2 Add an experimental wasm emitter that produces modules for a small supported bytecode subset.
- [x] 3.3 Add validation tests that compare emitted wasm structure against bytecode programs without executing Wasmer.
