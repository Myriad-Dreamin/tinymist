## ADDED Requirements

### Requirement: The tinymist configuration namespace has one meaning on every delivery path

Tinymist SHALL accept a configuration payload that carries settings either at the top level or inside a `tinymist` object, and SHALL apply the same meaning to that `tinymist` object whether the payload reaches it through `initializationOptions`, through a `workspace/didChangeConfiguration` notification, or through a `workspace/configuration` pull. A namespaced setting SHALL NOT be dropped or reverted to its default because of the path it arrived on.

#### Scenario: Namespaced notification payload applies

- **WHEN** a client sends `workspace/didChangeConfiguration` whose settings contain `{"tinymist": {"exportTarget": "bundle"}}`
- **THEN** Tinymist applies `exportTarget` from the namespace instead of leaving it at its default

#### Scenario: Namespaced pull answer applies

- **WHEN** a client answers a `workspace/configuration` request for the `tinymist` section with `{"exportTarget": "bundle"}` and leaves the other requested sections unanswered
- **THEN** Tinymist applies `exportTarget` from that answer

#### Scenario: Initialization options keep applying the namespace

- **WHEN** a client initializes Tinymist with `initializationOptions` containing `{"tinymist": {"exportTarget": "bundle"}}`
- **THEN** Tinymist applies `exportTarget` from the namespace

### Requirement: A namespaced setting takes precedence over the same setting outside the namespace

When a payload carries the same setting both at the top level and inside the `tinymist` object, Tinymist SHALL apply the namespaced value. A setting that only the top level carries SHALL still apply.

#### Scenario: Namespaced value wins

- **WHEN** a payload contains `{"exportPdf": "onSave", "tinymist": {"exportPdf": "onType"}}`
- **THEN** the effective `exportPdf` is `onType`

#### Scenario: A top-level-only setting still applies

- **WHEN** a payload contains `{"compileStatus": "enable", "tinymist": {"exportPdf": "onType"}}`
- **THEN** the effective `compileStatus` is enabled and the effective `exportPdf` is `onType`

### Requirement: Merging configuration answers preserves per-section client normalization

When Tinymist merges the answers of the `tinymist.<key>`, the `<key>`, and the `tinymist` sections of a `workspace/configuration` response, the answer of a single `tinymist.<key>` section SHALL take precedence over the `tinymist` object for that setting, and the `tinymist` object SHALL take precedence over the `<key>` section.

#### Scenario: A per-section answer keeps precedence over the namespace object

- **WHEN** a client answers the `tinymist.exportTarget` section with `"paged"` and the `tinymist` section with `{"exportTarget": "bundle"}`
- **THEN** the effective `exportTarget` is `paged`

### Requirement: Namespaced settings keep the existing rejection behavior

Tinymist SHALL handle a namespaced setting exactly as the same setting at the top level: a `null` value SHALL be accepted without a warning, and a value that cannot be deserialized SHALL be reported through the configuration warnings instead of silently reverting the setting or rewriting the whole configuration.

#### Scenario: Namespaced null values stay harmless

- **WHEN** a payload contains `{"tinymist": {"exportPdf": null, "fontPaths": null}}`
- **THEN** Tinymist accepts the payload without a configuration warning

#### Scenario: An invalid namespaced value is reported

- **WHEN** a payload contains `{"tinymist": {"exportPdf": "not-a-task-when"}}`
- **THEN** Tinymist reports a configuration warning naming `exportPdf`
