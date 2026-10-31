## ADDED Requirements

### Requirement: `--synctex` writes a SyncTeX file next to the PDF
`tinymist compile --synctex` SHALL write `<output>.synctex.gz` in the directory of the exported PDF after a successful PDF export, and SHALL write nothing extra without the flag.

#### Scenario: PDF export with the flag
- **WHEN** `tinymist compile --synctex main.typ out/main.pdf` succeeds
- **THEN** `out/main.synctex.gz` exists
- **AND** it is a gzip-compressed text file that `synctex_parser` opens without warnings

#### Scenario: PDF export without the flag
- **WHEN** `tinymist compile main.typ out/main.pdf` succeeds
- **THEN** no `out/main.synctex.gz` is written

#### Scenario: Compile error
- **WHEN** the document fails to compile
- **THEN** no SyncTeX file is written or replaced

### Requirement: `--synctex` is rejected where the file would be missing or wrong
`tinymist compile` SHALL fail with an error when `--synctex` is combined with `--pages` or with an output format other than PDF.

#### Scenario: Non-PDF output
- **WHEN** `tinymist compile --synctex main.typ main.png` runs
- **THEN** the command fails and writes no SyncTeX file

#### Scenario: Page subset
- **WHEN** `tinymist compile --synctex --pages 1 main.typ main.pdf` runs
- **THEN** the command fails and writes no SyncTeX file

### Requirement: SyncTeX records map text runs to source lines
Every run of text in the paged document SHALL appear as an hbox record whose tag and line name the source file and the 1-based line of its glyphs, with the baseline start as position and width, ascent and descent as extent, in scaled points measured from the top-left corner of the page.

#### Scenario: Forward search for a body line
- **GIVEN** a document whose chapter file `kapitel/a.typ` has prose on line 81 that is typeset on page 9
- **WHEN** a viewer asks for line 81 of `kapitel/a.typ`
- **THEN** the result is on page 9 at the position of that text

#### Scenario: Backward search on a body line
- **WHEN** a viewer asks for the source of a point inside typeset prose on page 9
- **THEN** the result names the file and the line whose glyphs cover that point

#### Scenario: Paragraph wrapped differently in the source
- **GIVEN** one typeset line whose words come from two consecutive source lines
- **THEN** the typeset line yields two hbox records, one per source line, each covering only the glyphs of that line

#### Scenario: Text inside a transformed group
- **GIVEN** text placed through `place`, `move`, `rotate` or `scale`
- **THEN** its record's position is the transformed baseline start on the page

### Requirement: Every source file gets one `Input:` record with an absolute path
The header SHALL list each source file that contributed text once, tagged consecutively from 1 in order of first appearance, with the absolute path the world resolves for it; package files resolve to their path in the package cache.

#### Scenario: Multi-file project
- **GIVEN** `main.typ` that includes `kapitel/a.typ` and `kapitel/b.typ`
- **THEN** the header has one `Input:` line per file that produced text, each with an absolute path
- **AND** records for text from `kapitel/b.typ` carry that file's tag

### Requirement: Outline entries carry no records
Text rendered by `outline()` entries SHALL produce no SyncTeX records, so a lookup for a heading's line finds the heading.

#### Scenario: Forward search for a heading line
- **GIVEN** a document with a table of contents on page 2 and a heading from line 16 of `kapitel/a.typ` on page 8
- **WHEN** a viewer asks for line 16 of `kapitel/a.typ`
- **THEN** the first result is on page 8

### Requirement: The task model carries the flag
`ExportPdfTask` SHALL have a boolean `synctex` field, default false and omitted from serialization when false, so lock files can request the file, and `tinymist generate-script` SHALL emit `--synctex` for a task that has it.

#### Scenario: Lock file without the field
- **WHEN** a lock file written before this change is read
- **THEN** its PDF tasks deserialize with `synctex` false

#### Scenario: Generated build script
- **GIVEN** a lock file written by `tinymist compile --save-lock --synctex main.typ`
- **WHEN** `tinymist generate-script` runs
- **THEN** the script's compile command contains `--synctex`
