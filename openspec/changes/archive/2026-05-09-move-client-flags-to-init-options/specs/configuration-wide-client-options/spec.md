## ADDED Requirements

### Requirement: Client options participate in normal configuration synchronization

Tinymist SHALL treat editor-integration options as configuration-wide values that can be supplied by LSP `initializationOptions`, `workspace/configuration` responses, and `workspace/didChangeConfiguration` notifications. Initialization options provide bootstrap values for the starting configuration, while later configuration synchronization can update the effective values.

Editor-integration options include `compileStatus`, `triggerSuggest`, `triggerParameterHints`, `triggerSuggestAndParameterHints`, `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`.

#### Scenario: Initialization options seed the starting client options

- **WHEN** a client initializes Tinymist with editor-integration options in `initializationOptions`
- **THEN** Tinymist parses those options through the same configuration model used for normal settings
- **AND** Tinymist uses those values as the effective configuration until later configuration synchronization updates them

#### Scenario: Runtime configuration refresh updates client options

- **WHEN** a client returns editor-integration options from `workspace/configuration`
- **THEN** Tinymist parses those values into the effective configuration
- **AND** the parsed values replace the previous effective values for those options

#### Scenario: Runtime configuration notifications update client options

- **WHEN** a client sends `workspace/didChangeConfiguration` with a complete effective configuration containing editor-integration options
- **THEN** Tinymist parses those options from the notification
- **AND** the parsed values replace the previous effective values for those options

### Requirement: Client option changes use a project restart boundary

Tinymist SHALL detect changes to editor-integration options that affect project-facing behavior and SHALL reload projects when those values change. This restart boundary applies to `compileStatus`, `triggerSuggest`, `triggerParameterHints`, `triggerSuggestAndParameterHints`, `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`.

#### Scenario: Code-lens client support changes restart projects

- **WHEN** the effective `supportClientCodelens` value changes during configuration synchronization
- **THEN** Tinymist reloads projects before continuing with the new code-lens behavior

#### Scenario: Compile-status opt-in changes restart projects

- **WHEN** the effective `compileStatus` value changes during configuration synchronization
- **THEN** Tinymist updates compile-status notification configuration
- **AND** Tinymist reloads projects so project status behavior uses the new opt-in consistently

#### Scenario: Delegated file-system request support changes restart projects

- **WHEN** the effective `delegateFsRequests` value changes during configuration synchronization
- **THEN** Tinymist discards cached access models that depend on that option
- **AND** Tinymist reloads projects using the new file-system access behavior

### Requirement: Editor integrations return effective client options during config sync

Shipped editor integrations SHALL return the effective editor-integration option values when Tinymist requests runtime configuration. Values injected by the editor integration during startup SHALL also be present in later `workspace/configuration` responses when Tinymist asks for those sections.

#### Scenario: Injected VS Code client flags are returned during configuration polling

- **WHEN** Tinymist requests a VS Code configuration section for an injected client flag such as `tinymist.supportClientCodelens`
- **THEN** the VS Code extension returns the current injected value for that flag when VS Code has no user setting for the section
- **AND** Tinymist receives a complete effective value instead of `null`

#### Scenario: User-facing config values remain authoritative

- **WHEN** Tinymist requests a user-facing configuration section such as `tinymist.compileStatus`
- **THEN** the editor returns the current user configuration value
- **AND** Tinymist applies it through the normal configuration update path
