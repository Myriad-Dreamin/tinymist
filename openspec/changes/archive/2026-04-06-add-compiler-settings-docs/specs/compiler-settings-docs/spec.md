## ADDED Requirements

### Requirement: Tinymist provides a shared compiler-settings guide
Tinymist documentation SHALL include a shared guide that explains how compiler settings affect the Typst environment used for editing, previewing, and exporting.

#### Scenario: Shared guide covers fonts, packages, and supported extra arguments
- **WHEN** a user reads the compiler-settings guide
- **THEN** the docs explain `tinymist.fontPaths`, `tinymist.systemFonts`, package-related compiler paths, and supported `tinymist.typstExtraArgs` usage
- **AND** the docs include concrete configuration examples for font setup and package setup

#### Scenario: Shared guide explains reproducible font configuration
- **WHEN** a user reads about font configuration
- **THEN** the docs explain that `tinymist.systemFonts = false` helps reproducible builds by avoiding host-specific system-font discovery
- **AND** the docs show how to pair that setting with explicit `tinymist.fontPaths`

#### Scenario: Shared guide limits extra-argument examples to supported settings
- **WHEN** the docs show `tinymist.typstExtraArgs` examples
- **THEN** the examples use arguments Tinymist parses today, such as `--input`, `--root`, `--font-path`, `--ignore-system-fonts`, `--package-path`, `--package-cache-path`, `--creation-timestamp`, and `--cert`
- **AND** the docs do not imply that unsupported flags are accepted automatically

### Requirement: Friendly VS Code docs remain in place while linking to canonical compiler-setting details
The VS Code frontend documentation SHALL keep step-by-step setup prose for users while pointing to the shared compiler-settings guide for deeper explanations.

#### Scenario: VS Code walkthrough remains explicit
- **WHEN** a user reads the VS Code frontend documentation
- **THEN** the docs still include direct instructions for configuring compiler-related settings in VS Code
- **AND** the change does not reduce that section to a bare settings-reference link

#### Scenario: Preview and export docs point to the shared guide
- **WHEN** a user reads preview or export documentation
- **THEN** the docs explain that those features use the same compiler font and package environment
- **AND** they point the reader to the shared compiler-settings guide for setup details

#### Scenario: Settings reference aligns with reproducibility guidance
- **WHEN** a user reads the VS Code settings reference for `tinymist.systemFonts`, `tinymist.fontPaths`, or `tinymist.typstExtraArgs`
- **THEN** the descriptions explain the reproducible-build role of `tinymist.systemFonts = false`
- **AND** the descriptions clarify the interaction between dedicated settings and `typstExtraArgs`

### Requirement: Embedded font docs are generated from `typst-assets` source
Tinymist documentation SHALL derive the embedded font inventory from the current `typst-assets` source through repository tooling instead of a hand-maintained font list.

#### Scenario: Generated font list tracks the current embedded bundle
- **WHEN** `typst-assets` changes the embedded font list and the docs are regenerated
- **THEN** the compiler-settings docs rebuild the embedded-font inventory from the `typst-assets` source
- **AND** the font names do not require manual editing in the documentation page itself

#### Scenario: Docs explain the emoji-font gap with the official Typst app experience
- **WHEN** a user checks the embedded-font section to understand emoji rendering differences
- **THEN** the docs explain that Tinymist's binary does not embed the extra Twitter emoji font used in the official Typst app experience
- **AND** the docs tell the user they can add an emoji font manually if they want similar rendering
