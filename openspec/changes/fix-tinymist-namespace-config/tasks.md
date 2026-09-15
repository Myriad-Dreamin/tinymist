## 1. Namespace-aware update path

- [x] 1.1 Unpack a `tinymist`-namespaced object in `Config::update_by_map`, with the namespaced key taking precedence over the top-level one.
- [x] 1.2 Reduce `Config::update` to the object check plus `update_by_map`, so initialization and runtime delivery share one semantic.
- [x] 1.3 Keep the existing `null` tolerance and the `warnings` / `show_config_warnings` channel for rejected values.

## 2. Pull path

- [x] 2.1 Merge the `tinymist.<key>`, `<key>`, and `tinymist` section answers in `Config::values_to_map` into the flat shape `update_by_map` reads.
- [x] 2.2 Keep an answer for a single `tinymist.<key>` section authoritative over the `tinymist` object, so per-key client normalization is not lost.
- [x] 2.3 Verify that an answer for the whole `tinymist` section alone reaches the configuration, which is the only answer a pure eglot client gives.

## 3. Regression coverage

- [x] 3.1 Assert that a namespaced payload applies through `update_by_map`, the entry point of `workspace/didChangeConfiguration`.
- [x] 3.2 Assert that a namespaced key wins over the same key at the top level, and that a top-level-only setting still applies.
- [x] 3.3 Assert that namespaced `null` values remain harmless and that an invalid namespaced value is reported as a warning.
- [x] 3.4 Assert that `values_to_map` unpacks the `tinymist` section answer and that per-key answers keep precedence.
