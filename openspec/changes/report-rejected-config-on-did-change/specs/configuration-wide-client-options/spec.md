## ADDED Requirements

### Requirement: Rejected configuration values are reported on every synchronization path

Tinymist SHALL report every configuration value it rejects during configuration synchronization
to the client via `window/showMessageRequest`, regardless of whether the value arrived through
LSP `initializationOptions`, a `workspace/configuration` response, or a
`workspace/didChangeConfiguration` notification. Each rejected value SHALL be reported exactly
once per synchronization, and a synchronization whose entire payload is rejected SHALL report the
rejection reason to the client even though the notification protocol cannot deliver the handler's
error result.

#### Scenario: Push notification reports a rejected value

- **WHEN** a client sends `workspace/didChangeConfiguration` whose settings contain a value that
  fails to deserialize
- **THEN** Tinymist sends exactly one `window/showMessageRequest` naming the rejected key
- **AND** the rest of the payload is applied with the rejected key falling back to its default

#### Scenario: Pull response reports a rejected value exactly once

- **WHEN** a `workspace/configuration` answer contains a value that fails to deserialize
- **THEN** Tinymist sends exactly one `window/showMessageRequest` naming the rejected key

#### Scenario: Fully rejected update reports the rejection reason

- **WHEN** a client sends `workspace/didChangeConfiguration` whose settings cause the whole
  update to be rejected, such as a non-absolute `rootPath`
- **THEN** Tinymist rolls back to the previous configuration
- **AND** Tinymist sends one `window/showMessageRequest` explaining why the update was rejected

#### Scenario: Valid values and rollbacks produce no spurious messages

- **WHEN** a synchronization applies only valid values
- **THEN** Tinymist sends no `window/showMessageRequest`
- **AND** a later synchronization does not replay warnings from an earlier rejected update
