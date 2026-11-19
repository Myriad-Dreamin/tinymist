## ADDED Requirements

### Requirement: `--watch` recompiles and re-exports on dependency changes
`tinymist compile --watch` SHALL compile and export the task once at start and again after every change to a file the document depends on, with the same output path, format, export options and lock file as the one-shot run, until the process is interrupted. Exports SHALL NOT overlap, and a compilation that is replaced by a newer one before its export starts MAY be skipped.

#### Scenario: Initial run
- **WHEN** `tinymist compile --watch main.typ out/main.pdf` starts on a document that compiles
- **THEN** `out/main.pdf` is written
- **AND** `compiled successfully` is printed on stderr after the file is written

#### Scenario: Change in an included file
- **GIVEN** a running `--watch` on `main.typ`, which includes `chapter.typ`
- **WHEN** `chapter.typ` is saved
- **THEN** the document is compiled and exported again
- **AND** `compiled successfully` is printed again

#### Scenario: Export options are kept
- **WHEN** `--watch` is combined with other export options, for example `--pdf-standard`
- **THEN** every export in the watch run applies them

#### Scenario: Lock file
- **WHEN** `--watch` is combined with `--save-lock`
- **THEN** the lock file equals that of a run without `--watch`

#### Scenario: Changes during an export
- **GIVEN** an export or `--on-export` command that is still running
- **WHEN** further changes are saved
- **THEN** the next export starts after it has finished, with the newest version

#### Scenario: Entry file appears later
- **GIVEN** a `--watch` run on a file that does not exist yet
- **WHEN** the file is created
- **THEN** the document is compiled and exported

### Requirement: Diagnostics and status after every compilation
Each compilation in watch mode SHALL print its diagnostics in the human format on stderr, followed by `compiled successfully` after a completed export, `compiled with errors` when errors prevent the export, or `export failed:` with the error when the export itself fails.

#### Scenario: Compile error while watching
- **WHEN** a saved change introduces an error
- **THEN** the error diagnostics are printed
- **AND** `compiled with errors` is printed
- **AND** the previous output file is left in place

#### Scenario: Error fixed
- **WHEN** the next saved change removes the error
- **THEN** the document is exported and `compiled successfully` is printed

### Requirement: `--on-export` runs a command after every successful export
With `--on-export <COMMAND>`, `tinymist compile --watch` SHALL run the command through the system shell after every successful export, once per written file, with the file's absolute path as the last argument and in the environment variable `TINYMIST_OUTPUT`, before printing `compiled successfully`. A failing command SHALL be reported on stderr without stopping the watch; the option SHALL be rejected without `--watch`.

#### Scenario: Viewer reload
- **GIVEN** `tinymist compile --watch --on-export reload-viewer main.typ out/main.pdf`
- **WHEN** an export completes
- **THEN** `reload-viewer /abs/out/main.pdf` runs with `TINYMIST_OUTPUT=/abs/out/main.pdf`
- **AND** `compiled successfully` is printed after it returns

#### Scenario: Compile error
- **WHEN** a compilation has errors
- **THEN** the command does not run

#### Scenario: Command fails
- **WHEN** the command exits with a non-zero status or cannot be started
- **THEN** a line naming the command and the failure is printed on stderr
- **AND** watching continues

#### Scenario: Without `--watch`
- **WHEN** `--on-export` is given without `--watch`
- **THEN** the command line is rejected

### Requirement: One-shot behavior is unchanged
`tinymist compile` without `--watch` SHALL behave as before: one compilation, one export, exit status 1 on compile errors.

#### Scenario: Without the flag
- **WHEN** `tinymist compile main.typ out/main.pdf` runs
- **THEN** the process exits after the export
