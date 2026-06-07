## Context

The VS Code extension resolves `tinymist.previewer` once before opening a webview or calling an extension-provided `handlePreview` function. The provider contract currently validates Tinymist version compatibility, then accepts any compatible provider as suitable for every preview target.

Tinymist already has a `tinymist.exportTarget` setting with `paged` and `html` values. Native previewers can be target-specific: for example, the GPU viewer currently consumes the paged preview protocol and should not take over HTML preview tasks.

## Goals / Non-Goals

**Goals:**

- Add a backwards-compatible target support declaration to extension previewers.
- Use the configured export target while resolving extension previewers.
- Fall back to the built-in previewer when a configured extension previewer does not support the current target.
- Include the current target in document preview tasks passed to provider handlers.

**Non-Goals:**

- Change the built-in preview frontend contract or static server protocol.
- Add target support declaration to `html:<path>` previewer settings.
- Implement link, HTML embedding, or accessibility semantics in the GPU/native viewer.

## Decisions

- Use `supportedTargets?: ("paged" | "html")[]` on `TinymistPreviewer`.
  - Rationale: a positive allow-list is compact, explicit, and easy to validate.
  - Alternative considered: `unsupportedTargets` or boolean maps. Those encode the current two-target model but become awkward if a third target is added later.

- Treat omitted `supportedTargets` as supporting both targets.
  - Rationale: existing providers remain compatible without code changes.
  - Alternative considered: require all providers to update. That would unnecessarily break third-party providers.

- Fall back only for unsupported target declarations.
  - Rationale: a provider explicitly saying "not this target" is a soft mismatch, unlike missing extensions or version incompatibility where the configured provider is unusable or unsafe.
  - Existing resolution errors for missing providers, incompatible versions, and malformed provider shape remain errors.

- Resolve against `tinymist.exportTarget`, defaulting unknown values to `paged`.
  - Rationale: VS Code package configuration already constrains the setting; the fallback handles manually edited invalid settings without making previewer resolution fragile.

## Risks / Trade-offs

- [A provider typo in `supportedTargets` may make a provider unexpectedly unsupported] -> Filter support to known target strings and test the current target explicitly.
- [Changing `tinymist.exportTarget` after the previewer cache is populated may reuse the wrong provider] -> Include target in the resolver cache key and invalidate the cache on export target configuration changes.
- [Provider handlers may need target context even when they support both targets] -> Add `target` to `TinymistPreviewTask` while keeping all existing task fields unchanged.
