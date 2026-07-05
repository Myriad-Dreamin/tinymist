## ADDED Requirements

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
