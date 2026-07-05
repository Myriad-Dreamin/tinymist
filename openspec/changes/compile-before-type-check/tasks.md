## 1. Pipeline Integration

- [x] 1.1 Add compile-before-check entry points in the type checker.
- [x] 1.2 Route supported expressions through bytecode evaluation for deduced types.
- [x] 1.3 Preserve `TypeInfo` population by quoting semantic values back to `Ty`.

## 2. Function Calls

- [x] 2.1 Replace deferred function body vectors with closure state in the VM path.
- [x] 2.2 Force non-running closures on demand at call sites.
- [x] 2.3 Residualize recursive or blocked calls as neutral values.

## 3. Checking Semantics

- [x] 3.1 Move binary operation compatibility and folding into VM primitives.
- [x] 3.2 Move selection and apply resultant logic into VM primitives.
- [x] 3.3 Keep experimental warnings once-only and non-user-facing.

## 4. Queries and Snapshots

- [x] 4.1 Make `precise_sig_of_def` force closure results through the VM.
- [x] 4.2 Add fixtures covering non-recursive helpers, recursive calls, binary folding, and documentation signatures.
- [x] 4.3 Run and review `tinymist-query` type-check snapshots for stronger/weaker changes.
