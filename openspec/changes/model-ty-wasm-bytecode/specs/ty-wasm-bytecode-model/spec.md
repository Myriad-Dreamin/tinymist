## ADDED Requirements

### Requirement: Type bytecode model
The system SHALL define an internal bytecode representation for type deduction programs.

#### Scenario: Compile expression to bytecode
- **WHEN** the checker compiles a supported expression for type deduction
- **THEN** it produces a bytecode program containing deterministic instructions for constants, locals, globals, closures, calls, selection, conditionals, binary operations, and returns

### Requirement: Semantic type value domain
The system SHALL define semantic type values for computed types, function closures, arguments, records, arrays, tuples, meta variables, and neutral residual operations.

#### Scenario: Represent a stuck call
- **WHEN** evaluation reaches a call whose callee or result cannot be reduced because of a meta, unknown variable, or recursive running closure
- **THEN** the VM represents the result as a neutral call value instead of blocking or immediately erasing it to `Any`

### Requirement: Quote semantic values to Ty
The system SHALL quote semantic type values back to existing `Ty` values for storage in `TypeInfo`, signatures, docs, and snapshots.

#### Scenario: Quote public output
- **WHEN** a bytecode evaluation result is recorded in `TypeInfo`
- **THEN** the result is converted to `Ty` without exposing VM-only semantic value types through public analysis APIs

### Requirement: WebAssembly emission contract
The system SHALL define a WebAssembly emission contract for type bytecode that uses handles and host functions for type values.

#### Scenario: Emit wasm-compatible bytecode
- **WHEN** a supported type bytecode program is emitted to WebAssembly
- **THEN** the emitted module uses host calls and numeric handles rather than embedding Rust `Ty` memory layout in wasm memory
