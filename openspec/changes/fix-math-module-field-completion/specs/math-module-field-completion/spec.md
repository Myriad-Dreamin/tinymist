## ADDED Requirements

### Requirement: Math-mode module field access includes exported functions
Tinymist SHALL include exported pure functions when completing a module-valued field access expression inside math mode.

#### Scenario: Builtin module functions appear for `calc.`
- **WHEN** a user requests completion on a math-mode expression like `$ calc. $`
- **THEN** the completion list includes exported function members from the `calc` module such as `odd`

#### Scenario: Prefix filtering retains matching function exports
- **WHEN** a user requests completion on a math-mode expression like `$ calc.o $`
- **THEN** the completion list includes matching exported function members such as `odd` instead of dropping them because the cursor is in math mode

### Requirement: Math-mode module completion preserves existing non-module behavior
Tinymist SHALL preserve current math-mode dot-access behavior for non-module values while adding module function exports.

#### Scenario: Symbol field access remains available
- **WHEN** a user requests completion on a symbol-valued math expression like `$ arrow. $`
- **THEN** tinymist continues to offer the existing symbol field members such as `b`

#### Scenario: Math postfix completions remain available
- **WHEN** a user requests completion on a math expression where postfix completions are currently supported, such as `$ arrow. $`
- **THEN** tinymist continues to offer the existing postfix completions such as `abs`
