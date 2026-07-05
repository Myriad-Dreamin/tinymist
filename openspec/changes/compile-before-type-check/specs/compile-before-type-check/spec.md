## ADDED Requirements

### Requirement: Compile before deduce
The checker SHALL compile supported syntax into type bytecode before computing deduced types.

#### Scenario: Deduced function signature
- **WHEN** the checker sees a supported function definition
- **THEN** it records a closure-backed signature whose result is represented by a meta variable that can be forced by calls or precise signature queries

### Requirement: Demand-driven function result evaluation
The checker SHALL evaluate function bodies on demand when a call needs the callee result and the body can run without blocking on another worker.

#### Scenario: Non-recursive helper call
- **WHEN** a function body calls a same-scope helper whose closure is not running
- **THEN** the VM evaluates the helper body and uses the resulting semantic type to compute the caller result

### Requirement: Cycle residualization
The checker SHALL residualize cyclic function-result dependencies as neutral values.

#### Scenario: Recursive call cycle
- **WHEN** evaluating a closure encounters a call to a closure already running in the current VM
- **THEN** the result is a neutral residual and evaluation continues without waiting on a shared cache

### Requirement: Check after semantic evaluation
The checker SHALL apply compatibility checks and once-only experimental warnings to semantic operands rather than relying on a separate syntax-only pass.

#### Scenario: Binary incompatibility
- **WHEN** VM evaluation processes a binary operation whose semantic operand types are incompatible
- **THEN** the checker records the experimental warning once and returns the appropriate semantic result or residual

### Requirement: Precise signature query
The public precise signature query SHALL force the relevant closure result before returning documentation or signature-help data.

#### Scenario: Documentation asks for a function signature
- **WHEN** documentation generation asks for a function definition signature
- **THEN** it receives the precise quoted signature through the analysis query API and does not need to know checker internals
