## ADDED Requirements

### Requirement: Named argument labels participate in parameter references
Tinymist SHALL treat a named argument label that semantically binds to a user-defined parameter as a reference to that parameter for `textDocument/references` and `textDocument/rename`.

#### Scenario: Direct named call is included
- **WHEN** a user requests references or rename on a user-defined parameter such as `side` and a call site invokes the same callable with a named argument like `bubble(side: left)`
- **THEN** the result includes the `side` label at that direct call site

#### Scenario: `.with(...)` named argument is included
- **WHEN** a user requests references or rename on a user-defined parameter such as `side` and the parameter is supplied through `bubble.with(side: left)`
- **THEN** the result includes the `side` label inside the `.with(...)` call

#### Scenario: Rename edits only the label token
- **WHEN** rename updates a named argument label that binds to the selected parameter
- **THEN** only the label text is replaced and the colon, whitespace, and value expression remain unchanged

#### Scenario: Unrelated same-name labels are excluded
- **WHEN** another callable has a distinct parameter with the same name as the selected parameter
- **THEN** references and rename for the selected parameter MUST NOT include the unrelated named argument label
