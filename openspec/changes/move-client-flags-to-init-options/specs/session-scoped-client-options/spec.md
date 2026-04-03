## ADDED Requirements

### Requirement: Session-scoped client options are established during initialization

Tinymist SHALL derive session-scoped client options from the LSP `initialize` request and keep those values stable for the lifetime of that server session. Session-scoped options include client capability flags and notification opt-ins that are not ordinary hot-reloadable workspace settings, including `compileStatus`, `triggerSuggest`, `triggerParameterHints`, `triggerSuggestAndParameterHints`, `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`.

#### Scenario: Client-side code-lens handling is chosen at initialize time

- **WHEN** the client initializes Tinymist with `supportClientCodelens` set to `false`
- **THEN** Tinymist returns the direct export command code lens for the session instead of client-handled `tinymist.runCodeLens` entries

#### Scenario: Initialize-time client flags survive later config refreshes

- **WHEN** Tinymist starts with any session-scoped client option enabled in `initializationOptions`
- **AND** the client later sends or is polled for runtime workspace configuration that omits that key or returns `null`
- **THEN** Tinymist keeps the initialize-time session value instead of resetting it to a default

### Requirement: Runtime workspace configuration does not mutate session-scoped client options

Tinymist SHALL exclude session-scoped client options from normal workspace-configuration synchronization and SHALL NOT treat `workspace/didChangeConfiguration` as authoritative for those options after initialization.

#### Scenario: Workspace configuration polling excludes session-scoped keys

- **WHEN** Tinymist requests runtime workspace configuration from a client after initialization
- **THEN** the request only includes ordinary workspace settings
- **AND** it does not request sections for session-scoped client options such as `supportClientCodelens` or `compileStatus`

#### Scenario: Runtime notifications do not rewrite session options

- **WHEN** a client sends `workspace/didChangeConfiguration` containing a session-scoped key such as `compileStatus`
- **THEN** Tinymist ignores that key for the current session
- **AND** existing session-scoped behavior remains unchanged until the next `initialize`

### Requirement: Compile-status emission is a session-scoped opt-in

Tinymist SHALL use the initialize-time `compileStatus` option to decide whether to emit `tinymist/compileStatus` notifications for the current LSP session.

#### Scenario: Explicit opt-in enables notifications

- **WHEN** the client initializes Tinymist with `compileStatus` set to `"enable"`
- **THEN** Tinymist emits `tinymist/compileStatus` notifications for primary-project status updates during that session

#### Scenario: Omitted or disabled opt-in suppresses notifications

- **WHEN** the client initializes Tinymist without `compileStatus` or with `compileStatus` set to `"disable"`
- **THEN** Tinymist does not emit `tinymist/compileStatus` notifications during that session

#### Scenario: Mid-session setting changes wait for a new session

- **WHEN** a user changes a client-side setting that affects `compileStatus` after Tinymist has already initialized
- **THEN** Tinymist keeps the current compile-status notification mode for the rest of that session
- **AND** the changed value only takes effect after the client starts a new Tinymist session
