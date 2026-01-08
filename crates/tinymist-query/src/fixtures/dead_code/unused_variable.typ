// Test unused variable detection
#let unused_var = 42
#let used_var = 100

// This should not trigger warning
#used_var
