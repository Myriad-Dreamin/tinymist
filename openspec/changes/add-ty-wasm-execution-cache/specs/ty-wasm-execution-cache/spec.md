## ADDED Requirements

### Requirement: Interpreter execution backend
The system SHALL provide a Rust interpreter backend for type bytecode programs.

#### Scenario: Execute bytecode in interpreter
- **WHEN** the checker requests execution of a supported bytecode program
- **THEN** the interpreter evaluates the program to a semantic type value without invoking Wasmer

### Requirement: Non-blocking closure call state
The system SHALL represent closure call execution with local states that distinguish fresh, running, completed, and stuck calls.

#### Scenario: Recursive call while running
- **WHEN** a closure call attempts to force a closure that is already running in the current VM
- **THEN** execution returns a neutral residual value rather than blocking on the running call

### Requirement: Completed-result global caches
The system SHALL cache completed bytecode execution results globally while excluding in-progress computations from global blocking caches.

#### Scenario: Reuse completed call
- **WHEN** a later checker invocation evaluates the same completed closure call under a compatible key
- **THEN** the VM may reuse the cached result without re-running the closure body

### Requirement: Experimental Wasmer backend
The system SHALL provide an experimental Wasmer backend behind an explicit feature flag.

#### Scenario: Wasmer disabled by default
- **WHEN** the workspace is built without the experimental Wasmer feature
- **THEN** type bytecode execution uses the Rust interpreter and does not require Wasmer dependencies

### Requirement: Execution metrics
The system SHALL expose metrics for VM steps, cache hits and misses, residualized cycles, and Wasmer compile and run time when enabled.

#### Scenario: Collect package scan metrics
- **WHEN** package-scale type analysis runs with VM metrics enabled
- **THEN** the output includes enough counters to compare interpreter, cache, and Wasmer behavior
