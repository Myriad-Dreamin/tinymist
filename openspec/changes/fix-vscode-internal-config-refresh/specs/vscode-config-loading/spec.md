## ADDED Requirements

### Requirement: Runtime configuration refresh preserves extension-managed internal settings
The Tinymist VS Code extension SHALL attach its extension-managed internal settings to every configuration payload delivered to the language server after startup. This SHALL preserve the same internal capability contract used during initialization, including the completion trigger flags, `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and the current `delegateFsRequests` value.

#### Scenario: Refresh payload keeps extension-managed capability flags
- **WHEN** Tinymist synchronizes workspace configuration to the language server after a VS Code settings change during an active session
- **THEN** the payload still includes the extension-managed internal settings that were attached during startup
- **AND** the server does not lose those internal capability flags only because the configuration was refreshed

#### Scenario: Unrelated user setting change does not clear internal settings
- **WHEN** a user changes a normal Tinymist workspace setting such as `tinymist.preview` or `tinymist.fontPaths`
- **THEN** Tinymist forwards the updated user setting to the language server
- **AND** the same refresh keeps the extension-managed internal settings intact

#### Scenario: Runtime refresh preserves existing user-setting normalization
- **WHEN** a runtime configuration refresh includes a valid `tinymist.fontPaths` array with VS Code variables such as `${workspaceFolder}/fonts`
- **THEN** Tinymist still expands the variables before forwarding the effective `fontPaths` value
- **AND** the refresh also includes the extension-managed internal settings required by the session
