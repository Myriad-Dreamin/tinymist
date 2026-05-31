## Context

Tinymist's VS Code preview flow currently resolves HTML in one place: `getPreviewHtml` returns the built-in HTML from the language server in Tinymist mode or the bundled compat HTML in legacy mode. `openPreviewInWebView` and `ContentPreviewProvider` then assume the resource root is Tinymist's own frontend directory and rewrite `/typst-webview-assets` against that hardcoded location.

That structure works for the built-in preview, but it leaves no clean seam for loading preview HTML from another extension or a local file. It also caches preview HTML globally, which means source changes are easy to miss, and it has no workspace-trust guard around loading arbitrary local content.

The change spans runtime resolution, configuration schema, webview resource handling, and the VS Code test harness, so it benefits from an explicit design before implementation. This revision also simplifies the configuration surface so users only need to set one previewer string.

## Goals / Non-Goals

**Goals:**
- Allow trusted workspaces to override preview HTML with a single `tinymist.previewer` setting.
- Keep `myriad-dreamin.tinymist` as the default `tinymist.previewer` value and resolve it to Tinymist's built-in previewer.
- Treat `tinymist.previewer` values beginning with `html:` as filesystem paths to HTML files compatible with `tools/typst-preview-frontend`.
- Treat any other non-empty `tinymist.previewer` value as an extension id for an external previewer provider.
- Keep the built-in preview as the default and fallback source.
- Define a stable provider contract for VS Code extensions that want to supply preview HTML.
- Enforce provider compatibility with the running Tinymist extension, using exact version matching by default.
- Add repository-owned tests that exercise both `html:` and extension-id provider paths with a minimal extension under `contrib/`.

**Non-Goals:**
- Redesign the preview frontend protocol, websocket transport, or placeholder substitution model.
- Support remote URLs or arbitrary network-loaded previewers.
- Guarantee live hot reload of previewer assets without reopening the preview.
- Generalize the provider contract beyond the VS Code extension host in this change.

## Decisions

### 1. Introduce a single previewer resolver

Tinymist will introduce a resolver layer that decides which preview HTML source to use before any webview is created. The resolver returns:

- The resolved HTML string
- The URI of the HTML file that supplied it
- The local resource roots that the webview may access
- Source metadata for diagnostics and tests

Resolution order:

1. If the workspace is untrusted, always use the built-in Tinymist HTML.
2. If `tinymist.previewer` is unset, empty, or `myriad-dreamin.tinymist`, use the built-in Tinymist HTML.
3. If `tinymist.previewer` starts with `html:`, treat the suffix as a path to an HTML file and try to load it.
4. Otherwise, treat `tinymist.previewer` as an extension id and try to resolve an extension-based previewer provider.
5. If a configured provider cannot be read or fails compatibility checks, report the problem; `html:` previewers fall back to the built-in Tinymist HTML, while extension-id previewers fail previewer resolution.

Alternative considered:
- Keep separate settings for extension ids and HTML paths.
Why not:
- A single previewer string is easier to document, easier to inspect in tests, and avoids precedence rules between multiple knobs.

### 2. Use an `html:` prefix for local HTML previewers

`tinymist.previewer` values with the `html:` prefix will be interpreted as filesystem-backed HTML previewers. The suffix participates in VS Code variable substitution and must resolve to a readable HTML file. The target HTML must be compatible with the preview frontend contract used by `tools/typst-preview-frontend`.

To avoid multi-root ambiguity and accidental path drift, implementation should treat the substituted suffix as an absolute path requirement and surface a warning if it is not absolute or unreadable.

Alternative considered:
- Infer whether the string is a path or an extension id based on filesystem existence.
Why not:
- Prefix-based parsing is deterministic, avoids ambiguous values, and makes configuration/debugging clearer.

### 3. Use an extension export contract for extension-id providers

Provider values without the `html:` prefix will be treated as extension ids. The configured provider extension will be discovered with `vscode.extensions.getExtension(<id>)`. After activation, Tinymist will read an exported provider API instead of invoking a convention-based command.

Proposed shape:

