## Context

Tinymist reads its settings from three client deliveries, and each used a different entry point:

| Delivery | Entry point | Namespace |
| --- | --- | --- |
| `initializationOptions` | `Config::update` | unpacked |
| `workspace/didChangeConfiguration` (payload) | `Config::update_by_map` | ignored |
| `workspace/configuration` (pull) | `Config::values_to_map` + `Config::update_by_map` | ignored |

`update_by_map` is the only function that assigns fields: it looks each setting up by key and runs it through `try_deserialize!`. Unpacking the namespace before that single lookup point makes all three paths agree, and needs no change to the assignment code or to `ConstConfig` handling.

## Goals / Non-Goals

- Goals: one namespace semantic for every delivery path; the documented precedence preserved; no behavior change for `null` or for deserialization failures.
- Non-Goals: reworking `Config`'s structure or the `CONFIG_ITEMS` list; changing the set of asked sections; changing editor frontends.

## Decisions

### Unpack the namespace inside `update_by_map`, not at each call site

Both the notification path and the pull path funnel into `update_by_map`, so a namespace-aware `update_by_map` covers them at once, and `Config::update` simply delegates to it. The alternative — teaching `on_changed_configuration` to call `Config::update` — would leave `update_by_map`'s semantics unchanged, but it would not fix the pull path, which reaches `update_by_map` through `values_to_map`, not through `update`. Since the pull path must be fixed for a client that never pushes settings, the shared entry point is `update_by_map`.

The namespaced object is merged over the top level, which is the precedence the existing comment in `Config::update` already declares ("Configurations in the tinymist namespace take precedence"). Merging rather than replacing keeps a setting that the payload carries only at the top level.

### Merge the pull answers before `update_by_map`, not inside it

In the pull path the server asks for both the `tinymist.<key>` and the `<key>` section of every setting, so `update_by_map` receives an answer for each. A client that answers the `tinymist` section as well returns a namespaced object there, which is the only answer a pure eglot client gives.

The three answers are merged in `values_to_map` with the ladder

1. `<key>` answer,
2. overridden by the `tinymist` object,
3. overridden by the `tinymist.<key>` answer.

Step 3 is the delicate one. A `tinymist.<key>` answer is not interchangeable with the same setting inside the `tinymist` object: VS Code substitutes editor variables (and validates `fontPaths`) in the per-section answer only. Letting the object win would regress `fontPaths` substitution for a client that answers both. The per-key answer is also the more specific one, so it keeps precedence, and the object fills only the settings the client left unanswered.

Merging in `values_to_map` rather than inside `update_by_map` keeps a single place where the flattened shape is produced: `update_by_map` always receives a flat map, whether it came from a notification, from initialization options, or from a pull.

### Error and null behavior

`try_deserialize!` is untouched. A `null` value is still skipped without a warning, and a value that fails to deserialize still lands in `self.warnings` and the `show_config_warnings` channel; the config is never silently rewritten on failure. `on_changed_configuration` still rolls back to `old_config` and returns `invalid_params` when `update_by_map` returns an error.

## Risks / Trade-offs

- A payload in which the client intends the top-level key to override a nested namespaced key now keeps the nested one. That is the documented behavior of the initialization path and of the specification being added here, so the other paths converge on it rather than the other way around.
- `values_to_map` no longer preserves the entire raw answer as `"tinymist"` in the returned map. `update_by_map` never read that key, so no behavior is lost; the settings it carries are now reachable under their own names.

## Migration Plan

None required: the change makes the notification and pull paths behave like `initializationOptions` already did, so clients that relied on the working path are unaffected and clients that used the namespace stop seeing their settings dropped.

## Open Questions

None.
