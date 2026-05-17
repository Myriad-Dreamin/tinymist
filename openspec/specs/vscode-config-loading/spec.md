# vscode-config-loading Specification

## Purpose
The vscode-config-loading specification defines how the Tinymist VS Code extension loads and refreshes workspace configuration safely. It ensures malformed `tinymist.fontPaths` values do not crash extension activation or later configuration reads, preserves variable substitution for valid string arrays, requires actionable feedback when user configuration is invalid, and keeps extension-managed internal settings present during runtime refreshes.
## Requirements
### Requirement: VS Code activation tolerates malformed `tinymist.fontPaths`
The Tinymist VS Code extension SHALL validate `tinymist.fontPaths` before iterating or performing VS Code variable substitution. If the setting value is not an array of strings, the extension SHALL ignore that malformed value for the current configuration load and continue activation.

#### Scenario: Singleton string does not crash activation
- **WHEN** `tinymist.fontPaths` is configured as a string such as `"fonts"` during extension activation
- **THEN** Tinymist continues activation without throwing a `TypeError`
- **AND** Tinymist does not forward the malformed `fontPaths` value as a valid font-path list

#### Scenario: Mixed-type array does not crash activation
- **WHEN** `tinymist.fontPaths` is configured as an array containing any non-string item
- **THEN** Tinymist continues activation without throwing an exception
- **AND** Tinymist ignores the malformed `fontPaths` value for that configuration load

### Requirement: Tinymist shows an actionable error when `tinymist.fontPaths` is malformed
When Tinymist ignores a malformed `tinymist.fontPaths` value, the VS Code extension SHALL show an actionable error message that names the setting and explains that the expected shape is an array of strings.

#### Scenario: Error message explains the expected JSON shape
- **WHEN** Tinymist detects that `tinymist.fontPaths` is malformed
- **THEN** the error message mentions `tinymist.fontPaths`
- **AND** the error message explains that the setting must be an array of strings, such as `["fonts"]`

#### Scenario: Repeated reads do not spam the same error message
- **WHEN** the extension reads the same malformed `tinymist.fontPaths` setting multiple times during one VS Code session
- **THEN** Tinymist does not repeatedly show duplicate error messages for the same invalid setting shape

### Requirement: Valid `tinymist.fontPaths` arrays preserve variable substitution
For valid `tinymist.fontPaths` arrays, Tinymist SHALL preserve the existing behavior that expands VS Code variables for each configured path entry.

#### Scenario: Valid array entries are expanded
- **WHEN** `tinymist.fontPaths` is configured as an array of strings that includes VS Code variables such as `${workspaceFolder}/fonts`
- **THEN** Tinymist substitutes variables for each entry
- **AND** Tinymist forwards the expanded string array as the effective `fontPaths` value

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
