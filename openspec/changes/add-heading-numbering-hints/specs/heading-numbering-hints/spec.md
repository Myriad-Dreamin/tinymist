## ADDED Requirements

### Requirement: Heading numbering is shown as an inlay hint
Tinymist SHALL answer `textDocument/inlayHint` with the resolved numbering of each numbered heading in the requested range, taken from the last successfully compiled document.

#### Scenario: Markup heading in a compiled document
- **WHEN** the document has compiled successfully and the range contains a markup heading whose `numbering` is set, such as `== Background` numbered `1.1`
- **THEN** the response contains a hint labelled `1.1` positioned at the end of the heading's `==` marker

#### Scenario: Heading function call in a compiled document
- **WHEN** the range contains a `#heading(..)` call whose element is numbered `2` in the compiled document
- **THEN** the response contains a hint labelled `2` positioned at the end of the call

#### Scenario: Heading without numbering
- **WHEN** a heading has no numbering, because no `numbering` is set or it is set to `none`
- **THEN** no numbering hint is produced for that heading

#### Scenario: Document not compiled
- **WHEN** there is no successfully compiled document for the request
- **THEN** no numbering hints are produced and other inlay hints are unaffected

### Requirement: Preview outline titles carry the heading numbering
The preview outline sent in `tinymist/documentOutline` SHALL prefix each numbered heading's title with its resolved numbering followed by a space, and SHALL leave unnumbered headings' titles unchanged.

#### Scenario: Numbered heading in the outline
- **WHEN** the compiled document contains a heading `Background` numbered `1.1`
- **THEN** its outline item title is `1.1 Background`

#### Scenario: Unnumbered heading in the outline
- **WHEN** the compiled document contains a heading `Preface` with no numbering
- **THEN** its outline item title is `Preface`
