## ADDED Requirements

### Requirement: Package scan output location
The package scan tooling SHALL write generated reports and intermediate results under `target/`.

#### Scenario: Run package scan
- **WHEN** a developer runs the package-scale type checker measurement
- **THEN** generated typings, timing reports, diffs, summaries, and intermediate files are written under `target/tyck-package-scan/`

### Requirement: Per-package pretty reports
The package scan tooling SHALL produce pretty per-package typing reports in addition to machine-readable data.

#### Scenario: Inspect one package
- **WHEN** a package has changed typings
- **THEN** the developer can open a package-specific pretty report showing relevant `#let` typings and changed entries

### Requirement: Baseline-current comparison
The package scan tooling SHALL compare baseline and current outputs by package and file.

#### Scenario: Compare revisions
- **WHEN** baseline and current scan outputs are available
- **THEN** the tooling reports changed, stronger, weaker, unchanged, timeout, and error counts per package

### Requirement: Performance metrics
The package scan tooling SHALL record elapsed time per file and per package.

#### Scenario: Identify slow files
- **WHEN** a package scan completes
- **THEN** the summary identifies the slowest files and packages with measured elapsed time

### Requirement: Timeout regression detection
The package scan tooling SHALL report timeout regressions separately from typing diffs.

#### Scenario: Timeout appears
- **WHEN** the current run times out on a file or package that baseline completed
- **THEN** the summary marks it as a timeout regression
