## ADDED Requirements

### Requirement: Trusted workspaces can override preview HTML through a single previewer setting
The VS Code Tinymist extension SHALL allow trusted workspaces to override the default preview HTML by setting `tinymist.previewer`.
#### Scenario: HTML previewer path is used
- **WHEN** a trusted workspace sets `tinymist.previewer` to `html:<path>` where `<path>` resolves to a readable HTML file
- **THEN** Tinymist uses that HTML file as the preview webview content instead of the built-in preview HTML

#### Scenario: Default previewer id uses built-in HTML
- **WHEN** a workspace sets `tinymist.previewer` to `myriad-dreamin.tinymist`
- **THEN** Tinymist uses its built-in preview HTML without resolving the value as an external extension provider

#### Scenario: Invalid previewer falls back to built-in HTML
- **WHEN** the configured `tinymist.previewer` value cannot be resolved to a readable HTML file or a usable extension provider
- **THEN** Tinymist reports the loading problem and uses its built-in preview HTML

### Requirement: Provider values without the html prefix are interpreted as extension ids
The VS Code Tinymist extension SHALL interpret non-empty `tinymist.previewer` values other than `myriad-dreamin.tinymist` that do not start with `html:` as extension ids for previewer providers.

#### Scenario: Extension id provider is used
- **WHEN** a trusted workspace configures `tinymist.previewer` with an installed and compatible extension id
- **THEN** Tinymist activates that extension and uses the provider-supplied HTML path for preview

#### Scenario: Missing extension id reports an error
- **WHEN** `tinymist.previewer` is set to an extension id that is not installed or does not expose the required provider contract
- **THEN** Tinymist reports the problem and fails previewer resolution without using its built-in preview HTML

### Requirement: HTML previewers use the typst preview frontend contract
The VS Code Tinymist extension SHALL require `html:` previewer targets to resolve to an HTML entry file compatible with `tools/typst-preview-frontend`.

#### Scenario: Compatible HTML previewer is accepted
- **WHEN** `tinymist.previewer` resolves through `html:<path>` to an HTML entry file that follows the preview frontend contract expected by Tinymist
- **THEN** Tinymist loads that HTML file as the preview frontend

#### Scenario: Incompatible HTML previewer falls back
- **WHEN** `tinymist.previewer` resolves through `html:<path>` to an HTML file that does not satisfy Tinymist's preview frontend expectations at runtime
- **THEN** Tinymist reports the problem and uses its built-in preview HTML

### Requirement: Provider compatibility is enforced before use
Tinymist SHALL verify that a configured extension-based previewer provider is compatible with the running Tinymist extension before using provider-supplied HTML. If the provider does not supply a custom compatibility predicate, Tinymist SHALL require the provider's declared Tinymist version to exactly match the running Tinymist version.

#### Scenario: Default compatibility accepts exact version match
- **WHEN** a configured extension-based provider declares a Tinymist version equal to the running Tinymist version and does not define a custom compatibility predicate
- **THEN** Tinymist accepts the provider previewer

#### Scenario: Default compatibility rejects mismatched version
- **WHEN** a configured extension-based provider declares a Tinymist version different from the running Tinymist version and does not define a custom compatibility predicate
- **THEN** Tinymist rejects the provider previewer and uses its built-in preview HTML

#### Scenario: Custom compatibility check rejects provider
- **WHEN** a configured extension-based provider defines a custom compatibility predicate and that predicate returns false for the running Tinymist version
- **THEN** Tinymist rejects the provider previewer and uses its built-in preview HTML

### Requirement: Untrusted workspaces ignore previewer overrides
Tinymist MUST ignore `tinymist.previewer` in untrusted workspaces and MUST use its built-in preview HTML instead.

#### Scenario: Untrusted workspace ignores html previewer
- **WHEN** a workspace is untrusted and `tinymist.previewer` is configured as `html:<path>`
- **THEN** Tinymist ignores the provider setting and uses its built-in preview HTML

#### Scenario: Untrusted workspace ignores extension-id provider
- **WHEN** a workspace is untrusted and `tinymist.previewer` is configured as an extension id
- **THEN** Tinymist ignores the provider setting and uses its built-in preview HTML
