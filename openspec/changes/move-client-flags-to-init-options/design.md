## Context

Tinymist currently has one broad configuration parsing path:

- `Config::extract_lsp_params` reads `initializationOptions` during `initialize`
- `workspace/didChangeConfiguration` notifications and `workspace/configuration` polling later feed back into `Config::update_by_map`

That shared path is useful and should stay. Issue `#2390` highlights that options such as `supportClientCodelens` and `compileStatus` can be editor-integration values rather than ordinary user workspace settings, but making them init-only would create a different problem: some clients can only expose these values through workspace configuration and configuration-change notifications.

The right boundary is therefore not a separate parse path. These options should remain configuration-wide, and the special handling should be limited to their apply semantics:

```text
initializationOptions
workspace/configuration
workspace/didChangeConfiguration
        │
        ▼
 unified Config parse/update
        │
        ▼
 effective Config snapshot
        │
        ▼
 diff by apply scope
        │
        ├─ hot-reload fields
        ├─ editor actor fields
        └─ restart-scoped client options -> reload projects
```

## Goals / Non-Goals

**Goals:**

- Keep editor-integration options in the normal configuration lifecycle.
- Add the missing client option keys to runtime configuration polling so refreshes provide complete effective values.
- Ensure configuration parsing updates these options from both initialization and runtime configuration sources.
- Reload projects when restart-scoped client options change.
- Ensure shipped editor integrations return injected client-capability values during runtime configuration polling.
- Add regression coverage for request lists, parsing, and restart-scope diffing.

**Non-Goals:**

- Move client options to an init-only protocol.
- Redesign every Tinymist config field.
- Expose non-user client capability flags as user settings.
- Fix the fallback `tinymist.exportPdf` relative-path error also mentioned in `#2390`.
- Define a new cross-editor protocol beyond LSP initialization options and workspace configuration.

## Decisions

### 1. Keep one configuration parse path

Tinymist should continue parsing these values through `Config::update_by_map`. Initialization options are an early configuration source, not a separate lifetime class. Runtime configuration responses and complete runtime configuration notifications should be able to update the same fields.

This avoids per-key preservation logic such as "keep the initialize value if runtime config omits this key." Instead, the configuration source is responsible for returning the complete effective value for keys Tinymist requests.

### 2. Poll the complete client option set

Runtime configuration polling should include the editor-integration options that Tinymist parses today:

- `compileStatus`
- `triggerSuggest`
- `triggerParameterHints`
- `triggerSuggestAndParameterHints`
- `supportHtmlInMarkdown`
- `supportClientCodelens`
- `supportExtendedCodeAction`
- `customizedShowDocument`
- `delegateFsRequests`

This makes parsing deterministic: when Tinymist asks for configuration, it asks for all fields needed to build the effective config snapshot.

### 3. Apply these options through a project restart boundary

Some consumers of these values are project-facing or actor-facing and are not clean hot-reload fields. Tinymist should compare a focused restart-scoped client option snapshot after config parsing and call `reload_projects` when it changes.

This keeps the parse model ordinary while making the application boundary explicit.

### 4. Editor integrations must return injected capability values

The VS Code extension injects capability values such as `supportClientCodelens` and `triggerSuggest` into the initialization config object. Those keys are not normal user-facing VS Code settings, so a plain `workspace/configuration` lookup can return `null` or `undefined` for them.

The extension's configuration middleware should fill those requested sections from the injected config object when VS Code has no user setting value. User-facing settings such as `compileStatus` should remain driven by the normal VS Code configuration result.

## Risks / Trade-offs

- [Third-party clients may send partial `didChangeConfiguration` objects] -> Tinymist's direct notification path continues to expect an effective configuration object. Clients that cannot provide one should send an empty or non-object settings payload so Tinymist polls `workspace/configuration`.
- [Project reloads may be broader than strictly necessary] -> The restart-scoped option set is intentionally focused on editor-integration values where a consistent boundary is more important than micro-optimizing hot updates.
- [Injected client flags are not user settings] -> Keep them out of user-facing config surfaces and only synthesize them inside editor integration configuration responses.

## Migration Plan

1. Update the OpenSpec capability and tasks from init-only session options to configuration-wide restart-scoped client options.
2. Add missing editor-integration keys to `Config::get_items` polling.
3. Add a restart-scoped client option diff in server config application and reload projects when it changes.
4. Update VS Code configuration middleware to return injected capability values for requested sections.
5. Add focused regression tests for config polling, parsing updates, and VS Code response synthesis.
