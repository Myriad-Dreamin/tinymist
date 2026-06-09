## 1. Resolve module-valued field-access sources for item imports

- [ ] 1.1 Extend the non-wildcard import source resolution path so `check_import` recognizes field-access expressions that resolve to modules.
- [ ] 1.2 Feed the resolved module scope into `import_decls` for item imports such as `#import foo.bar: baz` while keeping unsupported non-module expressions failing closed.
- [ ] 1.3 Verify existing wildcard field-access imports continue to work through their current resolution path.

## 2. Add regression coverage

- [ ] 2.1 Add an `expr_of` or equivalent scope fixture covering `#import module.field: item1, item2` so the imported bindings appear in static analysis output.
- [ ] 2.2 Add an editor-facing regression fixture, such as hover or goto-definition, showing a name imported from a field-access source resolves to the exported definition.
- [ ] 2.3 Add regression coverage for a nested import item path from a field-access source plus a negative case where the source does not resolve to a module.

## 3. Validate the change

- [ ] 3.1 Run focused `tinymist-query` import-analysis and editor-feature tests that exercise the new field-access item import coverage and review the resulting snapshots.
