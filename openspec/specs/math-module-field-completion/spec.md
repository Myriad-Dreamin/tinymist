# math-module-field-completion Specification

## Purpose
The math-module-field-completion specification defines how Tinymist completes code-interpolated module field access inside math equations. It requires exported pure functions from module-valued targets such as `#calc.` to remain available in math mode, while preserving the existing math-mode behavior for non-module field access and postfix completions.
## Requirements
### Requirement: Math-mode code-interpolated module field access includes exported functions
Tinymist SHALL include exported pure functions when completing a code-interpolated module-valued field access expression (for example, `#calc.`) inside math-mode equations.

#### Scenario: Builtin module functions appear for `#calc.`
- **WHEN** a user requests completion on a math-mode expression like `$ #calc. $`
- **THEN** the completion list includes exported function members from the `calc` module such as `odd`

#### Scenario: Prefix filtering retains matching function exports
- **WHEN** a user requests completion on a math-mode expression like `$ #calc.o $`
- **THEN** the completion list includes matching exported function members such as `odd` instead of dropping them because the cursor is in math mode

### Requirement: Math-mode module completion preserves existing non-module behavior
Tinymist SHALL preserve current math-mode dot-access behavior for non-module values while adding module function exports.

#### Scenario: Plain math `calc.` does not expose module exports
- **WHEN** a user requests completion on a plain math expression like `$ calc. $`
- **THEN** tinymist does not surface exported `calc` helper functions such as `odd`

#### Scenario: Plain math `calc.o` does not expose module exports
- **WHEN** a user requests completion on a plain math expression like `$ calc.o $`
- **THEN** tinymist does not surface exported `calc` helper functions such as `odd`

#### Scenario: Symbol field access remains available
- **WHEN** a user requests completion on a symbol-valued math expression like `$ arrow. $`
- **THEN** tinymist continues to offer the existing symbol field members such as `b`

#### Scenario: Math postfix completions remain available
- **WHEN** a user requests completion on a math expression where postfix completions are currently supported, such as `$ arrow. $`
- **THEN** tinymist continues to offer the existing postfix completions such as `abs`
