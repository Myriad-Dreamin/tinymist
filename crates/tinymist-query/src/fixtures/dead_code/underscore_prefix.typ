// Test that underscore-prefixed names are not warned
#let _unused = 42
#let _another_unused() = "test"

#let normal_unused = 100

// None of the underscore ones should warn
