## ADDED Requirements

### Requirement: Native viewer preserves link hit areas

The native Tinymist viewer SHALL preserve semantic link hit areas for rendered paged preview pages.

#### Scenario: Link rectangle is hit-tested in page coordinates

- **WHEN** a rendered page contains a vector link item
- **THEN** the native viewer records the link target and its page-coordinate hit rectangle
- **AND** a page click inside that rectangle matches the link target

#### Scenario: Non-link clicks keep source sync behavior

- **WHEN** a user clicks a rendered page outside any supported semantic link
- **THEN** the native viewer sends the existing source-position sync request to the preview server

### Requirement: Native viewer opens supported external links

The native Tinymist viewer SHALL open only external links with `http`, `https`, or `mailto` schemes through the system default handler.

#### Scenario: HTTP link opens externally

- **WHEN** a user clicks a semantic link whose target starts with `http://`
- **THEN** the native viewer opens the link through the system default handler
- **AND** the click is not sent as a source-position sync request

#### Scenario: HTTPS link opens externally

- **WHEN** a user clicks a semantic link whose target starts with `https://`
- **THEN** the native viewer opens the link through the system default handler
- **AND** the click is not sent as a source-position sync request

#### Scenario: Mail link opens externally

- **WHEN** a user clicks a semantic link whose target starts with `mailto:`
- **THEN** the native viewer opens the link through the system default handler
- **AND** the click is not sent as a source-position sync request

#### Scenario: Unsupported link target falls back to source sync

- **WHEN** a user clicks a semantic link whose target uses any other scheme or is not an absolute external link
- **THEN** the native viewer does not open the target externally
- **AND** the click is handled as a normal source-position sync request
