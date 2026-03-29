## ADDED Requirements

### Requirement: Generated standalone Tinymist Rust tool skill
The repository SHALL provide a generated standalone Codex skill for authoring Rust-based Typst tools with Tinymist APIs. The generated skill SHALL be checked in as a single `SKILL.md` file with valid YAML frontmatter at the beginning of the file and SHALL contain two top-level parts: how to make a tool, and how to use Tinymist Rust APIs.

#### Scenario: Skill generation produces a portable output
- **WHEN** the repository documentation generation workflow regenerates the Tinymist Rust tool skill
- **THEN** it writes a single standalone `SKILL.md` file under `.codex/skills/`
- **AND** the generated file begins with valid YAML frontmatter at byte zero
- **AND** the generated file does not require sibling reference files in order to be usable after copy into another repository

#### Scenario: Skill output preserves the required two-part structure
- **WHEN** a maintainer reads the generated Tinymist Rust tool skill
- **THEN** the skill contains a section that explains how to make a tool
- **AND** the skill contains a second section that explains how to use Tinymist Rust APIs

### Requirement: Typst developer docs are the source of truth
The repository SHALL keep the source content for the Tinymist Rust tool skill in Typst documents under `docs/tinymist/dev`. The API guidance portion SHALL be authored as modular Typst documents in a dedicated subdirectory and assembled into the generated skill through `#include` and `#import`, rather than hand-maintained directly in the generated Markdown output.

#### Scenario: Source documents live in developer docs
- **WHEN** a maintainer needs to update the Tinymist Rust tool skill content
- **THEN** the source documents to edit are Typst files under `docs/tinymist/dev`
- **AND** the maintainer does not need to edit the generated `SKILL.md` directly

#### Scenario: API section is assembled from modular Typst sources
- **WHEN** the skill source for Tinymist Rust APIs is organized in the repository
- **THEN** the API guidance lives in a dedicated `docs/tinymist/dev` subdirectory
- **AND** the top-level skill entry document assembles that API guidance with `#include` and `#import`

### Requirement: Skill guidance covers Tinymist tool-authoring patterns
The generated skill SHALL teach recommended Tinymist tool-authoring patterns through concrete examples and anti-examples. The guidance SHALL cover dependency import patterns for Tinymist and Typst, and it SHALL include concrete scenario guidance for word count, compile in parallel, and watch-plus-query workflows. The guidance SHALL use maintained repository tools such as `crates/crityp` and `crates/tinymist-cli` as reference points, and it SHALL distinguish recommended API layers from lower-level internal machinery that should not be the default downstream recipe.

#### Scenario: How-to section covers dependency and example patterns
- **WHEN** a reader follows the skill's how-to-make-a-tool section
- **THEN** the section includes examples for importing Tinymist and Typst from git using a specific tag or revision
- **AND** the section includes concrete guidance or examples for word count, compile in parallel, and watch-plus-query tools

#### Scenario: API section includes do and do-not guidance
- **WHEN** a reader uses the skill's Tinymist Rust API section
- **THEN** the section explains recommended layers such as compile-once setup, compilation/query helpers, and long-lived project flows
- **AND** the section includes explicit do and do-not examples for those APIs
- **AND** the section avoids presenting lower-level internal watcher machinery as the default downstream approach for watch-based tools

#### Scenario: Guidance stays grounded in maintained repository examples
- **WHEN** the skill explains how to build small Rust-based Typst tools
- **THEN** it references maintained repository examples such as `crates/crityp` and `crates/tinymist-cli`
- **AND** it uses those examples to justify the recommended patterns and anti-patterns described in the skill
