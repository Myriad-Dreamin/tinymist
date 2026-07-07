## 1. Extend semantic reference matching

- [ ] 1.1 Extend shared parameter reference lookup so it collects named argument label spans from direct calls that bind to the selected user-defined parameter.
- [ ] 1.2 Extend the same lookup to `.with(...)` named arguments while excluding unrelated same-name parameters on other callables.
- [ ] 1.3 Ensure the returned rename ranges cover only the label token for matched named arguments.

## 2. Wire LSP behavior through the shared path

- [ ] 2.1 Keep `textDocument/references` and `textDocument/rename` on the shared reference path so both features surface the new named-argument matches.
- [ ] 2.2 Verify unsupported rename targets, such as field accesses and builtin/native parameters, keep their current behavior.

## 3. Add regression coverage

- [ ] 3.1 Add reference and rename fixtures for direct named arguments on user-defined functions.
- [ ] 3.2 Add reference and rename fixtures for `.with(...)` named arguments plus a non-match case for another callable with the same parameter name.
- [ ] 3.3 Run focused `tinymist-query` reference and rename tests and review the snapshot output.
