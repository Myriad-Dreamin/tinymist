## ADDED Requirements

### Requirement: Static import analysis resolves module-valued field-access sources
Tinymist SHALL treat a field-access expression that resolves to a module as a valid source for a non-wildcard `#import ...: item` statement during static analysis.

#### Scenario: Simple item list from a field-access source is bound
- **WHEN** a user writes `#import theoretic.presets.corners: theorem, lemma` and `theoretic.presets.corners` resolves to a module
- **THEN** tinymist statically binds `theorem` and `lemma` to the corresponding exports from that module for editor-side semantic features

#### Scenario: Nested import item path is resolved from the selected module
- **WHEN** a user writes an import item path such as `#import pkg.section: nested.value` and `pkg.section` resolves to a module
- **THEN** tinymist resolves the item path against that module's export scope and binds the imported name to the targeted export

#### Scenario: Unsupported non-module source stays unresolved
- **WHEN** a non-wildcard import item list uses a field-access source that does not resolve to a module
- **THEN** tinymist MUST NOT create imported bindings for the requested items

### Requirement: Supported field-access item imports do not degrade to empty static scope
Tinymist SHALL provide static import scope for supported module-valued field-access item imports instead of degrading them to an empty import scope that requires dynamic import analysis to recover editor behavior.

#### Scenario: Supported item import produces static scope immediately
- **WHEN** tinymist analyzes a supported import like `#import theoretic.presets.corners: theorem`
- **THEN** the imported item scope is available through static analysis for subsequent editor features in the file

#### Scenario: Existing wildcard field-access import behavior is preserved
- **WHEN** a user writes `#import lib.draw: *`
- **THEN** tinymist continues to resolve the wildcard import as before while adding support for non-wildcard item imports from module-valued field-access sources
