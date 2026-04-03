## ADDED Requirements

### Requirement: Citation completion inserts a valid bibliography-key syntax
Tinymist SHALL insert bibliography-key completions in `#cite(...)` using a syntax that is valid for the selected key. Keys that are representable as Typst label literals SHALL be inserted as `<key>`, and keys that are not representable as Typst label literals SHALL be inserted as `label("key")`.

#### Scenario: Compatible bibliography key keeps angle-bracket syntax
- **WHEN** a user accepts citation completion for a bibliography key such as `Russell:1908` that is valid in Typst label-literal syntax
- **THEN** Tinymist inserts that key in the cite argument as `<Russell:1908>`

#### Scenario: Incompatible bibliography key uses explicit label syntax
- **WHEN** a user accepts citation completion for a bibliography key such as `DBLP:books/lib/Knuth86a` that is not valid in Typst label-literal syntax
- **THEN** Tinymist inserts that key in the cite argument as `label("DBLP:books/lib/Knuth86a")`

#### Scenario: Prefix completion for an incompatible key remains valid
- **WHEN** a user accepts citation completion after typing a prefix for a bibliography key that requires explicit label syntax
- **THEN** Tinymist replaces the active cite-argument prefix with `label("...")` text for the selected key instead of inserting an invalid `<...>` form

### Requirement: Bibliography completion variants share the same citation insertion text
When Tinymist offers multiple completion items for the same bibliography entry, such as the raw key and the entry title, all variants SHALL insert the same citation argument text for that entry.

#### Scenario: Title-backed completion preserves explicit label fallback
- **WHEN** a bibliography entry title is shown as a citation completion item for a key that requires explicit label syntax
- **THEN** accepting the title-backed completion inserts `label("key")` for that bibliography key

#### Scenario: Title-backed completion preserves compatible-key angle syntax
- **WHEN** a bibliography entry title is shown as a citation completion item for a key that is valid in Typst label-literal syntax
- **THEN** accepting the title-backed completion inserts `<key>` for that bibliography key
