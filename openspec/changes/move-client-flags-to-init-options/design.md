## Context

Tinymist's server configuration currently has one broad parsing path:

- `Config::extract_lsp_params` reads `initializationOptions` during `initialize`
- later `workspace/didChangeConfiguration` notifications and `workspace/configuration` polling feed back into the same `Config::update_by_map` logic

That shared path is appropriate for real workspace settings such as `exportPdf`, `semanticTokens`, or formatter options. It is not a good fit for client-session metadata that is injected by the editor integration at startup and is not meant to be requested from workspace settings afterward.

Issue `#2390` highlights the problem with `supportClientCodelens` and `compileStatus`, but the current runtime config list also includes other initialize-time client flags:

- `triggerSuggest`
- `triggerParameterHints`
- `triggerSuggestAndParameterHints`
- `supportHtmlInMarkdown`
- `supportClientCodelens`
- `supportExtendedCodeAction`
- `customizedShowDocument`
- `delegateFsRequests`

When a client later refreshes configuration, missing or `null` values for those keys can fall back to defaults and silently change server behavior mid-session. That means code-lens handling, completion callbacks, markdown rendering expectations, and other client-coupled behavior can drift away from the values the client declared during `initialize`.

## Goals / Non-Goals

**Goals:**

- Separate session-scoped client options from hot-reloadable workspace config.
- Make the initialize-time values for client capability flags stable for the lifetime of an LSP session.
- Treat `compileStatus` as a session-scoped opt-in that is declared during initialization rather than polled as ordinary workspace config.
- Prevent runtime configuration refreshes from clearing or mutating initialize-time client flags.
- Add regression coverage for both initialize payloads and later config-refresh behavior.

**Non-Goals:**

- Redesign every Tinymist config field or rename all existing options.
- Change unrelated runtime settings such as formatter, export, preview, lint, or semantic token behavior.
- Fix the fallback `tinymist.exportPdf` relative-path error also mentioned in `#2390`.
- Define a brand-new cross-editor config protocol beyond what Tinymist already passes in `initializationOptions`.

## Decisions

### 1. Split session-scoped client options from runtime workspace config

Tinymist should model initialize-time client metadata separately from ordinary workspace settings, even if both are still represented inside the same top-level `initializationOptions` JSON object on the wire. The important distinction is lifecycle: session-scoped options are captured once during `initialize` and then treated as immutable for that server session.

Alternative considered:

- Keep one config bucket and only special-case `supportClientCodelens`. Rejected because `#2390` exposes a broader lifecycle problem and the same config-refresh bug already applies to other injected client flags.

### 2. Stop requesting session-scoped keys through `workspace/configuration`

Runtime config polling should only ask the client for settings that are truly workspace configuration. Session-scoped keys should be removed from the request list and ignored if a client still includes them in `didChangeConfiguration`.

This change should cover at least:

- `compileStatus`
- `triggerSuggest`
- `triggerParameterHints`
- `triggerSuggestAndParameterHints`
- `supportHtmlInMarkdown`
- `supportClientCodelens`
- `supportExtendedCodeAction`
- `customizedShowDocument`
- `delegateFsRequests`

Alternative considered:

- Continue requesting the keys but preserve previous values when the response is `null`. Rejected because it keeps the protocol boundary ambiguous and still treats client metadata as if it were workspace config.

### 3. Treat `compileStatus` as a session-scoped opt-in

`compileStatus` controls whether Tinymist sends `tinymist/compileStatus` notifications and performs related status-word-count integration. That behavior should be decided for the current LSP session during initialization and remain stable until the next `initialize`.

Clients that expose `compileStatus` as a user-facing setting may still let users change it, but the effect should be applied by starting a new server session rather than by mutating the running server through `workspace/didChangeConfiguration`.

Alternative considered:

- Keep `compileStatus` hot-reloadable while moving only `supportClientCodelens` to initialize-time config. Rejected because `#2390` explicitly calls out `compileStatus` as the same kind of session property, and changing it live would preserve the current split-brain config model.

### 4. Lock the lifecycle with focused config and LSP tests

The change should add or adjust tests that prove:

- initialize-time session options are parsed from `initializationOptions`
- later config refreshes do not overwrite those options
- `supportClientCodelens` keeps the same code-lens mode across config reloads
- `compileStatus` keeps the same notification behavior for the whole session

Alternative considered:

- Rely on existing smoke coverage only. Rejected because the bug is specifically about the interaction between initialization and later config refreshes.

## Risks / Trade-offs

- [Users may expect `compileStatus` changes to apply instantly] -> Mitigate with clear docs and client behavior that treats the setting as restart-scoped instead of silently pretending it is hot-reloadable.
- [Some third-party clients may currently send these keys via runtime config only] -> Mitigate by keeping initialize-time defaults conservative and updating shipped integrations and docs to show the intended contract.
- [Separating session and runtime config could touch several code paths] -> Mitigate by keeping the split narrow and test-driven around the request list, config parsing, and the specific behaviors from `#2390`.

## Migration Plan

1. Introduce a dedicated session-scoped config path for initialize-time client options.
2. Remove session-scoped keys from runtime config polling and ignore them during `didChangeConfiguration`.
3. Update shipped editor integrations and initialization fixtures to send the session-scoped values through `initializationOptions`.
4. Add regression tests around config refreshes so initialize-time client flags stay stable.
5. Update docs to distinguish session-scoped options from normal workspace settings, especially for `compileStatus`.
