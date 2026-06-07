## ADDED Requirements

### Requirement: Extension previewers declare supported targets

Extension-based previewer providers MAY declare `supportedTargets` as a list of preview targets containing `paged`, `html`, or both. Tinymist SHALL treat omitted `supportedTargets` as support for both preview targets. Tinymist SHALL fall back to the built-in previewer when a configured extension previewer does not support the current `tinymist.exportTarget`.

#### Scenario: Paged-only provider handles paged target

- **WHEN** `tinymist.exportTarget` is `paged`
- **AND** a trusted workspace configures a compatible extension previewer with `supportedTargets` containing `paged`
- **THEN** Tinymist uses that extension previewer

#### Scenario: Paged-only provider falls back for HTML target

- **WHEN** `tinymist.exportTarget` is `html`
- **AND** a trusted workspace configures a compatible extension previewer with `supportedTargets` containing `paged` but not `html`
- **THEN** Tinymist reports the target mismatch
- **AND** Tinymist uses its built-in previewer

#### Scenario: Provider without target declaration remains compatible

- **WHEN** a trusted workspace configures a compatible extension previewer without `supportedTargets`
- **THEN** Tinymist treats the extension previewer as supporting both `paged` and `html`

### Requirement: Document preview handlers receive the current target

Tinymist SHALL include the selected preview target in the document preview task passed to extension previewer `handlePreview` handlers.

#### Scenario: Handler receives HTML target

- **WHEN** `tinymist.exportTarget` is `html`
- **AND** Tinymist calls a compatible extension previewer's `handlePreview` handler
- **THEN** the task includes `target` set to `html`

#### Scenario: Handler receives paged target

- **WHEN** `tinymist.exportTarget` is `paged`
- **AND** Tinymist calls a compatible extension previewer's `handlePreview` handler
- **THEN** the task includes `target` set to `paged`
