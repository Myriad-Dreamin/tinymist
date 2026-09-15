## Why

Tinymist accepts settings in two shapes: flat (`{"exportTarget": "bundle"}`) and under the `tinymist` namespace (`{"tinymist": {"exportTarget": "bundle"}}`). Only the `initializationOptions` path honoured the namespace: it routed through `Config::update`, which unpacks `update["tinymist"]` in addition to the top-level keys.

The other two delivery paths looked only at the top level, so every namespaced setting silently reverted to its default there:

- `workspace/didChangeConfiguration` → `on_changed_configuration` → `Config::update_by_map`.
- `workspace/configuration` → `Config::values_to_map` → `Config::update_by_map`, where the answer for the requested `tinymist` section is a namespaced object as well.

`update_by_map` looks up each setting by key at the top level only, and `assign_config!` falls back to `unwrap_or_default()` on a miss, so nothing is reported: the user sees the configuration accepted and having no effect. The `tinymist` namespace is the documented form for clients and the only form that survives a `workspace/configuration` pull, so a client that does not push settings (a pure eglot client) is affected on every pull. This is issue `#2714`, reported for users in `#2701`. The VS Code client was unaffected because it delivers flattened keys through `initializationOptions`, which is why this went unnoticed.

## What Changes

- Make `Config::update_by_map` namespace-aware so that all configuration delivery paths agree on what a `tinymist` key means, and `Config::update` reuse it instead of unpacking the namespace itself.
- Unpack the namespaced object answered for the whole `tinymist` section in `Config::values_to_map`, so the pull path covers a client that answers only that section.
- Keep the namespaced key authoritative over the same key outside of the namespace, on every path. In the pull path, an answer for a single `tinymist.<key>` section stays authoritative over the whole `tinymist` section, because the client resolved it for that key and some clients normalize it there only (VS Code substitutes editor variables in `tinymist.<key>` answers but not inside the `tinymist` object).
- Keep the existing `null` tolerance and the deserialization-warning channel unchanged, including for namespaced settings.
- Add regression coverage for the namespace on the notification path, the pull path, and the existing precedence and `null` behaviors.

## Capabilities

### New Capabilities
- `tinymist-config-namespace`: The `tinymist` configuration namespace keeps one meaning across `initializationOptions`, `workspace/didChangeConfiguration`, and `workspace/configuration`, and its settings take precedence over the same settings outside the namespace.

### Modified Capabilities

None.

## Impact

- `crates/tinymist/src/config.rs`: `Config::update`, `Config::update_by_map`, `Config::values_to_map`, plus regression tests.
- No editor frontend, CLI, or configuration schema change: the intended contract was already documented, so this only makes the three paths obey it.
