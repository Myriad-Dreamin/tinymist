## ADDED Requirements

### Requirement: `$name` preserves the full source stem in export paths
Tinymist SHALL preserve every non-extension character from the source filename when resolving `$name` inside `tinymist.outputPath`, even when the Typst filename stem contains one or more dots.

#### Scenario: Multi-dot source names survive PDF extension application
- **GIVEN** a source file named `Chapter 1.1.1.typ`
- **WHEN** `tinymist.outputPath` is `$root/$dir/$name` and Tinymist exports a PDF
- **THEN** the resolved artifact path ends with `Chapter 1.1.1.pdf`
- **AND** Tinymist MUST NOT truncate the name to `Chapter 1.1.pdf`

#### Scenario: Repeated dots in the source stem are preserved
- **GIVEN** a source file named `test....typ`
- **WHEN** `tinymist.outputPath` is `$root/$dir/$name` and Tinymist exports a PDF
- **THEN** the resolved artifact path ends with `test....pdf`

### Requirement: Empty `$dir` stays workspace-relative
Tinymist SHALL treat an empty `$dir` substitution as the current workspace-relative directory, not as a filesystem-root prefix.

#### Scenario: Workspace-root file with explicit `$dir/$name` exports beside the source
- **GIVEN** a Typst file named `Chapter 1.1.typ` at the workspace root
- **WHEN** `tinymist.outputPath` is `$dir/$name`
- **THEN** Tinymist resolves the export path to `Chapter 1.1.pdf` beside the source file inside the workspace root
- **AND** Tinymist MUST NOT attempt to write `/Chapter 1.pdf`

#### Scenario: Nested file keeps its containing directory
- **GIVEN** a Typst file at `chapters/Chapter 1.1.typ`
- **WHEN** `tinymist.outputPath` is `$dir/$name`
- **THEN** Tinymist resolves the export path to `chapters/Chapter 1.1.pdf` inside the workspace root

### Requirement: Explicit `$dir/$name` matches the default output location
Tinymist SHALL resolve an explicit `tinymist.outputPath = "$dir/$name"` to the same artifact location as leaving `tinymist.outputPath` empty.

#### Scenario: Explicit and default output paths agree for workspace-root files
- **WHEN** the same workspace-root Typst file is exported once with `tinymist.outputPath` unset and once with `tinymist.outputPath` set to `$dir/$name`
- **THEN** both exports target the same artifact path

#### Scenario: Explicit and default output paths agree for nested files
- **WHEN** the same nested Typst file is exported once with `tinymist.outputPath` unset and once with `tinymist.outputPath` set to `$dir/$name`
- **THEN** both exports target the same artifact path