```ts
export interface TinymistPreviewerProvider {
  providePreviewer(): Promise<TinymistPreviewer> | TinymistPreviewer;
}

export interface TinymistPreviewer {
  htmlPath: string;
  compatibleTinymistVersion: string;
  isCompatible?(
    tinymistVersion: string,
  ): Promise<boolean> | boolean;
}
```

Rules:

- `htmlPath` may be absolute or relative to the provider extension root. Relative paths are preferred for packaged extensions.
- If `isCompatible` is omitted, Tinymist performs the default compatibility check by requiring `compatibleTinymistVersion === currentTinymistVersion`.
- If `isCompatible` is present, Tinymist still requires `compatibleTinymistVersion` for diagnostics, but the provider callback decides whether the current Tinymist version is acceptable.

Alternative considered:
- A command-based contract such as `${extensionId}.provideTinymistPreviewer`.
Why not:
- Extension exports provide a typed API, clearer activation/error handling, and simpler test fixtures.

### 4. Use the resolved HTML location as the webview resource root

Preview webviews currently rewrite asset paths assuming Tinymist's own `out/frontend` directory. After this change, the preview renderer should use the selected HTML file's directory as the primary resource root, while keeping the existing websocket placeholder substitutions intact.

This preserves compatibility with:

- Tinymist's built-in preview frontend
- `html:` providers that point at HTML built against `tools/typst-preview-frontend`
- External provider extensions that ship the same asset layout
- Minimal self-contained HTML fixtures used in tests

Alternative considered:
- Require providers to return a fully inlined HTML string with no local assets.
Why not:
- That would make it harder to reuse the existing preview frontend build outputs and would make provider extensions awkward to package.

### 5. Make provider selection observable for tests

The current preview e2e coverage only verifies that preview tasks exist. To test this feature safely, Tinymist should expose source metadata through an internal inspection command so tests can assert whether preview used:

- The built-in previewer
- A configured extension-id provider
- A configured `html:` provider
- A fallback after an error

This avoids brittle DOM inspection of VS Code webviews while still proving that provider resolution works.

Alternative considered:
- Drive the webview DOM directly from tests.
Why not:
- VS Code's extension tests do not provide stable, low-friction DOM access to webview contents, and a metadata-based assertion is more robust.

### 6. Load the test provider as a second development extension

The hello-world provider extension will live under `contrib/` and be loaded during `editors/vscode` e2e runs as an additional `extensionDevelopmentPath`. The local `@vscode/test-electron` runner already supports `string[]` for this option, so the test harness can activate Tinymist and the fixture provider in the same VS Code instance.

This keeps the provider fixture in-repo, versioned with Tinymist, and easy to evolve with the contract.

## Risks / Trade-offs

- [Provider API drift] -> If the extension export contract changes without updating fixtures or third-party extensions, Tinymist must surface a clear warning and fall back to the built-in preview.
- [Absolute-only `html:` suffix is stricter than some users may expect] -> Document the intended use with `${workspaceFolder}`-style substitution in examples and diagnostics.
- [Cached HTML may hide configuration changes] -> Cache the resolved previewer by source key or clear caches when `tinymist.previewer` changes.
- [Loading arbitrary HTML increases attack surface] -> Honor overrides only in trusted workspaces and limit webview local resource roots to the resolved HTML directory and any required extension roots.
- [E2E tests still cannot inspect rendered DOM] -> Use internal source-inspection metadata and a very small provider fixture to validate behavior without UI scraping.

## Migration Plan

No migration is required for existing users because the default behavior remains Tinymist's built-in preview HTML.

Rollout steps:

1. Add `tinymist.previewer` and the new resolver behind the existing preview feature.
2. Keep built-in preview HTML as the fallback for all error paths.
3. Land the `contrib/` provider fixture and the accompanying tests in the same change.

Rollback is straightforward: removing or ignoring `tinymist.previewer` returns Tinymist to its current built-in HTML path.

## Open Questions

- Should the internal source-inspection command remain permanently available for regression tests, or should it be guarded more explicitly as a development-only command?
